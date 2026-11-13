use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(feature = "test-support")]
use g5_contracts::terminal::TerminalTestHook;
use g5_contracts::terminal::{
    TerminalClientMessage, TerminalClose, TerminalRejection, TerminalServerMessage,
    HOSTED_TERMINAL_PREFIX,
};
use g5_internal_api::stream_mux::{self, Channel, Mux, MuxReader, MuxStream};
use g5d_agent::port_scan::{PortEvent, PortScanConfig, PortScanner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::config::{DispatchConfig, DISPATCHER_CONTRACT_VERSION};
use crate::files::{FileError, FileService, MAX_FILE_BYTES};
use crate::git_transport::GitTransport;
use crate::journal::{Journal, DEFAULT_JOURNAL_BUDGET_BYTES};
use crate::lease::Lease;
use crate::modules::{builtin, Catalog, ExecId, ModuleRuntime, Secrets};
use crate::protocol::StreamPermit;
use crate::protocol::{now_ms, Admission, Context, RpcError, RpcResult};
use crate::watcher::NativeWatcher;
use crate::workspace_flush::WorkspaceFlush;
use g5_internal_api::stream_mux::MuxWriter;
use g5d_foundation::terminal::{
    TerminalCloseReason, TerminalConfig, TerminalManager, TerminalSession,
};

// JSON permits a six-byte escape for each decoded UTF-8 control byte. This
// bound includes an envelope allowance; decoded content is separately capped.
pub const MAX_RPC_BYTES: usize = MAX_FILE_BYTES * 6 + 64 * 1024;
const MAX_INFLIGHT_RPCS: usize = 8;
const MAX_COMPLETED_MUTATIONS: usize = 1024;
const MUTATION_RETENTION: Duration = Duration::from_secs(300);
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(50);
const STREAM_DRAIN_TIMEOUT: Duration = Duration::from_secs(7);
#[cfg(feature = "test-support")]
static TEST_TERMINAL_SHUTDOWN_COMPLETE: AtomicBool = AtomicBool::new(false);
const METHODS: &[&str] = &[
    "fs.list",
    "fs.tree",
    "fs.read",
    "fs.stat",
    "fs.write",
    "fs.delete",
    "fs.rename",
    "fs.watch",
    "ports.list",
    "runtime.inspect.summary",
    "health",
    "admission.close",
    "admission.open",
    "modules.list",
    "modules.describe",
    "modules.invoke",
    "modules.status",
    "modules.cancel",
    "modules.logs",
    "modules.events",
    "modules.history",
    "modules.input.respond",
    "modules.input.steer",
    "modules.background.end",
    "events.replay",
    "events.ack",
    "outcomes.pending",
    "diagnostics.capture",
    "diagnostics.probe",
    "workspace.flush",
];

struct CachedMutation {
    digest: Vec<u8>,
    expires: Instant,
    result: RpcResult,
}
struct Runtime {
    config: Arc<DispatchConfig>,
    files: Arc<FileService>,
    admission: Arc<Admission>,
    modules: Arc<ModuleRuntime>,
    lease: Mutex<Lease>,
    lifecycle: Arc<tokio::sync::Mutex<()>>,
    mutations: Mutex<HashMap<String, CachedMutation>>,
    mutation_serial: tokio::sync::Mutex<()>,
    scanner: PortScanner,
    ports: tokio::sync::RwLock<Vec<u16>>,
    events: broadcast::Sender<Value>,
    file_events: broadcast::Sender<Value>,
    watches: Arc<Mutex<NativeWatcher>>,
    started: Instant,
    boot_time: u64,
    flush: OnceLock<Arc<WorkspaceFlush>>,
    terminals: Arc<TerminalManager>,
    rpc_control_active: Arc<AtomicBool>,
}

struct RpcControlGuard(Arc<AtomicBool>);

type ConnectionAttachments = Arc<Mutex<HashSet<(String, u64)>>>;

struct ConnectionAttachmentGuard {
    attachments: ConnectionAttachments,
    terminal_id: String,
    generation: u64,
    finished: bool,
}

impl ConnectionAttachmentGuard {
    fn new(attachments: ConnectionAttachments, terminal_id: String, generation: u64) -> Self {
        attachments
            .lock()
            .unwrap()
            .insert((terminal_id.clone(), generation));
        Self {
            attachments,
            terminal_id,
            generation,
            finished: false,
        }
    }

    fn finish(&mut self) {
        self.finished = true;
    }
}

impl Drop for ConnectionAttachmentGuard {
    fn drop(&mut self) {
        if self.finished {
            self.attachments
                .lock()
                .unwrap()
                .remove(&(self.terminal_id.clone(), self.generation));
        }
    }
}

impl Drop for RpcControlGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Every configured secret value is redacted from module logs, events and
/// diagnostics regardless of which key carried it (design v9 §6.6 item 6).
pub(crate) fn configured_secrets(config: &DispatchConfig) -> Secrets {
    let mut values = vec![
        config.workspace_token.clone(),
        config
            .listener_token
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    ];
    values.extend(
        std::env::vars()
            .filter(|(key, _)| key.starts_with("G5D_SECRET_"))
            .map(|(_, value)| value),
    );
    Secrets::new(values)
}

pub async fn run_from_env() -> anyhow::Result<()> {
    let config = Arc::new(DispatchConfig::from_env()?);
    // Confirm workspace ownership before admitting repository writers. Source
    // recovery normally hands over the tree first; traversing it also repairs
    // an interrupted handoff without trusting a cached completion marker.
    if let Some(uid) = config.module_uid {
        if g5d_foundation::ownership::can_hand_over() {
            let config = config.clone();
            let started = Instant::now();
            tokio::task::spawn_blocking(move || {
                g5d_foundation::ownership::chown_guest_tree(
                    &config.paths.workspace_root,
                    uid,
                    config.module_gid,
                )
            })
            .await??;
            tracing::info!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                "workspace ownership repair completed"
            );
        }
    }
    let lease = Lease::new(
        config.workspace_token.clone(),
        config.vm_id,
        config.lease_ttl_secs,
    )?;
    let files = Arc::new(
        FileService::new(config.repo_roots(), config.runtime_excludes())?
            .with_owner(config.module_uid, config.module_gid),
    );
    let git = Arc::new(crate::git::GitRuntime::install(&config)?);
    // The catalog is hash-pinned by the host bundle; a missing pin, a
    // mismatch or a missing catalog fails closed before the listener exists
    // (§6.7 item 13). There is no unpinned production mode.
    let expected_hash = std::env::var("G5D_MODULE_CATALOG_HASH")
        .ok()
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "G5D_MODULE_CATALOG_HASH must pin the module catalog (64 hex characters)"
            )
        })?;
    let catalog_file = config.paths.catalog_path();
    let catalog = Catalog::load(&catalog_file, Some(&expected_hash))
        .map_err(|error| anyhow::anyhow!("module catalog {}: {error}", catalog_file.display()))?;
    // Dispatcher-owned state: the directory itself stays traversable so a
    // module identity can read the nonsecret Git configuration and excludes
    // installed there; the journal, watermarks, dispositions and credential
    // helper inside it are owner-only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            &config.paths.state_dir,
            std::fs::Permissions::from_mode(0o711),
        )?;
    }
    let journal = Arc::new(Journal::open(
        &config.paths.state_dir,
        DEFAULT_JOURNAL_BUDGET_BYTES,
    )?);
    config.publish_workspace_token(lease.token())?;
    let secrets = configured_secrets(&config);
    let admission: Arc<Admission> = Arc::default();
    let (module_events, _) = broadcast::channel(1024);
    let modules = ModuleRuntime::new(
        config.clone(),
        catalog,
        catalog_file,
        journal,
        secrets,
        admission.clone(),
        git.clone(),
        module_events,
    )
    .arc();
    let mut terminal_env: HashMap<String, String> = git
        .process_environment()
        .into_iter()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    // Use the workspace home and CLI path assembled by GitRuntime. Never
    // replace them with the dispatcher's own home/path.
    for key in g5d_foundation::runtime_environment::NATIVE_LOADER_PATH_KEYS {
        terminal_env.remove(key);
    }
    terminal_env.extend(
        g5d_foundation::runtime_environment::workspace_cli_environment(
            config.api_url.as_str(),
            &config.workspace_id.to_string(),
            &config.workspace_token_file().to_string_lossy(),
        ),
    );
    let terminals = Arc::new(TerminalManager::new(TerminalConfig {
        max_sessions: std::env::var("G5D_MAX_PTY_STREAMS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=16).contains(value))
            .unwrap_or(8),
        workspace_path: config.paths.workspace_root.clone(),
        env_vars: terminal_env,
        uid: config.module_uid,
        gid: config.module_gid,
        ..TerminalConfig::default()
    }));
    // Executions this process never saw finish are settled before anything
    // can ask about them; a restart never relaunches an identity.
    modules
        .finalize_interrupted()
        .map_err(|error| anyhow::anyhow!("journal recovery: {}", error.value()))?;
    let listener = TcpListener::bind((config.listen_addr(), config.dispatch_port)).await?;
    let scanner = PortScanner::new(PortScanConfig {
        interval: Duration::from_secs(1),
        exclude_ports: vec![config.dispatch_port],
        identify_processes: false,
    });
    let (events, _) = broadcast::channel(128);
    let (watcher, mut native_events) = tokio::task::spawn_blocking({
        let repositories = config.repo_roots();
        let files = files.clone();
        move || NativeWatcher::new(repositories, files)
    })
    .await
    .map_err(|error| anyhow::anyhow!("native watcher initialization task: {error}"))??;
    let (file_events, _) = broadcast::channel(1024);
    let runtime = Arc::new(Runtime {
        config,
        files,
        admission,
        modules,
        lease: Mutex::new(lease),
        lifecycle: Arc::new(tokio::sync::Mutex::new(())),
        mutations: Mutex::new(HashMap::new()),
        mutation_serial: tokio::sync::Mutex::new(()),
        scanner,
        ports: tokio::sync::RwLock::new(Vec::new()),
        events,
        file_events,
        watches: Arc::new(Mutex::new(watcher)),
        started: Instant::now(),
        boot_time: now_ms(),
        flush: OnceLock::new(),
        terminals,
        rpc_control_active: Arc::new(AtomicBool::new(false)),
    });
    runtime
        .modules
        .set_terminal_host(Arc::clone(&runtime.terminals));
    runtime.modules.set_observer(Box::new({
        let runtime = runtime.clone();
        move || {
            let ports = runtime
                .ports
                .try_read()
                .map(|ports| ports.clone())
                .unwrap_or_default();
            json!({"health":health(&runtime),"ports":ports,"resources":resource_observation()})
        }
    }));
    // The token-isolated internal Git transport reads the current lease
    // token at request time, so a rotated token is used immediately.
    let transport = GitTransport::start(
        git.clone(),
        Arc::new({
            let runtime = runtime.clone();
            move || runtime.lease.lock().unwrap().token().to_owned()
        }),
        runtime.config.git_door_port,
    )
    .await?;
    // Keep native origin routes alive until this dispatcher incarnation exits.
    let _native_git_leases = git.configure_native_remotes(&transport)?;
    runtime
        .flush
        .set(Arc::new(WorkspaceFlush::new(
            runtime.config.clone(),
            git,
            transport.clone(),
        )))
        .map_err(|_| anyhow::anyhow!("workspace flush runtime already installed"))?;
    let code_pushes = transport.pushes();
    runtime.modules.set_transport(transport);
    let git_listener = crate::workspace_git::bind(&runtime.modules).await?;
    let mut background = JoinSet::new();
    background.spawn(crate::workspace_git::serve(
        git_listener,
        runtime.modules.clone(),
    ));
    background.spawn(crate::workspace_git::refresh_loop(runtime.modules.clone()));
    background.spawn(crate::workspace_git::memory_sync_loop(
        runtime.modules.clone(),
        code_pushes,
    ));
    background.spawn(scan_ports(runtime.clone()));
    background.spawn({
        let file_events = runtime.file_events.clone();
        async move {
            while let Some(event) = native_events.next().await {
                let _ = file_events.send(event);
            }
        }
    });
    background.spawn({
        let runtime = runtime.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            loop {
                tick.tick().await;
                if !runtime.lease.lock().unwrap().valid() {
                    runtime.admission.refuse_new();
                }
            }
        }
    });
    let mut handshakes = JoinSet::new();
    let mut active: Option<(CancellationToken, tokio::task::JoinHandle<()>)> = None;
    // The host dials one connection at a time and abandons one that sends no
    // `runtime.ready` within its probe window, so it only ever keeps the
    // newest. An abandoned socket still carries the listener token and can
    // finish its handshake after a newer one, notably when the host probed
    // between bind and the first accept. Accept order decides which is newer.
    let mut accepted_seq = 0_u64;
    let mut installed_seq = 0_u64;
    loop {
        let (seq, socket) = tokio::select! {
            accepted = listener.accept(), if handshakes.len() < 32 => {
                let Some(mut socket) = accepted_socket(accepted).await else {
                    continue;
                };
                accepted_seq += 1;
                let seq = accepted_seq;
                // Lets a test order its connections by acceptance, not by timing.
                #[cfg(feature = "test-support")]
                if let Some(log) = std::env::var_os("G5D_DISPATCH_TEST_ACCEPT_LOG") {
                    use std::io::Write;
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(log)
                    {
                        let _ = writeln!(file, "{seq}");
                    }
                }
                let expected = runtime.config.listener_token;
                handshakes.spawn(async move {
                    authenticate(&mut socket, &expected)
                        .await
                        .then_some((seq, socket))
                });
                continue;
            }
            authenticated = handshakes.join_next(), if !handshakes.is_empty() => {
                match authenticated {
                    Some(Ok(Some(authenticated))) => authenticated,
                    _ => continue,
                }
            }
        };
        if seq < installed_seq {
            tracing::warn!(
                seq,
                installed_seq,
                "dispatcher closed a host connection accepted before the active one"
            );
            continue;
        }
        installed_seq = seq;
        if let Some((cancel, task)) = active.take() {
            cancel.cancel();
            let _ = task.await;
            runtime.lease.lock().unwrap().peer_replaced();
        }
        let cancel = CancellationToken::new();
        active = Some((
            cancel.clone(),
            tokio::spawn(connection(socket, runtime.clone(), cancel)),
        ));
    }
}

