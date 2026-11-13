//! Workspace-VM-only host runtime composition.
//!
//! The host owns one long-lived VM per active workspace. Task assignment,
//! worker polling, task-scoped roots, and parked provider runtimes are deliberately
//! absent from this module.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use g5_internal_api::host_ctl::{
    CurrentIncarnation, HostHeartbeat, HostHello, StopMode, VmHeartbeat, VmInventory,
    PROTOCOL_VERSION,
};
use g5d_foundation::runtime_environment::{
    PROCESS_INCARNATION_ENV, PROCESS_INCARNATION_OUTER_ENV, PROCESS_SOCKET_DIR_ENV,
};
use g5d_isolation::{
    ChildReapReceipt, IsolationManager, MicrovmBackend, MicrovmRunSpec, ProcessBackend, ProcessTag,
    RunHandle, RunResult,
};
use uuid::Uuid;

use crate::ctl::{disk_free_mib, memory_free_mib, system_load, HostControlState};
use crate::workspace_vm::{OtherSlotUsage, WorkspaceVmRuntime};

/// Narrow boot/kill seam consumed by the workspace runtime.
#[async_trait]
pub trait VmLauncher: Send + Sync {
    async fn run(&self, spec: MicrovmRunSpec) -> anyhow::Result<RunResult>;
    async fn start(&self, spec: MicrovmRunSpec) -> anyhow::Result<RunHandle>;
    async fn kill(&self, handle: &RunHandle) -> anyhow::Result<ChildReapReceipt>;
    async fn stop(
        &self,
        handle: &RunHandle,
        mode: StopMode,
        grace: std::time::Duration,
    ) -> anyhow::Result<ChildReapReceipt> {
        let _ = (mode, grace);
        self.kill(handle).await
    }
}

/// Production isolation launcher.
pub struct IsolationVmLauncher {
    manager: Arc<IsolationManager>,
}

impl IsolationVmLauncher {
    #[must_use]
    pub fn new(manager: Arc<IsolationManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl VmLauncher for IsolationVmLauncher {
    async fn run(&self, spec: MicrovmRunSpec) -> anyhow::Result<RunResult> {
        self.manager.run(&spec).await
    }

    async fn start(&self, spec: MicrovmRunSpec) -> anyhow::Result<RunHandle> {
        self.manager.start(&spec).await
    }

    async fn kill(&self, handle: &RunHandle) -> anyhow::Result<ChildReapReceipt> {
        self.manager.kill(handle).await
    }

    async fn stop(
        &self,
        handle: &RunHandle,
        mode: StopMode,
        grace: std::time::Duration,
    ) -> anyhow::Result<ChildReapReceipt> {
        let grace = match mode {
            StopMode::Graceful => grace,
            StopMode::Kill => std::time::Duration::ZERO,
        };
        self.manager.terminate(handle, grace).await
    }
}

/// Hello capability a host advertises for its isolation level
/// (`g5.runtime.boot.backend.process-local`): the control plane admits a
/// process host only when it was itself started for local development.
pub use g5_internal_api::host_ctl::{CAPABILITY_ISOLATION_MICROVM, CAPABILITY_ISOLATION_PROCESS};

/// Local-development launcher (`g5.runtime.boot.backend.process-local`): each
/// workspace incarnation is a `g5d dispatch` host process group instead of a
/// microVM. The runtime names what the incarnation is (identity, token, port,
/// workspace and runtime roots); this launcher names where it lives on the
/// host, under `<state_dir>/process-vms/<vm_id>` with its own `home`, `state`
/// and `tmp` directories and a `dispatch.log`, never inside the checkout.
/// Group spawn and group kill are the process backend's, so custody sees the
/// same reap receipts as every other backend.
pub struct ProcessVmLauncher {
    backend: ProcessBackend,
    state_dir: PathBuf,
    /// Socket directories of started incarnations, removed after their stop.
    socket_dirs: std::sync::Mutex<std::collections::HashMap<String, PathBuf>>,
}

/// A Unix socket path must fit `sun_path`: 104 bytes on macOS including the
/// terminator, 108 on Linux. The smaller bound applies on every platform so a
/// checkout that works on one works on the other.
const SOCKET_PATH_MAX: usize = 104;

/// Operator override for the short directory under which process-mode
/// incarnations bind their sockets; otherwise the per-user runtime directory.
pub const PROCESS_SOCKET_ROOT_ENV: &str = "G5D_PROCESS_SOCKET_ROOT";

/// Environment the launcher adds to a process-mode dispatcher: the per-VM
/// home and state directories the dispatcher reads in process mode.
pub const PROCESS_HOME_ENV: &str = "G5D_PROCESS_HOME";
pub const PROCESS_STATE_DIR_ENV: &str = "G5D_PROCESS_STATE_DIR";

impl ProcessVmLauncher {
    #[must_use]
    pub fn new(state_dir: PathBuf) -> Self {
        Self {
            backend: ProcessBackend::new(),
            state_dir,
            socket_dirs: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Where a process-mode incarnation keeps its home, state, tmp and log.
    #[must_use]
    pub fn vm_dir(&self, vm_id: Uuid) -> PathBuf {
        self.state_dir.join("process-vms").join(vm_id.to_string())
    }

    /// The short, owner-only directory under which incarnations bind their
    /// workspace Git socket: a checkout path has no bound, a socket path has
    /// one, so the socket lives beside the user's runtime files instead
    /// (`G5D_PROCESS_SOCKET_ROOT`, else `$XDG_RUNTIME_DIR/g5d`, else the
    /// per-user temporary directory macOS gives every login, else
    /// `/tmp/g5d-<uid>`). Created or verified as this user's, mode 0700.
    pub fn socket_root() -> anyhow::Result<PathBuf> {
        // SAFETY: geteuid has no failure modes and touches no memory.
        let uid = unsafe { libc::geteuid() };
        let root = if let Some(root) =
            std::env::var_os(PROCESS_SOCKET_ROOT_ENV).filter(|value| !value.is_empty())
        {
            PathBuf::from(root)
        } else if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|dir| dir.is_dir())
        {
            runtime.join("g5d")
        } else if cfg!(target_os = "macos") {
            std::env::var_os("TMPDIR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .filter(|dir| dir.is_dir())
                .map(|dir| dir.join("g5d"))
                .unwrap_or_else(|| PathBuf::from(format!("/tmp/g5d-{uid}")))
        } else {
            PathBuf::from(format!("/tmp/g5d-{uid}"))
        };
        std::fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
                .with_context(|| format!("restrict {}", root.display()))?;
            if std::fs::metadata(&root)?.uid() != uid {
                return Err(anyhow!(
                    "process socket root {} is not owned by this user; set {PROCESS_SOCKET_ROOT_ENV}",
                    root.display()
                ));
            }
        }
        Ok(root)
    }

    /// The incarnation's socket directory, bounded so the Git socket path
    /// fits every supported platform; refused rather than truncated.
    fn vm_socket_dir(vm_id: Uuid) -> anyhow::Result<PathBuf> {
        let dir = Self::socket_root()?.join(&vm_id.simple().to_string()[..12]);
        let socket = dir.join(g5_contracts::workspace_git::SOCKET_FILE_NAME);
        if socket.as_os_str().len() >= SOCKET_PATH_MAX {
            return Err(anyhow!(
                "process socket path {} is longer than a Unix socket path allows ({SOCKET_PATH_MAX} bytes); set {PROCESS_SOCKET_ROOT_ENV} to a shorter directory",
                socket.display()
            ));
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(dir)
    }

    /// Durable ownership record of a started incarnation, so a host that
    /// restarts after dying can find and settle what the incarnation left
    /// running before it reclaims any root (`g5.runtime.custody`).
    fn incarnation_record(dir: &Path) -> PathBuf {
        dir.join("incarnation.json")
    }

    /// Marker that a restarted host proved the incarnation's exit.
    fn settled_marker(dir: &Path) -> PathBuf {
        dir.join("settled")
    }

    /// Whether `dir` carries exit proof: the marker the proof publisher
    /// writes is an empty regular file, so only that shape is proof. An
    /// absent marker is no proof; anything else at that path (a directory, a
    /// symlink) was never written by a proof and is damage, which is an error
    /// rather than either answer (review R28).
    fn settled_proof(dir: &Path) -> anyhow::Result<bool> {
        let marker = Self::settled_marker(dir);
        match std::fs::symlink_metadata(&marker) {
            Ok(meta) if meta.file_type().is_file() && meta.len() == 0 => Ok(true),
            Ok(_) => Err(anyhow!(
                "{} is not the exit proof a host writes (an empty regular file); remove it only once the incarnation's processes are known to be gone",
                marker.display()
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).with_context(|| format!("inspect {}", marker.display())),
        }
    }

    /// Where an incarnation directory is assembled before it is published;
    /// not a UUID name, so settlement never reads it as an incarnation.
    fn staging_dir(dir: &Path) -> PathBuf {
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        dir.with_file_name(format!(".staging-{name}"))
    }

    /// Where a settled incarnation directory is moved when the same
    /// incarnation starts again (a readiness retry): not a UUID name, so
    /// settlement never reads it; the next host start removes it.
    fn retired_dir(dir: &Path) -> PathBuf {
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis())
            .unwrap_or_default();
        dir.with_file_name(format!(".retired-{name}-{unix_ms}"))
    }

    /// Written **before** the incarnation is spawned (review R10): ownership
    /// must be recoverable before any process exists, so a record failure
    /// starts nothing instead of abandoning a live child. The start time it
    /// records is the census fence: every process of the incarnation starts
    /// after it, so settlement may disregard what started before.
    fn write_incarnation_record(dir: &Path, vm_id: Uuid) -> anyhow::Result<()> {
        let started_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis())
            .unwrap_or_default();
        Self::publish_record(
            dir,
            serde_json::json!({
                "vm_id": vm_id,
                "leader_pid": None::<u32>,
                "started_unix_ms": started_unix_ms,
            }),
        )
    }

    /// Add the leader pid to the published record after the spawn, as a
    /// convenience that lets settlement name the session. The fence is kept
    /// exactly as recorded before the spawn: a later time would exclude
    /// members that started between the spawn and this update (review R23).
    fn record_leader_pid(dir: &Path, vm_id: Uuid, leader_pid: u32) -> anyhow::Result<()> {
        let path = Self::incarnation_record(dir);
        let mut record: serde_json::Value = std::fs::read(&path)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(anyhow::Error::from))
            .with_context(|| format!("read {}", path.display()))?;
        if record["vm_id"] != serde_json::json!(vm_id) || !record["started_unix_ms"].is_u64() {
            anyhow::bail!(
                "{} is not the pre-spawn record of incarnation {vm_id}",
                path.display()
            );
        }
        record["leader_pid"] = serde_json::json!(leader_pid);
        Self::publish_record(dir, record)
    }

    fn publish_record(dir: &Path, record: serde_json::Value) -> anyhow::Result<()> {
        let path = Self::incarnation_record(dir);
        let staged = dir.join(format!(".incarnation-{}.json", Uuid::new_v4()));
        std::fs::write(&staged, serde_json::to_vec(&record)?)
            .with_context(|| format!("write {}", staged.display()))?;
        std::fs::rename(&staged, &path).with_context(|| format!("write {}", path.display()))?;
        match std::fs::remove_file(Self::settled_marker(dir)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("clear {}", Self::settled_marker(dir).display()))
            }
        }
    }

