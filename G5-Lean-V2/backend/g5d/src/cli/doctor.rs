//! `g5d doctor` — Run system diagnostics.
//!
//! Checks COW support, microVM readiness, git version, toolchain detection,
//! workspace validation, cache status, and the workspace host layout.

use std::io;
use std::path::Path;
use std::process::Command as ProcessCommand;

use tokio::process::Command as TokioCommand;

use g5_contracts::error::G5Error;

use super::{format_success, resolve_workspace_path, DoctorArgs, GlobalFlags};
use crate::cli::config::{IsolationMode, WorkspaceConfig};

/// Linux path baked by PR #114's worker image — the runtime binary directory
/// mount-injected into every microVM at `/opt/g5/bin` (per `host::assignment::GUEST_RUNTIME_BIN_DIR`).
const DEFAULT_RUNTIME_BIN_DIR: &str = "/var/g5d-data/runtime/bin";

/// Linux path baked by PR #114's worker image — env file consumed by the
/// `g5d-host.service` systemd unit.
const DEFAULT_HOST_UP_ENV_FILE: &str = "/etc/g5/host.env";

/// systemd unit name shipped by PR #114.
const HOST_UP_SYSTEMD_UNIT: &str = "g5d-host.service";

// The `Workspace reflink` probe targets the same path the host supervisor
// writes to, resolved via `g5d_foundation::workspace::resolved_workspaces_root()`.
// Codex review B1 of PR-rev7-6: hardcoding the path here had the probe
// testing one directory while the supervisor wrote to another.

/// Env-var keys the `g5d-host.service` unit expects to read out of
/// `/etc/g5/host.env`. Must match the Packer template at
/// `infra/packer/g5-worker/files/g5d-host.env`; doctor reports
/// `missing(<key>)` when an expected key is absent so an operator catches
/// template drift before tasks start failing.
const HOST_UP_ENV_KEYS: &[&str] = &[
    "G5_CONTROL_PLANE_URL",
    "G5_HOST_JWT",
    "G5D_HOST_ID",
    "G5_HOST_REGION",
    "G5_RUNTIME_NODE",
    "G5D_HOST_RUNTIME_BIN_DIR",
    "G5_RUNTIME_LIB_DIR",
    "RUST_LOG",
];