async fn accepted_socket(accepted: std::io::Result<(TcpStream, SocketAddr)>) -> Option<TcpStream> {
    match accepted {
        Ok((socket, _)) => Some(socket),
        Err(error) => {
            tracing::warn!(%error, "dispatcher accept failed");
            tokio::time::sleep(ACCEPT_ERROR_BACKOFF).await;
            None
        }
    }
}

async fn authenticate(socket: &mut TcpStream, expected: &[u8; 32]) -> bool {
    let mut actual = [0; 32];
    if !matches!(
        tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut actual)).await,
        Ok(Ok(_))
    ) {
        return false;
    }
    actual
        .iter()
        .zip(expected)
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}
async fn scan_ports(runtime: Arc<Runtime>) {
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tick.tick().await;
        match runtime.scanner.scan_once().await {
            Ok(events) => {
                let mut ports = runtime.ports.write().await;
                for event in events {
                    let notification = match event {
                        PortEvent::Open { port, .. } => {
                            if !ports.contains(&port) {
                                ports.push(port);
                            }
                            json!({"jsonrpc":"2.0","method":"port.opened","params":{"port":port,"vm_id":runtime.config.vm_id}})
                        }
                        PortEvent::Close { port } => {
                            ports.retain(|p| *p != port);
                            json!({"jsonrpc":"2.0","method":"port.closed","params":{"port":port,"vm_id":runtime.config.vm_id}})
                        }
                    };
                    let _ = runtime.events.send(notification);
                }
                ports.sort_unstable();
            }
            Err(error) => tracing::warn!(%error,"dispatcher port scan failed"),
        }
    }
}
async fn connection(socket: TcpStream, runtime: Arc<Runtime>, cancel: CancellationToken) {
    let mut mux = Mux::new(socket, false);
    let mut streams = JoinSet::new();
    let attachments: ConnectionAttachments = Arc::new(Mutex::new(HashSet::new()));
    loop {
        let stream = tokio::select! {
            _ = cancel.cancelled() => None,
            stream = mux.accept() => stream,
            _ = streams.join_next(), if !streams.is_empty() => continue,
        };
        let Some(mut stream) = stream else {
            break;
        };
        let runtime = runtime.clone();
        let stream_cancel = cancel.clone();
        let attachments = attachments.clone();
        streams.spawn(async move {
            match stream.channel() {
                Channel::Rpc => serve_rpc(stream, runtime, stream_cancel).await,
                Channel::Http => serve_http(stream, runtime, stream_cancel).await,
                Channel::Pty => serve_pty(stream, runtime, stream_cancel, attachments).await,
                channel => {
                    tracing::warn!(?channel, "dispatcher refused an unsupported mux stream");
                    stream.close().await;
                }
            }
        });
    }
    // Drop the mux first so every stream observes EOF even when connection
    // replacement triggered cancellation before the peer closed its socket.
    drop(mux);
    cancel.cancel();
    let drained = tokio::time::timeout(stream_drain_timeout(), async {
        while streams.join_next().await.is_some() {}
    })
    .await
    .is_ok();
    if !drained {
        tracing::error!("dispatcher stream teardown exceeded its bound");
        #[cfg(feature = "test-support")]
        if let Ok(value) = std::env::var("G5D_DISPATCH_TEST_PAUSE_BEFORE_DRAIN_ABORT_MS") {
            tokio::time::sleep(Duration::from_millis(
                value.parse::<u64>().expect("test abort pause"),
            ))
            .await;
        }
        // Join aborted handlers before taking the final attachment snapshot.
        // A handler that attached during timeout cleanup retains its entry
        // when aborted, including one that never reached the normal close path.
        streams.abort_all();
        while streams.join_next().await.is_some() {}
    }
    // Also cover a handler that exited unexpectedly before the drain began.
    let attached: Vec<_> = attachments.lock().unwrap().iter().cloned().collect();
    for (terminal_id, generation) in attached {
        let reason = if drained {
            TerminalCloseReason::ConnectionEnd
        } else {
            TerminalCloseReason::DrainTimeout
        };
        tracing::info!(%terminal_id, generation, close_reason = reason.as_str(), "closing terminal during connection teardown");
        if let Err(error) = runtime
            .terminals
            .close_interactive(&terminal_id, generation)
            .await
        {
            tracing::error!(%error, %terminal_id, generation, "terminal cleanup failed during dispatcher teardown");
        }
    }
}

fn stream_drain_timeout() -> Duration {
    #[cfg(feature = "test-support")]
    if let Ok(value) = std::env::var("G5D_DISPATCH_TEST_STREAM_DRAIN_MS") {
        return Duration::from_millis(value.parse::<u64>().expect("test drain timeout"));
    }
    STREAM_DRAIN_TIMEOUT
}

fn claim_rpc_control(runtime: &Runtime) -> Option<RpcControlGuard> {
    runtime
        .rpc_control_active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .ok()
        .map(|_| RpcControlGuard(runtime.rpc_control_active.clone()))
}

