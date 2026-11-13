//! Versioned worker-host control-plane protocol.
//!
//! One host-initiated `ctl` stream rides the authenticated host tunnel. The
//! stream uses newline-delimited JSON-RPC 2.0 messages. This module owns only
//! the wire contract; host execution and control-plane persistence live in
//! their respective crates.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 2;
/// v1 keeps existing VM control during rollout; only v2 publishes sizing.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[u32] = &[1, PROTOCOL_VERSION];
pub const DEFAULT_HEARTBEAT_SECS: u64 = 10;
pub const DEFAULT_RESULT_RETENTION_SECS: u64 = 600;
pub const MAX_LOG_TAIL_BYTES: usize = 64 * 1024;
pub const MAX_CTL_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_INFLIGHT_COMMANDS: usize = 16;

pub mod method {
    pub const HOST_HELLO: &str = "host.hello";
    pub const HOST_HEARTBEAT: &str = "host.heartbeat";
    pub const HOST_DRAIN: &str = "host.drain";
    pub const HOST_DRAINED: &str = "host.drained";
    pub const VM_START: &str = "vm.start";
    pub const VM_STOP: &str = "vm.stop";
    pub const VM_STATUS: &str = "vm.status";
    pub const VM_EVENT: &str = "vm.event";
    pub const VM_EVENT_ACK: &str = "vm.event.ack";
    pub const VM_FILE_CHANGED: &str = "vm.file_changed";
    /// Content-free hint that executions on a VM journaled new module events
    /// (`g5.runtime.module-event-hints`). Sent only after the control plane
    /// enabled it in `host.accept`.
    pub const VM_MODULE_EVENTS: &str = "vm.module_events";
    pub const LEASE_RENEW: &str = "lease.renew";
    /// Hosts no longer keep released workspace roots, so they answer this with
    /// `not_found` and no current control plane sends it. The method stays so a
    /// control plane from before that change can still finish a transfer or
    /// delete against a current host.
    pub const ROOTS_PURGE: &str = "roots.purge";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcMessage {
    Request(RpcRequest),
    Response(RpcResponse),
    Notification(RpcNotification),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcRequest {
    pub jsonrpc: JsonRpcVersion,
    pub id: String,
    pub method: String,
    pub params: Value,
}

impl RpcRequest {
    pub fn new<T: Serialize>(
        id: impl Into<String>,
        method: impl Into<String>,
        params: &T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            jsonrpc: JsonRpcVersion,
            id: id.into(),
            method: method.into(),
            params: serde_json::to_value(params)?,
        })
    }

