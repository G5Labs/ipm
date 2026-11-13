//! Declared-module runtime: catalog resolution, invocation dispositions,
//! journaled events, native-reply delivery, slot accounting and diagnostics.
//!
//! This is the *sole* platform execution path on a workspace VM (design v9
//! §6.6–§6.7). Callers name a catalog entry, an operation and typed input;
//! they never supply argv, environment, cwd or a signal target. `proc.spawn`
//! exists only as [`runner`], reached after resolution and grant intersection.
//!
//! Concurrency model: `admit` serialises every identity decision (invoke
//! check-and-reserve, cancel, finish) so two handlers can never both launch
//! or both settle one `exec_id`; `emit_lock` keeps journal order and stream
//! order identical; `delivery` serialises native replies with each other and
//! with the terminal settlement, so the outcome is always the last event.

pub mod builtin;
pub mod catalog;
pub mod exec_id;
pub mod redact;
pub mod runner;
mod steering;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Instant;

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub use catalog::{Catalog, CatalogError, Lifecycle, ModuleEntry};
pub use exec_id::{ExecId, ExecKind};
pub use redact::Secrets;

use crate::config::DispatchConfig;
use crate::git_transport::GitTransport;
use crate::journal::{digest_of, Disposition, Journal, JournalEvent};
use crate::protocol::{Admission, MutationPermit, RpcError, RpcResult};

/// Bounded per-execution log retention served by `modules.logs`.
pub const MAX_RETAINED_LOG_BYTES: usize = 256 * 1024;
/// Bounded validated native response accepted by `modules.input.respond`.
pub const MAX_VALIDATED_RESPONSE_BYTES: usize = 256 * 1024;
/// Executions remembered for `modules.history` after they finish.
const MAX_HISTORY: usize = 512;
/// The operation a module declares to accept native replies on its control
/// channel (Agent API §5: `operations.input.respond`).
pub const INPUT_RESPOND_OPERATION: &str = "input.respond";
pub const INPUT_STEER_OPERATION: &str = "input.steer";
/// The operation a module declares to accept the owner's End for a
/// provider's background wait on its control channel.
pub const BACKGROUND_END_OPERATION: &str = "background.end";
/// How long a delivery waits for the module transport to take the bytes.
/// The forwarder itself abandons and closes the channel at this bound, so a
/// negative disposition is never contradicted by a later write.
pub const DELIVERY_ACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Dispatch-resolved secret values are bounded before they enter a module's
/// private environment. They never appear in the journaled invocation input.
pub const MAX_DISPATCH_SECRETS: usize = 128;
pub const MAX_DISPATCH_SECRET_VALUE_BYTES: usize = 1024 * 1024;

/// Capability names understood by this runtime. The control plane issues a
/// grant as a list of these; a manifest requests a list of these.
pub mod capability {
    pub const WORKSPACE_READ: &str = "workspace.read";
    pub const WORKSPACE_WRITE: &str = "workspace.write";
    pub const NATIVE_FILES: &str = "workspace.native_files";
    pub const NATIVE_GIT: &str = "workspace.native_git";
    pub const NATIVE_SHELL: &str = "workspace.native_shell";
    pub const NATIVE_TESTS: &str = "workspace.native_tests";
    pub const PORTS_BOUNDED: &str = "ports.bounded";
    pub const PREVIEW_SERVICE: &str = "preview.service";
    pub const NETWORK_AGENT_CONTROL_PLANE: &str = "network.agent_control_plane";
    pub const NETWORK_AGENT_PROVIDER: &str = "network.agent_provider";
}

/// Reserved keys that would let a caller steer the process rather than the
/// module. They are refused *as input keys* (N98/N106): ordinary prompt text
/// that merely contains these words is fine, because only typed object keys
/// at the top level of `input` or `params` are inspected.
const FORBIDDEN_CONTROL_KEYS: &[&str] = &[
    "argv",
    "args",
    "command",
    "cmd",
    "env",
    "environment",
    "cwd",
    "working_dir",
    "signal",
    "kill",
    "pid",
    "entrypoint",
    "uid",
    "gid",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub grant_id: uuid::Uuid,
    pub capabilities: Vec<String>,
}

/// A journaled event ready to stream: JSON-RPC notification shape.
pub fn event_notification(exec_id: &ExecId, event: &JournalEvent) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "module.event",
        "params": {
            "exec_id": exec_id.as_str(),
            "seq": event.seq,
            "kind": event.kind,
            "payload": event.payload,
            "terminal": event.terminal,
        }
    })
}

#[derive(Clone, Debug)]
pub struct ReplyRecord {
    pub provider_request_id: String,
    pub response_digest: String,
    pub disposition: &'static str,
    /// The delivery disposition reached the journal. A transport result
    /// without journal evidence is acknowledged only once it is persisted.
    pub journaled: bool,
}

/// A message on an execution's private control channel. Only the runtime
/// writes to it; the module reads it from its stdin. Repository descendants
/// never inherit it. `ack` reports whether the bytes reached the transport.
#[derive(Debug)]
pub enum ControlMessage {
    Steer {
        line: Value,
        ack: oneshot::Sender<bool>,
    },
    /// A validated provider response for a live native request.
    Respond {
        request_id: String,
        provider_request_id: String,
        response_digest: String,
        validated_response: Value,
        ack: oneshot::Sender<bool>,
    },
    /// The owner's End for the provider's background wait.
    EndBackgroundWait {
        request_id: String,
        ack: oneshot::Sender<bool>,
    },
}

pub enum ExecState {
    Running {
        cancel: CancellationToken,
        control: mpsc::Sender<ControlMessage>,
    },
    Finished {
        outcome: Value,
    },
}

pub struct Execution {
    pub exec_id: ExecId,
    pub module_id: String,
    pub module_version: String,
    pub manifest_hash: String,
    pub operation: String,
    pub input_digest: String,
    pub grant_id: uuid::Uuid,
    /// The validated actor the facade authenticated for this invocation
    /// (the current owner for public invokes).
    pub actor_user_id: Option<uuid::Uuid>,
    pub capabilities: Vec<String>,
    pub lifecycle: Lifecycle,
    /// The module declared `input.respond`, so native replies may be routed.
    pub accepts_input: bool,
    /// The module declared `background.end`, so the owner's End may be routed.
    pub accepts_end: bool,
    pub accepts_steer: bool,
    pub state: ExecState,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    /// False when the terminal event or disposition could not be persisted;
    /// the in-memory outcome is then the only copy and is reported as such.
    pub terminal_persisted: bool,
    pub logs: Vec<u8>,
    pub log_bytes_dropped: u64,
    pub replies: HashMap<String, ReplyRecord>,
    steers: HashMap<String, steering::SteerRecord>,
    _permit: Option<MutationPermit>,
}

impl Execution {
    fn audit(&self, config: &DispatchConfig) -> Value {
        json!({
            "workspace_id": config.workspace_id,
            "vm_id": config.vm_id,
            "exec_id": self.exec_id.as_str(),
            "module_id": self.module_id,
            "version": self.module_version,
            "manifest_hash": self.manifest_hash,
            "grant_id": self.grant_id,
            "actor_user_id": self.actor_user_id,
            "input_digest": self.input_digest,
            "capability_grant": self.capabilities,
            "operation": self.operation,
            "lifecycle": self.lifecycle,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
            "status": match &self.state {
                ExecState::Running { .. } => json!("running"),
                ExecState::Finished { outcome } => json!({"finished": outcome}),
            },
            "persistence": if self.terminal_persisted { "journaled" } else { "failed" },
        })
    }
}

