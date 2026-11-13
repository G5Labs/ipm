//! Authenticated workspace host tunnel.
//!
//! Workspace hosts are outbound-only: there is no inbound ingress. The host
//! dials a persistent WebSocket to g5-api, authenticates with its host JWT, and
//! keeps it open. The multiplexed connection carries host control plus
//! incarnation-routed RPC, HTTP and PTY streams.
//!
//! The carrier reuses the channel-tagged, credit-flow-controlled framing of
//! [`g5_internal_api::stream_mux`] verbatim inside WebSocket binary frames.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::{watch, Mutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};
use uuid::Uuid;

use g5_contracts::error::G5Error;
use g5_internal_api::stream_mux::{Channel, Mux, MuxReader, MuxStream, MuxWriter};

use crate::ctl::{
    handle_dispatcher_control_message, serve_control_stream, EventJournal, HostCommandProcessor,
    HostControlState, LeaseRenewalBroker, SessionOutcome,
};

/// Byte capacity of the in-process duplex bridging the WebSocket to the mux.
const TUNNEL_BRIDGE_BUF: usize = 256 * 1024;
/// Chunk size for draining the duplex/TCP into outbound frames.
const RELAY_CHUNK: usize = 64 * 1024;
/// Reconnect backoff bounds.
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// WS keepalive cadence: the host pings g5-api on this interval (g5-api's
/// axum layer auto-answers pongs), so an idle-but-healthy carrier always has
/// inbound traffic and LB/NAT flow state stays warm.
const TUNNEL_PING_INTERVAL: Duration = Duration::from_secs(15);
/// Receive deadline (3× the ping cadence, tolerating transient stalls): if no
/// WS message arrives for this long the connection is a half-open corpse — an
/// api pod replacement behind the LB can die without a FIN/RST ever reaching
/// the host — and the carrier must tear down so the reconnect loop takes over.
const TUNNEL_LIVENESS_TIMEOUT: Duration = Duration::from_secs(45);

/// Configuration for the workspace host tunnel carrier.
#[derive(Debug, Clone)]
pub struct HostTunnelConfig {
    /// g5-api base URL (e.g. `https://api.g5.dev` or `http://localhost:8080`).
    pub api_url: String,
    /// This host's `worker_hosts.host_id` (the tunnel-attach path segment; must
    /// match the per-host JWT's subject g5-api validates).
    pub host_id: Uuid,
    /// Live per-host JWT (`Scope::WorkerHost`) presented as `Authorization:
    /// Bearer`. External-host heartbeat rotation updates this shared handle;
    /// each reconnect reads the latest value.
    pub host_jwt: Arc<RwLock<String>>,
}

/// The persistent host→api workspace tunnel carrier.
pub struct HostTunnelCarrier {
    config: HostTunnelConfig,
    control: Option<ControlSessionConfig>,
}

/// Dependencies whose state must survive WebSocket reconnects.
#[derive(Clone)]
pub struct ControlSessionConfig {
    pub state: Arc<dyn HostControlState>,
    pub journal: Arc<EventJournal>,
    pub commands: Arc<HostCommandProcessor>,
    pub workspace_runtime: Arc<crate::workspace_vm::WorkspaceVmRuntime>,
    pub lease_renewals: Arc<LeaseRenewalBroker>,
}

impl HostTunnelCarrier {
    #[must_use]
    pub fn new(config: HostTunnelConfig) -> Self {
        Self {
            config,
            control: None,
        }
    }

    /// Attach the host control session and workspace stream router.
    #[must_use]
    pub fn with_control(mut self, control: ControlSessionConfig) -> Self {
        self.control = Some(control);
        self
    }

