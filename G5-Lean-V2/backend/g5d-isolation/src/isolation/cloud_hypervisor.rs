//! Platform-owned VM supervisor. Workload, runtime and bootstrap input are separate
//! block devices; no guest-written filesystem is ever mounted on the host.
use super::{platform_command, VmExecArgs};
use g5_contracts::G5Error;
use serde::{Deserialize, Serialize};
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub const VMM_BIN_ENV: &str = "G5D_CLOUD_HYPERVISOR_BIN";
pub const KERNEL_ENV: &str = "G5D_GUEST_KERNEL";
pub const INITRAMFS_ENV: &str = "G5D_GUEST_INITRAMFS";
pub const GUEST_ENV: &str = "G5D_INTERNAL_GUEST_ENV";
pub const RESTORE_INPUT: &str = "G5D_INTERNAL_GUEST_RESTORE";
/// The host's cache profile (ontology `g5.runtime.cache.profile`): published by
/// host bring-up beside the runtime disk, read by guests at `/opt/g5/<name>`.
pub const CACHE_PROFILE_NAME: &str = "cache-profile.env";
/// Non-secret host configuration of a few lines; anything larger is not a profile.
const CACHE_PROFILE_LIMIT: u64 = 64 * 1024;

/// Stage the host's cache profile from the runtime directory into this VM's
/// bootstrap input, where the trusted guest init binds it over the runtime
/// disk's empty placeholder before any consumer runs. No file means the host
/// has no door: nothing is staged and the guest keeps the placeholder, which
/// reads as no profile. A present file that is not a bounded regular file is a
/// host misconfiguration and fails the boot instead of silently losing the cache.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn stage_cache_profile(runtime: &Path, input: &Path) -> anyhow::Result<bool> {
    use anyhow::Context;
    use std::{
        io::{Read, Write},
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    let source = runtime.join(CACHE_PROFILE_NAME);
    let metadata = match std::fs::symlink_metadata(&source) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).context("host cache profile"),
    };
    anyhow::ensure!(
        metadata.is_file(),
        "host cache profile must be a regular file"
    );
    anyhow::ensure!(
        metadata.len() <= CACHE_PROFILE_LIMIT,
        "host cache profile exceeds {CACHE_PROFILE_LIMIT} bytes"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(&source)?
        .take(CACHE_PROFILE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= CACHE_PROFILE_LIMIT,
        "host cache profile exceeds {CACHE_PROFILE_LIMIT} bytes"
    );
    // World-readable: the guest's unprivileged tool users read it as data. The
    // creation mode is subject to the service's umask, so set it explicitly.
    let mut staged = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(input.join(CACHE_PROFILE_NAME))?;
    staged.write_all(&bytes)?;
    staged.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    Ok(true)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn guest_host_aliases(entries: &[String]) -> anyhow::Result<Vec<(String, std::net::Ipv4Addr)>> {
    entries
        .iter()
        .map(|entry| {
            let (host, ip) = entry
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("invalid guest alias"))?;
            anyhow::ensure!(
                !host.is_empty()
                    && host
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'),
                "invalid guest hostname"
            );
            let ip: std::net::Ipv4Addr = ip.parse()?;
            Ok((
                host.into(),
                if ip.is_loopback() {
                    super::passt::HOST_DOOR_ADDR
                } else {
                    ip
                },
            ))
        })
        .collect()
}