async fn serve_http(stream: MuxStream, runtime: Arc<Runtime>, cancel: CancellationToken) {
    let (writer, mut reader) = stream.into_split();
    let mut header = Vec::with_capacity(stream_mux::HTTP_PORT_HEADER_LEN);
    let received = tokio::time::timeout(Duration::from_secs(10), async {
        while header.len() < stream_mux::HTTP_PORT_HEADER_LEN {
            let Some(chunk) = reader.recv().await else {
                return false;
            };
            header.extend_from_slice(&chunk);
        }
        true
    })
    .await;
    let (Ok(true), Some(port)) = (received, stream_mux::decode_http_port(&header)) else {
        writer.close().await;
        return;
    };
    // A scanned LISTEN socket or a port the preview service reported ready.
    // The latter admits a newly ready preview before the periodic scan catches up.
    let admitted = port != 0
        && (runtime.ports.read().await.contains(&port)
            || runtime.modules.service_port_active(port));
    if !admitted {
        tracing::warn!(
            port,
            "dispatcher refused an HTTP stream to a port no listener or preview reported"
        );
        writer.close().await;
        return;
    }
    // A preview WebSocket can write for as long as its stream stays open.
    // Stop/transfer cancels this bridge before draining; a non-interrupting
    // fork leaves it connected and only drains finite mutations.
    let Ok(admission) = runtime.admission.enter_stream() else {
        writer.close().await;
        return;
    };
    let admission_cancel = admission.cancellation();
    let upstream = match tokio::select! {
        _ = admission_cancel.cancelled() => {
            writer.close().await;
            return;
        }
        result = TcpStream::connect(("127.0.0.1", port)) => result,
    } {
        Ok(upstream) => upstream,
        Err(error) => {
            tracing::warn!(port, %error, "dispatcher could not connect an HTTP stream to its upstream");
            writer.close().await;
            return;
        }
    };
    let (mut upstream_read, mut upstream_write) = upstream.into_split();
    let leftover = header.split_off(stream_mux::HTTP_PORT_HEADER_LEN);
    let to_upstream = async move {
        if !leftover.is_empty() && upstream_write.write_all(&leftover).await.is_err() {
            return;
        }
        while let Some(chunk) = reader.recv().await {
            if upstream_write.write_all(&chunk).await.is_err() {
                return;
            }
        }
        let _ = upstream_write.shutdown().await;
    };
    let from_upstream = async {
        let mut buffer = vec![0_u8; 32 * 1024];
        loop {
            match upstream_read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) if writer.send(&buffer[..read]).await.is_err() => break,
                Ok(_) => {}
            }
        }
    };
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = admission_cancel.cancelled() => {}
        _ = to_upstream => {}
        _ = from_upstream => {}
    }
    writer.close().await;
}

/// The session kind an attachment serves. Relay rules are keyed on it, never
/// on the client: a hosted session (`g5.preview.output.live-tail`) drops
/// input, applies resize and treats close as detach.
enum PtySession {
    Shell(Arc<TerminalSession>),
    Hosted {
        cursor: g5d_foundation::terminal::HostedCursor,
    },
}

struct PtyAttachment {
    session: PtySession,
    terminal_id: String,
    generation: u64,
    attachment_cancel: CancellationToken,
    admission_cancel: CancellationToken,
    _admission: StreamPermit,
    /// Shells register with the connection so an aborted handler still closes
    /// them; a hosted session needs no cleanup, its viewer is only detached.
    guard: Option<ConnectionAttachmentGuard>,
}

enum PtyExit {
    Detached,
    Superseded,
    Close(TerminalCloseReason),
}