    /// The `ws(s)://…/api/internal/hosts/<host_id>/tunnel` URL derived from the
    /// `http(s)` api base.
    fn tunnel_url(&self) -> String {
        let base = self.config.api_url.trim_end_matches('/');
        let ws_base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            // Already a ws(s) scheme or schemeless — use as-is.
            base.to_string()
        };
        format!(
            "{ws_base}/api/internal/hosts/{}/tunnel",
            self.config.host_id
        )
    }

    /// Run the carrier until `shutdown` flips true: dial, serve, and reconnect
    /// with exponential backoff on drop/error. Never returns while the host is
    /// up (spawned as a detached background task alongside the supervisor).
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut backoff = BACKOFF_MIN;
        info!(
            host_id = %self.config.host_id,
            url = %self.tunnel_url(),
            "Workspace host tunnel carrier starting"
        );
        loop {
            if *shutdown.borrow() {
                break;
            }
            let outcome = tokio::select! {
                _ = shutdown.changed() => { if *shutdown.borrow() { break; } continue; }
                outcome = self.connect_and_serve() => outcome,
            };
            match outcome {
                Ok(()) => {
                    debug!(host_id = %self.config.host_id, "Workspace host tunnel closed; reconnecting");
                    backoff = BACKOFF_MIN;
                }
                Err(e) => {
                    warn!(host_id = %self.config.host_id, error = %e, "Workspace host tunnel error; will retry");
                }
            }
            // Interruptible backoff before reconnecting.
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = shutdown.changed() => { if *shutdown.borrow() { break; } }
            }
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
        info!(host_id = %self.config.host_id, "Workspace host tunnel carrier stopped");
    }

    /// Dial the tunnel, build the mux over it, and serve accepted workspace
    /// streams until the connection drops.
    async fn connect_and_serve(&self) -> Result<(), G5Error> {
        let url = self.tunnel_url();
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|e| G5Error::Internal {
                message: format!("workspace host tunnel: bad url {url}: {e}"),
            })?;
        let host_jwt = self
            .config
            .host_jwt
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let bearer = format!("Bearer {host_jwt}");
        request.headers_mut().insert(
            AUTHORIZATION,
            bearer.parse().map_err(|e| G5Error::Internal {
                message: format!("workspace host tunnel: bad authorization header: {e}"),
            })?,
        );

        let (ws, _resp) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("workspace host tunnel: dial failed: {e}"),
            })?;
        info!(host_id = %self.config.host_id, "Workspace host tunnel connected");

        // Bridge the WebSocket to the byte duplex consumed by the mux.
        let (mux_io, ws_io) = tokio::io::duplex(TUNNEL_BRIDGE_BUF);
        let mut mux = Mux::new(mux_io, false);
        let bridge = tokio::spawn(bridge_ws_to_duplex(ws, ws_io, self.config.host_id));

        if let Some(control) = self.control.clone() {
            let ctl_stream = mux.open(Channel::Ctl).await?;
            let commands = control.commands.clone();
            let mut ctl = tokio::spawn(serve_control_stream(
                ctl_stream,
                control.state,
                control.journal.clone(),
                control.commands,
                control.lease_renewals.clone(),
            ));
            loop {
                tokio::select! {
                    stream = mux.accept() => {
                        let Some(mut stream) = stream else { break; };
                        match stream.channel() {
                            channel @ (Channel::Rpc | Channel::Http | Channel::Pty) => {
                                let runtime = control.workspace_runtime.clone();
                                let renewals = control.lease_renewals.clone();
                                let journal = control.journal.clone();
                                tokio::spawn(async move {
                                    serve_workspace_stream(
                                        stream, runtime, channel, renewals, journal,
                                    )
                                    .await;
                                });
                            }
                            _ => {
                                warn!(channel = ?stream.channel(), "Host tunnel refused unexpected control-plane stream");
                                stream.close().await;
                            }
                        }
                    }
                    result = &mut ctl => {
                        match result {
                            Ok(Ok(SessionOutcome::Disconnected)) => break,
                            Ok(Ok(SessionOutcome::Rejected(reject))) => {
                                // A protocol reject is permanent for this host
                                // binary/configuration until an operator fixes
                                // it. Keep existing VMs alive, refuse new starts,
                                // and retain the reconnect loop for recovery.
                                commands.self_drain().await;
                                bridge.abort();
                                return Err(G5Error::Internal {
                                    message: format!("host ctl rejected: {:?}", reject.reason),
                                });
                            }
                            Ok(Err(error)) => {
                                bridge.abort();
                                return Err(G5Error::Internal {
                                    message: format!("host ctl failed: {error:#}"),
                                });
                            }
                            Err(error) => {
                                bridge.abort();
                                return Err(G5Error::Internal {
                                    message: format!("host ctl task failed: {error}"),
                                });
                            }
                        }
                    }
                }
            }
            ctl.abort();
        } else {
            while let Some(mut stream) = mux.accept().await {
                warn!(channel = ?stream.channel(), "Host tunnel refused stream without a control session");
                stream.close().await;
            }
        }

        // Mux torn down (WS dropped). Stop the bridge and reconnect.
        bridge.abort();
        Ok(())
    }
}

