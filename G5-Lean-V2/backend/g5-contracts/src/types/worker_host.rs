//! `WorkerHost` DTO and `WorkerHostStatus` enum.
//!
//! Mirrors the `worker_hosts` table (`g5-api` migration 078) defined under
//! `aux_files/future_implementation/microvm_completion_plan.md` §5.7 and
//! `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md` [D1].
//!
//! Lives in `g5-common` (not `g5-api`) because both the API server and `g5d`
//! eventually read or construct these values: the API server persists rows,
//! `g5d` reports its own host record back during heartbeat / capability
//! reporting in a later PR.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WorkerHostStatus
// ---------------------------------------------------------------------------

/// Lifecycle state of a worker host (bare cloud VM).
///
/// `unreachable` is distinct from intentional draining: missed heartbeats do
/// not themselves authorize teardown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkerHostStatus {
    /// The control plane has created the row but the cloud VM is not yet ready
    /// to accept tasks. Initial state.
    Provisioning,
    /// The host's `g5d` has reported in and the host can accept microVM
    /// bring-up requests.
    Ready,
    /// The control plane cannot currently reach the host. This is recoverable
    /// and is not an implicit drain request.
    Unreachable,
    /// The host is being drained — running tasks finish, no new tasks scheduled.
    Draining,
    /// The host is gone (deleted, terminated, or unrecoverable). Terminal state.
    Terminated,
}

impl WorkerHostStatus {
    /// Canonical wire string, matching the SQL CHECK values.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Ready => "ready",
            Self::Unreachable => "unreachable",
            Self::Draining => "draining",
            Self::Terminated => "terminated",
        }
    }

    /// Every variant, useful for tests and DB constraint verification.
    #[must_use]
    pub const fn all() -> &'static [WorkerHostStatus] {
        &[
            Self::Provisioning,
            Self::Ready,
            Self::Unreachable,
            Self::Draining,
            Self::Terminated,
        ]
    }
}

impl fmt::Display for WorkerHostStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for WorkerHostStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "provisioning" => Ok(Self::Provisioning),
            "ready" => Ok(Self::Ready),
            "unreachable" => Ok(Self::Unreachable),
            "draining" => Ok(Self::Draining),
            "terminated" => Ok(Self::Terminated),
            _ => Err(format!("unknown worker host status: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// WorkerHost — DTO mirroring the worker_hosts row
// ---------------------------------------------------------------------------

/// One worker host: a bare cloud VM that runs workspace VMs via Cloud Hypervisor.
///
/// Mirrors the `worker_hosts` table column-for-column. Nullable database
/// columns map to `Option<…>`. The `cache_state` and `attestation_state`
/// JSONB columns are kept as `serde_json::Value` for V1 — they have a
/// documented structure but are not strictly typed (see ontology [D1]).
///
/// `serde` (de)serialisation uses `camelCase` to match the existing API
/// envelope conventions.
///
/// Behind the `sqlx` feature the struct also derives `sqlx::FromRow`: every
/// field name matches a `worker_hosts` column one-for-one, so the provisioning
/// list/insert queries (ontology [D12]/[D10]) map rows directly without a
/// hand-written mapper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct WorkerHost {
    /// Stable identity (the `worker_hosts.host_id` PK). Embedded as `sub` in
    /// the host JWT.
    pub host_id: Uuid,

    /// Owning organisation. Embedded as `org` in the host JWT.
    pub org_id: Uuid,

    /// The per-org GCP VPC the host runs in. `None` until PR-rev-7-9c lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vpc_id: Option<String>,

    /// The GCE instance ID for the underlying VM. `None` until PR-rev-7-9c.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_instance_id: Option<String>,

    /// GCE machine type. Default at the DB layer: `"c3-standard-4"` (migration 130).
    pub instance_type: String,

    /// Cloud region (e.g. `"us-central1"`).
    pub region: String,

    /// Registry kind introduced by the workspace-VM runtime schema.
    #[serde(default = "default_worker_host_kind")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub kind: String,

    #[serde(default = "default_slots_total")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub slots_total: i32,
    /// True only after authenticated host registration publishes the resolved size.
    #[serde(default)]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub slot_size_reported: bool,
    #[serde(default = "default_slot_cpus")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub slot_cpus: i32,
    #[serde(default = "default_slot_mem_mib")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub slot_mem_mib: i32,
    #[serde(default = "default_scratch_disk_mib")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub scratch_disk_mib: i32,
    #[serde(default)]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub runtime_identity: serde_json::Value,
    #[serde(default)]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub capabilities: Vec<String>,

    /// Durable occupancy derived from active `workspace_vms`; never copied
    /// from a host heartbeat counter.
    #[serde(default)]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub occupancy: i64,

    /// Server-generated summary for operator displays.
    #[serde(default)]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub heartbeat_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub heartbeat_age_secs: Option<i64>,
    /// Server-derived: `current` when the published `runtime_identity`
    /// matches the control plane's bundled runtime, `stale` when it differs,
    /// `unknown` when the host has published none. Only `current` hosts
    /// receive placement (`g5.runtime.placement.runtime-identity`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub runtime_state: String,

    /// Bundled kernel banner; supports the smoke gate's kernel.sha256 chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,

    /// Count of currently-running microVMs on this host. Reported on
    /// heartbeat; persisted via `worker_hosts.active_microvm_count`
    /// (migration 083, ontology [D30]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_microvm_count: Option<i32>,

    /// Used bytes on the rootfs cache disk. Reported on heartbeat;
    /// persisted via `worker_hosts.cache_disk_used_bytes` (migration
    /// 083, ontology [D30]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_disk_used_bytes: Option<i64>,

    /// Total bytes on the rootfs cache disk. Reported on heartbeat;
    /// persisted via `worker_hosts.cache_disk_total_bytes` (migration
    /// 083, ontology [D30]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_disk_total_bytes: Option<i64>,

    /// Per-cache warm/cold flags (sccache, package caches, base repos).
    /// Structure documented but not strictly typed in V1.
    #[serde(default)]
    pub cache_state: serde_json::Value,

    /// IsolationAttestor results once the rev-7 attestor lands.
    #[serde(default)]
    pub attestation_state: serde_json::Value,

    /// Lifecycle state.
    pub status: WorkerHostStatus,

    /// Immutable creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Latest heartbeat received from the host's `g5d`. `None` until the host
    /// first reports in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub credential_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub credential_generation: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub drain_requested_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub drain_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub drain_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub last_error: Option<serde_json::Value>,

    /// Classified cloud bring-up failure code (`worker_hosts.provision_error_code`,
    /// migration 102). `None` when bring-up has not failed (or has not been
    /// attempted). The UI uses this to render a precise badge ("No capacity",
    /// future codes) instead of leaving the row stuck on "Provisioning". The
    /// codeset spans `'zone_capacity_exhausted'`, `'abandoned_bringup'`,
    /// `'never_registered'`, and `'base_image_below_glibc_floor'`
    /// (migrations 102/105/232/264); extend the column CHECK,
    /// the persistence function, the classifier, and the UI badge map in
    /// lockstep when a new code is added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision_error_code: Option<String>,

    /// Zone-stockout retry attempts consumed (`worker_hosts.provision_retry_count`,
    /// migration 228, ontology cloud_provider [D66]). `0` for rows that never
    /// retried; the UI renders "retrying (attempt N)" alongside the
    /// `zone_capacity_exhausted` badge while a retry is scheduled.
    #[serde(default)]
    pub provision_retry_count: i32,

    /// Next scheduled stockout retry (`worker_hosts.provision_next_retry_at`,
    /// migration 228, [D66]). `None` = not scheduled (never failed, claimed,
    /// or budget exhausted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision_next_retry_at: Option<DateTime<Utc>>,

    // ── Runtime / profile contract (PR1) ──────────────────────────────────
    /// Provision-time resolved digest-pinned base image ref
    /// (`<registry>/<image>@sha256:<64hex>`). `None` is retained only for
    /// legacy rows created before explicit image-source resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_ref: Option<String>,

    /// `sha256:<64 lowercase hex>` extracted from `base_image_ref`. `None` is
    /// retained only for legacy rows. Shape CHECK-enforced (migration 113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_digest: Option<String>,

    /// Provision-time image selection kind (`track` or `pin`). `None` only on
    /// legacy rows whose source cannot be reconstructed truthfully.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_source_kind: Option<String>,

    /// Exact selected track name or pinned ref paired with source kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_source_value: Option<String>,

    /// Forward-compat: runtime schema string. PR2 populates; PR1 leaves NULL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_schema: Option<String>,

    /// Forward-compat OCI/runtime labels (PR2). Defaults to `{}`; documented-but-
    /// untyped JSON in PR1 (same posture as `cache_state`).
    #[serde(default)]
    pub runtime_labels: serde_json::Value,

    /// Forward-compat: when `runtime_labels` was last refreshed. PR2 populates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_labels_refreshed_at: Option<DateTime<Utc>>,

    /// Worker profile (`fat-default` | `slim`). NOT NULL at the DB layer with a
    /// `fat-default` default, so always present on the wire.
    pub worker_profile: String,

    /// The operator's explicit provision-time concurrency pin
    /// (`worker_hosts.workspace_vm_limit`, migration 146, ontology [D54]/[D55]).
    /// `None` ⇔ column NULL ⇔ the host tracks the current default budget
    /// (`default_workspace_vm_limit`), which is resolved at bring-up/rotation
    /// time and deliberately never persisted or serialized here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_vm_limit: Option<i32>,

    /// The user who requested this host (`worker_hosts.created_by_user_id`,
    /// migration 061). A rotation replacement keeps its predecessor's
    /// creator. `None` for hosts created before the column existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub created_by_user_id: Option<Uuid>,

    /// Creator profile for operator displays, resolved by the list route
    /// from `users`; never stored on the host row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub created_by_display_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub created_by_email: Option<String>,
}

/// Provisional registry values before a host publishes its measured slot size.
/// Placement must require `slot_size_reported`; these are not bootstrap overrides.
pub const DEFAULT_WORKER_HOST_SLOT_CPUS: i32 = 2;
pub const DEFAULT_WORKER_HOST_SLOT_MEM_MIB: i32 = 4096;
pub const DEFAULT_WORKER_HOST_SCRATCH_DISK_MIB: i32 = 10240;
pub const MIN_WORKER_HOST_SCRATCH_DISK_MIB: i32 = 1024;
pub const MAX_WORKER_HOST_SCRATCH_DISK_MIB: i32 = 1073741824;

// ---------------------------------------------------------------------------
// WorkerHostBootDiskSizing (ontology g5.runtime.placement.host-disk)
// ---------------------------------------------------------------------------

/// Organization defaults, in binary GiB. Provider/API names ending in `gb`
/// are retained for wire compatibility; their values are GiB.
pub const WORKER_HOST_BOOT_DISK_BASE_GB: u32 = 120;
pub const WORKER_HOST_BOOT_DISK_PER_SLOT_GB: u32 = 80;
/// Optional source preparation's bundle, build tree and staging disk. Admission
/// checks these against actual free space; provisioning does not reserve them.
pub const WORKER_HOST_SOURCE_PREPARATION_DISKS: u32 = 3;
pub const WORKER_HOST_CACHE_DOOR_GB: u32 = 20;
pub const WORKER_HOST_BOOT_DISK_FLOOR_GB: u32 = 30;
pub const MIN_WORKER_HOST_BOOT_DISK_GB: u32 = 30;
pub const MAX_WORKER_HOST_BOOT_DISK_GB: u32 = 2000;
pub const MIN_HOST_DISK_ALLOWANCE_GIB: u32 = 30;
pub const MIN_VM_DISK_ALLOWANCE_GIB: u32 = 1;

/// Organization policy published to Add Host. All quantities are binary GiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerHostBootDiskSizing {
    pub base_gb: u32,
    pub per_slot_gb: u32,
    /// Legacy wire field; no independent provisioning floor is applied.
    pub floor_gb: u32,
    pub min_gb: u32,
    pub max_gb: u32,
}