async fn serve_pty(
    stream: MuxStream,
    runtime: Arc<Runtime>,
    cancel: CancellationToken,
    attachments: ConnectionAttachments,
) {
    let (writer, mut reader) = stream.into_split();
    let Some(mut attachment) = open_pty(&mut reader, &writer, &runtime, &cancel, attachments).await
    else {
        return;
    };
    let exit = relay_pty(&mut reader, &writer, &runtime, &cancel, &mut attachment).await;
    #[cfg(feature = "test-support")]
    if cancel.is_cancelled() && std::env::var_os("G5D_DISPATCH_TEST_STALL_PTY_TEARDOWN").is_some() {
        // Exercise the connection's bounded fallback with an attached stream
        // that cannot finish normal cancellation in time.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    finish_pty(&runtime, &writer, &cancel, attachment, exit).await;
}

async fn open_pty(
    reader: &mut MuxReader,
    writer: &MuxWriter,
    runtime: &Runtime,
    cancel: &CancellationToken,
    attachments: ConnectionAttachments,
) -> Option<PtyAttachment> {
    let Ok(Some(line)) = tokio::time::timeout(
        Duration::from_secs(10),
        reader.read_line_with_limit(g5_contracts::terminal::TERMINAL_OPEN_FRAME_MAX_BYTES),
    )
    .await
    else {
        writer.close().await;
        return None;
    };
    let Ok(TerminalClientMessage::Open {
        cols,
        rows,
        terminal_id,
        offset,
        secrets,
        #[cfg(feature = "test-support")]
        test_hook,
        ..
    }) = serde_json::from_str(&line)
    else {
        writer.close().await;
        return None;
    };
    // The same policy the API admitted the set under
    // (`g5.terminal.secret-admission`); a refusal names its reason so the
    // browser shows it instead of reconnecting.
    if let Err(error) = g5_contracts::types::task_env::validate_task_env_secrets(
        secrets
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    ) {
        let rejected = TerminalServerMessage::Rejected {
            reason: TerminalRejection::SecretsInvalid,
            detail: Some(error.to_string()),
        }
        .to_line()
        .expect("terminal rejection serialization");
        let _ = writer.send(&rejected).await;
        tracing::info!(code = error.code(), "terminal open refused its secrets");
        writer.close().await;
        return None;
    }
    // Browser PTY streams use the stream admission slot, not a mutation slot.
    // Its permit lives until finish_pty completes.
    let Ok(admission) = runtime.admission.enter_stream() else {
        writer.close().await;
        return None;
    };
    let admission_cancel = admission.cancellation();
    if terminal_id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > g5_contracts::terminal::MAX_TERMINAL_ID_BYTES)
    {
        writer.close().await;
        return None;
    }
    // A hosted session is attached by name and only if the dispatcher already
    // hosts it: nothing a client sends can create one, and the shell path
    // below never sees its namespace.
    if let Some(name) = terminal_id
        .as_deref()
        .filter(|id| id.starts_with(HOSTED_TERMINAL_PREFIX))
    {
        return open_hosted_pty(writer, runtime, name, cols, rows, offset, admission).await;
    }
    #[cfg(feature = "test-support")]
    if test_hook == Some(TerminalTestHook::LateAttachment) {
        if std::env::var_os("G5D_DISPATCH_TEST_WAIT_FOR_SHUTDOWN").is_some() {
            eprintln!("test waiting for terminal shutdown snapshot");
            tokio::time::timeout(Duration::from_secs(5), async {
                while !TEST_TERMINAL_SHUTDOWN_COMPLETE.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("terminal shutdown did not reach the snapshot");
        }
        if let Ok(value) = std::env::var("G5D_DISPATCH_TEST_DELAY_ATTACH_MS") {
            eprintln!("test delayed PTY attachment");
            tokio::time::sleep(Duration::from_millis(
                value.parse::<u64>().expect("test attach delay"),
            ))
            .await;
        }
    }
    let attachment = match runtime
        .terminals
        .attach_interactive(terminal_id.as_deref(), cols, rows, secrets)
        .await
    {
        Ok(attachment) => attachment,
        Err(error) => {
            let rejected = TerminalServerMessage::Rejected {
                reason: TerminalRejection::TerminalUnavailable,
                detail: None,
            }
            .to_line()
            .expect("terminal rejection serialization");
            let _ = writer.send(&rejected).await;
            tracing::info!(?terminal_id, %error, "terminal reattachment refused");
            writer.close().await;
            return None;
        }
    };
    let session = attachment.session;
    let terminal_id = session.id().to_string();
    let generation = attachment.generation;
    let opened_attachment = PtyAttachment {
        session: PtySession::Shell(session.clone()),
        terminal_id: terminal_id.clone(),
        generation,
        attachment_cancel: attachment.cancel,
        admission_cancel,
        _admission: admission,
        guard: Some(ConnectionAttachmentGuard::new(
            attachments,
            terminal_id.clone(),
            generation,
        )),
    };
    #[cfg(feature = "test-support")]
    if test_hook == Some(TerminalTestHook::FailCloseOnce)
        && std::env::var_os("G5D_DISPATCH_TEST_FAIL_CLOSE_ONCE").is_some()
    {
        session.fail_next_shutdown_for_test();
    }
    #[cfg(feature = "test-support")]
    if test_hook == Some(TerminalTestHook::LateAttachment) {
        if let Ok(value) = std::env::var("G5D_DISPATCH_TEST_STALL_AFTER_ATTACH_MS") {
            eprintln!("test late PTY attached {terminal_id} pid={}", session.pid());
            tokio::time::sleep(Duration::from_millis(
                value.parse::<u64>().expect("test attachment stall"),
            ))
            .await;
        }
    }
    let replay = session.replay_from(offset).await;
    let opened = TerminalServerMessage::Opened {
        terminal_id: terminal_id.clone(),
        reattached: attachment.reattached,
        offset: replay.offset,
        reset: replay.reset,
    }
    .to_line()
    .expect("terminal open serialization");
    if writer.send(&opened).await.is_err()
        || (!replay.data.is_empty() && writer.send(&replay.data).await.is_err())
    {
        finish_pty(
            runtime,
            writer,
            cancel,
            opened_attachment,
            PtyExit::Close(TerminalCloseReason::StreamFailure),
        )
        .await;
        return None;
    }
    Some(opened_attachment)
}

/// Attach a viewer to a hosted session (`g5.preview.output.live-tail`). The
/// answer is `opened` with the retained tail from the requested offset, or
/// the same `rejected` an unknown shell gets. Secrets are ignored: nothing
/// is spawned.
async fn open_hosted_pty(
    writer: &MuxWriter,
    runtime: &Runtime,
    name: &str,
    cols: u16,
    rows: u16,
    offset: Option<u64>,
    admission: StreamPermit,
) -> Option<PtyAttachment> {
    let admission_cancel = admission.cancellation();
    let attachment = match runtime
        .terminals
        .attach_hosted(name, cols, rows, offset)
        .await
    {
        Ok(attachment) => attachment,
        Err(error) => {
            // Not a refusal the owner can correct: no owner-facing detail.
            let rejected = TerminalServerMessage::Rejected {
                reason: TerminalRejection::TerminalUnavailable,
                detail: None,
            }
            .to_line()
            .expect("terminal rejection serialization");
            let _ = writer.send(&rejected).await;
            tracing::info!(terminal_id = %name, %error, "hosted terminal attachment refused");
            writer.close().await;
            return None;
        }
    };
    let opened = TerminalServerMessage::Opened {
        terminal_id: name.to_string(),
        reattached: attachment.reattached,
        offset: attachment.replay.offset,
        reset: attachment.replay.reset,
    }
    .to_line()
    .expect("terminal open serialization");
    let generation = attachment.generation;
    let opened_attachment = PtyAttachment {
        session: PtySession::Hosted {
            cursor: attachment.cursor,
        },
        terminal_id: name.to_string(),
        generation,
        attachment_cancel: attachment.cancel,
        admission_cancel,
        _admission: admission,
        guard: None,
    };
    if writer.send(&opened).await.is_err()
        || (!attachment.replay.data.is_empty()
            && writer.send(&attachment.replay.data).await.is_err())
    {
        runtime.terminals.detach_hosted(name, generation).await;
        writer.close().await;
        return None;
    }
    Some(opened_attachment)
}

async fn relay_pty(
    reader: &mut MuxReader,
    writer: &MuxWriter,
    runtime: &Runtime,
    cancel: &CancellationToken,
    attachment: &mut PtyAttachment,
) -> PtyExit {
    let terminal_id = &attachment.terminal_id;
    let generation = attachment.generation;
    let attachment_cancel = &attachment.attachment_cancel;
    let shell = match &mut attachment.session {
        PtySession::Shell(session) => Arc::clone(session),
        PtySession::Hosted { cursor } => {
            return relay_hosted_pty(
                reader,
                writer,
                runtime,
                cancel,
                terminal_id,
                generation,
                attachment_cancel,
                &attachment.admission_cancel,
                &attachment._admission,
                cursor,
            )
            .await
        }
    };
    loop {
        // Cancellation is biased ahead of buffered stale input/output.
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return PtyExit::Close(TerminalCloseReason::ConnectionEnd),
            _ = attachment.admission_cancel.cancelled() => return PtyExit::Close(attachment._admission.close_reason()),
            _ = attachment_cancel.cancelled() => return PtyExit::Superseded,
            message = reader.read_line_with_limit(1024 * 1024) => {
                let Some(message) = message else { return PtyExit::Detached; };
                if attachment_cancel.is_cancelled() {
                    return PtyExit::Superseded;
                }
                match serde_json::from_str::<TerminalClientMessage>(&message) {
                    Ok(TerminalClientMessage::Input { data }) => {
                        if runtime.terminals.write_interactive(terminal_id, generation, data.as_bytes()).await.is_err() {
                            return if attachment_cancel.is_cancelled() {
                                PtyExit::Superseded
                            } else {
                                PtyExit::Close(TerminalCloseReason::StreamFailure)
                            };
                        }
                    }
                    Ok(TerminalClientMessage::Resize { cols, rows }) => {
                        if cols == 0 || rows == 0 {
                            continue;
                        }
                        if runtime.terminals.resize_interactive(terminal_id, generation, cols, rows).await.is_err() {
                            return if attachment_cancel.is_cancelled() {
                                PtyExit::Superseded
                            } else {
                                PtyExit::Close(TerminalCloseReason::StreamFailure)
                            };
                        }
                    }
                    Ok(TerminalClientMessage::Close) => return PtyExit::Close(TerminalCloseReason::ClientClose),
                    Ok(TerminalClientMessage::Open { .. }) | Err(_) => return PtyExit::Close(TerminalCloseReason::StreamFailure),
                }
            }
            emitted = shell.read_raw_for_attachment(attachment_cancel) => {
                if attachment_cancel.is_cancelled() {
                    return PtyExit::Superseded;
                }
                match emitted {
                    Ok(Some(bytes)) if writer.send(&bytes).await.is_err() => return PtyExit::Detached,
                    Ok(Some(_)) => {}
                    Ok(None) => return PtyExit::Close(TerminalCloseReason::ProcessExit),
                    Err(_) => return PtyExit::Close(TerminalCloseReason::StreamFailure),
                }
            }
        }
    }
}

/// The relay for a hosted session's viewer: output flows from the retained
/// ring by cursor; `input` is dropped, `resize` applied, `close` detaches.
/// When the writer side has ended and every retained byte was delivered,
/// one `closed`/`session_ended` control line ends the stream so the browser
/// stops reconnecting; the session itself stays retained.
#[allow(clippy::too_many_arguments)]
async fn relay_hosted_pty(
    reader: &mut MuxReader,
    writer: &MuxWriter,
    runtime: &Runtime,
    cancel: &CancellationToken,
    terminal_id: &str,
    generation: u64,
    attachment_cancel: &CancellationToken,
    admission_cancel: &CancellationToken,
    admission: &StreamPermit,
    cursor: &mut g5d_foundation::terminal::HostedCursor,
) -> PtyExit {
    use g5d_foundation::terminal::HostedReadEnd;
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return PtyExit::Close(TerminalCloseReason::ConnectionEnd),
            _ = admission_cancel.cancelled() => return PtyExit::Close(admission.close_reason()),
            _ = attachment_cancel.cancelled() => return PtyExit::Superseded,
            message = reader.read_line_with_limit(1024 * 1024) => {
                let Some(message) = message else { return PtyExit::Detached; };
                if attachment_cancel.is_cancelled() {
                    return PtyExit::Superseded;
                }
                match serde_json::from_str::<TerminalClientMessage>(&message) {
                    // A hosted session has no input path: the application
                    // reads a terminal nobody types into.
                    Ok(TerminalClientMessage::Input { .. }) => {}
                    Ok(TerminalClientMessage::Resize { cols, rows }) => {
                        if cols == 0 || rows == 0 {
                            continue;
                        }
                        if runtime.terminals.resize_hosted(terminal_id, generation, cols, rows).await.is_err() {
                            return if attachment_cancel.is_cancelled() {
                                PtyExit::Superseded
                            } else {
                                PtyExit::Close(TerminalCloseReason::StreamFailure)
                            };
                        }
                    }
                    // Close detaches the viewer; it never ends the session.
                    Ok(TerminalClientMessage::Close) => return PtyExit::Close(TerminalCloseReason::ClientClose),
                    Ok(TerminalClientMessage::Open { .. }) | Err(_) => return PtyExit::Close(TerminalCloseReason::StreamFailure),
                }
            }
            emitted = cursor.next(attachment_cancel) => {
                match emitted {
                    Ok(bytes) => {
                        if writer.send(&bytes).await.is_err() {
                            return PtyExit::Detached;
                        }
                    }
                    Err(HostedReadEnd::Superseded) => return PtyExit::Superseded,
                    Err(HostedReadEnd::Ended) => {
                        let closed = TerminalServerMessage::Closed {
                            reason: TerminalClose::SessionEnded,
                        }
                        .to_line()
                        .expect("terminal close serialization");
                        let _ = writer.send(&closed).await;
                        return PtyExit::Close(TerminalCloseReason::SessionEnded);
                    }
                    // The viewer is further behind than the ring retains: end
                    // this stream so the browser reattaches with its offset
                    // and receives the normal `reset`.
                    Err(HostedReadEnd::Lagged) => return PtyExit::Close(TerminalCloseReason::ViewerLagged),
                }
            }
        }
    }
}

async fn close_pty(
    runtime: &Runtime,
    terminal_id: &str,
    generation: u64,
    reason: TerminalCloseReason,
) -> Result<(), g5_contracts::G5Error> {
    tracing::info!(%terminal_id, generation, close_reason = reason.as_str(), "closing terminal");
    runtime
        .terminals
        .close_interactive(terminal_id, generation)
        .await
}

