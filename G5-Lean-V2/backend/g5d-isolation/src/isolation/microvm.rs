//! Cloud Hypervisor backend, preserving host custody, console and port ownership.
use anyhow::Context;
use std::path::PathBuf;

#[cfg(feature = "cloud-hypervisor")]
use super::{child_reap_channel, BackendType};
use super::{
    ChildReapReceipt, IsolationLevel, MicrovmBackend, MicrovmRunSpec, RunHandle, RunHandleInner,
    RunResult, FORCED_CHILD_REAP_TIMEOUT, GRACEFUL_CHILD_REAP_TIMEOUT,
};
use crate::isolation::port_map::PortAllocator;
#[cfg(feature = "cloud-hypervisor")]
use g5d_foundation::collect_secret_env_values;
use g5d_foundation::redact_secrets;

// ---------------------------------------------------------------------------
// CloudHypervisorBackend (Linux/KVM production path)
// ---------------------------------------------------------------------------

/// Shared by host image preparation and per-VM materialization so both use
/// the same templates and filesystem (required for reflink clones).
pub fn rootfs_state_dir() -> PathBuf {
    std::env::var_os("G5D_ROOTFS_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/g5d/rootfs"))
}

/// Check every published slot even when this host has no configured image to prewarm.
/// This is a free-space admission check, not a reservation against other writers.
pub fn check_workload_capacity(
    root: &std::path::Path,
    size_mib: u32,
    slots: u32,
) -> anyhow::Result<()> {
    #[cfg(not(feature = "cloud-hypervisor"))]
    {
        let _ = (root, size_mib, slots);
        anyhow::bail!("block workload capacity requires Cloud Hypervisor support");
    }
    #[cfg(feature = "cloud-hypervisor")]
    {
        anyhow::ensure!(
            size_mib > 0 && slots > 0,
            "block slot capacity must be positive"
        );
        let bytes = u64::from(size_mib)
            .checked_mul(u64::from(slots))
            .and_then(|mib| mib.checked_mul(1024 * 1024))
            .ok_or_else(|| anyhow::anyhow!("block slot capacity overflow"))?;
        std::fs::create_dir_all(root)?;
        super::workload_root::check_capacity(root, bytes)?;
        Ok(())
    }
}

/// Prepare the configured pristine block template before host activation.
/// Run outside the daemon so timeout/cancellation kills the builder and its helpers.
pub async fn prewarm_workload_template(
    template: &std::path::Path,
    size_mib: u32,
    slots: u32,
) -> anyhow::Result<()> {
    prepare_workload_templates(template, size_mib, slots, None).await
}

/// Artifact download/validation remains in the kill-on-drop preparation helper,
/// outside the supervisor. A missing companion uses the same local formatter.
pub async fn prepare_workload_image(image: &str, size_mib: u32, slots: u32) -> anyhow::Result<()> {
    let mut builder = std::process::Command::new(std::env::current_exe()?);
    builder.stdin(std::process::Stdio::null());
    #[cfg(feature = "cloud-hypervisor")]
    super::workload_root::kill_with_parent(&mut builder);
    let mut command = tokio::process::Command::from(builder);
    command
        .args([
            "oci",
            "prepare-workload",
            image,
            "--prefer-artifact",
            "--size-mib",
        ])
        .arg(size_mib.to_string())
        .arg("--slots")
        .arg(slots.to_string())
        .kill_on_drop(true);
    let status = tokio::time::timeout(std::time::Duration::from_secs(1800), command.status())
        .await
        .context("workload image preparation timed out")??;
    anyhow::ensure!(
        status.success(),
        "workload image preparation failed: {status}"
    );
    Ok(())
}

/// Delivery may add a pinned repository baseline; runtime prewarm never learns
/// repository state from a VM or from workspace-controlled environment.
pub async fn prepare_workload_templates(
    template: &std::path::Path,
    size_mib: u32,
    slots: u32,
    repo_seed_manifest: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    prepare_workload_templates_in_scope(template, size_mib, slots, repo_seed_manifest, None).await
}

