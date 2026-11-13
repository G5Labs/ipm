//! In-memory per-host workspace reverse-tunnel registry.
//!
//! The worker-infrastructure design (`designs/WORKER_INFRASTRUCTURE.md` §1.4.2,
//! locked Option A) keeps worker hosts **outbound-only**: there is no inbound
//! ingress to a host. The host dials out a persistent WebSocket tunnel to
//! `GET /api/internal/hosts/:hostId/tunnel`. g5-api accepts that upgrade, builds
//! a [`Mux`] over it (g5-api is the **initiator** — it opens logical streams; the
//! host accepts and bridges each to the selected workspace VM), and registers the
//! live mux here keyed by `worker_hosts.host_id`.
//!
//! Browser-facing preview and terminal routes resolve the current workspace
//! incarnation, open an HTTP or PTY stream through this tunnel, and relay bytes
//! without exposing the host or dispatcher credential.
//!
//! The registry is process-local (one entry per connected host). A host
//! reconnect replaces the stale entry; a disconnect removes it (the proxy route
//! then fails closed until the host re-dials).
//!
//! ## Deployment constraint (sticky routing)
//!
//! Because the registry lives in the address space of the g5-api process that
//! accepted the host's outbound tunnel, the browser's
//! `GET /api/v1/preview/:taskId/*path` request **must reach the same g5-api
//! instance** that holds that host's tunnel, or [`TunnelRegistry::get`] returns
//! `None` and the proxy fails closed with a 503. Two supported topologies in
//! V1:
//!
//! * **Single g5-api instance** (the demo/default deployment) — trivially
//!   satisfied; every request and every tunnel land on the one process.
//! * **Multiple instances behind a load balancer** — the balancer must route
//!   the host tunnel attach and the matching `/api/v1/preview/` requests to the
//!   same backend (e.g. consistent hashing on `host_id`, or session affinity).
//!
//! A cross-instance shared tunnel fabric (routing a browser request to whichever
//! instance happens to hold the tunnel) is intentionally out of scope for V1 and
//! tracked as future work; the proxy's fail-closed 503 makes a routing
//! mismatch a clean unavailability rather than a silent cross-tenant hazard.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::Notify;
use uuid::Uuid;

use g5_contracts::error::G5Error;
use g5_internal_api::stream_mux::{Channel, Mux, MuxHandle, MuxStream};

use super::host_ctl::{
    serve_control_stream, ControlPlaneHostState, ControlSession, HostCommandClient, HostConnection,
};

/// Byte capacity of the in-process duplex that bridges the WebSocket to the
/// [`Mux`]. The mux applies its own per-logical-stream credit flow control, so
/// this bridge buffer only smooths WS-message ↔ byte-stream framing; one full
/// per-stream window is ample.
const TUNNEL_BRIDGE_BUF: usize = 256 * 1024;

/// Chunk size for draining the duplex into outbound WS binary frames.
const WS_READ_CHUNK: usize = 64 * 1024;

/// One live host→api reverse tunnel: a [`Mux`] over the host's outbound
/// WebSocket. g5-api opens incarnation-routed RPC, HTTP and PTY streams on it.
pub struct HostTunnel {
    host_id: Uuid,
    /// The mux is shared so concurrent browser connections can each open their
    /// own preview stream. `Mux::open` takes `&self`, so `Arc` is sufficient —
    /// no outer lock. Dropping the last `Arc` tears the mux (and its reader/
    /// writer tasks) down.
    mux: MuxHandle,
    active: Arc<HostConnection>,
    terminated: AtomicBool,
    terminated_notify: Notify,
    commands: RwLock<Option<HostCommandClient>>,
}

impl HostTunnel {
    /// The `worker_hosts.host_id` this tunnel serves.
    #[must_use]
    pub fn host_id(&self) -> Uuid {
        self.host_id
    }