async fn finish_pty(
    runtime: &Runtime,
    writer: &MuxWriter,
    cancel: &CancellationToken,
    mut attachment: PtyAttachment,
    exit: PtyExit,
) {
    let terminal_id = &attachment.terminal_id;
    let generation = attachment.generation;
    if matches!(attachment.session, PtySession::Hosted { .. }) {
        // A hosted viewer's end, whatever its reason, only detaches it: the
        // session's life belongs to its writer and the manager.
        match exit {
            // The replaced viewer is told so, as the stream's last frame:
            // without the reason the browser would read a plain close as a
            // dropped connection and reattach, superseding the newer viewer
            // in turn (two tabs on one log alternated every few seconds).
            // The newer viewer's generation already owns the session; there
            // is nothing to detach here.
            PtyExit::Superseded => {
                tracing::info!(%terminal_id, generation, "hosted terminal viewer superseded");
                let closed = TerminalServerMessage::Closed {
                    reason: TerminalClose::Superseded,
                }
                .to_line()
                .expect("terminal close serialization");
                let _ = writer.send(&closed).await;
            }
            PtyExit::Close(reason) => {
                tracing::info!(%terminal_id, generation, close_reason = reason.as_str(), "detaching hosted terminal viewer");
                runtime
                    .terminals
                    .detach_hosted(terminal_id, generation)
                    .await;
            }
            PtyExit::Detached => {
                runtime
                    .terminals
                    .detach_hosted(terminal_id, generation)
                    .await;
            }
        }
        writer.close().await;
        return;
    }
    let mut cleanup_succeeded = true;
    match exit {
        PtyExit::Superseded => {}
        PtyExit::Close(reason) => {
            if let Err(error) = close_pty(runtime, terminal_id, generation, reason).await {
                cleanup_succeeded = false;
                tracing::error!(%error, %terminal_id, generation, close_reason = reason.as_str(), "terminal close failed");
            }
        }
        PtyExit::Detached => {
            if runtime
                .terminals
                .detach_interactive(terminal_id, generation)
                .await
            {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        if let Err(error) = close_pty(runtime, terminal_id, generation, TerminalCloseReason::ConnectionEnd).await {
                            cleanup_succeeded = false;
                            tracing::error!(%error, %terminal_id, generation, "terminal close after transport loss failed");
                        }
                    }
                    _ = attachment.admission_cancel.cancelled() => {
                        // A PTY created after shutdown's session snapshot still
                        // needs its own close before this stream drains.
                        let reason = attachment._admission.close_reason();
                        if let Err(error) = close_pty(runtime, terminal_id, generation, reason).await {
                            cleanup_succeeded = false;
                            tracing::error!(%error, %terminal_id, generation, "terminal close after workspace stop failed");
                        }
                    }
                    _ = attachment.attachment_cancel.cancelled() => {}
                    _ = tokio::time::sleep(runtime.config.pty_reattach_grace) => {
                        match runtime.terminals.close_if_detached(terminal_id, generation).await {
                            Ok(true) => tracing::info!(%terminal_id, generation, close_reason = TerminalCloseReason::DetachGraceExpired.as_str(), "closed detached terminal"),
                            Ok(false) => {}
                            Err(error) => {
                                cleanup_succeeded = false;
                                tracing::error!(%error, %terminal_id, generation, "detached terminal cleanup failed");
                            }
                        }
                    }
                }
            }
        }
    }
    writer.close().await;
    if cleanup_succeeded {
        if let Some(guard) = attachment.guard.as_mut() {
            guard.finish();
        }
    }
}

async fn serve_rpc(stream: MuxStream, runtime: Arc<Runtime>, cancel: CancellationToken) {
    let control = claim_rpc_control(&runtime);
    let (writer, mut reader) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<Value>(32);
    let mut request_tasks = JoinSet::new();
    let mut control_tasks = JoinSet::new();
    let requests = Arc::new(tokio::sync::Semaphore::new(MAX_INFLIGHT_RPCS));
    let writer_task = tokio::spawn(async move {
        while let Some(value) = rx.recv().await {
            let mut bytes = serde_json::to_vec(&value).expect("JSON serialization");
            bytes.push(b'\n');
            if let Err(error) = writer.send(&bytes).await {
                tracing::warn!(
                    ?error,
                    "dispatcher rpc writer failed; closing the connection"
                );
                break;
            }
        }
        writer.close().await;
    });
    let watches = runtime.watches.clone();
    let (replay_gate, releases) = ReplayGate::new();
    if control.is_some() {
        let mut file_events = runtime.file_events.subscribe();
        let port_events = runtime.events.subscribe();
        let module_events = runtime.modules.events.subscribe();
        let observed_ports = runtime.ports.read().await.iter().copied().collect();
        let _=tx.send(json!({"jsonrpc":"2.0","method":"runtime.ready","params":{"workspace_id":runtime.config.workspace_id,"vm_id":runtime.config.vm_id,"epoch":runtime.config.epoch,"module_catalog_version":runtime.modules.catalog.catalog_version,"module_catalog_hash":runtime.modules.catalog.content_hash,"dispatcher_contract_version":DISPATCHER_CONTRACT_VERSION,"pending_outcomes":runtime.modules.pending_outcome_count()}})).await;
        // Journaled module events: the journal is the source of truth, so a
        // lagged peer is told to `events.replay` rather than fed a partial stream.
        control_tasks.spawn(forward_module_events(
            module_events,
            runtime.clone(),
            tx.clone(),
            replay_gate.clone(),
            releases,
        ));
        control_tasks.spawn({
            let tx = tx.clone();
            let runtime = runtime.clone();
            let cancel = cancel.clone();
            let watches = watches.clone();
            async move {
                loop {
                    let mut params = match file_events.recv().await {
                        Ok(params) => params,
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "dispatcher file event relay lagged");
                            let rescans = { watches.lock().unwrap().rescan_events() };
                            for mut params in rescans {
                                params["vm_id"] = json!(runtime.config.vm_id);
                                if tx
                                    .send(json!({"jsonrpc":"2.0", "method":"file.changed", "params":params}))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    };
                    if !runtime.lease.lock().unwrap().valid() {
                        continue;
                    }
                    params["vm_id"] = json!(runtime.config.vm_id);
                    if tx
                        .send(json!({"jsonrpc":"2.0", "method":"file.changed", "params":params}))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                tracing::warn!(
                    "dispatcher native watcher stream ended; closing the rpc connection"
                );
                cancel.cancel();
            }
        });
        // Subscribe before scheduling the forwarder, so ready cannot race registration.
        control_tasks.spawn(forward_port_events(
            port_events,
            observed_ports,
            runtime.clone(),
            tx.clone(),
        ));
        control_tasks.spawn({let runtime=runtime.clone();let tx=tx.clone();async move {
            let mut tick=tokio::time::interval(Duration::from_millis(50));
            loop {
                tick.tick().await;
                let seq=runtime.lease.lock().unwrap().due();
                if let Some(seq)=seq {
                    if tx.send(json!({"jsonrpc":"2.0","id":format!("lease:{seq}"),"method":"lease.renew","params":{"workspace_id":runtime.config.workspace_id,"vm_id":runtime.config.vm_id,"epoch":runtime.config.epoch,"seq":seq}})).await.is_err(){break;}
                }
            }
        }});
    }
    let mut pending = Vec::new();
    loop {
        let line = tokio::select! {_=cancel.cancelled()=>break,line=read_line(&mut reader,&mut pending)=>match line {Some(line)=>line,None=>{
            tracing::warn!(pending = pending.len(), "dispatcher rpc stream ended (peer closed or oversized line)");
            break
        }}};
        let value: Value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(_) => {
                let _=tx.send(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"PARSE_ERROR"}})).await;
                continue;
            }
        };
        if value["jsonrpc"] == "2.0"
            && value.get("method").is_none()
            && (value.get("result").is_some() || value.get("error").is_some())
        {
            // A renewal refusal is a response, not a new malformed request.
            // The lease's normal retry/expiry policy remains authoritative.
            if value.get("error").is_some() {
                continue;
            }
            if let Some(id) = value["id"]
                .as_str()
                .and_then(|id| id.strip_prefix("lease:"))
                .and_then(|seq| seq.parse::<u64>().ok())
            {
                let result = &value["result"];
                if result["seq"].as_u64() == Some(id) {
                    if let (Some(token), Some(ttl)) =
                        (result["token"].as_str(), result["lease_ttl_secs"].as_u64())
                    {
                        if runtime
                            .lease
                            .lock()
                            .unwrap()
                            .install_with_publish(id, token.to_owned(), ttl, |token| {
                                runtime.modules.add_secret(token);
                                runtime.config.publish_workspace_token(token)?;
                                Ok(())
                            })
                            .is_err()
                        {
                            tracing::warn!("dispatcher rejected lease renewal reply");
                        }
                    }
                }
            }
            continue;
        }
        if value["jsonrpc"] != "2.0"
            || !value["method"].is_string()
            || !value.is_object()
            || value
                .get("id")
                .is_some_and(|id| !id.is_null() && !id.is_string() && !id.is_number())
        {
            let _=tx.send(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"INVALID_REQUEST"}})).await;
            continue;
        }
        // JSON-RPC notifications do not execute mutable operations without a
        // reply/idempotency outcome for the lifecycle caller to observe.
        let Some(id) = value.get("id").cloned() else {
            continue;
        };
        let permit = match requests.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                let _=tx.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":429,"message":"TOO_MANY_REQUESTS"}})).await;
                continue;
            }
        };
        let runtime = runtime.clone();
        let tx = tx.clone();
        let watches = watches.clone();
        let replay_gate = replay_gate.clone();
        request_tasks.spawn(async move {
            let _permit = permit;
            let method = value["method"].as_str().unwrap();
            let params = value.get("params").cloned().unwrap_or(json!({}));
            let remaining = params["context"]["deadline_ms"]
                .as_u64()
                .map(|deadline| deadline.saturating_sub(now_ms()).min(86_400_000))
                .unwrap_or(0);
            let mut after: Vec<(String, u64)> = Vec::new();
            let result = tokio::time::timeout(
                Duration::from_millis(remaining),
                dispatch(runtime, method, params, watches, &replay_gate, &mut after),
            )
            .await
            .unwrap_or_else(|_| Err(RpcError::new(408, "DEADLINE_EXCEEDED")));
            let response = match result {
                Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error.value()}),
            };
            let _ = tx.send(response).await;
            // Live events held back during a replay follow its response.
            // Live events held back during a replay follow its response — the
            // error response included: the release is signalled only once the
            // reply is queued, and the forwarder, the sole writer of live
            // events, drains the backlog one event at a time under the gate.
            for (exec_id, covered) in after {
                replay_gate.end(&exec_id, covered);
            }
        });
        while request_tasks.try_join_next().is_some() {}
    }
    // A one-request API stream is half-closed after its complete request.
    // Finish every admitted request before closing the response direction so
    // EOF cannot race and abort the only reply. The long-lived control
    // forwarders stop after their own request handlers have drained.
    while request_tasks.join_next().await.is_some() {}
    control_tasks.abort_all();
    while control_tasks.join_next().await.is_some() {}
    drop(tx);
    let _ = writer_task.await;
}

