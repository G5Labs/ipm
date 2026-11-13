//! In-process host/dispatcher peer for API integration tests.
//!
//! The peer uses the production mux registry and JSON-RPC framing. It only
//! replaces the guest-side dispatcher implementation, keeping workspace
//! incarnation lookup, owner admission, lease checks and tunnel routing real.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use g5_auth::WorkspaceIncarnationClaims;
use g5_internal_api::host_ctl::{
    method, CommandResult, HostVmState, VmExit, VmSizeSpec, VmStatusReport,
};
use g5_internal_api::host_ctl_test_support::{
    running_inventory, ScriptedHostFaults, ScriptedHostPeer,
};
use g5_internal_api::stream_mux::{Channel, Mux, MuxStream};
use g5_internal_api::workspace_flush::{FlushReason, FlushRepoReceipt, SequencerState};
use g5d_dispatch::file_tree::TreeSource;
use g5d_dispatch::files::{blob_hash, ObjectFormat};
use g5d_dispatch::protocol::{Context as DispatchContext, RpcError};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{TestApp, TestWorkspace, TEST_WORKER_IMAGE_REF};

type FileKey = (Uuid, String);

/// The most a whole-file read reply carries, like the guest's cap: a
/// larger file must be windowed or named by its hash.
const PEER_REPLY_LIMIT: usize = 3 * 1024 * 1024;

/// The checkout HEAD the scripted guest reports beside an `fs.tree` listing.
/// Deliberately not a workspace's durable head, so a test can tell a live
/// label from a durable one.
pub const LIVE_HEAD_OID: &str = "f1e2d3c4b5a6978877665544332211ffeeddccbb";

#[derive(Clone)]
struct StoredFile {
    content: String,
    hash: String,
}

struct ReadBarrier {
    path: String,
    required: usize,
    arrived: AtomicUsize,
    release: Notify,
}

#[derive(Default)]
struct RuntimePeerOptions {
    observed_streams: Option<tokio::sync::mpsc::UnboundedSender<MuxStream>>,
    preview_response: Option<&'static [u8]>,
    read_barrier: Option<Arc<ReadBarrier>>,
    rpc_barrier: Option<Arc<RpcBarrier>>,
    dropped_replies: Option<Arc<DroppedReplies>>,
    flush_failure: Option<String>,
    invoke_failure: Option<String>,
    unstarted_cancellation: bool,
    command_response_delay: Duration,
    /// Scripted `workspace.changes` outcome members (the association report
    /// the guest would compute); `None` reports an empty, clean vector.
    changes_members: Option<Value>,
    review_outcomes: Option<Value>,
    changes_failure: Option<String>,
    /// A `workspace.changes` execution starts but never reports an outcome,
    /// and every status poll fails, until it is cancelled.
    changes_unresponsive: bool,
    /// A `workspace.changes` execution starts and reports `running` until it
    /// is cancelled, so the caller reaches its deadline.
    changes_never_finish: bool,
    /// The dispatcher predates `fs.tree` and answers it with method-not-found.
    tree_rpc_unavailable: bool,
    /// The dispatcher lists the tree but could not read the checkout HEAD.
    tree_without_head: bool,
    host_resources: Option<(u32, VmSizeSpec)>,
    /// The host's reply to `vm.start`; `None` answers `Started`.
    vm_start_result: Option<CommandResult>,
    /// The host's reply to `vm.stop`; `None` answers `Stopped`.
    vm_stop_result: Option<CommandResult>,
    /// Journal records every task execution replays, the way the dispatcher
    /// journals a running module's output.
    journal_events: Vec<Value>,
}

struct RpcBarrier {
    /// Every request carrying one of these methods waits at the gate.
    methods: Vec<&'static str>,
    gate: Semaphore,
}

impl RpcBarrier {
    fn holds(&self, method: &str) -> bool {
        self.methods.contains(&method)
    }
}

/// The first `remaining` streams carrying `method` close without a reply,
/// the way a call on a dispatcher connection the host lost or replaced does.
struct DroppedReplies {
    method: &'static str,
    remaining: AtomicUsize,
    /// The idempotency key of every `method` request, dropped or answered.
    keys: Mutex<Vec<String>>,
}

#[derive(Clone)]
struct RuntimePeerContext {
    vm_id: Uuid,
    files: Arc<Mutex<HashMap<FileKey, StoredFile>>>,
    /// Files whose write or delete the peer refuses with a version conflict,
    /// the way the guest does when the file moved under the caller.
    refused_writes: Arc<Mutex<HashSet<FileKey>>>,
    /// Files whose read the peer answers with a runtime error, the way a
    /// guest that lost its disk or its connection would.
    refused_reads: Arc<Mutex<HashSet<FileKey>>>,
    rpc_methods: Arc<Mutex<Vec<String>>>,
    /// Every `modules.invoke` request's params, in order.
    invoke_params: Arc<Mutex<Vec<Value>>>,
    read_barrier: Option<Arc<ReadBarrier>>,
    rpc_barrier: Option<Arc<RpcBarrier>>,
    dropped_replies: Option<Arc<DroppedReplies>>,
    flush_failure: Option<String>,
    pool: PgPool,
    repo_storage: g5_api_core::services::git::repo_store::TenantRepoStorage,
    git_bin: std::path::PathBuf,
    claims: WorkspaceIncarnationClaims,
    cancelled_execs: Arc<Mutex<HashSet<String>>>,
    invoke_failure: Option<String>,
    unstarted_cancellation: bool,
    changes_members: Option<Value>,
    review_outcomes: Option<Value>,
    changes_failure: Arc<Mutex<Option<String>>>,
    changes_unresponsive: bool,
    changes_never_finish: bool,
    /// The dispatcher predates `fs.tree` and answers it with method-not-found.
    tree_rpc_unavailable: bool,
    /// The dispatcher lists the tree but could not read the checkout HEAD.
    tree_without_head: bool,
    journal_events: JournalFeed,
    acked_seqs: Arc<Mutex<Vec<u64>>>,
    ack_failure: Arc<AtomicBool>,
}

/// The journal a peer's task executions replay, appended to by tests while
/// a follower streams it (`g5.chat.live-delivery`).
#[derive(Clone, Default)]
pub struct JournalFeed {
    events: Arc<Mutex<Vec<Value>>>,
}

impl JournalFeed {
    pub async fn append(&self, event: Value) {
        self.events.lock().await.push(event);
    }

    pub async fn extend(&self, events: impl IntoIterator<Item = Value>) {
        self.events.lock().await.extend(events);
    }

    pub async fn len(&self) -> usize {
        self.events.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.events.lock().await.is_empty()
    }

    async fn page(&self, from_seq: u64, limit: usize, max_bytes: usize) -> (Vec<Value>, bool) {
        let events = self.events.lock().await;
        let mut page = Vec::new();
        let mut bytes = 0usize;
        let mut has_more = false;
        for event in events
            .iter()
            .filter(|event| event["seq"].as_u64().is_some_and(|seq| seq >= from_seq))
        {
            let size = event.to_string().len();
            if !page.is_empty() && (page.len() >= limit || bytes + size > max_bytes) {
                has_more = true;
                break;
            }
            bytes += size;
            page.push(event.clone());
        }
        (page, has_more)
    }
}