impl Default for WorkerHostBootDiskSizing {
    fn default() -> Self {
        Self::new(
            WORKER_HOST_BOOT_DISK_BASE_GB,
            WORKER_HOST_BOOT_DISK_PER_SLOT_GB,
        )
    }
}

impl WorkerHostBootDiskSizing {
    pub fn new(host_gib: u32, vm_gib: u32) -> Self {
        Self {
            base_gb: host_gib,
            per_slot_gb: vm_gib,
            floor_gb: MIN_WORKER_HOST_BOOT_DISK_GB,
            min_gb: MIN_WORKER_HOST_BOOT_DISK_GB,
            max_gb: MAX_WORKER_HOST_BOOT_DISK_GB,
        }
    }

    /// Display the actual minimum even when unsupported. Never clamp it.
    pub fn default_gb(&self, slots: u32) -> u32 {
        self.base_gb
            .saturating_add(self.per_slot_gb.saturating_mul(slots))
    }

    pub fn exceeds_cap(&self, slots: u32) -> bool {
        self.default_gb(slots) > self.max_gb
    }
    pub fn accepts(&self, gib: u32) -> bool {
        (self.min_gb..=self.max_gb).contains(&gib)
    }

    /// Resolve once at creation. Incompatible retained overrides are errors too.
    pub fn resolve(
        &self,
        stored_gib: Option<i32>,
        slots: u32,
    ) -> Result<u32, crate::error::G5Error> {
        let minimum = self.default_gb(slots);
        let requested = stored_gib
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(minimum);
        if slots == 0
            || self.base_gb < MIN_HOST_DISK_ALLOWANCE_GIB
            || self.per_slot_gb < MIN_VM_DISK_ALLOWANCE_GIB
            || stored_gib.is_some_and(|v| v < 0)
            || minimum > self.max_gb
            || requested < minimum
            || !self.accepts(requested)
        {
            return Err(crate::error::G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "bootDiskGb", "error": "Disk must be at least H + V × slots and within supported limits (GiB); a retained override must also meet this minimum",
                    "value": stored_gib, "min": minimum, "max": self.max_gb,
                    "hostDiskAllowanceGib": self.base_gb, "vmDiskAllowanceGib": self.per_slot_gb, "slots": slots
                }),
            });
        }
        Ok(requested)
    }
}

/// Immutable managed-host allocation. V uses MiB to preserve legacy capacities
/// exactly, including values that were not whole GiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDiskAllocation {
    pub host_disk_gib: u32,
    pub vm_disk_mib: u32,
    pub slots: u32,
    pub disk_gib: u32,
}

fn default_worker_host_kind() -> String {
    "gce".to_string()
}

const fn default_slots_total() -> i32 {
    1
}

const fn default_slot_cpus() -> i32 {
    DEFAULT_WORKER_HOST_SLOT_CPUS
}

const fn default_slot_mem_mib() -> i32 {
    DEFAULT_WORKER_HOST_SLOT_MEM_MIB
}

const fn default_scratch_disk_mib() -> i32 {
    DEFAULT_WORKER_HOST_SCRATCH_DISK_MIB
}

// ---------------------------------------------------------------------------
// Provisioning REST DTOs (PR-rev-7-9b)
//
// The wire contract for the `POST/DELETE/GET /api/v1/orgs/:orgId/worker-hosts`
// endpoints. Lives beside `WorkerHost` so g5-api and future g5d/CLI consumers
// share one definition (ontology [D14]).
// ---------------------------------------------------------------------------

/// Default GCE machine type for a newly-provisioned worker host.
///
/// Mirrors the `worker_hosts.instance_type` column DEFAULT (migration 130 re-set
/// it to C3 from the original migration-078 `n2-standard-8`). Used when a
/// provision request omits (or blanks) `instance_type`. C3 is the standardized
/// worker family (worker_hosts ontology [D45]).
pub const DEFAULT_WORKER_HOST_INSTANCE_TYPE: &str = "c3-standard-4";

/// Read the vCPU count from an allowlisted standard machine type. Callers
/// validate the allowlist separately; unexpected names return `None`.
pub fn parse_vcpus_from_instance_type(instance_type: &str) -> Option<u32> {
    instance_type.rsplit('-').next()?.parse::<u32>().ok()
}