struct Slots {
    bounded_in_use: u32,
    per_module: HashMap<String, u32>,
}

pub struct ModuleRuntime {
    pub config: Arc<DispatchConfig>,
    pub catalog: Catalog,
    pub journal: Arc<Journal>,
    secrets: RwLock<Secrets>,
    /// Bumped by every `add_secret`; streaming readers refresh on change.
    secrets_version: AtomicU64,
    admission: Arc<Admission>,
    git: Arc<crate::git::GitRuntime>,
    transport: OnceLock<Arc<GitTransport>>,
    /// Serializes platform Git ref transfers within this incarnation.
    git_operations: Arc<tokio::sync::Mutex<()>>,
    /// Weak self-reference for execution-owned tasks; set by [`Self::arc`].
    self_handle: OnceLock<std::sync::Weak<Self>>,
    executions: Mutex<HashMap<ExecId, Execution>>,
    /// Ports the `workspace.preview` module reported ready, per execution.
    /// The module's `preview.ready` event admits its port to the HTTP stream
    /// gate without waiting for the next listener scan. Cleared when the
    /// execution settles.
    service_ports: Mutex<HashMap<ExecId, u16>>,
    history: Mutex<Vec<ExecId>>,
    slots: Mutex<Slots>,
    /// Serialises identity decisions: invoke check-and-reserve, cancel, finish.
    admit: Mutex<()>,
    /// Journal order and stream order are the same order.
    emit_lock: Mutex<()>,
    /// Serialises native-reply deliveries with each other and with settlement.
    /// Owned guards let an execution-owned delivery task outlive the handler
    /// that requested it.
    delivery: Arc<tokio::sync::Mutex<()>>,
    /// Journaled events fan out to every connected `rpc` peer from here.
    pub events: broadcast::Sender<Value>,
    /// Where the pinned catalog was loaded from; the integrity probe re-hashes it.
    catalog_path: std::path::PathBuf,
    /// Live runtime observation (ports, health, resources) supplied by the
    /// server so diagnostics never reach into it directly.
    observer: Mutex<Option<Box<dyn Fn() -> Value + Send + Sync>>>,
    /// The terminal manager that hosts a preview's application output
    /// (`g5.preview.output.live-tail`), installed by the server. Absent in
    /// unit fixtures, where modules spawn without a TTY as before.
    terminal_host: OnceLock<Arc<g5d_foundation::terminal::TerminalManager>>,
    started: Instant,
}

