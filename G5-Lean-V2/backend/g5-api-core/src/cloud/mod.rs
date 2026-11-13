//! Cloud-provider abstraction for worker-host bring-up (PR-rev-7-9c).
//!
//! The control plane provisions a `worker_hosts` record (PR-rev-7-9b) and then
//! brings up the backing cloud resources — a per-org VPC and a Compute Engine
//! instance — through the [`CloudProvider`] trait defined here. Keeping the
//! backend behind a trait means:
//!
//! - the wire-in handlers (`routes/worker_hosts.rs`) and the reconciler
//!   (`cloud::reconciler`) never hard-code GCP;
//! - tests run fully offline against [`MockCloudProvider`] — no real GCP call
//!   ever leaves the process;
//! - a future AWS/Azure backend is contained to a new `impl CloudProvider`.
//!
//! V1 ships exactly one production implementation — [`gcp::GcpCloudProvider`] —
//! per the locked rev-7 scope (`aux_files/future_implementation/microvm_completion_plan.md`
//! §5.7/§5.8, GCP-only). Contract:
//! `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md` [D17]–[D24].

pub mod bringup;
pub mod gcp;
pub mod reconciler;
pub mod rotation;
pub mod stale_host;
pub mod startup_script;

/// Compute the GCP zone string for a region using the same env-driven suffix
/// `GcpCloudProvider::zone_for` uses, so the orphan-reconciler ([D39]) can
/// build the deterministic `cloud_instance_id` (`{zone}/{name}`) that the
/// standard reconciler pass will decode via `gcp::decode_instance_id` next
/// tick. Free function so the reconciler does not need a `GcpCloudProvider`
/// reference (it works with the abstract `CloudProvider` trait).
///
/// Behavior matches `GcpCloudProvider::from_env`'s sanitization rules: a
/// missing/malformed `G5_GCP_ZONE_SUFFIX` falls back to `a` (the production
/// default). The malformed-suffix case is also rejected at provider-construction
/// time in `GcpCloudProvider::from_env`, so reaching this fallback in
/// production means the operator never had a working GCP provider in the first
/// place.
#[must_use]
pub fn gcp_zone_for_region(region: &str) -> String {
    let suffix = std::env::var("G5_GCP_ZONE_SUFFIX")
        .ok()
        .as_deref()
        .and_then(gcp::sanitize_zone_suffix_pub)
        .unwrap_or_else(|| "a".to_string());
    format!("{region}-{suffix}")
}

/// Slug budget inside an instance name: 63 (GCE name cap) − 8 (`g5-host-`)
/// − 36 (UUID) − 1 (separator hyphen). See ontology [D44].
const INSTANCE_NAME_SLUG_BUDGET: usize = 18;

/// The deterministic instance name for a worker host:
/// `g5-host-{sanitized_org_slug}-{host_id}`, falling back to the legacy
/// `g5-host-{host_id}` when the slug sanitizes to empty (ontology [D44]).
///
/// Shared by `gcp::provision_instance` and the orphan-reconciler lookup
/// ([D39]) — both sides MUST derive the name through this one function so the
/// reconciler finds what provisioning created. Free function here (not in
/// [`gcp`]) so the reconciler keeps working against the abstract
/// [`CloudProvider`] trait.
#[must_use]
pub fn host_instance_name(host_id: Uuid, org_slug: &str) -> String {
    let slug = sanitize_slug_for_instance_name(org_slug);
    if slug.is_empty() {
        format!("g5-host-{host_id}")
    } else {
        format!("g5-host-{slug}-{host_id}")
    }
}

/// Identity of the user who triggered a host provision, threaded into
/// `provision_instance` so the backend can stamp creator-provenance labels
/// (`g5_created_by_id` / `g5_created_by`, ontology [D44]). `email` is `None`
/// when the lookup failed — the backend then stamps the UUID label only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisioningActor {
    /// The authenticated user's UUID, verified by `AuthContext::require_user_id`.
    pub user_id: Uuid,
    /// The user's email (`users.email`), when resolved.
    pub email: Option<String>,
}