/// Default slots for the allowlisted standard machines: four vCPUs per VM,
/// with at least one slot. Settings validation and provisioning share this
/// budget so they calculate the same minimum disk size.
pub fn default_workspace_vm_limit(instance_vcpus: u32) -> u32 {
    (instance_vcpus / 4).max(1)
}

/// Machine types a worker host may be provisioned with, by Add Host or by
/// managed capacity's organization setting. The GCP path interpolates the
/// value, so the allowlist also closes path injection and bounds cost.
pub const WORKER_HOST_INSTANCE_TYPE_ALLOWLIST: &[&str] = &[
    // N2 (Intel Cascade Lake / Ice Lake) — the historical default.
    "n2-standard-4",
    "n2-standard-8",
    "n2-standard-16",
    "n2-standard-32",
    // C2 (Intel Cascade Lake) — compute-optimized; higher per-core clock.
    // Note c2-standard-30 (not 32) — that's the actual max SKU in the family.
    "c2-standard-4",
    "c2-standard-8",
    "c2-standard-16",
    "c2-standard-30",
    // C3 (Intel Emerald Rapids) — latest-gen compute-optimized; nested-virt
    // capable; third capacity pool. C3 standard SKUs are 4/8/22/44/88/176;
    // 88+176 are deliberately excluded (cost-vector ceiling — see [D41]).
    "c3-standard-4",
    "c3-standard-8",
    "c3-standard-22",
    "c3-standard-44",
];

/// GCP regions a worker host may be provisioned in (ontology [D18]).
///
/// Centralised here so the API boundary, the provider's per-region CIDR
/// lookup, and any future bring-up tooling share one source of truth. The
/// value flows into `format!`-built GCP REST paths
/// (`regions/{region}/...`, `zones/{region}-{suffix}/...`), so allowlisting
/// the request value closes the same URL-path-injection vector the
/// `instance_type` allowlist already closes (ontology [D10], `routes/worker_hosts.rs`).
pub const WORKER_HOST_REGION_ALLOWLIST: &[&str] = &[
    "us-central1",
    "us-east1",
    "us-east4",
    "us-west1",
    "us-west2",
    "europe-west1",
    "europe-west4",
    "asia-east1",
    "asia-northeast1",
];

/// Default worker profile when a provision request omits (or blanks) it.
/// Mirrors the `worker_hosts.worker_profile` column DEFAULT in migration 113.
pub const DEFAULT_WORKER_PROFILE: &str = "fat-default";

/// Worker profiles accepted at the API boundary. Single source of truth; the
/// migration-113 CHECK enforces the same set at the DB layer.
pub const WORKER_PROFILE_ALLOWLIST: &[&str] = &["fat-default", "slim"];

/// Why a base image ref was rejected. Carried into the route's 422 detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseImageRefError {
    /// The (already-trimmed) ref was empty.
    Empty,
    /// No `@sha256:` digest section — a tag-only ref like `ubuntu:24.04`. PR1
    /// accepts only digest-pinned refs (it does not resolve tags).
    MissingDigest,
    /// `@sha256:` present but the digest is not exactly 64 lowercase hex chars.
    MalformedDigest,
}

impl BaseImageRefError {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "base image ref must be non-empty",
            Self::MissingDigest => {
                "base image ref must be digest-pinned: <registry>/<image>@sha256:<64 lowercase hex>"
            }
            Self::MalformedDigest => "base image digest must be sha256:<64 lowercase hex>",
        }
    }
}

/// Validate a digest-pinned base image ref and return its `sha256:<64hex>` digest
/// token. PR1 accepts ONLY `<registry>/<image>@sha256:<64 lowercase hex>`: the
/// name must be lowercase ASCII, include a registry/image path slash, allow a
/// single `:` only in the registry host segment for `host:port`, and carry no
/// tag separator in image path segments. PR1 does NOT resolve tags or fetch OCI
/// labels (PR2). No `regex` crate — hand-written, dependency-free.
pub fn validate_base_image_ref(image_ref: &str) -> Result<String, BaseImageRefError> {
    let ref_str = image_ref.trim();
    if ref_str.is_empty() {
        return Err(BaseImageRefError::Empty);
    }
    // Split on the LAST '@' so the digest is the suffix; the name (which may
    // contain a registry host:port) is everything before it.
    let (name, digest) = ref_str
        .rsplit_once('@')
        .ok_or(BaseImageRefError::MissingDigest)?;
    validate_base_image_name(name)?;
    let hex = digest
        .strip_prefix("sha256:")
        .ok_or(BaseImageRefError::MalformedDigest)?;
    if hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(digest.to_string()) // full "sha256:<hex>" token
    } else {
        Err(BaseImageRefError::MalformedDigest)
    }
}

/// Minimum guest glibc the host-staged, dynamically linked G5 runtime requires —
/// the ABI floor every customer `base_image` must satisfy
/// (`system_invariants/microvm_execution_boundary#D12`; the `ubuntu:26.04`
/// common base ships glibc 2.43). `(major, minor)`.
pub const GUEST_GLIBC_FLOOR: (u32, u32) = (2, 43);

/// Parse the `glibc-<major>.<minor>` component out of a
/// `com.g5labs.runtime.base_image` ABI label value (for example
/// `ubuntu-26.04-glibc-2.43`). Dependency-free, mirroring
/// `validate_base_image_ref`'s no-regex posture. Returns `None` when the value
/// carries no parseable `glibc-<digits>.<digits>` token — such a value is NOT
/// floor-checkable and stays the operator's responsibility
/// (`microvm_execution_boundary#D12`).
pub fn parse_declared_glibc(base_image_label_value: &str) -> Option<(u32, u32)> {
    // A label may carry more than one `glibc-` token. Reading only the first
    // occurrence lets a malformed leading token mask a real declaration further
    // along (`glibc-invalid-glibc-2.39`), and because the floor checks below
    // treat `None` as "not below the floor", that masking would fail OPEN and
    // admit a guest under the floor. Parse every occurrence and keep the LOWEST
    // declared version, so an ambiguous label is judged by its weakest claim.
    base_image_label_value
        .split("glibc-")
        .skip(1)
        .filter_map(|after| {
            // Leading `<digits>[.<digits>...]` run; stop at the first char that
            // is neither an ASCII digit nor '.'.
            let token: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let mut parts = token.split('.');
            let major: u32 = parts.next()?.parse().ok()?;
            let minor: u32 = parts.next()?.parse().ok()?;
            Some((major, minor))
        })
        .min()
}

/// Canonical fail-closed message for a sub-floor guest, verbatim per
/// `system_invariants/microvm_execution_boundary#D12` / `provisioning_api#D57`.
pub fn glibc_floor_violation_message() -> String {
    format!(
        "guest below the glibc {}.{} floor — repin to a 26.04 base",
        GUEST_GLIBC_FLOOR.0, GUEST_GLIBC_FLOOR.1
    )
}

/// `true` only when the ABI label value POSITIVELY declares a glibc BELOW the
/// floor. A missing/unparseable `glibc-<x>.<y>` token or an at-or-above value
/// returns `false` (accept), so an unlabeled or non-conforming image is never
/// failed into rejection — the guard rejects only a positively-declared
/// sub-floor ABI (`provisioning_api#D57` / `microvm_execution_boundary#D12`).
pub fn base_image_below_glibc_floor(base_image_label_value: &str) -> bool {
    matches!(parse_declared_glibc(base_image_label_value), Some(v) if v < GUEST_GLIBC_FLOOR)
}

fn validate_base_image_name(name: &str) -> Result<(), BaseImageRefError> {
    let mut segments = name.split('/');
    let Some(host) = segments.next() else {
        return Err(BaseImageRefError::MissingDigest);
    };
    let path_segments: Vec<&str> = segments.collect();
    if host.is_empty() || path_segments.is_empty() || path_segments.iter().any(|s| s.is_empty()) {
        return Err(BaseImageRefError::MissingDigest);
    }

    let host_colons = host.bytes().filter(|b| *b == b':').count();
    if host_colons > 1 {
        return Err(BaseImageRefError::MissingDigest);
    }
    if let Some((host_name, port)) = host.split_once(':') {
        if host_name.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BaseImageRefError::MissingDigest);
        }
    }
    if path_segments.iter().any(|segment| segment.contains(':')) {
        return Err(BaseImageRefError::MissingDigest);
    }
    if !name.bytes().all(|b| {
        b.is_ascii_lowercase()
            || b.is_ascii_digit()
            || matches!(b, b'.' | b'-' | b'_' | b'/' | b':')
    }) {
        return Err(BaseImageRefError::MissingDigest);
    }

    Ok(())
}

