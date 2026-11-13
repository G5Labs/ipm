//! `g5d host` — run the workspace-VM host control plane.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use g5_contracts::error::G5Error;
use g5_contracts::types::worker_host::{
    DEFAULT_WORKER_HOST_SCRATCH_DISK_MIB, MAX_WORKER_HOST_SCRATCH_DISK_MIB,
    MIN_WORKER_HOST_SCRATCH_DISK_MIB,
};
use g5d_foundation::oci::OciExtractor;
use g5d_foundation::workspace::{self, RepoCache, WorkspaceCheckout};
use g5d_host::ctl::{EventJournal, HostCommandProcessor};
use g5d_host::runtime::{
    IsolationVmLauncher, NoOtherSlotUsage, ProcessVmLauncher, VmLauncher, WorkspaceHostState,
};
use g5d_host::tunnel::{ControlSessionConfig, HostTunnelCarrier, HostTunnelConfig};
use g5d_host::workspace_vm::{WorkspaceVmRuntime, WorkspaceVmRuntimeConfig};
use g5d_isolation::{
    IsolationConfig, IsolationLevel, IsolationManager, IsolationMode, ResourceConfig,
};
use serde_json::Value;
use tokio::sync::watch;
use tracing::{info, warn};
use uuid::Uuid;

use crate::cli::{GlobalFlags, HostArgs};

#[path = "host_resources.rs"]
mod resources;

/// How this host isolates its workspaces, resolved once from the environment
/// and passed explicitly (`g5.runtime.boot.backend`,
/// `g5.runtime.boot.backend.process-local`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostIsolation {
    /// One Cloud Hypervisor microVM per workspace: the hosted default.
    MicroVm,
    /// Every workspace is a host process group. Localhost development only;
    /// reachable solely through the explicit mode plus acknowledgement.
    Process,
}

pub const ISOLATION_MODE_ENV: &str = "G5D_ISOLATION_MODE";
pub const ALLOW_PROCESS_ISOLATION_ENV: &str = "G5D_ALLOW_PROCESS_ISOLATION";
pub const WORKLOAD_ROOT_ENV: &str = "G5D_WORKLOAD_ROOT";
/// `on` boots every workspace VM with hardware virtualization exposed
/// (`g5.architecture.microvm_backend.nested`); unset or `off` keeps today's
/// guests. Written into the host environment at bring-up from the API's
/// `G5_WORKER_NESTED_VIRTUALIZATION`.
pub const NESTED_VIRTUALIZATION_ENV: &str = "G5D_NESTED_VIRTUALIZATION";

/// Resolve once at host startup; a typo must not silently select another
/// filesystem layout (`g5.architecture.build_scratch`).
fn resolve_block_workload(value: Option<&str>) -> Result<bool, G5Error> {
    match value {
        None | Some("block") => Ok(true),
        Some(other) => Err(G5Error::Config(format!(
            "unrecognized {WORKLOAD_ROOT_ENV} value {other:?}: expected block; virtio-fs has been removed"
        ))),
    }
}

/// Resolve the nested-virtualization switch once; a typo is an error, never
/// a silently different exposure.
fn resolve_nested_virtualization(value: Option<&str>) -> Result<bool, G5Error> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("off") => Ok(false),
        Some("on") => Ok(true),
        Some(other) => Err(G5Error::Config(format!(
            "unrecognized {NESTED_VIRTUALIZATION_ENV} value {other:?}: expected on or off"
        ))),
    }
}

/// Nested virtualization needs a VMM to pass `nested=on` to and a platform
/// kernel with KVM built in; otherwise guests would see VMX but no `/dev/kvm`.
/// Refusing at startup keeps such a host from registering and failing Starts.
fn validate_nested_virtualization(
    nested_virtualization: bool,
    isolation: HostIsolation,
    linux: bool,
    kernel_has_kvm: impl FnOnce() -> Result<bool, G5Error>,
) -> Result<(), G5Error> {
    if !nested_virtualization {
        return Ok(());
    }
    if isolation != HostIsolation::MicroVm || !linux {
        return Err(G5Error::Config(format!(
            "{NESTED_VIRTUALIZATION_ENV}=on requires Linux microVM isolation"
        )));
    }
    if !kernel_has_kvm()? {
        return Err(G5Error::Config(format!(
            "{NESTED_VIRTUALIZATION_ENV}=on requires a platform kernel with KVM built in; the installed kernel.config lacks CONFIG_KVM=y, so deploy the KVM-enabled worker image before enabling the switch"
        )));
    }
    Ok(())
}

/// Whether the pinned platform kernel declares built-in KVM. Without the
/// Cloud Hypervisor feature there is no platform kernel to inspect.
fn platform_kernel_declares_kvm() -> Result<bool, G5Error> {
    #[cfg(feature = "cloud-hypervisor")]
    {
        g5d_isolation::cloud_hypervisor::platform_kernel_has_kvm().map_err(G5Error::Config)
    }
    #[cfg(not(feature = "cloud-hypervisor"))]
    {
        Ok(false)
    }
}

fn validate_block_workload(
    block_workload: bool,
    isolation: HostIsolation,
    linux: bool,
    scratch_disk_mib: u32,
) -> Result<(), G5Error> {
    if !block_workload {
        return Ok(());
    }
    if isolation != HostIsolation::MicroVm || !linux {
        return Err(G5Error::Config(format!(
            "{WORKLOAD_ROOT_ENV}=block requires Linux microVM isolation"
        )));
    }
    if scratch_disk_mib == 0 {
        return Err(G5Error::Config(format!(
            "{WORKLOAD_ROOT_ENV}=block requires positive G5D_SCRATCH_DISK_MIB capacity for the complete workload filesystem"
        )));
    }
    Ok(())
}

fn resolve_scratch_disk_mib(value: Option<&str>, block_workload: bool) -> Result<u32, G5Error> {
    let parsed = value
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| {
            (MIN_WORKER_HOST_SCRATCH_DISK_MIB..=MAX_WORKER_HOST_SCRATCH_DISK_MIB).contains(value)
        });
    if block_workload && value.is_some() && parsed.is_none() {
        return Err(G5Error::Config(format!(
            "{WORKLOAD_ROOT_ENV}=block requires G5D_SCRATCH_DISK_MIB in {MIN_WORKER_HOST_SCRATCH_DISK_MIB}..={MAX_WORKER_HOST_SCRATCH_DISK_MIB}; the capacity includes the image, workspace, Docker and build outputs"
        )));
    }
    // Preserve the legacy fallback. The default is a published capacity, not
    // a claim that every image and workspace will fit in that capacity.
    Ok(parsed.unwrap_or(DEFAULT_WORKER_HOST_SCRATCH_DISK_MIB) as u32)
}

fn resolve_dispatcher_ready_timeout(value: Option<&str>, block_workload: bool) -> Duration {
    // Cold environment preparation pins a private disk before this timer starts.
    // This window covers bootstrap and dispatcher readiness; keep it below the
    // API's 125s command wait. Cold preparation still consumes the initial lease,
    // so an expired Start must retry with current authority and its now-warm cache.
    Duration::from_secs(
        value
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(if block_workload { 120 } else { 30 })
            .max(1),
    )
}