/// Reduce an org slug to an RFC-1035-safe instance-name fragment: lowercase,
/// `_` → `-`, drop other non-`[a-z0-9-]` characters, collapse consecutive
/// hyphens, trim hyphens at both ends, truncate to
/// [`INSTANCE_NAME_SLUG_BUDGET`], and re-trim a trailing hyphen the truncation
/// may expose. Returns an empty string when nothing survives (ontology [D44]).
fn sanitize_slug_for_instance_name(raw: &str) -> String {
    let mut out = String::with_capacity(INSTANCE_NAME_SLUG_BUDGET);
    for c in raw.chars() {
        let mapped = match c {
            'A'..='Z' => c.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' => c,
            '_' | '-' => '-',
            _ => continue,
        };
        if mapped == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(mapped);
        if out.len() == INSTANCE_NAME_SLUG_BUDGET {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use uuid::Uuid;

// ─── Value types ─────────────────────────────────────────────────────────────

/// Backend-neutral lifecycle of a *cloud instance* (the VM), as distinct from
/// [`g5_contracts::types::worker_host::WorkerHostStatus`], which is the
/// *control-plane* lifecycle of the host record. The reconciler ([D20]) maps
/// `InstanceState` onto `WorkerHostStatus` transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InstanceState {
    /// The instance is being created or staged; not yet accepting work.
    Provisioning,
    /// The instance is running.
    Running,
    /// The instance is shutting down / suspending.
    Stopping,
    /// The instance is gone (deleted, terminated, or unrecoverable).
    Terminated,
    /// The backend reported a status we do not map to the above.
    Unknown,
}

/// Opaque handle to a per-org network, returned by
/// [`CloudProvider::ensure_vpc_for_org`] and passed into
/// [`CloudProvider::provision_instance`]. See ontology [D18].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpcRef {
    /// The provider's VPC identity. For GCP this is the network name
    /// (`g5-org-{org_id}`).
    pub vpc_id: String,
    /// The cloud region the host will run in (e.g. `us-central1`).
    pub region: String,
    /// Fully-qualified network resource self-link (provider-specific).
    pub network_self_link: String,
    /// Fully-qualified regional subnetwork self-link, when one applies.
    pub subnetwork_self_link: Option<String>,
}

/// Handle to a provisioned cloud instance, persisted onto
/// `worker_hosts.cloud_instance_id`. IPs may be `None` immediately after the
/// insert (they populate once the instance reaches RUNNING) and are not
/// required by 9c. See ontology [D19].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudInstanceRef {
    /// The provider instance identity persisted on the host row.
    pub cloud_instance_id: String,
    /// Private RFC-1918 address inside the per-org VPC, when known.
    pub internal_ip: Option<String>,
    /// External address, when one exists (V1 GCP instances have none —
    /// egress is via Cloud NAT).
    pub external_ip: Option<String>,
    /// The provider zone the instance lives in (GCP instances are zonal).
    pub zone: Option<String>,
    /// Immutable outer boot image the instance was pinned to at insert
    /// (`worker_hosts.boot_image_id`, ontology [D56], issue #1424). `None` when
    /// boot-image resolution failed and the insert fell back to the raw family
    /// `sourceImage` — the row converges on a later rotation.
    pub boot_image_id: Option<String>,
}

/// A by-name instance lookup hit (ontology [D61], issue #1512). Produced by
/// [`CloudProvider::get_instance_by_name`], which searches the region's
/// primary-first UP-zone set ([D59]) — `zone` is the zone the instance was
/// actually found in, so attach/repoint callers encode the true
/// `{zone}/{name}` linkage instead of re-deriving the configured primary zone
/// (the wrong-zone-attach defect behind the #1512 race).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundInstance {
    /// Backend-neutral lifecycle state of the found instance.
    pub state: InstanceState,
    /// The zone the instance was found in (e.g. `us-central1-a`).
    pub zone: String,
}

/// One instance from a by-prefix listing (ontology [D65], issue #1464).
/// Produced by [`CloudProvider::list_instances_by_prefix`], which the
/// stale-host detector's cloud-orphan sweep uses to enumerate `g5-host-*`
/// instances and join them back against `worker_hosts` — the reverse direction
/// no other loop covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedInstance {
    /// The instance name (e.g. `g5-host-{org-slug}-{host_id}`).
    pub name: String,
    /// The zone the instance lives in (e.g. `us-central1-a`).
    pub zone: String,
    /// Backend-neutral lifecycle state at listing time.
    pub state: InstanceState,
}

/// Resolved immutable identity of the outer GCE boot image the fleet boots from
/// (ontology [D56], issue #1424). Produced by [`CloudProvider::resolve_boot_image`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootImageIdentity {
    /// Version-stamped GCE image name (e.g. `g5-worker-v20260723-1046-9749e00`) —
    /// the comparison key persisted as `worker_hosts.boot_image_id`.
    pub image_id: String,
    /// Fully-qualified image self-link, used to pin `sourceImage` at insert so
    /// the recorded id equals the booted image (TOCTOU-safe).
    pub self_link: String,
}