/// Conservative GNU-tar bound without reading contents or following symlinks.
/// The actual writer is capped by the bound, so races or unusual headers fail
/// before they can consume the full host disk.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn workspace_archive_bound(work: &Path, limit: u64) -> anyhow::Result<u64> {
    let mut pending = vec![work.to_owned()];
    let mut bytes = 20 * 512u64; // tar's final blocking record
    while let Some(path) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path)?;
        let content = if metadata.is_file() {
            metadata
                .len()
                .checked_add(511)
                .and_then(|n| (n / 512).checked_mul(512))
                .ok_or_else(|| anyhow::anyhow!("workspace archive size overflow"))?
        } else {
            0
        };
        // GNU long-name/link records each need a header and rounded payload.
        // Count them even for short names; hard links count as full copies.
        let name_len = path.strip_prefix(work)?.as_os_str().as_bytes().len() as u64 + 3;
        let link_len = if metadata.is_symlink() {
            std::fs::read_link(&path)?.as_os_str().as_bytes().len() as u64 + 1
        } else if metadata.is_file() && metadata.nlink() > 1 {
            // GNU tar may encode a hard link to an earlier, longer path.
            // Linux's maximum resolvable path bounds that record even when
            // the current hard-link name is short.
            libc::PATH_MAX as u64 + 1
        } else {
            0
        };
        let headers = 3 * 512 + name_len.div_ceil(512) * 512 + link_len.div_ceil(512) * 512;
        bytes = bytes
            .checked_add(headers)
            .and_then(|n| n.checked_add(content))
            .ok_or_else(|| anyhow::anyhow!("workspace archive size overflow"))?;
        anyhow::ensure!(
            bytes <= limit,
            "workspace import exceeds its admitted archive budget"
        );
        if metadata.is_dir() {
            for child in std::fs::read_dir(path)? {
                pending.push(child?.path());
            }
        }
    }
    Ok(bytes)
}

/// Only the host supervisor writes this file. Never derive these fields from image
/// contents; it can contain credentials and must not implement Debug or enter logs.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestBootConfig {
    pub version: u32,
    pub env: BTreeMap<String, String>,
    pub command: Vec<String>,
    pub workdir: String,
    pub host_aliases: Vec<(String, std::net::Ipv4Addr)>,
    pub import_workspace: bool,
    pub restore: Option<g5_internal_api::host_ctl::VmStart>,
}

fn artifact(name: &str, default: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| default.into())
}
pub fn vmm_binary() -> PathBuf {
    artifact(VMM_BIN_ENV, "/opt/g5-host/bin/cloud-hypervisor")
}
pub fn kernel() -> PathBuf {
    artifact(KERNEL_ENV, "/opt/g5-host/boot/vmlinux")
}
pub fn initramfs() -> PathBuf {
    artifact(INITRAMFS_ENV, "/opt/g5-host/boot/initramfs.cpio")
}
pub fn available() -> bool {
    cfg!(target_os = "linux")
        && super::detect::check_kvm()
        && [vmm_binary(), kernel(), initramfs()]
            .iter()
            .all(|p| p.is_file())
}

/// The platform kernel's build configuration, installed beside the pinned
/// kernel by `scripts/build-cloud-hypervisor-runtime.sh`.
pub fn platform_kernel_config() -> PathBuf {
    kernel().with_file_name("kernel.config")
}

/// The `--cpus` value: `nested=on` exposes hardware virtualization to the guest
/// (`g5.architecture.microvm_backend.nested`); omitted, Cloud Hypervisor v54
/// keeps nested off.
#[must_use]
pub fn cpus_argument(cpus: u8, nested_virtualization: bool) -> String {
    if nested_virtualization {
        format!("boot={cpus},nested=on")
    } else {
        format!("boot={cpus}")
    }
}

/// Whether a kernel build configuration has KVM built in. Nested
/// virtualization without it gives the guest VMX but no `/dev/kvm`, so hosts
/// and `vm-exec` refuse that combination instead of booting a misleading guest.
pub fn kernel_config_has_kvm(path: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("platform kernel config {}: {error}", path.display()))?;
    Ok(text.lines().any(|line| line.trim() == "CONFIG_KVM=y"))
}

pub fn platform_kernel_has_kvm() -> Result<bool, String> {
    kernel_config_has_kvm(&platform_kernel_config())
}

