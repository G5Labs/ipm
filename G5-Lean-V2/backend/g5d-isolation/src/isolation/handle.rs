//! Task-scoped isolation handle — bundles the workspace + cache context that
//! tools need when running inside a task's isolated environment.
//!
//! # What this is (post this cleanup PR)
//!
//! In the legacy design, `TaskIsolationHandle::start` booted a long-running
//! microVM with an idle shell, held the resulting `RunHandle` for the
//! task's lifetime, and routed every `run_in_terminal` call into that VM
//! via a per-call `manager.run()`. That model was a semantic fiction: the
//! "held VM" was never actually re-entered — every command spun up a new
//! per-call VM under the hood — and the held-handle path silently
//! degraded to host execution whenever VM start failed.
//!
//! The held-VM lifecycle has been removed. `TaskIsolationHandle` now
//! carries workspace + cache metadata (guest mount points, cache env vars,
//! cache host directory) plus a shared reference to the active
//! `IsolationManager`, so callers can ask "what guest path corresponds to
//! this host path?" and (for the merge window before the dedicated host
//! supervisor lands) route per-command VM execution through the manager
//! without re-introducing a long-lived held-VM.
//!
//! # Per-call execution shim (`exec_command_in_vm`)
//!
//! The legacy held-VM path was a silent-host-degrade hazard: if
//! `manager.start()` failed, terminal commands quietly fell back to the
//! host PTY even though the worker was configured for VM isolation. The
//! shim added in this cleanup PR is intentionally explicit:
//!
//! - It runs ONE `manager.run()` per terminal command (no long-lived VM).
//! - It is only called by `RunInTerminalTool` when the active
//!   `IsolationLevel` is `Vm` and a `TaskIsolationHandle` is present.
//! - If `manager.run()` returns an error, the caller MUST propagate it to
//!   the agent — there is no fallback to host execution. The
//!   `IsolationManager` itself already fails closed at startup if the
//!   configured `IsolationMode` cannot be honored, so reaching this code
//!   path under VM mode means the backend was healthy at boot.
//!
//! Per-command VM lifecycle ownership moves to the dedicated host
//! supervisor in a subsequent host-supervisor PR (`g5d host`); this
//! shim closes the host-execution gap during the merge window without
//! resurrecting the held-VM fiction.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use g5_contracts::error::G5Error;
use g5d_foundation::{TaskCommandExecutor, TaskCommandRequest, TaskCommandResult};

use super::{IsolationLevel, IsolationManager, MicrovmMount, MicrovmRunSpec, RunResult};

/// A task-scoped handle that carries workspace + cache context for tools
/// running on behalf of a task. Held alive for the agent execute phase and
/// dropped in CLEANUP.
pub struct TaskIsolationHandle {
    manager: Arc<IsolationManager>,
    workspace_path: PathBuf,
    guest_workspace: String,
    cache_host_dir: Option<PathBuf>,
    cache_guest_dir: String,
    cache_env_vars: Vec<(String, String)>,
}

#[async_trait]
impl TaskCommandExecutor for TaskIsolationHandle {
    fn isolation_level(&self) -> IsolationLevel {
        self.isolation_level()
    }

    fn host_workspace(&self) -> &std::path::Path {
        self.workspace_path()
    }

    fn guest_workspace(&self) -> &str {
        self.guest_workspace()
    }

    async fn execute_command(
        &self,
        request: TaskCommandRequest,
    ) -> Result<TaskCommandResult, G5Error> {
        let result = self
            .exec_command_in_vm(
                &request.command,
                request.extra_env,
                request.cpus,
                request.mem_mib,
            )
            .await?;
        Ok(TaskCommandResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
        })
    }
}

impl TaskIsolationHandle {
    /// Build a task isolation handle.
    ///
    /// No VM is booted here — the supervisor (a subsequent host-supervisor PR) owns VM lifecycle.
    /// This constructor only records the workspace + cache parameters the
    /// rest of the worker needs to address files by host *and* guest path.
    ///
    /// # Arguments
    /// - `cache_host_dir`: Host path to the L1 cache mount root, if any.
    /// - `cache_env_vars`: Per-ecosystem env vars (e.g. `CARGO_HOME=/cache/cargo`).
    pub async fn start(
        manager: Arc<IsolationManager>,
        workspace_path: PathBuf,
        cache_host_dir: Option<PathBuf>,
        cache_env_vars: Vec<(String, String)>,
    ) -> Result<Self, G5Error> {
        let guest_workspace = "/work".to_string();
        let cache_guest_dir = "/cache".to_string();

        tracing::info!(
            backend = manager.backend().name(),
            isolation_level = %manager.isolation_level(),
            "Task isolation context ready (no held VM — see TaskIsolationHandle docs)"
        );

        Ok(TaskIsolationHandle {
            manager,
            workspace_path,
            guest_workspace,
            cache_host_dir,
            cache_guest_dir,
            cache_env_vars,
        })
    }