/// Typed cloud errors. Non-exhaustive so a backend can add variants without a
/// breaking change. No variant carries a secret — every value is safe to log.
/// See ontology [D17].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CloudError {
    /// The backend is not configured (missing project / credentials). The
    /// production wiring represents "no cloud" as `cloud_provider = None`
    /// rather than surfacing this, but a provider may still return it.
    #[error("cloud provider not configured: {message}")]
    NotConfigured {
        /// What was missing.
        message: String,
    },
    /// The provider was constructed, but a runtime input (env var, request
    /// field) is malformed in a way the provider refuses to coerce. Distinct
    /// from [`CloudError::NotConfigured`] so the caller can fail closed at the
    /// request boundary instead of treating the provider itself as broken.
    #[error("invalid cloud configuration: {message}")]
    InvalidConfig {
        /// What was malformed.
        message: String,
    },
    /// Credential acquisition or token exchange failed.
    #[error("cloud auth failed: {message}")]
    Auth {
        /// Human-readable cause (never the credential itself).
        message: String,
    },
    /// The cloud API rejected our credentials at request time (HTTP 401/403
    /// from the provider, distinct from [`CloudError::Auth`] which covers
    /// the credential-acquisition / token-exchange flow). Surfaced as a
    /// dedicated variant so the operator log + audit pipeline can
    /// distinguish "bad credentials / missing IAM grant" from the noisier
    /// `Api` bucket. Not retried.
    #[error("cloud auth denied (status {status}): {detail}")]
    AuthDenied {
        /// HTTP status code from the provider (typically 401 or 403).
        status: u16,
        /// The IAM scope or operation that was denied (URL or label).
        scope: String,
        /// Sanitized provider message (no credential material).
        detail: String,
    },
    /// The cloud API returned a non-success status that is not retryable.
    #[error("cloud API error (status {status}): {message}")]
    Api {
        /// HTTP status code from the provider.
        status: u16,
        /// Sanitized provider message.
        message: String,
    },
    /// A transport / network error talking to the provider.
    #[error("cloud transport error: {message}")]
    Transport {
        /// Underlying transport cause.
        message: String,
    },
    /// A long-running operation did not reach DONE within the deadline.
    #[error("cloud operation timed out: {operation}")]
    Timeout {
        /// The operation that timed out.
        operation: String,
    },
    /// The requested resource was not found.
    #[error("cloud resource not found: {resource}")]
    NotFound {
        /// The resource name/path.
        resource: String,
    },
    /// Any other unexpected condition.
    #[error("unexpected cloud error: {message}")]
    Unexpected {
        /// Human-readable cause.
        message: String,
    },
}

/// True when a [`CloudError`] represents a **zonal capacity stockout**
/// (`ZONE_RESOURCE_POOL_EXHAUSTED`) rather than a misconfiguration or transient
/// infra error. GCP returns the same HTTP 503 for both, so detection keys off
/// the provider message: the canonical GCP reason (with or without
/// `_WITH_DETAILS`) or the human phrase GCP folds into `error.message` only
/// ("does not have enough resources"). Case-insensitive substring match so
/// upstream casing variations do not silently drop the classification.
///
/// Single source of truth for BOTH `provision_instance`'s multi-zone fallback
/// (ontology [D59]) and `classify_provision_error`'s `zone_capacity_exhausted`
/// tag (ontology [D34]). Non-`Api` variants (AuthDenied, Transport, Timeout,
/// …) are misconfigurations/transient infra — never "no capacity".
pub(crate) fn is_zone_capacity_exhausted(err: &CloudError) -> bool {
    let CloudError::Api { message, .. } = err else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    lower.contains("zone_resource_pool_exhausted")
        || lower.contains("does not have enough resources")
}

// ─── Trait ───────────────────────────────────────────────────────────────────

/// The cloud backend the control plane provisions worker hosts against.
///
/// All methods are idempotent-friendly where it matters: `ensure_vpc_for_org`
/// MUST be safe to call repeatedly for the same org (see [`gcp`] for the GCP
/// idempotency contract, ontology [D18]).
#[async_trait]
pub trait CloudProvider: Send + Sync {
    /// Ensure the per-org network exists (create if missing) and return a
    /// handle to it. Idempotent. See ontology [D18].
    async fn ensure_vpc_for_org(&self, org_id: Uuid) -> Result<VpcRef, CloudError>;

    /// Provision a new instance for `host_id` in `vpc_ref`, booting `image`
    /// with `startup_script` injected as boot metadata. See ontology [D19].
    ///
    /// `org_slug` feeds the deterministic instance name
    /// ([`host_instance_name`], ontology [D44]); pass `""` to fall back to the
    /// legacy `g5-host-{host_id}` shape. `created_by` is the provisioning
    /// user's identity, stamped as the `g5_created_by_id` (UUID) and
    /// `g5_created_by` (sanitized email) labels when `Some` ([D44]).
    ///
    /// `boot_disk_gb` is THIS host's resolved boot-disk size — per host, not per
    /// provider, because the disk is shared by all of the host's VMs and so
    /// scales with its VM budget (ontology `g5.runtime.placement.host-disk`).
    /// Callers resolve it through [`bringup::resolve_boot_disk_gb`]; there is no
    /// provider-wide default behind this argument.
    #[allow(clippy::too_many_arguments)] // provider call surface, repo precedent
    async fn provision_instance(
        &self,
        host_id: Uuid,
        org_slug: &str,
        created_by: Option<&ProvisioningActor>,
        vpc_ref: &VpcRef,
        instance_type: &str,
        region: &str,
        image: &str,
        startup_script: &str,
        boot_disk_gb: u32,
    ) -> Result<CloudInstanceRef, CloudError>;

    /// Tear down a previously-provisioned instance. See ontology [D23].
    async fn terminate_instance(&self, cloud_instance_id: &str) -> Result<(), CloudError>;

    /// Poll the current backend state of an instance. See ontology [D20].
    async fn get_instance_status(
        &self,
        cloud_instance_id: &str,
    ) -> Result<InstanceState, CloudError>;