pub async fn prepare_workload_templates_in_scope(
    template: &std::path::Path,
    size_mib: u32,
    slots: u32,
    repo_seed_manifest: Option<&std::path::Path>,
    process_scope: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    #[cfg(not(feature = "cloud-hypervisor"))]
    {
        let _ = (template, size_mib, slots, repo_seed_manifest, process_scope);
        anyhow::bail!("block template preparation requires Cloud Hypervisor support");
    }
    #[cfg(feature = "cloud-hypervisor")]
    {
        let reserve_mib = u64::from(size_mib) * u64::from(slots);
        // The helper inherits parent-death killing, as do its formatter/copy
        // children. A daemon crash cannot leave an unbounded orphan writer.
        let mut builder = std::process::Command::new(std::env::current_exe()?);
        builder.stdin(std::process::Stdio::null());
        super::workload_root::kill_with_parent(&mut builder);
        let mut command = tokio::process::Command::from(builder);
        #[cfg(target_os = "linux")]
        if let Some(group) = process_scope {
            use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
            let file = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(group.join("cgroup.procs"))?;
            unsafe {
                command.pre_exec(move || {
                    if libc::write(file.as_raw_fd(), b"0".as_ptr().cast(), 1) != 1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        command
            .arg("vm-exec")
            .arg("--prepare-block-template")
            .arg("--rootfs")
            .arg(template.join("rootfs"))
            .arg("--scratch-disk-mib")
            .arg(size_mib.to_string())
            .arg("--block-template-reserve-mib")
            .arg(reserve_mib.to_string())
            .kill_on_drop(true);
        if let Some(manifest) = repo_seed_manifest {
            command.arg("--block-template-repo-seed").arg(manifest);
            if process_scope.is_some() {
                command.arg("--block-template-seed-stage-only");
            }
        }
        let status = tokio::time::timeout(std::time::Duration::from_secs(600), command.status())
            .await
            .map_err(|_| anyhow::anyhow!("block template preparation timed out after 600s"))??;
        anyhow::ensure!(
            status.success(),
            "block template preparation failed: {status}"
        );
        Ok(())
    }
}

/// The seed metadata is a hint; the later clone repeats eligibility under its
/// publication lock after the caller independently refreshes read authority.
pub async fn eligible_repository_template(
    image: &str,
    size: u32,
    scope: &super::WorkloadRepoScope,
) -> anyhow::Result<Option<PathBuf>> {
    #[cfg(feature = "cloud-hypervisor")]
    {
        let template = g5d_foundation::oci::OciExtractor::from_env()
            .prepare_rootfs_template(image, &rootfs_state_dir().join("templates"))
            .await?;
        if super::workload_seed::eligible(&template, size, scope)? {
            return Ok(Some(template));
        }
    }
    #[cfg(not(feature = "cloud-hypervisor"))]
    let _ = (image, size, scope);
    Ok(None)
}

pub async fn pin_repository_template(
    template: PathBuf,
    root: PathBuf,
    size: u32,
    scope: super::WorkloadRepoScope,
    stores: std::collections::BTreeMap<uuid::Uuid, PathBuf>,
) -> anyhow::Result<bool> {
    #[cfg(feature = "cloud-hypervisor")]
    return tokio::task::spawn_blocking(move || {
        let Some(guard) = super::workload_seed::try_clone(&template, &root, size, scope, &stores)?
        else {
            return Ok(false);
        };
        std::fs::rename(&guard.0, root.join(".g5/repo-seed.ext4"))?;
        Ok(true)
    })
    .await?;
    #[cfg(not(feature = "cloud-hypervisor"))]
    {
        let _ = (template, root, size, scope, stores);
        Ok(false)
    }
}

pub async fn repository_template_is_current(
    template: PathBuf,
    size: u32,
    scope: super::WorkloadRepoScope,
    commits: Vec<(uuid::Uuid, String)>,
) -> anyhow::Result<bool> {
    #[cfg(feature = "cloud-hypervisor")]
    return tokio::task::spawn_blocking(move || {
        super::workload_seed::current_generation(&template, size, scope, commits)
            .map_err(Into::into)
    })
    .await?;
    #[cfg(not(feature = "cloud-hypervisor"))]
    {
        let _ = (template, size, scope, commits);
        Ok(false)
    }
}

pub async fn publish_repository_template(
    template: PathBuf,
    size: u32,
    scope: super::WorkloadRepoScope,
    commits: Vec<(uuid::Uuid, String)>,
) -> anyhow::Result<()> {
    #[cfg(feature = "cloud-hypervisor")]
    return tokio::task::spawn_blocking(move || {
        super::workload_seed::publish_prepared(&template, size, scope, commits).map_err(Into::into)
    })
    .await?;
    #[cfg(not(feature = "cloud-hypervisor"))]
    {
        let _ = (template, size, scope, commits);
        anyhow::bail!("repository templates require Cloud Hypervisor")
    }
}

/// Process resource owner for optional source preparation. Host cache locks stay
/// in the daemon and outlive cancellation until the shared Git runner is idle.
pub struct SourcePreparationScope {
    #[cfg(feature = "cloud-hypervisor")]
    scope: super::vm_scope::VmScope,
}
impl SourcePreparationScope {
    pub fn new(data: &std::path::Path) -> anyhow::Result<Self> {
        #[cfg(feature = "cloud-hypervisor")]
        {
            let mut unused = tokio::process::Command::new("/bin/true");
            let scope = super::vm_scope::VmScope::create(&mut unused)?;
            scope.background_limits(data)?;
            Ok(Self { scope })
        }
        #[cfg(not(feature = "cloud-hypervisor"))]
        {
            let _ = data;
            anyhow::bail!("source preparation requires Cloud Hypervisor support")
        }
    }
    pub fn path(&self) -> &std::path::Path {
        #[cfg(feature = "cloud-hypervisor")]
        return &self.scope.path;
        #[cfg(not(feature = "cloud-hypervisor"))]
        unreachable!("no source scope without Cloud Hypervisor")
    }
    pub async fn finish(&self) -> anyhow::Result<()> {
        #[cfg(feature = "cloud-hypervisor")]
        return self.scope.finish().await;
        #[cfg(not(feature = "cloud-hypervisor"))]
        unreachable!("no source scope without Cloud Hypervisor")
    }
}

/// True if `s` looks like a filesystem path (so the caller is responsible
/// for pre-extracting the rootfs) rather than an OCI image reference. We
/// only treat absolute paths and relative paths with explicit `./`, `../`,
/// or `~/` prefixes as paths; everything else is parsed as an image ref.
#[cfg_attr(not(feature = "cloud-hypervisor"), allow(dead_code))]
fn looks_like_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with("./") || s.starts_with("../") || s.starts_with("~/")
}

#[cfg(feature = "cloud-hypervisor")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedRootfs {
    path: PathBuf,
    image_env: Vec<(String, String)>,
    /// `Some(dir)` when `resolve_rootfs` materialized this rootfs itself
    /// (per-VM template clone, ontology D40) and therefore owns its removal
    /// after the VM exits. `None` for caller-provided pre-extracted
    /// directories (smoke fixture, operator rootfs), which are never removed.
    owned_dir: Option<PathBuf>,
    template_dir: Option<PathBuf>,
}

/// Supervises a Cloud Hypervisor VM and passt sidecar for each workspace.
/// Without the `cloud-hypervisor` feature, starts fail explicitly.
pub struct CloudHypervisorBackend {
    /// Trusted host configuration; never derived from a workspace or image env.
    host_api_url: Option<String>,
    /// Cached hypervisor-availability probe result.
    available: bool,
    /// Process-local host-port allocator over the IANA ephemeral range
    /// (`49152..=65535`). Callers that want guest port forwarding ask the
    /// backend's allocator for a [`crate::isolation::port_map::PortLease`],
    /// then populate [`MicrovmRunSpec::port_maps`] with the `(host, guest)`
    /// pair before calling `start`/`run`. The lease must outlive the VM (the
    /// caller holds it alongside the resulting [`RunHandle`] so the host port
    /// is reclaimed exactly when the VM is torn down). See ontology D33.
    port_allocator: PortAllocator,
}

impl Default for CloudHypervisorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudHypervisorBackend {
    pub fn with_host_api_url(mut self, url: Option<String>) -> Self {
        self.host_api_url = url;
        self
    }

    #[cfg(feature = "cloud-hypervisor")]
    fn apply_host_network_env(&self, cmd: &mut tokio::process::Command) {
        if let Some(url) = &self.host_api_url {
            cmd.env(super::guest_egress::HOST_API_URL_ENV, url);
        }
    }
    /// Create a new `CloudHypervisorBackend`. Probes for hypervisor availability
    /// (Linux/KVM plus the pinned platform artifacts) during construction.
    #[must_use]
    pub fn new() -> Self {
        if !cfg!(feature = "cloud-hypervisor") {
            return CloudHypervisorBackend {
                host_api_url: None,
                available: false,
                port_allocator: PortAllocator::new(),
            };
        }
        #[cfg(feature = "cloud-hypervisor")]
        let hypervisor_ok = super::cloud_hypervisor::available();
        #[cfg(not(feature = "cloud-hypervisor"))]
        let hypervisor_ok = false;
        CloudHypervisorBackend {
            host_api_url: None,
            available: hypervisor_ok,
            port_allocator: PortAllocator::new(),
        }
    }

    /// Test-only constructor that forces a backend to report unavailable.
    /// Available crate-wide so cross-module routing tests (notably in
    /// `crate::tools::terminal`) can assert backend-error propagation
    /// without booting a real hypervisor.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn forced_unavailable() -> Self {
        CloudHypervisorBackend {
            host_api_url: None,
            available: false,
            port_allocator: PortAllocator::new(),
        }
    }

    /// Access the backend's host-port allocator.
    ///
    /// Workers that need guest port forwarding obtain a
    /// [`crate::isolation::port_map::PortLease`] via this allocator before
    /// constructing a [`MicrovmRunSpec`], then set `spec.port_maps` to the
    /// allocated `(host, guest)` pairs.
    ///
    /// **Lease-lifetime caller invariant** (D32): `MicrovmRunSpec.port_maps`
    /// carries raw `u16` pairs, so the caller must hold the matching
    /// `PortLease` alongside the resulting [`RunHandle`] for the entire VM
    /// lifetime — releasing the lease early would let `allocate()` hand the
    /// same host port to a different VM. The workspace runtime satisfies this
    /// by keeping the lease and VM handle in the same incarnation record.
    #[must_use]
    pub fn port_allocator(&self) -> &PortAllocator {
        &self.port_allocator
    }

    /// Generate a unique VM name using UUID v7.
    #[cfg_attr(not(feature = "cloud-hypervisor"), allow(dead_code))]
    fn generate_vm_name() -> String {
        format!("g5d-{}", uuid::Uuid::now_v7())
    }

    /// Resolve a `MicrovmRunSpec.image` to an on-disk rootfs directory.
    ///
    /// Both local OCI layouts and registry references use the verified template
    /// extractor. Keep the pristine template shared and create only the small
    /// owned bootstrap directory here; vm-exec reflinks its prepared block image.
    /// Bare rootfs directories cannot supply the required template identity.
    ///
    /// Path-shaped strings that point to non-existent directories are
    /// rejected with a helpful error rather than silently sent to the
    /// registry as image references.
    ///
    /// The legacy `skopeo` + `umoci` shell-out is not re-introduced; see
    /// ontology `g5/g5d/oci` for the in-process pipeline contract.
    #[cfg(feature = "cloud-hypervisor")]
    async fn resolve_rootfs(image: &str, _block_workload: bool) -> anyhow::Result<ResolvedRootfs> {
        let state_root = rootfs_state_dir();
        let extractor = g5d_foundation::oci::OciExtractor::from_env();
        let template = if looks_like_path(image) {
            let path = PathBuf::from(image);
            anyhow::ensure!(
                path.is_dir(),
                "rootfs image path is not a directory: {image}"
            );
            anyhow::ensure!(
                path.join("oci-layout").is_file(),
                "block boot requires an OCI image layout, not a bare rootfs"
            );
            extractor
                .prepare_rootfs_template_from_layout(&path, &state_root.join("templates"))
                .await?
        } else {
            extractor
                .prepare_rootfs_template(image, &state_root.join("templates"))
                .await?
        };
        let config: g5d_foundation::oci::ImageConfig =
            serde_json::from_slice(&tokio::fs::read(template.join("image-config.json")).await?)?;
        let image_env = parse_image_env(&config.env)?;
        // Only small owned bootstrap state is created per VM; the OCI directory
        // and immutable block template are reused without another whole-tree clone.
        let owned = state_root.join(uuid::Uuid::now_v7().to_string());
        let path = owned.join("rootfs");
        tokio::fs::create_dir_all(path.join(".g5")).await?;
        Ok(ResolvedRootfs {
            path,
            image_env,
            owned_dir: Some(owned),
            template_dir: Some(template),
        })
    }

    #[cfg(feature = "cloud-hypervisor")]
    fn vm_exec_binary() -> PathBuf {
        let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("g5d"));
        // Test seam: inside an integration-test harness `current_exe()` is the
        // libtest binary, which has no `vm-exec` entrypoint — real-boot E2E
        // tests point this at the actual `g5d` bin (`CARGO_BIN_EXE_g5d`).
        // Compiled out of production builds.
        #[cfg(any(test, feature = "test-helpers"))]
        let self_exe = std::env::var("G5D_TEST_VM_EXEC_BIN")
            .map(PathBuf::from)
            .unwrap_or(self_exe);
        self_exe
    }

    /// Pin before publishing the VM handle, so cold formatting does not consume
    /// the dispatcher's shorter readiness window. Preparation has its own bounded
    /// helper and the same exit-proof custody as a VM startup failure.
    #[cfg(feature = "cloud-hypervisor")]
    async fn prepare_private_workload(rootfs: &ResolvedRootfs, size: u32) -> anyhow::Result<()> {
        let template = rootfs
            .template_dir
            .clone()
            .ok_or_else(|| anyhow::anyhow!("missing workload template"))?;
        let root = rootfs.path.clone();
        let warm = tokio::task::spawn_blocking(move || {
            let Some(guard) = super::workload_template::try_clone(&template, &root, size)? else {
                return Ok::<_, g5_contracts::G5Error>(false);
            };
            super::workload_template::publish_private_pin(guard, &root)?;
            Ok(true)
        })
        .await??;
        if warm {
            return Ok(());
        }
        let mut builder = std::process::Command::new(Self::vm_exec_binary());
        builder
            .args(["vm-exec", "--prepare-private-workload", "--rootfs"])
            .arg(&rootfs.path)
            .arg("--scratch-disk-mib")
            .arg(size.to_string())
            .env(super::workload_root::OWNED_ROOT_ENV, &rootfs.path)
            .env(
                super::workload_template::TEMPLATE_ENV,
                rootfs.template_dir.as_ref().unwrap(),
            )
            .stdin(std::process::Stdio::null());
        super::workload_root::kill_with_parent(&mut builder);
        let mut command = tokio::process::Command::from(builder);
        command
            .kill_on_drop(true)
            .stderr(std::process::Stdio::piped());
        let scope = super::vm_scope::VmScope::create(&mut command)?;
        let mut child = command.spawn()?;
        let mut stderr = child.stderr.take().expect("piped preparation stderr");
        let diagnostics = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut tail = Vec::new();
            let mut chunk = [0_u8; 4096];
            while let Ok(n) = stderr.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                tail.extend_from_slice(&chunk[..n]);
                if tail.len() > 16384 {
                    tail.drain(..tail.len() - 16384);
                }
            }
            String::from_utf8_lossy(&tail).into_owned()
        });
        let result = tokio::time::timeout(std::time::Duration::from_secs(600), child.wait()).await;
        match result {
            Ok(Ok(status)) => {
                scope.wait_for_exit().await;
                let tail = diagnostics.await.unwrap_or_default();
                if !status.success() {
                    if let Some(pressure) = tail
                        .lines()
                        .find_map(g5d_foundation::disk::DiskPressure::parse)
                    {
                        return Err(pressure.into());
                    }
                    anyhow::bail!(
                        "cold private workload preparation failed: {status}: {}",
                        tail.trim()
                    );
                }
                Ok(())
            }
            other => {
                terminate_failed_vm_start(&mut child, &scope).await;
                anyhow::bail!(
                    "cold private workload preparation failed or exceeded 600s: {other:?}"
                )
            }
        }
    }

    /// Remove a per-VM rootfs clone after its VM exited (ontology D40).
    /// Best-effort: a failed removal is left for host rotation to bound —
    /// it never fails the task.
    #[cfg(feature = "cloud-hypervisor")]
    async fn remove_owned_rootfs(owned_dir: Option<PathBuf>, vm_name: &str) {
        let Some(dir) = owned_dir else { return };
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => tracing::info!(
                vm_name = %vm_name,
                rootfs = %dir.display(),
                "Per-VM rootfs removed after VM exit"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(
                vm_name = %vm_name,
                rootfs = %dir.display(),
                error = %e,
                "Per-VM rootfs GC failed; leaving dir for host rotation"
            ),
        }
    }

    /// Detached variant of [`Self::remove_owned_rootfs`] for happy paths,
    /// where unlinking a multi-GB tree must not delay result delivery.
    /// Error paths await the removal inline instead — a detached task could
    /// be lost if the caller shuts the runtime down right after the error
    /// (CodeRabbit review of PR #778).
    #[cfg(feature = "cloud-hypervisor")]
    fn spawn_owned_rootfs_cleanup(owned_dir: Option<PathBuf>, vm_name: String) {
        if owned_dir.is_none() {
            return;
        }
        tokio::spawn(async move { Self::remove_owned_rootfs(owned_dir, &vm_name).await });
    }

    /// Build a `g5d vm-exec` command from a `MicrovmRunSpec`.
    ///
    /// Spawns the current executable (`std::env::current_exe()`) so the
    /// child process supervises Cloud Hypervisor and passt. The rootfs must be a
    /// resolved directory (see [`resolve_rootfs`]).
    #[cfg(feature = "cloud-hypervisor")]
    fn build_vm_exec_command(
        spec: &MicrovmRunSpec,
        rootfs: &ResolvedRootfs,
    ) -> tokio::process::Command {
        let guest_env = resolved_guest_env(rootfs, spec);
        Self::build_vm_exec_command_with_env(spec, rootfs, &guest_env)
    }

    #[cfg(feature = "cloud-hypervisor")]
    fn build_vm_exec_command_with_env(
        spec: &MicrovmRunSpec,
        rootfs: &ResolvedRootfs,
        guest_env: &[(String, String)],
    ) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(Self::vm_exec_binary());
        close_vm_exec_stdin(&mut cmd);

        cmd.arg("vm-exec")
            .arg("--rootfs")
            .arg(&rootfs.path)
            .arg("--cpus")
            .arg(spec.cpus.to_string())
            .arg("--mem-mib")
            .arg(spec.mem_mib.to_string());

        // This internal handoff is set only for a clone owned by this launch.
        // Never inherit a caller's stale ownership binding in legacy mode.
        cmd.env_remove(super::workload_root::OWNED_ROOT_ENV);
        cmd.env_remove(super::workload_template::TEMPLATE_ENV);
        cmd.env_remove(super::cloud_hypervisor::RESTORE_INPUT);
        if spec.nested_virtualization {
            cmd.arg("--nested-virtualization");
        }
        if true {
            cmd.arg("--block-workload");
            if rootfs
                .owned_dir
                .as_deref()
                .is_some_and(|owned| rootfs.path.parent() == Some(owned))
            {
                cmd.env(super::workload_root::OWNED_ROOT_ENV, &rootfs.path);
                if let Some(template) = &rootfs.template_dir {
                    cmd.env(super::workload_template::TEMPLATE_ENV, template);
                    if let Some(restore) = &spec.guest_restore {
                        cmd.env(
                            super::cloud_hypervisor::RESTORE_INPUT,
                            serde_json::to_string(restore).expect("admitted restore serializes"),
                        );
                    }
                }
            }
        }

        // Primary workspace mount.
        cmd.arg("-V").arg(format!(
            "{}:{}",
            spec.mount_host_dir.display(),
            spec.mount_guest_dir
        ));

        // The runtime mount becomes an independent read-only block device.
        for mount in &spec.extra_mounts {
            cmd.arg("-V").arg(mount.vm_exec_volume_arg());
        }

        // All guest environment values travel privately, never in process argv.
        let environment: std::collections::BTreeMap<_, _> = guest_env.iter().cloned().collect();
        cmd.env(
            super::cloud_hypervisor::GUEST_ENV,
            serde_json::to_string(&environment).expect("guest environment is serializable"),
        );

        // vm-exec owns passt ingress publication for each admitted port mapping.
        for (host_port, guest_port) in &spec.port_maps {
            cmd.arg("-p").arg(format!("{}:{}", host_port, guest_port));
        }

        // The published scratch budget now bounds the complete private workload.
        if let Some(scratch) = &spec.scratch_disk {
            cmd.arg("--scratch-disk-mib")
                .arg(scratch.size_mib.to_string());
        }

        // Guest `/etc/hosts` aliases for CI control-plane routing: forwarded
        // from the supervisor's `G5D_GUEST_HOST_ALIASES` env to `vm-exec` as
        // explicit argv. Legacy loopback aliases are mapped by vm-exec to the
        // allowlisted passt host door. Empty/unset → no flags,
        // so production boots (publicly-resolvable control plane) are unchanged.
        for alias in parse_guest_host_aliases_env(
            std::env::var("G5D_GUEST_HOST_ALIASES")
                .as_deref()
                .unwrap_or(""),
        ) {
            cmd.arg("--guest-host-alias").arg(alias);
        }

        if !spec.mount_guest_dir.is_empty() {
            cmd.arg("--workdir").arg(&spec.mount_guest_dir);
        }

        cmd.arg("--");
        for arg in &spec.cmd {
            cmd.arg(arg);
        }

        // `kill()` escalates to the whole vm-exec process group. Make the
        // spawned child the group leader before it can exec the VM supervisor. On
        // Linux, also make the kernel kill the VMM if its g5d host parent
        // exits without running the async shutdown path. The parent check
        // closes the race where g5d dies between spawn and PR_SET_PDEATHSIG.
        #[cfg(target_os = "linux")]
        let expected_parent = unsafe { libc::getpid() };
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() != expected_parent {
                        libc::raise(libc::SIGKILL);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "g5d host exited while starting vm-exec",
                        ));
                    }
                }
                Ok(())
            });
        }

        cmd
    }
}