    /// The incarnation directory whose socket directory this is.
    fn vm_dir_for_socket(&self, socket_dir: &Path) -> Option<PathBuf> {
        let short = socket_dir.file_name()?.to_string_lossy().into_owned();
        std::fs::read_dir(self.state_dir.join("process-vms"))
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|dir| {
                dir.file_name()
                    .and_then(|name| Uuid::parse_str(&name.to_string_lossy()).ok())
                    .is_some_and(|vm_id| vm_id.simple().to_string().starts_with(&short))
            })
    }

    fn forget_socket_dir(&self, handle_id: &str) {
        let dir = self.socket_dirs.lock().unwrap().remove(handle_id);
        if let Some(dir) = dir {
            if let Some(vm_dir) = self.vm_dir_for_socket(&dir) {
                let _ = std::fs::write(Self::settled_marker(&vm_dir), b"");
            }
            if let Err(error) = std::fs::remove_dir_all(&dir) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::debug!(%error, dir = %dir.display(), "process socket directory not removed");
                }
            }
        }
    }

    fn command(
        &self,
        spec: &MicrovmRunSpec,
    ) -> anyhow::Result<(tokio::process::Command, Uuid, PathBuf)> {
        // A VM-only boot field reaching this launcher is a host bug, not
        // something to drop quietly (`g5.runtime.boot.backend.process-local`).
        if !spec.extra_mounts.is_empty()
            || !spec.port_maps.is_empty()
            || spec.scratch_disk.is_some()
            || spec.block_workload
        {
            return Err(anyhow!(
                "process launcher received a microVM-only boot field (mounts, port maps, forwards, scratch disk or block workload)"
            ));
        }
        let (program, args) = spec
            .cmd
            .split_first()
            .ok_or_else(|| anyhow!("process launcher received an empty command"))?;
        let vm_id = spec
            .env
            .iter()
            .find(|(key, _)| key == "G5D_VM_ID")
            .and_then(|(_, value)| Uuid::parse_str(value).ok())
            .ok_or_else(|| {
                anyhow!("process launcher requires G5D_VM_ID in the boot environment")
            })?;
        let dir = self.vm_dir(vm_id);
        let home = dir.join("home");
        let state = dir.join("state");
        let tmp = dir.join("tmp");
        // The incarnation directory is built complete, record included, under
        // a staging name and published by one rename: a registry entry either
        // carries its ownership record or does not exist, so a host that dies
        // here leaves nothing a later settlement must guess about (review R19).
        let staging = Self::staging_dir(&dir);
        for path in ["home", "state", "tmp"] {
            let path = staging.join(path);
            std::fs::create_dir_all(&path).with_context(|| format!("create {}", path.display()))?;
        }
        let socket_dir = Self::vm_socket_dir(vm_id)?;
        Self::write_incarnation_record(&staging, vm_id)?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(staging.join("dispatch.log"))
            .with_context(|| format!("open {}", staging.join("dispatch.log").display()))?;
        // The same incarnation starts again after a readiness retry: its
        // previous attempt's directory is in place with the exit proof the
        // kill recorded (`settled`), and is retired, log and all, before the
        // new attempt is published (review R20). A directory without that
        // proof is an unsettled or foreign incarnation and refuses the start.
        match std::fs::symlink_metadata(&dir) {
            // The entry is consulted only when it is the directory a host
            // published; a symlink or a file there is damage and is left in
            // place, whatever lies behind it (review R31).
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(anyhow!(
                    "incarnation {vm_id} is recorded at {} as something other than an incarnation directory; it is not this attempt's to replace",
                    dir.display()
                ));
            }
            Ok(_) => match Self::settled_proof(&dir) {
                Ok(true) => {
                    let retired = Self::retired_dir(&dir);
                    if let Err(error) = std::fs::rename(&dir, &retired) {
                        let _ = std::fs::remove_dir_all(&staging);
                        return Err(error).with_context(|| {
                            format!(
                                "retire the settled attempt {} before a new one",
                                dir.display()
                            )
                        });
                    }
                }
                Ok(false) => {
                    let _ = std::fs::remove_dir_all(&staging);
                    return Err(anyhow!(
                        "incarnation {vm_id} is already recorded at {} without exit proof; it is not this attempt's to replace",
                        dir.display()
                    ));
                }
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&staging);
                    return Err(error.context(format!(
                        "incarnation {vm_id} is already recorded at {} with damaged exit proof; it is not this attempt's to replace",
                        dir.display()
                    )));
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(error).with_context(|| format!("inspect {}", dir.display()));
            }
        }
        if let Err(error) = std::fs::rename(&staging, &dir) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error)
                .with_context(|| format!("publish incarnation record {}", dir.display()));
        }
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args)
            .current_dir(&spec.mount_host_dir)
            .env_clear()
            .envs(spec.env.iter().map(|(key, value)| (key, value)))
            .env("HOME", &home)
            .env("TMPDIR", &tmp)
            .env(PROCESS_HOME_ENV, &home)
            .env(PROCESS_STATE_DIR_ENV, &state)
            .env(PROCESS_SOCKET_DIR_ENV, &socket_dir)
            // Every process of the incarnation inherits this marker, so a stop
            // or a dispatcher exit can also reach work that left the session.
            .env(PROCESS_INCARNATION_ENV, vm_id.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(log.try_clone()?))
            .stderr(std::process::Stdio::from(log));
        // A host running inside an outer incarnation (the `full-stack` preview
        // starts the previewed instance's host) keeps the outer marker on its
        // dispatchers under the outer key, so the outer stop reaches them.
        if let Some(outer) = std::env::var_os(PROCESS_INCARNATION_ENV).filter(|v| !v.is_empty()) {
            cmd.env(PROCESS_INCARNATION_OUTER_ENV, outer);
        }
        tracing::info!(
            %vm_id,
            program = %Path::new(program).display(),
            workspace_root = %spec.mount_host_dir.display(),
            vm_dir = %dir.display(),
            socket_dir = %socket_dir.display(),
            "starting workspace dispatcher as a host process group"
        );
        Ok((cmd, vm_id, socket_dir))
    }
}