    /// Look up an instance by its deterministic name within `region`, without
    /// knowing the cloud_instance_id. Returns `Ok(Some(FoundInstance))` — the
    /// state plus the zone it was found in ([D61]) — when found, `Ok(None)`
    /// when every probed zone reports a 404, `Err` on other failures.
    ///
    /// This powers the orphan-reconciler's second pass (ontology [D39]):
    /// after the round-3 async-spawn fix to [D22], a handler dropped mid-
    /// bring-up (process restart, hard crash) can still leave a
    /// `cloud_instance_id IS NULL` row with a live GCE instance named by
    /// [`host_instance_name`] (legacy instances: `g5-host-{host_id}`, [D44]).
    /// The reconciler looks the instance up by that name (with a legacy-name
    /// fallback) and either attaches the cloud_instance_id (found) or counts
    /// the row toward the terminate threshold (not found).
    async fn get_instance_by_name(
        &self,
        region: &str,
        name: &str,
    ) -> Result<Option<FoundInstance>, CloudError>;

    /// List instances whose name starts with `prefix` across `region`'s
    /// primary-first UP-zone set (the same set provisioning can place
    /// instances in, [D59]) — the stale-host detector's cloud-orphan sweep
    /// (ontology [D65], issue #1464). Returns every match with its zone and
    /// state; an empty vec when nothing matches.
    async fn list_instances_by_prefix(
        &self,
        region: &str,
        prefix: &str,
    ) -> Result<Vec<ListedInstance>, CloudError>;

    /// The boot-disk image self-link / family the worker fleet boots from (the
    /// signed worker image from PR #114). Sourced from provider config so the
    /// wire-in handler ([D22]) need not embed GCP image specifics.
    fn worker_image(&self) -> &str;

    /// Resolve [`Self::worker_image`] (an image *family* or a specific image
    /// ref) to the concrete immutable GCE image the fleet currently boots from.
    /// Used per-pass by boot-image-aware rotation ([D56]) as the "desired" outer
    /// boot image, and at provision time to pin `sourceImage` + record the
    /// booted image on [`CloudInstanceRef::boot_image_id`]. See ontology [D56],
    /// issue #1424.
    async fn resolve_boot_image(&self) -> Result<BootImageIdentity, CloudError>;
}

// ─── MockCloudProvider ───────────────────────────────────────────────────────

/// A recorded call against [`MockCloudProvider`], for test assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCall {
    /// `ensure_vpc_for_org(org_id)`.
    EnsureVpc { org_id: Uuid },
    /// `provision_instance(...)` with the salient arguments.
    ProvisionInstance {
        /// The host the instance backs.
        host_id: Uuid,
        /// The org slug threaded into the instance name (ontology [D44]).
        org_slug: String,
        /// The provisioning user stamped as the creator labels ([D44]).
        created_by: Option<ProvisioningActor>,
        /// The VPC handle's `vpc_id`.
        vpc_id: String,
        /// The requested machine type.
        instance_type: String,
        /// The requested region.
        region: String,
        /// The boot image self-link/family.
        image: String,
        /// The full startup script (so tests can assert env plumbing).
        startup_script: String,
        /// The per-host boot-disk size the caller resolved (ontology
        /// `g5.runtime.placement.host-disk`), so tests can assert the size for
        /// a given VM budget without reaching GCE.
        boot_disk_gb: u32,
    },
    /// `terminate_instance(cloud_instance_id)`.
    TerminateInstance { cloud_instance_id: String },
    /// `get_instance_status(cloud_instance_id)`.
    GetInstanceStatus { cloud_instance_id: String },
    /// `get_instance_by_name(region, name)` (orphan-reconciler lookup).
    GetInstanceByName {
        /// The cloud region the lookup ran in.
        region: String,
        /// The deterministic instance name (e.g. `g5-host-{host_id}`).
        name: String,
    },
    /// `list_instances_by_prefix(region, prefix)` (stale-host detector
    /// cloud-orphan sweep, [D65]).
    ListInstancesByPrefix {
        /// The cloud region the listing ran in.
        region: String,
        /// The name prefix filter (e.g. `g5-host-`).
        prefix: String,
    },
}