/// A VMM is never interactive. Its own process group is separate from the
/// host's, so inheriting a foreground host terminal can stop the VMM during
/// a terminal ioctl before the guest boots. The supervisor supplies the VMM
/// its own held-open serial pipe; interactive sessions use dispatcher PTYs.
#[cfg(feature = "cloud-hypervisor")]
fn close_vm_exec_stdin(cmd: &mut tokio::process::Command) {
    cmd.stdin(std::process::Stdio::null());
}

/// Parse `G5D_GUEST_HOST_ALIASES` (comma-separated `HOST=IP` entries) into the
/// per-VM `--guest-host-alias` flag values. Whitespace around each entry is
/// trimmed; blank entries and entries without an `=` are dropped (the `vm-exec`
/// side re-validates each `HOST=IP`). An empty/unset env yields no aliases — the
/// production default, where the control plane is publicly DNS-resolvable.
#[cfg(any(feature = "cloud-hypervisor", test))]
fn parse_guest_host_aliases_env(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.contains('='))
        .map(str::to_string)
        .collect()
}

#[cfg(feature = "cloud-hypervisor")]
fn parse_image_env(entries: &[String]) -> anyhow::Result<Vec<(String, String)>> {
    entries
        .iter()
        .map(|entry| {
            let (key, value) = entry.split_once('=').ok_or_else(|| {
                anyhow::anyhow!("OCI image config Env entry is missing '=': {entry:?}")
            })?;
            if key.is_empty() {
                anyhow::bail!("OCI image config Env entry has an empty key: {entry:?}");
            }
            if key.contains('\0') || value.contains('\0') {
                anyhow::bail!("OCI image config Env entry contains a NUL byte: {entry:?}");
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(feature = "cloud-hypervisor")]
fn merge_image_and_spec_env(
    image_env: &[(String, String)],
    spec_env: &[(String, String)],
) -> Vec<(String, String)> {
    use std::collections::HashSet;

    let spec_keys: HashSet<&str> = spec_env.iter().map(|(key, _)| key.as_str()).collect();
    let mut seen_image_keys = HashSet::new();
    let mut merged_image = Vec::new();

    // The key-list carrier is host-generated only: an image that declares
    // it itself could otherwise name a spec-only variable for forwarding
    // (codex review P2), so its entry is dropped before anything is named.
    let carrier = g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_ENV;
    for (key, value) in image_env.iter().rev() {
        if key == carrier
            || spec_keys.contains(key.as_str())
            || !seen_image_keys.insert(key.as_str())
        {
            continue;
        }
        merged_image.push((key.clone(), value.clone()));
    }
    merged_image.reverse();

    // Name the image keys that survived the merge (ontology
    // `g5.runtime.image-env`): the dispatcher forwards exactly these to agent
    // turns, terminals and previews, so it never has to guess which of its
    // inherited variables came from the image versus the spec. A key the
    // separator could not represent is not shell-exportable anyway, and the
    // dispatcher's reserved-key policy would drop it. The list rides as ONE
    // `-e` argument, so it is bounded well under the kernel's per-string
    // argv limit: names past the bound are dropped, at a key boundary, with
    // a warning, rather than turning a pathological image into a boot
    // failure (codex review P2).
    let image_keys = image_env_key_list(merged_image.iter().map(|(key, _)| key.as_str()));

    let mut merged = merged_image;
    if !image_keys.is_empty() {
        merged.push((
            g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_ENV.to_string(),
            image_keys,
        ));
    }
    merged.extend(spec_env.iter().cloned());
    merged
}

/// Join the surviving image key names for `G5D_IMAGE_ENV_KEYS`, keeping the
/// image's order, skipping names the separator cannot carry, and stopping at
/// the key boundary before `IMAGE_ENV_KEYS_MAX_LEN` would be exceeded.
#[cfg(feature = "cloud-hypervisor")]
fn image_env_key_list<'a>(keys: impl Iterator<Item = &'a str>) -> String {
    let separator = g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_SEPARATOR;
    let max_len = g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_MAX_LEN;
    let mut list = String::new();
    let mut dropped = 0usize;
    for key in keys.filter(|key| !key.contains(separator)) {
        let needed = key.len() + usize::from(!list.is_empty());
        if dropped > 0 || list.len() + needed > max_len {
            dropped += 1;
            continue;
        }
        if !list.is_empty() {
            list.push(separator);
        }
        list.push_str(key);
    }
    if dropped > 0 {
        tracing::warn!(
            dropped,
            max_len,
            "image declares more environment key names than the guest carrier holds; \
             the excess keys are not forwarded to agents"
        );
    }
    list
}

#[cfg(feature = "cloud-hypervisor")]
fn resolved_guest_env(rootfs: &ResolvedRootfs, spec: &MicrovmRunSpec) -> Vec<(String, String)> {
    merge_image_and_spec_env(&rootfs.image_env, &spec.env)
}

impl std::fmt::Debug for CloudHypervisorBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudHypervisorBackend")
            .field("available", &self.available)
            .field(
                "port_allocator.allocated",
                &self.port_allocator.allocated_count(),
            )
            .finish()
    }
}