impl ModuleRuntime {
    /// Composition root for the runtime; the server wires every collaborator once.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<DispatchConfig>,
        catalog: Catalog,
        catalog_path: std::path::PathBuf,
        journal: Arc<Journal>,
        secrets: Secrets,
        admission: Arc<Admission>,
        git: Arc<crate::git::GitRuntime>,
        events: broadcast::Sender<Value>,
    ) -> Self {
        Self {
            config,
            catalog,
            journal,
            secrets: RwLock::new(secrets),
            secrets_version: AtomicU64::new(0),
            admission,
            git,
            transport: OnceLock::new(),
            git_operations: Arc::new(tokio::sync::Mutex::new(())),
            self_handle: OnceLock::new(),
            executions: Mutex::new(HashMap::new()),
            service_ports: Mutex::new(HashMap::new()),
            history: Mutex::new(Vec::new()),
            slots: Mutex::new(Slots {
                bounded_in_use: 0,
                per_module: HashMap::new(),
            }),
            admit: Mutex::new(()),
            emit_lock: Mutex::new(()),
            delivery: Arc::new(tokio::sync::Mutex::new(())),
            events,
            catalog_path,
            observer: Mutex::new(None),
            terminal_host: OnceLock::new(),
            started: Instant::now(),
        }
    }

    /// Install the terminal manager that hosts preview output sessions. The
    /// server calls this once; a second call is ignored.
    pub fn set_terminal_host(&self, terminals: Arc<g5d_foundation::terminal::TerminalManager>) {
        let _ = self.terminal_host.set(terminals);
    }

    /// The terminal manager hosting preview output, when the server wired one.
    pub fn terminal_host(&self) -> Option<Arc<g5d_foundation::terminal::TerminalManager>> {
        self.terminal_host.get().cloned()
    }

    /// Record the port a running `workspace.preview` execution reported
    /// ready. Any other module, or an execution that is not running, is
    /// ignored: only the preview service may open the HTTP gate.
    pub fn note_service_port(&self, exec_id: &ExecId, port: u16) -> bool {
        if port == 0 {
            return false;
        }
        let admitted = {
            let executions = self.executions.lock().unwrap();
            executions.get(exec_id).is_some_and(|execution| {
                execution.module_id == builtin::PREVIEW
                    && matches!(execution.state, ExecState::Running { .. })
            })
        };
        if admitted {
            self.service_ports
                .lock()
                .unwrap()
                .insert(exec_id.clone(), port);
        }
        admitted
    }

    /// Whether a running preview execution reported `port` ready.
    pub fn service_port_active(&self, port: u16) -> bool {
        let ports = self.service_ports.lock().unwrap();
        let executions = self.executions.lock().unwrap();
        ports.iter().any(|(exec_id, candidate)| {
            *candidate == port
                && executions
                    .get(exec_id)
                    .is_some_and(|execution| matches!(execution.state, ExecState::Running { .. }))
        })
    }

    fn forget_service_port(&self, exec_id: &ExecId) {
        self.service_ports.lock().unwrap().remove(exec_id);
    }

    /// Test seam: a running execution record for `module_id` without a
    /// process behind it.
    #[cfg(test)]
    pub(crate) fn register_running_execution_for_test(&self, exec_id: &ExecId, module_id: &str) {
        let (control, _) = mpsc::channel(1);
        self.executions.lock().unwrap().insert(
            exec_id.clone(),
            Execution {
                exec_id: exec_id.clone(),
                module_id: module_id.to_owned(),
                module_version: "test".to_owned(),
                manifest_hash: "test".to_owned(),
                operation: "start".to_owned(),
                input_digest: String::new(),
                grant_id: uuid::Uuid::new_v4(),
                actor_user_id: None,
                capabilities: Vec::new(),
                lifecycle: Lifecycle::Service,
                accepts_input: false,
                accepts_end: false,
                accepts_steer: false,
                state: ExecState::Running {
                    cancel: CancellationToken::new(),
                    control,
                },
                started_at: crate::protocol::now_ms(),
                ended_at: None,
                terminal_persisted: false,
                logs: Vec::new(),
                log_bytes_dropped: 0,
                replies: HashMap::new(),
                steers: HashMap::new(),
                _permit: None,
            },
        );
    }

    /// Wrap the runtime in the `Arc` its execution-owned tasks reach it by.
    pub fn arc(self) -> Arc<Self> {
        let runtime = Arc::new(self);
        let _ = runtime.self_handle.set(Arc::downgrade(&runtime));
        runtime
    }

    /// A shared handle to this runtime for execution-owned tasks.
    fn clone_handle(&self) -> Arc<Self> {
        self.self_handle
            .get()
            .and_then(|weak| weak.upgrade())
            .expect("module runtime is always constructed behind an Arc")
    }

    pub fn set_observer(&self, observer: Box<dyn Fn() -> Value + Send + Sync>) {
        *self.observer.lock().unwrap() = Some(observer);
    }

    /// Bounded live observation: `{"health":…, "ports":[…], "resources":…}`.
    pub fn observe(&self) -> Value {
        match self.observer.lock().unwrap().as_ref() {
            Some(observer) => observer(),
            None => json!({"health":{},"ports":[],"resources":{}}),
        }
    }

    pub fn set_transport(&self, transport: Arc<GitTransport>) {
        let _ = self.transport.set(transport);
    }

    /// The token-isolated internal Git transport, when the server started one.
    pub fn transport(&self) -> Option<&Arc<GitTransport>> {
        self.transport.get()
    }

    pub fn catalog_path(&self) -> &std::path::Path {
        &self.catalog_path
    }

    /// Snapshot of every configured secret value for redaction.
    pub fn secrets(&self) -> Secrets {
        self.secrets.read().unwrap().clone()
    }

    /// Register a further secret value (a rotated lease token, a supervisor
    /// capability) so later logs, events and outcomes redact it too.
    pub fn add_secret(&self, value: &str) {
        let mut secrets = self.secrets.write().unwrap();
        *secrets = secrets.with(value);
        self.secrets_version.fetch_add(1, Ordering::SeqCst);
    }

    /// Changes whenever the secret set grows; streaming readers compare it.
    pub fn secrets_version(&self) -> u64 {
        self.secrets_version.load(Ordering::SeqCst)
    }

    /// Settle every execution whose supervision this process did not
    /// witness to the end: a restart within the incarnation lost the process
    /// tree, so the identity gets a definite terminal outcome instead of a
    /// second launch — `interrupted` when undecided, `cancelled` when a
    /// cancellation was already recorded, the recorded outcome when the
    /// disposition was written but the terminal event was not. Called once
    /// by the server before it listens; a persistence failure refuses startup.
    pub fn finalize_interrupted(&self) -> Result<(), RpcError> {
        for (exec_id, disposition) in self.journal.interrupted().to_vec() {
            let outcome = match &disposition {
                None => runner::outcome("interrupted", json!({"reason":"supervision_lost"})),
                Some(Disposition::Cancelled { reason, settled }) => {
                    settled.clone().unwrap_or_else(|| {
                        runner::outcome("cancelled", json!({"reason":reason,"supervision":"lost"}))
                    })
                }
                Some(Disposition::AlreadyFinished { outcome, .. }) => outcome.clone(),
            };
            // What is retained is exactly what is journaled: the recorded
            // settlement and decision were sanitised when they were written,
            // so the outcome is never rescanned — the settlement (a cancelled
            // identity keeps its decision and gains the outcome), then the
            // terminal event carrying the very same value.
            self.journal
                .record_settlement(&exec_id, &outcome)
                .map_err(journal_error)?;
            self.emit_sanitized(&exec_id, "invocation.outcome", outcome, true)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Catalog surface
    // -----------------------------------------------------------------------

    pub fn list(&self) -> Value {
        json!({
            "catalog_version": self.catalog.catalog_version,
            "catalog_hash": self.catalog.content_hash,
            "bounded_execution_slots": self.catalog.bounded_execution_slots,
            "modules": self.catalog.modules.iter().map(|m| json!({
                "module_id": m.module_id,
                "module_version": m.module_version,
                "manifest_hash": m.manifest_hash,
                "lifecycle": m.lifecycle,
                "operations": m.operations.keys().collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }

    pub fn describe(&self, module_id: &str, version: Option<&str>) -> RpcResult {
        let module = self
            .catalog
            .find(module_id, version)
            .ok_or_else(|| RpcError::new(404, "MODULE_NOT_FOUND"))?;
        Ok(serde_json::to_value(module).unwrap_or(Value::Null))
    }

    // -----------------------------------------------------------------------
    // Invocation
    // -----------------------------------------------------------------------

    fn disposition_reply(disposition: Disposition) -> Value {
        disposition.to_value()
    }

    /// `modules.invoke`. Idempotent on `exec_id`; returns a disposition and
    /// never spawns for an identity with a recorded cancellation. Everything
    /// that decides the identity's fate runs under `admit`.
    pub async fn invoke(self: &Arc<Self>, params: &Value, actor: Option<uuid::Uuid>) -> RpcResult {
        reject_control_overrides(params)?;
        let exec_id = parse_exec_id(params, &self.config)?;
        let module_id = params["module_id"]
            .as_str()
            .ok_or_else(|| RpcError::new(-32602, "INVALID_MODULE_ID"))?;
        let version = params["version"].as_str();
        let operation = params["operation"]
            .as_str()
            .ok_or_else(|| RpcError::new(-32602, "INVALID_OPERATION"))?;
        let input = params.get("input").cloned().unwrap_or(json!({}));
        if !input.is_object() {
            return Err(RpcError::new(-32602, "INVALID_INPUT"));
        }
        reject_control_overrides(&input)?;
        let dispatch_secrets = parse_dispatch_secrets(params.get("secrets"))?;
        let grant: Grant =
            serde_json::from_value(params.get("grant").cloned().unwrap_or(Value::Null))
                .map_err(|_| RpcError::new(-32602, "INVALID_GRANT"))?;

        // 1. Resolve the immutable catalog entry and the operation. Pure;
        //    nothing is reserved yet.
        let module = self
            .catalog
            .find(module_id, version)
            .cloned()
            .ok_or_else(|| RpcError::new(404, "MODULE_NOT_FOUND"))?;
        if module.module_id == builtin::BOOTSTRAP && exec_id.kind() != ExecKind::Task {
            return Err(RpcError::new(-32602, "INVALID_EXEC_ID"));
        }
        if [
            INPUT_RESPOND_OPERATION,
            BACKGROUND_END_OPERATION,
            INPUT_STEER_OPERATION,
        ]
        .contains(&operation)
        {
            // Control-channel operations are reached through their own
            // methods (modules.input.respond, modules.background.end) only.
            return Err(RpcError::new(404, "OPERATION_NOT_FOUND"));
        }
        let schemas = module
            .operations
            .get(operation)
            .ok_or_else(|| RpcError::new(404, "OPERATION_NOT_FOUND"))?
            .clone();
        if let Some(schema) = params["input_schema"].as_str() {
            if schema != schemas.input_schema {
                return Err(RpcError {
                    code: -32602,
                    name: "INPUT_SCHEMA_MISMATCH",
                    data: json!({"expected":schemas.input_schema}),
                });
            }
        }
        // Built-in operations have strict typed inputs; a malformed or
        // undeclared field is refused before anything is reserved or spawned.
        if module.builtin || module.module_id == builtin::PREVIEW {
            builtin::validate_input(&module.module_id, operation, &input)?;
        }
        if module.module_id == "agent.turn" {
            let app_server = input.pointer("/provider/options/codex_transport");
            if matches!(
                module.module_version.as_str(),
                "1.0.8" | "1.0.10" | "1.0.12" | "1.0.14"
            )
                != app_server.is_some()
                || app_server.is_some_and(|v| {
                    v.as_str() != Some(g5d_agent::runner::codex_app_server::TRANSPORT)
                })
                || (app_server.is_some() && input["provider"]["provider_id"] != "codex")
                // This guest has no exec launcher: an exec-contract Codex
                // attempt is refused, never converted (`g5.chat.codex-exec-retirement`).
                || (app_server.is_none() && input["provider"]["provider_id"] == "codex")
            {
                return Err(RpcError {
                    code: -32602,
                    name: "CODEX_TRANSPORT_CONTRACT_MISMATCH",
                    data: json!({"message":"Codex transport must match its frozen module contract"}),
                });
            }
            let formal = input.pointer("/provider/options/formal_review");
            if matches!(
                module.module_version.as_str(),
                "1.0.6" | "1.0.7" | "1.0.9" | "1.0.10" | "1.0.11" | "1.0.12" | "1.0.13" | "1.0.14"
            ) != formal.is_some()
            {
                return Err(RpcError {
                    code: -32602,
                    name: "FORMAL_REVIEW_MODULE_REQUIRED",
                    data: json!({"message":"formal review requires its dedicated module contract"}),
                });
            }
            if let Some(formal) = formal {
                let parsed = serde_json::from_value::<g5_contracts::types::FormalApprovalInvocation>(
                    formal.clone(),
                );
                if parsed.as_ref().is_err()
                    || parsed.as_ref().is_ok_and(|review| {
                        review.validate().is_err()
                            || !formal_module_accepts(
                                &module.module_version,
                                review.resolution.schema_version,
                            )
                    })
                {
                    return Err(RpcError {
                        code: -32602,
                        name: "INVALID_FORMAL_REVIEW",
                        data: json!({}),
                    });
                }
            }
        }
        // 2. Capability intersection. A grant can only narrow a manifest.
        //    A preview-scoped grant authorises workspace.preview only.
        let capabilities = module.intersect(&grant.capabilities);
        let preview_scoped = grant
            .capabilities
            .iter()
            .any(|c| c == capability::PREVIEW_SERVICE);
        if preview_scoped && module.module_id != "workspace.preview" {
            return Err(RpcError {
                code: 403,
                name: "CAPABILITY_DENIED",
                data: json!({"reason":"a preview grant cannot authorize this module"}),
            });
        }
        if capabilities.len() != module.requested_capabilities.len() {
            let missing: Vec<&String> = module
                .requested_capabilities
                .iter()
                .filter(|c| !capabilities.contains(c))
                .collect();
            return Err(RpcError {
                code: 403,
                name: "CAPABILITY_DENIED",
                data: json!({"missing":missing}),
            });
        }
        // P4.8 deliberately removes the invoke-bound bootstrap capability.
        // During the MVP the trusted workspace.bootstrap supervisor uses the
        // incarnation credential already owned by this dispatcher, and the
        // API revalidates the current running command attempt before effects.
        if params.get("supervisor_capability").is_some() {
            return Err(RpcError::new(-32602, "UNDECLARED_FIELD"));
        }

        // 3. Decide the identity under `admit`: durable disposition first,
        //    then live state, then reservation and the durable start event.
        let input_digest = digest_of(&input);
        let cancel = CancellationToken::new();
        let (control_tx, control_rx) = mpsc::channel(32);
        {
            let _admit = self.admit.lock().unwrap();
            if let Some(disposition) = self.journal.disposition(&exec_id).map_err(journal_error)? {
                return Ok(Self::disposition_reply(disposition));
            }
            {
                let executions = self.executions.lock().unwrap();
                if let Some(existing) = executions.get(&exec_id) {
                    return Ok(match &existing.state {
                        ExecState::Running { .. } => json!({"disposition":"already_running"}),
                        ExecState::Finished { outcome } => json!({
                            "disposition":"already_finished",
                            "outcome":outcome,
                            "outcome_digest":digest_of(outcome),
                        }),
                    });
                }
            }
            let permit = self.admission.enter()?;
            {
                let mut slots = self.slots.lock().unwrap();
                let per_module = slots
                    .per_module
                    .entry(module.module_id.clone())
                    .or_insert(0);
                if *per_module >= module.limits.concurrency {
                    return Err(RpcError {
                        code: 409,
                        name: "MODULE_CONCURRENCY_EXHAUSTED",
                        data: json!({"module_id":module.module_id,"concurrency":module.limits.concurrency}),
                    });
                }
                if module.lifecycle.is_bounded() {
                    if slots.bounded_in_use >= self.catalog.bounded_execution_slots {
                        return Err(RpcError {
                            code: 409,
                            name: "EXECUTION_SLOTS_EXHAUSTED",
                            data: json!({"bounded_execution_slots":self.catalog.bounded_execution_slots,"in_use":slots.bounded_in_use,"state":"busy"}),
                        });
                    }
                    slots.bounded_in_use += 1;
                }
                *slots.per_module.get_mut(&module.module_id).unwrap() += 1;
            }
            for secret in dispatch_secrets.values() {
                self.add_secret(secret);
            }
            self.executions.lock().unwrap().insert(
                exec_id.clone(),
                Execution {
                    exec_id: exec_id.clone(),
                    module_id: module.module_id.clone(),
                    module_version: module.module_version.clone(),
                    manifest_hash: module.manifest_hash.clone(),
                    operation: operation.to_owned(),
                    input_digest: input_digest.clone(),
                    grant_id: grant.grant_id,
                    actor_user_id: actor,
                    capabilities: capabilities.clone(),
                    lifecycle: module.lifecycle,
                    accepts_input: module.operations.contains_key(INPUT_RESPOND_OPERATION),
                    accepts_end: module.operations.contains_key(BACKGROUND_END_OPERATION),
                    accepts_steer: module.operations.contains_key(INPUT_STEER_OPERATION),
                    state: ExecState::Running {
                        cancel: cancel.clone(),
                        control: control_tx,
                    },
                    started_at: crate::protocol::now_ms(),
                    ended_at: None,
                    terminal_persisted: false,
                    logs: Vec::new(),
                    log_bytes_dropped: 0,
                    replies: HashMap::new(),
                    steers: HashMap::new(),
                    _permit: Some(permit),
                },
            );
            // The intent must be durable before anything runs. If the
            // journal refuses it, nothing was launched: release the record,
            // the permit and the slots so the identity can be retried rather
            // than stuck as `already_running` with no process behind it.
            if let Err(error) = self.emit(
                &exec_id,
                "invocation.started",
                json!({
                    "module_id": module.module_id,
                    "version": module.module_version,
                    "manifest_hash": module.manifest_hash,
                    "operation": operation,
                    "input_digest": input_digest,
                    "grant_id": grant.grant_id,
                    "actor_user_id": actor,
                    "capability_grant": capabilities,
                }),
                false,
            ) {
                self.executions.lock().unwrap().remove(&exec_id);
                self.release_slots(&module);
                return Err(error);
            }
        }
        // 4. Spawn outside the lock; settlement re-enters it.
        let runtime = self.clone();
        let exec = exec_id.clone();
        let operation = operation.to_owned();
        tokio::spawn(async move {
            let outcome = runner::run(
                runtime.clone(),
                &exec,
                &module,
                operation,
                input,
                capabilities,
                dispatch_secrets,
                cancel,
                control_rx,
            )
            .await;
            runtime.finish(&exec, &module, outcome).await;
        });
        Ok(json!({"disposition":"started"}))
    }

    /// Journal every delivery disposition that reached the transport but not
    /// yet the journal. Returns whether all evidence is now durable.
    fn persist_unjournaled_replies(&self, exec_id: &ExecId) -> bool {
        let pending: Vec<(String, String, String, &'static str)> = {
            let executions = self.executions.lock().unwrap();
            executions
                .get(exec_id)
                .map(|execution| {
                    execution
                        .replies
                        .iter()
                        .filter(|(_, record)| !record.journaled && record.disposition != "pending")
                        .map(|(request_id, record)| {
                            (
                                request_id.clone(),
                                record.provider_request_id.clone(),
                                record.response_digest.clone(),
                                record.disposition,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut complete = true;
        for (request_id, provider_request_id, response_digest, disposition) in pending {
            let journaled = self.emit(
                exec_id,
                "input.respond",
                json!({
                    "request_id":request_id,"provider_request_id":provider_request_id,
                    "response_digest":response_digest,"disposition":disposition,
                }),
                false,
            );
            match journaled {
                Ok(_) => {
                    if let Some(record) = self
                        .executions
                        .lock()
                        .unwrap()
                        .get_mut(exec_id)
                        .and_then(|e| e.replies.get_mut(&request_id))
                    {
                        record.journaled = true;
                    }
                }
                Err(error) => {
                    tracing::error!(
                        exec_id = exec_id.as_str(),
                        request_id,
                        ?error,
                        "dispatcher could not persist delivery evidence at settlement"
                    );
                    complete = false;
                }
            }
        }
        complete
    }

    fn release_slots(&self, module: &ModuleEntry) {
        let mut slots = self.slots.lock().unwrap();
        if module.lifecycle.is_bounded() {
            slots.bounded_in_use = slots.bounded_in_use.saturating_sub(1);
        }
        if let Some(count) = slots.per_module.get_mut(&module.module_id) {
            *count = count.saturating_sub(1);
        }
    }

    /// Settle an execution: journal the terminal event, retain the
    /// disposition, transition the live record, release slots and the
    /// admission permit. Runs after any in-flight delivery, so the outcome is
    /// the journal's last event; a persistence failure keeps the outcome in
    /// memory and is reported instead of hidden.
    async fn finish(&self, exec_id: &ExecId, module: &ModuleEntry, outcome: Value) {
        let _delivery = self.delivery.clone().lock_owned().await;
        // Redacted exactly once: this value is what the settlement retains,
        // what the terminal event carries, what status and a retried invoke
        // return, and what the digest binds. Redaction is not idempotent for
        // every secret (one that is a substring of the mask), so no surface
        // rescans it.
        let outcome = self.secrets().redact_json(&outcome);
        // The incarnation-long settlement record is written first: once the
        // terminal event exists it may be acknowledged and pruned, and the
        // retained outcome must be exactly the one that was delivered (a
        // cancelled identity keeps its decision and gains the outcome). If it
        // cannot be written, the terminal event is not journaled either.
        let recorded = match self.journal.record_settlement(exec_id, &outcome) {
            Ok(_) => true,
            Err(error) => {
                tracing::error!(exec_id = exec_id.as_str(), %error, "dispatcher could not record a settlement");
                false
            }
        };
        // Every delivery whose journal evidence is still missing is persisted
        // before the journal closes; otherwise that evidence could never be
        // written and the settlement is reported as unpersisted.
        let evidence = recorded && self.persist_unjournaled_replies(exec_id);
        let journaled = evidence
            && match self.emit_sanitized(exec_id, "invocation.outcome", outcome.clone(), true) {
                Ok(_) => true,
                Err(error) => {
                    tracing::error!(
                        exec_id = exec_id.as_str(),
                        ?error,
                        "dispatcher could not journal a terminal outcome"
                    );
                    false
                }
            };
        let _admit = self.admit.lock().unwrap();
        {
            let mut executions = self.executions.lock().unwrap();
            if let Some(execution) = executions.get_mut(exec_id) {
                execution.state = ExecState::Finished { outcome };
                execution.ended_at = Some(crate::protocol::now_ms());
                execution.terminal_persisted = journaled && recorded;
                execution._permit = None;
            }
        }
        self.forget_service_port(exec_id);
        self.release_slots(module);
        let mut history = self.history.lock().unwrap();
        history.push(exec_id.clone());
        if history.len() > MAX_HISTORY {
            let evicted = history.remove(0);
            let mut executions = self.executions.lock().unwrap();
            // An outcome that only exists in memory is never evicted.
            if executions
                .get(&evicted)
                .is_some_and(|e| e.terminal_persisted)
            {
                executions.remove(&evicted);
            }
        }
    }

    /// Journal an event, then stream it, under one lock so the stream never
    /// carries `seq` N+1 before N.
    pub fn emit(
        &self,
        exec_id: &ExecId,
        kind: &str,
        payload: Value,
        terminal: bool,
    ) -> Result<JournalEvent, RpcError> {
        let payload = self.secrets().redact_json(&payload);
        self.emit_sanitized(exec_id, kind, payload, terminal)
    }

    /// Journal and stream a payload that is already sanitised (a settlement
    /// redacted once at `finish`, a recovered settlement written sanitised).
    fn emit_sanitized(
        &self,
        exec_id: &ExecId,
        kind: &str,
        payload: Value,
        terminal: bool,
    ) -> Result<JournalEvent, RpcError> {
        let _order = self.emit_lock.lock().unwrap();
        let event = self
            .journal
            .append(exec_id, kind, payload, terminal)
            .map_err(journal_error)?;
        self.observe_steering(exec_id, &event);
        let _ = self.events.send(event_notification(exec_id, &event));
        Ok(event)
    }

    pub fn append_log(&self, exec_id: &ExecId, bytes: &[u8]) {
        let mut executions = self.executions.lock().unwrap();
        if let Some(execution) = executions.get_mut(exec_id) {
            let room = MAX_RETAINED_LOG_BYTES.saturating_sub(execution.logs.len());
            let take = bytes.len().min(room);
            execution.logs.extend_from_slice(&bytes[..take]);
            execution.log_bytes_dropped += (bytes.len() - take) as u64;
        }
    }

    pub fn status(&self, params: &Value) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        let executions = self.executions.lock().unwrap();
        Ok(match executions.get(&exec_id) {
            None => match self.journal.disposition(&exec_id).map_err(journal_error)? {
                Some(Disposition::AlreadyFinished { outcome, .. }) => {
                    json!({"status":"finished","outcome":outcome,"persistence":"journaled"})
                }
                Some(Disposition::Cancelled {
                    reason,
                    settled: Some(outcome),
                }) => {
                    json!({"status":"finished","outcome":outcome,"cancelled":reason,"persistence":"journaled"})
                }
                Some(Disposition::Cancelled {
                    reason,
                    settled: None,
                }) => {
                    json!({"status":"cancelled","reason":reason})
                }
                None => json!({"status":"unknown"}),
            },
            Some(execution) => match &execution.state {
                ExecState::Running { .. } => json!({"status":"running"}),
                ExecState::Finished { outcome } => json!({
                    "status":"finished",
                    "outcome":outcome,
                    "persistence": if execution.terminal_persisted { "journaled" } else { "failed" },
                }),
            },
        })
    }

    /// `modules.cancel`. Records a cancellation disposition even for an
    /// identity that has never been invoked, so a delayed invoke cannot
    /// launch cancelled work. Decided under `admit` like invoke.
    pub fn cancel(&self, params: &Value) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        // The reason is retained for the incarnation and echoed on every
        // surface: it never carries a configured secret — redacted whole,
        // before the display bound can cut a secret into a retained prefix.
        let reason = self
            .secrets()
            .redact_str(params["reason"].as_str().unwrap_or("cancelled"))
            .chars()
            .take(256)
            .collect::<String>();
        let _admit = self.admit.lock().unwrap();
        // The durable table answers first, exactly as `invoke` reads it, so
        // a cancelled identity reports `cancelled` on every surface.
        if let Some(disposition) = self.journal.disposition(&exec_id).map_err(journal_error)? {
            return Ok(Self::disposition_reply(disposition));
        }
        let live_cancel = {
            let executions = self.executions.lock().unwrap();
            match executions.get(&exec_id).map(|e| &e.state) {
                Some(ExecState::Finished { outcome }) => {
                    return Ok(
                        json!({"disposition":"already_finished","outcome":outcome,"outcome_digest":digest_of(outcome)}),
                    );
                }
                Some(ExecState::Running { cancel, .. }) => Some(cancel.clone()),
                None => None,
            }
        };
        let recorded = self
            .journal
            .record_disposition(
                &exec_id,
                &Disposition::Cancelled {
                    reason: reason.clone(),
                    settled: None,
                },
            )
            .map_err(journal_error)?;
        Ok(match recorded {
            Disposition::AlreadyFinished { .. } => Self::disposition_reply(recorded),
            Disposition::Cancelled { reason, .. } => match live_cancel {
                Some(cancel) => {
                    // A closing preview leaves the HTTP gate at once: its
                    // app is being stopped and cleaned up, so no new stream
                    // may reach a half-torn-down application.
                    self.forget_service_port(&exec_id);
                    cancel.cancel();
                    json!({"disposition":"signalling","reason":reason})
                }
                None => json!({"disposition":"cancelled","reason":reason}),
            },
        })
    }

    // -----------------------------------------------------------------------
    // Native-reply delivery primitive
    // -----------------------------------------------------------------------

    /// `modules.input.respond`. Delivers one validated provider response to a
    /// live execution's declared control channel. Deliveries are serialised;
    /// the immutable request binding is reserved before delivery, the
    /// transport acknowledges the bytes before `delivered` is journaled, and
    /// identical retries return the stored disposition without redelivery.
    pub async fn respond(&self, params: &Value, idempotency_key: &str) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        let request_id = string_field(params, "request_id", 128)?;
        if idempotency_key != request_id {
            return Err(RpcError {
                code: -32602,
                name: "INVALID_IDEMPOTENCY_KEY",
                data: json!({"reason":"idempotency_key must equal request_id"}),
            });
        }
        let provider_request_id = string_field(params, "provider_request_id", 256)?;
        let response_digest = string_field(params, "response_digest", 128)?.to_ascii_lowercase();
        let task_id = string_field(params, "task_id", 64)?;
        let attempt = params["attempt"]
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "INVALID_ATTEMPT"))?;
        let expected = format!("task.{task_id}.{attempt}.{}", self.config.vm_id);
        // Only the task variant is admitted here, and the tuple must spell the
        // same execution: an operation identity cannot receive provider input.
        if exec_id.kind() != ExecKind::Task || exec_id.as_str() != expected {
            return Ok(json!({"disposition":"stale_execution"}));
        }
        let validated_response = params
            .get("validated_response")
            .cloned()
            .ok_or_else(|| RpcError::new(-32602, "INVALID_RESPONSE"))?;
        let encoded = serde_json::to_vec(&validated_response).unwrap_or_default();
        if encoded.len() > MAX_VALIDATED_RESPONSE_BYTES {
            return Err(RpcError::new(413, "PAYLOAD_TOO_LARGE"));
        }
        // The digest the control plane bound on acceptance must match the
        // bytes it is now delivering; a mismatch is a conflicting answer.
        let actual_digest = format!("{:x}", Sha256::digest(&encoded));
        if actual_digest != response_digest {
            return Err(RpcError {
                code: 409,
                name: "RESPONSE_DIGEST_MISMATCH",
                data: json!({"expected":response_digest,"actual":actual_digest}),
            });
        }

        // One delivery at a time: overlapping retries observe each other's
        // reservation, and settlement cannot interleave with delivery. The
        // guard is owned so the delivery task below can carry it.
        let guard = self.delivery.clone().lock_owned().await;
        let control = {
            let mut executions = self.executions.lock().unwrap();
            let Some(execution) = executions.get_mut(&exec_id) else {
                return Ok(json!({"disposition":"stale_execution"}));
            };
            if !execution.accepts_input {
                return Err(RpcError {
                    code: -32602,
                    name: "INPUT_CHANNEL_UNDECLARED",
                    data: json!({"module_id":execution.module_id}),
                });
            }
            if let Some(previous) = execution.replies.get(&request_id) {
                // The first accepted binding is immutable: the same request
                // with a different answer or a different provider request
                // identity is a conflict, never a second delivery.
                if previous.response_digest != response_digest
                    || previous.provider_request_id != provider_request_id
                {
                    return Err(RpcError {
                        code: 409,
                        name: "RESPONSE_CONFLICT",
                        data: json!({"request_id":request_id,"accepted_digest":previous.response_digest,"accepted_provider_request_id":previous.provider_request_id}),
                    });
                }
                // An identical retry must not redeliver; the recorded
                // disposition is returned for the attempt's lifetime — but
                // only once its journal evidence exists. A transport result
                // whose journal write failed is persisted now, without
                // touching the transport again.
                let disposition = match previous.disposition {
                    "delivered" | "already_delivered" => "already_delivered",
                    other => other,
                };
                let recorded = previous.disposition;
                let needs_journal = !previous.journaled;
                drop(executions);
                if needs_journal {
                    self.emit(
                        &exec_id,
                        "input.respond",
                        json!({
                            "request_id":request_id,"provider_request_id":provider_request_id,
                            "response_digest":response_digest,"disposition":recorded,
                        }),
                        false,
                    )?;
                    if let Some(record) = self
                        .executions
                        .lock()
                        .unwrap()
                        .get_mut(&exec_id)
                        .and_then(|e| e.replies.get_mut(&request_id))
                    {
                        record.journaled = true;
                    }
                }
                return Ok(json!({"disposition":disposition,"request_id":request_id}));
            }
            match &execution.state {
                ExecState::Running { control, .. } => {
                    // Reserve the immutable binding before any byte moves:
                    // whatever happens to this handler, no second delivery
                    // of this request can start.
                    execution.replies.insert(
                        request_id.clone(),
                        ReplyRecord {
                            provider_request_id: provider_request_id.clone(),
                            response_digest: response_digest.clone(),
                            disposition: "pending",
                            journaled: false,
                        },
                    );
                    control.clone()
                }
                ExecState::Finished { .. } => {
                    // The journal is closed by the outcome; the post-terminal
                    // disposition is retained in memory for the attempt.
                    execution.replies.insert(
                        request_id.clone(),
                        ReplyRecord {
                            provider_request_id: provider_request_id.clone(),
                            response_digest: response_digest.clone(),
                            disposition: "request_gone",
                            journaled: true,
                        },
                    );
                    return Ok(json!({"disposition":"request_gone","request_id":request_id}));
                }
            }
        };
        // The delivery is owned by the execution, not by this RPC handler: a
        // deadline or a replaced connection drops the handler, never the
        // half-done delivery, so its disposition is always recorded.
        let (done_tx, done_rx) = oneshot::channel();
        let runtime = self.clone_handle();
        let exec = exec_id.clone();
        let (request, provider, digest) = (
            request_id.clone(),
            provider_request_id.clone(),
            response_digest.clone(),
        );
        tokio::spawn(async move {
            let _guard = guard;
            let (ack_tx, ack_rx) = oneshot::channel();
            let queued = control
                .send(ControlMessage::Respond {
                    request_id: request.clone(),
                    provider_request_id: provider.clone(),
                    response_digest: digest.clone(),
                    validated_response,
                    ack: ack_tx,
                })
                .await
                .is_ok();
            // The forwarder abandons the write at the same bound and closes
            // the channel, so a timeout here is never followed by a write.
            let delivered = queued
                && matches!(
                    tokio::time::timeout(DELIVERY_ACK_TIMEOUT * 2, ack_rx).await,
                    Ok(Ok(true))
                );
            let disposition = if delivered {
                "delivered"
            } else {
                "request_gone"
            };
            {
                let mut executions = runtime.executions.lock().unwrap();
                if let Some(execution) = executions.get_mut(&exec) {
                    execution.replies.insert(
                        request.clone(),
                        ReplyRecord {
                            provider_request_id: provider.clone(),
                            response_digest: digest.clone(),
                            disposition,
                            journaled: false,
                        },
                    );
                }
            }
            let journaled = runtime.emit(
                &exec,
                "input.respond",
                json!({
                    "request_id":request,"provider_request_id":provider,
                    "response_digest":digest,"disposition":disposition,
                }),
                false,
            );
            if journaled.is_ok() {
                if let Some(record) = runtime
                    .executions
                    .lock()
                    .unwrap()
                    .get_mut(&exec)
                    .and_then(|e| e.replies.get_mut(&request))
                {
                    record.journaled = true;
                }
            }
            let _ = done_tx.send(journaled.map(|_| disposition));
        });
        match done_rx.await {
            Ok(Ok(disposition)) => Ok(json!({"disposition":disposition,"request_id":request_id})),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(RpcError::new(-32603, "INTERNAL_ERROR")),
        }
    }
    /// `modules.background.end`. Delivers the owner's End for a live agent
    /// attempt's background wait on the execution's control channel. The
    /// module applies it between provider turns and records a notice;
    /// repeated Ends are harmless, so nothing is reserved or journaled here.
    pub async fn end_background_wait(&self, params: &Value, idempotency_key: &str) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        let request_id = string_field(params, "request_id", 128)?;
        if idempotency_key != request_id {
            return Err(RpcError {
                code: -32602,
                name: "INVALID_IDEMPOTENCY_KEY",
                data: json!({"reason":"idempotency_key must equal request_id"}),
            });
        }
        let task_id = string_field(params, "task_id", 64)?;
        let attempt = params["attempt"]
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "INVALID_ATTEMPT"))?;
        let expected = format!("task.{task_id}.{attempt}.{}", self.config.vm_id);
        if exec_id.kind() != ExecKind::Task || exec_id.as_str() != expected {
            return Ok(json!({"disposition":"stale_execution","request_id":request_id}));
        }
        let control = {
            let executions = self.executions.lock().unwrap();
            let Some(execution) = executions.get(&exec_id) else {
                return Ok(json!({"disposition":"stale_execution","request_id":request_id}));
            };
            if !execution.accepts_end {
                return Err(RpcError {
                    code: -32602,
                    name: "END_CHANNEL_UNDECLARED",
                    data: json!({"module_id":execution.module_id}),
                });
            }
            match &execution.state {
                ExecState::Running { control, .. } => control.clone(),
                ExecState::Finished { .. } => {
                    return Ok(json!({"disposition":"execution_finished","request_id":request_id}))
                }
            }
        };
        let (ack_tx, ack_rx) = oneshot::channel();
        let queued = control
            .send(ControlMessage::EndBackgroundWait {
                request_id: request_id.clone(),
                ack: ack_tx,
            })
            .await
            .is_ok();
        let delivered = queued
            && matches!(
                tokio::time::timeout(DELIVERY_ACK_TIMEOUT * 2, ack_rx).await,
                Ok(Ok(true))
            );
        Ok(json!({
            "disposition": if delivered { "delivered" } else { "request_gone" },
            "request_id": request_id,
        }))
    }

    // -----------------------------------------------------------------------
    // Journal surface
    // -----------------------------------------------------------------------

    /// One bounded page of unacknowledged events (`g5.workspace.module-execution`):
    /// `limit` and `max_bytes` are optional and clamped; `has_more` and
    /// `next_seq` tell a consumer whether and where to continue. A caller
    /// from before paging ignores them and continues above its watermark.
    pub fn replay(&self, params: &Value) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        let from_seq = params["from_seq"].as_u64().unwrap_or(1);
        let limit = params["limit"]
            .as_u64()
            .unwrap_or(crate::journal::DEFAULT_REPLAY_LIMIT);
        let max_bytes = params["max_bytes"]
            .as_u64()
            .unwrap_or(crate::journal::DEFAULT_REPLAY_MAX_BYTES);
        let page = self
            .journal
            .replay_page(&exec_id, from_seq, limit, max_bytes)
            .map_err(journal_error)?;
        Ok(json!({
            "exec_id": exec_id.as_str(),
            "events": page.events,
            "has_more": page.has_more,
            "next_seq": page.next_seq,
        }))
    }

    pub fn ack(&self, params: &Value) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        let up_to_seq = params["up_to_seq"]
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "INVALID_SEQ"))?;
        let acked = self
            .journal
            .ack(&exec_id, up_to_seq)
            .map_err(journal_error)?;
        Ok(json!({"exec_id":exec_id.as_str(),"acked_through":acked}))
    }

    /// Terminal outcomes the control plane has not acknowledged: those still
    /// in the journal, plus any that only exist in memory because the journal
    /// refused them (reported with `source: "memory"`).
    pub fn outcomes_pending(&self) -> RpcResult {
        let mut pending = self.journal.pending_outcomes();
        for entry in &mut pending {
            entry["source"] = json!("journal");
        }
        let executions = self.executions.lock().unwrap();
        for execution in executions.values() {
            if let ExecState::Finished { outcome } = &execution.state {
                if !execution.terminal_persisted {
                    pending.push(json!({
                        "exec_id": execution.exec_id.as_str(),
                        "seq": Value::Null,
                        "outcome": outcome,
                        "source": "memory",
                    }));
                }
            }
        }
        Ok(json!({"pending":pending}))
    }

    /// Everything `outcomes.pending` would return, counted: journaled
    /// terminal outcomes not yet acknowledged plus finished outcomes the
    /// journal could not persist — so a reconnecting host that sees zero can
    /// safely skip the call.
    pub fn pending_outcome_count(&self) -> usize {
        self.journal.pending_outcomes().len() + self.unpersisted_outcomes()
    }

    /// Terminal outcomes the journal could not persist.
    pub fn unpersisted_outcomes(&self) -> usize {
        self.executions
            .lock()
            .unwrap()
            .values()
            .filter(|e| matches!(e.state, ExecState::Finished { .. }) && !e.terminal_persisted)
            .count()
    }

    // -----------------------------------------------------------------------
    // Diagnostics surface
    // -----------------------------------------------------------------------

    pub fn logs(&self, params: &Value) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        let cursor = params["cursor"].as_u64().unwrap_or(0) as usize;
        let executions = self.executions.lock().unwrap();
        let execution = executions
            .get(&exec_id)
            .ok_or_else(|| RpcError::new(404, "EXECUTION_NOT_FOUND"))?;
        let start = cursor.min(execution.logs.len());
        let chunk = &execution.logs[start..];
        Ok(json!({
            "exec_id": exec_id.as_str(),
            "cursor": execution.logs.len(),
            "chunk": String::from_utf8_lossy(chunk),
            "dropped_bytes": execution.log_bytes_dropped,
        }))
    }

    pub fn history(&self, params: &Value) -> RpcResult {
        let module_filter = params["filter"]["module_id"].as_str().map(str::to_owned);
        let executions = self.executions.lock().unwrap();
        let mut rows: Vec<Value> = executions
            .values()
            .filter(|e| module_filter.as_deref().is_none_or(|m| e.module_id == m))
            .map(|e| e.audit(&self.config))
            .collect();
        rows.sort_by_key(|row| row["started_at"].as_u64().unwrap_or(0));
        Ok(json!({"executions":rows}))
    }

    pub fn running_count(&self) -> usize {
        self.executions
            .lock()
            .unwrap()
            .values()
            .filter(|e| matches!(e.state, ExecState::Running { .. }))
            .count()
    }

    /// Record and signal cancellation for every execution still running in
    /// this incarnation. The per-execution cancellation path remains the sole
    /// owner of journal disposition and delayed-invoke fencing.
    pub fn cancel_running(&self, reason: &str) -> Result<usize, RpcError> {
        let exec_ids = self
            .executions
            .lock()
            .unwrap()
            .values()
            .filter(|execution| matches!(execution.state, ExecState::Running { .. }))
            .map(|execution| execution.exec_id.as_str().to_owned())
            .collect::<Vec<_>>();
        for exec_id in &exec_ids {
            self.cancel(&json!({"exec_id": exec_id, "reason": reason}))?;
        }
        Ok(exec_ids.len())
    }

    pub fn slots_in_use(&self) -> u32 {
        self.slots.lock().unwrap().bounded_in_use
    }

    pub fn uptime_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }

    pub fn git(&self) -> &crate::git::GitRuntime {
        &self.git
    }
}