/// In-process [`CloudProvider`] for tests. Records every call and returns
/// programmable canned results — the ONLY provider used in tests (no real GCP
/// call ever leaves the process). See ontology [D17].
///
/// Not feature-gated so integration tests in a separate crate
/// (`tests/api_worker_hosts_cloud.rs`) can construct it via
/// `g5_api::cloud::MockCloudProvider`.
pub struct MockCloudProvider {
    image: String,
    calls: Mutex<Vec<MockCall>>,
    next_instance: AtomicU64,
    statuses: Mutex<HashMap<String, InstanceState>>,
    /// Programmable `(region, name) → FoundInstance` map for
    /// `get_instance_by_name` (orphan-pass / gone-streak lookup). A present
    /// entry returns `Ok(Some(found))` with its state AND zone ([D61]); an
    /// absent entry returns `Ok(None)` (the cloud reports 404, the orphan
    /// terminate path for the reconciler).
    ///
    /// Keyed on `(region, name)` and NOT name alone so tests that pass the
    /// wrong region get `Ok(None)` — without this, the new
    /// `CloudProvider::get_instance_by_name(region, name)` API silently
    /// ignored the region argument it was added to exercise. Round-3 follow-up
    /// to coderabbitai's minor finding on the original 104 patch.
    by_name: Mutex<HashMap<(String, String), FoundInstance>>,
    /// Programmable `region → Vec<ListedInstance>` map for
    /// `list_instances_by_prefix` ([D65] cloud-orphan sweep). Entries whose
    /// name does not start with the requested prefix are filtered out at call
    /// time, matching the production GCE filter semantics. An absent region
    /// returns an empty listing.
    listings: Mutex<HashMap<String, Vec<ListedInstance>>>,
    /// Programmable outer boot image [`resolve_boot_image`] returns and that
    /// `provision_instance` stamps onto `CloudInstanceRef.boot_image_id` ( [D56]
    /// desired/recorded boot image). Advance it in a test to simulate a
    /// `g5-worker` family promotion.
    boot_image: Mutex<BootImageIdentity>,
    fail_ensure_vpc: AtomicBool,
    fail_provision: AtomicBool,
    /// Optional test-only delay before `provision_instance` resolves. This
    /// models a healthy provider call that outlives a narrow caller-side poll.
    provision_delay_ms: AtomicU64,
    /// Optional test-only delay before `resolve_boot_image` answers.
    resolve_boot_image_delay_ms: AtomicU64,
    /// When true, `provision_instance` fails with the canonical GCE
    /// exhausted-everywhere stockout shape (`ZONE_RESOURCE_POOL_EXHAUSTED`,
    /// HTTP 503) — classifies as `zone_capacity_exhausted` and, when retry is
    /// enabled, schedules a [D66] retry.
    fail_provision_stockout: AtomicBool,
    fail_terminate: AtomicBool,
    /// When true, `get_instance_by_name` returns a transport error (used to
    /// prove the reconciler treats lookup errors as "wait, don't increment").
    fail_get_by_name: AtomicBool,
    /// When true, `resolve_boot_image` errors and `provision_instance` records
    /// `boot_image_id = None` — exercises the [D56] best-effort no-signal path.
    fail_resolve_boot_image: AtomicBool,
}

impl Default for MockCloudProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCloudProvider {
    /// Construct a mock that succeeds on every call by default and reports
    /// freshly-provisioned instances as [`InstanceState::Provisioning`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            image: "projects/g5-test/global/images/family/g5-worker".to_string(),
            calls: Mutex::new(Vec::new()),
            next_instance: AtomicU64::new(1),
            statuses: Mutex::new(HashMap::new()),
            by_name: Mutex::new(HashMap::new()),
            listings: Mutex::new(HashMap::new()),
            boot_image: Mutex::new(BootImageIdentity {
                image_id: "g5-worker-mock-v1".to_string(),
                self_link: "projects/g5-test/global/images/g5-worker-mock-v1".to_string(),
            }),
            fail_ensure_vpc: AtomicBool::new(false),
            fail_provision: AtomicBool::new(false),
            provision_delay_ms: AtomicU64::new(0),
            resolve_boot_image_delay_ms: AtomicU64::new(0),
            fail_provision_stockout: AtomicBool::new(false),
            fail_terminate: AtomicBool::new(false),
            fail_get_by_name: AtomicBool::new(false),
            fail_resolve_boot_image: AtomicBool::new(false),
        }
    }

    /// Set the outer boot image [`resolve_boot_image`] resolves to and that
    /// `provision_instance` records — advance it to simulate a `g5-worker`
    /// family promotion ([D56]). `self_link` is derived from `image_id`.
    pub fn set_boot_image(&self, image_id: &str) {
        *self
            .boot_image
            .lock()
            .expect("mock boot_image lock poisoned") = BootImageIdentity {
            image_id: image_id.to_string(),
            self_link: format!("projects/g5-test/global/images/{image_id}"),
        };
    }

    /// Make `resolve_boot_image` fail (and `provision_instance` record a `None`
    /// boot image) — exercises the [D56] best-effort no-signal / fallback path.
    pub fn fail_resolve_boot_image(&self, fail: bool) {
        self.fail_resolve_boot_image.store(fail, Ordering::SeqCst);
    }

    /// Snapshot of every recorded call, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().expect("mock calls lock poisoned").clone()
    }

    /// Force the next `get_instance_status` for `cloud_instance_id` to report
    /// `state` (used to drive reconciler transitions deterministically).
    pub fn set_status(&self, cloud_instance_id: &str, state: InstanceState) {
        self.statuses
            .lock()
            .expect("mock statuses lock poisoned")
            .insert(cloud_instance_id.to_string(), state);
    }

    /// Make the next `ensure_vpc_for_org` fail.
    pub fn fail_ensure_vpc(&self, fail: bool) {
        self.fail_ensure_vpc.store(fail, Ordering::SeqCst);
    }

    /// Make the next `provision_instance` fail.
    pub fn fail_provision(&self, fail: bool) {
        self.fail_provision.store(fail, Ordering::SeqCst);
    }

    /// Delay `provision_instance` by `duration` without changing its result.
    /// The default is zero, so existing mock behavior remains immediate.
    pub fn set_provision_delay(&self, duration: std::time::Duration) {
        let delay_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        self.provision_delay_ms.store(delay_ms, Ordering::SeqCst);
    }

    /// Delay `resolve_boot_image` by `duration` without changing its result.
    pub fn set_resolve_boot_image_delay(&self, duration: std::time::Duration) {
        let delay_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        self.resolve_boot_image_delay_ms
            .store(delay_ms, Ordering::SeqCst);
    }

    /// Make `provision_instance` fail with the canonical GCE stockout shape
    /// (classifies as `zone_capacity_exhausted`, [D34]/[D66]).
    pub fn fail_provision_stockout(&self, fail: bool) {
        self.fail_provision_stockout.store(fail, Ordering::SeqCst);
    }

    /// Make the next `terminate_instance` fail.
    pub fn fail_terminate(&self, fail: bool) {
        self.fail_terminate.store(fail, Ordering::SeqCst);
    }

    /// Programmable hook for `get_instance_by_name`: register a
    /// `(region, name) → InstanceState` mapping found in the region's primary
    /// zone (`{region}-a`, the mock's stand-in for the configured primary).
    /// Absent entries return `Ok(None)`, matching the GCE 404 → `NotFound`
    /// semantics on the production path. Keyed on `(region, name)` so a test
    /// that mismatches the region argument gets a 404 (which is what the
    /// production GCE backend would return — GCE instances are zonal under
    /// the GET path).
    pub fn set_instance_by_name(&self, region: &str, name: &str, state: InstanceState) {
        self.set_instance_by_name_in_zone(region, name, state, &format!("{region}-a"));
    }

    /// [`Self::set_instance_by_name`] with an explicit found-zone — simulates
    /// an instance the [D59] stockout fallback placed outside the primary zone
    /// so tests can assert found-zone attach/repoint behavior ([D61]).
    pub fn set_instance_by_name_in_zone(
        &self,
        region: &str,
        name: &str,
        state: InstanceState,
        zone: &str,
    ) {
        self.by_name
            .lock()
            .expect("mock by_name lock poisoned")
            .insert(
                (region.to_string(), name.to_string()),
                FoundInstance {
                    state,
                    zone: zone.to_string(),
                },
            );
    }

    /// Remove a previously-set `(region, name)` mapping so the next lookup
    /// returns `Ok(None)` (used to flip a row from "found alive" to "missing"
    /// mid-test).
    pub fn forget_instance_by_name(&self, region: &str, name: &str) {
        self.by_name
            .lock()
            .expect("mock by_name lock poisoned")
            .remove(&(region.to_string(), name.to_string()));
    }

    /// Make `get_instance_by_name` return a transport error (the reconciler
    /// must treat this as "wait, don't increment the miss counter").
    pub fn fail_get_by_name(&self, fail: bool) {
        self.fail_get_by_name.store(fail, Ordering::SeqCst);
    }

    /// Program the `list_instances_by_prefix` result for `region` ([D65]
    /// cloud-orphan sweep tests). Replaces any prior listing for the region.
    pub fn set_region_listing(&self, region: &str, instances: Vec<ListedInstance>) {
        self.listings
            .lock()
            .expect("mock listings lock poisoned")
            .insert(region.to_string(), instances);
    }

    /// Remove one instance from a programmed region listing (simulates the
    /// instance disappearing between passes).
    pub fn remove_from_region_listing(&self, region: &str, name: &str) {
        if let Some(list) = self
            .listings
            .lock()
            .expect("mock listings lock poisoned")
            .get_mut(region)
        {
            list.retain(|i| i.name != name);
        }
    }

    fn record(&self, call: MockCall) {
        self.calls
            .lock()
            .expect("mock calls lock poisoned")
            .push(call);
    }
}