#[async_trait]
impl VmLauncher for ProcessVmLauncher {
    async fn run(&self, _spec: MicrovmRunSpec) -> anyhow::Result<RunResult> {
        Err(anyhow!("workspace hosts never use one-shot runs"))
    }

    async fn start(&self, spec: MicrovmRunSpec) -> anyhow::Result<RunHandle> {
        // A session, not just a group: the dispatcher's previews and terminals
        // run in process groups and sessions of their own, and a stop must
        // reach them all; the tag finds those that left the session.
        let (cmd, vm_id, socket_dir) = self.command(&spec)?;
        let handle = ProcessBackend::spawn_session(
            cmd,
            Some(ProcessTag {
                key: PROCESS_INCARNATION_ENV.to_string(),
                value: vm_id.to_string(),
            }),
        )?;
        // Best effort: the pre-spawn record already carries everything
        // settlement needs; the leader pid only lets it name the session.
        if let Some(leader_pid) = handle.child_pid() {
            if let Err(error) = Self::record_leader_pid(&self.vm_dir(vm_id), vm_id, leader_pid) {
                tracing::warn!(%vm_id, %error, "incarnation record kept without the leader pid");
            }
        }
        self.socket_dirs
            .lock()
            .unwrap()
            .insert(handle.id.clone(), socket_dir);
        Ok(handle)
    }

    async fn kill(&self, handle: &RunHandle) -> anyhow::Result<ChildReapReceipt> {
        let receipt = self.backend.kill(handle).await?;
        self.forget_socket_dir(&handle.id);
        Ok(receipt)
    }

    /// A workspace stop tears the incarnation down like a VM teardown: the
    /// whole group dies at once and nothing inside it is signalled first. The
    /// control plane drains through RPC before it stops a VM, the dispatcher
    /// has no shutdown of its own, and a group-wide SIGTERM would reach the
    /// preview, which reads SIGTERM as an owner Close and would run the
    /// profile cleanup a workspace stop must not run
    /// (`g5.preview.service.close`). The grace is therefore not applied to
    /// host processes; exit proof is the waiter's receipt as for every stop.
    async fn stop(
        &self,
        handle: &RunHandle,
        mode: StopMode,
        grace: std::time::Duration,
    ) -> anyhow::Result<ChildReapReceipt> {
        if mode == StopMode::Graceful && !grace.is_zero() {
            tracing::debug!(
                handle = %handle.id,
                ?grace,
                "process mode: stop grace not applicable; the incarnation's process group is torn down at once"
            );
        }
        let receipt = self
            .backend
            .terminate(handle, std::time::Duration::ZERO)
            .await?;
        self.forget_socket_dir(&handle.id);
        Ok(receipt)
    }
}

/// Settle the process-mode incarnations a previous host left under
/// `state_dir`: for every recorded incarnation without a settled marker,
/// tear down what still runs (the leader's session when the leader is still
/// that process, plus everything carrying the incarnation marker) and prove
/// its exit before this host reclaims roots or admits work. Returns how many
/// incarnations were settled; an incarnation whose exit cannot be proven is
/// an error, so the host refuses to start rather than reclaim a live root
/// (`g5.runtime.custody`, review R6). Runs in every host mode, so a flip
/// back to microVMs still settles what a process host left behind.
pub async fn settle_process_incarnations(state_dir: &Path) -> anyhow::Result<usize> {
    let root = state_dir.join("process-vms");
    // An absent registry is a first start; anything else that keeps the
    // registry from being inspected is uncertainty and refuses the start
    // (review R11).
    // A genuinely absent registry is a first start; a registry of any shape
    // other than a directory (a symlink, dangling or not, a file) or one that
    // cannot be inspected is uncertainty and refuses the start (review R27).
    // The registry is never followed as a symlink.
    match std::fs::symlink_metadata(&root) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => anyhow::bail!(
            "the incarnation registry {} is not a directory; a previous host's incarnations cannot be inspected through it",
            root.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("cannot inspect the incarnation registry {}", root.display())
            })
        }
    }
    let entries = std::fs::read_dir(&root)
        .with_context(|| format!("cannot inspect the incarnation registry {}", root.display()))?;
    let mut settled = 0;
    for entry in entries {
        let entry = entry.with_context(|| format!("list {}", root.display()))?;
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(vm_id) = Uuid::parse_str(&name) else {
            // A staging directory a dying host left unpublished recorded no
            // incarnation and spawned nothing, and a retired directory holds a
            // proven attempt's remains; anything else is not ours.
            if name.starts_with(".staging-") || name.starts_with(".retired-") {
                std::fs::remove_dir_all(&dir)
                    .with_context(|| format!("remove unpublished {}", dir.display()))?;
            }
            continue;
        };
        // A recorded incarnation is a directory this host published; a
        // UUID-named entry of any other shape (a file, a symlink, even one to
        // an intact directory) is damage that proves nothing and is refused
        // like a damaged record, never skipped or followed (review R25).
        let meta = std::fs::symlink_metadata(&dir)
            .with_context(|| format!("inspect incarnation {}", dir.display()))?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            anyhow::bail!(
                "incarnation {vm_id} is recorded at {} as something other than an incarnation directory; stop any process carrying {}={vm_id} and remove the entry before starting this host",
                dir.display(),
                PROCESS_INCARNATION_ENV
            );
        }
        if ProcessVmLauncher::settled_proof(&dir).with_context(|| {
            format!("incarnation {vm_id} carries damaged exit proof; stop any process carrying {}={vm_id} and remove the entry before starting this host", PROCESS_INCARNATION_ENV)
        })? {
            continue;
        }
        // A published directory always carries its record (it is published by
        // one rename, record included), so a missing, unreadable or incomplete
        // record is damage, never a first start: without the recorded start
        // time nothing bounds which unreadable processes are the
        // incarnation's, and the directory's own timestamps move with the
        // damage itself. Refuse rather than guess (review R19).
        let record_path = ProcessVmLauncher::incarnation_record(&dir);
        let record: serde_json::Value = std::fs::read(&record_path)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(anyhow::Error::from))
            .with_context(|| {
                format!(
                    "incarnation {vm_id} has no readable ownership record at {}; stop any process carrying {}={vm_id} and remove the directory before starting this host",
                    record_path.display(),
                    PROCESS_INCARNATION_ENV
                )
            })?;
        // The record must be this directory's, complete and plausible before
        // its fence is trusted: a foreign or malformed record, or a fence in
        // the future (a copied later record, a clock that moved backwards),
        // cannot say which unreadable processes are not this incarnation's
        // (review R32).
        let damaged = |what: &str| {
            anyhow!(
                "incarnation {vm_id} has an ownership record that {what} at {}; stop any process carrying {}={vm_id} and remove the directory before starting this host",
                record_path.display(),
                PROCESS_INCARNATION_ENV
            )
        };
        if record["vm_id"] != serde_json::json!(vm_id) {
            return Err(damaged("names another incarnation"));
        }
        if !record["leader_pid"].is_null()
            && record["leader_pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok())
                .is_none()
        {
            return Err(damaged("has a malformed leader pid"));
        }
        let started = record["started_unix_ms"]
            .as_u64()
            .and_then(|ms| std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_millis(ms)))
            .ok_or_else(|| damaged("has no usable start time"))?;
        if started > std::time::SystemTime::now() {
            return Err(damaged("starts in the future"));
        }
        let started = Some(started);
        let tag = ProcessTag {
            key: PROCESS_INCARNATION_ENV.to_string(),
            value: vm_id.to_string(),
        };
        // The recorded leader pid names the session only while that pid is
        // still a process of this incarnation; otherwise only the marker can
        // attribute processes.
        let leader = record["leader_pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok());
        let session = leader.filter(|pid| {
            g5d_foundation::process_termination::incarnation_census(
                None, None, &tag.key, &tag.value, started,
            )
            .is_ok_and(|census| census.tagged.contains(pid))
        });
        // A dead host's orphans were reparented away from any tree this host
        // can see; no adopter is named, so every eligible unreadable process
        // stays uncertain (review R15).
        g5d_isolation::teardown_incarnation(
            session,
            Some(&tag),
            started,
            None,
            std::time::Duration::from_secs(10),
        )
        .await
        .with_context(|| {
            format!(
                "incarnation {vm_id} left by a previous host is still alive or unproven; stop its processes before starting this host"
            )
        })?;
        std::fs::write(ProcessVmLauncher::settled_marker(&dir), b"")
            .with_context(|| format!("mark incarnation {vm_id} settled"))?;
        settled += 1;
        tracing::info!(%vm_id, "settled a process-mode incarnation left by a previous host");
    }
    Ok(settled)
}

/// P4.9 removes every competing task-scoped slot owner. Workspace VMs therefore
/// consume the host budget directly.
#[derive(Default)]
pub struct NoOtherSlotUsage;