/// Request body for `POST /api/v1/orgs/:orgId/worker-hosts`.
///
/// Provisions a database-only host record (PR-rev-7-9b performs no cloud
/// bring-up — see ontology [I6]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionWorkerHostRequest {
    /// Cloud region the host will run in (e.g. `"us-central1"`). Required;
    /// the handler rejects an empty/whitespace value with `422`.
    pub region: String,

    /// GCE machine type. Optional — defaults to
    /// [`DEFAULT_WORKER_HOST_INSTANCE_TYPE`] when omitted or blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_type: Option<String>,

    /// Optional digest-pinned base image (`<registry>/<image>@sha256:<64hex>`).
    /// Mutually exclusive with `imageTrack`; when both overrides are absent,
    /// the server resolves the org policy or returns 422 before side effects.
    /// Tag-only / non-digest refs also return 422.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_ref: Option<String>,

    /// Optional promoted image track. Mutually exclusive with `baseImageRef`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_track: Option<String>,

    /// Optional worker profile; omitted/blank ⇒ `fat-default`. Allowed:
    /// `fat-default`, `slim` (others → 422).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_profile: Option<String>,

    /// Optional host VM budget — how many concurrent workspace VMs (microVMs) the host
    /// runs. Omitted values track the server's current instance-size default;
    /// supplied values are validated `1 ..= vCPUs(instanceType)` (1× ceiling,
    /// no oversubscription). The explicit value is persisted for rotation and
    /// becomes `G5D_HOST_MAX_MICROVMS`; equal-slot CPU/RAM/scratch sizes are
    /// persisted separately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_vm_limit: Option<u32>,

    /// Optional boot-disk size override in GiB (legacy field name). It must
    /// be at least H + V × slots and no more than 2000 GiB (422 otherwise).
    /// Creation saves H, V, slots and resolved size together; same-host retries
    /// use that allocation. New replacements use current settings and retain
    /// an explicit override only when it still meets the calculated minimum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_disk_gb: Option<u32>,
}

/// Response body for a successful `POST /api/v1/orgs/:orgId/worker-hosts`
/// (`201 Created`).
///
/// `host_jwt` is the per-host bearer token, returned exactly once and never
/// persisted in plaintext (mirrors the worker-token create contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionWorkerHostResponse {
    /// The newly-allocated `worker_hosts.host_id`.
    pub host_id: Uuid,

    /// The minted `host_jwt` the host will present to the control plane.
    pub host_jwt: String,

    /// The saved allocation every launcher must use, including database-only
    /// development hosts. Later organization settings do not change it.
    pub disk_allocation: HostDiskAllocation,

    /// Lifecycle state at creation — always [`WorkerHostStatus::Provisioning`]
    /// in PR-rev-7-9b.
    pub status: WorkerHostStatus,

    /// The persisted cloud region.
    pub region: String,

    /// The persisted GCE machine type (after the default was applied).
    pub instance_type: String,

    /// Immutable creation timestamp.
    pub created_at: DateTime<Utc>,

    // ── Runtime / profile contract (PR1) — echo the persisted values ──────
    /// The persisted digest-pinned base image ref. The optional wire shape is
    /// retained for mixed-version compatibility; new provisioning always
    /// resolves it before creating the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_ref: Option<String>,
    /// The `sha256:<64hex>` digest extracted from `base_image_ref`. The
    /// optional wire shape is retained for mixed-version compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_digest: Option<String>,
    pub base_image_source_kind: String,
    pub base_image_source_value: String,
    /// Forward-compat runtime schema (PR2 populates; NULL in PR1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_schema: Option<String>,
    /// Forward-compat runtime labels (PR2). Defaults to `{}`.
    #[serde(default)]
    pub runtime_labels: serde_json::Value,
    /// Forward-compat refresh timestamp for `runtime_labels` (PR2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_labels_refreshed_at: Option<DateTime<Utc>>,
    /// The persisted worker profile (`fat-default` default).
    pub worker_profile: String,
}

/// Response body for `GET /api/v1/orgs/:orgId/worker-hosts` (`200 OK`).
///
/// `total` is the unpaginated count for the applied status filter so clients
/// can paginate over `hosts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkerHostsResponse {
    /// The page of hosts (already limit/offset-sliced and ordered by
    /// `created_at DESC, host_id DESC` — the `host_id` tie-breaker keeps
    /// offset-based pagination deterministic when multiple rows share
    /// `created_at`, per ontology [D12]).
    pub hosts: Vec<WorkerHost>,

    /// Total rows matching the org + optional status filter, ignoring paging.
    pub total: i64,

    /// The calling org's own immutable-pin policy
    /// (`orgs.default_base_image_ref`). When the org follows a promoted track,
    /// `org_default_base_image_track` is returned instead. Never exposes other
    /// orgs' image policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_default_base_image_ref: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_default_base_image_track: Option<String>,

    #[serde(default)]
    pub promoted_image_tracks: Vec<WorkerImageTrack>,

    /// The boot-disk sizing coefficients this server applies, resolved from its
    /// configured scratch size. Published so the Add worker host dialog shows
    /// the default the server would actually use instead of recomputing the
    /// rule from its own constants (ontology `g5.runtime.placement.host-disk`).
    pub boot_disk_sizing: WorkerHostBootDiskSizing,
}

/// Optional operator-supplied reason for a host drain. The server normalizes
/// an omitted or blank reason to `operator`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrainWorkerHostRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Durable receipt returned as soon as the drain has been recorded. VM stop
/// progress remains on the host/workspace runtime records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrainWorkerHostResponse {
    pub host_id: Uuid,
    pub drain_id: Uuid,
    pub active_vms: u32,
    pub already_draining: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerImageTrack {
    pub name: String,
    pub image_ref: String,
    pub digest: String,
}

/// Request body for changing the org's worker-image source policy. Exactly one
/// of `imageTrack` and `baseImageRef` must be supplied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkerImagePolicyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_track: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image_ref: Option<String>,
}

/// The immutable source snapshot stored as the org's default for future hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerImagePolicyResponse {
    pub source_kind: String,
    pub source_value: String,
    pub resolved_image_ref: String,
    pub resolved_digest: String,
}

// ---------------------------------------------------------------------------
// Host-level observability heartbeat payload (PR-rev-7-11)
//
// Carried as an optional block on the worker heartbeat
// (`HeartbeatRequest.host_observability`, g5-common internal_api). The host's
// g5d self-reports these on every heartbeat; g5-api persists them into the
// matching `worker_hosts` columns (ontology [D30]). Every field except
// `host_id` is optional so a partial heartbeat only updates what it carries.
// ---------------------------------------------------------------------------

/// The last IsolationAttestor outcome, persisted under `attestation_state.last`.
///
/// This is the existing `attestation_state` JSONB shape (ontology [D1]/[D30]):
/// a future attestor can add sibling history keys without a schema change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastAttestationResult {
    /// Whether the last attestation passed.
    pub ok: bool,
    /// When the attestation ran.
    pub at: DateTime<Utc>,
    /// Failure detail when `ok == false`; `None` on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Host self-reported observability, attached to a worker heartbeat.
///
/// `host_id` keys the `worker_hosts` row to update; the remaining fields are
/// optional and only the populated ones are persisted (the g5-api UPDATE uses
/// `COALESCE` for the typed columns and a shallow JSONB merge for the
/// `cache_state`/`attestation_state` columns — ontology [D30]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostObservation {
    /// The `worker_hosts.host_id` this observation is for.
    pub host_id: Uuid,

    /// Count of currently-running microVMs on the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_microvm_count: Option<u32>,

    /// Host rootfs-cache disk usage in bytes (df-style "used").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_disk_used_bytes: Option<u64>,

    /// Host rootfs-cache disk capacity in bytes (df-style "total").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_disk_total_bytes: Option<u64>,

    /// Sorted, de-duplicated set of OCI image digests materialised on disk.
    /// Merged into `cache_state.imageDigestsCached`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_digests_cached: Option<Vec<String>>,

    /// Last attestation outcome. Merged into `attestation_state.last`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attestation_result: Option<LastAttestationResult>,
}