/// Line-relay a guest-console stream to host tracing under the `g5d::guest`
/// target. The caller passes secrets collected from the resolved guest env
/// (image config defaults merged with task env), and this relay redacts them via
/// the shared host credential-scrub policy (`host::supervisor::redact_secrets`;
/// `g5/g5d/host` D10) rather than re-deriving it (codex dup review, PR #258).
///
/// BYTE-oriented: a non-UTF-8 console byte must not break the loop and stall
/// draining (codex review, PR #258) — `read_until` + lossy decode keep draining
/// past malformed bytes. Owns the piped stream so it survives the
/// `RunHandle`-only (pid-kept) drop of the `Child`, streaming until the guest
/// closes the stream at VM exit.
/// Returns the relay task's `JoinHandle` — a "console drained" receipt: the
/// task finishes only after consuming the stream to EOF (VM exit), so a
/// consumer that reads the retained tail after observing VM death must first
/// await it (bounded) via `RunHandle::wait_console_drained`, or the final
/// buffered lines can be lost to the read/exit race (`microvm_runtime` D58,
/// review round 2 P1).
#[cfg_attr(not(feature = "cloud-hypervisor"), allow(dead_code))]
pub(crate) fn spawn_guest_console_relay<R>(
    stream: R,
    which: super::GuestConsoleStream,
    vm_name: String,
    secrets: Vec<String>,
    tail: Option<std::sync::Arc<super::GuestConsoleTail>>,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncBufReadExt;
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break, // EOF — guest closed the stream (VM exit)
                Ok(_) => {
                    let decoded = String::from_utf8_lossy(&buf);
                    let line = redact_secrets(decoded.trim_end(), &secrets);
                    // Retain the SAME redacted line the tracing relay emits —
                    // the single existing capture point stays the only one
                    // (`microvm_runtime` D58; issue G5Labs/G5#2400).
                    if let Some(tail) = tail.as_ref() {
                        tail.push(which, &line);
                    }
                    tracing::info!(target: "g5d::guest", vm = %vm_name, stream = which.as_str(), "{line}");
                }
                Err(_) => break,
            }
        }
    })
}