#[async_trait]
impl OtherSlotUsage for NoOtherSlotUsage {
    async fn slots_in_use(&self) -> usize {
        0
    }
}

/// Read-only projection used by the authenticated host control stream.
pub struct WorkspaceHostState {
    host_id: Uuid,
    slots_total: u32,
    workspaces_root: PathBuf,
    runtime: Arc<WorkspaceVmRuntime>,
    incarnation_generation: AtomicU64,
    /// The newest roster generation any caller has asked to apply; an older
    /// caller still waiting for the lock leaves without work.
    requested_generation: AtomicU64,
    /// Serialize roster admission, never the resulting exit-proof waits.
    reconciliation: tokio::sync::Mutex<()>,
}

impl WorkspaceHostState {
    #[must_use]
    pub fn new(
        host_id: Uuid,
        slots_total: u32,
        workspaces_root: PathBuf,
        runtime: Arc<WorkspaceVmRuntime>,
    ) -> Self {
        Self {
            host_id,
            slots_total,
            workspaces_root,
            runtime,
            incarnation_generation: AtomicU64::new(0),
            requested_generation: AtomicU64::new(0),
            reconciliation: tokio::sync::Mutex::new(()),
        }
    }

    async fn inventory(&self) -> Vec<VmInventory> {
        self.runtime.inventory().await
    }

    async fn vm_heartbeats(&self) -> Vec<VmHeartbeat> {
        self.runtime.heartbeat_vms().await
    }
}

#[async_trait]
impl HostControlState for WorkspaceHostState {
    async fn hello(&self) -> HostHello {
        let mut capabilities = vec![
            "workspace_vm:v1".to_string(),
            "workspace_dispatch:v1".to_string(),
            self.runtime.isolation_capability().to_string(),
        ];
        if self.runtime.can_prepare_sources() {
            capabilities.push(g5_internal_api::host_ctl::CAPABILITY_SOURCE_PREPARATION.into());
        }
        if self.runtime.nested_virtualization() {
            capabilities.push(g5_internal_api::host_ctl::CAPABILITY_NESTED_VIRTUALIZATION.into());
        }
        // Every host forwards module-event hints from its dispatcher control
        // connections; the control plane enables them in its accept.
        capabilities.push(g5_internal_api::host_ctl::CAPABILITY_MODULE_EVENT_HINTS.into());
        HostHello {
            host_id: self.host_id,
            protocol_version: PROTOCOL_VERSION,
            capabilities,
            slots_total: self.slots_total,
            slot_size: Some(self.runtime.slot_size()),
            inventory: self.inventory().await,
            retained_roots: Vec::new(),
            runtime: self.runtime.runtime_identity(),
        }
    }

    async fn heartbeat(&self) -> HostHeartbeat {
        HostHeartbeat {
            refresh_source_preparation: false,
            slots_in_use: u32::try_from(self.runtime.active_count().await).unwrap_or(u32::MAX),
            boots_in_progress: self.runtime.boots_in_progress().await,
            load: system_load(),
            memory_free_mib: memory_free_mib(),
            disk_free_mib: disk_free_mib(&self.workspaces_root),
            retained_roots: Vec::new(),
            vms: self.vm_heartbeats().await,
        }
    }

    async fn active_vm_count(&self) -> usize {
        self.runtime.active_count().await
    }

    /// Order roster admission by generation. The runtime installs independent
    /// cleanup owners before returning, so a stuck exit proof cannot block a
    /// newer roster or the reconnect handshake.
    async fn reconcile_current_incarnations(
        &self,
        generation: u64,
        current: &[CurrentIncarnation],
    ) {
        let newest = self
            .requested_generation
            .fetch_max(generation, Ordering::AcqRel)
            .max(generation);
        if generation < newest {
            return;
        }
        let _ordered = self.reconciliation.lock().await;
        if self.requested_generation.load(Ordering::Acquire) > generation {
            return;
        }
        if generation > self.incarnation_generation.load(Ordering::Acquire) {
            self.runtime.reconcile(current).await;
            // Only mark applied after every cleanup owner is installed. A
            // cancelled admission can be retried with the same generation.
            self.incarnation_generation
                .store(generation, Ordering::Release);
        }
    }
}

/// Hidden-environment fixtures make a child non-dumpable so its `environ`
/// cannot be read; a root runner reads it anyway (`CAP_SYS_PTRACE`), so those
/// tests skip loudly instead of asserting a precondition the kernel grants.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn environment_can_be_hidden() -> bool {
    // SAFETY: geteuid has no failure modes and touches no memory.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("SKIP: root reads every environment; the hidden-environment fixture needs an unprivileged runner");
        return false;
    }
    true
}

#[cfg(all(test, unix))]
mod process_launcher_tests {
    use super::*;
    use std::time::Duration;

    fn spec(cmd: &[&str], root: &Path, vm_id: Uuid) -> MicrovmRunSpec {
        MicrovmRunSpec {
            image: String::new(),
            cmd: cmd.iter().map(|part| part.to_string()).collect(),
            env: vec![
                ("G5D_VM_ID".to_string(), vm_id.to_string()),
                (
                    "PATH".to_string(),
                    std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
                ),
            ],
            mount_host_dir: root.to_path_buf(),
            mount_guest_dir: root.to_string_lossy().into_owned(),
            extra_mounts: Vec::new(),
            cpus: 1,
            mem_mib: 512,
            port_maps: Vec::new(),

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        }
    }