/// Parse the canonical identity and fence it to this incarnation: an
/// identity minted for another VM can neither launch nor record here.
/// Which assignment schemas each formal `agent.turn` module accepts
/// (`g5.agent-approval.contract-rollout`). Claude keeps one module per
/// schema; each Codex App Server module accepts every schema retained when it
/// shipped, so `1.0.10` never receives schema 5 and `1.0.12` never schema 6.
fn formal_module_accepts(module_version: &str, schema_version: u8) -> bool {
    match module_version {
        "1.0.6" => schema_version == 2,
        "1.0.7" => schema_version == 3,
        "1.0.9" => schema_version == 4,
        "1.0.11" => schema_version == 5,
        "1.0.13" => schema_version == 6,
        "1.0.10" => (2..=4).contains(&schema_version),
        "1.0.12" => (2..=5).contains(&schema_version),
        "1.0.14" => (2..=6).contains(&schema_version),
        _ => false,
    }
}

fn parse_exec_id(params: &Value, config: &DispatchConfig) -> Result<ExecId, RpcError> {
    let exec_id = params["exec_id"]
        .as_str()
        .and_then(ExecId::parse)
        .ok_or_else(|| RpcError::new(-32602, "INVALID_EXEC_ID"))?;
    if exec_id.vm_id() != config.vm_id.to_string() {
        return Err(RpcError {
            code: -32602,
            name: "INVALID_EXEC_ID",
            data: json!({"reason":"foreign_incarnation"}),
        });
    }
    Ok(exec_id)
}