    /// Build a tunnel from an already-constructed mux. Used by [`TunnelRegistry::attach`]
    /// and by unit tests that drive a mux pair without a real WebSocket.
    fn from_mux(host_id: Uuid, mux: MuxHandle) -> Self {
        Self {
            host_id,
            mux,
            active: Arc::new(HostConnection::new(true)),
            terminated: AtomicBool::new(false),
            terminated_notify: Notify::new(),
            commands: RwLock::new(None),
        }
    }

    #[must_use]
    pub fn command_client(&self) -> Option<HostCommandClient> {
        self.commands
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn set_command_client(&self, client: HostCommandClient) {
        *self
            .commands
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(client);
    }

    fn deactivate_locked(&self, guard: &tokio::sync::OwnedMutexGuard<()>) {
        self.active.deactivate_locked(guard);
        *self
            .commands
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    fn terminate_locked(&self, guard: &tokio::sync::OwnedMutexGuard<()>) {
        self.deactivate_locked(guard);
        self.terminated.store(true, Ordering::Release);
        self.terminated_notify.notify_waiters();
    }

    /// Open a raw JSON-RPC relay to one live workspace dispatcher. The host
    /// resolves the VM UUID against its custody map and adds the private
    /// listener credential; callers never receive that credential or a host
    /// address.
    pub(crate) async fn open_workspace_rpc(&self, vm_id: Uuid) -> Result<MuxStream, G5Error> {
        self.open_workspace_stream(vm_id, Channel::Rpc).await
    }

    /// Open an incarnation-routed data stream to a workspace dispatcher. The
    /// host consumes the VM UUID and relays the remaining bytes on the same
    /// channel, so HTTP and PTY admission stay bound to host custody.
    pub(crate) async fn open_workspace_stream(
        &self,
        vm_id: Uuid,
        channel: Channel,
    ) -> Result<MuxStream, G5Error> {
        if !matches!(channel, Channel::Rpc | Channel::Http | Channel::Pty) {
            return Err(G5Error::Internal {
                message: "invalid workspace dispatcher stream channel".to_string(),
            });
        }
        if !self.active.is_active() {
            return Err(G5Error::ServiceUnavailable {
                message: "worker host tunnel is not active".to_string(),
            });
        }
        let stream = self.mux.open(channel).await?;
        stream.send(vm_id.as_bytes()).await?;
        Ok(stream)
    }
}

/// Process-local registry of live host reverse tunnels, keyed by host_id.
pub struct TunnelRegistry {
    tunnels: DashMap<Uuid, Arc<HostTunnel>>,
    control_backend: RwLock<Option<Arc<dyn ControlPlaneHostState>>>,
}

impl TunnelRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tunnels: DashMap::new(),
            control_backend: RwLock::new(None),
        }
    }

    pub fn set_control_backend(&self, backend: Arc<dyn ControlPlaneHostState>) {
        *self
            .control_backend
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(backend);
    }

    /// Look up the live tunnel for a host, if one is currently connected.
    #[must_use]
    pub fn get(&self, host_id: &Uuid) -> Option<Arc<HostTunnel>> {
        self.tunnels.get(host_id).map(|e| e.value().clone())
    }

    /// Scheduling hint only: RPC admission still rechecks the connection.
    /// Exclude deactivated entries even before their registry cleanup completes.
    pub(crate) fn reachable_host_ids(&self) -> Vec<Uuid> {
        self.tunnels
            .iter()
            .filter(|entry| entry.value().active.is_active())
            .map(|entry| *entry.key())
            .collect()
    }

    /// Placement/status hint only; command dispatch still checks admission.
    /// An RPC tunnel without its control client cannot start or inspect a VM.
    pub(crate) fn reachable_control_host_ids(&self) -> Vec<Uuid> {
        self.tunnels
            .iter()
            .filter(|entry| {
                entry.value().active.is_active() && entry.value().command_client().is_some()
            })
            .map(|entry| *entry.key())
            .collect()
    }

    /// Close the currently attached tunnel for a terminated host.
    pub async fn terminate(&self, host_id: &Uuid) -> bool {
        let Some(tunnel) = self.get(host_id) else {
            return false;
        };
        let guard = tunnel.active.publication_guard().await;
        tunnel.terminate_locked(&guard);
        true
    }

    /// Number of live host tunnels (observability/tests).
    #[must_use]
    pub fn len(&self) -> usize {
        self.tunnels.len()
    }

    /// Whether no host tunnels are connected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tunnels.is_empty()
    }

    /// Accept a host's outbound tunnel WebSocket: build the mux, register it, and
    /// run the WS↔mux byte bridge until either side closes. On return the entry
    /// is removed (unless a newer reconnect already replaced it).
    ///
    /// g5-api is the mux **initiator** (`Mux::new(.., true)`): it allocates even
    /// stream ids and opens control-plane streams; the host accepts them.
    pub async fn attach(self: &Arc<Self>, host_id: Uuid, socket: WebSocket) {
        let (mux_io, ws_io) = tokio::io::duplex(TUNNEL_BRIDGE_BUF);
        let mux = Mux::new(mux_io, true);
        let bridge = tokio::spawn(bridge_ws_to_duplex(socket, ws_io));
        self.serve_mux(host_id, mux).await;
        bridge.abort();
    }

    /// Attach an in-process mux peer to the same registry path used by a host
    /// WebSocket. Integration tests use this to exercise runtime RPC admission
    /// and framing without replacing the production runtime facade.
    #[cfg(feature = "test-helpers")]
    pub async fn attach_test_mux(self: &Arc<Self>, host_id: Uuid, mux: Mux) {
        self.serve_mux(host_id, mux).await;
    }

    async fn serve_mux(self: &Arc<Self>, host_id: Uuid, mut mux: Mux) {
        let tunnel = Arc::new(HostTunnel::from_mux(host_id, mux.handle()));

        // A reconnect replaces any stale tunnel for this host.
        loop {
            let previous = self.get(&host_id);
            let guard = match &previous {
                Some(previous) => Some(previous.active.publication_guard().await),
                None => None,
            };
            match self.tunnels.entry(host_id) {
                dashmap::mapref::entry::Entry::Vacant(entry) => {
                    entry.insert(tunnel.clone());
                    break;
                }
                dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                    if previous
                        .as_ref()
                        .is_some_and(|previous| Arc::ptr_eq(previous, entry.get()))
                    {
                        previous
                            .as_ref()
                            .unwrap()
                            .terminate_locked(guard.as_ref().unwrap());
                        entry.insert(tunnel.clone());
                        break;
                    }
                }
            }
        }
        tracing::info!(%host_id, live = self.tunnels.len(), "Preview reverse tunnel attached");

        let mut control_task = None;
        loop {
            if tunnel.terminated.load(Ordering::Acquire) {
                break;
            }
            let stream = if let Some(task) = control_task.as_mut() {
                tokio::select! {
                    _ = tunnel.terminated_notify.notified() => break,
                    stream = mux.accept() => stream,
                    result = task => {
                        match result {
                            Ok(Ok(())) => tracing::info!(%host_id, "Host ctl session closed"),
                            Ok(Err(error)) => tracing::warn!(%host_id, %error, "Host ctl session ended with error"),
                            Err(error) => tracing::warn!(%host_id, %error, "Host ctl session task failed"),
                        }
                        break;
                    }
                }
            } else {
                tokio::select! {
                    _ = tunnel.terminated_notify.notified() => break,
                    stream = mux.accept() => stream,
                }
            };
            let Some(mut stream) = stream else {
                break;
            };
            if stream.channel() != Channel::Ctl || control_task.is_some() {
                tracing::warn!(%host_id, channel = ?stream.channel(), "Host tunnel refused unexpected host-opened stream");
                stream.close().await;
                continue;
            }
            let backend = self
                .control_backend
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            let Some(backend) = backend else {
                tracing::warn!(%host_id, "Host ctl stream refused because no state backend is installed");
                stream.close().await;
                continue;
            };
            // The command client is published only once the host's hello is
            // accepted and its inventory reconciled: a Start enqueued earlier
            // could be released by that hello and still reach the host
            // (`g5.runtime.start.settlement`).
            let session = ControlSession::publishing_on_accept({
                let tunnel = tunnel.clone();
                move |client| tunnel.set_command_client(client)
            });
            let active = tunnel.active.clone();
            control_task = Some(tokio::spawn(async move {
                serve_control_stream(host_id, stream, backend, session, active).await
            }));
        }
        if let Some(task) = control_task {
            task.abort();
        }
        let guard = tunnel.active.publication_guard().await;
        tunnel.deactivate_locked(&guard);

        // Only remove if we are still the current tunnel — a newer reconnect that
        // replaced us must keep its entry.
        self.tunnels
            .remove_if(&host_id, |_, current| Arc::ptr_eq(current, &tunnel));
        tracing::info!(%host_id, live = self.tunnels.len(), "Preview reverse tunnel detached");
    }
}