/// A running workspace incarnation backed by a small scripted dispatcher.
pub struct TestWorkspaceRuntimePeer {
    host_id: Uuid,
    vm_id: Uuid,
    registry: Arc<g5_api::ws::preview_tunnel::TunnelRegistry>,
    control_peer: ScriptedHostPeer,
    files: Arc<Mutex<HashMap<FileKey, StoredFile>>>,
    refused_writes: Arc<Mutex<HashSet<FileKey>>>,
    refused_reads: Arc<Mutex<HashSet<FileKey>>>,
    rpc_methods: Arc<Mutex<Vec<String>>>,
    journal: JournalFeed,
    acked_seqs: Arc<Mutex<Vec<u64>>>,
    ack_failure: Arc<AtomicBool>,
    invoke_params: Arc<Mutex<Vec<Value>>>,
    cancelled_execs: Arc<Mutex<HashSet<String>>>,
    rpc_barrier: Option<Arc<RpcBarrier>>,
    dropped_replies: Option<Arc<DroppedReplies>>,
    tasks: Vec<JoinHandle<()>>,
    cleanup_runtime: tokio::runtime::Handle,
    preview_requests: Arc<Mutex<Vec<String>>>,
    preview_stream_count: Arc<AtomicUsize>,
    /// Accepted owner terminal streams, held open like a guest PTY would be
    /// so terminal-transport tests can keep a connection alive.
    pty_streams: Arc<Mutex<Vec<MuxStream>>>,
    changes_failure: Arc<Mutex<Option<String>>>,
}