fn string_field(params: &Value, key: &str, max: usize) -> Result<String, RpcError> {
    let value = params[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
        .ok_or_else(|| RpcError {
            code: -32602,
            name: "INVALID_FIELD",
            data: json!({"field":key}),
        })?;
    Ok(value.to_owned())
}

fn parse_dispatch_secrets(value: Option<&Value>) -> Result<BTreeMap<String, String>, RpcError> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| RpcError::new(-32602, "INVALID_SECRETS"))?;
    if object.len() > MAX_DISPATCH_SECRETS {
        return Err(RpcError::new(413, "TOO_MANY_SECRETS"));
    }
    let mut secrets = BTreeMap::new();
    for (key, value) in object {
        let valid_name = !key.is_empty()
            && key.len() <= 128
            && key.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_uppercase() || (index > 0 && byte.is_ascii_digit())
            });
        if !valid_name {
            return Err(RpcError::new(-32602, "INVALID_SECRET_NAME"));
        }
        let value = value
            .as_str()
            .ok_or_else(|| RpcError::new(-32602, "INVALID_SECRET_VALUE"))?;
        if value.len() > MAX_DISPATCH_SECRET_VALUE_BYTES {
            return Err(RpcError::new(413, "SECRET_VALUE_TOO_LARGE"));
        }
        secrets.insert(key.clone(), value.to_string());
    }
    Ok(secrets)
}