#[async_trait]
impl CloudProvider for MockCloudProvider {
    async fn ensure_vpc_for_org(&self, org_id: Uuid) -> Result<VpcRef, CloudError> {
        self.record(MockCall::EnsureVpc { org_id });
        if self.fail_ensure_vpc.load(Ordering::SeqCst) {
            return Err(CloudError::Api {
                status: 503,
                message: "mock ensure_vpc failure".to_string(),
            });
        }
        let name = format!("g5-org-{org_id}");
        Ok(VpcRef {
            vpc_id: name.clone(),
            region: "mock-region".to_string(),
            network_self_link: format!("mock://networks/{name}"),
            subnetwork_self_link: Some(format!("mock://subnetworks/{name}")),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn provision_instance(
        &self,
        host_id: Uuid,
        org_slug: &str,
        created_by: Option<&ProvisioningActor>,
        vpc_ref: &VpcRef,
        instance_type: &str,
        region: &str,
        image: &str,
        startup_script: &str,
        boot_disk_gb: u32,
    ) -> Result<CloudInstanceRef, CloudError> {
        self.record(MockCall::ProvisionInstance {
            host_id,
            org_slug: org_slug.to_string(),
            created_by: created_by.cloned(),
            vpc_id: vpc_ref.vpc_id.clone(),
            instance_type: instance_type.to_string(),
            region: region.to_string(),
            image: image.to_string(),
            startup_script: startup_script.to_string(),
            boot_disk_gb,
        });
        let provision_delay_ms = self.provision_delay_ms.load(Ordering::SeqCst);
        if provision_delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(provision_delay_ms)).await;
        }
        if self.fail_provision_stockout.load(Ordering::SeqCst) {
            return Err(CloudError::Api {
                status: 503,
                message: "Operation failed: ZONE_RESOURCE_POOL_EXHAUSTED (mock stockout)"
                    .to_string(),
            });
        }
        if self.fail_provision.load(Ordering::SeqCst) {
            return Err(CloudError::Api {
                status: 500,
                message: "mock provision failure".to_string(),
            });
        }
        let n = self.next_instance.fetch_add(1, Ordering::SeqCst);
        let cloud_instance_id = format!("mock-instance-{n}");
        self.statuses
            .lock()
            .expect("mock statuses lock poisoned")
            .insert(cloud_instance_id.clone(), InstanceState::Provisioning);
        // Resolve-and-pin ([D56]): record the current boot image, or `None` when
        // resolution is programmed to fail (the fallback path).
        let boot_image_id = if self.fail_resolve_boot_image.load(Ordering::SeqCst) {
            None
        } else {
            Some(
                self.boot_image
                    .lock()
                    .expect("mock boot_image lock poisoned")
                    .image_id
                    .clone(),
            )
        };
        Ok(CloudInstanceRef {
            cloud_instance_id,
            internal_ip: Some("10.128.0.2".to_string()),
            external_ip: None,
            zone: Some(format!("{region}-a")),
            boot_image_id,
        })
    }

    async fn terminate_instance(&self, cloud_instance_id: &str) -> Result<(), CloudError> {
        self.record(MockCall::TerminateInstance {
            cloud_instance_id: cloud_instance_id.to_string(),
        });
        if self.fail_terminate.load(Ordering::SeqCst) {
            return Err(CloudError::Api {
                status: 500,
                message: "mock terminate failure".to_string(),
            });
        }
        self.statuses
            .lock()
            .expect("mock statuses lock poisoned")
            .insert(cloud_instance_id.to_string(), InstanceState::Terminated);
        Ok(())
    }

    async fn get_instance_status(
        &self,
        cloud_instance_id: &str,
    ) -> Result<InstanceState, CloudError> {
        self.record(MockCall::GetInstanceStatus {
            cloud_instance_id: cloud_instance_id.to_string(),
        });
        Ok(self
            .statuses
            .lock()
            .expect("mock statuses lock poisoned")
            .get(cloud_instance_id)
            .copied()
            .unwrap_or(InstanceState::Unknown))
    }

    async fn get_instance_by_name(
        &self,
        region: &str,
        name: &str,
    ) -> Result<Option<FoundInstance>, CloudError> {
        self.record(MockCall::GetInstanceByName {
            region: region.to_string(),
            name: name.to_string(),
        });
        if self.fail_get_by_name.load(Ordering::SeqCst) {
            return Err(CloudError::Transport {
                message: "mock get_instance_by_name failure".to_string(),
            });
        }
        Ok(self
            .by_name
            .lock()
            .expect("mock by_name lock poisoned")
            .get(&(region.to_string(), name.to_string()))
            .cloned())
    }

    async fn list_instances_by_prefix(
        &self,
        region: &str,
        prefix: &str,
    ) -> Result<Vec<ListedInstance>, CloudError> {
        self.record(MockCall::ListInstancesByPrefix {
            region: region.to_string(),
            prefix: prefix.to_string(),
        });
        Ok(self
            .listings
            .lock()
            .expect("mock listings lock poisoned")
            .get(region)
            .map(|list| {
                list.iter()
                    .filter(|i| i.name.starts_with(prefix))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    fn worker_image(&self) -> &str {
        &self.image
    }

    async fn resolve_boot_image(&self) -> Result<BootImageIdentity, CloudError> {
        let delay_ms = self.resolve_boot_image_delay_ms.load(Ordering::SeqCst);
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        if self.fail_resolve_boot_image.load(Ordering::SeqCst) {
            return Err(CloudError::Api {
                status: 500,
                message: "mock resolve_boot_image failure".to_string(),
            });
        }
        Ok(self
            .boot_image
            .lock()
            .expect("mock boot_image lock poisoned")
            .clone())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn mock_provision_records_and_returns_instance() {
        let mock = MockCloudProvider::new();
        let org_id = Uuid::new_v4();
        let host_id = Uuid::new_v4();

        let vpc = mock.ensure_vpc_for_org(org_id).await.expect("ensure vpc");
        assert_eq!(vpc.vpc_id, format!("g5-org-{org_id}"));

        let inst = mock
            .provision_instance(
                host_id,
                "test-org",
                None,
                &vpc,
                "n2-standard-8",
                "us-central1",
                mock.worker_image(),
                "#!/bin/bash\n",
                365,
            )
            .await
            .expect("provision");
        assert_eq!(inst.cloud_instance_id, "mock-instance-1");
        assert_eq!(inst.zone.as_deref(), Some("us-central1-a"));

        // Freshly provisioned → Provisioning until set otherwise.
        let state = mock
            .get_instance_status(&inst.cloud_instance_id)
            .await
            .expect("status");
        assert_eq!(state, InstanceState::Provisioning);

        // Calls recorded in order with expected args.
        let calls = mock.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0], MockCall::EnsureVpc { org_id });
        match &calls[1] {
            MockCall::ProvisionInstance {
                host_id: h,
                instance_type,
                region,
                boot_disk_gb,
                ..
            } => {
                assert_eq!(*h, host_id);
                assert_eq!(instance_type, "n2-standard-8");
                assert_eq!(region, "us-central1");
                assert_eq!(*boot_disk_gb, 365);
            }
            other => panic!("expected ProvisionInstance, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mock_terminate_marks_terminated() {
        let mock = MockCloudProvider::new();
        let vpc = mock.ensure_vpc_for_org(Uuid::new_v4()).await.unwrap();
        let inst = mock
            .provision_instance(
                Uuid::new_v4(),
                "test-org",
                None,
                &vpc,
                "n2-standard-8",
                "us-central1",
                "img",
                "script",
                200,
            )
            .await
            .unwrap();

        mock.terminate_instance(&inst.cloud_instance_id)
            .await
            .expect("terminate");
        let state = mock
            .get_instance_status(&inst.cloud_instance_id)
            .await
            .unwrap();
        assert_eq!(state, InstanceState::Terminated);
    }

    #[tokio::test]
    async fn mock_status_is_programmable() {
        let mock = MockCloudProvider::new();
        mock.set_status("inst-x", InstanceState::Running);
        assert_eq!(
            mock.get_instance_status("inst-x").await.unwrap(),
            InstanceState::Running
        );
        // Unknown instance → Unknown.
        assert_eq!(
            mock.get_instance_status("nope").await.unwrap(),
            InstanceState::Unknown
        );
    }

    #[test]
    fn host_instance_name_includes_sanitized_slug() {
        let id = Uuid::parse_str("e2be17df-df50-4471-9f4e-8bfb9ff6c5a2").unwrap();
        // 18-char slug uses the full budget → exactly 63 chars.
        let name = host_instance_name(id, "zhengchun-test-org");
        assert_eq!(
            name,
            "g5-host-zhengchun-test-org-e2be17df-df50-4471-9f4e-8bfb9ff6c5a2"
        );
        assert_eq!(name.len(), 63);
        // Uppercase + underscores (nanoid-style fixtures) normalize.
        assert_eq!(
            host_instance_name(id, "Test_Org-X"),
            format!("g5-host-test-org-x-{id}")
        );
    }

    #[test]
    fn host_instance_name_truncates_and_trims() {
        let id = Uuid::nil();
        // 19+ chars truncate to the 18 budget…
        let name = host_instance_name(id, "abcdefghijklmnopqrstu");
        assert_eq!(name, format!("g5-host-abcdefghijklmnopqr-{id}"));
        assert!(name.len() <= 63);
        // …and a hyphen exposed at the cut is trimmed, not kept.
        assert_eq!(
            host_instance_name(id, "abcdefghijklmnopq-stu"),
            format!("g5-host-abcdefghijklmnopq-{id}")
        );
        // Consecutive separators collapse; edge hyphens trim.
        assert_eq!(
            host_instance_name(id, "--a__b--c--"),
            format!("g5-host-a-b-c-{id}")
        );
    }

    #[test]
    fn host_instance_name_falls_back_to_legacy_shape() {
        let id = Uuid::nil();
        let legacy = format!("g5-host-{id}");
        assert_eq!(host_instance_name(id, ""), legacy);
        assert_eq!(host_instance_name(id, "日本語"), legacy);
        assert_eq!(host_instance_name(id, "---"), legacy);
    }

    #[tokio::test]
    async fn mock_failures_are_injectable() {
        let mock = MockCloudProvider::new();
        mock.fail_provision(true);
        let vpc = mock.ensure_vpc_for_org(Uuid::new_v4()).await.unwrap();
        let err = mock
            .provision_instance(Uuid::new_v4(), "t-org", None, &vpc, "t", "r", "i", "s", 200)
            .await
            .expect_err("provision should fail when flagged");
        assert!(matches!(err, CloudError::Api { status: 500, .. }));
    }

    #[tokio::test]
    async fn concurrent_ensure_vpc_for_same_org_is_idempotent() {
        // Two concurrent first-create callers for the same org must both succeed
        // and observe the same VPC ref. The mock's `ensure_vpc_for_org` is
        // intrinsically idempotent (it computes the name from `org_id`); the
        // real GCP provider achieves the same shape by treating `HTTP 409
        // alreadyExists` on the `networks.insert` POST as success (see
        // `gcp::GcpCloudProvider::post_create`). This test pins the contract.
        let mock = Arc::new(MockCloudProvider::new());
        let org_id = Uuid::new_v4();

        let mock_a = mock.clone();
        let mock_b = mock.clone();
        let task_a = tokio::spawn(async move { mock_a.ensure_vpc_for_org(org_id).await });
        let task_b = tokio::spawn(async move { mock_b.ensure_vpc_for_org(org_id).await });

        let vpc_a = task_a.await.unwrap().expect("first ensure_vpc succeeded");
        let vpc_b = task_b.await.unwrap().expect("second ensure_vpc succeeded");

        assert_eq!(
            vpc_a.vpc_id, vpc_b.vpc_id,
            "both observers see the same VPC ref"
        );
        assert_eq!(vpc_a.vpc_id, format!("g5-org-{org_id}"));

        // Both callers were observed; neither call short-circuited the trait.
        let calls = mock.calls();
        assert_eq!(
            calls
                .iter()
                .filter(|c| matches!(c, MockCall::EnsureVpc { .. }))
                .count(),
            2
        );
    }
}