    /// `g5.runtime.boot.backend.process-local`: the incarnation's home, state
    /// and log live beside the host state, never in the checkout, and the
    /// handle names the host loopback so readiness dials it unchanged.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn launcher_keeps_incarnation_state_beside_the_host_and_publishes_loopback() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let vm_id = Uuid::new_v4();
        let handle = launcher
            .start(spec(
                &[
                    "sh",
                    "-c",
                    r#"printf '%s\n%s\n%s' "$HOME" "$TMPDIR" "$PWD" >&2; exit 3"#,
                ],
                &workspace,
                vm_id,
            ))
            .await
            .unwrap();
        assert_eq!(
            handle.vmm_loopback_addr,
            Some(std::net::Ipv4Addr::LOCALHOST)
        );
        let receipt = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(receipt) = handle.current_reap_receipt().unwrap() {
                    break receipt;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("natural exit publishes a reap receipt");
        assert_eq!(receipt.exit_code(), Some(3));
        let dir = launcher.vm_dir(vm_id);
        let log = std::fs::read_to_string(dir.join("dispatch.log")).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines[0], dir.join("home").to_string_lossy());
        assert_eq!(lines[1], dir.join("tmp").to_string_lossy());
        // The shell reports its working directory resolved (macOS `/var` is
        // `/private/var`), so compare canonical paths.
        assert_eq!(
            std::fs::canonicalize(lines[2]).unwrap(),
            std::fs::canonicalize(&workspace).unwrap()
        );
        assert!(dir.join("state").is_dir());
        assert!(
            std::fs::read_dir(&workspace).unwrap().next().is_none(),
            "nothing may be created inside the workspace root"
        );
        // Kill after a natural exit returns the same receipt.
        assert_eq!(launcher.kill(&handle).await.unwrap(), receipt);
    }

    fn python3() -> Option<&'static str> {
        std::process::Command::new("python3")
            .arg("-c")
            .arg("import subprocess, signal")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|_| "python3")
    }

    /// Whether `pid` is still a running process. A killed orphan stays a
    /// zombie until init reaps it, which is asynchronous; a zombie runs
    /// nothing and counts as gone, as the teardown itself treats it.
    fn running(pid: i32) -> bool {
        // SAFETY: signal zero only probes process existence.
        let exists = unsafe { libc::kill(pid, 0) } == 0;
        exists && !zombie(pid)
    }

    #[cfg(target_os = "linux")]
    fn zombie(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |stat| {
            stat.rsplit_once(')')
                .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'))
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn zombie(pid: i32) -> bool {
        std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .starts_with('Z')
            })
    }

    /// `running` with a short allowance for the kernel to deliver SIGKILL.
    async fn settles_dead(pid: i32) -> bool {
        tokio::time::timeout(Duration::from_secs(2), async {
            while running(pid) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    async fn pid_from(path: &Path) -> i32 {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(pid) = std::fs::read_to_string(path)
                    .ok()
                    .and_then(|text| text.trim().parse().ok())
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("child did not publish its pid")
    }

    /// Review R2: the Git socket path stays within the Unix bound however
    /// long the checkout and host state paths are, and nothing of the
    /// socket lives under the checkout.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn launcher_gives_the_git_socket_a_bounded_path_under_a_long_state_root() {
        let temp = tempfile::tempdir().unwrap();
        let long = "a-very-long-checkout-directory-name-that-pushes-the-state-path-well-past-the-socket-bound";
        let state = temp.path().join(long).join(long).join("state");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        assert!(state.as_os_str().len() > SOCKET_PATH_MAX);
        let launcher = ProcessVmLauncher::new(state.clone());
        let vm_id = Uuid::new_v4();
        let handle = launcher
            .start(spec(
                &[
                    "sh",
                    "-c",
                    r#"printf '%s\n%s\n' "$G5D_PROCESS_SOCKET_DIR" "$G5D_PROCESS_INCARNATION" >&2"#,
                ],
                &workspace,
                vm_id,
            ))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while handle.current_reap_receipt().unwrap().is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let log = std::fs::read_to_string(launcher.vm_dir(vm_id).join("dispatch.log")).unwrap();
        let mut lines = log.lines();
        let socket_dir = PathBuf::from(lines.next().unwrap());
        assert_eq!(lines.next().unwrap(), vm_id.to_string());
        let socket = socket_dir.join(g5_contracts::workspace_git::SOCKET_FILE_NAME);
        assert!(
            socket.as_os_str().len() < SOCKET_PATH_MAX,
            "{} must fit a Unix socket path",
            socket.display()
        );
        assert!(!socket_dir.starts_with(&state) && !socket_dir.starts_with(&workspace));
        assert!(socket_dir.is_dir(), "{}", socket_dir.display());
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&socket_dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        // A real bind at that path succeeds, which is what the dispatcher does.
        let listener =
            std::os::unix::net::UnixListener::bind(&socket).expect("the bounded socket path binds");
        drop(listener);
        launcher.kill(&handle).await.unwrap();
        assert!(
            !socket_dir.exists(),
            "the socket directory is removed after the stop"
        );
    }

    /// Review R3: a leader that exits while work it started is still running
    /// (a dispatcher crash under a live preview) does not yield exit proof
    /// until that work is gone.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn exit_proof_waits_for_the_descendants_of_a_dead_leader() {
        let Some(python) = python3() else {
            eprintln!("SKIP: python3 is required for the descendant fixture");
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let child_pid = temp.path().join("child.pid");
        let code = format!(
            "import subprocess; p = subprocess.Popen(['sleep', '600']); open({:?}, 'w').write(str(p.pid))",
            child_pid.to_string_lossy()
        );
        let handle = launcher
            .start(spec(&[python, "-c", &code], temp.path(), Uuid::new_v4()))
            .await
            .unwrap();
        let orphan = pid_from(&child_pid).await;
        let receipt = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(receipt) = handle.current_reap_receipt().unwrap() {
                    break receipt;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the leader's exit becomes a receipt once its work is gone");
        assert_eq!(receipt.exit_code(), Some(0));
        assert!(
            !running(orphan),
            "exit proof was published while the leader's child {orphan} was running"
        );
        assert_eq!(
            launcher
                .stop(&handle, StopMode::Kill, Duration::ZERO)
                .await
                .unwrap(),
            receipt
        );
    }

    /// Review R6: a host that dies leaves its incarnations running; the next
    /// host settles them from the ownership record, proving their exit,
    /// before it reclaims anything. `host_child_fixture` plays the dead host.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_restarted_host_settles_live_incarnations_before_reclaiming_roots() {
        if python3().is_none() {
            eprintln!("SKIP: python3 is required for the host-death fixture");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let mut host = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime::process_launcher_tests::host_child_fixture",
                "--include-ignored",
                "--nocapture",
            ])
            .env("G5D_TEST_HOST_CHILD", temp.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .unwrap();
        let leader = pid_from(&temp.path().join("leader.pid")).await;
        let detached = pid_from(&temp.path().join("detached.pid")).await;
        // The host dies without any shutdown path.
        host.kill().unwrap();
        host.wait().unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        // On Linux the parent-death backstop takes the leader; the detached
        // process is outside it in every case and is what settlement is for.
        assert!(
            running(detached),
            "the detached process survives host death"
        );
        let settled = settle_process_incarnations(&state).await.unwrap();
        assert_eq!(settled, 1);
        assert!(settles_dead(leader).await, "leader {leader} still running");
        assert!(
            settles_dead(detached).await,
            "detached {detached} still running"
        );
        let dirs: Vec<_> = std::fs::read_dir(state.join("process-vms"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(dirs.len(), 1);
        assert!(ProcessVmLauncher::settled_marker(&dirs[0]).exists());
        assert_eq!(settle_process_incarnations(&state).await.unwrap(), 0);
        assert_eq!(settle_process_incarnations(temp.path()).await.unwrap(), 0);
    }

    /// Review R15: a dead host's hidden survivor (marker kept, environment
    /// unreadable) is uncertainty for the next host: settlement refuses, no
    /// settled marker is written, and only once the survivor is gone does the
    /// incarnation settle.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_restarted_host_refuses_to_settle_while_a_hidden_survivor_lives() {
        if python3().is_none() {
            eprintln!("SKIP: python3 is required for the host-death fixture");
            return;
        }
        if !environment_can_be_hidden() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let mut host = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime::process_launcher_tests::host_child_fixture",
                "--include-ignored",
                "--nocapture",
            ])
            .env("G5D_TEST_HOST_CHILD", temp.path())
            .env("G5D_TEST_HOST_CHILD_HIDDEN", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .unwrap();
        let hidden = pid_from(&temp.path().join("detached.pid")).await;
        tokio::time::timeout(Duration::from_secs(10), async {
            while !temp.path().join("hidden.marker").exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the hidden child hid its environment");
        assert!(std::fs::read(format!("/proc/{hidden}/environ")).is_err());
        host.kill().unwrap();
        host.wait().unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            running(hidden),
            "the hidden survivor outlives the dead host"
        );
        let error = settle_process_incarnations(&state)
            .await
            .expect_err("a live hidden survivor is uncertainty, not a settled incarnation");
        let chain = format!("{error:#}");
        assert!(chain.contains(&hidden.to_string()), "{chain}");
        let dirs: Vec<_> = std::fs::read_dir(state.join("process-vms"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(dirs.len(), 1);
        assert!(!ProcessVmLauncher::settled_marker(&dirs[0]).exists());
        // SAFETY: the test owns this fixture process.
        unsafe {
            libc::kill(hidden, libc::SIGKILL);
        }
        assert!(settles_dead(hidden).await);
        assert_eq!(settle_process_incarnations(&state).await.unwrap(), 1);
        assert!(ProcessVmLauncher::settled_marker(&dirs[0]).exists());
    }

    /// Plays a host that starts one incarnation (a leader with a detached
    /// child) and then dies; driven by the test above, never run directly.
    #[tokio::test]
    #[ignore = "fixture process for a_restarted_host_settles_live_incarnations_before_reclaiming_roots"]
    async fn host_child_fixture() {
        let Some(root) = std::env::var_os("G5D_TEST_HOST_CHILD") else {
            return;
        };
        let root = PathBuf::from(root);
        let launcher = ProcessVmLauncher::new(root.join("state"));
        // With `G5D_TEST_HOST_CHILD_HIDDEN` the detached child keeps its
        // marker but hides its environment (review R15).
        let detached = if std::env::var_os("G5D_TEST_HOST_CHILD_HIDDEN").is_some() {
            format!(
                "['python3', '-c', 'import ctypes, time; ctypes.CDLL(None).prctl(4, 0, 0, 0, 0); open({:?}, \"w\").write(\"hidden\"); time.sleep(600)']",
                root.join("hidden.marker").to_string_lossy()
            )
        } else {
            "['sleep', '600']".to_string()
        };
        let code = format!(
            "import os, subprocess, time\n\
             open({:?}, 'w').write(str(os.getpid()))\n\
             p = subprocess.Popen({detached}, start_new_session=True)\n\
             open({:?}, 'w').write(str(p.pid))\n\
             time.sleep(600)",
            root.join("leader.pid").to_string_lossy(),
            root.join("detached.pid").to_string_lossy()
        );
        let _handle = launcher
            .start(spec(&["python3", "-c", &code], &root, Uuid::new_v4()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(600)).await;
    }

    /// Review R19: a published incarnation directory always carries its
    /// record; one whose record is missing, unreadable as JSON or without a
    /// start time is damage and refuses settlement instead of being treated
    /// as a first start or bounded by the directory's own timestamps. An
    /// unpublished staging directory recorded nothing and is removed.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_damaged_ownership_record_refuses_settlement() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let registry = state.join("process-vms");
        let vm_id = Uuid::new_v4();
        let dir = registry.join(vm_id.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        for (damage, expect) in [
            (None, "no readable ownership record"),
            (Some("not json"), "no readable ownership record"),
            (
                Some(r#"{"vm_id":"VM","leader_pid":null}"#),
                "no usable start time",
            ),
        ] {
            match damage {
                Some(text) => std::fs::write(
                    ProcessVmLauncher::incarnation_record(&dir),
                    text.replace("VM", &vm_id.to_string()),
                )
                .unwrap(),
                None => {
                    let _ = std::fs::remove_file(ProcessVmLauncher::incarnation_record(&dir));
                }
            }
            let error = settle_process_incarnations(&state)
                .await
                .expect_err("a damaged record is uncertainty, not a settled incarnation");
            let chain = format!("{error:#}");
            assert!(chain.contains(expect), "{chain}");
            assert!(chain.contains(&vm_id.to_string()), "{chain}");
            assert!(!ProcessVmLauncher::settled_marker(&dir).exists());
        }
        // Repairing the record settles the (empty) incarnation normally.
        std::fs::write(
            ProcessVmLauncher::incarnation_record(&dir),
            serde_json::to_vec(&serde_json::json!({
                "vm_id": vm_id,
                "leader_pid": null,
                "started_unix_ms": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            }))
            .unwrap(),
        )
        .unwrap();
        let staging = ProcessVmLauncher::staging_dir(&registry.join(Uuid::new_v4().to_string()));
        std::fs::create_dir_all(staging.join("home")).unwrap();
        assert_eq!(settle_process_incarnations(&state).await.unwrap(), 1);
        assert!(ProcessVmLauncher::settled_marker(&dir).exists());
        assert!(
            !staging.exists(),
            "an unpublished staging directory is removed"
        );
    }

    /// Review R27: a registry that exists as anything other than a directory
    /// (a dangling symlink reads as absent to `read_dir`) is uncertainty, not
    /// a first start; only a genuinely absent registry is.
    #[cfg(unix)]
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_registry_that_is_not_a_directory_refuses_settlement() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        let registry = state.join("process-vms");
        std::os::unix::fs::symlink(temp.path().join("absent"), &registry).unwrap();
        let error = settle_process_incarnations(&state)
            .await
            .expect_err("a dangling registry symlink is not a first start");
        assert!(
            format!("{error:#}").contains("not a directory"),
            "{error:#}"
        );
        std::fs::remove_file(&registry).unwrap();
        let preserved = temp.path().join("saved-process-vms");
        std::fs::create_dir_all(&preserved).unwrap();
        std::os::unix::fs::symlink(&preserved, &registry).unwrap();
        assert!(settle_process_incarnations(&state).await.is_err());
        std::fs::remove_file(&registry).unwrap();
        std::fs::write(&registry, b"not a directory").unwrap();
        assert!(settle_process_incarnations(&state).await.is_err());
        std::fs::remove_file(&registry).unwrap();
        assert_eq!(settle_process_incarnations(&state).await.unwrap(), 0);
    }

    /// Review R28: exit proof is the empty regular file the proof publisher
    /// writes; a directory or a symlink at the marker's path is damage, which
    /// refuses settlement and refuses a same-incarnation start instead of
    /// counting as exit.
    #[cfg(unix)]
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn damaged_exit_proof_is_not_exit() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let vm_id = Uuid::new_v4();
        let dir = launcher.vm_dir(vm_id);
        std::fs::create_dir_all(&dir).unwrap();
        ProcessVmLauncher::write_incarnation_record(&dir, vm_id).unwrap();
        let marker = ProcessVmLauncher::settled_marker(&dir);
        for damage in ["directory", "dangling", "resolving", "nonempty"] {
            match damage {
                "directory" => std::fs::create_dir(&marker).unwrap(),
                "dangling" => {
                    std::os::unix::fs::symlink(temp.path().join("absent"), &marker).unwrap()
                }
                "nonempty" => std::fs::write(&marker, b"not exit proof").unwrap(),
                _ => std::os::unix::fs::symlink(temp.path().join("state"), &marker).unwrap(),
            }
            let error = settle_process_incarnations(&temp.path().join("state"))
                .await
                .expect_err("damaged exit proof is not exit");
            let chain = format!("{error:#}");
            assert!(chain.contains("damaged exit proof"), "{damage}: {chain}");
            assert!(chain.contains(&vm_id.to_string()), "{damage}: {chain}");
            let error = launcher
                .start(spec(&["sleep", "600"], temp.path(), vm_id))
                .await
                .expect_err("damaged exit proof does not retire the attempt");
            assert!(
                format!("{error:#}").contains("damaged exit proof"),
                "{damage}: {error:#}"
            );
            assert!(
                dir.exists(),
                "{damage}: the recorded attempt stays in place"
            );
            if damage == "directory" {
                std::fs::remove_dir(&marker).unwrap();
            } else {
                std::fs::remove_file(&marker).unwrap();
            }
        }
        // Without the damage the (empty) incarnation settles and writes real
        // proof, and the genuine proof lets the incarnation start again.
        assert_eq!(
            settle_process_incarnations(&temp.path().join("state"))
                .await
                .unwrap(),
            1
        );
        assert!(std::fs::symlink_metadata(&marker).unwrap().is_file());
        let handle = launcher
            .start(spec(&["sleep", "600"], temp.path(), vm_id))
            .await
            .unwrap();
        launcher.kill(&handle).await.unwrap();
    }

    /// Review R31: a same-incarnation start consults an entry only when it is
    /// the directory a host published; a symlink to a foreign, genuinely
    /// settled directory is damage that refuses and stays in place.
    #[cfg(unix)]
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_symlinked_entry_refuses_a_same_incarnation_start() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let foreign = temp.path().join("foreign");
        std::fs::create_dir_all(&foreign).unwrap();
        ProcessVmLauncher::write_incarnation_record(&foreign, Uuid::new_v4()).unwrap();
        std::fs::write(ProcessVmLauncher::settled_marker(&foreign), b"").unwrap();
        let vm_id = Uuid::new_v4();
        let dir = launcher.vm_dir(vm_id);
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&foreign, &dir).unwrap();
        let error = launcher
            .start(spec(&["sleep", "600"], temp.path(), vm_id))
            .await
            .expect_err("a symlinked entry is not this attempt's to replace");
        assert!(
            format!("{error:#}").contains("other than an incarnation directory"),
            "{error:#}"
        );
        assert!(std::fs::symlink_metadata(&dir)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(foreign.exists());
        let names: Vec<_> = std::fs::read_dir(temp.path().join("state/process-vms"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec![vm_id.to_string()],
            "nothing retired, nothing staged"
        );
    }

    /// Review R32: an ownership record is trusted only when it names its
    /// directory's incarnation, is complete and records a fence in the past;
    /// otherwise it is damage that refuses settlement, and a huge start time
    /// is refused rather than overflowing.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn an_ownership_record_must_name_its_directory_with_a_past_fence() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let vm_id = Uuid::new_v4();
        let dir = state.join("process-vms").join(vm_id.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let write = |record: serde_json::Value| {
            std::fs::write(
                ProcessVmLauncher::incarnation_record(&dir),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap()
        };
        for (record, expect) in [
            (
                serde_json::json!({"vm_id": Uuid::new_v4(), "leader_pid": null, "started_unix_ms": now_ms}),
                "names another incarnation",
            ),
            (
                serde_json::json!({"leader_pid": null, "started_unix_ms": now_ms}),
                "names another incarnation",
            ),
            (
                serde_json::json!({"vm_id": vm_id, "leader_pid": "4242", "started_unix_ms": now_ms}),
                "malformed leader pid",
            ),
            (
                serde_json::json!({"vm_id": vm_id, "leader_pid": null, "started_unix_ms": now_ms + 60_000}),
                "starts in the future",
            ),
            // The largest value converts without overflow and is simply
            // ahead of the clock.
            (
                serde_json::json!({"vm_id": vm_id, "leader_pid": null, "started_unix_ms": u64::MAX}),
                "starts in the future",
            ),
        ] {
            write(record);
            let error = settle_process_incarnations(&state)
                .await
                .expect_err("a damaged record is uncertainty");
            let chain = format!("{error:#}");
            assert!(chain.contains(expect), "{chain}");
            assert!(!ProcessVmLauncher::settled_marker(&dir).exists());
        }
        // The pre-spawn record (leader unknown) with a past fence settles.
        write(serde_json::json!({"vm_id": vm_id, "leader_pid": null, "started_unix_ms": now_ms}));
        assert_eq!(settle_process_incarnations(&state).await.unwrap(), 1);
    }

    /// Review R25: a UUID-named registry entry that is not a directory (a
    /// file, or a symlink even to an intact incarnation directory) proves
    /// nothing and refuses settlement instead of being skipped or followed.
    #[cfg(unix)]
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_registry_entry_that_is_not_a_directory_refuses_settlement() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let registry = state.join("process-vms");
        std::fs::create_dir_all(&registry).unwrap();
        // An intact incarnation directory preserved under another name.
        let preserved = temp.path().join("preserved");
        std::fs::create_dir_all(&preserved).unwrap();
        let vm_id = Uuid::new_v4();
        ProcessVmLauncher::write_incarnation_record(&preserved, vm_id).unwrap();
        let entry = registry.join(vm_id.to_string());
        std::os::unix::fs::symlink(&preserved, &entry).unwrap();
        let error = settle_process_incarnations(&state)
            .await
            .expect_err("a symlinked entry is damage, not an incarnation");
        let chain = format!("{error:#}");
        assert!(chain.contains(&vm_id.to_string()), "{chain}");
        assert!(
            chain.contains("other than an incarnation directory"),
            "{chain}"
        );
        assert!(!ProcessVmLauncher::settled_marker(&preserved).exists());
        std::fs::remove_file(&entry).unwrap();
        let file_id = Uuid::new_v4();
        std::fs::write(registry.join(file_id.to_string()), b"not a directory").unwrap();
        let error = settle_process_incarnations(&state)
            .await
            .expect_err("a file entry is damage, not an incarnation");
        assert!(format!("{error:#}").contains(&file_id.to_string()));
        std::fs::remove_file(registry.join(file_id.to_string())).unwrap();
        assert_eq!(settle_process_incarnations(&state).await.unwrap(), 0);
    }

    /// Review R20: a readiness retry starts the same incarnation again after
    /// its first attempt's exit was proven; the proven attempt is retired and
    /// the new one published, while a directory without exit proof refuses.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_proven_attempt_is_retired_when_the_same_incarnation_starts_again() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let vm_id = Uuid::new_v4();
        let dir = launcher.vm_dir(vm_id);
        let first = launcher
            .start(spec(&["sleep", "600"], temp.path(), vm_id))
            .await
            .unwrap();
        let first_pid = first.child_pid().unwrap() as i32;
        // Without exit proof the directory is not this attempt's to replace.
        let error = launcher
            .start(spec(&["sleep", "600"], temp.path(), vm_id))
            .await
            .expect_err("an unsettled incarnation refuses a second start");
        assert!(error.to_string().contains("without exit proof"), "{error}");
        assert!(running(first_pid));
        launcher.kill(&first).await.unwrap();
        assert!(ProcessVmLauncher::settled_marker(&dir).exists());
        let second = launcher
            .start(spec(&["sleep", "600"], temp.path(), vm_id))
            .await
            .expect("a proven attempt may be started again");
        assert!(!ProcessVmLauncher::settled_marker(&dir).exists());
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(ProcessVmLauncher::incarnation_record(&dir)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            record["leader_pid"],
            serde_json::json!(second.child_pid().unwrap())
        );
        let retired: Vec<_> = std::fs::read_dir(temp.path().join("state/process-vms"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".retired-"))
            })
            .collect();
        assert_eq!(retired.len(), 1, "{retired:?}");
        assert!(ProcessVmLauncher::settled_marker(&retired[0]).exists());
        assert!(retired[0].join("dispatch.log").exists());
        launcher.kill(&second).await.unwrap();
        // The next host start sweeps the retired attempt with the registry.
        assert_eq!(
            settle_process_incarnations(&temp.path().join("state"))
                .await
                .unwrap(),
            0
        );
        assert!(!retired[0].exists());
    }

    /// Review R23: the leader-pid update keeps the pre-spawn start fence, so
    /// a member that started between the spawn and the update stays inside
    /// the recorded interval; an update without the pre-spawn record fails.
    #[test]
    fn the_leader_pid_update_keeps_the_start_fence() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("vm");
        std::fs::create_dir_all(&dir).unwrap();
        let vm_id = Uuid::new_v4();
        let read = || -> serde_json::Value {
            serde_json::from_slice(
                &std::fs::read(ProcessVmLauncher::incarnation_record(&dir)).unwrap(),
            )
            .unwrap()
        };
        ProcessVmLauncher::write_incarnation_record(&dir, vm_id).unwrap();
        let before = read();
        let fence = before["started_unix_ms"].as_u64().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        ProcessVmLauncher::record_leader_pid(&dir, vm_id, 4242).unwrap();
        let after = read();
        assert_eq!(after["started_unix_ms"].as_u64().unwrap(), fence);
        assert_eq!(after["leader_pid"], serde_json::json!(4242));
        assert_eq!(after["vm_id"], serde_json::json!(vm_id));
        std::fs::remove_file(ProcessVmLauncher::incarnation_record(&dir)).unwrap();
        assert!(ProcessVmLauncher::record_leader_pid(&dir, vm_id, 4242).is_err());
        std::fs::write(
            ProcessVmLauncher::incarnation_record(&dir),
            br#"{"vm_id":"other"}"#,
        )
        .unwrap();
        assert!(ProcessVmLauncher::record_leader_pid(&dir, vm_id, 4242).is_err());
    }

    /// Review R19: the incarnation directory is published complete by one
    /// rename, so a start never leaves a directory without its record.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_started_incarnation_is_published_with_its_record() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let vm_id = Uuid::new_v4();
        let handle = launcher
            .start(spec(&["sleep", "600"], temp.path(), vm_id))
            .await
            .unwrap();
        let dir = launcher.vm_dir(vm_id);
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(ProcessVmLauncher::incarnation_record(&dir)).unwrap(),
        )
        .unwrap();
        assert_eq!(record["vm_id"], serde_json::json!(vm_id));
        assert!(record["started_unix_ms"].as_u64().is_some());
        assert!(record["leader_pid"].as_u64().is_some());
        assert!(dir.join("dispatch.log").exists());
        assert!(!ProcessVmLauncher::staging_dir(&dir).exists());
        launcher.kill(&handle).await.unwrap();
    }

    /// Review R10: ownership is recorded before anything is spawned, so a
    /// record that cannot be written starts no process at all.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_failed_ownership_record_starts_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let vm_id = Uuid::new_v4();
        // A directory where the record file must go: a deterministic EISDIR.
        std::fs::create_dir_all(launcher.vm_dir(vm_id).join("incarnation.json")).unwrap();
        let marker = temp.path().join("started.txt");
        let error = launcher
            .start(spec(
                &[
                    "sh",
                    "-c",
                    &format!("touch '{}'; sleep 600", marker.display()),
                ],
                temp.path(),
                vm_id,
            ))
            .await
            .expect_err("a record failure must fail the start");
        assert!(error.to_string().contains("incarnation"), "{error}");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            !marker.exists(),
            "no process may run without its ownership record"
        );
        assert!(
            !ProcessVmLauncher::staging_dir(&launcher.vm_dir(vm_id)).exists(),
            "a failed publication leaves no staging directory"
        );
        // The unreadable record is uncertainty for settlement too, and so is
        // a published directory without one (review R19); only removing the
        // foreign directory lets the host start.
        assert!(settle_process_incarnations(&temp.path().join("state"))
            .await
            .is_err());
        std::fs::remove_dir(launcher.vm_dir(vm_id).join("incarnation.json")).unwrap();
        assert!(settle_process_incarnations(&temp.path().join("state"))
            .await
            .is_err());
        std::fs::remove_dir_all(launcher.vm_dir(vm_id)).unwrap();
        assert_eq!(
            settle_process_incarnations(&temp.path().join("state"))
                .await
                .unwrap(),
            0
        );
    }

    /// Review R11: a registry that exists but cannot be inspected refuses the
    /// host instead of passing for a first start.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn an_unreadable_registry_refuses_settlement() {
        // SAFETY: geteuid has no failure modes.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("SKIP: root ignores directory permissions");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let launcher = ProcessVmLauncher::new(state.clone());
        let handle = launcher
            .start(spec(&["sleep", "600"], temp.path(), Uuid::new_v4()))
            .await
            .unwrap();
        let registry = state.join("process-vms");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&registry, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let result = settle_process_incarnations(&state).await;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&registry, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let error = result.expect_err("an uninspectable registry is not a first start");
        assert!(error.to_string().contains("registry"), "{error}");
        assert_eq!(
            settle_process_incarnations(&temp.path().join("absent"))
                .await
                .unwrap(),
            0,
            "an absent registry is a first start"
        );
        launcher.kill(&handle).await.unwrap();
    }

    /// Review R7: a detached child that keeps its marker but makes its
    /// environment unreadable is uncertainty, not exit. A stop reports the
    /// unproven exit instead of a receipt, custody is kept, and once the
    /// process is gone a later stop succeeds.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn an_unreadable_marker_is_uncertainty_not_exit_proof() {
        let Some(python) = python3() else {
            eprintln!("SKIP: python3 is required for the hidden-marker fixture");
            return;
        };
        if !environment_can_be_hidden() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let vm_id = Uuid::new_v4();
        let hidden_pid = temp.path().join("hidden.pid");
        let marker = temp.path().join("marker.txt");
        let hidden_py = temp.path().join("hidden.py");
        std::fs::write(
            &hidden_py,
            format!(
                "import ctypes, os, time\nctypes.CDLL(None).prctl(4, 0, 0, 0, 0)\nopen({:?}, 'w').write(str(os.getpid()))\nopen({:?}, 'w').write(os.environ['G5D_PROCESS_INCARNATION'])\ntime.sleep(600)\n",
                hidden_pid.to_string_lossy(),
                marker.to_string_lossy()
            ),
        )
        .unwrap();
        // Double fork: the hidden child's parent exits, so its ancestry no
        // longer reaches the leader; it reparents to this host and can only
        // be uncertain.
        let code = format!(
            "import subprocess, time; subprocess.Popen(['sh', '-c', '(python3 {} &)'], start_new_session=True); time.sleep(600)",
            hidden_py.to_string_lossy()
        );
        let handle = launcher
            .start(spec(&[python, "-c", &code], temp.path(), vm_id))
            .await
            .unwrap();
        let hidden = pid_from(&hidden_pid).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while std::fs::read_to_string(&marker).ok().as_deref() != Some(&vm_id.to_string()) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the hidden child kept the marker");
        assert!(
            std::fs::read(format!("/proc/{hidden}/environ")).is_err(),
            "the fixture must hide its environment"
        );
        let error = launcher
            .stop(&handle, StopMode::Kill, Duration::ZERO)
            .await
            .expect_err("an unreadable marker must not become exit proof");
        assert!(
            error.to_string().contains("not yet proven")
                && error.to_string().contains(&hidden.to_string()),
            "{error}"
        );
        assert!(
            running(hidden),
            "the uncertain process is not killed on a guess"
        );
        assert!(handle.current_reap_receipt().unwrap().is_none());
        // Once the uncertain process is gone, the same stop succeeds.
        // SAFETY: the test owns this fixture process.
        unsafe {
            libc::kill(hidden, libc::SIGKILL);
        }
        assert!(settles_dead(hidden).await);
        let receipt = launcher
            .stop(&handle, StopMode::Kill, Duration::ZERO)
            .await
            .expect("exit is provable once nothing is uncertain");
        assert_eq!(receipt.exit_code(), None);
    }

    /// Review R7, the attributable case: a hidden descendant whose ancestry
    /// still reaches the leader is a member and is torn down with proof.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn a_hidden_descendant_of_the_leader_is_attributed_by_ancestry_and_stopped() {
        let Some(python) = python3() else {
            eprintln!("SKIP: python3 is required for the hidden-marker fixture");
            return;
        };
        if !environment_can_be_hidden() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let hidden_pid = temp.path().join("hidden.pid");
        let hidden_py = temp.path().join("hidden.py");
        std::fs::write(
            &hidden_py,
            format!(
                "import ctypes, os, time\nctypes.CDLL(None).prctl(4, 0, 0, 0, 0)\nopen({:?}, 'w').write(str(os.getpid()))\ntime.sleep(600)\n",
                hidden_pid.to_string_lossy()
            ),
        )
        .unwrap();
        let code = format!(
            "import subprocess, time; subprocess.Popen(['python3', {:?}], start_new_session=True); time.sleep(600)",
            hidden_py.to_string_lossy()
        );
        let handle = launcher
            .start(spec(&[python, "-c", &code], temp.path(), Uuid::new_v4()))
            .await
            .unwrap();
        let hidden = pid_from(&hidden_pid).await;
        assert!(std::fs::read(format!("/proc/{hidden}/environ")).is_err());
        let receipt = launcher
            .stop(&handle, StopMode::Kill, Duration::ZERO)
            .await
            .expect("an attributable hidden descendant is torn down with proof");
        assert_eq!(receipt.exit_code(), None);
        assert!(
            settles_dead(hidden).await,
            "hidden descendant {hidden} still running"
        );
    }

    /// Review R4: work that left the dispatcher's session (a PTY terminal's
    /// `setsid`, a detached application) and ignores SIGHUP is still torn
    /// down by a stop, found through the incarnation marker it inherited.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn stop_reaches_detached_sessions_through_the_incarnation_marker() {
        let Some(python) = python3() else {
            eprintln!("SKIP: python3 is required for the detached-session fixture");
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let child_pid = temp.path().join("detached.pid");
        let code = format!(
            "import signal, subprocess, time\n\
             p = subprocess.Popen(['sleep', '600'], start_new_session=True, preexec_fn=lambda: signal.signal(signal.SIGHUP, signal.SIG_IGN))\n\
             open({:?}, 'w').write(str(p.pid))\n\
             time.sleep(600)",
            child_pid.to_string_lossy()
        );
        let handle = launcher
            .start(spec(&[python, "-c", &code], temp.path(), Uuid::new_v4()))
            .await
            .unwrap();
        let detached = pid_from(&child_pid).await;
        // SAFETY: getsid only reads kernel process state.
        let leader_sid = unsafe { libc::getsid(handle.child_pid().unwrap() as i32) };
        let detached_sid = unsafe { libc::getsid(detached) };
        assert_ne!(
            leader_sid, detached_sid,
            "the fixture must leave the session"
        );
        let receipt = launcher
            .stop(&handle, StopMode::Kill, Duration::ZERO)
            .await
            .expect("stop returns with exit proof");
        assert_eq!(receipt.exit_code(), None);
        assert!(
            settles_dead(detached).await,
            "stop returned while the detached process {detached} was running"
        );
    }

    /// A stop in either mode tears the whole group down at once, like a VM
    /// teardown, and returns only with reap proof: a leader that would trap
    /// SIGTERM never gets the chance to run a close path.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn launcher_stops_the_group_with_reap_proof() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let child_pid_file = temp.path().join("child.pid");
        let trapped = temp.path().join("trapped");
        // Job control (`set -m`) puts the background child in a process
        // group of its own, like a preview application under the dispatcher.
        let script = format!(
            "trap 'touch {}; exit 0' TERM; set -m; sleep 600 & echo $! > '{}'; wait",
            trapped.display(),
            child_pid_file.display()
        );
        for (mode, expected) in [(StopMode::Kill, None), (StopMode::Graceful, None)] {
            let label = format!("{mode:?}");
            let _ = std::fs::remove_file(&child_pid_file);
            let handle = launcher
                .start(spec(&["sh", "-c", &script], &workspace, Uuid::new_v4()))
                .await
                .unwrap();
            let grandchild: i32 = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(pid) = std::fs::read_to_string(&child_pid_file)
                        .ok()
                        .and_then(|text| text.trim().parse().ok())
                    {
                        break pid;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let receipt = launcher
                .stop(&handle, mode, Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(receipt.exit_code(), expected, "{label}");
            // The grandchild in its own group shares the session and is gone
            // too, and its zombie is reaped by the orphan reaper.
            if tokio::time::timeout(Duration::from_secs(10), async {
                // SAFETY: signal zero only probes process existence.
                while unsafe { libc::kill(grandchild, 0) } == 0 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .is_err()
            {
                let stat = std::fs::read_to_string(format!("/proc/{grandchild}/stat"))
                    .unwrap_or_else(|error| format!("<{error}>"));
                panic!(
                    "{label}: grandchild {grandchild} still exists after the stop; stat: {stat} (this process {})",
                    std::process::id()
                );
            }
            assert!(
                !trapped.exists(),
                "{label}: no process in the group may observe a graceful signal"
            );
        }
    }

    /// VM-only boot fields never reach a host process quietly.
    #[tokio::test]
    #[serial_test::serial(process_incarnations)]
    async fn launcher_refuses_vm_only_boot_fields_and_needs_an_incarnation() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = ProcessVmLauncher::new(temp.path().join("state"));
        let mut with_scratch = spec(&["true"], temp.path(), Uuid::new_v4());
        with_scratch.scratch_disk = Some(g5d_isolation::ScratchDiskSpec { size_mib: 1 });
        let error = launcher.start(with_scratch).await.unwrap_err();
        assert!(error.to_string().contains("microVM-only"), "{error}");
        let mut with_ports = spec(&["true"], temp.path(), Uuid::new_v4());
        with_ports.port_maps = vec![(1, 2)];
        assert!(launcher.start(with_ports).await.is_err());
        let mut with_block_workload = spec(&["true"], temp.path(), Uuid::new_v4());
        with_block_workload.block_workload = true;
        let error = launcher.start(with_block_workload).await.unwrap_err();
        assert!(error.to_string().contains("microVM-only"), "{error}");
        let mut without_vm = spec(&["true"], temp.path(), Uuid::new_v4());
        without_vm.env.clear();
        let error = launcher.start(without_vm).await.unwrap_err();
        assert!(error.to_string().contains("G5D_VM_ID"), "{error}");
        assert!(
            !temp.path().join("state").exists(),
            "a refused start creates nothing"
        );
    }
}