    /// Isolation level reported by the active backend.
    pub fn isolation_level(&self) -> IsolationLevel {
        self.manager.isolation_level()
    }

    /// Guest-side workspace mount point (always `/work`).
    pub fn guest_workspace(&self) -> &str {
        &self.guest_workspace
    }

    /// Host-side workspace path.
    pub fn workspace_path(&self) -> &PathBuf {
        &self.workspace_path
    }

    /// Active backend's base OCI image reference (used to build a
    /// per-call `MicrovmRunSpec`).
    pub fn base_image(&self) -> &str {
        self.manager.base_image()
    }

    /// Run a shell command in a per-call microVM via the active
    /// `IsolationManager`. Intended for the explicit shim that
    /// `RunInTerminalTool` uses when the configured isolation level is
    /// `Vm`; see the module docs for the fail-closed contract.
    ///
    /// The spec is constructed from the task's workspace + cache
    /// context: the workspace is mounted at `/work`, the L1 cache (if
    /// any) is mounted at `/cache`, and the cache env vars are merged
    /// in front of any caller-supplied env. The command is wrapped in
    /// `/bin/sh -c "<command>"` so callers can pass arbitrary shell
    /// pipelines.
    pub async fn exec_command_in_vm(
        &self,
        command: &str,
        extra_env: Vec<(String, String)>,
        cpus: u8,
        mem_mib: u32,
    ) -> Result<RunResult, G5Error> {
        let mut env = self.cache_env_vars.clone();
        for (k, v) in extra_env {
            env.push((k, v));
        }
        let mut extra_mounts = Vec::new();
        if let Some(cache_dir) = &self.cache_host_dir {
            extra_mounts.push(MicrovmMount::read_write(
                cache_dir.clone(),
                self.cache_guest_dir.clone(),
            ));
        }
        let spec = MicrovmRunSpec {
            image: self.manager.base_image().to_string(),
            cmd: vec!["/bin/sh".to_string(), "-c".to_string(), command.to_string()],
            env,
            mount_host_dir: self.workspace_path.clone(),
            mount_guest_dir: self.guest_workspace.clone(),
            extra_mounts,
            cpus,
            mem_mib,
            // No port forwarding on the per-call shell path — the host-port
            // allocator (ontology D33) is reserved for worker-launched task
            // microVMs that bind a guest socket. See `CloudHypervisorBackend::port_allocator`.
            port_maps: vec![],
            // No host-loopback service to reach on the per-call shell path
            // (microvm_runtime D46).
            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };
        self.manager
            .run(&spec)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("microVM per-call execution failed: {e}"),
            })
    }

    /// No-op shutdown kept for call-site compatibility. The handle holds no
    /// long-lived VM, so there is nothing to tear down here.
    pub async fn shutdown(&self) {
        // intentionally empty — see module docs
    }
}

/// Build a `TaskIsolationHandle` for tests that need to assert routing
/// decisions in dependents (notably `RunInTerminalTool::execute`)
/// without actually booting a backend. Available only with the
/// `test-helpers` cargo feature so it never compiles into release.
#[cfg(any(test, feature = "test-helpers"))]
pub fn test_handle_for_routing(
    manager: Arc<IsolationManager>,
    workspace_path: PathBuf,
) -> TaskIsolationHandle {
    TaskIsolationHandle {
        manager,
        workspace_path,
        guest_workspace: "/work".to_string(),
        cache_host_dir: None,
        cache_guest_dir: "/cache".to_string(),
        cache_env_vars: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_level_handle_carries_workspace_context() {
        let config = super::super::IsolationConfig {
            mode: super::super::IsolationMode::Process,
            ..Default::default()
        };
        let manager = IsolationManager::new(config).await.unwrap();
        let manager = Arc::new(manager);

        let handle =
            TaskIsolationHandle::start(manager, PathBuf::from("/tmp/test-workspace"), None, vec![])
                .await
                .unwrap();

        assert_eq!(handle.isolation_level(), IsolationLevel::Process);
        assert_eq!(handle.guest_workspace(), "/work");
        assert_eq!(
            handle.workspace_path(),
            &PathBuf::from("/tmp/test-workspace")
        );

        // shutdown is a no-op now; calling it must not panic.
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_handle_workspace_path_accessor() {
        let config = super::super::IsolationConfig {
            mode: super::super::IsolationMode::Process,
            ..Default::default()
        };
        let manager = IsolationManager::new(config).await.unwrap();
        let manager = Arc::new(manager);

        let handle =
            TaskIsolationHandle::start(manager, PathBuf::from("/home/user/project"), None, vec![])
                .await
                .unwrap();

        assert_eq!(handle.guest_workspace(), "/work");
        assert_eq!(
            handle.workspace_path(),
            &PathBuf::from("/home/user/project")
        );
    }
}