/// Route one control-plane RPC relay to the exact live workspace VM. The
/// outer stream begins with the 16 raw UUID bytes; everything after that is
/// line-delimited dispatcher JSON-RPC. The host authenticates a fresh inner
/// dispatcher connection and opens its private RPC mux stream.
async fn serve_workspace_stream(
    stream: MuxStream,
    runtime: Arc<crate::workspace_vm::WorkspaceVmRuntime>,
    channel: Channel,
    renewals: Arc<LeaseRenewalBroker>,
    journal: Arc<EventJournal>,
) {
    let (outer_writer, mut outer_reader) = stream.into_split();
    let Ok(Some((vm_id, leftover))) =
        tokio::time::timeout(Duration::from_secs(10), read_vm_header(&mut outer_reader)).await
    else {
        outer_writer.close().await;
        return;
    };
    let Some(endpoint) = runtime.endpoint(vm_id).await else {
        outer_writer.close().await;
        return;
    };
    let Ok((inner, _stream_guard)) = endpoint.open(channel).await else {
        outer_writer.close().await;
        return;
    };
    let (inner_writer, inner_reader) = inner.into_split();
    if channel == Channel::Rpc {
        let control = DispatcherControlContext {
            renewals,
            journal,
            workspace_id: endpoint.workspace_id,
            vm_id,
            epoch: endpoint.epoch,
        };
        bridge_rpc_streams(
            outer_reader,
            outer_writer,
            inner_reader,
            inner_writer,
            leftover,
            control,
        )
        .await;
    } else {
        bridge_mux_streams(
            outer_reader,
            outer_writer,
            inner_reader,
            inner_writer,
            leftover,
        )
        .await;
    }
}

/// The incarnation a relayed dispatcher RPC stream is bound to; dispatcher
/// control frames on it are pinned to this tuple, never to what they claim.
struct DispatcherControlContext {
    renewals: Arc<LeaseRenewalBroker>,
    journal: Arc<EventJournal>,
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
}

async fn bridge_rpc_streams(
    mut outer_reader: MuxReader,
    outer_writer: MuxWriter,
    mut inner_reader: MuxReader,
    inner_writer: MuxWriter,
    leftover: Vec<u8>,
    control: DispatcherControlContext,
) {
    let inner_writer = Arc::new(Mutex::new(inner_writer));
    let request_writer = inner_writer.clone();
    let to_inner = tokio::spawn(async move {
        if !leftover.is_empty() && request_writer.lock().await.send(&leftover).await.is_err() {
            return;
        }
        while let Some(chunk) = outer_reader.recv().await {
            if request_writer.lock().await.send(&chunk).await.is_err() {
                return;
            }
        }
        request_writer.lock().await.close().await;
    });
    let control_writer = inner_writer;
    let to_outer = tokio::spawn(async move {
        while let Some(line) = inner_reader.read_line_with_limit(4 * 1024 * 1024).await {
            if handle_dispatcher_control_message(
                &line,
                &control_writer,
                &control.renewals,
                &control.journal,
                control.workspace_id,
                control.vm_id,
                control.epoch,
            )
            .await
            {
                continue;
            }
            let mut encoded = line.into_bytes();
            encoded.push(b'\n');
            if outer_writer.send(&encoded).await.is_err() {
                return;
            }
        }
        outer_writer.close().await;
    });
    let _ = tokio::join!(to_inner, to_outer);
}