/// The one place the host decides its isolation. `process` is accepted only
/// with the explicit acknowledgement; an unset or `microvm` value keeps the
/// microVM requirement; anything else is an error rather than a warning, so a
/// typo can never select a weaker boundary or be ignored.
pub fn resolve_host_isolation(
    get: impl Fn(&str) -> Option<String>,
) -> Result<HostIsolation, G5Error> {
    let mode = get(ISOLATION_MODE_ENV)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    match mode.as_deref() {
        None | Some("microvm") => Ok(HostIsolation::MicroVm),
        Some("process") => {
            let acknowledged = get(ALLOW_PROCESS_ISOLATION_ENV)
                .map(|value| value.trim().to_ascii_lowercase())
                .is_some_and(|value| value == "1" || value == "true");
            if acknowledged {
                Ok(HostIsolation::Process)
            } else {
                Err(G5Error::Config(format!(
                    "{ISOLATION_MODE_ENV}=process runs every workspace as an unisolated host process and is for localhost development only; set {ALLOW_PROCESS_ISOLATION_ENV}=1 to acknowledge that, or use microvm"
                )))
            }
        }
        Some(other) => Err(G5Error::Config(format!(
            "unrecognized {ISOLATION_MODE_ENV} value {other:?}: expected microvm or process"
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub host_id: Uuid,
    pub org_id: Uuid,
    pub api_url: String,
    pub host_jwt: String,
    pub base_image: Option<String>,
    pub isolation: HostIsolation,
    pub block_workload: bool,
    pub nested_virtualization: bool,
    pub slots_total: u32,
    pub cpus_per_vm: u8,
    pub mem_mib_per_vm: u32,
    pub scratch_disk_mib: u32,
    pub runtime_bin_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_root: PathBuf,
    pub workspaces_root: PathBuf,
    pub dispatcher_ready_timeout: Duration,
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn jwt_claims(token: &str) -> Result<Value, G5Error> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| G5Error::Config("G5_HOST_JWT is not a JWT".to_string()))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|error| G5Error::Config(format!("G5_HOST_JWT payload is invalid: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| G5Error::Config(format!("G5_HOST_JWT claims are invalid: {error}")))
}

fn uuid_claim(claims: &Value, names: &[&str]) -> Option<Uuid> {
    names.iter().find_map(|name| {
        claims
            .get(*name)
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
    })
}

pub fn build_config(args: &HostArgs, global: &GlobalFlags) -> Result<HostConfig, G5Error> {
    build_config_with_hardware(args, global, resources::probe().map_err(G5Error::Config)?)
}

fn build_config_with_hardware(
    args: &HostArgs,
    global: &GlobalFlags,
    hardware: resources::HostHardware,
) -> Result<HostConfig, G5Error> {
    let host_jwt = args
        .host_jwt
        .clone()
        .or_else(|| env_non_empty("G5_HOST_JWT"))
        .ok_or_else(|| G5Error::Config("g5d host requires G5_HOST_JWT".to_string()))?;
    if host_jwt.starts_with("g5w_") {
        return Err(G5Error::Config(
            "g5d host requires a host JWT; worker registration tokens do not authorize host control"
                .to_string(),
        ));
    }
    let claims = jwt_claims(&host_jwt)?;
    let token_host_id = uuid_claim(&claims, &["host_id", "sub"])
        .ok_or_else(|| G5Error::Config("host JWT has no UUID host identity".to_string()))?;
    let token_org_id = uuid_claim(&claims, &["org_id", "org"])
        .ok_or_else(|| G5Error::Config("host JWT has no UUID organization".to_string()))?;
    let host_id = args
        .host_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|error| G5Error::Config(format!("invalid --host-id: {error}")))?
        .unwrap_or(token_host_id);
    if host_id != token_host_id {
        return Err(G5Error::Config(
            "--host-id does not match the host JWT".to_string(),
        ));
    }
    let org_id = args
        .org_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|error| G5Error::Config(format!("invalid --org-id: {error}")))?
        .unwrap_or(token_org_id);
    if org_id != token_org_id {
        return Err(G5Error::Config(
            "--org-id does not match the host JWT".to_string(),
        ));
    }

    let isolation = resolve_host_isolation(|key| std::env::var(key).ok())?;
    let block_workload = isolation == HostIsolation::MicroVm
        && resolve_block_workload(std::env::var(WORKLOAD_ROOT_ENV).ok().as_deref())?;
    let nested_virtualization =
        resolve_nested_virtualization(std::env::var(NESTED_VIRTUALIZATION_ENV).ok().as_deref())?;
    validate_nested_virtualization(
        nested_virtualization,
        isolation,
        cfg!(target_os = "linux"),
        platform_kernel_declares_kvm,
    )?;
    // Two process-mode workspaces would share the host's loopback ports
    // (`g5.runtime.boot.backend.process-local`), so one slot unless asked.
    let slots_total = args.max_microvms.unwrap_or(match isolation {
        HostIsolation::MicroVm => 4,
        HostIsolation::Process => 1,
    });
    let (cpus_per_vm, mem_mib_per_vm) =
        resources::resolve(hardware, slots_total, args.cpus_per_vm, args.mem_mib_per_vm)
            .map_err(G5Error::Config)?;
    let runtime_bin_dir = args
        .runtime_bin_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("/var/g5d-data/runtime/bin"));
    let state_dir = args
        .state_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("/var/g5d-data/state"));
    let cache_root = args
        .cache_root
        .clone()
        .unwrap_or_else(workspace::resolved_repo_cache_root);
    let workspaces_root = args
        .workspaces_root
        .clone()
        .unwrap_or_else(workspace::resolved_workspaces_root);
    let api_url = global
        .api_url
        .clone()
        .or_else(|| env_non_empty("G5_CONTROL_PLANE_URL"))
        .or_else(|| env_non_empty("G5D_API_URL"))
        .unwrap_or_else(|| "http://localhost:8080".to_string());
    let scratch_disk_mib = resolve_scratch_disk_mib(
        std::env::var("G5D_SCRATCH_DISK_MIB").ok().as_deref(),
        block_workload,
    )?;
    validate_block_workload(
        block_workload,
        isolation,
        cfg!(target_os = "linux"),
        scratch_disk_mib,
    )?;
    let dispatcher_ready_timeout = resolve_dispatcher_ready_timeout(
        env_non_empty("G5D_DISPATCHER_READY_TIMEOUT_SECS").as_deref(),
        block_workload,
    );
    let base_image = args
        .base_image
        .as_ref()
        .map(|image| image.trim().to_string());
    if let Some(image) = &base_image {
        if ["/", "./", "../", "~/"]
            .iter()
            .any(|prefix| image.starts_with(prefix))
        {
            return Err(G5Error::Config(
                "invalid host workspace image: expected an OCI reference, not a rootfs path"
                    .to_string(),
            ));
        }
        g5d_foundation::oci::parse_image_ref(image)
            .map_err(|error| G5Error::Config(format!("invalid host workspace image: {error}")))?;
    }

    Ok(HostConfig {
        host_id,
        org_id,
        api_url,
        host_jwt,
        base_image,
        isolation,
        block_workload,
        nested_virtualization,
        slots_total,
        cpus_per_vm,
        mem_mib_per_vm,
        scratch_disk_mib,
        runtime_bin_dir,
        state_dir,
        cache_root,
        workspaces_root,
        dispatcher_ready_timeout,
    })
}

fn verify_layout(config: &HostConfig) -> Result<(), G5Error> {
    let g5d = config.runtime_bin_dir.join("g5d");
    if !g5d.is_file() {
        return Err(G5Error::Config(format!(
            "workspace runtime binary is absent: {}",
            g5d.display()
        )));
    }
    let runtime_root = config
        .runtime_bin_dir
        .parent()
        .unwrap_or(&config.runtime_bin_dir);
    for relative in ["modules/catalog.json", "modules/catalog.sha256"] {
        let path = runtime_root.join(relative);
        if !path.is_file() {
            return Err(G5Error::Config(format!(
                "workspace runtime catalog is absent: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

/// The tools a process-mode host's runtime directory must supply beside the
/// catalog and harness: what the shipped catalog's modules and the `full-stack`
/// preview profile run (`g5.runtime.boot.backend.process-local`).
const PROCESS_RUNTIME_TOOLS: &[&str] = &["g5d", "claude", "codex", "node", "git", "docker"];

/// The fake provider CLIs a deterministic lane resolves beside the runtime's
/// `g5d` instead of the real providers. Required only when such a lane is
/// selected in this host's environment ([`DETERMINISTIC_PROVIDER_ENV`]); a
/// developer's host never needs them.
const FAKE_PROVIDER_TOOLS: &[&str] = &["fake-claude", "fake-codex", "fake-g5-tools"];

/// The host environment flags that select the fake providers for every
/// workspace of this host.
const DETERMINISTIC_PROVIDER_ENV: &[&str] = &["G5_CLAUDE_CODE_FAKE", "G5_CODEX_FAKE"];

fn deterministic_providers_selected(get: impl Fn(&str) -> Option<String>) -> bool {
    DETERMINISTIC_PROVIDER_ENV.iter().any(|key| {
        get(key).is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
    })
}

/// Process-mode form of [`verify_layout`]: the host-native runtime directory
/// must be complete before this host registers, with the same fail-closed
/// shape as the Linux bundle. Its `g5d` must be this very executable, because
/// the dispatcher and the catalog's `g5d` entrypoints run the host binary.
fn verify_process_layout(
    config: &HostConfig,
    executable: &std::path::Path,
    deterministic_providers: bool,
) -> Result<(), G5Error> {
    for tool in PROCESS_RUNTIME_TOOLS {
        let path = config.runtime_bin_dir.join(tool);
        if !path.is_file() {
            return Err(G5Error::Config(format!(
                "host-native runtime tool is absent: {} (process mode needs {})",
                path.display(),
                PROCESS_RUNTIME_TOOLS.join(", ")
            )));
        }
    }
    if deterministic_providers {
        for tool in FAKE_PROVIDER_TOOLS {
            let path = config.runtime_bin_dir.join(tool);
            if !path.is_file() {
                return Err(G5Error::Config(format!(
                    "host-native runtime tool is absent: {} (a deterministic provider flag is set, so process mode needs {})",
                    path.display(),
                    FAKE_PROVIDER_TOOLS.join(", ")
                )));
            }
        }
    }
    let runtime_root = config
        .runtime_bin_dir
        .parent()
        .unwrap_or(&config.runtime_bin_dir);
    for relative in [
        "modules/catalog.json",
        "modules/catalog.sha256",
        "assets/harness/AGENTS.md",
    ] {
        let path = runtime_root.join(relative);
        if !path.is_file() {
            return Err(G5Error::Config(format!(
                "host-native runtime is incomplete: {} is absent",
                path.display()
            )));
        }
    }
    let resolve = |path: &std::path::Path| {
        path.canonicalize().map_err(|error| {
            G5Error::Config(format!(
                "cannot resolve runtime path {}: {error}",
                path.display()
            ))
        })
    };
    let runtime_g5d = resolve(&config.runtime_bin_dir.join("g5d"))?;
    let host = resolve(executable)?;
    if runtime_g5d != host {
        return Err(G5Error::Config(format!(
            "process mode runs the dispatcher from this host executable; {} must resolve to {} (it resolves to {})",
            config.runtime_bin_dir.join("g5d").display(),
            host.display(),
            runtime_g5d.display()
        )));
    }
    Ok(())
}

/// The exported guest bundle must not contain or alias the host executable.
/// Image and container packaging supply independent copies (SEC-C01).
fn verify_runtime_separation(
    runtime_root: &std::path::Path,
    executable: &std::path::Path,
    library_paths: &[std::path::PathBuf],
) -> Result<(), G5Error> {
    let resolve = |path: &std::path::Path| {
        path.canonicalize().map_err(|error| {
            G5Error::Config(format!(
                "cannot resolve runtime path {}: {error}",
                path.display()
            ))
        })
    };
    let root = resolve(runtime_root)?;
    for path in library_paths {
        // Empty loader entries mean the current directory. Nonexistent entries
        // are ignored by the loader; existing symlinks must be resolved.
        let path = if path.as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            path.as_path()
        };
        if path.exists() && resolve(path)?.starts_with(&root) {
            return Err(G5Error::Config(
                "host library search path must not use the exported guest runtime".into(),
            ));
        }
    }
    let host = resolve(executable)?;
    let guest = resolve(&root.join("bin/g5d"))?;
    let mut aliases_host = guest == host;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let host_meta = host
            .metadata()
            .map_err(|e| G5Error::Config(e.to_string()))?;
        let guest_meta = guest
            .metadata()
            .map_err(|e| G5Error::Config(e.to_string()))?;
        aliases_host |= host_meta.dev() == guest_meta.dev() && host_meta.ino() == guest_meta.ino();
    }
    if host.starts_with(&root) || !guest.starts_with(&root) || aliases_host {
        return Err(G5Error::Config("guest runtime must be a separate bundle from the host executable; stage independent host and guest copies".into()));
    }
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() -> std::io::Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> std::io::Result<()> {
    tokio::signal::ctrl_c().await
}

/// Run until SIGINT/SIGTERM, then stop every workspace VM before returning.
pub async fn execute(args: &HostArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let config = build_config(args, global)?;
    execute_with_extractor(args, config, OciExtractor::from_env()).await
}

async fn execute_with_extractor(
    args: &HostArgs,
    config: HostConfig,
    extractor: OciExtractor,
) -> Result<(), G5Error> {
    validate_block_workload(
        config.block_workload,
        config.isolation,
        cfg!(target_os = "linux"),
        config.scratch_disk_mib,
    )?;
    std::fs::create_dir_all(&config.state_dir).map_err(|error| G5Error::Internal {
        message: format!("failed to create {}: {error}", config.state_dir.display()),
    })?;
    std::fs::create_dir_all(&config.workspaces_root).map_err(|error| G5Error::Internal {
        message: format!(
            "failed to create {}: {error}",
            config.workspaces_root.display()
        ),
    })?;
    let executable = std::env::current_exe()
        .map_err(|error| G5Error::Config(format!("cannot identify host executable: {error}")))?;
    match config.isolation {
        HostIsolation::MicroVm => {
            if args.skip_layout_check {
                warn!("workspace host runtime layout check was explicitly skipped");
            } else {
                verify_layout(&config)?;
            }
            // Runtime separation is an isolation boundary, even when optional
            // layout completeness checks are skipped for development.
            let runtime_root = config
                .runtime_bin_dir
                .parent()
                .unwrap_or(&config.runtime_bin_dir);
            let library_paths: Vec<_> = [
                "LD_LIBRARY_PATH",
                "DYLD_LIBRARY_PATH",
                "DYLD_FALLBACK_LIBRARY_PATH",
            ]
            .iter()
            .filter_map(std::env::var_os)
            .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
            .collect();
            verify_runtime_separation(runtime_root, &executable, &library_paths)?;
        }
        HostIsolation::Process => {
            // `g5.runtime.boot.backend.process-local`: the dispatcher is this
            // executable by design, so the guest-bundle separation check does
            // not apply; the host-native runtime completeness check does.
            warn!(
                host_id = %config.host_id,
                "process isolation: every workspace runs as an unisolated host process group (localhost development only)"
            );
            if args.skip_layout_check {
                warn!("host-native runtime layout check was explicitly skipped");
            } else {
                verify_process_layout(
                    &config,
                    &executable,
                    deterministic_providers_selected(|key| std::env::var(key).ok()),
                )?;
            }
            info!("process mode: guest runtime separation check skipped (the dispatcher is the host executable by design)");
        }
    }

    // A fresh host is activated by its first hello/heartbeat. Prepare before
    // starting the carrier so neither placement nor rotation can see it as
    // ready while its configured image is still cold. Failure exits without
    // registering; the service supervisor retries using the existing cache.
    // The template prepared here is the only one the cache door may seed
    // sccache objects from: it is the organization's platform image, pulled
    // by the host itself, never a workspace-selected custom image.
    let mut seed_templates = Vec::new();
    if let (Some(image), HostIsolation::Process) = (&config.base_image, config.isolation) {
        warn!(
            host_id = %config.host_id,
            image,
            "process mode: host workspace image preparation skipped (not applicable; workspaces run on the host's own userland)"
        );
    } else if let Some(image) = &config.base_image {
        let started = std::time::Instant::now();
        // Select once: a mutable tag must not change between directory assembly
        // and downloading its companion block artifact.
        let pinned_image = extractor.pin_image_ref(image).await.map_err(|error| {
            G5Error::Config(format!("host workspace image resolution failed: {error}"))
        })?;
        let image = &pinned_image;
        let template_root = g5d_isolation::microvm::rootfs_state_dir().join("templates");
        info!(host_id = %config.host_id, image, "Preparing host workspace image");
        let template = tokio::time::timeout(
            Duration::from_secs(600),
            extractor.prepare_rootfs_template(image, &template_root),
        )
        .await
        .map_err(|_| G5Error::Internal {
            message: format!("host workspace image preparation timed out after 600s: {image}"),
        })?
        .map_err(|error| G5Error::Internal {
            message: format!("host workspace image preparation failed for {image}: {error}"),
        })?;
        if config.block_workload {
            g5d_isolation::microvm::prepare_workload_image(
                image,
                config.scratch_disk_mib,
                config.slots_total,
            )
            .await
            .map_err(|error| {
                G5Error::Config(format!(
                    "host block template preparation failed before registration: {error}"
                ))
            })?;
        }
        seed_templates.push(template);
        info!(
            host_id = %config.host_id,
            image,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Host workspace image prepared"
        );
    }

    let launcher: Arc<dyn VmLauncher> = match config.isolation {
        HostIsolation::MicroVm => {
            let isolation = Arc::new(
                IsolationManager::new_with_host_api_url(
                    IsolationConfig {
                        mode: IsolationMode::MicroVm,
                        fallback: None,
                        base_image: String::new(),
                        resources: ResourceConfig {
                            cpus: config.cpus_per_vm,
                            memory_gb: (config.mem_mib_per_vm / 1024).max(1),
                        },
                    },
                    Some(config.api_url.clone()),
                )
                .await?,
            );
            if isolation.isolation_level() < IsolationLevel::Vm {
                return Err(G5Error::Config(
                    "g5d host requires the Cloud Hypervisor microVM backend".to_string(),
                ));
            }
            Arc::new(IsolationVmLauncher::new(isolation))
        }
        // No isolation manager is constructed here, so its environment
        // re-read cannot revisit the decision made once above.
        HostIsolation::Process => Arc::new(ProcessVmLauncher::new(config.state_dir.clone())),
    };

    let token = Arc::new(RwLock::new(config.host_jwt.clone()));
    let allow_hosts = env_non_empty("G5D_GIT_HTTP_ALLOW_HOSTS")
        .map(|raw| workspace::parse_http_allow_hosts(&raw))
        .unwrap_or_default();
    let cache = RepoCache::new(config.cache_root.clone())
        .with_git_auth(token.clone())
        .with_http_allow_hosts(allow_hosts);
    // A process-mode host may sit on a filesystem without reflink, such as
    // ext4; it copies the bare cache there.
    // MicroVM hosts keep reflink-or-fail-loud.
    let checkout = Arc::new(
        WorkspaceCheckout::new(cache, config.workspaces_root.clone())
            .with_copy_when_reflink_unsupported(config.isolation == HostIsolation::Process),
    );
    let journal = Arc::new(
        EventJournal::open(
            config
                .state_dir
                .join("host-control")
                .join(format!("{}.json", config.host_id)),
        )
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("failed to open host event journal: {error:#}"),
        })?,
    );
    let runtime_root = config
        .runtime_bin_dir
        .parent()
        .unwrap_or(&config.runtime_bin_dir)
        .to_path_buf();
    let lease_renewals = Arc::new(g5d_host::ctl::LeaseRenewalBroker::default());
    let runtime = Arc::new(WorkspaceVmRuntime::new(
        WorkspaceVmRuntimeConfig {
            org_id: config.org_id,
            slots_total: config.slots_total,
            cpus_per_vm: config.cpus_per_vm,
            mem_mib_per_vm: config.mem_mib_per_vm,
            scratch_disk_mib: config.scratch_disk_mib,
            block_workload: config.block_workload,
            nested_virtualization: config.nested_virtualization,
            api_url: config.api_url.clone(),
            runtime_root,
            workspaces_root: config.workspaces_root.clone(),
            dispatcher_ready_timeout: config.dispatcher_ready_timeout,
            process_host_executable: match config.isolation {
                HostIsolation::MicroVm => None,
                HostIsolation::Process => Some(executable.clone()),
            },
        },
        launcher,
        checkout,
        journal.clone(),
        lease_renewals.clone(),
        Arc::new(NoOtherSlotUsage),
    ));
    // Incarnations a previous host left running (process mode, in either
    // direction of a mode flip) are settled with exit proof before any root
    // is reclaimed or any work admitted (`g5.runtime.custody`).
    let settled = g5d_host::runtime::settle_process_incarnations(&config.state_dir)
        .await
        .map_err(|error| {
            G5Error::Config(format!(
                "a previous host's process-mode incarnation is still alive or its exit cannot be proven: {error:#}"
            ))
        })?;
    if settled > 0 {
        info!(host_id = %config.host_id, settled, "settled process-mode incarnations left by a previous host");
    }
    runtime.release_unheld_roots().await;
    let state = Arc::new(WorkspaceHostState::new(
        config.host_id,
        config.slots_total,
        config.workspaces_root.clone(),
        runtime.clone(),
    ));
    let commands =
        Arc::new(HostCommandProcessor::default().with_workspace_runtime(runtime.clone()));
    let carrier = HostTunnelCarrier::new(HostTunnelConfig {
        api_url: config.api_url.clone(),
        host_id: config.host_id,
        host_jwt: token,
    })
    .with_control(ControlSessionConfig {
        state,
        journal,
        commands,
        workspace_runtime: runtime.clone(),
        lease_renewals,
    });

    info!(
        host_id = %config.host_id,
        org_id = %config.org_id,
        slots_total = config.slots_total,
        isolation = ?config.isolation,
        nested_virtualization = config.nested_virtualization,
        "workspace VM host started"
    );
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    // Cache door (ontology g5.runtime.cache.door): on only when bring-up
    // configured a port. It is never required for correctness, so a door
    // that cannot start is reported and the host keeps serving without it.
    let door_task = match g5d_host::cache_door::CacheDoorConfig::from_env(seed_templates) {
        Ok(Some(_)) if config.isolation == HostIsolation::Process => {
            warn!(host_id = %config.host_id, "process mode: cache door skipped (not applicable; no guests to serve)");
            None
        }
        Ok(Some(door_config)) => {
            match g5d_host::cache_door::CacheDoor::from_config(door_config).await {
                Ok(door) => Some(tokio::spawn(g5d_host::cache_door::serve(
                    door,
                    shutdown_rx.clone(),
                ))),
                Err(error) => {
                    warn!(host_id = %config.host_id, %error, "cache door not started");
                    None
                }
            }
        }
        Ok(None) => None,
        Err(error) => {
            warn!(host_id = %config.host_id, %error, "cache door configuration rejected; door not started");
            None
        }
    };
    let carrier_task = tokio::spawn(carrier.run(shutdown_rx));
    shutdown_signal().await.map_err(|error| G5Error::Internal {
        message: format!("failed to receive shutdown signal: {error}"),
    })?;
    let _ = shutdown_tx.send(true);
    runtime.shutdown().await;
    let _ = carrier_task.await;
    if let Some(door_task) = door_task {
        match door_task.await {
            Ok(Err(error)) => {
                warn!(host_id = %config.host_id, %error, "cache door exited with error")
            }
            Err(error) => warn!(host_id = %config.host_id, %error, "cache door task failed"),
            Ok(Ok(())) => {}
        }
    }
    info!(host_id = %config.host_id, "workspace VM host stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use g5d_foundation::oci::registry::{Descriptor, Manifest};
    use g5d_foundation::oci::{ImageConfig, ImageRef, LayerCache, OciError, RegistryClient};
    use tokio::sync::Notify;

    fn fixture_config(args: &HostArgs, global: &GlobalFlags) -> Result<HostConfig, G5Error> {
        build_config_with_hardware(
            args,
            global,
            resources::HostHardware {
                cpus: 8,
                ram_mib: 16_384,
            },
        )
    }

    struct BlockedRegistry {
        requested: Arc<Notify>,
        release: Arc<Notify>,
    }

    #[async_trait::async_trait]
    impl RegistryClient for BlockedRegistry {
        async fn fetch_manifest(&self, _: &ImageRef) -> Result<Manifest, OciError> {
            self.requested.notify_one();
            self.release.notified().await;
            Err(OciError::ManifestDecode("unavailable test image".into()))
        }

        async fn fetch_config(
            &self,
            _: &ImageRef,
            _: &Descriptor,
        ) -> Result<ImageConfig, OciError> {
            unreachable!("manifest failed")
        }

        async fn fetch_layer_blob(
            &self,
            _: &ImageRef,
            _: &Descriptor,
            _: &std::path::Path,
        ) -> Result<(), OciError> {
            unreachable!("manifest failed")
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn host_does_not_connect_while_image_preparation_is_pending_or_failed() {
        assert_preparation_blocks_registration(false).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn host_does_not_connect_when_image_preparation_times_out() {
        assert_preparation_blocks_registration(true).await;
    }

    async fn assert_preparation_blocks_registration(expire: bool) {
        let api = wiremock::MockServer::start().await;
        let temp = tempfile::tempdir().unwrap();
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = host_jwt(host_id, org_id);
        let runtime_bin = temp.path().join("runtime/bin");
        std::fs::create_dir_all(&runtime_bin).unwrap();
        std::fs::write(runtime_bin.join("g5d"), b"guest binary").unwrap();
        let image = format!("ghcr.io/example/workspace@sha256:{}", "a".repeat(64));
        let cli = crate::cli::Cli::try_parse_from([
            "g5d",
            "--api-url",
            &api.uri(),
            "host",
            "--max-microvms",
            "1",
            "--host-id",
            &host_id.to_string(),
            "--org-id",
            &org_id.to_string(),
            "--host-jwt",
            &token,
            "--base-image",
            &image,
            "--state-dir",
            temp.path().join("state").to_str().unwrap(),
            "--workspaces-root",
            temp.path().join("workspaces").to_str().unwrap(),
            "--runtime-bin-dir",
            runtime_bin.to_str().unwrap(),
            "--skip-layout-check",
        ])
        .unwrap();
        let crate::cli::Command::Host(args) = cli.command else {
            panic!("host command")
        };
        let config = fixture_config(&args, &cli.global).unwrap();
        assert_eq!(config.base_image.as_deref(), Some(image.as_str()));
        let requested = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let extractor = OciExtractor::with_components(
            LayerCache::new(temp.path().join("cache")),
            Box::new(BlockedRegistry {
                requested: requested.clone(),
                release: release.clone(),
            }),
        );
        let host =
            tokio::spawn(async move { execute_with_extractor(&args, config, extractor).await });
        tokio::time::timeout(Duration::from_secs(5), requested.notified())
            .await
            .expect("startup must prepare the configured image");
        assert!(!host.is_finished());
        assert!(
            api.received_requests().await.unwrap().is_empty(),
            "preparing host must not register"
        );
        if expire {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(601)).await;
        } else {
            release.notify_one();
        }
        let error = tokio::time::timeout(Duration::from_secs(5), host)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(
            error.to_string().contains(if expire {
                "host workspace image preparation timed out"
            } else {
                "host workspace image preparation failed"
            }),
            "{error}"
        );
        assert!(
            api.received_requests().await.unwrap().is_empty(),
            "failed host must not register"
        );
    }

    #[test]
    #[serial_test::serial]
    fn host_preparation_image_is_read_from_bootstrap_environment() {
        let previous = std::env::var_os("G5D_HOST_BASE_IMAGE");
        let image = format!(
            "us-central1-docker.pkg.dev/test/worker/base@sha256:{}",
            "b".repeat(64)
        );
        std::env::set_var("G5D_HOST_BASE_IMAGE", &image);
        let parsed = crate::cli::Cli::try_parse_from(["g5d", "host"]);
        match previous {
            Some(value) => std::env::set_var("G5D_HOST_BASE_IMAGE", value),
            None => std::env::remove_var("G5D_HOST_BASE_IMAGE"),
        }
        let crate::cli::Command::Host(args) = parsed.unwrap().command else {
            panic!("host command")
        };
        assert_eq!(args.base_image.as_deref(), Some(image.as_str()));
    }

    #[test]
    #[serial_test::serial]
    fn invalid_preparation_image_fails_configuration_before_startup() {
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        for image in ["  ", "/nonexistent/rootfs"] {
            let cli = crate::cli::Cli::try_parse_from([
                "g5d",
                "host",
                "--max-microvms",
                "1",
                "--host-jwt",
                &host_jwt(host_id, org_id),
                "--host-id",
                &host_id.to_string(),
                "--org-id",
                &org_id.to_string(),
                "--base-image",
                image,
            ])
            .unwrap();
            let crate::cli::Command::Host(args) = cli.command else {
                panic!("host command")
            };
            let error = fixture_config(&args, &cli.global).unwrap_err();
            assert!(matches!(error, G5Error::Config(_)));
            assert!(error.to_string().contains("invalid host workspace image"));
        }
    }

    fn host_jwt(host_id: Uuid, org_id: Uuid) -> String {
        let claims = serde_json::json!({"sub": host_id, "org_id": org_id});
        format!(
            "x.{}.x",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        )
    }

    #[test]
    fn workload_root_defaults_to_block_and_refuses_removed_backend() {
        assert!(resolve_block_workload(None).unwrap());
        assert!(resolve_block_workload(Some("virtiofs")).is_err());
        assert!(resolve_block_workload(Some("block")).unwrap());
        for value in ["", "BLOCK", " block ", "ext4", "blok"] {
            let error = resolve_block_workload(Some(value)).unwrap_err();
            assert!(
                error.to_string().contains(WORKLOAD_ROOT_ENV),
                "{value}: {error}"
            );
        }
    }

    #[test]
    fn nested_virtualization_switch_is_explicit_and_refuses_typos() {
        assert!(!resolve_nested_virtualization(None).unwrap());
        assert!(!resolve_nested_virtualization(Some("off")).unwrap());
        assert!(!resolve_nested_virtualization(Some("")).unwrap());
        assert!(resolve_nested_virtualization(Some(" on ")).unwrap());
        for value in ["1", "true", "ON", "yes", "nested"] {
            let error = resolve_nested_virtualization(Some(value)).unwrap_err();
            assert!(
                error.to_string().contains(NESTED_VIRTUALIZATION_ENV),
                "{value}: {error}"
            );
        }
    }

    #[test]
    fn nested_virtualization_requires_linux_microvm_and_a_kvm_kernel() {
        validate_nested_virtualization(true, HostIsolation::MicroVm, true, || Ok(true)).unwrap();
        for (isolation, linux, kvm, message) in [
            (HostIsolation::Process, true, true, "Linux microVM"),
            (HostIsolation::MicroVm, false, true, "Linux microVM"),
            (HostIsolation::MicroVm, true, false, "CONFIG_KVM=y"),
        ] {
            let error =
                validate_nested_virtualization(true, isolation, linux, || Ok(kvm)).unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
            // The switch off never consults the kernel or the mode.
            validate_nested_virtualization(false, isolation, linux, || {
                panic!("kernel config read with the switch off")
            })
            .unwrap();
        }
        // An unreadable kernel configuration is reported, not treated as off.
        let error = validate_nested_virtualization(true, HostIsolation::MicroVm, true, || {
            Err(G5Error::Config("platform kernel config: missing".into()))
        })
        .unwrap_err();
        assert!(error.to_string().contains("missing"), "{error}");
    }

    #[test]
    fn block_workload_requires_linux_microvm_and_capacity() {
        validate_block_workload(true, HostIsolation::MicroVm, true, 65_536).unwrap();
        for (isolation, linux, capacity, message) in [
            (HostIsolation::Process, true, 65_536, "Linux microVM"),
            (HostIsolation::MicroVm, false, 65_536, "Linux microVM"),
            (HostIsolation::MicroVm, true, 0, "positive"),
        ] {
            let error = validate_block_workload(true, isolation, linux, capacity).unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
            // Existing process/macOS/zero-scratch callers retain their mode.
            validate_block_workload(false, isolation, linux, capacity).unwrap();
        }
    }

    #[test]
    fn block_workload_capacity_is_one_published_budget_without_invalid_fallback() {
        for block in [false, true] {
            assert_eq!(
                resolve_scratch_disk_mib(Some("65536"), block).unwrap(),
                65_536
            );
            assert_eq!(
                resolve_scratch_disk_mib(None, block).unwrap(),
                DEFAULT_WORKER_HOST_SCRATCH_DISK_MIB as u32
            );
        }
        for value in ["0", "-1", "invalid", "", "1073741825"] {
            assert!(
                resolve_scratch_disk_mib(Some(value), true).is_err(),
                "{value}"
            );
            assert_eq!(
                resolve_scratch_disk_mib(Some(value), false).unwrap(),
                DEFAULT_WORKER_HOST_SCRATCH_DISK_MIB as u32
            );
        }
    }

    #[test]
    fn block_workload_preparation_shares_the_bounded_dispatcher_ready_window() {
        assert_eq!(
            resolve_dispatcher_ready_timeout(None, false),
            Duration::from_secs(30)
        );
        assert_eq!(
            resolve_dispatcher_ready_timeout(None, true),
            Duration::from_secs(120)
        );
        for block in [false, true] {
            assert_eq!(
                resolve_dispatcher_ready_timeout(Some("90"), block),
                Duration::from_secs(90)
            );
            assert_eq!(
                resolve_dispatcher_ready_timeout(Some("0"), block),
                Duration::from_secs(1)
            );
            assert_eq!(
                resolve_dispatcher_ready_timeout(Some("invalid"), block),
                resolve_dispatcher_ready_timeout(None, block)
            );
        }
    }

    /// `g5.runtime.boot.backend.process-local`: process mode needs the explicit
    /// mode and the acknowledgement; the acknowledgement alone selects nothing,
    /// an unset or `microvm` mode keeps the VM requirement, and a typo is an
    /// error rather than a silently ignored value.
    #[test]
    fn process_isolation_requires_the_explicit_mode_and_the_acknowledgement() {
        let resolve = |vars: &[(&str, &str)]| {
            resolve_host_isolation(|key| {
                vars.iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| value.to_string())
            })
        };
        assert_eq!(resolve(&[]).unwrap(), HostIsolation::MicroVm);
        assert_eq!(
            resolve(&[(ISOLATION_MODE_ENV, "microvm")]).unwrap(),
            HostIsolation::MicroVm
        );
        assert_eq!(
            resolve(&[(ALLOW_PROCESS_ISOLATION_ENV, "1")]).unwrap(),
            HostIsolation::MicroVm
        );
        let refused = resolve(&[(ISOLATION_MODE_ENV, "process")]).unwrap_err();
        assert!(matches!(refused, G5Error::Config(_)));
        assert!(refused.to_string().contains(ALLOW_PROCESS_ISOLATION_ENV));
        let refused = resolve(&[
            (ISOLATION_MODE_ENV, "process"),
            (ALLOW_PROCESS_ISOLATION_ENV, "yes"),
        ])
        .unwrap_err();
        assert!(refused.to_string().contains(ALLOW_PROCESS_ISOLATION_ENV));
        for ack in ["1", "true", " TRUE "] {
            assert_eq!(
                resolve(&[
                    (ISOLATION_MODE_ENV, "process"),
                    (ALLOW_PROCESS_ISOLATION_ENV, ack),
                ])
                .unwrap(),
                HostIsolation::Process,
                "{ack}"
            );
        }
        let typo = resolve(&[(ISOLATION_MODE_ENV, "porcess")]).unwrap_err();
        assert!(typo.to_string().contains("unrecognized"));
    }

    /// The process-mode runtime directory is validated with the bundle's
    /// fail-closed shape: every tool, the pinned catalog and the harness must
    /// be present, and its `g5d` must be this very executable.
    #[test]
    fn process_layout_check_fails_closed_on_an_incomplete_host_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let runtime_bin = temp.path().join("runtime/bin");
        std::fs::create_dir_all(&runtime_bin).unwrap();
        let config = HostConfig {
            host_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            api_url: "http://localhost:7500".into(),
            host_jwt: String::new(),
            base_image: None,
            isolation: HostIsolation::Process,
            block_workload: false,
            nested_virtualization: false,
            slots_total: 1,
            cpus_per_vm: 2,
            mem_mib_per_vm: 4096,
            scratch_disk_mib: 10240,
            runtime_bin_dir: runtime_bin.clone(),
            state_dir: temp.path().join("state"),
            cache_root: temp.path().join("cache"),
            workspaces_root: temp.path().join("workspaces"),
            dispatcher_ready_timeout: Duration::from_secs(30),
        };
        let executable = std::env::current_exe().unwrap();
        let error = verify_process_layout(&config, &executable, false).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("host-native runtime tool is absent"),
            "{error}"
        );
        assert!(error.to_string().contains("g5d"), "{error}");

        // Stage every tool as the staging script does: the host build for
        // g5d, the host's own binaries for the rest.
        std::os::unix::fs::symlink(&executable, runtime_bin.join("g5d")).unwrap();
        for tool in ["claude", "codex", "node", "git", "docker"] {
            std::fs::write(runtime_bin.join(tool), b"#!/bin/sh\n").unwrap();
        }
        let error = verify_process_layout(&config, &executable, false).unwrap_err();
        assert!(
            error.to_string().contains("modules/catalog.json"),
            "{error}"
        );
        let runtime_root = temp.path().join("runtime");
        std::fs::create_dir_all(runtime_root.join("modules")).unwrap();
        std::fs::write(runtime_root.join("modules/catalog.json"), b"{}").unwrap();
        std::fs::write(runtime_root.join("modules/catalog.sha256"), b"x").unwrap();
        let error = verify_process_layout(&config, &executable, false).unwrap_err();
        assert!(
            error.to_string().contains("assets/harness/AGENTS.md"),
            "{error}"
        );
        std::fs::create_dir_all(runtime_root.join("assets/harness")).unwrap();
        std::fs::write(runtime_root.join("assets/harness/AGENTS.md"), b"# harness").unwrap();
        verify_process_layout(&config, &executable, false).expect("complete host runtime");

        // The fake provider CLIs are required only for a deterministic lane.
        let error = verify_process_layout(&config, &executable, true).unwrap_err();
        assert!(error.to_string().contains("fake-claude"), "{error}");
        for tool in ["fake-claude", "fake-codex", "fake-g5-tools"] {
            std::fs::write(runtime_bin.join(tool), b"#!/bin/sh\n").unwrap();
        }
        verify_process_layout(&config, &executable, true).expect("deterministic lane runtime");
        assert!(deterministic_providers_selected(|key| {
            (key == "G5_CLAUDE_CODE_FAKE").then(|| "1".to_string())
        }));
        assert!(!deterministic_providers_selected(|key| {
            (key == "G5_CLAUDE_CODE_FAKE").then(|| "0".to_string())
        }));
        assert!(!deterministic_providers_selected(|_| None));

        // A runtime g5d that is another binary cannot be the dispatcher.
        std::fs::remove_file(runtime_bin.join("g5d")).unwrap();
        std::fs::write(runtime_bin.join("g5d"), b"#!/bin/sh\n").unwrap();
        let error = verify_process_layout(&config, &executable, false).unwrap_err();
        assert!(error.to_string().contains("must resolve to"), "{error}");
        // A missing tool other than g5d is named.
        std::fs::remove_file(runtime_bin.join("g5d")).unwrap();
        std::os::unix::fs::symlink(&executable, runtime_bin.join("g5d")).unwrap();
        std::fs::remove_file(runtime_bin.join("docker")).unwrap();
        let error = verify_process_layout(&config, &executable, false).unwrap_err();
        assert!(error.to_string().contains("docker"), "{error}");
    }

    /// `g5.runtime.boot.backend`: with `microvm` configured (the default) and no
    /// Cloud Hypervisor backend reachable, the host fails closed instead of degrading to
    /// a process launcher, acknowledgement or not.
    #[tokio::test]
    #[serial_test::serial]
    async fn microvm_mode_without_cloud_hypervisor_fails_closed_instead_of_degrading() {
        let api = wiremock::MockServer::start().await;
        let temp = tempfile::tempdir().unwrap();
        let runtime_bin = temp.path().join("runtime/bin");
        std::fs::create_dir_all(&runtime_bin).unwrap();
        // A separate guest copy, so the SEC-C01 separation check passes and
        // the backend selection is what decides.
        std::fs::write(runtime_bin.join("g5d"), b"guest binary").unwrap();
        let previous = std::env::var_os(ALLOW_PROCESS_ISOLATION_ENV);
        std::env::set_var(ALLOW_PROCESS_ISOLATION_ENV, "1");
        std::env::remove_var(ISOLATION_MODE_ENV);
        let cli = crate::cli::Cli::try_parse_from([
            "g5d",
            "--api-url",
            &api.uri(),
            "host",
            "--host-jwt",
            &host_jwt(Uuid::new_v4(), Uuid::new_v4()),
            "--runtime-bin-dir",
            runtime_bin.to_str().unwrap(),
            "--state-dir",
            temp.path().join("state").to_str().unwrap(),
            "--workspaces-root",
            temp.path().join("workspaces").to_str().unwrap(),
            "--skip-layout-check",
        ])
        .unwrap();
        let crate::cli::Command::Host(args) = cli.command else {
            panic!("host command")
        };
        let config = fixture_config(&args, &cli.global).unwrap();
        match previous {
            Some(value) => std::env::set_var(ALLOW_PROCESS_ISOLATION_ENV, value),
            None => std::env::remove_var(ALLOW_PROCESS_ISOLATION_ENV),
        }
        assert_eq!(config.isolation, HostIsolation::MicroVm);
        assert_eq!(
            config.slots_total, 4,
            "the microVM default slot count is unchanged"
        );
        let host = tokio::spawn(async move {
            execute_with_extractor(&args, config, OciExtractor::from_env()).await
        });
        match tokio::time::timeout(Duration::from_secs(10), host).await {
            Ok(result) => {
                let error = result.unwrap().unwrap_err();
                assert!(
                    error.to_string().contains("No isolation backend available")
                        || error.to_string().contains("Cloud Hypervisor"),
                    "{error}"
                );
                assert!(
                    api.received_requests().await.unwrap().is_empty(),
                    "a host without its configured backend must not register"
                );
            }
            Err(_) => {
                // A machine with a working Cloud Hypervisor backend started the host;
                // the fail-closed path is not observable here.
                eprintln!("SKIP: Cloud Hypervisor backend is available on this machine");
            }
        }
    }

    /// `g5.runtime.boot.backend.process-local`: a process host defaults to one
    /// slot.
    #[test]
    #[serial_test::serial]
    fn process_mode_defaults_to_one_slot() {
        let temp = tempfile::tempdir().unwrap();
        let runtime_bin = temp.path().join("runtime/bin");
        std::fs::create_dir_all(&runtime_bin).unwrap();
        let previous = (
            std::env::var_os(ISOLATION_MODE_ENV),
            std::env::var_os(ALLOW_PROCESS_ISOLATION_ENV),
        );
        std::env::set_var(ISOLATION_MODE_ENV, "process");
        std::env::set_var(ALLOW_PROCESS_ISOLATION_ENV, "1");
        let cli = crate::cli::Cli::try_parse_from([
            "g5d",
            "--api-url",
            "http://127.0.0.1:9",
            "host",
            "--host-jwt",
            &host_jwt(Uuid::new_v4(), Uuid::new_v4()),
            "--base-image",
            &format!("ghcr.io/example/workspace@sha256:{}", "a".repeat(64)),
            "--runtime-bin-dir",
            runtime_bin.to_str().unwrap(),
            "--state-dir",
            temp.path().join("state").to_str().unwrap(),
            "--workspaces-root",
            temp.path().join("workspaces").to_str().unwrap(),
            "--skip-layout-check",
        ])
        .unwrap();
        let crate::cli::Command::Host(args) = cli.command else {
            panic!("host command")
        };
        let config = fixture_config(&args, &cli.global);
        for (key, value) in [
            (ISOLATION_MODE_ENV, previous.0),
            (ALLOW_PROCESS_ISOLATION_ENV, previous.1),
        ] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        let config = config.expect("process mode config");
        assert_eq!(config.isolation, HostIsolation::Process);
        assert_eq!(config.slots_total, 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    #[serial_test::serial]
    async fn host_rejects_shared_runtime_before_registration_even_with_layout_checks_skipped() {
        let api = wiremock::MockServer::start().await;
        let temp = tempfile::tempdir().unwrap();
        let runtime_bin = temp.path().join("runtime/bin");
        std::fs::create_dir_all(&runtime_bin).unwrap();
        std::os::unix::fs::symlink(std::env::current_exe().unwrap(), runtime_bin.join("g5d"))
            .unwrap();
        let cli = crate::cli::Cli::try_parse_from([
            "g5d",
            "--api-url",
            &api.uri(),
            "host",
            "--max-microvms",
            "1",
            "--host-jwt",
            &host_jwt(Uuid::new_v4(), Uuid::new_v4()),
            "--runtime-bin-dir",
            runtime_bin.to_str().unwrap(),
            "--state-dir",
            temp.path().join("state").to_str().unwrap(),
            "--workspaces-root",
            temp.path().join("workspaces").to_str().unwrap(),
            "--skip-layout-check",
        ])
        .unwrap();
        let crate::cli::Command::Host(args) = cli.command else {
            panic!("host command")
        };
        let config = fixture_config(&args, &cli.global).unwrap();
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            execute_with_extractor(&args, config, OciExtractor::from_env()),
        )
        .await
        .expect("unsafe runtime must be rejected before starting the host")
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("guest runtime must be a separate bundle"),
            "{error}"
        );
        assert!(api.received_requests().await.unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_separation_rejects_shared_directory_symlinks_and_hardlinks() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("guest");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let host = temp.path().join("host-g5d");
        let guest = root.join("bin/g5d");
        std::fs::write(&host, b"binary").unwrap();
        std::fs::write(&guest, b"binary").unwrap();
        verify_runtime_separation(&root, &host, &[]).unwrap();
        let private_libs = temp.path().join("host-libs");
        let guest_libs = root.join("libs");
        std::fs::create_dir(&private_libs).unwrap();
        std::fs::create_dir(&guest_libs).unwrap();
        verify_runtime_separation(&root, &host, std::slice::from_ref(&private_libs)).unwrap();
        assert!(
            verify_runtime_separation(&root, &host, std::slice::from_ref(&guest_libs)).is_err()
        );
        let alias = temp.path().join("guest-libs-alias");
        std::os::unix::fs::symlink(&guest_libs, &alias).unwrap();
        assert!(verify_runtime_separation(&root, &host, &[private_libs, alias]).is_err());
        assert!(verify_runtime_separation(&root, &guest, &[]).is_err());
        std::fs::remove_file(&guest).unwrap();
        std::os::unix::fs::symlink(&host, &guest).unwrap();
        assert!(verify_runtime_separation(&root, &host, &[]).is_err());
        std::fs::remove_file(&guest).unwrap();
        std::fs::hard_link(&host, &guest).unwrap();
        assert!(verify_runtime_separation(&root, &host, &[]).is_err());
    }

    #[test]
    fn jwt_identity_must_match_cli_identity() {
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = host_jwt(host_id, org_id);
        let args = HostArgs {
            host_id: Some(Uuid::new_v4().to_string()),
            org_id: None,
            host_jwt: Some(token),
            base_image: None,
            max_microvms: None,
            runtime_bin_dir: None,
            state_dir: None,
            cache_root: None,
            workspaces_root: None,
            cpus_per_vm: None,
            mem_mib_per_vm: None,
            skip_layout_check: false,
        };
        let error = fixture_config(
            &args,
            &GlobalFlags {
                workspace: None,
                verbose: 0,
                quiet: false,
                json: false,
                api_url: None,
            },
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("--host-id does not match the host JWT"));
    }

    #[test]
    #[serial_test::serial]
    fn omitted_slot_overrides_derive_measured_resources() {
        let prior_scratch = std::env::var_os("G5D_SCRATCH_DISK_MIB");
        std::env::remove_var("G5D_SCRATCH_DISK_MIB");

        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let config = build_config_with_hardware(
            &HostArgs {
                host_id: None,
                org_id: None,
                host_jwt: Some(host_jwt(host_id, org_id)),
                base_image: None,
                max_microvms: Some(6),
                runtime_bin_dir: None,
                state_dir: None,
                cache_root: None,
                workspaces_root: None,
                cpus_per_vm: None,
                mem_mib_per_vm: None,
                skip_layout_check: false,
            },
            &GlobalFlags {
                workspace: None,
                verbose: 0,
                quiet: false,
                json: false,
                api_url: None,
            },
            resources::HostHardware {
                cpus: 22,
                ram_mib: 88_490,
            },
        )
        .expect("default host config");

        if let Some(value) = prior_scratch {
            std::env::set_var("G5D_SCRATCH_DISK_MIB", value);
        }
        assert_eq!(config.cpus_per_vm, 3);
        assert_eq!(config.mem_mib_per_vm, 13_273);
        assert_eq!(
            config.scratch_disk_mib,
            DEFAULT_WORKER_HOST_SCRATCH_DISK_MIB as u32
        );
    }
}
