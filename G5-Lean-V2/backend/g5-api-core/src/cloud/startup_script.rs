//! Worker-host bootstrap startup script (PR-rev-7-9c).
//!
//! [`build_startup_script`] returns the bash script placed in the GCE
//! instance's `metadata.items[startup-script]` (ontology [D19]/[D21]). On first
//! boot of the signed worker image (PR #114) the script writes the control-plane
//! env file the baked-in `g5d-host.service` systemd unit reads, then enables
//! and restarts that unit. The unit itself — kernel, VMM, the G5
//! runtime binaries — is part of the image; this script only plumbs the
//! per-host identity through and flips the service on.
//! Guest image preparation is owned by
//! `g5d-isolation/src/isolation/microvm.rs`.
//!
//! Contract: `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md` [D21].

use g5_contracts::types::worker_host::{HostDiskAllocation, WORKER_HOST_CACHE_DOOR_GB};
use uuid::Uuid;

/// Path to the env file the PR #114 systemd unit reads on start.
const HOST_UP_ENV_PATH: &str = "/etc/g5/host.env";
/// The systemd unit baked into the signed worker image.
const HOST_UP_SERVICE: &str = "g5d-host.service";
/// Systemd drop-in path used by first-boot bootstrap as a compatibility guard
/// for image-family drift. Newer images bake the same value into the unit.
const HOST_UP_DROPIN_DIR: &str = "/etc/systemd/system/g5d-host.service.d";
const HOST_UP_LIMITS_DROPIN_PATH: &str = "/etc/systemd/system/g5d-host.service.d/10-limits.conf";
/// Descriptor budget for concurrent host dispatch, Git and image preparation.
const HOST_UP_NOFILE_LIMIT: &str = "1048576";
/// Default isolation mode for API-provisioned workspace hosts. Every host the
/// operator API provisions boots one Cloud Hypervisor guest per attached workspace, the
/// same runtime substrate used by the CI E2E lane. The Terraform-bootstrapped
/// `module "worker_host"` writes its own equivalent environment file.
const HOST_ISOLATION_MODE: &str = "microvm";
/// Default customer `base_image` the supervisor passes to workspace VMs.
/// **Required** — the supervisor fail-closes with "base_image is required"
/// when missing (`g5d/src/cli/host.rs`). Mirrors the persistent
/// `module "worker_host"`'s default (`infra/modules/worker-host/variables.tf`).
/// A DockerHub OCI reference; used when `build_startup_script`'s `base_image_ref`
/// is `None`. A digest-pinned customer ref is threaded from
/// `worker_hosts.base_image_ref` (ontology [D42]). The default must carry both
/// git and a native C build toolchain: the in-VM `microvm` worker clones inside
/// the guest ([D21]), and agent-run Rust/native package tests need `cc`, `ld`,
/// `make`, `pkg-config`, and common development headers in the guest base image.
/// Bare `ubuntu:26.04` has neither git nor the test linker stack; the slimmer
/// `buildpack-deps:resolute-scm` has git but no `cc`. The [D42] digest-pin rule
/// governs only the customer `base_image_ref`, not this internal default tag.
/// Pinned to the 26.04 (glibc 2.43) floor the mounted runtime now requires
/// (microvm_execution_boundary [D12]); raised from `buildpack-deps:noble`.
const DEFAULT_BASE_IMAGE: &str = "buildpack-deps:resolute";
/// The host executes its own bundle outside the writable data tree (SEC-C01).
/// RUNTIME_BIN_DIR separately identifies the guest export. Keep these defaults
/// aligned with the Packer wrapper and host.env; bootstrap replaces host.env.
const RUNTIME_NODE_PATH: &str = "/opt/g5-host/bin/node";
const RUNTIME_BIN_DIR: &str = "/var/g5d-data/runtime/bin";
const RUNTIME_LIB_DIR: &str = "/opt/g5-host/libs";
/// In-process OCI extract's default cache root (`/var/cache/g5d/oci-layers`)
/// lives on the signed image's **read-only** root fs, so any attempt to extract
/// a customer `base_image` (e.g. `ubuntu:24.04`) fails with `EROFS`. Pin
/// `G5D_OCI_CACHE_DIR` to the writable `/var/g5d-data` volume — the same value
/// the persistent module's `host-bootstrap.sh.tpl` uses for the same reason.
const OCI_CACHE_DIR: &str = "/var/g5d-data/cache/oci-layers";
/// Default `RUST_LOG` — `info` global with `g5d=debug` so the supervisor's
/// own audit/lifecycle events are visible without flooding the journal with
/// dependency tracing.
const RUST_LOG: &str = "info,g5d=debug";
/// Env: cache door port for API-provisioned worker hosts (ontology
/// `g5.runtime.cache.door`). A positive port writes `G5D_CACHE_DOOR_PORT`
/// into `/etc/g5/host.env` and publishes the guest profile at
/// [`CACHE_PROFILE_PATH`]; unset or `0` leaves both out, so hosts behave as
/// before the door existed.
pub const ENV_WORKER_CACHE_DOOR_PORT: &str = "G5_WORKER_CACHE_DOOR_PORT";
/// Env: nested virtualization for API-provisioned worker hosts (ontology
/// `g5.architecture.microvm_backend.nested`). `1`/`true` writes
/// `G5D_NESTED_VIRTUALIZATION=on` into `/etc/g5/host.env`, so every workspace
/// VM the host boots exposes hardware virtualization and `/dev/kvm` to its
/// guest; unset or anything else leaves the line out and hosts keep nested
/// virtualization off. Like the cache door, only hosts provisioned afterwards
/// change, and the host refuses the switch without a KVM-enabled platform kernel.
pub const ENV_WORKER_NESTED_VIRTUALIZATION: &str = "G5_WORKER_NESTED_VIRTUALIZATION";
/// Env: the Artifact Registry prefix (`<host>/<project>/<repository>`) whose
/// content-keyed base tiers the door serves to guests as
/// `G5_BASE_IMAGE_REGISTRY`.
pub const ENV_WORKER_CACHE_REGISTRY_PREFIX: &str = "G5_WORKER_CACHE_REGISTRY_PREFIX";
/// Env: object-storage bucket holding the fleet sccache objects
/// (ontology `g5.runtime.cache.fleet`); the organization's prefix is derived
/// per host from [`ENV_WORKER_SCCACHE_PREFIX`] and the host's organization.
pub const ENV_WORKER_SCCACHE_BUCKET: &str = "G5_WORKER_SCCACHE_BUCKET";
/// Env: prefix inside that bucket under which `orgs/<org_id>` is scoped.
pub const ENV_WORKER_SCCACHE_PREFIX: &str = "G5_WORKER_SCCACHE_PREFIX";
/// Where bring-up publishes the host's cache profile: beside the runtime disk
/// in the runtime root (ontology `g5.runtime.cache.profile`). Process-mode
/// guests bind-mount that root at `/opt/g5`; microVM guests receive the file
/// on their per-VM bootstrap disk and the trusted init binds it over the
/// read-only runtime disk's placeholder, so every guest reads
/// `/opt/g5/cache-profile.env`. Bring-up runs as root outside the host
/// service's sandbox, which has this directory read-only.
pub const CACHE_PROFILE_PATH: &str = "/var/g5d-data/runtime/cache-profile.env";
/// The passt alias every guest resolves to the host's loopback.
pub const CACHE_DOOR_ALIAS: &str = "g5-host";
/// Tiers delivery publishes to the mirror and the door pulls through.
pub const CACHE_MIRROR_TIERS: &str = "base base-rust base-rust-warm base-node";
const DEFAULT_SCCACHE_PREFIX: &str = "guest-sccache";