/// Execute the `g5d doctor` command.
///
/// Runs diagnostic checks and reports system readiness:
/// 1. COW support detection
/// 2. MicroVM readiness
/// 3. Git configuration
/// 4. Toolchain detection
/// 5. Workspace validation (if in a workspace)
/// 6. Cache status
pub async fn execute(args: &DoctorArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let mut checks: Vec<DiagnosticCheck> = Vec::new();
    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "Failed to get current directory");
        std::path::PathBuf::new()
    });
    let workspace_path = resolve_workspace_path(global.workspace.as_ref(), &cwd);
    let isolation_mode = resolve_requested_isolation_mode(workspace_path.as_deref());

    // 1. COW support detection
    checks.push(check_cow_support());

    // 2. MicroVM readiness
    checks.push(check_microvm_readiness(isolation_mode));

    // 3. Git configuration
    checks.push(check_git());

    // 4. Toolchain detection
    checks.extend(check_toolchains());

    // 5. Workspace validation
    if let Some(ref ws_path) = workspace_path {
        checks.push(check_workspace(ws_path, global.api_url.as_deref()));
    }

    // 6. Cache status
    checks.push(check_cache());

    // 7. Workspace host layout (D32).
    checks.push(check_host_up_supervisor(
        Path::new(DEFAULT_RUNTIME_BIN_DIR),
        Path::new(DEFAULT_HOST_UP_ENV_FILE),
    ));

    // 8. PR-rev7-6 §5.4: workspace-reflink probe. Advisory — the
    // load-bearing enforcement is in the supervisor's materializer.
    // The probe path resolves via the same env helper the supervisor
    // uses for `workspaces_root`, so doctor always tests the directory
    // the supervisor actually writes to (codex review B1 of PR-rev7-6).
    //
    // Resolution order (codex round 3 of PR-rev7-6): `--workspaces-root`
    // CLI flag → `G5D_HOST_WORKSPACES_ROOT` env → resolved default.
    // `Option::or_else` over the helper preserves the same env→default
    // fallback the supervisor uses, so a sibling `g5d doctor` invocation
    // from another shell with the env var unset still tests the canonical
    // path. The CLI flag is the cross-shell alignment mechanism when an
    // operator runs `g5d host --workspaces-root /custom` in a separate
    // terminal.
    let (workspaces_root, workspaces_source) = resolve_doctor_workspaces_root(args);
    checks.push(check_workspace_reflink_support(&workspaces_root, workspaces_source).await);

    // Output results
    #[derive(serde::Serialize)]
    struct DoctorResult {
        status: String,
        checks: Vec<DiagnosticCheck>,
    }

    let all_ok = required_checks_passed(&checks);
    let advisory_issues = checks.iter().filter(|c| !c.required && !c.ok).count();
    let result = DoctorResult {
        status: if all_ok { "ok" } else { "issues_found" }.to_string(),
        checks: checks.clone(),
    };

    if global.json {
        println!("{}", format_success(&result, true));
    } else if !global.quiet {
        println!("g5d doctor — System Diagnostics\n");
        for check in &checks {
            let icon = match (check.ok, check.required) {
                (true, _) => "✓",
                (false, true) => "✗",
                (false, false) => "!",
            };
            let label = if check.required {
                check.name.clone()
            } else {
                format!("{} (advisory)", check.name)
            };
            println!("  {} {}: {}", icon, label, check.message);
        }
        println!();
        if all_ok {
            if advisory_issues == 0 {
                println!("All required checks passed.");
            } else {
                println!("All required checks passed. Advisory findings noted above.");
            }
        } else {
            println!("Some required checks failed. See above for details.");
        }
    }

    if all_ok {
        Ok(())
    } else {
        Err(G5Error::Internal {
            message: "Some diagnostic checks failed".to_string(),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct DiagnosticCheck {
    name: String,
    ok: bool,
    message: String,
    required: bool,
}

fn required_checks_passed(checks: &[DiagnosticCheck]) -> bool {
    checks
        .iter()
        .filter(|check| check.required)
        .all(|check| check.ok)
}

/// Check COW (copy-on-write) filesystem support.
fn check_cow_support() -> DiagnosticCheck {
    #[cfg(target_os = "macos")]
    {
        // Check for APFS by trying to detect filesystem type
        // On macOS, APFS supports clonefile(2)
        DiagnosticCheck {
            name: "COW".to_string(),
            ok: true,
            message: "APFS clonefile (macOS — assumed available)".to_string(),
            required: false,
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Check filesystem type for XFS/Btrfs reflink support
        if let Ok(output) = ProcessCommand::new("stat")
            .args(["-f", "-c", "%T", "/"])
            .output()
        {
            let fs_type = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let supported = matches!(fs_type.as_str(), "xfs" | "btrfs");
            DiagnosticCheck {
                name: "COW".to_string(),
                ok: supported,
                message: if supported {
                    format!("{} reflinks ✓", fs_type)
                } else {
                    format!(
                        "Not available ({}) — full clone fallback available",
                        fs_type
                    )
                },
                required: false,
            }
        } else {
            DiagnosticCheck {
                name: "COW".to_string(),
                ok: false,
                message: "Could not detect filesystem type".to_string(),
                required: false,
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        DiagnosticCheck {
            name: "COW".to_string(),
            ok: false,
            message: "Not available on this platform — full clone fallback available".to_string(),
            required: false,
        }
    }
}

#[derive(Clone, Copy)]
struct RequestedIsolationMode {
    mode: IsolationMode,
    explicit: bool,
}

fn resolve_requested_isolation_mode(workspace_path: Option<&Path>) -> RequestedIsolationMode {
    if let Ok(mode) = std::env::var("G5D_ISOLATION_MODE") {
        if let Ok(mode) = mode.parse::<IsolationMode>() {
            return RequestedIsolationMode {
                mode,
                explicit: true,
            };
        }
    }

    if let Some(workspace_path) = workspace_path {
        if let Some(requested) = resolve_workspace_requested_isolation_mode(workspace_path) {
            return requested;
        }
    }

    RequestedIsolationMode {
        mode: IsolationMode::MicroVm,
        explicit: false,
    }
}

fn resolve_workspace_requested_isolation_mode(
    workspace_path: &Path,
) -> Option<RequestedIsolationMode> {
    let config_path = workspace_path.join(".g5").join("workspace.yaml");
    let content = std::fs::read_to_string(config_path).ok()?;
    let value: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    let mode = value
        .get("isolation")?
        .get("mode")?
        .as_str()?
        .parse::<IsolationMode>()
        .ok()?;

    Some(RequestedIsolationMode {
        mode,
        explicit: true,
    })
}

/// Check the selected Cloud Hypervisor artifacts and Linux/KVM availability.
fn check_microvm_readiness(requested_mode: RequestedIsolationMode) -> DiagnosticCheck {
    let required = requested_mode.explicit && requested_mode.mode == IsolationMode::MicroVm;

    if !cfg!(feature = "cloud-hypervisor") {
        return DiagnosticCheck {
            name: "MicroVM".to_string(),
            ok: false,
            message: unavailable_microvm_message(
                requested_mode,
                "g5d was built without the cloud-hypervisor feature",
            ),
            required,
        };
    }

    #[cfg(feature = "cloud-hypervisor")]
    let available = crate::cli::isolation::cloud_hypervisor::available();
    #[cfg(not(feature = "cloud-hypervisor"))]
    let available = false;
    let mut message = if available {
        "Cloud Hypervisor artifacts present (KVM available)".to_string()
    } else {
        unavailable_microvm_message(
            requested_mode,
            "Cloud Hypervisor requires Linux/KVM and the platform VMM, kernel and initramfs",
        )
    };
    append_microvm_evidence(&mut message);
    DiagnosticCheck {
        name: "MicroVM".to_string(),
        ok: available,
        message,
        required,
    }
}

/// Append platform version evidence and Linux nested-virtualization status.
fn append_microvm_evidence(message: &mut String) {
    if let Some(version_summary) = vmm_version_summary() {
        message.push_str(" — ");
        message.push_str(&version_summary);
    }
    if cfg!(target_os = "linux") {
        let status = nested_virt_status(read_sys_file);
        message.push_str(" — nested-virt=");
        message.push_str(&status);
    }
}

/// Best-effort platform binary version and selected kernel digest.
#[cfg(feature = "cloud-hypervisor")]
fn vmm_version_summary() -> Option<String> {
    match crate::cli::isolation::cloud_hypervisor::VmmVersion::probe() {
        Ok(v) => Some(format!(
            "Cloud Hypervisor={} / kernel.sha256={}",
            v.version, v.kernel_sha256
        )),
        Err(e) => Some(format!("Cloud Hypervisor probe failed: {e}")),
    }
}

#[cfg(not(feature = "cloud-hypervisor"))]
fn vmm_version_summary() -> Option<String> {
    None
}

/// Read a path into a string, used as the seam for nested-virt probing in tests.
fn read_sys_file(path: &Path) -> io::Result<String> {
    std::fs::read_to_string(path)
}

/// Read Linux nested-virt status from `/sys/module/kvm_intel/parameters/nested`
/// or `/sys/module/kvm_amd/parameters/nested`, whichever is present.
///
/// On hosted Linux (GCE/AWS) the platform exposes `/dev/kvm` whether or not
/// nested virtualization is enabled for the instance; the VMM then fails at
/// boot with EINVAL. Reading the actual kernel-module parameter is the only
/// way to give the operator a single actionable line — "your KVM is there,
/// nested-virt is off, fix it in your cloud console".
fn nested_virt_status<R>(read_file: R) -> String
where
    R: Fn(&Path) -> io::Result<String>,
{
    let candidates = [
        ("kvm_intel", "/sys/module/kvm_intel/parameters/nested"),
        ("kvm_amd", "/sys/module/kvm_amd/parameters/nested"),
    ];
    for (module, path) in candidates {
        match read_file(Path::new(path)) {
            Ok(raw) => {
                let value = raw.trim();
                let enabled = matches!(value, "Y" | "y" | "1");
                let disabled = matches!(value, "N" | "n" | "0");
                if enabled {
                    return format!("enabled ({module})");
                }
                if disabled {
                    return format!("disabled ({module}={value})");
                }
                return format!("unknown ({module}={value})");
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return format!("unreadable ({module}: {err})"),
        }
    }
    "kvm module not loaded".to_string()
}

fn unavailable_microvm_message(requested_mode: RequestedIsolationMode, reason: &str) -> String {
    match (requested_mode.mode, requested_mode.explicit) {
        (IsolationMode::Process, true) => format!(
            "Not available — {}. Process isolation is configured, so microVM support is optional.",
            reason
        ),
        (IsolationMode::MicroVm, true) => format!(
            "Not available — {}. Explicit microvm mode would fail to start.",
            reason
        ),
        (IsolationMode::MicroVm, false) | (IsolationMode::Process, false) => format!(
            "Not available — {}. MicroVM support is optional unless explicitly configured.",
            reason
        ),
    }
}

/// Check git version and configuration.
fn check_git() -> DiagnosticCheck {
    match crate::cli::native_git::runner()
        .and_then(|runner| runner.version_blocking().map_err(Into::into))
    {
        Ok(version) => {
            // Parse version number — minimum 2.20 for worktree support
            let version_ok = parse_git_version(&version)
                .map(|(major, minor, _)| major > 2 || (major == 2 && minor >= 20))
                .unwrap_or(false);

            DiagnosticCheck {
                name: "Git".to_string(),
                ok: version_ok,
                message: if version_ok {
                    format!("{} ✓", version)
                } else {
                    format!("{} — minimum 2.20 required for worktree support", version)
                },
                required: true,
            }
        }
        _ => DiagnosticCheck {
            name: "Git".to_string(),
            ok: false,
            message: "Not found".to_string(),
            required: true,
        },
    }
}

/// Parse git version string like "git version 2.44.0" into (major, minor, patch).
fn parse_git_version(version_str: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = version_str.split_whitespace().collect();
    let version_part = parts.iter().find(|s| s.contains('.'))?;
    let nums: Vec<u32> = version_part
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() >= 2 {
        Some((nums[0], nums[1], nums.get(2).copied().unwrap_or(0)))
    } else {
        None
    }
}

/// Detect installed toolchains.
fn check_toolchains() -> Vec<DiagnosticCheck> {
    let checks = vec![
        // Rust
        check_command("Rust", ProcessCommand::new("rustc"), &["--version"]),
        // Node.js
        check_command("Node", ProcessCommand::new("node"), &["--version"]),
        // Python
        check_command("Python", ProcessCommand::new("python3"), &["--version"]),
        // Go
        check_command("Go", ProcessCommand::new("go"), &["version"]),
        // Java
        check_command("Java", ProcessCommand::new("java"), &["-version"]),
    ];

    checks
}

/// Check if a command is available and get its version.
fn check_command(name: &str, mut command: ProcessCommand, args: &[&str]) -> DiagnosticCheck {
    match command.args(args).output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let version = if version.is_empty() {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            } else {
                version
            };
            DiagnosticCheck {
                name: name.to_string(),
                ok: true,
                message: format!("{} ✓", version),
                required: false,
            }
        }
        _ => DiagnosticCheck {
            name: name.to_string(),
            ok: false,
            message: "Not found (optional until required by the active workspace or task)"
                .to_string(),
            required: false,
        },
    }
}

/// Validate workspace configuration.
fn check_workspace(workspace_path: &Path, api_url_override: Option<&str>) -> DiagnosticCheck {
    let workspace_yaml = workspace_path.join(".g5").join("workspace.yaml");
    if !workspace_yaml.exists() {
        return DiagnosticCheck {
            name: "Workspace".to_string(),
            ok: false,
            message: format!("No workspace.yaml at {}", workspace_yaml.display()),
            required: true,
        };
    }

    match WorkspaceConfig::load(workspace_path) {
        Ok(Some(config)) => {
            let repo_count = config.repos.len();
            let remote_status = if config.remote.enabled {
                "remote enabled"
            } else {
                "local only"
            };

            // Check that repo paths exist
            let mut missing_repos = Vec::new();
            for repo in &config.repos {
                let repo_path = workspace_path.join(&repo.path);
                if !repo_path.exists() {
                    missing_repos.push(repo.name.clone());
                }
            }

            let meta_ok = workspace_path.join(".g5").join("workspace-meta").exists();
            let mut issues = Vec::new();

            if config.remote.enabled {
                let has_workspace_id = config
                    .workspace_id
                    .as_deref()
                    .map(|id| !id.trim().is_empty())
                    .unwrap_or(false);
                if !has_workspace_id {
                    issues.push("remote enabled but workspace_id missing".to_string());
                }

                let effective_api_url = api_url_override
                    .map(str::trim)
                    .filter(|url| !url.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        config
                            .remote
                            .api_url
                            .as_deref()
                            .map(str::trim)
                            .filter(|url| !url.is_empty())
                            .map(str::to_string)
                    });
                match effective_api_url {
                    Some(url) => {
                        if reqwest::Url::parse(&url).is_err() {
                            issues.push(format!("remote api_url invalid: {}", url));
                        }
                    }
                    None => issues.push("remote enabled but api_url missing".to_string()),
                }
            }

            if missing_repos.is_empty() && meta_ok && issues.is_empty() {
                DiagnosticCheck {
                    name: "Workspace".to_string(),
                    ok: true,
                    message: format!("valid ✓ ({} repos, {})", repo_count, remote_status),
                    required: true,
                }
            } else {
                if !missing_repos.is_empty() {
                    issues.push(format!("missing repos: {}", missing_repos.join(", ")));
                }
                if !meta_ok {
                    issues.push("workspace-meta not found".to_string());
                }
                DiagnosticCheck {
                    name: "Workspace".to_string(),
                    ok: false,
                    message: format!(
                        "{} repos, {} — issues: {}",
                        repo_count,
                        remote_status,
                        issues.join("; ")
                    ),
                    required: true,
                }
            }
        }
        Ok(None) => DiagnosticCheck {
            name: "Workspace".to_string(),
            ok: false,
            message: "workspace.yaml not found".to_string(),
            required: true,
        },
        Err(e) => DiagnosticCheck {
            name: "Workspace".to_string(),
            ok: false,
            message: format!("Invalid workspace config: {}", e),
            required: true,
        },
    }
}

/// Check cache directory status.
fn check_cache() -> DiagnosticCheck {
    // Check default cache directory
    let cache_dir = std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(".g5").join("cache"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/g5-cache"));

    if cache_dir.exists() {
        // Get directory size (simplified — just check if writable)
        let writable = std::fs::metadata(&cache_dir)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false);

        DiagnosticCheck {
            name: "Cache".to_string(),
            ok: writable,
            message: if writable {
                format!("{} (writable)", cache_dir.display())
            } else {
                format!("{} (read-only!)", cache_dir.display())
            },
            required: true,
        }
    } else {
        DiagnosticCheck {
            name: "Cache".to_string(),
            ok: true,
            message: format!(
                "{} (not created yet — will be created on first use)",
                cache_dir.display()
            ),
            required: true,
        }
    }
}

/// Probe the workspace host on-host layout. Advisory by design —
/// see ontology block D32. The probe reports `required: false` everywhere; on
/// Linux it aggregates `ok = runtime_bin_layout_ok && env_file_ok` so a
/// missing supervisor layout shows up as an advisory finding (`!` icon, counted
/// in the "Advisory findings noted above" summary) instead of being silently
/// rendered as `✓`. systemd unit state is informational only — workers are
/// started by cloud-init at boot, so `disabled` is a valid steady state. On
/// non-Linux hosts the probe reports "not applicable" with `ok = true`.
fn check_host_up_supervisor(runtime_bin_dir: &Path, env_file: &Path) -> DiagnosticCheck {
    #[cfg(target_os = "linux")]
    {
        let layout = collect_host_up_layout(runtime_bin_dir, env_file, systemctl_is_enabled);
        DiagnosticCheck {
            name: "workspace host".to_string(),
            ok: layout.ok,
            message: layout.message,
            required: false,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (runtime_bin_dir, env_file);
        DiagnosticCheck {
            name: "workspace host".to_string(),
            ok: true,
            message: format!("not applicable on {}", std::env::consts::OS),
            required: false,
        }
    }
}

/// Structured result from probing the workspace host layout.
///
/// `ok` aggregates the load-bearing pieces — `runtime_bin_layout` and
/// `env_file` — that the supervisor must have to start the unit cleanly. The
/// systemd unit state is recorded inside `message` but does NOT influence
/// `ok`, because cloud-init bringup leaves the unit running-but-not-enabled
/// on many provisioned hosts.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct HostUpLayoutReport {
    ok: bool,
    message: String,
}

/// Build the workspace host evidence string and aggregate the layout-ok
/// signal. Extracted to a pure helper so the test suite can drive it with
/// tempdir paths + an injected systemd probe on macOS dev hosts.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn collect_host_up_layout<F>(
    runtime_bin_dir: &Path,
    env_file: &Path,
    systemctl_probe: F,
) -> HostUpLayoutReport
where
    F: FnOnce(&str) -> SystemdUnitStatus,
{
    let g5d_bin = runtime_bin_dir.join("g5d");
    let runtime_ok = g5d_bin.is_file();
    let env_status = check_host_up_env_file(env_file);
    let systemd_status = systemctl_probe(HOST_UP_SYSTEMD_UNIT);

    let message = format!(
        "runtime_bin_layout={runtime}({bin_path}) env_file={env_summary} systemd_unit={unit}",
        runtime = check_icon(runtime_ok),
        bin_path = g5d_bin.display(),
        env_summary = env_status.summary,
        unit = systemd_status,
    );

    HostUpLayoutReport {
        ok: runtime_ok && env_status.ok,
        message,
    }
}

/// Result of inspecting the host env file. `ok` is set to `true` only when
/// the file is readable AND every expected key in [`HOST_UP_ENV_KEYS`] is
/// present (commented or uncommented). `summary` is the inline fragment for
/// the workspace host evidence string (e.g. `✓(/etc/g5/host.env)`,
/// `missing(/etc/g5/host.env, keys: G5_HOST_JWT,RUST_LOG)`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct HostUpEnvStatus {
    ok: bool,
    summary: String,
}

/// Probe the host env file: confirm it is readable and that every expected
/// key from the Packer template is present. Commented placeholders (e.g.
/// `# G5_HOST_JWT=`) count as present — the Packer template ships the file
/// pre-populated, but operators sometimes comment values out during
/// debugging; doctor's job is to catch *missing* keys (template drift), not
/// to opine on values.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn check_host_up_env_file(path: &Path) -> HostUpEnvStatus {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let missing: Vec<&&str> = HOST_UP_ENV_KEYS
                .iter()
                .filter(|key| !host_up_env_key_present(&content, key))
                .collect();
            if missing.is_empty() {
                HostUpEnvStatus {
                    ok: true,
                    summary: format!("{}({})", check_icon(true), path.display()),
                }
            } else {
                let names: Vec<&str> = missing.iter().map(|k| **k).collect();
                HostUpEnvStatus {
                    ok: false,
                    summary: format!(
                        "{}({}, missing_keys: {})",
                        check_icon(false),
                        path.display(),
                        names.join(",")
                    ),
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => HostUpEnvStatus {
            ok: false,
            summary: format!("{}({})", check_icon(false), path.display()),
        },
        Err(err) => HostUpEnvStatus {
            ok: false,
            summary: format!(
                "{}({}, unreadable: {err})",
                check_icon(false),
                path.display()
            ),
        },
    }
}

/// Detect whether a `KEY=` line is present in the env-file content. Accepts
/// `KEY=value`, `KEY=`, and the commented variant `# KEY=` (or `#KEY=`),
/// because the Packer template uses commented placeholders during build and
/// operators sometimes re-comment them for local testing.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn host_up_env_key_present(content: &str, key: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        let uncommented = trimmed.strip_prefix('#').unwrap_or(trimmed).trim_start();
        uncommented
            .strip_prefix(key)
            .map(|rest| rest.trim_start().starts_with('='))
            .unwrap_or(false)
    })
}