    pub fn params<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.params.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcNotification {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    pub params: Value,
}

impl RpcNotification {
    pub fn new<T: Serialize>(
        method: impl Into<String>,
        params: &T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            jsonrpc: JsonRpcVersion,
            method: method.into(),
            params: serde_json::to_value(params)?,
        })
    }

    pub fn params<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.params.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcResponse {
    pub jsonrpc: JsonRpcVersion,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    pub fn result<T: Serialize>(
        id: impl Into<String>,
        result: &T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            jsonrpc: JsonRpcVersion,
            id: id.into(),
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    #[must_use]
    pub fn error(id: impl Into<String>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id: id.into(),
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    pub fn result_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.result.clone().unwrap_or(Value::Null))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JsonRpcVersion;

impl Serialize for JsonRpcVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let version = String::deserialize(deserializer)?;
        if version == "2.0" {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("jsonrpc must be 2.0"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostHello {
    pub host_id: Uuid,
    pub protocol_version: u32,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub slots_total: u32,
    /// Required for protocol v2; optional only to decode and reject old hellos.
    #[serde(default)]
    pub slot_size: Option<VmSizeSpec>,
    #[serde(default)]
    pub inventory: Vec<VmInventory>,
    /// Always empty: hosts delete a workspace root when its VM is released.
    /// Kept so hosts and control planes from before that change decode each
    /// other's messages; drop it with the next protocol version.
    #[serde(default)]
    pub retained_roots: Vec<RetainedRoot>,
    /// The host's runtime identity. A host from before this field, or one
    /// whose catalog cannot be read, publishes none and receives no placement
    /// (`g5.runtime.placement.runtime-identity`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<HostRuntimeIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HelloDisposition {
    Accept(HostAccept),
    Reject(HostReject),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostAccept {
    pub accepted: bool,
    pub protocol_version: u32,
    pub incarnation_generation: u64,
    #[serde(default)]
    pub current_incarnations: Vec<CurrentIncarnation>,
    pub config: HostCtlConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostReject {
    pub accepted: bool,
    pub reason: HostRejectReason,
    #[serde(default)]
    pub supported: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostRejectReason {
    UnsupportedVersion,
    IdentityMismatch,
    HostUnavailable,
    /// The host advertised an isolation level this control plane does not
    /// admit (`g5.runtime.boot.backend.process-local`): a process-isolation
    /// host is accepted only by a control plane started for local development.
    IsolationNotAdmitted,
}

/// Hello capabilities naming a host's isolation level
/// (`g5.runtime.boot.backend.process-local`). The control plane admits
/// [`CAPABILITY_ISOLATION_PROCESS`] only when [`ALLOW_PROCESS_HOSTS_ENV`] is
/// `true`, which only the local development stack sets.
pub const CAPABILITY_ISOLATION_MICROVM: &str = "isolation:cloud-hypervisor";
pub const CAPABILITY_ISOLATION_PROCESS: &str = "isolation:process";
pub const ALLOW_PROCESS_HOSTS_ENV: &str = "G5_ALLOW_PROCESS_ISOLATION_HOSTS";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostCtlConfig {
    pub heartbeat_secs: u64,
    pub result_retention_secs: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub source_preparation_enabled: bool,
    /// The control plane consumes `vm.module_events` hints; absent for older
    /// strict peers, which then never receive the notification.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub module_event_hints_enabled: bool,
}

impl Default for HostCtlConfig {
    fn default() -> Self {
        Self {
            heartbeat_secs: DEFAULT_HEARTBEAT_SECS,
            result_retention_secs: DEFAULT_RESULT_RETENTION_SECS,
            source_preparation_enabled: false,
            module_event_hints_enabled: false,
        }
    }
}

/// Capability negotiation keeps this optional payload away from older strict peers.
pub const CAPABILITY_SOURCE_PREPARATION: &str = "source-preparation:v1";
/// Hello capability of a host that can forward coalesced `vm.module_events`
/// hints from its dispatcher control connections.
pub const CAPABILITY_MODULE_EVENT_HINTS: &str = "module-event-hints:v1";
/// Most execution ids one `vm.module_events` hint names; a host coalesces
/// beyond that, which only delays a fetch to the next poll.
pub const MAX_MODULE_EVENT_HINT_EXECUTIONS: usize = 64;
/// Hello capability a host publishes when it boots workspace VMs with hardware
/// virtualization exposed (`g5.architecture.microvm_backend.nested`); fleet
/// visibility and a hook for future capability-based placement.
pub const CAPABILITY_NESTED_VIRTUALIZATION: &str = "nested-virtualization:v1";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSourcePreparation {
    pub org_id: Uuid,
    pub host_id: Uuid,
    pub source_workspace_id: Uuid,
    pub image_ref: String,
    pub membership_digest: String,
    pub repositories: Vec<VmRepoSpec>,
    pub expires_at: i64,
    pub read_token: String,
}
impl std::fmt::Debug for HostSourcePreparation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostSourcePreparation")
            .field("org_id", &self.org_id)
            .field("host_id", &self.host_id)
            .field("source_workspace_id", &self.source_workspace_id)
            .field("membership_digest", &self.membership_digest)
            .field("repositories", &self.repositories)
            .field("expires_at", &self.expires_at)
            .field("read_token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentIncarnation {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmInventory {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub state: HostVmState,
    pub boot_at: DateTime<Utc>,
}

/// A released workspace root, as hosts from before root deletion at release
/// reported it. Current hosts never report one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedRoot {
    pub workspace_id: Uuid,
    pub cache_generation: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostVmState {
    Booting,
    Ready,
    Running,
    Stopping,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostHeartbeat {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refresh_source_preparation: bool,
    pub slots_in_use: u32,
    pub boots_in_progress: u32,
    pub load: f64,
    pub memory_free_mib: u64,
    pub disk_free_mib: u64,
    /// Always empty; see [`HostHello::retained_roots`].
    #[serde(default)]
    pub retained_roots: Vec<RetainedRoot>,
    #[serde(default)]
    pub vms: Vec<VmHeartbeat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmHeartbeat {
    pub vm_id: Uuid,
    pub state: HostVmState,
    pub cpu_pct: f64,
    pub mem_mib: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatAck {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_preparation: Option<HostSourcePreparation>,
    pub incarnation_generation: u64,
    #[serde(default)]
    pub current_incarnations: Vec<CurrentIncarnation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmEvent {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub seq: u64,
    pub kind: VmEventKind,
    pub at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_tail: Option<String>,
}

impl VmEvent {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.seq == 0 {
            return Err("event sequence must start at one");
        }
        if self
            .log_tail
            .as_ref()
            .is_some_and(|tail| tail.len() > MAX_LOG_TAIL_BYTES)
        {
            return Err("event log tail exceeds 64 KiB");
        }
        if let VmEventKind::FileChanged { path, .. } = &self.kind {
            if path.len() > 4096 || path.contains('\0') {
                return Err("file change path is invalid");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmEventKind {
    Booted,
    Ready {
        epoch: u64,
        module_catalog_version: String,
        dispatcher_contract_version: u32,
    },
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
    Oom,
    StaleKilled,
    BootFailed {
        /// Diagnostics alone do not prove exit. A host that confirms cleanup
        /// journals a separate `Exited` event using the legacy wire format so
        /// older APIs preserve both facts and their replay identities.
        reason: String,
    },
    FileChanged {
        repo_id: Uuid,
        path: String,
        rescan_required: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmEventAck {
    pub vm_id: Uuid,
    pub through_seq: u64,
}

/// Ephemeral workspace-file invalidation. File contents remain authoritative
/// in the running workspace; losing or coalescing this hint only requires a
/// client rescan, so it must not consume the durable VM lifecycle journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmFileChanged {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub repo_id: Uuid,
    pub path: String,
    pub rescan_required: bool,
}

impl VmFileChanged {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.path.len() > 4096 || self.path.contains('\0') {
            return Err("file change path is invalid");
        }
        Ok(())
    }
}

/// Executions on one VM incarnation that journaled new module events since
/// the previous hint. Carries no event data: the guest journal remains the
/// source and ordering authority, and a lost or merged hint only delays the
/// control plane's next fetch to its poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmModuleEvents {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub exec_ids: Vec<String>,
}

impl VmModuleEvents {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.exec_ids.is_empty() || self.exec_ids.len() > MAX_MODULE_EVENT_HINT_EXECUTIONS {
            return Err("module event hint names an invalid number of executions");
        }
        if self
            .exec_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 256 || id.contains('\0'))
        {
            return Err("module event hint execution id is invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseRenew {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LeaseRenewResult {
    Granted(LeaseGrant),
    Stale(StaleLease),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseGrant {
    pub stale: bool,
    pub seq: u64,
    pub token: String,
    pub lease_ttl_secs: u64,
    pub incarnation_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaleLease {
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandParams<T> {
    pub idempotency_key: String,
    pub budget_ms: u64,
    #[serde(flatten)]
    pub command: T,
}

impl<T> CommandParams<T> {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.idempotency_key.is_empty() || self.idempotency_key.len() > 256 {
            return Err("invalid idempotency key");
        }
        if self.budget_ms == 0 {
            return Err("budget must be positive");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostDrain {
    pub drain_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostDrained {
    pub drain_id: Uuid,
}

/// Parameters of [`method::ROOTS_PURGE`], which current hosts answer with
/// `not_found`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootsPurge {
    pub workspace_id: Uuid,
    pub cache_generation: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmStart {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub spec: VmSpec,
    /// Incarnation credential for the trusted dispatcher. The host never
    /// persists this value or writes it to logs.
    pub workspace_token: String,
}

impl std::fmt::Debug for VmStart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VmStart")
            .field("workspace_id", &self.workspace_id)
            .field("vm_id", &self.vm_id)
            .field("spec", &self.spec)
            .field("workspace_token", &"[REDACTED]")
            .finish()
    }
}

pub const VM_SPEC_VERSION: u32 = 1;
pub const DISPATCHER_CONTRACT_VERSION: u32 = 1;

/// Spec env key carrying the guest's hostname (`workspace_hostname` of the
/// workspace name). The guest bootstrap sets the kernel nodename from it so
/// shells, `hostname` and `uname -n` name the workspace instead of the
/// kernel's `(none)`; the value is refreshed on every start.
pub const WORKSPACE_HOSTNAME_ENV: &str = "G5D_WORKSPACE_HOSTNAME";
pub const MAX_HOSTNAME_LEN: usize = 63;
const FALLBACK_HOSTNAME: &str = "workspace";

/// The hostname-safe form of a workspace name: lowercase ASCII letters,
/// digits and single hyphens, no leading or trailing hyphen, at most 63
/// bytes, never empty. Idempotent, so the guest re-applies it to whatever
/// it receives and still ends up with a valid RFC 1123 label.
#[must_use]
pub fn workspace_hostname(name: &str) -> String {
    let mut hostname = String::with_capacity(name.len().min(MAX_HOSTNAME_LEN));
    for ch in name.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '-',
        };
        if mapped == '-' && (hostname.is_empty() || hostname.ends_with('-')) {
            continue;
        }
        if hostname.len() == MAX_HOSTNAME_LEN {
            break;
        }
        hostname.push(mapped);
    }
    while hostname.ends_with('-') {
        hostname.pop();
    }
    if hostname.is_empty() {
        hostname.push_str(FALLBACK_HOSTNAME);
    }
    hostname
}

/// The module catalog this build ships (`g5d-dispatch/modules/catalog.json`)
/// with its pinned digest and version: what the control plane expects of a
/// host's runtime, and what the host image bakes under its runtime root.
pub const MODULE_CATALOG: &[u8] = include_bytes!("../../g5d-dispatch/modules/catalog.json");
pub const MODULE_CATALOG_HASH: &str =
    "28f10dc0ef1f33969c00047bd00bf2e8804365a92567f58a03cd05e13018f601";
pub const MODULE_CATALOG_VERSION: &str = "2026.11.10-agent-turn-output-unbounded";

/// A host's runtime identity (`g5.runtime.placement.runtime-identity`): the
/// facts a host checks every boot spec against, published on hello so the
/// control plane places only on hosts whose runtime matches its own bundle
/// instead of learning the mismatch from a rejected start.
/// Not `deny_unknown_fields`: a later field must not make an older control
/// plane drop the whole hello.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRuntimeIdentity {
    pub module_catalog_version: String,
    pub module_catalog_hash: String,
    pub dispatcher_contract_version: u32,
}

impl HostRuntimeIdentity {
    /// The identity this build expects of a host: its bundled catalog and
    /// dispatcher contract.
    #[must_use]
    pub fn bundled() -> Self {
        Self {
            module_catalog_version: MODULE_CATALOG_VERSION.to_string(),
            module_catalog_hash: MODULE_CATALOG_HASH.to_string(),
            dispatcher_contract_version: DISPATCHER_CONTRACT_VERSION,
        }
    }

    /// A host's own identity: the catalog under its runtime root, hashed as
    /// shipped, and the dispatcher contract its binary implements.
    pub fn read(runtime_root: &std::path::Path) -> std::io::Result<Self> {
        let catalog = std::fs::read(runtime_root.join("modules/catalog.json"))?;
        let parsed: Value = serde_json::from_slice(&catalog)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let module_catalog_version = parsed["catalog_version"]
            .as_str()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "runtime module catalog has no version",
                )
            })?
            .to_string();
        Ok(Self {
            module_catalog_version,
            module_catalog_hash: format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&catalog)),
            dispatcher_contract_version: DISPATCHER_CONTRACT_VERSION,
        })
    }

    /// Whether a host with this identity receives this build's placement:
    /// exactly the bundled identity, as the placement SQL compares it.
    #[must_use]
    pub fn is_current(&self) -> bool {
        *self == Self::bundled()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmSpec {
    pub spec_version: u32,
    pub dispatcher_contract_version: u32,
    pub incarnation: VmIncarnationSpec,
    pub image: VmImageSpec,
    pub size: VmSizeSpec,
    pub workspace_root: WorkspaceRootSpec,
    pub repos: Vec<VmRepoSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore: Option<VmRestoreSpec>,
    /// Checkouts beside the members, pinned at Start and not members: no
    /// workspace branch of their own, receipt or flush. A personal workspace
    /// gets Base's members (`g5.workspace.identity.reference`); any other
    /// workspace gets its owner's personal repository (`g5.personal.mount`),
    /// whose personal branch alone accepts pushes. Omitted from the wire when
    /// empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_repos: Vec<VmReferenceRepoSpec>,
    pub env: BTreeMap<String, String>,
    pub network: VmNetwork,
    pub entrypoint: Vec<String>,
    pub module_catalog_version: String,
    pub module_catalog_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmIncarnationSpec {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub org_id: Uuid,
    pub owner_user_id: Uuid,
    pub lease_ttl_secs: u64,
    pub idle_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmImageSpec {
    #[serde(rename = "ref")]
    pub image_ref: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmSizeSpec {
    pub cpus: u8,
    pub mem_mib: u32,
    pub scratch_disk_mib: u32,
}

/// Retention settings for the workspace root. Current hosts ignore both
/// fields and delete the root when the VM is released; the control plane
/// still sends them because hosts from before that change require the field.
/// Drop it with the next VM-spec version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRootSpec {
    pub retain_hours: u32,
    pub cache_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmRepoSpec {
    pub repo_id: Uuid,
    pub root_path: String,
    /// Authoritative workspace membership role. Older specs omit it; consumers
    /// must not infer a code repository from its name or path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub git_url: String,
    pub branch: String,
    pub branch_oid: String,
    /// Immediate parent ref selected when this attach operation was prepared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_branch: Option<String>,
    /// The guest fast-forwards this checkout when its branch moves
    /// (`g5.personal.follow`): set only for a personal workspace's private
    /// repository, which the owner's other workspaces also push. Omitted when
    /// false, so other workspaces' specs are unchanged on the wire.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow_branch: bool,
}

/// One reference checkout: Base's member, or the owner's personal repository,
/// `repo_id`, checked out writable on `branch` at `commit`, that branch's
/// durable head when Start was admitted. Unpushed edits are discarded at the
/// next Start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmReferenceRepoSpec {
    pub repo_id: Uuid,
    pub root_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub git_url: String,
    pub branch: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmRestoreSpec {
    pub flush_id: Uuid,
    pub repos: Vec<VmRestoreRepoSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmRestoreRepoSpec {
    pub repo_id: Uuid,
    pub branch_oid: String,
    pub local_branch_oid: String,
    pub local_head_oid: String,
    pub head_ref: Option<String>,
    pub sequencer: VmSequencer,
    pub snapshot_oid: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmSequencer {
    None,
    Rebase,
    Merge,
    CherryPick,
    Revert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmNetwork {
    /// The only guest NIC backend: virtio-net through the supervised passt
    /// vhost-user sidecar. The field stays on the wire so API and worker
    /// hosts of neighbouring releases keep exchanging `VmSpec` unchanged.
    Passt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopMode {
    Graceful,
    Kill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmStop {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub mode: StopMode,
    pub grace_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmStatus {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotUnavailableReason {
    Slots,
    Disk,
    BootCapacity,
    WorkspaceBusy,
    Draining,
}

impl SlotUnavailableReason {
    /// The wire name, as recorded with the refused start.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slots => "slots",
            Self::Disk => "disk",
            Self::BootCapacity => "boot_capacity",
            Self::WorkspaceBusy => "workspace_busy",
            Self::Draining => "draining",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecRejectReason {
    UnsupportedSpecVersion,
    OrgMismatch,
    IdentityMismatch,
    InvalidPath,
    SizeMismatch,
    CredentialInEnv,
    ImageDigestMismatch,
    RuntimeMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmStatusReport {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub state: HostVmState,
    pub boot_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_at: Option<DateTime<Utc>>,
    pub resources: VmSizeSpec,
    pub streams_open: u32,
}

/// Host-measured boot evidence for one incarnation. The host writes it as
/// the `ready` event's bounded `log_tail` (and its own log) so the control
/// plane and the qualification harness can record cold, restart and
/// new-child observations without a protocol change. The format is one line of
/// `key=value` fields; unknown fields are ignored. These are comparable
/// observations, not a latency SLA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSummary {
    /// Wall time from `vm.start` admission (root preparation, restore, VMM
    /// launch and dispatcher validation) to the validated `runtime.ready`.
    pub boot_to_ready_ms: u64,
    /// Object-store growth across the restore fetches, in KiB.
    pub restore_fetch_kib: u64,
    /// Whether the host booted this VM with hardware virtualization exposed
    /// (`g5.architecture.microvm_backend.nested`). Hosts from before the
    /// switch existed omit the field; it reads as `false`.
    pub nested_virtualization: bool,
}

impl std::fmt::Display for BootSummary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "boot_to_ready_ms={} restore_fetch_kib={} nested_virtualization={}",
            self.boot_to_ready_ms,
            self.restore_fetch_kib,
            if self.nested_virtualization {
                "on"
            } else {
                "off"
            }
        )
    }
}

impl std::str::FromStr for BootSummary {
    type Err = String;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut boot_to_ready_ms = None;
        let mut restore_fetch_kib = None;
        let mut nested_virtualization = false;
        for field in line.split_whitespace() {
            match field.split_once('=') {
                Some(("boot_to_ready_ms", value)) => {
                    boot_to_ready_ms =
                        Some(value.parse().map_err(|error| format!("{field}: {error}"))?)
                }
                Some(("restore_fetch_kib", value)) => {
                    restore_fetch_kib =
                        Some(value.parse().map_err(|error| format!("{field}: {error}"))?)
                }
                Some(("nested_virtualization", value)) => {
                    nested_virtualization = match value {
                        "on" => true,
                        "off" => false,
                        other => return Err(format!("{field}: expected on or off, got {other}")),
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            boot_to_ready_ms: boot_to_ready_ms
                .ok_or_else(|| "boot summary lacks boot_to_ready_ms".to_string())?,
            restore_fetch_kib: restore_fetch_kib
                .ok_or_else(|| "boot summary lacks restore_fetch_kib".to_string())?,
            nested_virtualization,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CommandResult {
    Accepted,
    Started,
    AlreadyRunning,
    Expired,
    Busy,
    IdempotencyConflict,
    Draining,
    Purged,
    NotFound,
    WorkspaceBusy,
    SlotUnavailable {
        reason: SlotUnavailableReason,
    },
    SpecRejected {
        reason: SpecRejectReason,
    },
    BootFailed {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        log_tail: Option<String>,
    },
    Stopped {
        exit: VmExit,
    },
    Status {
        vm: VmStatusReport,
    },
    Unsupported {
        command: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn workspace_hostname_is_a_valid_idempotent_label() {
        assert_eq!(
            workspace_hostname("issue-685-workspace-terminal-prompt"),
            "issue-685-workspace-terminal-prompt"
        );
        assert_eq!(
            workspace_hostname("Feature/Add Login!!"),
            "feature-add-login"
        );
        assert_eq!(workspace_hostname("--a__b.c--"), "a-b-c");
        assert_eq!(workspace_hostname("ünïcode näme"), "n-code-n-me");
        assert_eq!(workspace_hostname(""), "workspace");
        assert_eq!(workspace_hostname("///"), "workspace");
        let long = format!("{}-{}", "a".repeat(62), "b".repeat(10));
        let truncated = workspace_hostname(&long);
        assert_eq!(truncated, "a".repeat(62));
        assert!(truncated.len() <= MAX_HOSTNAME_LEN);
        for name in [
            "issue-685-workspace-terminal-prompt",
            "Feature/Add Login!!",
            &long,
            "",
        ] {
            let once = workspace_hostname(name);
            assert_eq!(workspace_hostname(&once), once, "{name:?}");
            assert!(once
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
            assert!(!once.starts_with('-') && !once.ends_with('-'));
        }
    }

    #[test]
    fn module_catalog_hash_matches_shipped_catalog() {
        assert_eq!(
            format!(
                "{:x}",
                <sha2::Sha256 as sha2::Digest>::digest(MODULE_CATALOG)
            ),
            MODULE_CATALOG_HASH
        );
        assert_eq!(
            include_str!("../../g5d-dispatch/modules/catalog.sha256").trim(),
            MODULE_CATALOG_HASH
        );
        assert_eq!(
            serde_json::from_slice::<Value>(MODULE_CATALOG).unwrap()["catalog_version"],
            MODULE_CATALOG_VERSION
        );
    }

    /// The identity a host reads from a runtime root holding the shipped
    /// catalog is the bundled one; any other catalog, or none, is not current.
    #[test]
    fn runtime_identity_read_from_the_shipped_catalog_is_current() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("modules")).unwrap();
        std::fs::write(root.path().join("modules/catalog.json"), MODULE_CATALOG).unwrap();
        let identity = HostRuntimeIdentity::read(root.path()).unwrap();
        assert_eq!(identity, HostRuntimeIdentity::bundled());
        assert!(identity.is_current());

        std::fs::write(
            root.path().join("modules/catalog.json"),
            br#"{"catalog_version":"older","modules":[]}"#,
        )
        .unwrap();
        let older = HostRuntimeIdentity::read(root.path()).unwrap();
        assert_eq!(older.module_catalog_version, "older");
        assert!(!older.is_current());

        std::fs::remove_file(root.path().join("modules/catalog.json")).unwrap();
        assert!(HostRuntimeIdentity::read(root.path()).is_err());
    }

    /// A hello from a host before the runtime field still decodes, without
    /// an identity; a current hello round-trips its identity.
    #[test]
    fn hello_runtime_identity_is_optional_on_the_wire() {
        let legacy: HostHello = serde_json::from_value(json!({
            "host_id": Uuid::nil(),
            "protocol_version": 2,
            "slots_total": 1,
        }))
        .unwrap();
        assert_eq!(legacy.runtime, None);
        let mut current = legacy.clone();
        current.runtime = Some(HostRuntimeIdentity::bundled());
        let wire = serde_json::to_value(&current).unwrap();
        assert_eq!(wire["runtime"]["module_catalog_hash"], MODULE_CATALOG_HASH);
        assert_eq!(serde_json::from_value::<HostHello>(wire).unwrap(), current);
    }

    #[test]
    fn json_rpc_version_rejects_other_versions() {
        let request = json!({
            "jsonrpc": "1.0",
            "id": "hello",
            "method": method::HOST_HELLO,
            "params": {}
        });
        assert!(serde_json::from_value::<RpcRequest>(request).is_err());
    }

    #[test]
    fn command_params_flatten_budget_and_payload() {
        let params = CommandParams {
            idempotency_key: "drain-once".to_string(),
            budget_ms: 5_000,
            command: HostDrain {
                drain_id: Uuid::nil(),
            },
        };
        let request = RpcRequest::new("cmd-1", method::HOST_DRAIN, &params).unwrap();
        assert_eq!(request.params["idempotency_key"], "drain-once");
        assert_eq!(request.params["budget_ms"], 5_000);
        assert_eq!(request.params["drain_id"], Uuid::nil().to_string());
        assert_eq!(
            request.params::<CommandParams<HostDrain>>().unwrap(),
            params
        );
    }

    #[test]
    fn boot_summary_round_trips_and_reads_older_hosts_warm_root_field() {
        let summary = BootSummary {
            boot_to_ready_ms: 13_151,
            restore_fetch_kib: 1_042,
            nested_virtualization: false,
        };
        assert_eq!(
            summary.to_string(),
            "boot_to_ready_ms=13151 restore_fetch_kib=1042 nested_virtualization=off"
        );
        assert_eq!(summary.to_string().parse::<BootSummary>(), Ok(summary));
        // A host from before root deletion at release still appends
        // `warm_root`; the field is ignored like any other unknown field. A
        // host from before the nested switch omits its token: read as off.
        assert_eq!(
            "boot_to_ready_ms=13151 restore_fetch_kib=1042 warm_root=true".parse::<BootSummary>(),
            Ok(summary)
        );
        let nested = BootSummary {
            boot_to_ready_ms: 13_151,
            restore_fetch_kib: 1_042,
            nested_virtualization: true,
        };
        assert_eq!(nested.to_string().parse::<BootSummary>(), Ok(nested));
        assert!(
            "boot_to_ready_ms=1 restore_fetch_kib=0 nested_virtualization=yes"
                .parse::<BootSummary>()
                .is_err()
        );
    }

    #[test]
    fn event_kind_round_trips_without_post_mvp_variants() {
        let event = VmEvent {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            seq: 1,
            kind: VmEventKind::Ready {
                epoch: 2,
                module_catalog_version: "catalog-v1".to_string(),
                dispatcher_contract_version: 1,
            },
            at: Utc::now(),
            log_tail: None,
        };
        event.validate().unwrap();
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["kind"]["kind"], "ready");
        assert_eq!(serde_json::from_value::<VmEvent>(value).unwrap(), event);
    }

    #[test]
    fn rpc_message_distinguishes_request_response_and_notification() {
        let request = RpcMessage::Request(
            RpcRequest::new("hello", method::HOST_HELLO, &json!({"x": 1})).unwrap(),
        );
        let response =
            RpcMessage::Response(RpcResponse::result("hello", &json!({"accepted": true})).unwrap());
        let notification = RpcMessage::Notification(
            RpcNotification::new(
                method::HOST_DRAINED,
                &HostDrained {
                    drain_id: Uuid::nil(),
                },
            )
            .unwrap(),
        );
        for message in [request, response, notification] {
            let encoded = serde_json::to_vec(&message).unwrap();
            assert_eq!(
                serde_json::from_slice::<RpcMessage>(&encoded).unwrap(),
                message
            );
        }
    }

    #[test]
    fn module_event_hint_round_trips_and_its_config_flag_stays_away_from_older_peers() {
        let hint = VmModuleEvents {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            epoch: 4,
            exec_ids: vec!["task.a.1.vm".into()],
        };
        let encoded = serde_json::to_value(&hint).unwrap();
        assert_eq!(
            serde_json::from_value::<VmModuleEvents>(encoded).unwrap(),
            hint
        );
        assert!(hint.validate().is_ok());
        assert!(VmModuleEvents {
            exec_ids: Vec::new(),
            ..hint.clone()
        }
        .validate()
        .is_err());
        assert!(VmModuleEvents {
            exec_ids: (0..=MAX_MODULE_EVENT_HINT_EXECUTIONS)
                .map(|i| format!("task.{i}"))
                .collect(),
            ..hint.clone()
        }
        .validate()
        .is_err());

        // An accept from before the flag decodes, and a disabled flag is not
        // serialized, so a strict older host never sees an unknown field.
        let older = json!({"heartbeat_secs": 30, "result_retention_secs": 600});
        let config: HostCtlConfig = serde_json::from_value(older).unwrap();
        assert!(!config.module_event_hints_enabled);
        let encoded = serde_json::to_value(&config).unwrap();
        assert!(encoded.get("module_event_hints_enabled").is_none());
        let enabled = HostCtlConfig {
            module_event_hints_enabled: true,
            ..HostCtlConfig::default()
        };
        assert_eq!(
            serde_json::to_value(&enabled).unwrap()["module_event_hints_enabled"],
            json!(true)
        );
    }
}