// ---------------------------------------------------------------------------
// sqlx Type / Encode / Decode for WorkerHostStatus
// ---------------------------------------------------------------------------

#[cfg(feature = "sqlx")]
mod sqlx_impl {
    use super::WorkerHostStatus;

    impl sqlx::Type<sqlx::Postgres> for WorkerHostStatus {
        fn type_info() -> sqlx::postgres::PgTypeInfo {
            <String as sqlx::Type<sqlx::Postgres>>::type_info()
        }

        fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
            <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
        }
    }

    impl<'q> sqlx::Encode<'q, sqlx::Postgres> for WorkerHostStatus {
        fn encode_by_ref(
            &self,
            buf: &mut sqlx::postgres::PgArgumentBuffer,
        ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
            <&str as sqlx::Encode<'q, sqlx::Postgres>>::encode(self.as_str(), buf)
        }
    }

    impl<'r> sqlx::Decode<'r, sqlx::Postgres> for WorkerHostStatus {
        fn decode(
            value: sqlx::postgres::PgValueRef<'r>,
        ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            let s = <&str as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
            s.parse::<Self>().map_err(|e| e.into())
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_str() {
        for status in WorkerHostStatus::all() {
            let s = status.as_str();
            let parsed: WorkerHostStatus = s.parse().unwrap_or_else(|e| panic!("{}", e));
            assert_eq!(*status, parsed);
        }
    }

    #[test]
    fn status_serde_round_trip() {
        for status in WorkerHostStatus::all() {
            let json = serde_json::to_string(status).unwrap();
            assert_eq!(json, format!("\"{}\"", status.as_str()));
            let parsed: WorkerHostStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, parsed);
        }
    }

    #[test]
    fn status_display_matches_as_str() {
        assert_eq!(WorkerHostStatus::Provisioning.to_string(), "provisioning");
        assert_eq!(WorkerHostStatus::Ready.to_string(), "ready");
        assert_eq!(WorkerHostStatus::Unreachable.to_string(), "unreachable");
        assert_eq!(WorkerHostStatus::Draining.to_string(), "draining");
        assert_eq!(WorkerHostStatus::Terminated.to_string(), "terminated");
    }

    #[test]
    fn status_rejects_unknown_string() {
        let err = "unknown".parse::<WorkerHostStatus>().unwrap_err();
        assert!(err.contains("unknown worker host status"));
    }

    #[test]
    fn dto_serde_round_trip_minimal_row() {
        let host = WorkerHost {
            host_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            org_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            vpc_id: None,
            cloud_instance_id: None,
            instance_type: "n2-standard-8".to_string(),
            region: "us-central1".to_string(),
            kind: "gce".to_string(),
            slots_total: 1,
            slot_size_reported: false,
            slot_cpus: 2,
            slot_mem_mib: 4096,
            scratch_disk_mib: 10240,
            runtime_identity: serde_json::json!({}),
            capabilities: Vec::new(),
            occupancy: 0,
            heartbeat_state: "never".to_string(),
            heartbeat_age_secs: None,
            runtime_state: String::new(),
            kernel_version: None,
            active_microvm_count: None,
            cache_disk_used_bytes: None,
            cache_disk_total_bytes: None,
            cache_state: serde_json::json!({}),
            attestation_state: serde_json::json!({}),
            status: WorkerHostStatus::Provisioning,
            created_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            last_heartbeat: None,
            credential_expires_at: None,
            credential_generation: 0,
            drain_requested_at: None,
            drain_reason: None,
            drain_id: None,
            last_error: None,
            provision_error_code: None,
            provision_retry_count: 0,
            provision_next_retry_at: None,
            base_image_ref: None,
            base_image_digest: None,
            base_image_source_kind: None,
            base_image_source_value: None,
            runtime_schema: None,
            runtime_labels: serde_json::json!({}),
            runtime_labels_refreshed_at: None,
            worker_profile: "fat-default".into(),
            workspace_vm_limit: None,
            created_by_user_id: None,
            created_by_display_name: None,
            created_by_email: None,
        };

        let json = serde_json::to_string(&host).unwrap();
        let parsed: WorkerHost = serde_json::from_str(&json).unwrap();
        assert_eq!(host, parsed);

        // Older hosts may still carry retired inventory fields during rollout.
        // Read them without retaining or re-emitting an obsolete runtime identity.
        let mut legacy = serde_json::to_value(&host).unwrap();
        legacy["libkrunVersion"] = serde_json::json!("1.18.1");
        legacy["libkrunfwVersion"] = serde_json::json!("4.0.0");
        let parsed: WorkerHost = serde_json::from_value(legacy).unwrap();
        assert_eq!(host, parsed);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    }

    #[test]
    fn dto_serde_omits_none_optionals_on_wire() {
        let host = WorkerHost {
            host_id: Uuid::nil(),
            org_id: Uuid::nil(),
            vpc_id: None,
            cloud_instance_id: None,
            instance_type: "n2-standard-8".to_string(),
            region: "us-central1".to_string(),
            kind: "gce".to_string(),
            slots_total: 1,
            slot_size_reported: false,
            slot_cpus: 2,
            slot_mem_mib: 4096,
            scratch_disk_mib: 10240,
            runtime_identity: serde_json::json!({}),
            capabilities: Vec::new(),
            occupancy: 0,
            heartbeat_state: "never".to_string(),
            heartbeat_age_secs: None,
            runtime_state: String::new(),
            kernel_version: None,
            active_microvm_count: None,
            cache_disk_used_bytes: None,
            cache_disk_total_bytes: None,
            cache_state: serde_json::json!({}),
            attestation_state: serde_json::json!({}),
            status: WorkerHostStatus::Ready,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            last_heartbeat: None,
            credential_expires_at: None,
            credential_generation: 0,
            drain_requested_at: None,
            drain_reason: None,
            drain_id: None,
            last_error: None,
            provision_error_code: None,
            provision_retry_count: 0,
            provision_next_retry_at: None,
            base_image_ref: None,
            base_image_digest: None,
            base_image_source_kind: None,
            base_image_source_value: None,
            runtime_schema: None,
            runtime_labels: serde_json::json!({}),
            runtime_labels_refreshed_at: None,
            worker_profile: "fat-default".into(),
            workspace_vm_limit: None,
            created_by_user_id: None,
            created_by_display_name: None,
            created_by_email: None,
        };
        let value: serde_json::Value = serde_json::to_value(&host).unwrap();
        // The Option<…> fields with skip_serializing_if must NOT appear in the
        // serialised body — keeps API responses tight when columns are still
        // NULL (the common case during PR-rev-7-9a).
        assert!(value.get("vpcId").is_none());
        assert!(value.get("cloudInstanceId").is_none());
        assert!(value.get("kernelVersion").is_none());
        assert!(value.get("lastHeartbeat").is_none());
        assert!(value.get("provisionErrorCode").is_none());
        // PR1 runtime/profile contract: the Option<…> runtime fields skip when
        // None; worker_profile + runtimeLabels are always present.
        assert!(value.get("baseImageRef").is_none());
        assert!(value.get("baseImageDigest").is_none());
        assert!(value.get("runtimeSchema").is_none());
        assert!(value.get("runtimeLabelsRefreshedAt").is_none());
        assert_eq!(value["workerProfile"], "fat-default");
        assert_eq!(value["runtimeLabels"], serde_json::json!({}));
        // [D55]: an unpinned (NULL) concurrency is absent, never `null`.
        assert!(value.get("workspaceVmLimit").is_none());
    }

    #[test]
    fn dto_serde_camel_case_field_names() {
        // Regression guard: the camelCase rename MUST hold so the API envelope
        // is stable for the upcoming PR-rev-7-9b provisioning endpoints.
        let host = WorkerHost {
            host_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
            org_id: Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap(),
            vpc_id: Some("vpc-abc".into()),
            cloud_instance_id: Some("gce-instance-xyz".into()),
            instance_type: "n2-standard-8".into(),
            region: "us-central1".into(),
            kind: "gce".to_string(),
            slots_total: 4,
            slot_size_reported: false,
            slot_cpus: 2,
            slot_mem_mib: 4096,
            scratch_disk_mib: 10240,
            runtime_identity: serde_json::json!({"g5d_version": "test"}),
            capabilities: vec!["isolation:cloud-hypervisor".to_string()],
            occupancy: 3,
            heartbeat_state: "live".to_string(),
            heartbeat_age_secs: Some(5),
            runtime_state: String::new(),
            kernel_version: Some("6.1.0".into()),
            active_microvm_count: Some(3),
            cache_disk_used_bytes: Some(1_073_741_824),
            cache_disk_total_bytes: Some(53_687_091_200),
            cache_state: serde_json::json!({"sccache": "warm"}),
            attestation_state: serde_json::json!({"attestor": "pass"}),
            status: WorkerHostStatus::Ready,
            created_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            last_heartbeat: Some(DateTime::from_timestamp(1_700_000_100, 0).unwrap()),
            credential_expires_at: None,
            credential_generation: 0,
            drain_requested_at: None,
            drain_reason: None,
            drain_id: None,
            last_error: None,
            provision_error_code: None,
            provision_retry_count: 0,
            provision_next_retry_at: None,
            base_image_ref: Some("ghcr.io/acme/app@sha256:".to_string() + &"a".repeat(64)),
            base_image_digest: Some("sha256:".to_string() + &"a".repeat(64)),
            base_image_source_kind: Some("pin".to_string()),
            base_image_source_value: Some("ghcr.io/acme/app@sha256:".to_string() + &"a".repeat(64)),
            runtime_schema: Some("g5.runtime/v1".into()),
            runtime_labels: serde_json::json!({"org.g5.profile": "fat-default"}),
            runtime_labels_refreshed_at: Some(DateTime::from_timestamp(1_700_000_200, 0).unwrap()),
            worker_profile: "slim".into(),
            workspace_vm_limit: Some(5),
            created_by_user_id: None,
            created_by_display_name: None,
            created_by_email: None,
        };
        let value: serde_json::Value = serde_json::to_value(&host).unwrap();
        assert!(value.get("hostId").is_some());
        assert!(value.get("orgId").is_some());
        assert!(value.get("vpcId").is_some());
        assert!(value.get("cloudInstanceId").is_some());
        assert!(value.get("instanceType").is_some());
        assert!(value.get("kernelVersion").is_some());
        assert!(value.get("cacheState").is_some());
        assert!(value.get("attestationState").is_some());
        assert!(value.get("createdAt").is_some());
        assert!(value.get("lastHeartbeat").is_some());
        // PR1 runtime/profile contract fields serialise camelCase when set.
        assert!(value.get("baseImageRef").is_some());
        assert!(value.get("baseImageDigest").is_some());
        assert_eq!(value["baseImageSourceKind"], "pin");
        assert!(value.get("baseImageSourceValue").is_some());
        assert!(value.get("runtimeSchema").is_some());
        assert!(value.get("runtimeLabels").is_some());
        assert!(value.get("runtimeLabelsRefreshedAt").is_some());
        assert_eq!(value["workerProfile"], "slim");
        // [D55] read-back: the persisted pin serialises camelCase when set...
        assert_eq!(value["workspaceVmLimit"], 5);
    }

    #[test]
    fn provision_request_defaults_instance_type_to_none() {
        // Only `region` supplied — `instanceType` is optional and the handler
        // applies DEFAULT_WORKER_HOST_INSTANCE_TYPE when it is None.
        let req: ProvisionWorkerHostRequest =
            serde_json::from_str(r#"{"region":"us-central1"}"#).unwrap();
        assert_eq!(req.region, "us-central1");
        assert_eq!(req.instance_type, None);
    }

    #[test]
    fn provision_request_accepts_explicit_instance_type_camel_case() {
        let req: ProvisionWorkerHostRequest =
            serde_json::from_str(r#"{"region":"us-east1","instanceType":"n2-standard-16"}"#)
                .unwrap();
        assert_eq!(req.region, "us-east1");
        assert_eq!(req.instance_type.as_deref(), Some("n2-standard-16"));
    }

    #[test]
    fn provision_request_workspace_vm_limit_defaults_to_none_and_round_trips() {
        // Omitted ⇒ None (handler applies the vCPU-count default).
        let minimal: ProvisionWorkerHostRequest =
            serde_json::from_str(r#"{"region":"us-central1"}"#).unwrap();
        assert_eq!(minimal.workspace_vm_limit, None);

        // Supplied via the camelCase wire name and round-trips back.
        let req: ProvisionWorkerHostRequest =
            serde_json::from_str(r#"{"region":"us-central1","workspaceVmLimit":2}"#).unwrap();
        assert_eq!(req.workspace_vm_limit, Some(2));
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["workspaceVmLimit"], 2);
        // None is skipped on the wire.
        assert!(serde_json::to_value(&minimal)
            .unwrap()
            .get("workspaceVmLimit")
            .is_none());
    }

    #[test]
    fn provision_request_boot_disk_gb_defaults_to_none_and_round_trips() {
        // Omitted ⇒ None ⇒ the host stores NULL and tracks the sizing rule.
        let minimal: ProvisionWorkerHostRequest =
            serde_json::from_str(r#"{"region":"us-central1"}"#).unwrap();
        assert_eq!(minimal.boot_disk_gb, None);
        assert!(serde_json::to_value(&minimal)
            .unwrap()
            .get("bootDiskGb")
            .is_none());

        let req: ProvisionWorkerHostRequest =
            serde_json::from_str(r#"{"region":"us-central1","bootDiskGb":512}"#).unwrap();
        assert_eq!(req.boot_disk_gb, Some(512));
        assert_eq!(serde_json::to_value(&req).unwrap()["bootDiskGb"], 512);
    }

    #[test]
    fn disk_policy_minimum_is_never_clamped() {
        let sizing = WorkerHostBootDiskSizing::default();
        for (slots, expected) in [(1, 200), (2, 280), (5, 520), (23, 1960)] {
            assert_eq!(sizing.resolve(None, slots).unwrap(), expected);
            assert_eq!(
                sizing.resolve(Some(expected as i32), slots).unwrap(),
                expected
            );
            assert!(sizing.resolve(Some(expected as i32 - 1), slots).is_err());
        }
        for slots in [0, 24, 44, u32::MAX] {
            assert!(sizing.resolve(None, slots).is_err());
        }
        for override_gib in [-1, 0, 80, 519, 2001] {
            assert!(sizing.resolve(Some(override_gib), 5).is_err());
        }
        assert_eq!(sizing.resolve(Some(2000), 5).unwrap(), 2000);
        let custom = WorkerHostBootDiskSizing::new(60, 24);
        assert_eq!(custom.resolve(None, 2).unwrap(), 108);
        assert!(custom.resolve(Some(107), 2).is_err());
        assert_eq!(custom.resolve(Some(200), 2).unwrap(), 200);
    }

    #[test]
    fn provision_response_serialises_camel_case() {
        let resp = ProvisionWorkerHostResponse {
            disk_allocation: HostDiskAllocation {
                host_disk_gib: 120,
                vm_disk_mib: 10240,
                slots: 3,
                disk_gib: 150,
            },
            host_id: Uuid::parse_str("55555555-5555-5555-5555-555555555555").unwrap(),
            host_jwt: "eyJ.header.sig".to_string(),
            status: WorkerHostStatus::Provisioning,
            region: "us-central1".to_string(),
            instance_type: DEFAULT_WORKER_HOST_INSTANCE_TYPE.to_string(),
            created_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            base_image_ref: Some("ghcr.io/g5/worker@sha256:".to_string() + &"b".repeat(64)),
            base_image_digest: Some("sha256:".to_string() + &"b".repeat(64)),
            base_image_source_kind: "track".to_string(),
            base_image_source_value: "current".to_string(),
            runtime_schema: None,
            runtime_labels: serde_json::json!({}),
            runtime_labels_refreshed_at: None,
            worker_profile: DEFAULT_WORKER_PROFILE.to_string(),
        };
        let value: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert!(value.get("hostId").is_some());
        assert!(value.get("hostJwt").is_some());
        assert_eq!(value["status"], "provisioning");
        assert_eq!(value["instanceType"], DEFAULT_WORKER_HOST_INSTANCE_TYPE);
        assert!(value.get("createdAt").is_some());
        // PR1: profile is always present; the optional runtime fields are
        // absent when None, runtimeLabels is always present.
        assert_eq!(value["workerProfile"], DEFAULT_WORKER_PROFILE);
        assert!(value.get("baseImageRef").is_some());
        assert_eq!(value["baseImageSourceKind"], "track");
        assert_eq!(value["baseImageSourceValue"], "current");
        assert!(value.get("runtimeLabels").is_some());

        // Round-trips back to an equal value.
        let parsed: ProvisionWorkerHostResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn list_response_round_trips_with_total() {
        let host = WorkerHost {
            host_id: Uuid::nil(),
            org_id: Uuid::nil(),
            vpc_id: None,
            cloud_instance_id: None,
            instance_type: DEFAULT_WORKER_HOST_INSTANCE_TYPE.to_string(),
            region: "us-central1".to_string(),
            kind: "gce".to_string(),
            slots_total: 1,
            slot_size_reported: false,
            slot_cpus: 2,
            slot_mem_mib: 4096,
            scratch_disk_mib: 10240,
            runtime_identity: serde_json::json!({}),
            capabilities: Vec::new(),
            occupancy: 0,
            heartbeat_state: "never".to_string(),
            heartbeat_age_secs: None,
            runtime_state: String::new(),
            kernel_version: None,
            active_microvm_count: None,
            cache_disk_used_bytes: None,
            cache_disk_total_bytes: None,
            cache_state: serde_json::json!({}),
            attestation_state: serde_json::json!({}),
            status: WorkerHostStatus::Provisioning,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            last_heartbeat: None,
            credential_expires_at: None,
            credential_generation: 0,
            drain_requested_at: None,
            drain_reason: None,
            drain_id: None,
            last_error: None,
            provision_error_code: None,
            provision_retry_count: 0,
            provision_next_retry_at: None,
            base_image_ref: None,
            base_image_digest: None,
            base_image_source_kind: None,
            base_image_source_value: None,
            runtime_schema: None,
            runtime_labels: serde_json::json!({}),
            runtime_labels_refreshed_at: None,
            worker_profile: "fat-default".into(),
            workspace_vm_limit: None,
            created_by_user_id: None,
            created_by_display_name: None,
            created_by_email: None,
        };
        let resp = ListWorkerHostsResponse {
            hosts: vec![host],
            total: 7,
            org_default_base_image_ref: None,
            org_default_base_image_track: None,
            promoted_image_tracks: Vec::new(),
            boot_disk_sizing: WorkerHostBootDiskSizing::default(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        // Absent-when-unset: the org-default field must not appear when None
        // (ontology [D51] additive wire compatibility).
        assert!(!json.contains("orgDefaultBaseImageRef"));
        // The sizing coefficients always ride the envelope: the dialog derives
        // its displayed default from them and holds no copy of the rule
        // (ontology g5.runtime.placement.host-disk).
        assert!(json.contains("\"bootDiskSizing\""));
        assert!(json.contains("\"perSlotGb\""));
        let parsed: ListWorkerHostsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, resp);
        assert_eq!(parsed.total, 7);

        let with_default = ListWorkerHostsResponse {
            org_default_base_image_ref: Some(format!(
                "reg.example.com/g5d-pairsoft-preview@sha256:{}",
                "a".repeat(64)
            )),
            ..resp
        };
        let json = serde_json::to_string(&with_default).unwrap();
        assert!(json.contains("orgDefaultBaseImageRef"));
        let parsed: ListWorkerHostsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, with_default);
    }

    #[test]
    fn host_observation_full_payload_round_trips_camel_case() {
        let obs = HostObservation {
            host_id: Uuid::parse_str("66666666-6666-6666-6666-666666666666").unwrap(),
            active_microvm_count: Some(3),
            cache_disk_used_bytes: Some(1_000),
            cache_disk_total_bytes: Some(10_000),
            image_digests_cached: Some(vec!["sha256:aaaa".into(), "sha256:bbbb".into()]),
            last_attestation_result: Some(LastAttestationResult {
                ok: true,
                at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
                error: None,
            }),
        };
        let value: serde_json::Value = serde_json::to_value(&obs).unwrap();
        assert_eq!(value["hostId"], "66666666-6666-6666-6666-666666666666");
        assert_eq!(value["activeMicrovmCount"], 3);
        assert_eq!(value["cacheDiskUsedBytes"], 1_000);
        assert_eq!(value["cacheDiskTotalBytes"], 10_000);
        assert_eq!(value["imageDigestsCached"][0], "sha256:aaaa");
        assert_eq!(value["lastAttestationResult"]["ok"], true);
        assert!(value["lastAttestationResult"].get("error").is_none());

        let parsed: HostObservation = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, obs);

        let mut legacy = serde_json::to_value(&obs).unwrap();
        legacy["libkrunVersion"] = serde_json::json!("1.18.1");
        legacy["libkrunfwVersion"] = serde_json::json!("4.0.0");
        let parsed: HostObservation = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed, obs);
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            serde_json::to_value(obs).unwrap()
        );
    }

    #[test]
    fn host_observation_partial_payload_omits_absent_fields() {
        // Only host_id + active count → every other optional must be absent on
        // the wire so the g5-api COALESCE/merge leaves those columns untouched.
        let obs = HostObservation {
            host_id: Uuid::nil(),
            active_microvm_count: Some(0),
            cache_disk_used_bytes: None,
            cache_disk_total_bytes: None,
            image_digests_cached: None,
            last_attestation_result: None,
        };
        let value: serde_json::Value = serde_json::to_value(&obs).unwrap();
        assert_eq!(value["activeMicrovmCount"], 0);
        assert!(value.get("cacheDiskUsedBytes").is_none());
        assert!(value.get("cacheDiskTotalBytes").is_none());
        assert!(value.get("imageDigestsCached").is_none());
        assert!(value.get("lastAttestationResult").is_none());

        // Round-trips back to an equal value (absent optionals deserialize None).
        let parsed: HostObservation = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, obs);
    }

    // ── PR1 runtime/profile contract: digest parser, request serde, constants ──

    #[test]
    fn validate_base_image_ref_extracts_digest() {
        let hex = "a".repeat(64);
        let reference = format!("registry.example.com:5000/img@sha256:{hex}");
        let digest = validate_base_image_ref(&reference).expect("digest-pinned ref accepted");
        assert_eq!(digest, format!("sha256:{hex}"));
        // Leading/trailing whitespace is trimmed before validation.
        let padded = format!("  ghcr.io/acme/app@sha256:{hex}  ");
        assert_eq!(
            validate_base_image_ref(&padded).unwrap(),
            format!("sha256:{hex}")
        );
    }

    #[test]
    fn validate_base_image_ref_rejects_tag_only() {
        assert_eq!(
            validate_base_image_ref("ubuntu:24.04"),
            Err(BaseImageRefError::MissingDigest)
        );
        // A bare name with a registry path but no @digest is still tag-only.
        assert_eq!(
            validate_base_image_ref("ghcr.io/acme/app:1.2.3"),
            Err(BaseImageRefError::MissingDigest)
        );
    }

    // ── Guest glibc ABI floor guard (provisioning_api#D57 / microvm#D12) ──

    #[test]
    fn parse_declared_glibc_extracts_major_minor() {
        assert_eq!(
            parse_declared_glibc("ubuntu-26.04-glibc-2.43"),
            Some((2, 43))
        );
        assert_eq!(
            parse_declared_glibc("ubuntu-24.04-glibc-2.39"),
            Some((2, 39))
        );
        // Trailing junk / patch component after the minor is ignored.
        assert_eq!(parse_declared_glibc("glibc-2.43-fat"), Some((2, 43)));
        assert_eq!(parse_declared_glibc("x-glibc-2.43.1-y"), Some((2, 43)));
        // A malformed leading token must not mask a real declaration further
        // along, and the lowest declared version wins so an ambiguous label is
        // judged by its weakest claim rather than failing open.
        assert_eq!(
            parse_declared_glibc("glibc-invalid-glibc-2.39"),
            Some((2, 39))
        );
        assert_eq!(
            parse_declared_glibc("glibc-2.43-then-glibc-2.39"),
            Some((2, 39))
        );
        assert_eq!(
            parse_declared_glibc("glibc-2.39-then-glibc-2.43"),
            Some((2, 39))
        );
    }

    #[test]
    fn parse_declared_glibc_none_when_no_token() {
        // No glibc token → not floor-checkable (operator responsibility, microvm#D12).
        assert_eq!(parse_declared_glibc("buildpack-deps:resolute"), None);
        assert_eq!(parse_declared_glibc(""), None);
        assert_eq!(parse_declared_glibc("ubuntu-26.04"), None);
        // A `glibc-` prefix with no parseable <major>.<minor>.
        assert_eq!(parse_declared_glibc("glibc-two.point"), None);
        assert_eq!(parse_declared_glibc("glibc-2"), None);
    }

    #[test]
    fn base_image_below_glibc_floor_rejects_only_positively_declared_subfloor() {
        // Below the 2.43 floor → reject.
        assert!(base_image_below_glibc_floor("ubuntu-24.04-glibc-2.39"));
        assert!(base_image_below_glibc_floor("debian-bookworm-glibc-2.36"));
        // At or above the floor → accept.
        assert!(!base_image_below_glibc_floor("ubuntu-26.04-glibc-2.43"));
        assert!(!base_image_below_glibc_floor("future-glibc-3.0"));
        assert!(!base_image_below_glibc_floor("future-glibc-2.50"));
        // Missing/unparseable token → accept (never fail into rejection).
        assert!(!base_image_below_glibc_floor("buildpack-deps:resolute"));
        assert!(!base_image_below_glibc_floor(""));
    }

    #[test]
    fn glibc_floor_message_matches_ontology_wording() {
        // Verbatim per microvm_execution_boundary#D12.
        assert_eq!(
            glibc_floor_violation_message(),
            "guest below the glibc 2.43 floor — repin to a 26.04 base"
        );
        assert_eq!(GUEST_GLIBC_FLOOR, (2, 43));
    }

    #[test]
    fn validate_base_image_ref_rejects_invalid_name_shape() {
        let hex = "a".repeat(64);

        // Repository names are lowercase ASCII only.
        assert_eq!(
            validate_base_image_ref(&format!("ghcr.io/Acme/app@sha256:{hex}")),
            Err(BaseImageRefError::MissingDigest)
        );
        assert_eq!(
            validate_base_image_ref(&format!("ghcr.io/acme/app\u{e9}@sha256:{hex}")),
            Err(BaseImageRefError::MissingDigest)
        );

        // A host:port separator is allowed only in the registry host segment.
        assert_eq!(
            validate_base_image_ref(&format!("ghcr.io/acme/app:1.2.3@sha256:{hex}")),
            Err(BaseImageRefError::MissingDigest)
        );
        assert_eq!(
            validate_base_image_ref(&format!("ghcr.io/acme:team/app@sha256:{hex}")),
            Err(BaseImageRefError::MissingDigest)
        );

        // Bad registry host:port forms are rejected rather than treated as tags.
        assert_eq!(
            validate_base_image_ref(&format!("ghcr.io:abc/acme/app@sha256:{hex}")),
            Err(BaseImageRefError::MissingDigest)
        );
        assert_eq!(
            validate_base_image_ref(&format!("ghcr.io:5000:6000/acme/app@sha256:{hex}")),
            Err(BaseImageRefError::MissingDigest)
        );
    }

    #[test]
    fn validate_base_image_ref_rejects_bad_digest() {
        let name = "ghcr.io/acme/app";
        // Short hex (63).
        assert_eq!(
            validate_base_image_ref(&format!("{name}@sha256:{}", "a".repeat(63))),
            Err(BaseImageRefError::MalformedDigest)
        );
        // Long hex (65).
        assert_eq!(
            validate_base_image_ref(&format!("{name}@sha256:{}", "a".repeat(65))),
            Err(BaseImageRefError::MalformedDigest)
        );
        // Non-hex char.
        assert_eq!(
            validate_base_image_ref(&format!("{name}@sha256:{}", "g".repeat(64))),
            Err(BaseImageRefError::MalformedDigest)
        );
        // Uppercase hex is rejected (matches the DB CHECK's case-sensitive regex).
        assert_eq!(
            validate_base_image_ref(&format!("{name}@sha256:{}", "A".repeat(64))),
            Err(BaseImageRefError::MalformedDigest)
        );
        // Wrong algorithm prefix.
        assert_eq!(
            validate_base_image_ref(&format!("{name}@sha512:{}", "a".repeat(64))),
            Err(BaseImageRefError::MalformedDigest)
        );
        // Empty / whitespace-only.
        assert_eq!(validate_base_image_ref(""), Err(BaseImageRefError::Empty));
        assert_eq!(
            validate_base_image_ref("   "),
            Err(BaseImageRefError::Empty)
        );
        // @digest present but no name before the '@'.
        assert_eq!(
            validate_base_image_ref(&format!("@sha256:{}", "a".repeat(64))),
            Err(BaseImageRefError::MissingDigest)
        );
        // Name with no '/' (no registry/image path) → MissingDigest.
        assert_eq!(
            validate_base_image_ref(&format!("app@sha256:{}", "a".repeat(64))),
            Err(BaseImageRefError::MissingDigest)
        );
    }

    #[test]
    fn provision_request_accepts_base_image_ref_and_worker_profile_camel_case() {
        let hex = "a".repeat(64);
        let body = format!(
            r#"{{"region":"us-central1","baseImageRef":"ghcr.io/acme/app@sha256:{hex}","workerProfile":"slim"}}"#
        );
        let req: ProvisionWorkerHostRequest = serde_json::from_str(&body).unwrap();
        assert_eq!(
            req.base_image_ref.as_deref(),
            Some(format!("ghcr.io/acme/app@sha256:{hex}").as_str())
        );
        assert_eq!(req.worker_profile.as_deref(), Some("slim"));

        // Omitted → None for both new optional request fields.
        let minimal: ProvisionWorkerHostRequest =
            serde_json::from_str(r#"{"region":"us-central1"}"#).unwrap();
        assert_eq!(minimal.base_image_ref, None);
        assert_eq!(minimal.worker_profile, None);
    }

    #[test]
    fn provision_response_round_trips_runtime_profile_fields() {
        let hex = "a".repeat(64);
        let resp = ProvisionWorkerHostResponse {
            disk_allocation: HostDiskAllocation {
                host_disk_gib: 120,
                vm_disk_mib: 10240,
                slots: 3,
                disk_gib: 150,
            },
            host_id: Uuid::nil(),
            host_jwt: "eyJ.header.sig".to_string(),
            status: WorkerHostStatus::Provisioning,
            region: "us-central1".to_string(),
            instance_type: DEFAULT_WORKER_HOST_INSTANCE_TYPE.to_string(),
            created_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            base_image_ref: Some(format!("ghcr.io/acme/app@sha256:{hex}")),
            base_image_digest: Some(format!("sha256:{hex}")),
            base_image_source_kind: "pin".to_string(),
            base_image_source_value: format!("ghcr.io/acme/app@sha256:{hex}"),
            runtime_schema: None,
            runtime_labels: serde_json::json!({}),
            runtime_labels_refreshed_at: None,
            worker_profile: "slim".to_string(),
        };
        let value: serde_json::Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(
            value["baseImageRef"],
            format!("ghcr.io/acme/app@sha256:{hex}")
        );
        assert_eq!(value["baseImageDigest"], format!("sha256:{hex}"));
        assert_eq!(value["workerProfile"], "slim");
        let parsed: ProvisionWorkerHostResponse = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn worker_profile_constants_match_contract() {
        assert_eq!(DEFAULT_WORKER_PROFILE, "fat-default");
        assert_eq!(WORKER_PROFILE_ALLOWLIST, &["fat-default", "slim"]);
        // The default must always be a member of the allowlist.
        assert!(WORKER_PROFILE_ALLOWLIST.contains(&DEFAULT_WORKER_PROFILE));
    }
}