/// Compact tick/cross marker for inline evidence strings. Distinct from the
/// top-level ✓/✗ used by the doctor renderer so the embedded marker is still
/// readable when the line is grepped or piped through `--json`.
fn check_icon(ok: bool) -> &'static str {
    if ok {
        "✓"
    } else {
        "✗"
    }
}

/// Result of `systemctl is-enabled <unit>`. We carry the exact systemd
/// terminology (`enabled`, `disabled`, `static`, `masked`, `not-installed`,
/// etc.) verbatim so an operator can copy-paste it into a search.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct SystemdUnitStatus(String);

impl std::fmt::Display for SystemdUnitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn systemctl_is_enabled(unit: &str) -> SystemdUnitStatus {
    // Quick PATH lookup — `systemctl` is absent on non-systemd containers and
    // local-dev hosts (e.g. WSL without systemd). When missing we want a
    // clean "systemd not detected" instead of `Err`.
    let on_path = ProcessCommand::new("sh")
        .args(["-c", "command -v systemctl >/dev/null 2>&1"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !on_path {
        return SystemdUnitStatus("systemd not detected".to_string());
    }
    match ProcessCommand::new("systemctl")
        .args(["is-enabled", unit])
        .output()
    {
        Ok(output) => {
            // Both success and failure paths print the status name on stdout
            // (systemctl exits non-zero for `disabled`, `static`, `masked`,
            // `not-installed`). We prefer stdout; fall back to stderr only
            // when stdout is empty.
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                return SystemdUnitStatus(stdout);
            }
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            SystemdUnitStatus(if stderr.is_empty() {
                "unknown".to_string()
            } else {
                stderr
            })
        }
        Err(e) => SystemdUnitStatus(format!("systemctl failed: {e}")),
    }
}

/// Source of the `workspaces_root` path the doctor's reflink probe will test.
/// Reported in the probe's message so operators see at a glance whether they
/// are exercising the CLI flag, the ambient env var, or the compiled-in
/// default — useful when a sibling `g5d host` and `g5d doctor` are run
/// in separate shells (codex round 3 of PR-rev7-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspacesRootSource {
    Flag,
    Env,
    Default,
}