impl TestWorkspaceRuntimePeer {
    /// Let a test control live PTY/HTTP bytes while retaining real API relays.
    pub async fn attach_with_streams(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
    ) -> (Self, tokio::sync::mpsc::UnboundedReceiver<MuxStream>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let peer = Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                observed_streams: Some(tx),
                ..RuntimePeerOptions::default()
            },
        )
        .await;
        (peer, rx)
    }

    pub async fn attach_with_preview_response(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        response: &'static [u8],
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                preview_response: Some(response),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn preview_requests(&self) -> Vec<String> {
        self.preview_requests.lock().await.clone()
    }

    pub fn preview_stream_count(&self) -> usize {
        self.preview_stream_count.load(Ordering::SeqCst)
    }

    /// Owner terminal streams the dispatcher has accepted and holds open.
    pub async fn pty_stream_count(&self) -> usize {
        self.pty_streams.lock().await.len()
    }

    pub async fn take_pty_stream(&self) -> MuxStream {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(stream) = self.pty_streams.lock().await.pop() {
                    return stream;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("terminal stream opened")
    }

    pub async fn attach(app: &TestApp, workspace: &TestWorkspace, owner_user_id: Uuid) -> Self {
        Self::attach_inner(app, workspace, owner_user_id, RuntimePeerOptions::default()).await
    }

    pub async fn attach_with_host_resources(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        slots: u32,
        size: VmSizeSpec,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                host_resources: Some((slots, size)),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_flush_failure(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        error: impl Into<String>,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                flush_failure: Some(error.into()),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// A host that answers every `vm.stop` with `vm_stop_result`, for
    /// example `Busy`.
    pub async fn attach_with_vm_stop_result(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        vm_stop_result: CommandResult,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                vm_stop_result: Some(vm_stop_result),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// A guest whose flush fails with `error_code` behind a host that answers
    /// every `vm.stop` with `vm_stop_result`, for example `Busy`.
    pub async fn attach_with_flush_failure_and_vm_stop_result(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        error_code: &str,
        vm_stop_result: CommandResult,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                flush_failure: Some(error_code.to_string()),
                vm_stop_result: Some(vm_stop_result),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_invoke_failure(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        error: impl Into<String>,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                invoke_failure: Some(error.into()),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// Attach with a scripted live association report: every
    /// `workspace.changes` invocation reports `members` as its outcome, so a
    /// test can stand in a foreign checkout, a detached head or a missing
    /// internal ref without a real guest.
    pub async fn attach_with_changes_report(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        members: Value,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                changes_members: Some(members),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// Script inspect outcomes by review mode to exercise mixed guest versions.
    pub async fn attach_with_review_outcomes(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        outcomes: Value,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                review_outcomes: Some(outcomes),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// A running VM whose dispatcher predates `fs.tree`, with the given
    /// Changes report.
    pub async fn attach_without_tree_rpc(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        members: Value,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                changes_members: Some(members),
                tree_rpc_unavailable: true,
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// A running VM whose dispatcher lists the tree but reads no checkout
    /// HEAD (a layout `head_oid` refuses), with the given Changes report.
    pub async fn attach_without_tree_head(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        members: Value,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                changes_members: Some(members),
                tree_without_head: true,
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_changes_failure(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        operation: &str,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                changes_failure: Some(operation.to_string()),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_unresponsive_changes(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                changes_unresponsive: true,
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_endless_changes(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                changes_never_finish: true,
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// A peer whose task executions replay `journal_events`.
    pub async fn attach_with_journal_events(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        journal_events: Vec<Value>,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                journal_events,
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_unstarted_cancellation(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                unstarted_cancellation: true,
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_host_command_delay(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        command_response_delay: Duration,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                command_response_delay,
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_read_barrier(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        path: impl Into<String>,
        required: usize,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                read_barrier: Some(Arc::new(ReadBarrier {
                    path: path.into(),
                    required,
                    arrived: AtomicUsize::new(0),
                    release: Notify::new(),
                })),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// The scripted host answers every `vm.start` with `result`, for example
    /// a capacity refusal.
    pub async fn attach_with_vm_start_result(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        result: CommandResult,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                vm_start_result: Some(result),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    pub async fn attach_with_rpc_barrier(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        method: &'static str,
    ) -> Self {
        Self::attach_with_rpc_barriers(app, workspace, owner_user_id, &[method]).await
    }

    /// A peer that holds every request carrying one of `methods` until the
    /// test releases the barrier.
    pub async fn attach_with_rpc_barriers(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        methods: &[&'static str],
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                rpc_barrier: Some(Arc::new(RpcBarrier {
                    methods: methods.to_vec(),
                    gate: Semaphore::new(0),
                })),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    /// A peer whose first `count` streams carrying `method` close without a
    /// reply; later ones are answered normally.
    pub async fn attach_dropping_replies(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        method: &'static str,
        count: usize,
    ) -> Self {
        Self::attach_inner(
            app,
            workspace,
            owner_user_id,
            RuntimePeerOptions {
                dropped_replies: Some(Arc::new(DroppedReplies {
                    method,
                    remaining: AtomicUsize::new(count),
                    keys: Mutex::new(Vec::new()),
                })),
                ..RuntimePeerOptions::default()
            },
        )
        .await
    }

    async fn attach_inner(
        app: &TestApp,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
        options: RuntimePeerOptions,
    ) -> Self {
        let RuntimePeerOptions {
            observed_streams,
            preview_response,
            read_barrier,
            rpc_barrier,
            dropped_replies,
            flush_failure,
            invoke_failure,
            unstarted_cancellation,
            command_response_delay,
            changes_members,
            review_outcomes,
            changes_failure,
            changes_unresponsive,
            changes_never_finish,
            tree_rpc_unavailable,
            tree_without_head,
            host_resources,
            vm_start_result,
            vm_stop_result,
            journal_events,
        } = options;
        let (slots_total, slot_size) = host_resources.unwrap_or((
            1,
            VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 10240,
            },
        ));
        assert!(
            app.state.install_host_control_backend(),
            "test app should install the production host-control backend"
        );
        sqlx::query("UPDATE orgs SET default_base_image_ref = $2 WHERE id = $1")
            .bind(workspace.org_id)
            .bind(TEST_WORKER_IMAGE_REF)
            .execute(&app.db)
            .await
            .expect("set test workspace image");
        let host_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO worker_hosts \
             (host_id, org_id, region, status, last_heartbeat, slot_size_reported, slots_total, slot_cpus, slot_mem_mib, scratch_disk_mib, runtime_identity) \
             VALUES ($1, $2, 'us-central1', 'ready', now(), true, $3, $4, $5, $6, $7)",
        )
        .bind(host_id)
        .bind(workspace.org_id)
        .bind(slots_total as i32).bind(i32::from(slot_size.cpus))
        .bind(slot_size.mem_mib as i32).bind(slot_size.scratch_disk_mib as i32)
        .bind(super::current_runtime_identity())
        .execute(&app.db)
        .await
        .expect("insert test runtime host");
        sqlx::query(
            "INSERT INTO workspace_vms \
             (org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at, \
              module_catalog_version) \
             VALUES ($1, $2, $3, $4, 1, 'running', now() + interval '1 hour', $5)",
        )
        .bind(workspace.org_id)
        .bind(workspace.id)
        .bind(host_id)
        .bind(vm_id.to_string())
        // A running incarnation always records the catalog its host verified
        // at Ready; contract negotiation depends on it.
        .bind(g5_internal_api::host_ctl::MODULE_CATALOG_VERSION)
        .execute(&app.db)
        .await
        .expect("insert running test workspace incarnation");
        let actual_owner: Uuid =
            sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id = $1")
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .expect("load test workspace owner");
        assert_eq!(
            actual_owner, owner_user_id,
            "runtime peer must be attached for the authoritative workspace owner"
        );

        let (api_io, host_io) = tokio::io::duplex(256 * 1024);
        let api_mux = Mux::new(api_io, true);
        let mut host_mux = Mux::new(host_io, false);
        let control_peer =
            ScriptedHostPeer::new(host_id, slots_total).with_slot_size(slot_size.clone());
        control_peer
            .set_inventory(vec![running_inventory(workspace.id, vm_id)])
            .await;
        let control_stream = host_mux
            .open(Channel::Ctl)
            .await
            .expect("open scripted host control stream");
        let workspace_id = workspace.id;
        let control_task = tokio::spawn({
            let control_peer = control_peer.clone();
            async move {
                let mut command_results = HashMap::new();
                command_results.insert(
                    method::VM_STOP.to_string(),
                    vm_stop_result.unwrap_or(CommandResult::Stopped {
                        exit: VmExit {
                            code: Some(0),
                            signal: None,
                        },
                    }),
                );
                command_results.insert(
                    method::VM_START.to_string(),
                    vm_start_result.unwrap_or(CommandResult::Started),
                );
                command_results.insert(
                    method::VM_STATUS.to_string(),
                    CommandResult::Status {
                        vm: VmStatusReport {
                            workspace_id,
                            vm_id,
                            epoch: 1,
                            state: HostVmState::Running,
                            boot_at: chrono::Utc::now(),
                            ready_at: Some(chrono::Utc::now()),
                            resources: slot_size.clone(),
                            streams_open: 0,
                        },
                    },
                );
                if let Err(error) = control_peer
                    .run_control_stream(
                        control_stream,
                        ScriptedHostFaults {
                            command_response_delay,
                            command_results,
                            ..ScriptedHostFaults::default()
                        },
                    )
                    .await
                {
                    eprintln!("scripted workspace host control stream failed: {error}");
                }
            }
        });
        let registry = app.state.preview_tunnels.clone();
        let registry_task = tokio::spawn({
            let registry = registry.clone();
            async move { registry.attach_test_mux(host_id, api_mux).await }
        });
        let files = Arc::new(Mutex::new(HashMap::new()));
        let refused_writes: Arc<Mutex<HashSet<FileKey>>> = Arc::new(Mutex::new(HashSet::new()));
        let refused_reads: Arc<Mutex<HashSet<FileKey>>> = Arc::new(Mutex::new(HashSet::new()));
        let rpc_methods = Arc::new(Mutex::new(Vec::new()));
        let journal = JournalFeed {
            events: Arc::new(Mutex::new(journal_events)),
        };
        let acked_seqs: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let ack_failure = Arc::new(AtomicBool::new(false));
        let invoke_params: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let peer_rpc_barrier = rpc_barrier.clone();
        let peer_dropped_replies = dropped_replies.clone();
        let cancelled_execs = Arc::new(Mutex::new(HashSet::new()));
        let peer_cancelled_execs = cancelled_execs.clone();
        let preview_requests = Arc::new(Mutex::new(Vec::new()));
        let preview_stream_count = Arc::new(AtomicUsize::new(0));
        let pty_streams = Arc::new(Mutex::new(Vec::new()));
        let pty_streams_writer = pty_streams.clone();
        let changes_failure = Arc::new(Mutex::new(changes_failure));
        let dispatcher_task = tokio::spawn({
            let preview_requests = preview_requests.clone();
            let preview_stream_count = preview_stream_count.clone();
            let files = files.clone();
            let refused_writes = refused_writes.clone();
            let refused_reads = refused_reads.clone();
            let rpc_methods = rpc_methods.clone();
            let invoke_params = invoke_params.clone();
            let rpc_barrier = rpc_barrier.clone();
            let dropped_replies = dropped_replies.clone();
            let changes_failure = changes_failure.clone();
            let pool = app.system_pool.clone();
            let claims = WorkspaceIncarnationClaims {
                org_id: workspace.org_id,
                workspace_id: workspace.id,
                owner_user_id,
                vm_id,
                epoch: 1,
                iat: chrono::Utc::now().timestamp(),
                exp: chrono::Utc::now().timestamp() + 3600,
                iss: Some("g5-api-test".to_string()),
                aud: g5_auth::WORKSPACE_INCARNATION_AUDIENCE.to_string(),
                jti: Uuid::new_v4().to_string(),
                token_use: g5_auth::WORKSPACE_INCARNATION_TOKEN_USE.to_string(),
            };
            let context = RuntimePeerContext {
                vm_id,
                files,
                refused_writes,
                refused_reads,
                rpc_methods,
                invoke_params,
                read_barrier,
                rpc_barrier,
                dropped_replies,
                flush_failure,
                pool,
                repo_storage: app.repo_storage.clone(),
                git_bin: app.state.git_bin.clone(),
                claims,
                cancelled_execs,
                invoke_failure,
                unstarted_cancellation,
                changes_members,
                review_outcomes,
                changes_failure,
                changes_unresponsive,
                changes_never_finish,
                tree_rpc_unavailable,
                tree_without_head,
                journal_events: journal.clone(),
                acked_seqs: acked_seqs.clone(),
                ack_failure: ack_failure.clone(),
            };
            let pty_streams = pty_streams_writer;
            async move {
                while let Some(stream) = host_mux.accept().await {
                    if matches!(stream.channel(), Channel::Http | Channel::Pty) {
                        if let Some(tx) = &observed_streams {
                            let _ = tx.send(stream);
                            continue;
                        }
                    }
                    if stream.channel() == Channel::Http {
                        preview_stream_count.fetch_add(1, Ordering::SeqCst);
                        if let Some(response) = preview_response {
                            let requests = preview_requests.clone();
                            tokio::spawn(async move {
                                serve_preview_stream(stream, vm_id, response, requests).await;
                            });
                        }
                        continue;
                    }
                    if stream.channel() == Channel::Pty {
                        // Dropping the stream would close the browser side;
                        // a real guest keeps the PTY attached.
                        pty_streams.lock().await.push(stream);
                        continue;
                    }
                    if stream.channel() != Channel::Rpc {
                        continue;
                    }
                    let context = context.clone();
                    tokio::spawn(async move {
                        serve_runtime_stream(stream, context).await;
                    });
                }
            }
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while registry
                .get(&host_id)
                .and_then(|tunnel| tunnel.command_client())
                .is_none()
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("test host tunnel attaches");

        Self {
            host_id,
            vm_id,
            registry,
            control_peer,
            files,
            refused_writes,
            refused_reads,
            rpc_methods,
            journal,
            acked_seqs,
            ack_failure,
            invoke_params,
            cancelled_execs: peer_cancelled_execs,
            rpc_barrier: peer_rpc_barrier,
            dropped_replies: peer_dropped_replies,
            tasks: vec![registry_task, control_task, dispatcher_task],
            cleanup_runtime: tokio::runtime::Handle::current(),
            preview_requests,
            preview_stream_count,
            pty_streams,
            changes_failure,
        }
    }

    pub fn host_id(&self) -> Uuid {
        self.host_id
    }

    pub fn vm_id(&self) -> Uuid {
        self.vm_id
    }

    pub async fn file_content(&self, repo_id: Uuid, path: &str) -> Option<String> {
        self.files
            .lock()
            .await
            .get(&(repo_id, path.to_string()))
            .map(|file| file.content.clone())
    }

    pub async fn allow_changes(&self) {
        *self.changes_failure.lock().await = None;
    }

    pub async fn received_host_command_methods(&self) -> Vec<String> {
        self.control_peer
            .received_commands()
            .await
            .into_iter()
            .map(|request| request.method)
            .collect()
    }

    /// Parameters of every host command with `method`, in arrival order.
    pub async fn received_host_command_params(&self, method: &str) -> Vec<Value> {
        self.control_peer
            .received_commands()
            .await
            .into_iter()
            .filter(|request| request.method == method)
            .map(|request| request.params)
            .collect()
    }

    pub async fn received_rpc_methods(&self) -> Vec<String> {
        self.rpc_methods.lock().await.clone()
    }

    /// The journal this peer's task executions replay; append while a
    /// follower streams it.
    pub fn journal(&self) -> JournalFeed {
        self.journal.clone()
    }

    /// The executions whose `modules.cancel` this peer answered, in no
    /// particular order.
    pub async fn cancelled_execs(&self) -> Vec<String> {
        let mut execs: Vec<String> = self.cancelled_execs.lock().await.iter().cloned().collect();
        execs.sort();
        execs
    }

    /// Every `events.ack` the API sent, as `up_to_seq`, in arrival order.
    pub async fn acked_seqs(&self) -> Vec<u64> {
        self.acked_seqs.lock().await.clone()
    }

    /// Make `events.ack` fail (or succeed again) from now on.
    pub fn set_ack_failure(&self, fail: bool) {
        self.ack_failure.store(fail, Ordering::Relaxed);
    }

    /// The params of every `modules.invoke` the API drove, in order.
    pub async fn invoke_params(&self) -> Vec<Value> {
        self.invoke_params.lock().await.clone()
    }

    pub fn release_rpc_barrier(&self) {
        self.release_rpc_barrier_permits(2);
    }

    /// Let `permits` held or future requests through the barrier.
    pub fn release_rpc_barrier_permits(&self, permits: usize) {
        self.rpc_barrier
            .as_ref()
            .expect("runtime peer has an RPC barrier")
            .gate
            .add_permits(permits);
    }

    pub fn release_one_rpc(&self) {
        self.rpc_barrier
            .as_ref()
            .expect("runtime RPC barrier")
            .gate
            .add_permits(1);
    }

    /// The idempotency key of every request carrying the dropped method, in
    /// arrival order.
    pub async fn dropped_method_keys(&self) -> Vec<String> {
        self.dropped_replies
            .as_ref()
            .expect("runtime peer drops replies")
            .keys
            .lock()
            .await
            .clone()
    }

    /// Seed the guest filesystem and return the version hash that callers must
    /// present for an update. This models a materialized workspace checkout;
    /// committed Git remains independent of ordinary draft writes.
    /// Every write or delete of `path` is answered with a version conflict
    /// until `allow_writes`, the way the guest refuses a file that moved.
    pub async fn refuse_writes(&self, repo_id: Uuid, path: &str) {
        self.refused_writes
            .lock()
            .await
            .insert((repo_id, path.to_string()));
    }

    pub async fn allow_writes(&self, repo_id: Uuid, path: &str) {
        self.refused_writes
            .lock()
            .await
            .remove(&(repo_id, path.to_string()));
    }

    /// Every read of `path` is answered with a runtime error until
    /// `allow_reads`, the way a guest that lost its disk would answer.
    pub async fn refuse_reads(&self, repo_id: Uuid, path: &str) {
        self.refused_reads
            .lock()
            .await
            .insert((repo_id, path.to_string()));
    }

    pub async fn allow_reads(&self, repo_id: Uuid, path: &str) {
        self.refused_reads
            .lock()
            .await
            .remove(&(repo_id, path.to_string()));
    }

    pub async fn seed_file(&self, repo_id: Uuid, path: &str, content: &str) -> String {
        // The guest names a file by its git blob id, in the member
        // repository's object format (sha1 for the test mirrors).
        let hash = blob_hash(ObjectFormat::Sha1, content.as_bytes());
        self.files.lock().await.insert(
            (repo_id, path.to_string()),
            StoredFile {
                content: content.to_string(),
                hash: hash.clone(),
            },
        );
        hash
    }
}

impl Drop for TestWorkspaceRuntimePeer {
    fn drop(&mut self) {
        let registry = self.registry.clone();
        let host_id = self.host_id;
        // Drop cannot await the publication guard; detach cleanup on the peer's runtime.
        drop(self.cleanup_runtime.spawn(async move {
            registry.terminate(&host_id).await;
        }));
        for task in &self.tasks {
            task.abort();
        }
    }
}

// Scripted guest HTTP listener; production owner/incarnation admission, mux
// preambles, proxy request framing and downstream sanitization remain real.
async fn serve_preview_stream(
    mut stream: MuxStream,
    vm_id: Uuid,
    response: &'static [u8],
    requests: Arc<Mutex<Vec<String>>>,
) {
    let exchange = async {
        assert_eq!(stream.recv().await.unwrap(), vm_id.as_bytes());
        assert_eq!(stream.recv().await.unwrap(), 4173_u16.to_be_bytes());
        let mut request = Vec::new();
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            request.extend(stream.recv().await.expect("preview request head"));
            assert!(request.len() <= 64 * 1024);
        }
        requests
            .lock()
            .await
            .push(String::from_utf8(request).unwrap());
        stream.send(response).await.unwrap();
        stream.close().await;
    };
    tokio::time::timeout(Duration::from_secs(5), exchange)
        .await
        .expect("preview exchange completes");
}

/// The peer's in-memory files as a tree walk source for `fs.tree`: one
/// directory level per `list`, as the guest's file service provides it.
struct PeerTreeSource<'a> {
    files: &'a HashMap<FileKey, StoredFile>,
    repo_id: Uuid,
}

impl g5d_dispatch::file_tree::TreeSource for PeerTreeSource<'_> {
    type Error = std::convert::Infallible;

    fn list(&self, directory: &str) -> Result<g5d_dispatch::file_tree::Listing, Self::Error> {
        let prefix = if directory.trim_end_matches('/').is_empty() {
            String::new()
        } else {
            format!("{}/", directory.trim_end_matches('/'))
        };
        let mut children = std::collections::BTreeMap::new();
        for ((candidate_repo_id, candidate_path), file) in self.files.iter() {
            if *candidate_repo_id != self.repo_id {
                continue;
            }
            let Some(relative) = candidate_path.strip_prefix(&prefix) else {
                continue;
            };
            if relative.is_empty() {
                continue;
            }
            match relative.split_once('/') {
                None => {
                    children.insert(relative.to_string(), (false, file.content.len() as u64));
                }
                Some((child, _)) => {
                    children.entry(child.to_string()).or_insert((true, 0));
                }
            }
        }
        if children.is_empty() && !prefix.is_empty() {
            return Ok(g5d_dispatch::file_tree::Listing::Missing);
        }
        Ok(g5d_dispatch::file_tree::Listing::Entries(
            children
                .into_iter()
                .map(
                    |(name, (is_dir, size))| g5d_dispatch::file_tree::SourceEntry {
                        name,
                        is_dir,
                        size,
                    },
                )
                .collect(),
        ))
    }

    fn read_gitignore(&self, directory: &str) -> Option<String> {
        let path = if directory.is_empty() {
            g5d_dispatch::file_tree::GITIGNORE.to_string()
        } else {
            format!("{directory}/{}", g5d_dispatch::file_tree::GITIGNORE)
        };
        self.files
            .get(&(self.repo_id, path))
            .map(|file| file.content.clone())
    }
}

async fn serve_runtime_stream(mut stream: MuxStream, context: RuntimePeerContext) {
    let RuntimePeerContext {
        vm_id,
        files,
        refused_writes,
        refused_reads,
        rpc_methods,
        invoke_params,
        read_barrier,
        rpc_barrier,
        dropped_replies,
        flush_failure,
        pool,
        repo_storage,
        git_bin,
        claims,
        cancelled_execs,
        invoke_failure,
        unstarted_cancellation,
        changes_members,
        review_outcomes,
        changes_failure,
        changes_unresponsive,
        changes_never_finish,
        tree_rpc_unavailable,
        tree_without_head,
        journal_events,
        acked_seqs,
        ack_failure,
    } = context;
    let Some(header) = stream.recv().await else {
        return;
    };
    if header != vm_id.as_bytes() {
        return;
    }
    let (writer, mut reader) = stream.into_split();
    // Mirror the production dispatcher: file writes exceed the stream's
    // small default line limit, including uploaded HTML questionnaires.
    let Some(line) = reader
        .read_line_with_limit(g5d_dispatch::MAX_RPC_BYTES)
        .await
    else {
        return;
    };
    let Ok(request) = serde_json::from_str::<Value>(&line) else {
        return;
    };
    let id = request["id"].clone();
    let method = request["method"].as_str().unwrap_or_default();
    rpc_methods.lock().await.push(method.to_string());
    if method == "modules.invoke" {
        // `modules.invoke:<module>/<operation>`, so a test can assert which
        // built-in operations the API drove and in what order.
        let module = request["params"]["module_id"].as_str().unwrap_or_default();
        let operation = request["params"]["operation"].as_str().unwrap_or_default();
        rpc_methods
            .lock()
            .await
            .push(format!("modules.invoke:{module}/{operation}"));
        invoke_params.lock().await.push(request["params"].clone());
    }
    if let Some(barrier) = rpc_barrier.as_ref().filter(|barrier| barrier.holds(method)) {
        barrier
            .gate
            .acquire()
            .await
            .expect("runtime RPC barrier remains open")
            .forget();
    }
    let params = &request["params"];
    if let Some(dropped) = dropped_replies
        .as_ref()
        .filter(|dropped| dropped.method == method)
    {
        dropped.keys.lock().await.push(
            params["context"]["idempotency_key"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        if dropped
            .remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            writer.close().await;
            return;
        }
    }
    let fail_changes = changes_failure.lock().await.as_deref() == params["operation"].as_str();
    // Apply the production dispatcher's method-level authority rule to the
    // envelope the API actually sent, so a request the real guest would
    // refuse is refused here too instead of being scripted through.
    let refusal = serde_json::from_value::<DispatchContext>(params["context"].clone())
        .map_err(|_| RpcError::new(-32602, "INVALID_CONTEXT"))
        .and_then(|context| context.authorize(method, claims.owner_user_id))
        .err();
    let reply = if let Some(error) = refusal {
        json!({"jsonrpc":"2.0","id":id,"error":error.value()})
    } else {
        match method {
            "admission.close" | "admission.open" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"closed": method == "admission.close"}
            }),
            "workspace.flush" => {
                let Some(request_key) = params["context"]["idempotency_key"].as_str() else {
                    return;
                };
                let result = match flush_failure {
                    Some(error) => Err(error),
                    None => {
                        complete_test_flush(
                            &pool,
                            repo_storage.clone(),
                            git_bin.clone(),
                            &claims,
                            request_key,
                        )
                        .await
                    }
                };
                match result {
                    Ok(receipt) => json!({"jsonrpc": "2.0", "id": id, "result": receipt}),
                    Err(error) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 503, "message": error, "data": {}}
                    }),
                }
            }
            "modules.invoke" => match invoke_failure {
                Some(error) => json!({
                    "jsonrpc":"2.0","id":id,
                    "error":{"code":503,"message":error,"data":{}}
                }),
                None if (changes_unresponsive || changes_never_finish)
                    && params["module_id"].as_str() == Some("workspace.changes") =>
                {
                    json!({"jsonrpc":"2.0","id":id,"result":{"disposition":"started"}})
                }
                None if review_outcomes.is_some()
                    && params["module_id"].as_str() == Some("workspace.changes")
                    && params["operation"].as_str() == Some("inspect") =>
                {
                    let mode = params["input"]["review_mode"].as_str().unwrap_or_default();
                    json!({"jsonrpc":"2.0","id":id,"result":{
                        "disposition":"started","outcome":review_outcomes.as_ref().and_then(|outcomes| outcomes.get(mode))
                    }})
                }
                None if changes_members.is_none()
                    && !fail_changes
                    && params["module_id"].as_str() == Some("workspace.changes")
                    && params["operation"].as_str() == Some("rebase") =>
                {
                    let member =
                        scripted_rebase(&pool, &repo_storage, &git_bin, &claims, &params["input"])
                            .await;
                    let status = if member["rebased"] == true {
                        "succeeded"
                    } else {
                        "failed"
                    };
                    json!({"jsonrpc":"2.0","id":id,"result":{"disposition":"started","outcome":{
                        "status":status,"result":{"members":[member]},"detail":{}
                    }}})
                }
                None if changes_members.is_none()
                    && !fail_changes
                    && params["module_id"].as_str() == Some("workspace.changes")
                    && params["operation"].as_str() == Some("adopt") =>
                {
                    let member =
                        scripted_adopt(&pool, &repo_storage, &git_bin, &claims, &params["input"])
                            .await;
                    let status = if member["adopted"] == true {
                        "succeeded"
                    } else {
                        "failed"
                    };
                    json!({"jsonrpc":"2.0","id":id,"result":{"disposition":"started","outcome":{
                        "status":status,"result":{"members":[member]},"detail":{}
                    }}})
                }
                None if fail_changes => {
                    json!({
                        "jsonrpc":"2.0","id":id,"result":{
                            "disposition":"started","outcome":{
                                "status":"failed","result":{"members":[{
                                    "repo_id":Uuid::nil(),"errors":["SCRIPTED_SAVE_FAILURE"]
                                }]},"detail":{}
                            }
                        }
                    })
                }
                None => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "disposition": "started",
                        "outcome": {
                            "status": "succeeded",
                            "result": {
                                "members": changes_members.clone().unwrap_or_else(|| {
                                    let operation = params["operation"].as_str().unwrap_or_default();
                                    let repo_id = &params["input"]["repo_id"];
                                    if !repo_id.is_null() && matches!(operation, "commit" | "push") {
                                        json!([{"repo_id":repo_id,"head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","committed":operation == "commit","pushed":operation == "push","files":1}])
                                    } else {
                                        json!([])
                                    }
                                })
                            },
                            "detail": {}
                        }
                    }
                }),
            },
            "modules.cancel" => {
                // Answer as the dispatcher does: a run whose journal already
                // holds its outcome is `already_finished`, otherwise the
                // guest is `signalling` it (`g5.chat.controls`).
                let exec_id = params["exec_id"].as_str().unwrap_or_default().to_string();
                cancelled_execs.lock().await.insert(exec_id.clone());
                let finished = exec_id.starts_with("task.")
                    && journal_events
                        .page(1, 4096, 3 * 1024 * 1024)
                        .await
                        .0
                        .iter()
                        .any(|event| event["terminal"].as_bool() == Some(true));
                let disposition = if finished {
                    "already_finished"
                } else {
                    "signalling"
                };
                json!({"jsonrpc":"2.0","id":id,"result":{"cancelled":true,"disposition":disposition}})
            }
            "modules.input.respond" => json!({
                "jsonrpc":"2.0","id":id,"result":{"disposition":"delivered"}
            }),
            "modules.input.steer" => json!({
                "jsonrpc":"2.0","id":id,"result":{
                    // Like modules.status below, only the explicit steering
                    // fixture models a live provider accepting stdin. Generic
                    // queue/Stop tests have no provider to flush input to.
                    "disposition":if rpc_barrier.as_ref().is_some_and(|barrier| barrier.holds("modules.input.steer")) {
                        "shipped"
                    } else {
                        "not_steerable"
                    },
                    "request_id":params["request_id"]
                }
            }),
            "modules.background.end" => json!({
                "jsonrpc":"2.0","id":id,
                "result":{"disposition":"delivered","request_id":params["request_id"]}
            }),
            "modules.status" => {
                let cancelled = if let Some(exec_id) = params["exec_id"].as_str() {
                    cancelled_execs.lock().await.contains(exec_id)
                } else {
                    false
                };
                if changes_unresponsive
                    && !cancelled
                    && params["exec_id"]
                        .as_str()
                        .is_some_and(|exec| exec.starts_with("operation."))
                {
                    json!({
                        "jsonrpc":"2.0","id":id,
                        "error":{"code":503,"message":"STATUS_UNAVAILABLE","data":{}}
                    })
                } else if changes_never_finish
                    && !cancelled
                    && params["exec_id"]
                        .as_str()
                        .is_some_and(|exec| exec.starts_with("operation."))
                {
                    json!({"jsonrpc":"2.0","id":id,"result":{"status":"running"}})
                } else {
                    json!({
                        "jsonrpc":"2.0","id":id,
                        "result": if cancelled && unstarted_cancellation {
                            json!({"status":"cancelled","reason":"cancelled_before_start"})
                        } else if cancelled {
                            json!({"status":"finished","persistence":"persisted"})
                        } else if rpc_barrier.as_ref().is_some_and(|barrier| {
                            barrier.holds("modules.input.steer") || barrier.holds("modules.status")
                        }) && params["exec_id"].as_str().is_some_and(|exec| exec.starts_with("task.")) {
                            // A held steering acknowledgement or status
                            // answer models an existing live execution, not
                            // a missing process.
                            json!({"status":"running"})
                        } else {
                            json!({"status":"unknown"})
                        }
                    })
                }
            }
            "events.replay" => {
                let exec_id = params["exec_id"].as_str().unwrap_or_default();
                let cancelled = cancelled_execs.lock().await.contains(exec_id);
                let from_seq = params["from_seq"].as_u64().unwrap_or(1);
                // A peer given a journal replays it even after a cancel, as
                // a dispatcher does for a run that finished before the cancel.
                let limit = params["limit"].as_u64().unwrap_or(1024).clamp(1, 4096) as usize;
                let max_bytes = params["max_bytes"]
                    .as_u64()
                    .unwrap_or(2 * 1024 * 1024)
                    .clamp(64 * 1024, 3 * 1024 * 1024) as usize;
                let mut has_more = false;
                let events = if cancelled
                    && !unstarted_cancellation
                    && from_seq <= 1
                    && journal_events.is_empty().await
                {
                    vec![json!({
                        "seq":1,
                        "kind":"invocation.outcome",
                        "payload":{"status":"cancelled","result":{"status":"cancelled"}},
                        "terminal":true
                    })]
                } else if exec_id.starts_with("operation.") && from_seq <= 1 {
                    // A workspace Changes query (`operation.<id>.1.<vm>`)
                    // journals its one terminal outcome, so the API's drain
                    // has something to acknowledge and the ack's authority is
                    // exercised against the dispatcher rule.
                    vec![json!({
                        "seq":1,
                        "kind":"invocation.outcome",
                        "payload":{"status":"succeeded","result":{"members":[]}},
                        "terminal":true
                    })]
                } else if exec_id.starts_with("task.") {
                    let (page, more) = journal_events.page(from_seq, limit, max_bytes).await;
                    has_more = more;
                    page
                } else {
                    Vec::new()
                };
                let next_seq = events
                    .last()
                    .and_then(|event| event["seq"].as_u64())
                    .map_or(from_seq, |seq| seq + 1);
                json!({"jsonrpc":"2.0","id":id,"result":{
                    "exec_id":exec_id,"events":events,"has_more":has_more,"next_seq":next_seq}})
            }
            "events.ack" => {
                if ack_failure.load(Ordering::Relaxed) {
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":503,"message":"ACK_UNAVAILABLE","data":{}}})
                } else {
                    let through = params["up_to_seq"].as_u64().unwrap_or(0);
                    acked_seqs.lock().await.push(through);
                    json!({
                        "jsonrpc":"2.0","id":id,"result":{"exec_id":exec_id_of(params),"acked_through":through}
                    })
                }
            }
            "fs.watch" => json!({
                "jsonrpc":"2.0","id":id,"result":{"watching":true}
            }),
            "fs.tree" if tree_rpc_unavailable => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": "METHOD_NOT_FOUND", "data": {}}
            }),
            "fs.tree" => {
                let Some(repo_id) = params["repo_id"]
                    .as_str()
                    .and_then(|value| value.parse::<Uuid>().ok())
                else {
                    return;
                };
                let Some(path) = params["path"].as_str() else {
                    return;
                };
                let depth = params["depth"].as_i64().unwrap_or(-1) as i32;
                let tracked: std::collections::BTreeSet<String> = params["tracked"]
                    .as_array()
                    .map(|paths| {
                        paths
                            .iter()
                            .filter_map(|path| path.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                let limits = g5d_dispatch::file_tree::WalkLimits::reply(
                    params["max_entries"].as_u64().unwrap_or(50_000) as usize,
                    params["max_bytes"].as_u64().unwrap_or(16 * 1024 * 1024) as usize,
                );
                let files = files.lock().await;
                let source = PeerTreeSource {
                    files: &files,
                    repo_id,
                };
                // Like the guest, a missing root is the caller's error.
                if matches!(
                    source.list(path),
                    Ok(g5d_dispatch::file_tree::Listing::Missing)
                ) {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 404, "message": "FILE_MISSING", "data": {}}
                    })
                } else {
                    // The guest's own walker defines the live tree; the peer
                    // only supplies the directory contents.
                    let walk =
                        g5d_dispatch::file_tree::walk_tree(&source, path, depth, &tracked, limits)
                            .unwrap_or_else(|never| match never {});
                    let entries = walk
                        .entries
                        .iter()
                        .map(|entry| {
                            json!({
                                "path": entry.path,
                                "type": if entry.is_dir { "directory" } else { "file" },
                                "size": entry.size,
                            })
                        })
                        .collect::<Vec<_>>();
                    json!({
                        "jsonrpc":"2.0","id":id,
                        "result":{
                            "path":path,
                            "entries":entries,
                            "truncated":walk.truncated,
                            // The checkout HEAD the guest reads from `.git`;
                            // the scripted Changes members carry the same.
                            "head_oid": if tree_without_head { None } else { Some(LIVE_HEAD_OID) },
                        }
                    })
                }
            }
            "fs.read" => {
                let Some(repo_id) = params["repo_id"]
                    .as_str()
                    .and_then(|value| value.parse::<Uuid>().ok())
                else {
                    return;
                };
                let Some(path) = params["path"].as_str() else {
                    return;
                };
                if let Some(barrier) = read_barrier.as_ref().filter(|barrier| barrier.path == path)
                {
                    let released = barrier.release.notified();
                    if barrier.arrived.fetch_add(1, Ordering::AcqRel) + 1 >= barrier.required {
                        barrier.release.notify_waiters();
                    } else {
                        released.await;
                    }
                }
                let read_refused = refused_reads
                    .lock()
                    .await
                    .contains(&(repo_id, path.to_string()));
                match files
                    .lock()
                    .await
                    .get(&(repo_id, path.to_string()))
                    .cloned()
                {
                    Some(_) if read_refused => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 500, "message": "RUNTIME_IO_ERROR", "data": {}}
                    }),
                    Some(file) => match (params.get("offset"), params.get("length")) {
                        // Like the guest: a whole-file reply has a size
                        // limit; a larger file is read in windows or
                        // named by its hash.
                        (None, None) if file.content.len() > PEER_REPLY_LIMIT => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {"code": 413, "message": "REPLY_TOO_LARGE", "data": {}}
                        }),
                        (None, None) => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {"content": file.content, "encoding": "utf-8", "hash": file.hash}
                        }),
                        // The guest's window rules: both or neither, and a
                        // length between 1 and the file byte limit.
                        (Some(offset), Some(length))
                            if offset.as_u64().is_some()
                                && length.as_u64().is_some_and(|length| {
                                    length > 0
                                        && length <= g5d_dispatch::files::MAX_FILE_BYTES as u64
                                }) =>
                        {
                            let offset = offset.as_u64().unwrap_or_default();
                            let length = length.as_u64().unwrap_or_default();
                            // Like the guest: the window's bytes as base64
                            // with the whole file's size and hash.
                            let bytes = file.content.as_bytes();
                            let start = (offset as usize).min(bytes.len());
                            let end = start.saturating_add(length as usize).min(bytes.len());
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": base64::engine::general_purpose::STANDARD
                                        .encode(&bytes[start..end]),
                                    "encoding": "base64",
                                    "hash": file.hash,
                                    "size": bytes.len(),
                                    "offset": start,
                                    "length": end - start,
                                }
                            })
                        }
                        _ => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {"code": -32602, "message": "INVALID_PARAMS", "data": {}}
                        }),
                    },
                    None => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 404, "message": "FILE_MISSING", "data": {}}
                    }),
                }
            }
            "fs.list" => {
                let Some(repo_id) = params["repo_id"]
                    .as_str()
                    .and_then(|value| value.parse::<Uuid>().ok())
                else {
                    return;
                };
                let Some(path) = params["path"].as_str() else {
                    return;
                };
                // Like the guest, one directory level per call: files directly
                // under `path` plus one "directory" entry per child folder.
                let prefix = if path.trim_end_matches('/').is_empty() {
                    String::new()
                } else {
                    format!("{}/", path.trim_end_matches('/'))
                };
                let files = files.lock().await;
                let mut entries = Vec::new();
                let mut seen_directories = std::collections::BTreeSet::new();
                for ((candidate_repo_id, candidate_path), file) in files.iter() {
                    if *candidate_repo_id != repo_id {
                        continue;
                    }
                    let Some(relative) = candidate_path.strip_prefix(&prefix) else {
                        continue;
                    };
                    if relative.is_empty() {
                        continue;
                    }
                    match relative.split_once('/') {
                        None => entries.push(json!({
                            "name": relative,
                            "path": candidate_path,
                            "type": "file",
                            "size": file.content.len(),
                            "mode": 0o644,
                            "mtime": 0,
                        })),
                        Some((directory, _)) => {
                            if seen_directories.insert(directory.to_string()) {
                                entries.push(json!({
                                    "name": directory,
                                    "path": format!("{prefix}{directory}"),
                                    "type": "directory",
                                    "size": 0,
                                    "mode": 0o755,
                                    "mtime": 0,
                                }));
                            }
                        }
                    }
                }
                entries.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
                if entries.is_empty() {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 404, "message": "FILE_MISSING", "data": {}}
                    })
                } else {
                    json!({
                        "jsonrpc":"2.0","id":id,
                        "result":{"path":path,"entries":entries}
                    })
                }
            }
            "fs.write" => {
                let Some(repo_id) = params["repo_id"]
                    .as_str()
                    .and_then(|value| value.parse().ok())
                else {
                    return;
                };
                let Some(path) = params["path"].as_str() else {
                    return;
                };
                let Some(content) = params["content"].as_str() else {
                    return;
                };
                let expected_hash = params["expected_hash"].as_str();
                let key = (repo_id, path.to_string());
                let refused = refused_writes.lock().await.contains(&key);
                let mut files = files.lock().await;
                let conflict = match (files.get(&key), expected_hash) {
                    (Some(_), None) => Some("FILE_EXISTS"),
                    (Some(current), Some(expected)) if current.hash != expected => {
                        Some("FILE_VERSION_CONFLICT")
                    }
                    (None, Some(_)) => Some("FILE_VERSION_CONFLICT"),
                    _ => None,
                };
                let conflict = if refused {
                    Some("FILE_VERSION_CONFLICT")
                } else {
                    conflict
                };
                if let Some(message) = conflict {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 409, "message": message, "data": {}}
                    })
                } else {
                    let hash = blob_hash(ObjectFormat::Sha1, content.as_bytes());
                    files.insert(
                        key,
                        StoredFile {
                            content: content.to_string(),
                            hash: hash.clone(),
                        },
                    );
                    json!({"jsonrpc": "2.0", "id": id, "result": {"hash": hash}})
                }
            }
            "fs.delete" => {
                let Some(repo_id) = params["repo_id"]
                    .as_str()
                    .and_then(|value| value.parse().ok())
                else {
                    return;
                };
                let Some(path) = params["path"].as_str() else {
                    return;
                };
                let expected_hash = params["expected_hash"].as_str();
                let key = (repo_id, path.to_string());
                let refused = refused_writes.lock().await.contains(&key);
                let mut files = files.lock().await;
                match files.get(&key) {
                    Some(_) if refused => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 409, "message": "FILE_VERSION_CONFLICT", "data": {}}
                    }),
                    None => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 404, "message": "FILE_MISSING", "data": {}}
                    }),
                    // Like the guest: a file delete without its hash is refused.
                    Some(_) if expected_hash.is_none() => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32602, "message": "INVALID_PARAMS", "data": {}}
                    }),
                    Some(current) if expected_hash.is_some_and(|hash| hash != current.hash) => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {"code": 409, "message": "FILE_VERSION_CONFLICT", "data": {}}
                        })
                    }
                    Some(_) => {
                        files.remove(&key);
                        json!({"jsonrpc": "2.0", "id": id, "result": {"path": path, "deleted": true}})
                    }
                }
            }
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": "METHOD_NOT_FOUND", "data": {}}
            }),
        }
    };
    let mut encoded = serde_json::to_vec(&reply).expect("serialize scripted runtime reply");
    encoded.push(b'\n');
    let _ = writer.send(&encoded).await;
    writer.close().await;
}