/// The cache door a host runs, as bring-up resolved it from the API's
/// environment and the host's organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCacheDoor {
    pub port: u16,
    pub registry_prefix: Option<String>,
    pub sccache_bucket: Option<String>,
    /// Already organization-scoped: `<prefix>/orgs/<org_id>`.
    pub sccache_prefix: Option<String>,
}

fn plain_value(value: Option<String>) -> Option<String> {
    let value = value?.trim().trim_matches('/').to_string();
    if value.is_empty() {
        return None;
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-' || b == b'/')
    {
        tracing::warn!(
            value,
            "cache door setting carries characters outside [A-Za-z0-9._/-]; ignored"
        );
        return None;
    }
    Some(value)
}

/// Resolve the nested-virtualization switch for newly provisioned hosts.
pub fn worker_nested_virtualization(env: impl Fn(&str) -> Option<String>) -> bool {
    env(ENV_WORKER_NESTED_VIRTUALIZATION)
        .map(|value| value.trim().eq_ignore_ascii_case("true") || value.trim() == "1")
        .unwrap_or(false)
}

/// Resolve the host's cache door from the environment (`None` = no door).
/// The sccache prefix is scoped to the organization here, so a host can
/// only ever be told about its own organization's fleet objects.
pub fn worker_cache_door(
    env: impl Fn(&str) -> Option<String>,
    org_id: Uuid,
) -> Option<HostCacheDoor> {
    let port = env(ENV_WORKER_CACHE_DOOR_PORT)?;
    let port = port.trim();
    if port.is_empty() {
        return None;
    }
    let port: u16 = match port.parse() {
        Ok(port) if port > 0 => port,
        _ => {
            tracing::warn!(
                value = port,
                "G5_WORKER_CACHE_DOOR_PORT is not a port number; provisioning without a cache door"
            );
            return None;
        }
    };
    let sccache_bucket = plain_value(env(ENV_WORKER_SCCACHE_BUCKET));
    let sccache_prefix = sccache_bucket.as_ref().map(|_| {
        format!(
            "{}/orgs/{org_id}",
            plain_value(env(ENV_WORKER_SCCACHE_PREFIX))
                .unwrap_or_else(|| DEFAULT_SCCACHE_PREFIX.to_string())
        )
    });
    Some(HostCacheDoor {
        port,
        registry_prefix: plain_value(env(ENV_WORKER_CACHE_REGISTRY_PREFIX)),
        sccache_bucket,
        sccache_prefix,
    })
}

/// Build the first-boot startup script for a worker host.
///
/// `region` is reported as `G5_HOST_REGION` (used for fleet/cost queries and
/// observability). `max_microvms` becomes `G5D_HOST_MAX_MICROVMS` — the host's
/// total concurrent VM budget: the request's `workspaceVmLimit` when supplied,
/// else the instance's vCPU count (ontology [D53]). The equal-slot CPU, RAM and
/// scratch values persisted on the host row are emitted explicitly because the
/// supervisor rejects a `VmSpec` whose size differs from its configuration.
///
/// `base_image_ref` is the digest-pinned **customer OCI base image for
/// microVMs** (`worker_hosts.base_image_ref`, ontology [D42]) written as
/// `G5D_HOST_BASE_IMAGE`. `None` ⇒ the `buildpack-deps:resolute` default
/// ([`DEFAULT_BASE_IMAGE`]). This is NOT the GCE boot-disk image
/// (`cloud.worker_image()`); it is never passed to `provision_instance`'s image
/// arg. The value goes through `shell_single_quote` like every other
/// interpolated value, so a hostile ref cannot break out of the assignment.
///
/// The full env set written matches the persistent `module "worker_host"`'s
/// `host-bootstrap.sh.tpl` line-for-line (see infra/modules/worker-host),
/// minus the legacy `G5_API_KEY`/`G5D_REGISTRATION_TOKEN` (replaced by the
/// per-host `host_jwt` minted by the Worker API) — Kata-vs-API parity sweep
/// from the PR #277 live-monitor session, chat intent C3.
///
/// # Security — `host_jwt` in instance metadata
///
/// The `host_jwt` is sensitive (it grants worker-host scope) and is readable
/// via `gcloud compute instances describe`. Plumbing it through instance
/// metadata is nonetheless acceptable — but the load-bearing reason is
/// DB-liveness gating, NOT GCP IAM partitioning (ontology [D21], opus47/codex
/// round-2 correction).
///
/// 1. GCE instance metadata is governed by **GCP IAM**, not G5 org roles.
///    Any principal with `compute.instances.get` permission on the project
///    can read this metadata across every org's instances in the project.
///    G5 multi-tenancy lives at the VPC + label + DB layer, not at the GCP
///    project boundary. We accept this risk because (a) the GCP project is
///    org-administered and accessed only by the deploying organisation's
///    cloud operators, (b) the token below is bound to a single host_id
///    and the `/api/internal/host/` surface only ([D4]/[D5]), and (c) the
///    DB-liveness gate makes a leaked-but-stale token inert.
/// 2. The token only authorizes operations for THIS specific host on the
///    `/api/internal/host/` surface ([D4]/[D5]); it carries no broader
///    capability and cannot reach user or worker-session endpoints.
/// 3. The moment the host record leaves `provisioning`/`ready` (terminated
///    or draining), the DB-liveness check ([D5]) rejects the token
///    regardless of signature validity — so a leaked-but-stale token is
///    inert. **This is the actual security property; the IAM framing in
///    earlier ontology revisions was wrong.**
///
/// No *other* secret (DB credentials, customer secrets, signing keys) is ever
/// placed in the script — those arrive from the control plane at task-assignment
/// time over an authenticated channel (plan §3.2).
#[must_use]
#[allow(clippy::too_many_arguments)] // one arg per env-file knob; the list mirrors host-bootstrap.sh.tpl's interpolations and each call site is a single bring-up path
pub fn build_startup_script(
    host_id: Uuid,
    host_jwt: &str,
    g5_api_url: &str,
    region: &str,
    allocation: HostDiskAllocation,
    base_image_ref: Option<&str>,
    cache_door: Option<&HostCacheDoor>,
    nested_virtualization: bool,
) -> String {
    // Single-quote every interpolated value and escape embedded single quotes
    // so a hostile/odd value cannot break out of the heredoc-free `cat` write
    // or the env assignments. The JWT is base64url + dots (no quotes), and the
    // URL/UUID are likewise quote-free in practice, but we escape defensively.
    let host_id = shell_single_quote(&host_id.to_string());
    let host_jwt = shell_single_quote(host_jwt);
    let g5_api_url = shell_single_quote(g5_api_url);
    let region = shell_single_quote(region);
    let base_image = shell_single_quote(base_image_ref.unwrap_or(DEFAULT_BASE_IMAGE));
    let HostDiskAllocation {
        vm_disk_mib,
        slots: max_microvms,
        ..
    } = allocation;
    // The door must never spend what one VM start needs free: the host's
    // start gate requires the recorded V, so that is the door's free-space
    // floor on this host.
    let door_min_free_bytes = u64::from(vm_disk_mib) << 20;
    let scratch_disk_mib = shell_single_quote(&vm_disk_mib.to_string());
    // Cache door (ontology g5.runtime.cache.door): the host.env keys the
    // supervisor reads and the guest profile published into the runtime
    // root. Values are validated to [A-Za-z0-9._/-] in `worker_cache_door`,
    // so the profile is plain KEY=value lines every guest reader can parse.
    let (cache_env_lines, cache_profile_block) = match cache_door {
        Some(door) => {
            // The byte bound is the share the boot disk sizing reserves for
            // the door, from the same constant, so the two cannot drift.
            let mut env = format!(
                "G5D_CACHE_DOOR_PORT={}\nG5D_CACHE_DOOR_MAX_BYTES={}\nG5D_CACHE_DOOR_MIN_FREE_BYTES={door_min_free_bytes}\n",
                door.port,
                u64::from(WORKER_HOST_CACHE_DOOR_GB) << 30
            );
            let mut profile = format!("G5_CACHE_DOOR=http://{CACHE_DOOR_ALIAS}:{}\n", door.port);
            if let Some(prefix) = &door.registry_prefix {
                // The whole prefix: the door serves only repositories under
                // it, never everything the host identity may read.
                env.push_str(&format!(
                    "G5D_CACHE_REGISTRY_UPSTREAMS={}\n",
                    shell_single_quote(prefix)
                ));
                profile.push_str(&format!(
                    "G5_BASE_IMAGE_REGISTRY={CACHE_DOOR_ALIAS}:{}/{prefix}\nG5_BASE_IMAGE_MIRROR_TIERS={CACHE_MIRROR_TIERS}\n",
                    door.port
                ));
            }
            if let (Some(bucket), Some(prefix)) = (&door.sccache_bucket, &door.sccache_prefix) {
                env.push_str(&format!(
                    "G5D_CACHE_SCCACHE_BUCKET={}\nG5D_CACHE_SCCACHE_PREFIX={}\n",
                    shell_single_quote(bucket),
                    shell_single_quote(prefix)
                ));
            }
            let block = format!(
                "\n# Cache door profile (ontology g5.runtime.cache.profile): every guest\n\
                 # reads it read-only at /opt/g5/cache-profile.env, projected per VM boot.\n\
                 mkdir -p {dir}\n\
                 tmp_profile=\"$(mktemp {path}.XXXXXX)\"\n\
                 cat > \"$tmp_profile\" <<'EOF'\n{profile}EOF\n\
                 chmod 0644 \"$tmp_profile\"\n\
                 chown root:root \"$tmp_profile\"\n\
                 mv -f \"$tmp_profile\" {path}\n\
                 log \"wrote {path}\"\n",
                dir = CACHE_PROFILE_PATH
                    .rsplit_once('/')
                    .map(|(d, _)| d)
                    .unwrap_or("/"),
                path = CACHE_PROFILE_PATH,
                profile = profile,
            );
            (env, block)
        }
        None => (String::new(), String::new()),
    };
    format!(
        r#"#!/usr/bin/env bash
# G5 worker-host bootstrap (PR-rev-7-9c). Runs once on first boot of the signed
# G5 worker image. Plumbs the per-host identity into the control-plane env file
# and starts the g5d host supervisor unit baked into the image.
set -euo pipefail

log() {{
  # Dual sink: stdout (GCP serial console) + syslog (journald / rsyslog).
  echo "[g5d-host] $*"
  logger -t g5d-host "$*" || true
}}

log "starting worker-host bootstrap"

umask 077
mkdir -p /etc/g5

# Write the control-plane env file (mode 0600, root-owned) with the exact keys
# the {service} unit reads. Written atomically via a temp file + rename.
#
# Env layout matches the persistent `module "worker_host"`'s
# host-bootstrap.sh.tpl. Notable required keys:
#   * G5D_HOST_BASE_IMAGE  — prepare this template before host registration.
#   * G5D_OCI_CACHE_DIR    — default `/var/cache/g5d/oci-layers` is on the RO
#                             root fs; extracting a customer base_image would
#                             fail with EROFS. Point at the writable /var/g5d-data.
#   * G5_RUNTIME_NODE/BIN_DIR/LIB_DIR — bundled node + libs paths in the image.
#   * G5D_ISOLATION_MODE=microvm — one Cloud Hypervisor guest per attached workspace.
tmp_env="$(mktemp /etc/g5/host.env.XXXXXX)"
cat > "$tmp_env" <<EOF
G5D_HOST_ID={host_id}
G5_HOST_JWT={host_jwt}
G5_CONTROL_PLANE_URL={g5_api_url}
G5_API_URL={g5_api_url}
G5D_API_URL={g5_api_url}
G5_HOST_REGION={region}
G5D_HOST_BASE_IMAGE={base_image}
G5D_HOST_MAX_MICROVMS={max_microvms}
G5D_SCRATCH_DISK_MIB={scratch_disk_mib}
G5D_ISOLATION_MODE={isolation_mode}
G5_RUNTIME_NODE={runtime_node}
G5D_HOST_RUNTIME_BIN_DIR={runtime_bin_dir}
G5_RUNTIME_LIB_DIR={runtime_lib_dir}
G5D_OCI_CACHE_DIR={oci_cache_dir}
RUST_LOG={rust_log}
{cache_env_lines}{nested_env_line}EOF
chmod 0600 "$tmp_env"
chown root:root "$tmp_env"
mv -f "$tmp_env" {env_path}
log "wrote {env_path}"
{cache_profile_block}
# Ensure the host supervisor has the same fd budget even if the instance boots
# from an older image family whose baked unit predates LimitNOFILE. The drop-in
# is idempotent and harmless when the unit already carries the same value.
mkdir -p {dropin_dir}
cat > {limits_dropin_path} <<EOF
[Service]
LimitNOFILE={nofile_limit}
EOF
chmod 0644 {limits_dropin_path}
log "wrote {limits_dropin_path}"

# Enable + restart the host supervisor unit (baked into the image). restart
# matters for older image-family boots where the unit may already be active
# before this drop-in lands; enable --now would not refresh that process limit.
systemctl daemon-reload
systemctl enable {service}
systemctl restart {service}
log "enabled and restarted {service}"

log "worker-host bootstrap complete"
"#,
        service = HOST_UP_SERVICE,
        env_path = HOST_UP_ENV_PATH,
        dropin_dir = HOST_UP_DROPIN_DIR,
        limits_dropin_path = HOST_UP_LIMITS_DROPIN_PATH,
        nofile_limit = HOST_UP_NOFILE_LIMIT,
        host_id = host_id,
        host_jwt = host_jwt,
        g5_api_url = g5_api_url,
        region = region,
        base_image = base_image,
        max_microvms = max_microvms,
        isolation_mode = HOST_ISOLATION_MODE,
        runtime_node = RUNTIME_NODE_PATH,
        runtime_bin_dir = RUNTIME_BIN_DIR,
        runtime_lib_dir = RUNTIME_LIB_DIR,
        oci_cache_dir = OCI_CACHE_DIR,
        rust_log = RUST_LOG,
        cache_env_lines = cache_env_lines,
        cache_profile_block = cache_profile_block,
        // Nested virtualization (ontology g5.architecture.microvm_backend.nested):
        // an explicit `on` only; absent keeps the supervisor's default (off).
        nested_env_line = if nested_virtualization {
            "G5D_NESTED_VIRTUALIZATION=on\n"
        } else {
            ""
        },
    )
}

/// Wrap a value in single quotes for safe shell interpolation, escaping any
/// embedded single quote via the standard `'\''` idiom.
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allocation(slots: u32) -> HostDiskAllocation {
        HostDiskAllocation {
            host_disk_gib: 120,
            vm_disk_mib: 81920,
            slots,
            disk_gib: 120 + 80 * slots,
        }
    }

    #[test]
    fn script_contains_env_keys_and_values() {
        let host_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let script = build_startup_script(
            host_id,
            "eyJ.header.sig",
            "https://api.g5.example",
            "us-east1",
            allocation(4),
            None,
            None,
            false,
        );

        assert!(script.starts_with("#!/usr/bin/env bash"));
        assert!(script.contains("G5D_HOST_ID='11111111-1111-1111-1111-111111111111'"));
        assert!(script.contains("G5_HOST_JWT='eyJ.header.sig'"));
        assert!(script.contains("G5_CONTROL_PLANE_URL='https://api.g5.example'"));
        assert!(script.contains("G5_API_URL='https://api.g5.example'"));
        assert!(script.contains("G5D_API_URL='https://api.g5.example'"));
        assert!(script.contains("G5_HOST_REGION='us-east1'"));
        // Required by the supervisor (cli/host_up.rs:360 — "base_image is
        // required"). Caught live on us-east1 in the PR #277 monitor session;
        // pre-fix the API-provisioned host crash-looped on this exact error.
        assert!(script.contains("G5D_HOST_BASE_IMAGE='buildpack-deps:resolute'"));
        assert!(script.contains("G5D_HOST_MAX_MICROVMS=4"));
        assert!(!script.contains("G5D_HOST_CPUS_PER_VM="));
        assert!(!script.contains("G5D_HOST_MEM_MIB_PER_VM="));
        assert!(script.contains("G5D_SCRATCH_DISK_MIB='81920'\n"));
        // API-provisioned hosts default to the workspace-VM path. The legacy
        // Terraform module writes the same setting through its own template.
        assert!(script.contains("G5D_ISOLATION_MODE=microvm\n"));
        assert!(!script.contains("G5D_LLM_PROXY="));
        // Runtime paths (bundled node + libs) match the persistent module's
        // host-bootstrap.sh.tpl and the packer image layout.
        assert!(script.contains("G5_RUNTIME_NODE=/opt/g5-host/bin/node"));
        assert!(script.contains("G5D_HOST_RUNTIME_BIN_DIR=/var/g5d-data/runtime/bin"));
        assert!(script.contains("G5_RUNTIME_LIB_DIR=/opt/g5-host/libs"));
        // OCI extract default lives on RO root fs; redirect to /var/g5d-data
        // or extracting a customer base_image fails with EROFS.
        assert!(script.contains("G5D_OCI_CACHE_DIR=/var/g5d-data/cache/oci-layers"));
        assert!(script.contains("RUST_LOG=info,g5d=debug"));
        assert!(script.contains("/etc/g5/host.env"));
        assert!(script.contains("mkdir -p /etc/systemd/system/g5d-host.service.d"));
        assert!(script.contains("[Service]\nLimitNOFILE=1048576"));
        assert!(script.contains("chmod 0644 /etc/systemd/system/g5d-host.service.d/10-limits.conf"));
        assert!(script.contains("systemctl daemon-reload"));
        assert!(script.contains("systemctl enable g5d-host.service"));
        assert!(script.contains("systemctl restart g5d-host.service"));
        assert!(!script.contains("systemctl enable --now g5d-host.service"));
        // 0600 mode is the documented contract for the secret-bearing env file.
        assert!(script.contains("chmod 0600"));
    }

    /// `g5.runtime.boot.backend.process-local`: API-provisioned hosts are
    /// microVM hosts; no input can make the bootstrap emit process isolation.
    #[test]
    fn script_never_emits_process_isolation() {
        assert_eq!(HOST_ISOLATION_MODE, "microvm");
        for image in [
            None,
            Some("docker.io/library/ubuntu@sha256:abcd"),
            Some("process"),
        ] {
            let script = build_startup_script(
                Uuid::new_v4(),
                "eyJ.header.sig",
                "https://api.example.test",
                "us-east1",
                allocation(2),
                image,
                None,
                false,
            );
            assert!(
                !script.contains("G5D_ISOLATION_MODE=process"),
                "cloud hosts never run process isolation:\n{script}"
            );
            assert!(script.contains("G5D_ISOLATION_MODE=microvm\n"));
            assert!(!script.contains("G5D_ALLOW_PROCESS_ISOLATION"));
        }
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_single_quote("ab'cd"), r"'ab'\''cd'");
        assert_eq!(shell_single_quote("plain"), "'plain'");
    }

    #[test]
    fn script_is_injection_safe_for_odd_values() {
        // A value carrying a single quote + shell metacharacters must stay
        // contained inside the quoted assignment — no unquoted breakout.
        let host_id = Uuid::nil();
        let script = build_startup_script(
            host_id,
            "x'; rm -rf / #",
            "https://h",
            "us-central1",
            allocation(4),
            None,
            None,
            false,
        );
        assert!(script.contains(r"G5_HOST_JWT='x'\''; rm -rf / #'"));
    }

    #[test]
    fn script_uses_provided_base_image_overriding_default() {
        // A digest-pinned customer ref is threaded into G5D_HOST_BASE_IMAGE in
        // place of the buildpack-deps:resolute default (ontology [D42]).
        let host_id = Uuid::nil();
        let reference = format!("ghcr.io/acme/app@sha256:{}", "a".repeat(64));
        let script = build_startup_script(
            host_id,
            "eyJ.header.sig",
            "https://api.g5.example",
            "us-east1",
            allocation(4),
            Some(&reference),
            None,
            false,
        );
        assert!(script.contains(&format!("G5D_HOST_BASE_IMAGE='{reference}'")));
        // The default must NOT appear when a customer ref was supplied.
        assert!(!script.contains("G5D_HOST_BASE_IMAGE='buildpack-deps:resolute'"));
    }

    #[test]
    fn script_base_image_is_injection_safe() {
        // A hostile base_image_ref must stay contained inside the quoted
        // assignment — mirror the host_jwt injection test.
        let host_id = Uuid::nil();
        let script = build_startup_script(
            host_id,
            "eyJ.header.sig",
            "https://h",
            "us-central1",
            allocation(4),
            Some("x'; rm -rf / #"),
            None,
            false,
        );
        assert!(script.contains(r"G5D_HOST_BASE_IMAGE='x'\''; rm -rf / #'"));
    }

    #[test]
    fn script_uses_the_recorded_allocation_without_capacity_fallbacks() {
        let allocation = HostDiskAllocation {
            host_disk_gib: 160,
            vm_disk_mib: 32768,
            slots: 3,
            disk_gib: 300,
        };
        let script = build_startup_script(
            Uuid::nil(),
            "jwt",
            "https://api.example.test",
            "us-east1",
            allocation,
            None,
            None,
            false,
        );
        assert!(script.contains("G5D_HOST_MAX_MICROVMS=3\n"));
        assert!(script.contains("G5D_SCRATCH_DISK_MIB='32768'\n"));
        // H only sizes the host disk: no shared quota or reservation path.
        assert!(!script.contains("G5D_HOST_DISK_ALLOWANCE_GIB"));
        assert!(!script.contains("G5D_PRIVATE_CAPACITY_PATH"));
        assert!(!script.contains("g5-host-disk.py"));
    }

    #[test]
    fn nested_virtualization_is_written_only_when_the_deployment_turns_it_on() {
        for (value, expected) in [
            (None, false),
            (Some(""), false),
            (Some("0"), false),
            (Some("false"), false),
            (Some("on"), false),
            (Some("1"), true),
            (Some(" true "), true),
            (Some("TRUE"), true),
        ] {
            assert_eq!(
                worker_nested_virtualization(|key| {
                    assert_eq!(key, ENV_WORKER_NESTED_VIRTUALIZATION);
                    value.map(str::to_string)
                }),
                expected,
                "{value:?}"
            );
        }
        let render = |nested: bool| {
            build_startup_script(
                Uuid::nil(),
                "eyJ.header.sig",
                "https://api.g5.example",
                "us-east1",
                allocation(4),
                None,
                None,
                nested,
            )
        };
        let on = render(true);
        assert!(on.contains("\nG5D_NESTED_VIRTUALIZATION=on\nEOF"), "{on}");
        let off = render(false);
        assert!(!off.contains("G5D_NESTED_VIRTUALIZATION"), "{off}");
        // The switch adds exactly one env line and nothing else.
        assert_eq!(
            on.replace("G5D_NESTED_VIRTUALIZATION=on\n", ""),
            off,
            "nested switch must only add its env line"
        );
    }

    #[test]
    fn cache_door_writes_host_env_keys_and_the_guest_profile() {
        let org = Uuid::parse_str("58008163-1b68-4c60-a9b0-1665a8f331d9").unwrap();
        let env = |key: &str| match key {
            ENV_WORKER_CACHE_DOOR_PORT => Some("7111".to_string()),
            ENV_WORKER_CACHE_REGISTRY_PREFIX => {
                Some("us-central1-docker.pkg.dev/g5-qa-1ccc/g5-tiers".to_string())
            }
            ENV_WORKER_SCCACHE_BUCKET => Some("g5-qa-1ccc-deploy-artifacts".to_string()),
            ENV_WORKER_SCCACHE_PREFIX => Some("/guest-sccache/".to_string()),
            _ => None,
        };
        let door = worker_cache_door(env, org).unwrap();
        assert_eq!(door.port, 7111);
        assert_eq!(
            door.sccache_prefix.as_deref(),
            Some("guest-sccache/orgs/58008163-1b68-4c60-a9b0-1665a8f331d9")
        );
        let script = build_startup_script(
            Uuid::nil(),
            "eyJ.header.sig",
            "https://api.g5.example",
            "us-east1",
            allocation(4),
            None,
            Some(&door),
            false,
        );
        assert!(script.contains("G5D_CACHE_DOOR_PORT=7111\n"));
        assert!(script.contains("G5D_CACHE_DOOR_MAX_BYTES=21474836480\n"));
        // One VM start's free-space requirement at the recorded V.
        assert!(script.contains(&format!(
            "G5D_CACHE_DOOR_MIN_FREE_BYTES={}\n",
            81920_u64 << 20
        )));
        assert!(script.contains(
            "G5D_CACHE_REGISTRY_UPSTREAMS='us-central1-docker.pkg.dev/g5-qa-1ccc/g5-tiers'\n"
        ));
        assert!(script.contains("G5D_CACHE_SCCACHE_BUCKET='g5-qa-1ccc-deploy-artifacts'\n"));
        assert!(script.contains(
            "G5D_CACHE_SCCACHE_PREFIX='guest-sccache/orgs/58008163-1b68-4c60-a9b0-1665a8f331d9'\n"
        ));
        assert!(script.contains("cat > \"$tmp_profile\" <<'EOF'\nG5_CACHE_DOOR=http://g5-host:7111\nG5_BASE_IMAGE_REGISTRY=g5-host:7111/us-central1-docker.pkg.dev/g5-qa-1ccc/g5-tiers\nG5_BASE_IMAGE_MIRROR_TIERS=base base-rust base-rust-warm base-node\nEOF\n"));
        assert!(script.contains("mv -f \"$tmp_profile\" /var/g5d-data/runtime/cache-profile.env\n"));
        // The profile is written before the service (re)starts.
        assert!(
            script.find("cache-profile.env").unwrap() < script.find("systemctl restart").unwrap()
        );

        // No door: byte-for-byte the previous script shape.
        let without = build_startup_script(
            Uuid::nil(),
            "eyJ.header.sig",
            "https://api.g5.example",
            "us-east1",
            allocation(4),
            None,
            None,
            false,
        );
        assert!(!without.contains("G5D_CACHE_DOOR_PORT"));
        assert!(!without.contains("cache-profile.env"));

        // Unparseable port or hostile characters never reach a host.
        assert!(worker_cache_door(
            |k| (k == ENV_WORKER_CACHE_DOOR_PORT).then(|| "door".to_string()),
            org
        )
        .is_none());
        assert!(worker_cache_door(
            |k| (k == ENV_WORKER_CACHE_DOOR_PORT).then(|| "0".to_string()),
            org
        )
        .is_none());
        let hostile = worker_cache_door(
            |k| match k {
                ENV_WORKER_CACHE_DOOR_PORT => Some("7111".to_string()),
                ENV_WORKER_CACHE_REGISTRY_PREFIX => Some("x'; rm -rf / #".to_string()),
                _ => None,
            },
            org,
        )
        .unwrap();
        assert_eq!(hostile.registry_prefix, None);
    }
}