impl WorkspacesRootSource {
    fn label(self) -> &'static str {
        match self {
            Self::Flag => "via --workspaces-root",
            Self::Env => "via G5D_HOST_WORKSPACES_ROOT env",
            Self::Default => "via default",
        }
    }
}

/// Resolve the workspaces root the doctor's reflink probe will test, and
/// report which source picked the path. Mirrors `HostUpArgs`' precedence so
/// `g5d host --workspaces-root /custom` and `g5d doctor --workspaces-root
/// /custom` exercise the same directory.
///
/// The source label is read from `args.workspaces_root_source`, which the
/// parse-time helper `Cli::from_matches_with_sources` populates from clap's
/// `ArgMatches::value_source`. This is authoritative: a previous
/// path-comparison heuristic (codex round 3) mislabeled the case where the
/// CLI flag and the env var both carried the same path (codex round 4 of
/// PR-rev7-6, Low #1). If the field is `None` — which happens only when a
/// caller bypasses the source-tracking parse path and the value is `Some` —
/// we conservatively label the source as `Env`, the same outcome clap would
/// produce when the env var was the sole input.
pub(crate) fn resolve_doctor_workspaces_root(
    args: &DoctorArgs,
) -> (std::path::PathBuf, WorkspacesRootSource) {
    use clap::parser::ValueSource;
    if let Some(ref path) = args.workspaces_root {
        let source = match args.workspaces_root_source {
            Some(ValueSource::CommandLine) => WorkspacesRootSource::Flag,
            Some(ValueSource::EnvVariable) => WorkspacesRootSource::Env,
            // `DefaultValue` cannot fire here because clap only records it
            // when an `arg(default_value = ...)` is in play, which this
            // flag does not have. Treat any other reported source as `Env`
            // so a misrouted caller cannot upgrade an env-supplied value
            // to an apparent flag override.
            _ => WorkspacesRootSource::Env,
        };
        return (path.clone(), source);
    }
    (
        g5d_foundation::workspace::resolved_workspaces_root(),
        WorkspacesRootSource::Default,
    )
}