/// Flush the workspace as the guest does: start, push the two flush leaves
/// (created here directly, once, as receive-pack creates them), complete.
/// The guest's `workspace.changes` `rebase` on exact inputs, run with real
/// Git in a scratch checkout of the member and published to G5 with the
/// captured head as the only lease (`g5.architecture.workspace_rebase`).
/// The production guest operation has its own dispatcher coverage.
async fn scripted_rebase(
    pool: &PgPool,
    repo_storage: &g5_api_core::services::git::repo_store::TenantRepoStorage,
    git_bin: &std::path::Path,
    claims: &WorkspaceIncarnationClaims,
    input: &Value,
) -> Value {
    let repo_id = input["repo_id"].clone();
    let refuse = |code: &str, extra: Value| {
        let mut member = json!({"repo_id":repo_id,"rebased":false,"errors":[code]});
        if let (Some(member), Some(extra)) = (member.as_object_mut(), extra.as_object()) {
            member.extend(extra.clone());
        }
        member
    };
    let (Some(expected), Some(onto_branch), Some(onto)) = (
        input["expected_head"].as_str(),
        input["onto_branch"].as_str(),
        input["onto_head"].as_str(),
    ) else {
        return refuse("INVALID_INPUT", json!({}));
    };
    let branch: String = sqlx::query_scalar(
        "SELECT branch_name FROM authz.lookup_workspace_runtime_durable_members($1, $2)
         WHERE repo_id::text = $3",
    )
    .bind(claims.workspace_id)
    .bind(claims.owner_user_id)
    .bind(input["repo_id"].as_str().unwrap_or_default())
    .fetch_one(pool)
    .await
    .expect("workspace branch");
    let central = repo_storage
        .repo_path(
            claims,
            claims.org_id,
            input["repo_id"]
                .as_str()
                .and_then(|id| Uuid::parse_str(id).ok())
                .expect("member repository id"),
        )
        .expect("member repository");
    let run = |dir: &std::path::Path, args: &[&str]| {
        let output = std::process::Command::new(git_bin)
            .current_dir(dir)
            .args([
                "-c",
                "user.name=Workspace VM",
                "-c",
                "user.email=vm@g5.test",
            ])
            .args(args)
            .output()
            .expect("git");
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )
    };
    if run(&central, &["rev-parse", &format!("refs/heads/{branch}")]).1 != expected {
        return refuse("INTERNAL_BRANCH_MOVED", json!({}));
    }
    if run(
        &central,
        &["rev-parse", &format!("refs/heads/{onto_branch}")],
    )
    .1 != onto
    {
        return refuse("TARGET_MOVED", json!({}));
    }
    let scratch = tempfile::tempdir().expect("scratch checkout");
    let work = scratch.path();
    run(work, &["init", "-q"]);
    run(
        work,
        &["fetch", "-q", central.to_str().unwrap(), expected, onto],
    );
    run(work, &["checkout", "-q", "-B", "work", expected]);
    let ancestor = |a: &str, b: &str| run(work, &["merge-base", "--is-ancestor", a, b]).0;
    if !ancestor(onto, expected) {
        if ancestor(expected, onto) {
            run(work, &["reset", "-q", "--hard", onto]);
        } else if !run(
            work,
            &[
                "rebase",
                "-q",
                "--no-rebase-merges",
                "--no-fork-point",
                onto,
            ],
        )
        .0
        {
            let conflicts = run(work, &["diff", "--name-only", "--diff-filter=U"]).1;
            run(work, &["rebase", "--abort"]);
            return refuse(
                "REBASE_CONFLICT",
                json!({"conflicts": conflicts.lines().collect::<Vec<_>>()}),
            );
        }
    }
    let head = run(work, &["rev-parse", "HEAD"]).1;
    if head == expected {
        return json!({"repo_id":repo_id,"rebased":true,"head":head,"published":false});
    }
    run(&central, &["fetch", "-q", work.to_str().unwrap(), "work"]);
    if !run(
        &central,
        &[
            "update-ref",
            &format!("refs/heads/{branch}"),
            &head,
            expected,
        ],
    )
    .0
    {
        return refuse("INTERNAL_BRANCH_MOVED", json!({"local_head":head}));
    }
    json!({"repo_id":repo_id,"rebased":true,"head":head,"previous_head":expected,"published":true})
}