async fn read_vm_header(reader: &mut MuxReader) -> Option<(Uuid, Vec<u8>)> {
    let mut bytes = Vec::with_capacity(16);
    while bytes.len() < 16 {
        bytes.extend_from_slice(&reader.recv().await?);
    }
    let vm_id = Uuid::from_slice(&bytes[..16]).ok()?;
    Some((vm_id, bytes[16..].to_vec()))
}

async fn bridge_mux_streams(
    mut outer_reader: MuxReader,
    outer_writer: MuxWriter,
    mut inner_reader: MuxReader,
    inner_writer: MuxWriter,
    leftover: Vec<u8>,
) {
    let to_inner = tokio::spawn(async move {
        if !leftover.is_empty() && inner_writer.send(&leftover).await.is_err() {
            return;
        }
        while let Some(chunk) = outer_reader.recv().await {
            if inner_writer.send(&chunk).await.is_err() {
                return;
            }
        }
        inner_writer.close().await;
    });
    let to_outer = tokio::spawn(async move {
        while let Some(chunk) = inner_reader.recv().await {
            if outer_writer.send(&chunk).await.is_err() {
                return;
            }
        }
        outer_writer.close().await;
    });
    let _ = tokio::join!(to_inner, to_outer);
}

async fn bridge_ws_to_duplex<S>(ws: S, duplex: DuplexStream, host_id: Uuid)
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + SinkExt<Message>
        + Unpin
        + Send
        + 'static,
{
    let (ws_tx, mut ws_rx) = ws.split();
    let ws_tx = Arc::new(tokio::sync::Mutex::new(ws_tx));
    let (mut duplex_rd, mut duplex_wr) = tokio::io::split(duplex);
    let last_rx = Arc::new(std::sync::Mutex::new(tokio::time::Instant::now()));

    let relay_tx = ws_tx.clone();
    let mut to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; RELAY_CHUNK];
        loop {
            match duplex_rd.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if relay_tx
                        .lock()
                        .await
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
        let _ = relay_tx.lock().await.send(Message::Close(None)).await;
    });

    let rx_seen = last_rx.clone();
    let mut from_ws = tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            *rx_seen.lock().expect("last_rx lock") = tokio::time::Instant::now();
            match msg {
                Ok(Message::Binary(data)) => {
                    if duplex_wr.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                // Text/Ping/Pong carry no tunnel payload (tungstenite answers
                // pings); they still count as liveness above.
                Ok(_) => {}
            }
        }
    });

    let ping_tx = ws_tx.clone();
    let mut watchdog = tokio::spawn(async move {
        loop {
            tokio::time::sleep(TUNNEL_PING_INTERVAL).await;
            let idle = last_rx.lock().expect("last_rx lock").elapsed();
            if idle >= TUNNEL_LIVENESS_TIMEOUT {
                return;
            }
            // Best-effort, time-bounded ping: a blocked send (dead TCP with a
            // full kernel buffer) must not keep the deadline check from running.
            let tx = ping_tx.clone();
            let _ = tokio::time::timeout(TUNNEL_PING_INTERVAL, async move {
                let _ = tx.lock().await.send(Message::Ping(Vec::new())).await;
            })
            .await;
        }
    });

    // First terminator wins: peer close/error (from_ws), local mux teardown
    // (to_ws), or liveness expiry (watchdog). Abort the rest so both duplex
    // halves and the WS drop together.
    tokio::select! {
        _ = &mut from_ws => {}
        _ = &mut to_ws => {}
        _ = &mut watchdog => {
            warn!(
                %host_id,
                timeout_secs = TUNNEL_LIVENESS_TIMEOUT.as_secs(),
                "Workspace host tunnel liveness timeout; tearing down carrier"
            );
        }
    }
    from_ws.abort();
    to_ws.abort();
    watchdog.abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctl::{STALE_INCARNATION_CODE, STALE_INCARNATION_MESSAGE};
    use g5_internal_api::host_ctl::{LeaseGrant, LeaseRenew, LeaseRenewResult};
    use serde_json::{json, Value};

    /// `g5.runtime.lease` ([G5Labs/G5-Lean#524]): the control-plane relay
    /// pins a dispatcher's `lease.renew` to the incarnation its stream is
    /// bound to. A sibling's tuple is refused on the dispatcher side without
    /// touching the host ctl bridge, the bound tuple is relayed and answered,
    /// and neither the refusal nor the grant leaks onto the API's
    /// method-reply stream.
    #[tokio::test]
    async fn relay_binds_lease_renewals_to_the_streams_incarnation() {
        let temp = tempfile::tempdir().unwrap();
        let journal = Arc::new(
            EventJournal::open(temp.path().join("journal.json"))
                .await
                .unwrap(),
        );
        let renewals = Arc::new(LeaseRenewalBroker::default());
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let epoch = 2;

        let (api_io, outer_host_io) = tokio::io::duplex(64 * 1024);
        let api = Mux::new(api_io, true);
        let mut outer_host = Mux::new(outer_host_io, false);
        let (inner_host_io, dispatcher_io) = tokio::io::duplex(64 * 1024);
        let inner_host = Mux::new(inner_host_io, true);
        let mut dispatcher = Mux::new(dispatcher_io, false);
        let api_stream = api.open(Channel::Rpc).await.expect("open API stream");
        let outer_stream = outer_host.accept().await.expect("accept outer stream");
        let inner_stream = inner_host
            .open(Channel::Rpc)
            .await
            .expect("open dispatcher stream");
        let dispatcher_stream = dispatcher.accept().await.expect("accept dispatcher stream");
        let (outer_writer, outer_reader) = outer_stream.into_split();
        let (inner_writer, inner_reader) = inner_stream.into_split();
        let relay = tokio::spawn(bridge_rpc_streams(
            outer_reader,
            outer_writer,
            inner_reader,
            inner_writer,
            Vec::new(),
            DispatcherControlContext {
                renewals: renewals.clone(),
                journal,
                workspace_id,
                vm_id,
                epoch,
            },
        ));
        let (api_writer, mut api_reader) = api_stream.into_split();
        let (dispatcher_writer, mut dispatcher_reader) = dispatcher_stream.into_split();

        // A co-resident sibling's live tuple, authored by this guest.
        let forged = json!({
            "jsonrpc": "2.0",
            "id": "lease:9",
            "method": "lease.renew",
            "params": {
                "workspace_id": Uuid::new_v4(),
                "vm_id": Uuid::new_v4(),
                "epoch": epoch,
                "seq": 9,
            },
        });
        dispatcher_writer
            .send(format!("{forged}\n").as_bytes())
            .await
            .expect("send forged renewal");
        let refusal: Value =
            serde_json::from_str(&dispatcher_reader.read_line().await.unwrap()).unwrap();
        assert_eq!(refusal["id"], "lease:9");
        assert_eq!(refusal["error"]["code"], STALE_INCARNATION_CODE);
        assert_eq!(refusal["error"]["message"], STALE_INCARNATION_MESSAGE);

        // The bound tuple is relayed, granted and answered to the dispatcher.
        let bound = LeaseRenew {
            workspace_id,
            vm_id,
            epoch,
            seq: 10,
        };
        let control_plane = tokio::spawn({
            let renewals = renewals.clone();
            let bound = bound.clone();
            async move {
                let pending = renewals
                    .next()
                    .await
                    .expect("the bound renewal reaches the bridge");
                assert_eq!(pending.renewal, bound, "no forged renewal was queued");
                pending
                    .response
                    .send(Ok(LeaseRenewResult::Granted(LeaseGrant {
                        stale: false,
                        seq: 10,
                        token: "renewed-token".into(),
                        lease_ttl_secs: 300,
                        incarnation_generation: 1,
                    })))
                    .unwrap();
            }
        });
        let request = json!({
            "jsonrpc": "2.0",
            "id": "lease:10",
            "method": "lease.renew",
            "params": bound,
        });
        dispatcher_writer
            .send(format!("{request}\n").as_bytes())
            .await
            .expect("send bound renewal");
        control_plane.await.unwrap();
        let grant: Value =
            serde_json::from_str(&dispatcher_reader.read_line().await.unwrap()).unwrap();
        assert_eq!(grant["id"], "lease:10");
        assert_eq!(grant["result"]["seq"], 10);
        assert_eq!(grant["result"]["token"], "renewed-token");

        // Ordinary API traffic still crosses the relay in both directions,
        // and the API saw neither the refusal nor the grant.
        api_writer
            .send(b"{\"jsonrpc\":\"2.0\",\"id\":\"api-1\",\"method\":\"health\",\"params\":{}}\n")
            .await
            .expect("relay API request");
        let forwarded: Value =
            serde_json::from_str(&dispatcher_reader.read_line().await.unwrap()).unwrap();
        assert_eq!(forwarded["id"], "api-1");
        dispatcher_writer
            .send(b"{\"jsonrpc\":\"2.0\",\"id\":\"api-1\",\"result\":{\"ok\":true}}\n")
            .await
            .expect("relay API reply");
        let reply: Value = serde_json::from_str(&api_reader.read_line().await.unwrap()).unwrap();
        assert_eq!(reply["id"], "api-1");
        assert_eq!(reply["result"]["ok"], true);
        api_writer.close().await;
        dispatcher_writer.close().await;
        assert!(api_reader.read_line().await.is_none());
        assert!(dispatcher_reader.read_line().await.is_none());
        relay.await.expect("relay task must not panic");
    }

    #[tokio::test]
    async fn relay_releases_http_streams_after_request_eof() {
        let (api_io, outer_host_io) = tokio::io::duplex(512 * 1024);
        let api = Mux::new(api_io, true);
        let mut outer_host = Mux::new(outer_host_io, false);
        let (inner_host_io, dispatcher_io) = tokio::io::duplex(512 * 1024);
        let inner_host = Mux::new(inner_host_io, true);
        let mut dispatcher = Mux::new(dispatcher_io, false);

        for index in 0..264_u32 {
            let api_stream = api.open(Channel::Http).await.expect("open API stream");
            let outer_stream = outer_host.accept().await.expect("accept outer stream");
            let inner_stream = inner_host
                .open(Channel::Http)
                .await
                .expect("open dispatcher stream");
            let dispatcher_stream = dispatcher.accept().await.expect("accept dispatcher stream");
            let (outer_writer, outer_reader) = outer_stream.into_split();
            let (inner_writer, inner_reader) = inner_stream.into_split();
            let relay = tokio::spawn(bridge_mux_streams(
                outer_reader,
                outer_writer,
                inner_reader,
                inner_writer,
                Vec::new(),
            ));
            let (api_writer, mut api_reader) = api_stream.into_split();
            let (dispatcher_writer, mut dispatcher_reader) = dispatcher_stream.into_split();
            let request = format!("request-{index}");
            api_writer
                .send(request.as_bytes())
                .await
                .expect("relay request");
            api_writer.close().await;
            assert_eq!(
                dispatcher_reader.recv().await.as_deref(),
                Some(request.as_bytes())
            );
            assert!(dispatcher_reader.recv().await.is_none());
            let response = format!("response-{index}");
            dispatcher_writer
                .send(response.as_bytes())
                .await
                .expect("relay response");
            dispatcher_writer.close().await;
            assert_eq!(
                api_reader.recv().await.as_deref(),
                Some(response.as_bytes())
            );
            assert!(api_reader.recv().await.is_none());
            tokio::time::timeout(Duration::from_secs(2), relay)
                .await
                .expect("relay must finish")
                .expect("relay task must not panic");
        }
    }

    #[test]
    fn tunnel_url_maps_http_to_ws_scheme() {
        let carrier = HostTunnelCarrier::new(HostTunnelConfig {
            api_url: "https://api.g5.dev/".to_string(),
            host_id: Uuid::nil(),
            host_jwt: Arc::new(RwLock::new("jwt".to_string())),
        });
        assert_eq!(
            carrier.tunnel_url(),
            format!("wss://api.g5.dev/api/internal/hosts/{}/tunnel", Uuid::nil())
        );

        let carrier = HostTunnelCarrier::new(HostTunnelConfig {
            api_url: "http://localhost:8080".to_string(),
            host_id: Uuid::nil(),
            host_jwt: Arc::new(RwLock::new("jwt".to_string())),
        });
        assert!(carrier
            .tunnel_url()
            .starts_with("ws://localhost:8080/api/internal/hosts/"));
    }
}