/// PR-rev7-6 §5.4: probe whether `cp --reflink=always` succeeds inside the
/// workspace VM roots directory (`/var/g5d-data/lib/workspaces/` on the signed
/// host image). The probe writes a 1-byte sentinel into the workspaces
/// root and asserts the reflink copy succeeds.
///
/// Advisory by design — the load-bearing enforcement is in the supervisor's
/// materializer (`g5/g5d/workspace` D6). Doctor surfaces this as a
/// non-required check so the operator catches a misconfigured XFS volume at
/// `g5d doctor` time rather than at the first workspace start. On
/// non-Linux hosts the probe reports `not applicable on <os>` with
/// `ok = true`.
///
/// The `source` argument labels which resolution arm (`--workspaces-root`,
/// env, or default) supplied the path. It is woven into the result message
/// so an operator running `g5d host --workspaces-root /custom` in one
/// shell and `g5d doctor` in another can spot a path mismatch immediately
/// (codex round 3 of PR-rev7-6).
async fn check_workspace_reflink_support(
    workspaces_root: &Path,
    source: WorkspacesRootSource,
) -> DiagnosticCheck {
    #[cfg(target_os = "linux")]
    {
        let report = probe_workspace_reflink(workspaces_root).await;
        DiagnosticCheck {
            name: "Workspace reflink".to_string(),
            ok: report.ok,
            message: format!("{} ({})", report.message, source.label()),
            required: false,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = workspaces_root;
        DiagnosticCheck {
            name: "Workspace reflink".to_string(),
            ok: true,
            message: format!(
                "not applicable on {} (probe path resolves {})",
                std::env::consts::OS,
                source.label()
            ),
            required: false,
        }
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct WorkspaceReflinkReport {
    ok: bool,
    message: String,
}

/// Concretely attempt the `cp --reflink=always` against a sentinel pair
/// inside `workspaces_root` and report the outcome. Extracted so the probe
/// can be re-driven with a tempdir in tests.
///
/// Codex round 2 / opus47 nit doctor.rs:1063-1122 of PR-rev7-6: migrated to
/// `tokio::process::Command` + `tokio::fs::*` for consistency with the
/// supervisor's own materialize-time spawn path. Doctor's `execute` is
/// async, so the prior `std::process::Command` was a synchronous block in
/// an async context.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
async fn probe_workspace_reflink(workspaces_root: &Path) -> WorkspaceReflinkReport {
    if let Err(e) = tokio::fs::create_dir_all(workspaces_root).await {
        return WorkspaceReflinkReport {
            ok: false,
            message: format!(
                "{} (mkdir failed: {e}) — set G5D_HOST_STATE_DIR or verify the worker image's \
                 /var/g5d-data/ tree is present",
                workspaces_root.display()
            ),
        };
    }
    let probe_dir = workspaces_root.join(".g5d-reflink-probe");
    let _ = tokio::fs::remove_dir_all(&probe_dir).await;
    if let Err(e) = tokio::fs::create_dir_all(&probe_dir).await {
        return WorkspaceReflinkReport {
            ok: false,
            message: format!(
                "{} (probe dir create failed: {e})",
                workspaces_root.display()
            ),
        };
    }
    let src = probe_dir.join("src");
    let dst = probe_dir.join("dst");
    if let Err(e) = tokio::fs::write(&src, b"x").await {
        let _ = tokio::fs::remove_dir_all(&probe_dir).await;
        return WorkspaceReflinkReport {
            ok: false,
            message: format!("{} (sentinel write failed: {e})", workspaces_root.display()),
        };
    }
    // Platform clone-or-fail flag — must match workspace::materialize's
    // reflink_copy (workspace D6): GNU --reflink=always on Linux, BSD -c
    // (APFS clonefile) on macOS, where --reflink does not exist. On macOS,
    // run the same clone-capability guard as the materializer first: cp -c
    // silently falls back to copyfile(2) on non-APFS volumes, which would
    // make this probe report success where the materializer fails loudly.
    #[cfg(target_os = "macos")]
    let clone_flag = "-c";
    #[cfg(not(target_os = "macos"))]
    let clone_flag = "--reflink=always";
    #[cfg(target_os = "macos")]
    if let Err(e) = g5d_foundation::workspace::restore::darwin_assert_clone_capable(&src, &dst) {
        let _ = tokio::fs::remove_dir_all(&probe_dir).await;
        return WorkspaceReflinkReport {
            ok: false,
            message: format!(
                "{} (clone-capability guard failed: {e}) — the materializer will refuse \
                 to boot microVMs until this is fixed (see ontology g5/g5d/workspace D6)",
                workspaces_root.display()
            ),
        };
    }
    let output = TokioCommand::new("cp")
        .arg(clone_flag)
        .arg(&src)
        .arg(&dst)
        .output()
        .await;
    let result = match output {
        Ok(out) if out.status.success() => WorkspaceReflinkReport {
            ok: true,
            message: format!("reflinks ✓ at {}", workspaces_root.display()),
        },
        Ok(out) => WorkspaceReflinkReport {
            ok: false,
            message: format!(
                "cp {clone_flag} failed at {} (exit {:?}): {} — the materializer will \
                 refuse to boot microVMs until this is fixed (see ontology g5/g5d/workspace D6)",
                workspaces_root.display(),
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim(),
            ),
        },
        Err(e) => WorkspaceReflinkReport {
            ok: false,
            message: format!("could not spawn cp at {}: {e}", workspaces_root.display()),
        },
    };
    let _ = tokio::fs::remove_dir_all(&probe_dir).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use tempfile::TempDir;

    fn write_workspace(dir: &TempDir, contents: &str) {
        let g5_dir = dir.path().join(".g5");
        std::fs::create_dir_all(&g5_dir).unwrap();
        std::fs::write(g5_dir.join("workspace.yaml"), contents).unwrap();
    }

    /// RAII guard that snapshots `G5D_HOST_WORKSPACES_ROOT` on construction,
    /// clears (or sets) it for the test body, and restores the original
    /// value when dropped — including on panic.
    ///
    /// Required by codex round 4 of PR-rev7-6 (Low #2): the previous
    /// snapshot-then-restore-then-assert pattern leaked the host process's
    /// pre-existing env var into the assertion, so a `cargo test` invoked
    /// with `G5D_HOST_WORKSPACES_ROOT` set in the parent shell would fail
    /// the default-fallthrough assertion. `Drop` guarantees the restore
    /// fires even if an assertion panics, which the prior inline restore
    /// did not.
    struct EnvGuard {
        key: &'static str,
        prior: Option<String>,
    }

    impl EnvGuard {
        fn unset(key: &'static str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, prior }
        }

        fn set(key: &'static str, value: &str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, prior }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prior.take() {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    /// Test helper that mirrors the production parse path
    /// (`Cli::parse_with_source_tracking`) so unit tests exercise the same
    /// `ValueSource` injection as the real binary. Without this, the
    /// `workspaces_root_source` field stays `None` and the resolver loses
    /// the provenance signal it relies on.
    fn parse_doctor_args(argv: &[&str]) -> super::super::DoctorArgs {
        use super::super::{Cli, Command};
        let matches = Cli::command()
            .try_get_matches_from(argv)
            .expect("argv should parse for test");
        let cli = Cli::from_matches_with_sources(&matches);
        match cli.command {
            Command::Doctor(args) => args,
            other => panic!("expected Command::Doctor, got {other:?}"),
        }
    }

    /// Codex round 3 of PR-rev7-6: regression test that `g5d doctor`
    /// accepts the new `--workspaces-root` flag and that
    /// `resolve_doctor_workspaces_root` reports the correct source label.
    /// The flag is the cross-shell alignment mechanism for operators who
    /// run `g5d host --workspaces-root /custom` and `g5d doctor` in
    /// separate terminals (the host process environment is process-local and
    /// cannot reach a sibling shell).
    #[test]
    #[serial_test::serial]
    fn parses_doctor_workspaces_root_flag_and_resolves_source() {
        use std::path::PathBuf;

        let _env = EnvGuard::unset(g5d_foundation::workspace::ENV_WORKSPACES_ROOT);

        let args = parse_doctor_args(&[
            "g5d",
            "doctor",
            "--workspaces-root",
            "/tmp/custom-workspaces",
        ]);
        assert_eq!(
            args.workspaces_root,
            Some(PathBuf::from("/tmp/custom-workspaces"))
        );

        let (path, source) = resolve_doctor_workspaces_root(&args);
        assert_eq!(path, PathBuf::from("/tmp/custom-workspaces"));
        assert_eq!(source, WorkspacesRootSource::Flag);
    }

    /// With no `--workspaces-root` flag and no env var, the doctor probe
    /// must fall through to `resolved_workspaces_root()` and report the
    /// `Default` source so operators see at a glance that they are
    /// testing the compiled-in canonical path.
    #[test]
    #[serial_test::serial]
    fn resolve_doctor_workspaces_root_falls_through_to_default() {
        // EnvGuard::unset clears `G5D_HOST_WORKSPACES_ROOT` for the
        // duration of the test even if the host process had it set, and
        // restores the prior value (or absence) on drop. Codex round 4
        // of PR-rev7-6 Low #2: the prior inline-restore pattern restored
        // the env var BEFORE the assertion ran, so the default-fallthrough
        // check could trip on the just-restored value.
        let _env = EnvGuard::unset(g5d_foundation::workspace::ENV_WORKSPACES_ROOT);

        let args = parse_doctor_args(&["g5d", "doctor"]);
        let (path, source) = resolve_doctor_workspaces_root(&args);

        assert_eq!(source, WorkspacesRootSource::Default);
        assert_eq!(path, g5d_foundation::workspace::resolved_workspaces_root());
    }

    /// When the env var alone supplies the path (no CLI flag), clap
    /// populates `args.workspaces_root` from the env, and the resolver
    /// must label the source as `Env` so doctor output distinguishes a
    /// shared-shell env handoff from an explicit per-invocation flag.
    #[test]
    #[serial_test::serial]
    fn resolve_doctor_workspaces_root_reports_env_source_when_env_alone() {
        use std::path::PathBuf;

        let _env = EnvGuard::set(
            g5d_foundation::workspace::ENV_WORKSPACES_ROOT,
            "/tmp/env-workspaces",
        );

        let args = parse_doctor_args(&["g5d", "doctor"]);
        let (path, source) = resolve_doctor_workspaces_root(&args);

        assert_eq!(path, PathBuf::from("/tmp/env-workspaces"));
        assert_eq!(source, WorkspacesRootSource::Env);
    }

    /// Codex round 4 of PR-rev7-6 (Low #1): when BOTH `--workspaces-root`
    /// and `G5D_HOST_WORKSPACES_ROOT` are supplied with the same path,
    /// clap's precedence picks the CLI flag. The doctor source label MUST
    /// say `Flag` so an operator who just ran `g5d host
    /// --workspaces-root /shared` in a sibling shell can still recognize
    /// that THIS invocation took the CLI route. The prior path-comparison
    /// heuristic mislabeled this case as `Env`.
    #[test]
    #[serial_test::serial]
    fn resolve_doctor_workspaces_root_prefers_flag_when_flag_and_env_match() {
        use std::path::PathBuf;

        let _env = EnvGuard::set(
            g5d_foundation::workspace::ENV_WORKSPACES_ROOT,
            "/tmp/shared-workspaces",
        );

        let args = parse_doctor_args(&[
            "g5d",
            "doctor",
            "--workspaces-root",
            "/tmp/shared-workspaces",
        ]);
        let (path, source) = resolve_doctor_workspaces_root(&args);

        assert_eq!(path, PathBuf::from("/tmp/shared-workspaces"));
        assert_eq!(source, WorkspacesRootSource::Flag);
    }

    #[test]
    fn test_parse_git_version() {
        assert_eq!(parse_git_version("git version 2.44.0"), Some((2, 44, 0)));
        assert_eq!(parse_git_version("git version 2.20.1"), Some((2, 20, 1)));
        assert_eq!(parse_git_version("git version 1.8.3.1"), Some((1, 8, 3)));
    }

    #[test]
    fn test_parse_git_version_invalid() {
        assert_eq!(parse_git_version("not a version"), None);
        assert_eq!(parse_git_version(""), None);
    }

    #[test]
    fn test_required_checks_ignore_advisory_failures() {
        let checks = vec![
            DiagnosticCheck {
                name: "Git".to_string(),
                ok: true,
                message: "ok".to_string(),
                required: true,
            },
            DiagnosticCheck {
                name: "MicroVM".to_string(),
                ok: false,
                message: "optional".to_string(),
                required: false,
            },
        ];

        assert!(required_checks_passed(&checks));
    }

    #[test]
    fn test_workspace_default_isolation_is_not_explicit_for_doctor() {
        let dir = TempDir::new().unwrap();
        write_workspace(&dir, "workspace_id: ws-123\n");

        let requested = resolve_workspace_requested_isolation_mode(dir.path());

        assert!(requested.is_none());
    }

    #[test]
    fn test_workspace_explicit_microvm_is_required_for_doctor() {
        let dir = TempDir::new().unwrap();
        write_workspace(
            &dir,
            r#"
workspace_id: ws-123
isolation:
  mode: microvm
"#,
        );

        let requested = resolve_workspace_requested_isolation_mode(dir.path()).unwrap();

        assert!(requested.explicit);
        assert_eq!(requested.mode, IsolationMode::MicroVm);
    }

    #[test]
    fn test_workspace_explicit_process_keeps_microvm_advisory() {
        let dir = TempDir::new().unwrap();
        write_workspace(
            &dir,
            r#"
workspace_id: ws-123
isolation:
  mode: process
"#,
        );

        let requested = resolve_workspace_requested_isolation_mode(dir.path()).unwrap();
        let check = check_microvm_readiness(requested);

        assert!(requested.explicit);
        assert_eq!(requested.mode, IsolationMode::Process);
        assert!(!check.required);
    }

    #[test]
    fn test_check_workspace_remote_enabled_requires_identity_and_api_url() {
        let dir = TempDir::new().unwrap();
        write_workspace(
            &dir,
            r#"
remote:
  enabled: true
"#,
        );
        std::fs::write(dir.path().join(".g5/workspace-meta"), "").unwrap();

        let check = check_workspace(dir.path(), None);
        assert!(!check.ok);
        assert!(check.message.contains("workspace_id missing"));
        assert!(check.message.contains("api_url missing"));
    }

    #[test]
    fn test_check_workspace_remote_enabled_accepts_api_override() {
        let dir = TempDir::new().unwrap();
        write_workspace(
            &dir,
            r#"
workspace_id: ws-123
remote:
  enabled: true
"#,
        );
        std::fs::write(dir.path().join(".g5/workspace-meta"), "").unwrap();

        let check = check_workspace(dir.path(), Some("http://localhost:7501"));
        assert!(check.ok, "{}", check.message);
    }

    // ---- Platform version + nested-virt evidence ----------------------

    #[test]
    fn nested_virt_status_reports_enabled_when_intel_module_is_y() {
        let read = |path: &Path| -> io::Result<String> {
            if path == Path::new("/sys/module/kvm_intel/parameters/nested") {
                Ok("Y\n".to_string())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
            }
        };
        assert_eq!(nested_virt_status(read), "enabled (kvm_intel)");
    }

    #[test]
    fn nested_virt_status_reports_disabled_when_amd_module_is_n() {
        let read = |path: &Path| -> io::Result<String> {
            if path == Path::new("/sys/module/kvm_intel/parameters/nested") {
                Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
            } else if path == Path::new("/sys/module/kvm_amd/parameters/nested") {
                Ok("N".to_string())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
            }
        };
        assert_eq!(nested_virt_status(read), "disabled (kvm_amd=N)");
    }

    #[test]
    fn nested_virt_status_reports_enabled_when_intel_module_is_1() {
        let read = |path: &Path| -> io::Result<String> {
            if path == Path::new("/sys/module/kvm_intel/parameters/nested") {
                Ok("1".to_string())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
            }
        };
        assert_eq!(nested_virt_status(read), "enabled (kvm_intel)");
    }

    #[test]
    fn nested_virt_status_reports_module_not_loaded_when_both_missing() {
        let read =
            |_: &Path| -> io::Result<String> { Err(io::Error::new(io::ErrorKind::NotFound, "x")) };
        assert_eq!(nested_virt_status(read), "kvm module not loaded");
    }

    #[test]
    fn nested_virt_status_distinguishes_permission_denied_from_missing() {
        let read = |path: &Path| -> io::Result<String> {
            if path == Path::new("/sys/module/kvm_intel/parameters/nested") {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "perm"))
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
            }
        };
        let s = nested_virt_status(read);
        assert!(
            s.starts_with("unreadable (kvm_intel:"),
            "permission-denied must surface as `unreadable`, got: {s}"
        );
    }

    #[test]
    fn nested_virt_status_reports_unknown_for_non_y_n_value() {
        let read = |path: &Path| -> io::Result<String> {
            if path == Path::new("/sys/module/kvm_intel/parameters/nested") {
                Ok("maybe".to_string())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
            }
        };
        assert_eq!(nested_virt_status(read), "unknown (kvm_intel=maybe)");
    }

    #[test]
    fn vmm_version_summary_runs_without_panic() {
        let summary = vmm_version_summary();
        if cfg!(feature = "cloud-hypervisor") {
            let s = summary.expect("cloud-hypervisor feature on => summary must be Some");
            assert!(
                s.starts_with("Cloud Hypervisor=") || s.starts_with("Cloud Hypervisor probe failed:"),
                "summary must start with Cloud Hypervisor version or a probe-failure string: got {s}"
            );
        } else {
            assert!(summary.is_none());
        }
    }

    // ---- D32: workspace host layout probe ---------------------------------

    /// Helper: env-file content covering every key in `HOST_UP_ENV_KEYS` (a
    /// mix of uncommented values and commented placeholders), matching the
    /// shape that the Packer template at
    /// `infra/packer/g5-worker/files/g5d-host.env` ships.
    #[cfg(target_os = "linux")]
    fn complete_host_up_env_content() -> String {
        let mut content = String::from("# Test fixture for host env file\n");
        for (i, key) in HOST_UP_ENV_KEYS.iter().enumerate() {
            if i % 2 == 0 {
                content.push_str(&format!("{key}=value-{i}\n"));
            } else {
                content.push_str(&format!("# {key}=\n"));
            }
        }
        content
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn host_up_layout_reports_missing_runtime_layout_and_env_file() {
        let dir = TempDir::new().unwrap();
        let report = collect_host_up_layout(
            &dir.path().join("missing-runtime"),
            &dir.path().join("missing.env"),
            |_| SystemdUnitStatus("systemd not detected".to_string()),
        );
        assert!(
            !report.ok,
            "missing layout must aggregate to ok=false (advisory finding): {}",
            report.message
        );
        assert!(
            report.message.contains("runtime_bin_layout=✗"),
            "missing runtime should be marked ✗: {}",
            report.message
        );
        assert!(
            report.message.contains("env_file=✗"),
            "missing env file should be marked ✗: {}",
            report.message
        );
        assert!(report.message.contains("systemd_unit=systemd not detected"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn host_up_layout_reports_present_runtime_layout_and_env_file() {
        let dir = TempDir::new().unwrap();
        let runtime_bin = dir.path().join("runtime/bin");
        std::fs::create_dir_all(&runtime_bin).unwrap();
        std::fs::write(runtime_bin.join("g5d"), b"#!/bin/sh\n").unwrap();
        let env_file = dir.path().join("host.env");
        std::fs::write(&env_file, complete_host_up_env_content()).unwrap();

        let report = collect_host_up_layout(&runtime_bin, &env_file, |unit| {
            assert_eq!(unit, "g5d-host.service");
            SystemdUnitStatus("enabled".to_string())
        });
        assert!(
            report.ok,
            "present layout must aggregate to ok=true: {}",
            report.message
        );
        assert!(
            report.message.contains("runtime_bin_layout=✓"),
            "present g5d binary should be marked ✓: {}",
            report.message
        );
        assert!(
            report.message.contains("env_file=✓"),
            "present env file should be marked ✓: {}",
            report.message
        );
        assert!(report.message.contains("systemd_unit=enabled"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn host_up_layout_reports_missing_env_keys_as_advisory_failure() {
        // Runtime layout is fine, but the env file is missing 3 of the 8
        // expected keys — operator template drift. The probe must aggregate
        // ok=false so the doctor footer counts an advisory finding.
        let dir = TempDir::new().unwrap();
        let runtime_bin = dir.path().join("runtime/bin");
        std::fs::create_dir_all(&runtime_bin).unwrap();
        std::fs::write(runtime_bin.join("g5d"), b"#!/bin/sh\n").unwrap();
        let env_file = dir.path().join("host.env");
        // Only 5 of 8 keys present.
        let mut content = String::new();
        for key in &HOST_UP_ENV_KEYS[..5] {
            content.push_str(&format!("{key}=\n"));
        }
        std::fs::write(&env_file, content).unwrap();

        let report = collect_host_up_layout(&runtime_bin, &env_file, |_| {
            SystemdUnitStatus("enabled".to_string())
        });
        assert!(
            !report.ok,
            "missing env keys must aggregate to ok=false: {}",
            report.message
        );
        assert!(
            report.message.contains("missing_keys:"),
            "missing-key listing must surface in evidence: {}",
            report.message
        );
        // All three missing keys must be enumerated.
        for missing_key in &HOST_UP_ENV_KEYS[5..] {
            assert!(
                report.message.contains(missing_key),
                "missing key {missing_key} must be listed: {}",
                report.message
            );
        }
    }

    #[test]
    fn host_up_env_key_present_accepts_commented_placeholders() {
        // Mirrors the Packer template's shape: a mix of uncommented values
        // (operator-populated) and commented placeholders (build-time
        // defaults). Both must count as "present" so doctor only flags
        // genuine template drift.
        let content = r#"
# G5_CONTROL_PLANE_URL=
G5_HOST_JWT=
# G5D_HOST_ID=
G5_HOST_REGION=
G5_RUNTIME_NODE=/opt/g5-host/bin/node
G5D_HOST_RUNTIME_BIN_DIR=/var/g5d-data/runtime/bin
G5_RUNTIME_LIB_DIR=/opt/g5-host/libs
RUST_LOG=info
"#;

        for key in HOST_UP_ENV_KEYS {
            assert!(
                host_up_env_key_present(content, key),
                "key {key} must be detected as present (commented or uncommented)"
            );
        }
    }

    #[test]
    fn host_up_env_key_present_rejects_unrelated_keys() {
        let content = "OTHER_KEY=value\nG5_CONTROL_PLANE=missing-underscore\n";
        assert!(!host_up_env_key_present(content, "G5_HOST_JWT"));
        // Substring of a real key but with different suffix must NOT match.
        assert!(!host_up_env_key_present(content, "G5_CONTROL_PLANE_URL"));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn host_up_probe_reports_not_applicable_on_non_linux() {
        let dir = TempDir::new().unwrap();
        let check = check_host_up_supervisor(dir.path(), dir.path());
        assert!(check.ok);
        assert!(!check.required);
        assert!(
            check.message.starts_with("not applicable on "),
            "non-Linux probe must report not-applicable: got {}",
            check.message
        );
    }

    // ---- PR-rev7-6 §5.4: workspace reflink probe ---------------------------

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn workspace_reflink_probe_reports_failure_against_non_reflink_tmpfs_on_linux() {
        // tempfile gives us /tmp/... which on most Linux CI hosts is tmpfs
        // (no reflink) or ext4 (no reflink unless reflink=1 was set). Either
        // way `cp --reflink=always` MUST fail; the probe MUST report
        // ok=false and surface the captured stderr.
        let dir = TempDir::new().unwrap();
        let check =
            check_workspace_reflink_support(dir.path(), WorkspacesRootSource::Default).await;
        // We allow the rare CI case where the tempdir IS on a reflink-
        // capable filesystem (XFS+reflink=1 or btrfs). In that case
        // ok=true is correct and the probe message announces ✓.
        assert_eq!(check.name, "Workspace reflink");
        assert!(!check.required);
        if check.ok {
            assert!(
                check.message.contains("reflinks ✓"),
                "ok=true should mention reflinks ✓: {}",
                check.message
            );
        } else {
            assert!(
                check.message.contains("cp --reflink=always failed")
                    || check.message.contains("mkdir failed"),
                "ok=false should explain reflink/cp failure: {}",
                check.message
            );
            // Operator-actionable hint must point at the materializer.
            assert!(
                check.message.contains("g5/g5d/workspace D6")
                    || check.message.contains("mkdir failed"),
                "non-reflink message should reference the ontology rule: {}",
                check.message
            );
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn workspace_reflink_probe_is_not_applicable_off_linux() {
        let dir = TempDir::new().unwrap();
        let check =
            check_workspace_reflink_support(dir.path(), WorkspacesRootSource::Default).await;
        assert!(check.ok);
        assert!(!check.required);
        assert!(check.message.starts_with("not applicable on "));
        // Codex round 3 of PR-rev7-6: the resolution-source label must
        // appear even on the non-applicable arm so cross-shell mismatches
        // are diagnosable from `g5d doctor` output alone.
        assert!(
            check.message.contains("via default"),
            "non-applicable probe must still label the resolution source: {}",
            check.message
        );
    }

    // ---- D31: probes are wired into the execute() check list --------------

    #[test]
    fn microvm_message_contains_vmm_and_nested_virt_when_supported() {
        // We can't construct a `RequestedIsolationMode` literal from the
        // outside (`explicit` is private), so call through the standard
        // resolver with a workspace that's explicit-MicroVm.
        let dir = TempDir::new().unwrap();
        write_workspace(
            &dir,
            r#"
workspace_id: ws-evidence
isolation:
  mode: microvm
"#,
        );
        let mode = resolve_workspace_requested_isolation_mode(dir.path()).unwrap();
        let check = check_microvm_readiness(mode);
        if cfg!(feature = "cloud-hypervisor") {
            assert!(
                check.message.contains("Cloud Hypervisor=")
                    || check.message.contains("Cloud Hypervisor probe failed:"),
                "MicroVM message must include Cloud Hypervisor version evidence: got {}",
                check.message
            );
        }
        if cfg!(target_os = "linux") {
            assert!(
                check.message.contains("nested-virt="),
                "MicroVM message must include nested-virt status on Linux: got {}",
                check.message
            );
        }
    }
}