impl Default for TunnelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Pump bytes between an axum [`WebSocket`] (binary frames) and a byte-oriented
/// [`DuplexStream`] (the mux's transport end). The mux frames ride inside WS
/// binary messages; ping/pong/text are ignored (axum answers pings itself).
async fn bridge_ws_to_duplex(socket: WebSocket, duplex: DuplexStream) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (mut duplex_rd, mut duplex_wr) = tokio::io::split(duplex);

    // duplex → WS: drain mux output bytes, frame them as WS binary messages.
    let to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; WS_READ_CHUNK];
        loop {
            match duplex_rd.read(&mut buf).await {
                Ok(0) => break, // mux torn down
                Ok(n) => {
                    if ws_tx
                        .send(Message::Binary(buf[..n].to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = ws_tx.send(Message::Close(None)).await;
    });

    // WS → duplex: feed inbound binary frames into the mux transport.
    let from_ws = tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if duplex_wr.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                // Text/Ping/Pong carry no tunnel payload.
                Ok(_) => {}
            }
        }
        // Dropping `duplex_wr` EOFs the mux reader, tearing every live stream down.
    });

    let _ = tokio::join!(to_ws, from_ws);
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use g5_internal_api::host_ctl::{
        CurrentIncarnation, HeartbeatAck, HostAccept, HostCtlConfig, HostDrained, HostHeartbeat,
        HostHello, LeaseRenew, LeaseRenewResult, StaleLease, VmEvent, VmEventAck, VmEventKind,
    };
    use g5_internal_api::host_ctl_test_support::{ScriptedHostFaults, ScriptedHostPeer};
    use g5_internal_api::stream_mux::Channel;
    use std::collections::HashSet;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use tokio::time::timeout;

    #[derive(Default)]
    struct DeduplicatingBackend {
        events: Mutex<HashSet<(Uuid, u64)>>,
    }

    #[async_trait]
    impl ControlPlaneHostState for DeduplicatingBackend {
        async fn accept_hello(
            &self,
            _host_id: Uuid,
            _hello: &HostHello,
            _connection_active: &HostConnection,
        ) -> Result<HostAccept, super::super::host_ctl::HostCtlError> {
            Ok(HostAccept {
                accepted: true,
                protocol_version: _hello.protocol_version,
                incarnation_generation: 1,
                current_incarnations: Vec::<CurrentIncarnation>::new(),
                config: HostCtlConfig::default(),
            })
        }

        async fn heartbeat(
            &self,
            _host_id: Uuid,
            _heartbeat: &HostHeartbeat,
        ) -> Result<HeartbeatAck, super::super::host_ctl::HostCtlError> {
            Ok(HeartbeatAck {
                source_preparation: None,
                incarnation_generation: 1,
                current_incarnations: Vec::new(),
            })
        }

        async fn ingest_vm_event(
            &self,
            _host_id: Uuid,
            event: &VmEvent,
        ) -> Result<VmEventAck, super::super::host_ctl::HostCtlError> {
            self.events.lock().await.insert((event.vm_id, event.seq));
            Ok(VmEventAck {
                vm_id: event.vm_id,
                through_seq: event.seq,
            })
        }

        async fn file_changed(
            &self,
            _host_id: Uuid,
            _event: &g5_internal_api::host_ctl::VmFileChanged,
        ) -> Result<(), super::super::host_ctl::HostCtlError> {
            Ok(())
        }

        async fn renew_lease(
            &self,
            _host_id: Uuid,
            _renewal: &LeaseRenew,
        ) -> Result<LeaseRenewResult, super::super::host_ctl::HostCtlError> {
            Ok(LeaseRenewResult::Stale(StaleLease { stale: true }))
        }

        async fn host_drained(
            &self,
            _host_id: Uuid,
            _drained: &HostDrained,
        ) -> Result<(), super::super::host_ctl::HostCtlError> {
            Ok(())
        }
    }

    /// Construct a registry tunnel directly over an in-process mux pair, standing
    /// in for the WebSocket transport, and return the peer mux that plays the
    /// host (it accepts the API-opened workspace streams).
    fn tunnel_pair(host_id: Uuid) -> (Arc<HostTunnel>, Mux, Mux) {
        let (a, b) = tokio::io::duplex(256 * 1024);
        let api_mux = Mux::new(a, true); // api = initiator
        let host_mux = Mux::new(b, false); // host accepts
        (
            Arc::new(HostTunnel::from_mux(host_id, api_mux.handle())),
            api_mux,
            host_mux,
        )
    }

    #[tokio::test]
    async fn open_workspace_http_stream_sends_vm_header_first() {
        let host_id = Uuid::new_v4();
        let (tunnel, _api, mut host) = tunnel_pair(host_id);
        let vm_id = Uuid::new_v4();

        let _client = tunnel
            .open_workspace_stream(vm_id, Channel::Http)
            .await
            .expect("open");

        let mut accepted = timeout(Duration::from_secs(2), host.accept())
            .await
            .expect("host accepts the stream")
            .expect("an HTTP stream");
        assert_eq!(accepted.channel(), Channel::Http);

        // The first frame is exactly the target incarnation UUID.
        let header = timeout(Duration::from_secs(2), accepted.recv())
            .await
            .expect("VM header arrives")
            .expect("UUID bytes");
        assert_eq!(header, vm_id.as_bytes());
    }

    #[tokio::test]
    async fn workspace_http_bytes_relay_after_header() {
        let host_id = Uuid::new_v4();
        let (tunnel, _api, mut host) = tunnel_pair(host_id);
        let vm_id = Uuid::new_v4();

        let client = tunnel
            .open_workspace_stream(vm_id, Channel::Http)
            .await
            .expect("open");
        client
            .send(b"GET / HTTP/1.1\r\n\r\n")
            .await
            .expect("send req");

        let mut accepted = host.accept().await.expect("stream");
        let header = accepted.recv().await.expect("VM header");
        assert_eq!(header, vm_id.as_bytes());
        let req = timeout(Duration::from_secs(2), accepted.recv())
            .await
            .expect("request bytes")
            .expect("bytes");
        assert_eq!(req, b"GET / HTTP/1.1\r\n\r\n");

        // Host responds; the api side observes it.
        accepted
            .send(b"HTTP/1.1 200 OK\r\n\r\nhi")
            .await
            .expect("resp");
        let mut client = client;
        let resp = timeout(Duration::from_secs(2), client.recv())
            .await
            .expect("response bytes")
            .expect("bytes");
        assert_eq!(resp, b"HTTP/1.1 200 OK\r\n\r\nhi");
    }

    #[tokio::test]
    async fn registry_get_absent_is_none() {
        let reg = TunnelRegistry::new();
        assert!(reg.is_empty());
        assert!(reg.get(&Uuid::new_v4()).is_none());
        assert!(!reg.terminate(&Uuid::new_v4()).await);
    }

    #[tokio::test]
    async fn registry_termination_closes_the_owned_mux() {
        let registry = Arc::new(TunnelRegistry::new());
        let host_id = Uuid::new_v4();
        let (api_io, host_io) = tokio::io::duplex(64 * 1024);
        let api_mux = Mux::new(api_io, true);
        let mut host_mux = Mux::new(host_io, false);
        let task = tokio::spawn({
            let registry = registry.clone();
            async move { registry.serve_mux(host_id, api_mux).await }
        });
        timeout(Duration::from_secs(2), async {
            while registry.get(&host_id).is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(registry.terminate(&host_id).await);
        timeout(Duration::from_secs(2), task)
            .await
            .expect("terminated tunnel exits")
            .unwrap();
        assert!(registry.get(&host_id).is_none());
        assert!(timeout(Duration::from_secs(6), host_mux.accept())
            .await
            .expect("peer sees mux closure")
            .is_none());
    }

    #[tokio::test]
    async fn registry_insert_get_and_reconnect_replaces() {
        let reg = Arc::new(TunnelRegistry::new());
        let host_id = Uuid::new_v4();

        let (t1, _a1, _h1) = tunnel_pair(host_id);
        reg.tunnels.insert(host_id, t1.clone());
        assert_eq!(reg.len(), 1);
        assert!(Arc::ptr_eq(&reg.get(&host_id).unwrap(), &t1));

        // Reconnect: a new tunnel replaces the stale one for the same host_id.
        let (t2, _a2, _h2) = tunnel_pair(host_id);
        reg.tunnels.insert(host_id, t2.clone());
        assert_eq!(reg.len(), 1);
        assert!(Arc::ptr_eq(&reg.get(&host_id).unwrap(), &t2));

        // A stale-tunnel detach must NOT remove the current (replaced) entry.
        reg.tunnels
            .remove_if(&host_id, |_, current| Arc::ptr_eq(current, &t1));
        assert_eq!(
            reg.len(),
            1,
            "stale detach must not evict the live reconnect"
        );
        assert!(Arc::ptr_eq(&reg.get(&host_id).unwrap(), &t2));
    }

    #[tokio::test]
    async fn scripted_host_replays_unacked_event_through_registry_tunnel() {
        let host_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let peer = ScriptedHostPeer::new(host_id, 2);
        let event = peer
            .record_event(workspace_id, vm_id, VmEventKind::Booted)
            .await;
        let backend = Arc::new(DeduplicatingBackend::default());
        let registry = Arc::new(TunnelRegistry::new());
        registry.set_control_backend(backend.clone());

        let (api_io, host_io) = tokio::io::duplex(256 * 1024);
        let api_mux = Mux::new(api_io, true);
        let registry_task = tokio::spawn({
            let registry = registry.clone();
            async move { registry.serve_mux(host_id, api_mux).await }
        });
        let first_peer = tokio::spawn({
            let peer = peer.clone();
            async move {
                let host_mux = Mux::new(host_io, false);
                let stream = host_mux.open(Channel::Ctl).await.unwrap();
                peer.run_control_stream(
                    stream,
                    ScriptedHostFaults {
                        disconnect_after_first_event: true,
                        ..ScriptedHostFaults::default()
                    },
                )
                .await
                .unwrap();
            }
        });
        first_peer.await.unwrap();
        timeout(Duration::from_secs(2), registry_task)
            .await
            .expect("first tunnel detaches")
            .unwrap();
        assert_eq!(peer.pending_events().await, vec![event.clone()]);

        let (api_io, host_io) = tokio::io::duplex(256 * 1024);
        let api_mux = Mux::new(api_io, true);
        let second_registry = tokio::spawn({
            let registry = registry.clone();
            async move { registry.serve_mux(host_id, api_mux).await }
        });
        let second_peer = tokio::spawn({
            let peer = peer.clone();
            async move {
                let host_mux = Mux::new(host_io, false);
                let stream = host_mux.open(Channel::Ctl).await.unwrap();
                peer.run_control_stream(
                    stream,
                    ScriptedHostFaults {
                        duplicate_replayed_events: true,
                        ..ScriptedHostFaults::default()
                    },
                )
                .await
            }
        });
        timeout(Duration::from_secs(2), async {
            loop {
                if peer.pending_events().await.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("replayed event acknowledged");
        assert_eq!(peer.acknowledged_through(vm_id).await, event.seq);
        assert_eq!(backend.events.lock().await.len(), 1);
        second_peer.abort();
        second_registry.abort();
    }
}