#[async_trait::async_trait]
impl MicrovmBackend for CloudHypervisorBackend {
    async fn run(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunResult> {
        // A fresh token that never fires: `run` is the uncancellable variant.
        self.run_cancellable(spec, &tokio_util::sync::CancellationToken::new())
            .await
    }

    /// Cancellable one-shot run (issue #1070, supervisor_lifecycle D25).
    ///
    /// Replaces the old `Command::output().await` shape with an explicit
    /// `spawn()` + piped-output readers + `select!` over child completion and
    /// the token, so a force-evicted VM is killed AND reaped BEFORE the
    /// per-VM rootfs clone is removed (cleanup never races process death;
    /// all VM descendants have exited by then).
    /// `kill_on_drop(true)` remains a safety net for runtime teardown paths
    /// that drop the future without cancelling.
    async fn run_cancellable(
        &self,
        _spec: &MicrovmRunSpec,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<RunResult> {
        #[cfg(not(feature = "cloud-hypervisor"))]
        {
            anyhow::bail!(
                "CloudHypervisorBackend requires the `cloud-hypervisor` Cargo feature to boot a VM. \
                 Rebuild g5d with the default features enabled."
            );
        }

        #[cfg(feature = "cloud-hypervisor")]
        {
            use tokio::io::AsyncReadExt;

            if !self.available {
                anyhow::bail!(
                    "CloudHypervisorBackend is not available on this host (no usable KVM and platform artifacts)."
                );
            }

            let vm_name = Self::generate_vm_name();
            tracing::info!(
                vm_name = %vm_name,
                image = %_spec.image,
                cpus = _spec.cpus,
                mem_mib = _spec.mem_mib,
                "Creating microVM (Cloud Hypervisor)"
            );

            let rootfs = Self::resolve_rootfs(&_spec.image, _spec.block_workload).await?;
            if _spec.block_workload
                && (!cfg!(target_os = "linux")
                    || rootfs
                        .owned_dir
                        .as_deref()
                        .is_none_or(|owned| rootfs.path.parent() != Some(owned)))
            {
                Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                anyhow::bail!("block workload requires Linux and a fresh backend-owned OCI clone; caller-owned bare rootfs is unsupported");
            }

            let start_time = std::time::Instant::now();
            let mut cmd = Self::build_vm_exec_command(_spec, &rootfs);
            self.apply_host_network_env(&mut cmd);
            cmd.stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            let scope = match super::vm_scope::VmScope::create(&mut cmd) {
                Ok(scope) => scope,
                Err(error) => {
                    Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                    return Err(error);
                }
            };
            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(e) => {
                    // Never booted; the fresh clone is already dead state.
                    // Awaited inline so the cleanup can't be lost to an
                    // immediate runtime shutdown after the error propagates.
                    Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                    return Err(e.into());
                }
            };

            // Detached pipe readers so `child.wait()` can be select!ed
            // against the cancel token without losing buffered output.
            let stdout_pipe = child.stdout.take();
            let stderr_pipe = child.stderr.take();
            let stdout_task = tokio::spawn(async move {
                let mut buf = Vec::new();
                if let Some(mut pipe) = stdout_pipe {
                    let _ = pipe.read_to_end(&mut buf).await;
                }
                buf
            });
            let stderr_task = tokio::spawn(async move {
                let mut buf = Vec::new();
                if let Some(mut pipe) = stderr_pipe {
                    let _ = pipe.read_to_end(&mut buf).await;
                }
                buf
            });

            let (status, forced) = tokio::select! {
                status = child.wait() => match status {
                    Ok(status) => (status, false),
                    Err(error) => {
                        tracing::warn!(%error, "VM wait failed; retaining custody while terminating");
                        (terminate_failed_vm_start(&mut child, &scope).await, true)
                    }
                },
                _ = _cancel.cancelled() => {
                    tracing::warn!(vm_name = %vm_name, "MicroVM run cancelled; killing and proving exit");
                    (terminate_failed_vm_start(&mut child, &scope).await, true)
                }
            };
            scope.wait_for_exit().await;
            let duration = start_time.elapsed();
            Self::spawn_owned_rootfs_cleanup(rootfs.owned_dir.clone(), vm_name.clone());

            let exit_code = status.code().unwrap_or(-1);
            let stdout =
                String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).to_string();
            let stderr =
                String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).to_string();

            tracing::info!(
                vm_name = %vm_name,
                exit_code = exit_code,
                forced = forced,
                duration_ms = duration.as_millis() as u64,
                "MicroVM run completed (Cloud Hypervisor)"
            );

            Ok(RunResult {
                exit_code,
                stdout,
                stderr,
                duration,
            })
        }
    }

    async fn start(&self, _spec: &MicrovmRunSpec) -> anyhow::Result<RunHandle> {
        #[cfg(not(feature = "cloud-hypervisor"))]
        {
            anyhow::bail!(
                "CloudHypervisorBackend requires the `cloud-hypervisor` Cargo feature to start a VM. \
                 Rebuild g5d with the default features enabled."
            );
        }

        #[cfg(feature = "cloud-hypervisor")]
        {
            if !self.available {
                anyhow::bail!(
                    "CloudHypervisorBackend is not available on this host (no usable KVM and platform artifacts)."
                );
            }

            let vm_name = Self::generate_vm_name();
            tracing::info!(
                vm_name = %vm_name,
                image = %_spec.image,
                "Starting microVM (long-running, Cloud Hypervisor)"
            );

            let rootfs = Self::resolve_rootfs(&_spec.image, _spec.block_workload).await?;
            if _spec.block_workload
                && (!cfg!(target_os = "linux")
                    || rootfs
                        .owned_dir
                        .as_deref()
                        .is_none_or(|owned| rootfs.path.parent() != Some(owned)))
            {
                Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                anyhow::bail!("block workload requires Linux and a fresh backend-owned OCI clone; caller-owned bare rootfs is unsupported");
            }

            if !_spec.mount_host_dir.join(".g5/repo-seed.ext4").is_file() {
                let prepared = match _spec.scratch_disk.as_ref() {
                    Some(disk) => Self::prepare_private_workload(&rootfs, disk.size_mib).await,
                    None => Err(anyhow::anyhow!("published workload capacity is required")),
                };
                if let Err(error) = prepared {
                    Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                    return Err(error);
                }
            }

            // In-VM observability (`g5/g5d/host` D10): PIPE the guest console and
            // line-relay it (redacted) to host tracing. Without this the carrier
            // (`start()`) path inherits stdio and never captures guest output, so a
            // hung in-VM sidecar is invisible. `run()` (boot-and-await) is unchanged.
            let guest_env = resolved_guest_env(&rootfs, _spec);
            let mut cmd = Self::build_vm_exec_command_with_env(_spec, &rootfs, &guest_env);
            self.apply_host_network_env(&mut cmd);
            cmd.stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            let scope = match super::vm_scope::VmScope::create(&mut cmd) {
                Ok(scope) => scope,
                Err(error) => {
                    Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                    return Err(error);
                }
            };
            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(e) => {
                    // Never booted; the fresh clone is already dead state.
                    // Inline (awaited), not detached: a detached task could
                    // be lost if the caller shuts the runtime down right
                    // after this error returns.
                    Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                    return Err(e.into());
                }
            };
            let child_pid = child.id();
            let vmm_loopback_addr = match child_pid {
                Some(pid) => match super::host_vmm_port_map_addr_for_pid(pid) {
                    Ok(addr) => addr,
                    Err(e) => {
                        terminate_failed_vm_start(&mut child, &scope).await;
                        Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                        return Err(anyhow::anyhow!(
                            "cannot resolve guest host-route address: {e}"
                        ));
                    }
                },
                None => {
                    terminate_failed_vm_start(&mut child, &scope).await;
                    Self::remove_owned_rootfs(rootfs.owned_dir.clone(), &vm_name).await;
                    return Err(anyhow::anyhow!(
                        "cannot resolve guest host-route address: vm-exec child has no PID"
                    ));
                }
            };

            let secrets = collect_secret_env_values(&guest_env);
            // Shared bounded tail retained on the RunHandle so a guest that
            // dies before serving leaves its final (already-redacted) output
            // readable after teardown (`microvm_runtime` D58).
            let console_tail = std::sync::Arc::new(super::GuestConsoleTail::default());
            // Drained receipts: the relay JoinHandles ride the RunHandle so a
            // post-exit tail reader can await EOF consumption first (D58).
            let mut console_relay_handles = Vec::new();
            if let Some(out) = child.stdout.take() {
                console_relay_handles.push(spawn_guest_console_relay(
                    out,
                    super::GuestConsoleStream::Stdout,
                    vm_name.clone(),
                    secrets.clone(),
                    Some(std::sync::Arc::clone(&console_tail)),
                ));
            }
            if let Some(err) = child.stderr.take() {
                console_relay_handles.push(spawn_guest_console_relay(
                    err,
                    super::GuestConsoleStream::Stderr,
                    vm_name.clone(),
                    secrets,
                    Some(std::sync::Arc::clone(&console_tail)),
                ));
            }
            let (reap_publisher, reap_monitor) = child_reap_channel();
            // A detached waiter task owns `child` (previously it was dropped
            // here and the pid alone was retained for `kill()`): it reaps the
            // VM supervisor when the VM exits — whether naturally or via
            // `kill()`'s pid-based signals, which are unaffected — and then
            // removes the per-VM rootfs clone (ontology D40). The relay tasks
            // above own the piped streams until the guest closes them.
            {
                let waiter_vm = vm_name.clone();
                let owned_dir = rootfs.owned_dir.clone();
                tokio::spawn(async move {
                    let status = match child.wait().await {
                        Ok(status) => status,
                        Err(error) => {
                            tracing::warn!(vm_name = %waiter_vm, %error, "VM wait failed; retaining custody while terminating");
                            terminate_failed_vm_start(&mut child, &scope).await
                        }
                    };
                    scope.wait_for_exit().await;
                    let receipt = reap_publisher.publish_reaped(status);
                    tracing::info!(vm_name = %waiter_vm, exit_code = ?receipt.exit_code(), "MicroVM process exited and was reaped");
                    Self::remove_owned_rootfs(owned_dir, &waiter_vm).await;
                });
            }

            Ok(RunHandle {
                id: vm_name.clone(),
                backend_type: BackendType::CloudHypervisor,
                vmm_loopback_addr: Some(vmm_loopback_addr),
                inner: RunHandleInner::Microvm { vm_name, child_pid },
                reap_monitor,
                console_tail: Some(console_tail),
                console_relay_handles: std::sync::Mutex::new(console_relay_handles),
            })
        }
    }

    async fn kill(&self, handle: &RunHandle) -> anyhow::Result<ChildReapReceipt> {
        let (vm_name, child_pid) = match &handle.inner {
            RunHandleInner::Microvm { vm_name, child_pid } => (vm_name.clone(), *child_pid),
            _ => anyhow::bail!("Invalid handle for CloudHypervisorBackend"),
        };

        if let Some(receipt) = handle.current_reap_receipt()? {
            return Ok(receipt);
        }

        tracing::info!(
            vm_name = %vm_name,
            child_pid = ?child_pid,
            "Killing microVM (Cloud Hypervisor)"
        );

        let Some(pid) = child_pid else {
            anyhow::bail!("Cloud Hypervisor handle has no child PID and no reap receipt");
        };
        #[cfg(unix)]
        {
            let receipt = super::terminate_handle_group(
                handle,
                pid,
                GRACEFUL_CHILD_REAP_TIMEOUT,
                FORCED_CHILD_REAP_TIMEOUT,
            )
            .await?;
            tracing::info!(vm_name = %vm_name, "MicroVM killed and reaped");
            return Ok(receipt);
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            anyhow::bail!("CloudHypervisorBackend kill is unsupported on this platform");
        }

        #[cfg(not(unix))]
        let receipt = handle
            .wait_for_reap(FORCED_CHILD_REAP_TIMEOUT)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "microVM {vm_name} did not publish a reap receipt after forced termination: {error}"
                )
            })?;
        #[cfg(not(unix))]
        {
            tracing::info!(vm_name = %vm_name, "MicroVM killed and reaped");
            Ok(receipt)
        }
    }

    fn probe(&self) -> bool {
        self.available
    }

    fn name(&self) -> &str {
        "cloud-hypervisor"
    }

    fn isolation_level(&self) -> IsolationLevel {
        IsolationLevel::Vm
    }
}