/// Per-connection ordering between `events.replay` and live delivery
/// (Agent API §4.3): from the moment a replay for an execution is admitted
/// until its reply has been queued on the connection writer, live events of
/// that execution are held. The release is performed by the one task that
/// writes live events — the forwarder — so a held event can never be
/// overtaken by a newer one, and a failed reply still releases the backlog.
/// Overlapping replays of one execution share the hold and release after
/// the last reply.
/// Live events held for one execution while its replays are in flight.
#[derive(Default)]
struct HeldEvents {
    /// Replays currently admitted for the execution.
    replays: usize,
    /// Highest sequence a replay reply on this connection already carried;
    /// a live event at or below it is a duplicate and is dropped — on
    /// release, or when it reaches the forwarder only after the release.
    /// Remembered for the connection's lifetime, not just while a backlog
    /// exists.
    covered: u64,
    queue: Vec<Value>,
}

struct ReplayGate {
    held: Mutex<HashMap<String, HeldEvents>>,
    release: mpsc::UnboundedSender<String>,
}

impl ReplayGate {
    fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<String>) {
        let (release, releases) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                held: Mutex::new(HashMap::new()),
                release,
            }),
            releases,
        )
    }

    fn begin(&self, exec_id: &str) {
        self.held
            .lock()
            .unwrap()
            .entry(exec_id.to_owned())
            .or_default()
            .replays += 1;
    }

    /// Release one replay whose reply covered events up to `covered`; the
    /// last release hands the backlog to the forwarder.
    fn end(&self, exec_id: &str, covered: u64) {
        let mut held = self.held.lock().unwrap();
        let Some(entry) = held.get_mut(exec_id) else {
            return;
        };
        entry.covered = entry.covered.max(covered);
        entry.replays = entry.replays.saturating_sub(1);
        if entry.replays == 0 {
            let _ = self.release.send(exec_id.to_owned());
        }
    }

    /// The forwarder enqueues one released event that no reply already
    /// carried, with the writer permit it reserved and while the gate is
    /// held, so a replay admitted in the meantime cannot slip its reply in
    /// front of the event; `false` when nothing is released (a newer replay
    /// re-took the hold, or the backlog is empty). An entry whose reply
    /// carried events stays: its covered sequence still fences late
    /// arrivals; one that never covered anything is removed.
    fn send_released(&self, exec_id: &str, permit: mpsc::Permit<'_, Value>) -> bool {
        let mut held = self.held.lock().unwrap();
        let Some(entry) = held.get_mut(exec_id) else {
            return false;
        };
        if entry.replays != 0 {
            return false;
        }
        while !entry.queue.is_empty() {
            let event = entry.queue.remove(0);
            if !Self::covered_by(entry, &event) {
                permit.send(event);
                return true;
            }
        }
        // Drained: the backlog's storage goes, and an entry with nothing to
        // fence (no reply ever carried an event) goes with it.
        entry.queue = Vec::new();
        if entry.covered == 0 {
            held.remove(exec_id);
        }
        false
    }

    /// Deliver a live event with the writer permit reserved for it: held
    /// behind a replay in flight or a backlog not yet released, dropped when
    /// a reply on this connection already carried it, else enqueued while
    /// the gate is held, so the decision and the queue position are one step.
    fn deliver(&self, notification: Value, permit: mpsc::Permit<'_, Value>) {
        let exec_id = notification["params"]["exec_id"]
            .as_str()
            .map(str::to_owned);
        let mut held = self.held.lock().unwrap();
        match exec_id.and_then(|exec_id| held.get_mut(&exec_id)) {
            Some(entry) if entry.replays != 0 || !entry.queue.is_empty() => {
                entry.queue.push(notification)
            }
            Some(entry) if Self::covered_by(entry, &notification) => {}
            _ => permit.send(notification),
        }
    }

    fn covered_by(entry: &HeldEvents, event: &Value) -> bool {
        event["params"]["seq"]
            .as_u64()
            .is_some_and(|seq| seq <= entry.covered)
    }
}