/// Refuse top-level control keys before resolution or spawn (N98/N106).
fn reject_control_overrides(value: &Value) -> Result<(), RpcError> {
    if let Some(map) = value.as_object() {
        for key in map.keys() {
            if FORBIDDEN_CONTROL_KEYS.contains(&key.as_str()) {
                return Err(RpcError {
                    code: 403,
                    name: "MODULE_CONTROL_OVERRIDE",
                    data: json!({"key":key}),
                });
            }
        }
    }
    Ok(())
}

fn journal_error(error: crate::journal::JournalError) -> RpcError {
    RpcError {
        code: -32603,
        name: "JOURNAL_ERROR",
        data: json!({"detail":error.to_string()}),
    }
}

#[cfg(test)]
mod formal_module_tests {
    use super::formal_module_accepts;

    #[test]
    fn formal_modules_accept_only_their_published_assignment_schemas() {
        for (module, accepted) in [
            ("1.0.6", &[2][..]),
            ("1.0.7", &[3]),
            ("1.0.9", &[4]),
            ("1.0.11", &[5]),
            ("1.0.13", &[6]),
            ("1.0.10", &[2, 3, 4]),
            ("1.0.12", &[2, 3, 4, 5]),
            ("1.0.14", &[2, 3, 4, 5, 6]),
            ("1.0.5", &[]),
            ("1.0.8", &[]),
        ] {
            for schema in 1..=7 {
                assert_eq!(
                    formal_module_accepts(module, schema),
                    accepted.contains(&schema),
                    "{module} schema {schema}"
                );
            }
        }
    }
}