/// Terminate and reap a child whose startup cannot safely publish a RunHandle.
/// `kill_on_drop(true)` is the final backstop, but this path waits explicitly so
/// rootfs cleanup and slot release never race a still-running VMM. Individual
/// waits are bounded; uncertainty retains startup custody until exit is proven.
/// The outer host service still owns its bounded shutdown.
#[cfg(feature = "cloud-hypervisor")]
async fn terminate_failed_vm_start(
    child: &mut tokio::process::Child,
    scope: &super::vm_scope::VmScope,
) -> std::process::ExitStatus {
    loop {
        let result: anyhow::Result<std::process::ExitStatus> = async {
            scope.kill()?;
            let kill_error = child.start_kill().err();
            match tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await {
                Ok(Ok(status)) => {
                    scope.wait_for_exit().await;
                    Ok(status)
                }
                Ok(Err(wait_error)) => Err(anyhow::anyhow!(
                    "failed to reap vm-exec after start_kill{}: {wait_error}",
                    kill_error
                        .as_ref()
                        .map(|error| format!(" ({error})"))
                        .unwrap_or_default()
                )),
                Err(_) => Err(anyhow::anyhow!(
                    "timed out after 5s waiting for vm-exec termination{}",
                    kill_error
                        .as_ref()
                        .map(|error| format!("; start_kill failed: {error}"))
                        .unwrap_or_default()
                )),
            }
        }
        .await;
        match result {
            Ok(status) => return status,
            Err(error) => {
                tracing::warn!(%error, "unpublished VM exit unconfirmed; retaining startup custody");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Wsl2Backend (parked — see module docs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn full_slot_capacity_does_not_require_a_template() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("rootfs");
        assert!(super::check_workload_capacity(&root, u32::MAX, u32::MAX).is_err());
        assert!(!root.exists(), "overflow must fail before creating state");
        assert!(super::check_workload_capacity(&root, 0, 2).is_err());
        assert!(super::check_workload_capacity(&root, 1, 0).is_err());
        super::check_workload_capacity(&root, 1, 2).unwrap();
        assert!(super::check_workload_capacity(&root, u32::MAX, 1).is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    }

    use super::*;
    use crate::isolation::BackendType;
    #[cfg(feature = "cloud-hypervisor")]
    use g5d_foundation::AGENT_CARRIER_TOKEN_ENV;
    use std::path::PathBuf;

    #[test]
    fn test_cloud_hypervisor_backend_debug() {
        let backend = CloudHypervisorBackend::new();
        let debug = format!("{:?}", backend);
        assert!(debug.contains("CloudHypervisorBackend"));
    }

    #[cfg(all(feature = "cloud-hypervisor", unix))]
    #[tokio::test]
    async fn vm_exec_closes_inherited_stdin_before_spawn() {
        let mut cmd = tokio::process::Command::new("/bin/cat");
        // A child with piped stdin would wait for input. The VM command must
        // override even an explicitly configured input stream with /dev/null.
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true);
        close_vm_exec_stdin(&mut cmd);
        let child = cmd.spawn().unwrap();
        assert!(child.stdin.is_none());
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(2), child.wait_with_output())
                .await
                .expect("child should observe immediate stdin EOF")
                .unwrap();
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn parse_guest_host_aliases_env_splits_trims_and_drops_invalid() {
        assert_eq!(parse_guest_host_aliases_env(""), Vec::<String>::new());
        assert_eq!(parse_guest_host_aliases_env("   "), Vec::<String>::new());
        // Entries without '=' are dropped (vm-exec re-validates HOST=IP shape).
        assert_eq!(parse_guest_host_aliases_env("g5-api"), Vec::<String>::new());
        assert_eq!(
            parse_guest_host_aliases_env(" g5-api=10.0.0.2 , , edge=10.0.0.3,bogus"),
            vec!["g5-api=10.0.0.2".to_string(), "edge=10.0.0.3".to_string()],
        );
    }

    #[test]
    fn test_cloud_hypervisor_backend_name_and_level() {
        let backend = CloudHypervisorBackend::new();
        assert_eq!(backend.name(), "cloud-hypervisor");
        assert_eq!(backend.isolation_level(), IsolationLevel::Vm);
    }

    #[test]
    fn test_cloud_hypervisor_generate_vm_name_unique() {
        let name1 = CloudHypervisorBackend::generate_vm_name();
        let name2 = CloudHypervisorBackend::generate_vm_name();
        assert!(name1.starts_with("g5d-"));
        assert!(name2.starts_with("g5d-"));
        assert_ne!(name1, name2);

        let uuid_part = &name1[4..];
        assert!(uuid_part.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[tokio::test]
    async fn test_cloud_hypervisor_run_when_unavailable() {
        let backend = CloudHypervisorBackend::forced_unavailable();
        let spec = MicrovmRunSpec {
            image: "test:latest".to_string(),
            cmd: vec!["echo".to_string()],
            env: vec![],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 512,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };
        let result = backend.run(&spec).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cloud_hypervisor_start_when_unavailable() {
        let backend = CloudHypervisorBackend::forced_unavailable();
        let spec = MicrovmRunSpec {
            image: "test:latest".to_string(),
            cmd: vec!["echo".to_string()],
            env: vec![],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 512,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };
        let result = backend.start(&spec).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cloud_hypervisor_kill_wrong_handle_type() {
        let backend = CloudHypervisorBackend::forced_unavailable();
        let handle = RunHandle {
            id: "process-123".to_string(),
            backend_type: BackendType::Process,
            vmm_loopback_addr: None,
            inner: RunHandleInner::Process {
                child_pid: 123,
                pgid: Some(123),
                session_leader: false,
                tag: None,
                started: None,
            },
            reap_monitor: crate::isolation::pending_child_reap_monitor(),
            console_tail: None,
            console_relay_handles: std::sync::Mutex::new(Vec::new()),
        };
        let result = backend.kill(&handle).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid handle"));
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[tokio::test]
    async fn test_cloud_hypervisor_resolve_rootfs_rejects_non_directory_path() {
        let err = CloudHypervisorBackend::resolve_rootfs("/definitely/not/a/directory/here", false)
            .await
            .expect_err("filesystem-path-shaped non-directory image must error");
        let msg = err.to_string();
        assert!(msg.contains("not a directory"), "msg was: {}", msg);
    }

    /// Minimal digest-consistent OCI image layout for routing tests: marker +
    /// index → manifest → config(Env) + one gzip layer holding `etc/marker`.
    #[cfg(feature = "cloud-hypervisor")]
    fn write_minimal_oci_layout(root: &std::path::Path, env: &[&str]) {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use sha2::{Digest as _, Sha256};

        let blobs = root.join("blobs").join("sha256");
        std::fs::create_dir_all(&blobs).unwrap();
        let put = |bytes: &[u8]| -> (String, usize) {
            let hex = hex::encode(Sha256::digest(bytes));
            std::fs::write(blobs.join(&hex), bytes).unwrap();
            (format!("sha256:{hex}"), bytes.len())
        };

        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        {
            let mut tar = tar::Builder::new(&mut gz);
            let data = b"layout-image";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "etc/marker", &data[..])
                .unwrap();
            tar.finish().unwrap();
        }
        let layer = gz.finish().unwrap();
        let (layer_digest, layer_len) = put(&layer);
        let config = serde_json::to_vec(&serde_json::json!({ "config": { "Env": env } })).unwrap();
        let (config_digest, config_len) = put(&config);
        let manifest = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": config_digest,
                "size": config_len,
            },
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": layer_digest,
                "size": layer_len,
            }],
        }))
        .unwrap();
        let (manifest_digest, manifest_len) = put(&manifest);
        let index = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "manifests": [{
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": manifest_digest,
                "size": manifest_len,
            }],
        }))
        .unwrap();
        std::fs::write(root.join("index.json"), index).unwrap();
        std::fs::write(
            root.join("oci-layout"),
            br#"{"imageLayoutVersion":"1.0.0"}"#,
        )
        .unwrap();
    }

    /// microvm_runtime D56 routing proof at the isolation seam: a layout
    /// directory resolves through the registry-equivalent pipeline (per-VM
    /// owned clone under the state root, `Env` parsed into `image_env`, layer
    /// content present in the resolved rootfs), while a bare directory keeps
    /// its historical env-less, unowned semantics.
    #[cfg(feature = "cloud-hypervisor")]
    #[tokio::test]
    #[serial_test::serial]
    async fn test_cloud_hypervisor_resolve_rootfs_routes_layout_dir_with_env_and_owned_clone() {
        let state = tempfile::tempdir().unwrap();
        let layout = tempfile::tempdir().unwrap();
        write_minimal_oci_layout(
            layout.path(),
            &["G5_BROWSER_TOOLS_AVAILABLE=true", "FOO=bar"],
        );
        // Drop guard: a panicking assertion must still restore both
        // process-wide vars — serial_test serializes but does not restore
        // state after an unwind.
        struct EnvRestore(&'static str, Option<String>);
        impl Drop for EnvRestore {
            fn drop(&mut self) {
                match self.1.take() {
                    Some(value) => std::env::set_var(self.0, value),
                    None => std::env::remove_var(self.0),
                }
            }
        }
        let _state_guard = EnvRestore(
            "G5D_ROOTFS_STATE_DIR",
            std::env::var("G5D_ROOTFS_STATE_DIR").ok(),
        );
        std::env::set_var("G5D_ROOTFS_STATE_DIR", state.path());
        let _cache_guard = EnvRestore("G5D_OCI_CACHE_DIR", std::env::var("G5D_OCI_CACHE_DIR").ok());
        std::env::set_var("G5D_OCI_CACHE_DIR", state.path().join("layer-cache"));

        let resolved =
            CloudHypervisorBackend::resolve_rootfs(&layout.path().to_string_lossy(), false)
                .await
                .expect("layout dir must resolve through the registry-equivalent pipeline");
        let owned = resolved
            .owned_dir
            .as_ref()
            .expect("layout resolution must own a per-VM clone for GC");
        assert!(
            owned.starts_with(state.path()),
            "owned clone must live under the state root: {owned:?}"
        );
        assert!(
            resolved
                .template_dir
                .as_ref()
                .unwrap()
                .join("rootfs/etc/marker")
                .is_file(),
            "layer content must reach the resolved rootfs"
        );
        assert!(resolved
            .image_env
            .iter()
            .any(|(k, v)| k == "G5_BROWSER_TOOLS_AVAILABLE" && v == "true"));
        assert!(resolved
            .image_env
            .iter()
            .any(|(k, v)| k == "FOO" && v == "bar"));
        assert!(
            state.path().join("templates").is_dir(),
            "layout materialization must go through the shared template cache"
        );

        let block = CloudHypervisorBackend::resolve_rootfs(&layout.path().to_string_lossy(), true)
            .await
            .expect("block seed must use the same trusted template pipeline");
        let block_owner = block.owned_dir.as_ref().unwrap();
        assert_eq!(block.path, block_owner.join("rootfs"));
        assert!(!block.path.join("etc/marker").exists());
        assert!(block
            .template_dir
            .as_ref()
            .unwrap()
            .join("rootfs/etc/marker")
            .is_file());
        // A formatter temporary can be a sibling of the seed while still
        // remaining inside the launch owner's cleanup boundary.
        let sibling = block_owner.join(".g5-workload-test.ext4");
        std::fs::write(&sibling, b"temporary").unwrap();
        CloudHypervisorBackend::remove_owned_rootfs(block.owned_dir.clone(), "test-block-owner")
            .await;
        assert!(!block_owner.exists());

        // Bare-directory semantics stay untouched: no marker → no env, no
        // owned clone, path passthrough.
        let bare = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(bare.path().join("etc")).unwrap();
        let error = CloudHypervisorBackend::resolve_rootfs(&bare.path().to_string_lossy(), false)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("OCI image layout"));
    }

    #[test]
    fn test_looks_like_path_classifier() {
        assert!(looks_like_path("/foo/bar"));
        assert!(looks_like_path("./local"));
        assert!(looks_like_path("../up"));
        assert!(looks_like_path("~/in-home"));
        assert!(!looks_like_path("alpine"));
        assert!(!looks_like_path("alpine:3.18"));
        assert!(!looks_like_path("library/alpine:3.18"));
        assert!(!looks_like_path(
            "alpine@sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        ));
    }

    /// Regression for rev-7 §5.6 wire-in: when `MicrovmRunSpec.port_maps` is
    /// non-empty, `build_vm_exec_command` must emit one `-p host:guest` flag
    /// per pair, in order, before the `--` argv terminator. Empty `port_maps`
    /// must NOT emit any `-p` (the no-port-forwarding path stays byte-for-byte
    /// unchanged).
    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn build_vm_exec_command_emits_port_map_flags() {
        // Helper: stringify the command's argv (program + args) into a flat
        // Vec<String> so we can scan the slice for `-p host:guest` pairs.
        fn argv(cmd: &tokio::process::Command) -> Vec<String> {
            let std_cmd = cmd.as_std();
            let mut argv: Vec<String> = vec![std_cmd.get_program().to_string_lossy().into_owned()];
            for a in std_cmd.get_args() {
                argv.push(a.to_string_lossy().into_owned());
            }
            argv
        }

        let rootfs = ResolvedRootfs {
            path: std::path::PathBuf::from("/tmp/rootfs"),
            owned_dir: None,
            template_dir: None,
            image_env: Vec::new(),
        };

        // Case 1: no port maps → no `-p` flag in argv.
        let spec_no_pm = MicrovmRunSpec {
            image: "alpine".to_string(),
            cmd: vec!["/bin/true".to_string()],
            env: vec![],
            mount_host_dir: std::path::PathBuf::from("/tmp/ws"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 256,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };
        let argv0 = argv(&CloudHypervisorBackend::build_vm_exec_command(
            &spec_no_pm,
            &rootfs,
        ));
        assert!(
            !argv0.iter().any(|a| a == "-p"),
            "empty port_maps must not emit a `-p` flag, got argv: {argv0:?}"
        );

        // Case 2: two port maps → exactly two `-p host:guest` pairs in order,
        // before the `--` argv terminator.
        let spec_with_pm = MicrovmRunSpec {
            port_maps: vec![(49160_u16, 8080_u16), (49161_u16, 8081_u16)],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
            ..spec_no_pm.clone()
        };
        let argv1 = argv(&CloudHypervisorBackend::build_vm_exec_command(
            &spec_with_pm,
            &rootfs,
        ));
        // Slice out everything before "--" so we don't accidentally match the
        // guest cmd (it has no `-p` here, but be defensive).
        let dash_dash_pos = argv1.iter().position(|a| a == "--").expect("argv has `--`");
        let head = &argv1[..dash_dash_pos];
        let mut pm_pairs: Vec<&str> = Vec::new();
        let mut i = 0;
        while i < head.len() {
            if head[i] == "-p" {
                let next = head.get(i + 1).map(|s| s.as_str()).unwrap_or("");
                pm_pairs.push(next);
                i += 2;
            } else {
                i += 1;
            }
        }
        assert_eq!(
            pm_pairs,
            vec!["49160:8080", "49161:8081"],
            "expected two `-p host:guest` flags in order, got argv head: {head:?}"
        );
    }

    /// Ontology D53: `Some(scratch_disk)` emits `--scratch-disk-mib` plus one
    /// `--scratch-disk-mount` per mount point; `None` (the default) emits no
    /// scratch flag at all — byte-for-byte unchanged boots.
    /// `nested_virtualization` is a host property carried on the run spec:
    /// on, `vm-exec` receives `--nested-virtualization`; off, the argv is
    /// byte-for-byte today's.
    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn build_vm_exec_command_emits_the_nested_virtualization_flag_only_when_on() {
        fn argv(cmd: &tokio::process::Command) -> Vec<String> {
            let std_cmd = cmd.as_std();
            let mut argv: Vec<String> = vec![std_cmd.get_program().to_string_lossy().into_owned()];
            for a in std_cmd.get_args() {
                argv.push(a.to_string_lossy().into_owned());
            }
            argv
        }
        let rootfs = ResolvedRootfs {
            path: std::path::PathBuf::from("/tmp/rootfs"),
            owned_dir: None,
            template_dir: None,
            image_env: Vec::new(),
        };
        let spec_off = MicrovmRunSpec {
            image: "alpine".to_string(),
            cmd: vec!["/bin/true".to_string()],
            env: vec![],
            mount_host_dir: std::path::PathBuf::from("/tmp/ws"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 256,
            extra_mounts: vec![],
            port_maps: vec![],
            scratch_disk: None,
            block_workload: true,
            nested_virtualization: false,
            guest_restore: None,
        };
        let spec_on = MicrovmRunSpec {
            nested_virtualization: true,
            ..spec_off.clone()
        };
        let off = argv(&CloudHypervisorBackend::build_vm_exec_command(
            &spec_off, &rootfs,
        ));
        let on = argv(&CloudHypervisorBackend::build_vm_exec_command(
            &spec_on, &rootfs,
        ));
        assert!(
            !off.iter().any(|a| a == "--nested-virtualization"),
            "{off:?}"
        );
        let flag = on
            .iter()
            .position(|a| a == "--nested-virtualization")
            .expect("nested flag emitted");
        let terminator = on.iter().position(|a| a == "--").unwrap_or(on.len());
        assert!(flag < terminator, "flag must precede the command: {on:?}");
        let mut on_without_flag = on.clone();
        on_without_flag.remove(flag);
        assert_eq!(on_without_flag, off, "the flag is the only difference");
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn build_vm_exec_command_emits_scratch_disk_flags() {
        fn argv(cmd: &tokio::process::Command) -> Vec<String> {
            let std_cmd = cmd.as_std();
            let mut argv: Vec<String> = vec![std_cmd.get_program().to_string_lossy().into_owned()];
            for a in std_cmd.get_args() {
                argv.push(a.to_string_lossy().into_owned());
            }
            argv
        }

        let rootfs = ResolvedRootfs {
            path: std::path::PathBuf::from("/tmp/rootfs"),
            owned_dir: None,
            template_dir: None,
            image_env: Vec::new(),
        };
        let spec_off = MicrovmRunSpec {
            image: "alpine".to_string(),
            cmd: vec!["/bin/true".to_string()],
            env: vec![],
            mount_host_dir: std::path::PathBuf::from("/tmp/ws"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 256,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };
        let argv_off = argv(&CloudHypervisorBackend::build_vm_exec_command(
            &spec_off, &rootfs,
        ));
        assert!(
            !argv_off.iter().any(|a| a.starts_with("--scratch-disk")),
            "scratch_disk: None must emit no scratch flags, got argv: {argv_off:?}"
        );

        let spec_on = MicrovmRunSpec {
            scratch_disk: Some(crate::isolation::ScratchDiskSpec { size_mib: 65536 }),
            ..spec_off.clone()
        };
        let block_spec = MicrovmRunSpec {
            block_workload: true,
            nested_virtualization: false,
            guest_restore: None,
            ..spec_on.clone()
        };
        let owned = ResolvedRootfs {
            path: PathBuf::from("/tmp/owner/rootfs"),
            owned_dir: Some(PathBuf::from("/tmp/owner")),
            template_dir: Some(PathBuf::from("/tmp/templates/sha256/resolved")),
            image_env: vec![],
        };
        let block_command = CloudHypervisorBackend::build_vm_exec_command(&block_spec, &owned);
        #[derive(clap::Parser)]
        struct BootCli {
            #[command(flatten)]
            args: super::super::VmExecArgs,
        }
        use clap::Parser;
        // Parse the exact production argv (vm-exec becomes argv[0]), not just
        // individual flags: preparation-only constraints must not reject Start.
        let parsed = BootCli::try_parse_from(block_command.as_std().get_args()).unwrap();
        assert!(!parsed.args.prepare_block_template);
        assert_eq!(parsed.args.block_template_reserve_mib, None);
        assert!(argv(&block_command)
            .iter()
            .any(|arg| arg == "--block-workload"));
        assert!(block_command.as_std().get_envs().any(|(key, value)| {
            key == super::super::workload_root::OWNED_ROOT_ENV
                && value == Some(owned.path.as_os_str())
        }));
        assert!(block_command.as_std().get_envs().any(|(key, value)| {
            key == super::super::workload_template::TEMPLATE_ENV
                && value
                    == owned
                        .template_dir
                        .as_deref()
                        .map(std::path::Path::as_os_str)
        }));
        // In particular, None == parent(/) == None must not authorize a bare root.
        let bare = ResolvedRootfs {
            path: PathBuf::from("/"),
            owned_dir: None,
            template_dir: None,
            image_env: vec![],
        };
        let bare_command = CloudHypervisorBackend::build_vm_exec_command(&block_spec, &bare);
        assert!(bare_command.as_std().get_envs().any(|(key, value)| {
            key == super::super::workload_root::OWNED_ROOT_ENV && value.is_none()
        }));
        let argv_on = argv(&CloudHypervisorBackend::build_vm_exec_command(
            &spec_on, &rootfs,
        ));
        let mib_pos = argv_on
            .iter()
            .position(|a| a == "--scratch-disk-mib")
            .expect("--scratch-disk-mib present");
        assert_eq!(argv_on.get(mib_pos + 1).map(String::as_str), Some("65536"));
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn build_vm_exec_command_renders_read_only_extra_mount() {
        fn argv(cmd: &tokio::process::Command) -> Vec<String> {
            let std_cmd = cmd.as_std();
            let mut argv: Vec<String> = vec![std_cmd.get_program().to_string_lossy().into_owned()];
            for a in std_cmd.get_args() {
                argv.push(a.to_string_lossy().into_owned());
            }
            argv
        }

        let rootfs = ResolvedRootfs {
            path: std::path::PathBuf::from("/tmp/rootfs"),
            owned_dir: None,
            template_dir: None,
            image_env: Vec::new(),
        };
        let spec = MicrovmRunSpec {
            image: "alpine".to_string(),
            cmd: vec!["/bin/true".to_string()],
            env: vec![],
            mount_host_dir: std::path::PathBuf::from("/tmp/ws"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 256,
            extra_mounts: vec![crate::isolation::MicrovmMount::read_only(
                "/host/runtime",
                "/opt/g5",
            )],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };
        let argv = argv(&CloudHypervisorBackend::build_vm_exec_command(
            &spec, &rootfs,
        ));
        assert!(
            argv.windows(2)
                .any(|pair| pair[0] == "-V" && pair[1] == "/host/runtime:/opt/g5:ro"),
            "read-only extra mount must render as -V host:guest:ro: {argv:?}"
        );
        assert_eq!(
            argv.iter().filter(|a| *a == "-V").count(),
            2,
            "exactly the primary mount and one docs device: {argv:?}"
        );
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn image_env_defaults_travel_privately_with_spec_overrides() {
        fn argv(cmd: &tokio::process::Command) -> Vec<String> {
            let std_cmd = cmd.as_std();
            let mut argv: Vec<String> = vec![std_cmd.get_program().to_string_lossy().into_owned()];
            for a in std_cmd.get_args() {
                argv.push(a.to_string_lossy().into_owned());
            }
            argv
        }

        let rootfs = ResolvedRootfs {
            path: std::path::PathBuf::from("/tmp/rootfs"),
            owned_dir: None,
            template_dir: None,
            image_env: vec![
                ("JAVA_HOME".to_string(), "/opt/java/openjdk".to_string()),
                ("MAVEN_HOME".to_string(), "/usr/share/maven".to_string()),
                (
                    "PATH".to_string(),
                    "/usr/share/maven/bin:/opt/java/openjdk/bin:/usr/bin".to_string(),
                ),
            ],
        };
        let spec = MicrovmRunSpec {
            image: "image".to_string(),
            cmd: vec!["/bin/true".to_string()],
            env: vec![
                ("PATH".to_string(), "/workspace/bin:/usr/bin".to_string()),
                ("G5D_TASK_ID".to_string(), "task-1".to_string()),
            ],
            mount_host_dir: std::path::PathBuf::from("/tmp/ws"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 256,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let command = CloudHypervisorBackend::build_vm_exec_command(&spec, &rootfs);
        let args = argv(&command);
        let environment = guest_environment(&command);
        assert!(!args.iter().any(|a| a == "-e" || a.contains("JAVA_HOME")));
        assert_eq!(environment["JAVA_HOME"], "/opt/java/openjdk");
        assert_eq!(environment["MAVEN_HOME"], "/usr/share/maven");
        assert_eq!(environment["PATH"], "/workspace/bin:/usr/bin");
        assert_eq!(environment["G5D_IMAGE_ENV_KEYS"], "JAVA_HOME:MAVEN_HOME");
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn image_env_keys_are_not_named_when_the_image_declares_none() {
        let merged =
            merge_image_and_spec_env(&[], &[("G5D_TASK_ID".to_string(), "task-1".to_string())]);
        assert_eq!(
            merged,
            vec![("G5D_TASK_ID".to_string(), "task-1".to_string())],
            "no image env means no key list and nothing else added"
        );
        let all_overridden = merge_image_and_spec_env(
            &[("PATH".to_string(), "/image".to_string())],
            &[("PATH".to_string(), "/spec".to_string())],
        );
        assert_eq!(
            all_overridden,
            vec![("PATH".to_string(), "/spec".to_string())],
            "an image key the spec overrides is neither forwarded nor named"
        );
    }

    /// [g5.runtime.image-env]: the key list is one argv string, so a
    /// pathological image (2,048 distinct 64-byte names, 133 KB joined) must
    /// not exceed the kernel's per-string limit and fail the spawn; the
    /// carrier stops at a key boundary and keeps the image's order.
    /// [g5.runtime.image-env]: the carrier is host-generated only. An image
    /// that declares `G5D_IMAGE_ENV_KEYS` itself must not get its value into
    /// the guest, whether or not any other image key survives.
    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn an_image_cannot_supply_its_own_key_list() {
        let carrier = g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_ENV;
        let spec = vec![("SPEC_ONLY".to_string(), "spec".to_string())];

        let only_carrier =
            merge_image_and_spec_env(&[(carrier.to_string(), "SPEC_ONLY".to_string())], &spec);
        assert_eq!(
            only_carrier, spec,
            "an image-declared carrier is dropped: {only_carrier:?}"
        );

        let with_keys = merge_image_and_spec_env(
            &[
                (carrier.to_string(), "SPEC_ONLY".to_string()),
                ("JAVA_HOME".to_string(), "/opt/java".to_string()),
            ],
            &spec,
        );
        let carriers: Vec<&String> = with_keys
            .iter()
            .filter(|(key, _)| key == carrier)
            .map(|(_, value)| value)
            .collect();
        assert_eq!(
            carriers,
            vec!["JAVA_HOME"],
            "only the host-generated list survives: {with_keys:?}"
        );
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn image_env_key_list_is_bounded_at_a_key_boundary() {
        let max_len = g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_MAX_LEN;
        let names: Vec<String> = (0..2048).map(|i| format!("K{i:0>63}")).collect();
        assert!(names.iter().all(|name| name.len() == 64));
        let list = image_env_key_list(names.iter().map(String::as_str));
        assert!(list.len() <= max_len, "{} > {max_len}", list.len());
        assert!(
            list.len() > max_len - 65,
            "the bound should be filled to the last whole key"
        );
        assert!(!list.ends_with(':') && !list.starts_with(':'));
        let kept: Vec<&str> = list.split(':').collect();
        assert_eq!(
            kept,
            names
                .iter()
                .take(kept.len())
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
        assert_eq!(kept.len(), (max_len + 1) / 65);

        // Below the bound nothing is dropped, and a name the separator cannot
        // carry is skipped without consuming the budget.
        assert_eq!(
            image_env_key_list(["A", "B:C", "D"].into_iter()),
            "A:D".to_string()
        );
        assert_eq!(image_env_key_list(std::iter::empty()), String::new());
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn image_env_secret_values_are_collected_for_guest_console_redaction() {
        let rootfs = ResolvedRootfs {
            path: std::path::PathBuf::from("/tmp/rootfs"),
            owned_dir: None,
            template_dir: None,
            image_env: vec![
                (
                    "IMAGE_API_TOKEN".to_string(),
                    "image-secret-token-12345".to_string(),
                ),
                (
                    "PATH".to_string(),
                    "/usr/share/maven/bin:/usr/bin".to_string(),
                ),
            ],
        };
        let spec = MicrovmRunSpec {
            image: "image".to_string(),
            cmd: vec!["/bin/true".to_string()],
            env: vec![("G5D_TASK_ID".to_string(), "task-1".to_string())],
            mount_host_dir: std::path::PathBuf::from("/tmp/ws"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 256,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let guest_env = resolved_guest_env(&rootfs, &spec);
        let secrets = collect_secret_env_values(&guest_env);
        assert!(
            secrets
                .iter()
                .any(|value| value == "image-secret-token-12345"),
            "credential-shaped image env values must be part of guest-console redaction"
        );

        let redacted = redact_secrets("guest printed image-secret-token-12345", &secrets);
        assert_eq!(redacted, "guest printed [REDACTED]");
    }

    #[cfg(feature = "cloud-hypervisor")]
    fn guest_environment(
        command: &tokio::process::Command,
    ) -> std::collections::BTreeMap<String, String> {
        let raw = command
            .as_std()
            .get_envs()
            .find_map(|(key, value)| {
                (key == super::super::cloud_hypervisor::GUEST_ENV)
                    .then_some(value)
                    .flatten()
            })
            .unwrap();
        serde_json::from_str(raw.to_str().unwrap()).unwrap()
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn carrier_token_and_host_network_env_use_their_trusted_channels() {
        fn argv(cmd: &tokio::process::Command) -> Vec<String> {
            let std_cmd = cmd.as_std();
            let mut argv: Vec<String> = vec![std_cmd.get_program().to_string_lossy().into_owned()];
            for a in std_cmd.get_args() {
                argv.push(a.to_string_lossy().into_owned());
            }
            argv
        }

        let rootfs = ResolvedRootfs {
            path: std::path::PathBuf::from("/tmp/rootfs"),
            owned_dir: None,
            template_dir: None,
            image_env: vec![],
        };
        let spec = MicrovmRunSpec {
            image: "image".to_string(),
            cmd: vec!["/bin/true".to_string()],
            env: vec![
                (
                    AGENT_CARRIER_TOKEN_ENV.to_string(),
                    "secret-token".to_string(),
                ),
                (
                    super::super::guest_egress::HOST_API_URL_ENV.to_string(),
                    "http://localhost:9999".to_string(),
                ),
                ("G5D_PASST_UID".to_string(), "0".to_string()),
            ],
            mount_host_dir: std::path::PathBuf::from("/tmp/ws"),
            mount_guest_dir: "/work".to_string(),
            cpus: 1,
            mem_mib: 256,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let mut cmd = CloudHypervisorBackend::build_vm_exec_command(&spec, &rootfs);
        CloudHypervisorBackend::new()
            .with_host_api_url(Some("http://localhost:8080".into()))
            .apply_host_network_env(&mut cmd);
        let argv = argv(&cmd);
        assert!(
            !argv.iter().any(|arg| arg.contains("secret-token")),
            "carrier token must not be serialized into argv: {argv:?}"
        );
        let std_cmd = cmd.as_std();
        let environment = guest_environment(&cmd);
        assert_eq!(environment[AGENT_CARRIER_TOKEN_ENV], "secret-token");
        assert!(!std_cmd
            .get_envs()
            .any(|(key, _)| key == AGENT_CARRIER_TOKEN_ENV));
        assert!(std_cmd.get_envs().any(|(key, value)| {
            key == super::super::guest_egress::HOST_API_URL_ENV
                && value == Some(std::ffi::OsStr::new("http://localhost:8080"))
        }));
        assert!(!std_cmd.get_envs().any(|(key, _)| key == "G5D_PASST_UID"));
        assert_eq!(environment["G5D_PASST_UID"], "0");
        assert!(!argv.iter().any(|arg| arg == "G5D_PASST_UID=0"));
    }

    #[cfg(feature = "cloud-hypervisor")]
    #[test]
    fn parse_image_env_preserves_values_with_equals_and_rejects_missing_key() {
        let parsed = parse_image_env(&[
            "PATH=/usr/share/maven/bin:/usr/bin".to_string(),
            "JAVA_TOOL_OPTIONS=-Dk=v".to_string(),
        ])
        .expect("valid OCI Env entries parse");
        assert_eq!(
            parsed,
            vec![
                (
                    "PATH".to_string(),
                    "/usr/share/maven/bin:/usr/bin".to_string()
                ),
                ("JAVA_TOOL_OPTIONS".to_string(), "-Dk=v".to_string()),
            ]
        );

        let err = parse_image_env(&["BROKEN".to_string()]).expect_err("missing '=' is invalid");
        assert!(
            err.to_string().contains("missing '='"),
            "unexpected error: {err}"
        );
    }
}