async fn forward_module_events(
    mut events: broadcast::Receiver<Value>,
    runtime: Arc<Runtime>,
    tx: mpsc::Sender<Value>,
    gate: Arc<ReplayGate>,
    mut releases: mpsc::UnboundedReceiver<String>,
) {
    loop {
        let notification = tokio::select! {
            received = events.recv() => match received {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    json!({"jsonrpc":"2.0","method":"module.events.lagged","params":{"skipped":skipped,"vm_id":runtime.config.vm_id}})
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            Some(exec_id) = releases.recv() => {
                // One event at a time under the gate: a replay admitted while
                // the backlog is still being written re-takes the hold, and
                // the remaining backlog waits for that replay's reply.
                // Writer capacity is reserved before the gate is consulted
                // and the event is enqueued while the gate is held, so no
                // event is ever detached from the hold — neither while a
                // send waits on backpressure nor between the gate's decision
                // and the queue.
                loop {
                    let Ok(permit) = tx.reserve().await else {
                        return;
                    };
                    if !gate.send_released(&exec_id, permit) {
                        break;
                    }
                }
                continue;
            }
        };
        let Ok(permit) = tx.reserve().await else {
            return;
        };
        gate.deliver(notification, permit);
    }
}

enum PortUpdate {
    Event(Value),
    Resync,
}

async fn next_port_update(events: &mut broadcast::Receiver<Value>) -> Option<PortUpdate> {
    match events.recv().await {
        Ok(event) => Some(PortUpdate::Event(event)),
        Err(broadcast::error::RecvError::Lagged(_)) => Some(PortUpdate::Resync),
        Err(broadcast::error::RecvError::Closed) => None,
    }
}

async fn forward_port_events(
    mut events: broadcast::Receiver<Value>,
    mut delivered: BTreeSet<u16>,
    runtime: Arc<Runtime>,
    tx: mpsc::Sender<Value>,
) {
    while let Some(update) = next_port_update(&mut events).await {
        if !runtime.lease.lock().unwrap().valid() {
            continue;
        }
        let notifications = match update {
            PortUpdate::Event(event) => {
                let port = event["params"]["port"].as_u64().unwrap() as u16;
                let changed = if event["method"] == "port.opened" {
                    delivered.insert(port)
                } else {
                    delivered.remove(&port)
                };
                if !changed {
                    continue;
                }
                vec![event]
            }
            PortUpdate::Resync => {
                // Recover the observable state after a slow peer overruns the
                // bounded broadcast queue; subsequent events keep flowing.
                let current: BTreeSet<_> = runtime.ports.read().await.iter().copied().collect();
                let mut notifications = Vec::new();
                for (method, ports) in [
                    ("port.closed", delivered.difference(&current)),
                    ("port.opened", current.difference(&delivered)),
                ] {
                    for port in ports {
                        notifications.push(json!({"jsonrpc":"2.0","method":method,"params":{"port":port,"vm_id":runtime.config.vm_id}}));
                    }
                }
                delivered = current;
                notifications
            }
        };
        for event in notifications {
            if tx.send(event).await.is_err() {
                return;
            }
        }
    }
}

async fn read_line(reader: &mut MuxReader, pending: &mut Vec<u8>) -> Option<Vec<u8>> {
    let mut scanned = 0;
    loop {
        if let Some(end) = pending[scanned..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|offset| offset + scanned)
        {
            let rest = pending.split_off(end + 1);
            let mut line = std::mem::replace(pending, rest);
            line.pop();
            return Some(line);
        }
        scanned = pending.len();
        let chunk = reader.recv().await?;
        if pending.len() + chunk.len() > MAX_RPC_BYTES {
            return None;
        }
        pending.extend_from_slice(&chunk);
    }
}
fn file_error(error: FileError) -> RpcError {
    RpcError {
        code: error.http_status() as i64,
        name: error.code,
        data: error.data,
    }
}

fn cached_mutation(runtime: &Runtime, key: &str, digest: &[u8]) -> Option<RpcResult> {
    let mut cache = runtime.mutations.lock().unwrap();
    cache.retain(|_, entry| entry.expires > Instant::now());
    cache.get(key).map(|entry| {
        if entry.digest == digest {
            entry.result.clone()
        } else {
            Err(RpcError::new(409, "IDEMPOTENCY_KEY_REUSED"))
        }
    })
}

async fn dispatch(
    runtime: Arc<Runtime>,
    method: &str,
    params: Value,
    watches: Arc<Mutex<NativeWatcher>>,
    replay_gate: &ReplayGate,
    after: &mut Vec<(String, u64)>,
) -> RpcResult {
    if !METHODS.contains(&method) {
        return Err(RpcError::new(-32601, "method_not_found"));
    }
    let context = Context::validate(&params, &runtime.config, method)?;
    if method == "health" {
        return Ok(health(&runtime));
    }
    if !runtime.lease.lock().unwrap().valid() {
        return Err(RpcError::new(409, "LEASE_EXPIRED"));
    }
    match method {
        "admission.close" => {
            let _lifecycle = runtime.lifecycle.lock().await;
            let operation_id = params.get("operation_id").and_then(Value::as_str);
            if operation_id.is_some_and(|value| value.is_empty() || value.len() > 128) {
                return Err(RpcError::new(-32602, "INVALID_PARAMS"));
            }
            let fork_mode = params["mode"].as_str() == Some("fork");
            if params.get("mode").is_some() && !fork_mode {
                return Err(RpcError::new(-32602, "INVALID_PARAMS"));
            }
            let interrupt = params["interrupt"].as_bool().unwrap_or(false);
            let prepare_only = params["prepare_only"].as_bool().unwrap_or(false);
            if (interrupt && !fork_mode) || (prepare_only && fork_mode) {
                return Err(RpcError::new(-32602, "INVALID_PARAMS"));
            }
            runtime.admission.refuse_new_for(operation_id);
            if prepare_only {
                return Ok(json!({"admission_open":false,"drained":false}));
            }
            let result = tokio::time::timeout(
                Duration::from_millis(context.deadline_ms.saturating_sub(now_ms())),
                async {
                    let reason = if fork_mode {
                        TerminalCloseReason::ForkInterrupt
                    } else {
                        TerminalCloseReason::WorkspaceStop
                    };
                    if !fork_mode || interrupt {
                        runtime.modules.cancel_running(if fork_mode {
                            "fork_interrupt"
                        } else {
                            "workspace_stop"
                        })?;
                        runtime
                            .terminals
                            .shutdown_with_reason(reason)
                            .await
                            .map_err(|error| {
                                tracing::error!(%error, "terminal lifecycle shutdown failed");
                                RpcError::new(500, "TERMINAL_SHUTDOWN_FAILED")
                            })?;
                        #[cfg(feature = "test-support")]
                        if std::env::var_os("G5D_DISPATCH_TEST_WAIT_FOR_SHUTDOWN").is_some() {
                            eprintln!("test terminal shutdown snapshot complete");
                            TEST_TERMINAL_SHUTDOWN_COMPLETE.store(true, Ordering::Release);
                        }
                    }
                    if fork_mode && !interrupt {
                        runtime.admission.close_for_fork(operation_id).await;
                    } else {
                        runtime
                            .admission
                            .close_for_reason(operation_id, reason)
                            .await;
                    }
                    Ok::<(), RpcError>(())
                },
            )
            .await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    if fork_mode {
                        runtime.admission.open_for(operation_id);
                    }
                    return Err(error);
                }
                Err(_) if fork_mode => {
                    runtime.admission.open_for(operation_id);
                    return Err(RpcError::new(409, "WORKSPACE_BUSY"));
                }
                Err(_) => return Err(RpcError::new(408, "DEADLINE_EXCEEDED")),
            }
            Ok(json!({"admission_open":false}))
        }
        "admission.open" => {
            let _lifecycle = runtime.lifecycle.lock().await;
            let operation_id = params.get("operation_id").and_then(Value::as_str);
            if operation_id.is_some_and(|value| value.is_empty() || value.len() > 128) {
                return Err(RpcError::new(-32602, "INVALID_PARAMS"));
            }
            if context.deadline_ms <= now_ms() {
                return Err(RpcError::new(408, "DEADLINE_EXCEEDED"));
            }
            if !runtime.lease.lock().unwrap().valid() {
                return Err(RpcError::new(409, "LEASE_EXPIRED"));
            }
            if !runtime.admission.open_for(operation_id) {
                return Err(RpcError::new(409, "STALE_LIFECYCLE_OPERATION"));
            }
            Ok(json!({"admission_open":true}))
        }
        "workspace.flush" => {
            tracing::warn!(workspace_id = %runtime.config.workspace_id, "workspace flush rpc waiting for lifecycle gate");
            let _lifecycle = runtime.lifecycle.clone().lock_owned().await;
            tracing::warn!(
                workspace_id = %runtime.config.workspace_id,
                "workspace flush rpc entered"
            );
            let reason = params["reason"]
                .as_str()
                .ok_or_else(|| RpcError::new(-32602, "INVALID_PARAMS"))?;
            let reason = match reason {
                "shutdown" => g5_internal_api::workspace_flush::FlushReason::Shutdown,
                "fork" => g5_internal_api::workspace_flush::FlushReason::Fork,
                _ => return Err(RpcError::new(-32602, "UNSUPPORTED_REASON")),
            };
            if runtime.admission.is_open()
                || runtime.admission.inflight() != 0
                || (reason == g5_internal_api::workspace_flush::FlushReason::Shutdown
                    && runtime.admission.active_streams() != 0)
            {
                return Err(RpcError::new(409, "WORKSPACE_BUSY"));
            }
            let flush = runtime
                .flush
                .get()
                .ok_or_else(|| RpcError::new(-32603, "FLUSH_RUNTIME_UNAVAILABLE"))?;
            serde_json::to_value(
                flush
                    .capture(reason, context.idempotency_key(), _lifecycle)
                    .await?,
            )
            .map_err(|_| RpcError::new(-32603, "INTERNAL_ERROR"))
        }
        "fs.write" | "fs.delete" | "fs.rename" => {
            let mut params = params;
            params.as_object_mut().unwrap().remove("context");
            let digest = Sha256::digest(serde_json::to_vec(&(method, &params)).unwrap()).to_vec();
            // Completed replays do not admit a new filesystem mutation, even
            // during a lifecycle barrier. New work still refuses immediately.
            if let Some(result) = cached_mutation(&runtime, &context.idempotency_key, &digest) {
                return result;
            }
            let permit = runtime.admission.enter()?;
            let method = method.to_owned();
            // The owned operation survives peer cancellation: an admitted write
            // must finish, retain its idempotency outcome and keep close waiting.
            tokio::spawn(async move {
                let _permit = permit;
                let _serial = runtime.mutation_serial.lock().await;
                if let Some(result) = cached_mutation(&runtime, &context.idempotency_key, &digest) {
                    return result;
                }
                {
                    let cache = runtime.mutations.lock().unwrap();
                    if cache.len() >= MAX_COMPLETED_MUTATIONS {
                        let retry_after_ms = cache.values()
                            .map(|entry| entry.expires.saturating_duration_since(Instant::now()).as_millis())
                            .min().unwrap_or(0) + 1;
                        return Err(RpcError { code:429, name:"TOO_MANY_REQUESTS", data:json!({"retry_after_ms":retry_after_ms,"limit":MAX_COMPLETED_MUTATIONS}) });
                    }
                }
                let files = runtime.files.clone();
                let deadline = context.deadline_ms;
                let (entered, result) = tokio::task::spawn_blocking(move || {
                    #[cfg(feature = "test-support")]
                    if let Ok(gate) = std::env::var("G5D_DISPATCH_TEST_MUTATION_GATE") {
                        if std::fs::write(format!("{gate}.entered"), b"admitted").is_err() {
                            return (false, Err(RpcError::new(-32603, "TEST_GATE_ERROR")));
                        }
                        let started = Instant::now();
                        while !std::path::Path::new(&format!("{gate}.release")).exists() {
                            if started.elapsed() > Duration::from_secs(10) {
                                return (false, Err(RpcError::new(408, "TEST_GATE_TIMEOUT")));
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    if deadline <= now_ms() {
                        return (false, Err(RpcError::new(408, "DEADLINE_EXCEEDED")));
                    }
                    (true, files.call(&method, &params).map_err(file_error))
                })
                .await
                .map_err(|_| RpcError::new(-32603, "INTERNAL_ERROR"))?;
                if entered {
                    runtime.mutations.lock().unwrap().insert(
                        context.idempotency_key,
                        CachedMutation {
                            digest,
                            expires: Instant::now() + MUTATION_RETENTION,
                            result: result.clone(),
                        },
                    );
                }
                result
            })
            .await
            .map_err(|_| RpcError::new(-32603, "INTERNAL_ERROR"))?
        }
        "fs.watch" => {
            let repo = params["repo_id"]
                .as_str()
                .ok_or_else(|| RpcError::new(-32602, "INVALID_PARAMS"))?
                .to_owned();
            let path = params["path"]
                .as_str()
                .ok_or_else(|| RpcError::new(-32602, "INVALID_PARAMS"))?
                .to_owned();
            let watched =
                tokio::task::spawn_blocking(move || watches.lock().unwrap().watch(&repo, &path))
                    .await
                    .map_err(|_| RpcError::new(-32603, "INTERNAL_ERROR"))?;
            match watched {
                Ok(()) => Ok(json!({"watching":true})),
                Err(error) if error.code == "WATCH_LIMIT_EXCEEDED" => {
                    Ok(json!({"watching":false,"rescan_required":true}))
                }
                Err(error) => Err(file_error(error)),
            }
        }
        "ports.list" => Ok(json!({"ports":runtime.ports.read().await.clone()})),
        "runtime.inspect.summary" => Ok(
            json!({"workspace_id":runtime.config.workspace_id,"vm_id":runtime.config.vm_id,"epoch":runtime.config.epoch,"dispatcher_contract_version":DISPATCHER_CONTRACT_VERSION,"module_catalog_version":runtime.modules.catalog.catalog_version,"module_catalog_hash":runtime.modules.catalog.content_hash,"dispatcher_version":env!("CARGO_PKG_VERSION"),"boot_time":runtime.boot_time,"uptime_ms":runtime.started.elapsed().as_millis(),"health":health(&runtime),"roots":runtime.config.layout.repos.iter().map(|repo|json!({"repo_id":repo.repo_id,"path":repo.root_path})).collect::<Vec<_>>(),"ports":runtime.ports.read().await.clone(),"resources":resource_observation(),"modules":{"running":runtime.modules.running_count(),"bounded_slots_in_use":runtime.modules.slots_in_use(),"bounded_execution_slots":runtime.modules.catalog.bounded_execution_slots,"pending_outcomes":runtime.modules.journal.pending_outcomes().len(),"unpersisted_outcomes":runtime.modules.unpersisted_outcomes()}}),
        ),
        "modules.list" => Ok(runtime.modules.list()),
        "modules.describe" => {
            let module_id = params["module_id"]
                .as_str()
                .ok_or_else(|| RpcError::new(-32602, "INVALID_PARAMS"))?;
            runtime
                .modules
                .describe(module_id, params["module_version"].as_str())
        }
        "modules.invoke" => {
            runtime
                .modules
                .invoke(&params, context.actor_user_id())
                .await
        }
        "modules.status" => runtime.modules.status(&params),
        "modules.cancel" => runtime.modules.cancel(&params),
        "modules.logs" => runtime.modules.logs(&params),
        "modules.events" | "events.replay" => {
            // Hold this execution's live events until the replay response
            // has been written; the handler releases them afterwards.
            // Only a canonical identity of this incarnation takes the hold:
            // anything else is refused by the replay itself and leaves no
            // trace in the gate.
            let vm_id = runtime.config.vm_id.to_string();
            let Some(exec_id) = params["exec_id"]
                .as_str()
                .filter(|id| ExecId::parse(id).is_some_and(|id| id.vm_id() == vm_id))
                .map(str::to_owned)
            else {
                return runtime.modules.replay(&params);
            };
            replay_gate.begin(&exec_id);
            let result = runtime.modules.replay(&params);
            // What this reply carries is exactly what the release may skip.
            let covered = result
                .as_ref()
                .ok()
                .and_then(|reply| reply["events"].as_array()?.last()?["seq"].as_u64())
                .unwrap_or(0);
            after.push((exec_id, covered));
            result
        }
        "modules.history" => runtime.modules.history(&params),
        "modules.input.respond" => {
            runtime
                .modules
                .respond(&params, &context.idempotency_key)
                .await
        }
        "modules.input.steer" => {
            runtime
                .modules
                .steer(&params, &context.idempotency_key)
                .await
        }
        "modules.background.end" => {
            runtime
                .modules
                .end_background_wait(&params, &context.idempotency_key)
                .await
        }
        "events.ack" => runtime.modules.ack(&params),
        "outcomes.pending" => runtime.modules.outcomes_pending(),
        "diagnostics.probe" => {
            let name = params["name"]
                .as_str()
                .ok_or_else(|| RpcError::new(-32602, "INVALID_PARAMS"))?
                .to_owned();
            if !builtin::PROBES.contains(&name.as_str()) {
                return Err(RpcError {
                    code: -32602,
                    name: "UNKNOWN_PROBE",
                    data: json!({"known":builtin::PROBES}),
                });
            }
            let cancel = CancellationToken::new();
            let probe = builtin::probe(&runtime.modules, &name, &cancel, None)
                .await
                .map_err(|_| RpcError::new(-32603, "INTERNAL_ERROR"))?;
            Ok(runtime.modules.secrets().redact_json(&probe))
        }
        "diagnostics.capture" => {
            let profile = params["profile"].as_str().unwrap_or("runtime").to_owned();
            let cancel = CancellationToken::new();
            builtin::capture(&runtime.modules, &profile, &cancel, None)
                .await
                .map_err(|_| RpcError::new(-32603, "INTERNAL_ERROR"))?
        }
        _ => {
            let files = runtime.files.clone();
            let method = method.to_owned();
            tokio::task::spawn_blocking(move || files.call(&method, &params).map_err(file_error))
                .await
                .map_err(|_| RpcError::new(-32603, "INTERNAL_ERROR"))?
        }
    }
}
fn health(runtime: &Runtime) -> Value {
    let lease = runtime.lease.lock().unwrap();
    json!({"vm_id":runtime.config.vm_id,"epoch":runtime.config.epoch,"admission_open":runtime.admission.is_open(),"lease_valid":lease.valid(),"lease_renewal_seq":lease.installed_seq,"inflight_mutations":runtime.admission.inflight(),"max_file_bytes":MAX_FILE_BYTES})
}

fn resource_observation() -> Value {
    #[cfg(target_os = "linux")]
    {
        let memory = std::fs::read_to_string("/proc/meminfo").ok();
        let available = memory.as_deref().and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("MemAvailable:")
                    .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
            })
        });
        json!({"memory_available_kib":available})
    }
    #[cfg(not(target_os = "linux"))]
    {
        json!({"memory_available_kib":null})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replay_gate_enqueues_under_its_lock_and_drops_covered_events() {
        let (gate, mut releases) = ReplayGate::new();
        let (tx, mut rx) = mpsc::channel::<Value>(4);
        let event = |seq: u64| json!({"jsonrpc":"2.0","method":"module.event","params":{"exec_id":"x","seq":seq}});
        // No replay in flight: enqueued with the permit at once.
        gate.deliver(event(1), tx.reserve().await.unwrap());
        assert_eq!(rx.recv().await.unwrap()["params"]["seq"], 1);
        // Mid-replay: held, the permit given back.
        gate.begin("x");
        gate.deliver(event(2), tx.reserve().await.unwrap());
        gate.deliver(event(3), tx.reserve().await.unwrap());
        assert!(rx.try_recv().is_err());
        // The reply carried seq 2: the release drops 2 and enqueues 3.
        gate.end("x", 2);
        assert_eq!(releases.recv().await.unwrap(), "x");
        assert!(gate.send_released("x", tx.reserve().await.unwrap()));
        assert_eq!(rx.recv().await.unwrap()["params"]["seq"], 3);
        assert!(!gate.send_released("x", tx.reserve().await.unwrap()));
        // A newer replay admitted before the release runs keeps the backlog
        // until its own reply is queued.
        gate.begin("x");
        gate.deliver(event(4), tx.reserve().await.unwrap());
        gate.end("x", 0);
        gate.begin("x");
        assert!(!gate.send_released("x", tx.reserve().await.unwrap()));
        assert!(rx.try_recv().is_err());
        gate.end("x", 0);
        assert!(gate.send_released("x", tx.reserve().await.unwrap()));
        assert_eq!(rx.recv().await.unwrap()["params"]["seq"], 4);
        assert!(!gate.send_released("x", tx.reserve().await.unwrap()));
        // An event a reply already carried that reaches the forwarder only
        // after the release (it was still in the broadcast receiver) is
        // dropped; a newer one goes through.
        gate.begin("x");
        gate.end("x", 6);
        assert_eq!(releases.recv().await.unwrap(), "x");
        assert!(!gate.send_released("x", tx.reserve().await.unwrap()));
        gate.deliver(event(5), tx.reserve().await.unwrap());
        gate.deliver(event(6), tx.reserve().await.unwrap());
        assert!(rx.try_recv().is_err());
        gate.deliver(event(7), tx.reserve().await.unwrap());
        assert_eq!(rx.recv().await.unwrap()["params"]["seq"], 7);
        // A live event arriving between the release signal and the release
        // itself waits behind the backlog.
        gate.begin("x");
        gate.deliver(event(8), tx.reserve().await.unwrap());
        gate.end("x", 7);
        gate.deliver(event(9), tx.reserve().await.unwrap());
        assert!(rx.try_recv().is_err());
        assert!(gate.send_released("x", tx.reserve().await.unwrap()));
        assert_eq!(rx.recv().await.unwrap()["params"]["seq"], 8);
        assert!(gate.send_released("x", tx.reserve().await.unwrap()));
        assert_eq!(rx.recv().await.unwrap()["params"]["seq"], 9);
        assert!(!gate.send_released("x", tx.reserve().await.unwrap()));
        assert!(gate.held.lock().unwrap().contains_key("x"));
        // An identity whose replays never carried an event leaves no entry
        // behind once its backlog is drained.
        gate.begin("y");
        gate.end("y", 0);
        assert!(!gate.send_released("y", tx.reserve().await.unwrap()));
        assert!(!gate.held.lock().unwrap().contains_key("y"));
        gate.deliver(
            json!({"jsonrpc":"2.0","method":"module.event","params":{"exec_id":"y","seq":1}}),
            tx.reserve().await.unwrap(),
        );
        assert_eq!(rx.recv().await.unwrap()["params"]["seq"], 1);
    }

    #[tokio::test]
    async fn transient_accept_error_backs_off_without_terminating_listener() {
        let started = Instant::now();
        let socket = tokio::time::timeout(
            Duration::from_secs(1),
            accepted_socket(Err(std::io::Error::from(
                std::io::ErrorKind::ConnectionAborted,
            ))),
        )
        .await
        .expect("accept error handling must remain bounded");

        assert!(socket.is_none());
        assert!(started.elapsed() >= ACCEPT_ERROR_BACKOFF);
    }

    #[tokio::test]
    async fn lagged_port_receiver_resyncs_and_continues() {
        let (sender, mut receiver) = broadcast::channel(2);
        for port in [10001, 10002, 10003] {
            sender.send(json!({"port":port})).unwrap();
        }
        assert!(matches!(
            next_port_update(&mut receiver).await,
            Some(PortUpdate::Resync)
        ));
        assert!(matches!(next_port_update(&mut receiver).await,
            Some(PortUpdate::Event(event)) if event["port"] == 10002));
        assert!(matches!(next_port_update(&mut receiver).await,
            Some(PortUpdate::Event(event)) if event["port"] == 10003));
        sender.send(json!({"port":10004})).unwrap();
        assert!(matches!(next_port_update(&mut receiver).await,
            Some(PortUpdate::Event(event)) if event["port"] == 10004));
        drop(sender);
        assert!(next_port_update(&mut receiver).await.is_none());
    }
}