/// The guest's `workspace.changes` `adopt` on exact inputs: the member branch
/// moves to the provider tip pinned as the divergence run's remote input,
/// with the captured G5 head as the only lease
/// (`g5.workspace.remote.adopt-provider-history`). The checkout-side
/// refusals have their own coverage in `g5-scm`.
async fn scripted_adopt(
    pool: &PgPool,
    repo_storage: &g5_api_core::services::git::repo_store::TenantRepoStorage,
    git_bin: &std::path::Path,
    claims: &WorkspaceIncarnationClaims,
    input: &Value,
) -> Value {
    let repo_id = input["repo_id"].clone();
    let refuse = |code: &str| json!({"repo_id":repo_id,"adopted":false,"errors":[code]});
    let (Some(expected), Some(tip), Some(run_id)) = (
        input["expected_head"].as_str(),
        input["provider_tip"].as_str(),
        input["run_id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok()),
    ) else {
        return refuse("INVALID_INPUT");
    };
    let branch: String = sqlx::query_scalar(
        "SELECT branch_name FROM authz.lookup_workspace_runtime_durable_members($1, $2)
         WHERE repo_id::text = $3",
    )
    .bind(claims.workspace_id)
    .bind(claims.owner_user_id)
    .bind(input["repo_id"].as_str().unwrap_or_default())
    .fetch_one(pool)
    .await
    .expect("workspace branch");
    let central = repo_storage
        .repo_path(
            claims,
            claims.org_id,
            input["repo_id"]
                .as_str()
                .and_then(|id| Uuid::parse_str(id).ok())
                .expect("member repository id"),
        )
        .expect("member repository");
    let run = |args: &[&str]| {
        let output = std::process::Command::new(git_bin)
            .current_dir(&central)
            .args(args)
            .output()
            .expect("git");
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )
    };
    if run(&["rev-parse", &format!("refs/heads/{branch}")]).1 != expected {
        return refuse("INTERNAL_BRANCH_MOVED");
    }
    let pinned = g5_contracts::workspace_git::resolution_ref(claims.workspace_id, run_id, "remote");
    if run(&["rev-parse", "--verify", &pinned]).1 != tip {
        return refuse("PROVIDER_INPUT_MOVED");
    }
    if expected == tip {
        return json!({"repo_id":repo_id,"adopted":true,"head":tip,"published":false});
    }
    if !run(&["update-ref", &format!("refs/heads/{branch}"), tip, expected]).0 {
        return refuse("INTERNAL_BRANCH_MOVED");
    }
    json!({"repo_id":repo_id,"adopted":true,"head":tip,"previous_head":expected,"published":true})
}