#[derive(Serialize)]
pub struct VmmVersion {
    pub backend: String,
    pub version: String,
    pub kernel_sha256: String,
}
impl VmmVersion {
    pub fn probe() -> Result<Self, G5Error> {
        use sha2::{Digest, Sha256};
        let out = platform_command(vmm_binary())
            .arg("--version")
            .output()
            .map_err(|e| G5Error::Config(format!("Cloud Hypervisor version probe: {e}")))?;
        if !out.status.success() {
            return Err(G5Error::Config(
                "Cloud Hypervisor version probe failed".into(),
            ));
        }
        let bytes = std::fs::read(kernel())
            .map_err(|e| G5Error::Config(format!("platform kernel: {e}")))?;
        Ok(Self {
            backend: "cloud-hypervisor".into(),
            version: String::from_utf8_lossy(&out.stdout).trim().into(),
            kernel_sha256: format!("{:x}", Sha256::digest(bytes)),
        })
    }
}

pub fn vm_exec_main(args: &VmExecArgs) -> Result<(), G5Error> {
    if args.report_version {
        println!(
            "{}",
            serde_json::to_string(&VmmVersion::probe()?)
                .map_err(|e| G5Error::Config(e.to_string()))?
        );
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let code = linux::run(args).map_err(|e| {
            if let Some(pressure) = g5d_foundation::disk::classify_io(e.as_ref(), "vm_bootstrap") {
                // The parent accepts this canonical host-written stderr line,
                // never arbitrary guest serial output or image error text.
                eprintln!("{}", pressure.reason());
                G5Error::HostDiskPressure(pressure)
            } else {
                G5Error::Config(format!("VM preparation/supervision: {e:#}"))
            }
        })?;
        if code != 0 {
            // run has already reaped both children and released its disk guards.
            std::process::exit(code);
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err(G5Error::Config(
        "Cloud Hypervisor requires Linux/KVM; select process mode for local development".into(),
    ))
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::isolation::{workload_root as root, workload_template as template};
    use anyhow::{ensure, Context};
    use std::{
        fs::OpenOptions,
        io::{Read, Write},
        os::unix::{
            fs::{OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
        process::{Child, Stdio},
        time::{Duration, Instant},
    };

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn private_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(bytes)?;
        Ok(())
    }

    fn block_image(source: &Path, destination: &Path, size_mib: u64) -> anyhow::Result<()> {
        let f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination)?;
        f.set_len(
            size_mib
                .checked_mul(1024 * 1024)
                .context("image size overflow")?,
        )?;
        let output = root::helper(root::find_mkfs()?)
            .args(["-F", "-q", "-d"])
            .arg(source)
            .arg(destination)
            .output()?;
        if !output.status.success() {
            return Err(root::write_failure(source, "bootstrap_population", &output.stderr).into());
        }
        Ok(())
    }

    fn identity() -> anyhow::Result<(u32, u32)> {
        let mut user: libc::passwd = unsafe { std::mem::zeroed() };
        let mut buffer = vec![0u8; 65536];
        let mut result = std::ptr::null_mut();
        let error = unsafe {
            libc::getpwnam_r(
                c"g5-vmm".as_ptr(),
                &mut user,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        ensure!(
            error == 0 && !result.is_null(),
            "installer-owned g5-vmm identity is missing"
        );
        let uid = user.pw_uid;
        let gid = user.pw_gid;
        ensure!(
            uid != 0 && gid != 0,
            "VMM must run under a non-root identity"
        );
        Ok((uid, gid))
    }

    // Stream serial output without retaining arbitrary guest output. A successful
    // VMM shutdown alone does not mean bootstrap or the guest command succeeded.
    fn relay_serial(mut serial: impl Read) -> std::io::Result<Option<i32>> {
        let mut chunk = [0; 8192];
        let mut line = Vec::with_capacity(64);
        let mut oversized = false;
        let mut exit = None;
        loop {
            let n = serial.read(&mut chunk)?;
            if n == 0 {
                return Ok(exit);
            }
            let _ = std::io::stdout().write_all(&chunk[..n]);
            for byte in &chunk[..n] {
                if *byte == b'\n' {
                    if !oversized {
                        if let Ok(text) = std::str::from_utf8(&line) {
                            if let Some(code) =
                                text.trim_end_matches('\r').strip_prefix("G5_GUEST_EXIT=")
                            {
                                exit = code.parse().ok();
                            }
                        }
                    }
                    line.clear();
                    oversized = false;
                } else if line.len() < 64 {
                    line.push(*byte);
                } else {
                    oversized = true;
                }
            }
        }
    }

    pub(super) fn run(args: &VmExecArgs) -> anyhow::Result<i32> {
        let rootfs = Path::new(args.rootfs.as_deref().context("missing rootfs")?).canonicalize()?;
        let capacity = args
            .scratch_disk_mib
            .context("published workload capacity is required")?;
        let selected = std::env::var_os(template::TEMPLATE_ENV)
            .map(PathBuf::from)
            .or_else(|| {
                rootfs
                    .parent()
                    .filter(|p| p.join(".complete").exists())
                    .map(Path::to_owned)
            });
        if args.prepare_block_template {
            let selected =
                selected.context("template preparation needs a completed OCI template")?;
            template::prewarm(
                &selected,
                capacity,
                args.block_template_reserve_mib
                    .unwrap_or(u64::from(capacity)),
                root::find_mkfs()?,
            )?;
            if let Some(manifest) = &args.block_template_repo_seed {
                super::super::workload_seed::prepare(
                    &selected,
                    capacity,
                    args.block_template_reserve_mib
                        .unwrap_or(u64::from(capacity)),
                    manifest,
                    root::find_mkfs()?,
                    args.block_template_seed_stage_only,
                )?;
            }
            return Ok(0);
        }
        if args.prepare_private_workload {
            root::validate_owned_root(&rootfs, std::env::var_os(root::OWNED_ROOT_ENV).as_deref())?;
            let selected =
                selected.context("private preparation requires a completed OCI template")?;
            let guard =
                template::clone_or_prepare(&selected, &rootfs, capacity, u64::from(capacity))?;
            template::publish_private_pin(guard, &rootfs)?;
            return Ok(0);
        }
        ensure!(
            available(),
            "Cloud Hypervisor, platform kernel/initramfs or KVM unavailable"
        );
        ensure!(
            args.cpus > 0 && args.mem_mib >= 128 && !args.cmd.is_empty(),
            "invalid VM resource/command specification"
        );
        if args.nested_virtualization {
            ensure!(
                platform_kernel_has_kvm().map_err(anyhow::Error::msg)?,
                "nested virtualization requires a platform kernel with KVM built in ({} lacks CONFIG_KVM=y)",
                platform_kernel_config().display()
            );
        }
        root::validate_owned_root(&rootfs, std::env::var_os(root::OWNED_ROOT_ENV).as_deref())?;
        let owned = rootfs.parent().context("missing owned VM directory")?;
        let selected = selected.context("VM requires a prepared environment template")?;
        root::ensure_directory(&rootfs, ".g5")?;
        let started = Instant::now();

        let mut work = None;
        let mut runtime = None;
        for mount in &args.volumes {
            let parts: Vec<_> = mount.split(':').collect();
            match parts.as_slice() {
                [source, "/work"] | [source, "/work", "rw"] => {
                    ensure!(
                        work.replace(PathBuf::from(source)).is_none(),
                        "duplicate workspace input"
                    );
                }
                [source, "/opt/g5", "ro"] => {
                    ensure!(
                        runtime.replace(PathBuf::from(source)).is_none(),
                        "duplicate runtime input"
                    );
                }
                _ => anyhow::bail!("unsupported volume: require /work and read-only /opt/g5"),
            }
        }
        let runtime = runtime
            .context("complete read-only runtime is required")?
            .canonicalize()?;
        let runtime_disk = runtime.join("runtime.ext4");
        ensure!(
            runtime_disk.is_file(),
            "prepared runtime block artifact is absent"
        );
        let input = owned.join("boot-input");
        std::fs::create_dir(&input)?;
        std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o700))?;
        // The host's current profile rides every boot, including restarts, so a
        // guest always describes the door of the host it actually runs on.
        let cache_profile = stage_cache_profile(&runtime, &input)?;
        let restore: Option<g5_internal_api::host_ctl::VmStart> = std::env::var(RESTORE_INPUT)
            .ok()
            .map(|s| serde_json::from_str(&s))
            .transpose()?;
        let seed_pin = work.as_ref().map(|work| work.join(".g5/repo-seed.ext4"));
        let disk = if restore.is_some() && seed_pin.as_ref().is_some_and(|pin| pin.is_file()) {
            let source = work
                .as_ref()
                .context("seed pin is required")?
                .join(".g5/repo-seed.ext4");
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::symlink_metadata(&source)?;
            ensure!(
                metadata.is_file()
                    && metadata.uid() == unsafe { libc::geteuid() }
                    && metadata.mode() & 0o077 == 0
                    && metadata.len() == u64::from(capacity) * 1024 * 1024,
                "invalid private repository seed pin"
            );
            let guard = root::BackingGuard(rootfs.join(root::BACKING_REL));
            ensure!(
                template::reflink(&source, &guard.0)?,
                "seed pin reflink unsupported"
            );
            guard
        } else if let Some(pin) = template::take_private_pin(&rootfs, capacity)? {
            pin
        } else {
            template::clone_or_prepare(&selected, &rootfs, capacity, u64::from(capacity))?
        };
        let import_workspace = restore.is_none();
        // The private disk stays a sparse clone; Start admission alone checked V.
        let mut archive_bytes = 0;
        if import_workspace {
            let work = work
                .context("workspace input is required")?
                .canonicalize()?;
            ensure!(
                work.is_dir() && !work.starts_with(owned) && !owned.starts_with(&work),
                "invalid workspace import root"
            );
            let archive_bound = workspace_archive_bound(&work, u64::from(capacity) * 1024 * 1024)?;
            let archive = input.join("work.tar");
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&archive)?;
            let mut writer = ChildGuard(
                root::helper("tar")
                    .args(["--numeric-owner", "-cpf", "-", "-C"])
                    .arg(work)
                    .arg(".")
                    .stdout(Stdio::piped())
                    .spawn()?,
            );
            let output = writer
                .0
                .stdout
                .take()
                .context("workspace archive output missing")?;
            let copied = std::io::copy(&mut output.take(archive_bound + 1), &mut f)?;
            ensure!(
                copied <= archive_bound,
                "workspace import grew beyond its admitted budget"
            );
            let status = writer.0.wait()?;
            ensure!(status.success(), "workspace archive failed");
            archive_bytes = std::fs::metadata(archive)?.len();
        }
        let mut env: BTreeMap<String, String> = std::env::var(GUEST_ENV)
            .ok()
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        for entry in &args.env {
            let (k, v) = entry
                .split_once('=')
                .context("invalid guest environment entry")?;
            ensure!(
                !k.is_empty() && !k.contains('\0') && !v.contains('\0'),
                "invalid guest environment"
            );
            env.insert(k.into(), v.into());
        }
        if let Ok(token) = std::env::var(g5d_foundation::AGENT_CARRIER_TOKEN_ENV) {
            env.insert(g5d_foundation::AGENT_CARRIER_TOKEN_ENV.into(), token);
        }
        let aliases = guest_host_aliases(&args.guest_host_alias)?;
        let config = GuestBootConfig {
            version: 2,
            env,
            command: args.cmd.clone(),
            workdir: args.workdir.clone().unwrap_or_else(|| "/work".into()),
            host_aliases: aliases,
            import_workspace,
            restore,
        };
        let config = serde_json::to_vec(&config)?;
        ensure!(
            config.len() <= 4 * 1024 * 1024,
            "bootstrap configuration exceeds guest limit"
        );
        let boot_size = 64 + archive_bytes.saturating_mul(2).div_ceil(1024 * 1024);
        private_file(&input.join("config.json"), &config)?;
        let boot_disk = owned.join("bootstrap.ext4");
        block_image(&input, &boot_disk, boot_size)?;
        std::fs::remove_dir_all(&input)?;
        let (uid, gid) = identity()?;
        let (_, net_gid) = super::super::passt::identity()?;
        // The sidecar may traverse, but not list/read private VM data. Files remain 0600.
        std::os::unix::fs::chown(owned, None, Some(net_gid))?;
        std::fs::set_permissions(owned, std::fs::Permissions::from_mode(0o710))?;
        for p in [&disk.0, &boot_disk] {
            std::os::unix::fs::chown(p, Some(uid), Some(gid))?;
        }
        // VMM needs to traverse rootfs/.g5 to open its private workload disk.
        let workload = owned.join("workload.ext4");
        std::fs::rename(&disk.0, &workload)?;
        drop(disk);
        let forwards = args
            .port_map
            .iter()
            .map(|s| {
                let (host, guest) = s.split_once(':').context("invalid port mapping")?;
                let host: u16 = host.parse()?;
                let guest: u16 = guest.parse()?;
                ensure!(
                    (49152..=65535).contains(&host) && guest != 0,
                    "invalid port range"
                );
                Ok((host, guest))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let loopback = super::super::vmm_loopback::vmm_loopback_addr_for_pid(std::process::id())?;
        let mut net = super::super::passt::spawn_passt_at(loopback, &forwards, &owned.join("net"))?;
        let mut kvm: libc::group = unsafe { std::mem::zeroed() };
        let mut buffer = vec![0u8; 65536];
        let mut result = std::ptr::null_mut();
        let error = unsafe {
            libc::getgrnam_r(
                c"kvm".as_ptr(),
                &mut kvm,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        ensure!(error == 0 && !result.is_null(), "KVM group is absent");
        let groups = [kvm.gr_gid, net_gid];
        for path in [&workload, &runtime_disk, &boot_disk, &net.socket] {
            ensure!(
                !path.to_string_lossy().contains([',', '\n', '\r']),
                "invalid VMM artifact path"
            );
        }
        let mac = super::super::passt::guest_mac_for_pid(std::process::id())
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<Vec<_>>()
            .join(":");
        let mut command = platform_command(vmm_binary());
        command
            .args([
                "--cpus",
                &cpus_argument(args.cpus, args.nested_virtualization),
                "--memory",
                &format!("size={}M,shared=on", args.mem_mib),
            ])
            .arg("--kernel")
            .arg(kernel())
            .arg("--initramfs")
            .arg(initramfs())
            .arg("--disk")
            .arg(format!("path={},image_type=raw", workload.display()))
            .arg(format!(
                "path={},image_type=raw,readonly=on,sparse=off",
                runtime_disk.display()
            ))
            .arg(format!(
                "path={},image_type=raw,readonly=on,sparse=off",
                boot_disk.display()
            ))
            .arg("--net")
            .arg(format!(
                "vhost_user=on,socket={},num_queues=2,queue_size=256,mac={mac}",
                net.socket.display()
            ))
            .args([
                "--cmdline",
                "console=ttyS0 panic=-1 quiet",
                "--serial",
                "tty",
                "--console",
                "off",
                "--landlock",
            ])
            // CH's tty serial reader treats stdin EOF as a VMM exit request.
            // Hold an idle pipe open; interactive PTYs use the dispatcher.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        unsafe {
            command.pre_exec(move || {
                if libc::setgroups(groups.len(), groups.as_ptr()) != 0
                    || libc::setgid(gid) != 0
                    || libc::setuid(uid) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        root::kill_with_parent(&mut command);
        let mut vmm = ChildGuard(command.spawn().context("launch Cloud Hypervisor")?);
        let serial = vmm.0.stdout.take().context("VMM serial pipe missing")?;
        let relay = std::thread::spawn(move || relay_serial(serial));
        tracing::info!(
            preparation_seconds = started.elapsed().as_secs_f64(),
            nested_virtualization = args.nested_virtualization,
            cache_profile,
            "Cloud Hypervisor launched with private block workload"
        );
        let exit_code = loop {
            if let Some(status) = vmm.0.try_wait()? {
                ensure!(status.success(), "Cloud Hypervisor exited: {status}");
                let guest_exit = relay
                    .join()
                    .map_err(|_| anyhow::anyhow!("serial relay failed"))??;
                let code = guest_exit.context("guest exited without a shutdown receipt")?;
                ensure!((0..=255).contains(&code), "invalid guest exit status");
                break code;
            }
            if let Some(status) = net.child.try_wait()? {
                // passt --one-off can exit before CH finishes tearing down a
                // populated guest. A loaded 7-GiB guest exceeds 250 ms here.
                // Reuse the normal shutdown budget only for a clean sidecar
                // exit; abnormal network loss must still terminate the VMM.
                let grace = if status.success() {
                    super::super::GRACEFUL_CHILD_REAP_TIMEOUT
                } else {
                    Duration::ZERO
                };
                let deadline = Instant::now() + grace;
                while vmm.0.try_wait()?.is_none() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                ensure!(
                    vmm.0.try_wait()?.is_some(),
                    "VM network backend exited: {status}"
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        Ok(exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpus_argument_adds_nested_only_when_requested() {
        assert_eq!(cpus_argument(2, false), "boot=2");
        assert_eq!(cpus_argument(2, true), "boot=2,nested=on");
    }

    #[test]
    fn cache_profile_is_staged_world_readable_only_when_the_host_published_one() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");
        let input = temp.path().join("boot-input");
        std::fs::create_dir(&runtime).unwrap();
        std::fs::create_dir(&input).unwrap();
        // A host without a door publishes nothing: the guest keeps the placeholder.
        assert!(!stage_cache_profile(&runtime, &input).unwrap());
        assert!(!input.join(CACHE_PROFILE_NAME).exists());
        let profile =
            "G5_CACHE_DOOR=http://g5-host:7111\nG5_BASE_IMAGE_MIRROR_TIERS=base base-rust\n";
        std::fs::write(runtime.join(CACHE_PROFILE_NAME), profile).unwrap();
        assert!(stage_cache_profile(&runtime, &input).unwrap());
        let staged = input.join(CACHE_PROFILE_NAME);
        assert_eq!(std::fs::read_to_string(&staged).unwrap(), profile);
        assert_eq!(
            std::fs::metadata(&staged).unwrap().permissions().mode() & 0o777,
            0o644
        );
        // Staging twice into one input is a programming error, not a silent overwrite.
        assert!(stage_cache_profile(&runtime, &input).is_err());
    }

    /// Child of `cache_profile_stays_world_readable_under_a_restrictive_umask`:
    /// the umask is process-wide, so the restrictive one lives in its own process.
    #[test]
    #[ignore]
    fn cache_profile_umask_child() {
        let Some(dir) = std::env::var_os("G5_TEST_CACHE_PROFILE_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        unsafe { libc::umask(0o077) };
        assert!(stage_cache_profile(&dir.join("runtime"), &dir.join("boot-input")).unwrap());
    }

    #[test]
    fn cache_profile_stays_world_readable_under_a_restrictive_umask() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("runtime")).unwrap();
        std::fs::create_dir(temp.path().join("boot-input")).unwrap();
        std::fs::write(
            temp.path().join("runtime").join(CACHE_PROFILE_NAME),
            "G5_CACHE_DOOR=http://g5-host:7111\n",
        )
        .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "isolation::cloud_hypervisor::tests::cache_profile_umask_child",
                "--ignored",
            ])
            .env("G5_TEST_CACHE_PROFILE_DIR", temp.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let staged = temp.path().join("boot-input").join(CACHE_PROFILE_NAME);
        assert_eq!(
            std::fs::metadata(&staged).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn cache_profile_rejects_oversize_symlinked_or_non_regular_files() {
        let temp = tempfile::tempdir().unwrap();
        for (name, prepare) in [
            (
                "oversize",
                Box::new(|runtime: &Path| {
                    std::fs::write(
                        runtime.join(CACHE_PROFILE_NAME),
                        vec![b'x'; CACHE_PROFILE_LIMIT as usize + 1],
                    )
                    .unwrap();
                }) as Box<dyn Fn(&Path)>,
            ),
            (
                "directory",
                Box::new(|runtime: &Path| {
                    std::fs::create_dir(runtime.join(CACHE_PROFILE_NAME)).unwrap();
                }),
            ),
            (
                "symlink",
                Box::new(|runtime: &Path| {
                    std::fs::write(runtime.join("real.env"), "G5_CACHE_DOOR=x\n").unwrap();
                    std::os::unix::fs::symlink("real.env", runtime.join(CACHE_PROFILE_NAME))
                        .unwrap();
                }),
            ),
        ] {
            let runtime = temp.path().join(name).join("runtime");
            let input = temp.path().join(name).join("boot-input");
            std::fs::create_dir_all(&runtime).unwrap();
            std::fs::create_dir_all(&input).unwrap();
            prepare(&runtime);
            let error = stage_cache_profile(&runtime, &input).unwrap_err();
            assert!(
                error.to_string().contains("host cache profile"),
                "{name}: {error:#}"
            );
            assert!(
                !input.join(CACHE_PROFILE_NAME).exists(),
                "{name} staged a file"
            );
        }
    }

    #[test]
    fn kernel_config_must_declare_built_in_kvm() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("kernel.config");
        std::fs::write(
            &config,
            "CONFIG_64BIT=y\n# CONFIG_KVM is not set\nCONFIG_KVM_GUEST=y\n",
        )
        .unwrap();
        assert!(!kernel_config_has_kvm(&config).unwrap());
        std::fs::write(
            &config,
            "CONFIG_VIRTUALIZATION=y\nCONFIG_KVM=y\nCONFIG_KVM_INTEL=y\n",
        )
        .unwrap();
        assert!(kernel_config_has_kvm(&config).unwrap());
        // A missing configuration is an error, never a silent "no KVM".
        let error = kernel_config_has_kvm(&temp.path().join("absent")).unwrap_err();
        assert!(error.contains("absent"), "{error}");
        assert_eq!(
            platform_kernel_config(),
            kernel().with_file_name("kernel.config")
        );
    }

    #[test]
    fn loopback_aliases_use_the_host_door() {
        let aliases = guest_host_aliases(&[
            "g5-api=127.0.0.1".into(),
            "other=127.45.3.2".into(),
            "remote=192.0.2.3".into(),
        ])
        .unwrap();
        assert_eq!(aliases[0].1, super::super::passt::HOST_DOOR_ADDR);
        assert_eq!(aliases[1].1, super::super::passt::HOST_DOOR_ADDR);
        assert_eq!(
            aliases[2].1,
            "192.0.2.3".parse::<std::net::Ipv4Addr>().unwrap()
        );
        for bad in ["=127.0.0.1", "bad name=127.0.0.1", "host=::1", "host"] {
            assert!(guest_host_aliases(&[bad.into()]).is_err());
        }
    }

    #[test]
    fn archive_admission_counts_small_files_and_does_not_follow_links() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path().join("work");
        std::fs::create_dir(&work).unwrap();
        for n in 0..100 {
            std::fs::write(work.join(format!("file-{n}")), b"x").unwrap();
        }
        let outside = temp.path().join("outside");
        std::fs::File::create(&outside)
            .unwrap()
            .set_len(1 << 30)
            .unwrap();
        std::os::unix::fs::symlink(&outside, work.join("link")).unwrap();
        let mut nested = work.clone();
        for _ in 0..4 {
            nested = nested.join("long-name-".repeat(20));
            std::fs::create_dir(&nested).unwrap();
        }
        let original = nested.join("original");
        std::fs::write(&original, b"hard link").unwrap();
        for i in 0..100 {
            std::fs::hard_link(&original, work.join(format!("hard-{i}"))).unwrap();
        }
        let bound = workspace_archive_bound(&work, 2 * 1024 * 1024).unwrap();
        let mut tar = std::process::Command::new("tar");
        // macOS bsdtar otherwise adds AppleDouble metadata absent from GNU tar.
        #[cfg(target_os = "macos")]
        tar.args(["--no-xattrs", "--no-mac-metadata"]);
        let output = tar
            .args(["-cf", "-", "-C"])
            .arg(&work)
            .arg(".")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stdout.len() as u64 <= bound);
        assert!(workspace_archive_bound(&work, bound - 1).is_err());
        std::fs::File::create(work.join("large"))
            .unwrap()
            .set_len(2 * 1024 * 1024)
            .unwrap();
        assert!(workspace_archive_bound(&work, 1024 * 1024).is_err());
    }
}