async fn complete_test_flush(
    pool: &PgPool,
    repo_storage: g5_api_core::services::git::repo_store::TenantRepoStorage,
    git_bin: std::path::PathBuf,
    claims: &WorkspaceIncarnationClaims,
    request_key: &str,
) -> Result<g5_internal_api::workspace_flush::FlushCompleteResponse, String> {
    let service = g5_api::services::workspace_flush_service::WorkspaceFlushService::new(
        pool.clone(),
        repo_storage.clone(),
        git_bin,
    );
    let started = service
        .start(claims, FlushReason::Shutdown, request_key)
        .await
        .map_err(|error| error.to_string())?;
    let rows = sqlx::query(
        "SELECT repo_id, branch_name FROM authz.lookup_workspace_runtime_durable_members($1, $2)
         ORDER BY repo_id",
    )
    .bind(claims.workspace_id)
    .bind(claims.owner_user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let receipts = rows
        .into_iter()
        .map(|row| {
            let repo_id: Uuid = row.try_get("repo_id").map_err(|error| error.to_string())?;
            let branch_name: String = row
                .try_get("branch_name")
                .map_err(|error| error.to_string())?;
            let path = repo_storage
                .repo_path(claims, claims.org_id, repo_id)
                .map_err(|_| "missing test mirror")?;
            let repo = git2::Repository::open(path).map_err(|error| error.to_string())?;
            let tip = repo
                .refname_to_id(&format!("refs/heads/{branch_name}"))
                .map_err(|error| error.to_string())?;
            for leaf in ["snapshot", "branch"] {
                let name = format!(
                    "refs/g5/flush/{}/{}/{leaf}",
                    claims.workspace_id, started.flush_id
                );
                if repo.refname_to_id(&name).is_err() {
                    repo.reference(&name, tip, false, "test flush leaf")
                        .map_err(|error| error.to_string())?;
                }
            }
            let oid = tip.to_string();
            Ok(FlushRepoReceipt {
                repo_id,
                branch_oid: oid.clone(),
                local_branch_oid: oid.clone(),
                local_head_oid: oid.clone(),
                head_ref: Some(format!("refs/heads/{branch_name}")),
                sequencer: SequencerState::None,
                snapshot_oid: oid,
                dirty: false,
                dirty_paths: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if receipts
        .iter()
        .map(|receipt| receipt.repo_id)
        .collect::<Vec<_>>()
        != started.required_repo_ids
    {
        return Err("test dispatcher flush member vector changed".to_string());
    }
    service
        .complete(claims, started.flush_id, receipts)
        .await
        .map_err(|error| error.to_string())
}

fn exec_id_of(params: &Value) -> Value {
    params["exec_id"].clone()
}
