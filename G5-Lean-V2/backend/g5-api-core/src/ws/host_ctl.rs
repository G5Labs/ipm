//! Control-plane side of the worker-host `ctl` stream.
//!
//! The transport loop is independent of database layout. Its backend trait is
//! implemented by the P4.1 state owner: hello and heartbeat read current
//! incarnations, VM-event ingestion durably inserts before acknowledgement,
//! and lease renewal updates the current incarnation. Keeping those operations
//! behind one trait lets the scripted host peer exercise the real codec and
//! tunnel without creating a second state machine.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use g5_internal_api::host_ctl::{
    method, CommandParams, CommandResult, HeartbeatAck, HelloDisposition, HostAccept, HostDrained,
    HostHeartbeat, HostHello, HostReject, HostRejectReason, LeaseRenew, LeaseRenewResult,
    RpcMessage, RpcNotification, RpcRequest, RpcResponse, VmEvent, VmEventAck, VmFileChanged,
    MAX_CTL_MESSAGE_BYTES, MAX_INFLIGHT_COMMANDS, SUPPORTED_PROTOCOL_VERSIONS,
};
use g5_internal_api::stream_mux::{MuxReader, MuxStream, MuxWriter};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum HostCtlError {
    #[error("host ctl stream closed")]
    Closed,
    #[error("host ctl request timed out")]
    Timeout,
    #[error("host ctl protocol error: {0}")]
    Protocol(String),
    #[error("host ctl backend error: {0}")]
    Backend(String),
}

/// Serializes sizing publication through commit with authenticated tunnel
/// takeover. The registry changes ownership only while holding this guard.
pub struct HostConnection {
    active: AtomicBool,
    publication: Arc<tokio::sync::Mutex<()>>,
}

impl HostConnection {
    pub fn new(active: bool) -> Self {
        Self {
            active: AtomicBool::new(active),
            publication: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub(crate) async fn publication_guard(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.publication.clone().lock_owned().await
    }

    pub(crate) fn deactivate_locked(&self, _guard: &tokio::sync::OwnedMutexGuard<()>) {
        self.active.store(false, Ordering::Release);
    }

    pub async fn deactivate(&self) {
        let guard = self.publication_guard().await;
        self.deactivate_locked(&guard);
    }
}

#[async_trait]
pub trait ControlPlaneHostState: Send + Sync {
    async fn accept_hello(
        &self,
        host_id: Uuid,
        hello: &HostHello,
        connection_active: &HostConnection,
    ) -> Result<HostAccept, HostCtlError>;

    async fn heartbeat(
        &self,
        host_id: Uuid,
        heartbeat: &HostHeartbeat,
    ) -> Result<HeartbeatAck, HostCtlError>;

    async fn ingest_vm_event(
        &self,
        host_id: Uuid,
        event: &VmEvent,
    ) -> Result<VmEventAck, HostCtlError>;

    async fn file_changed(&self, host_id: Uuid, event: &VmFileChanged) -> Result<(), HostCtlError>;

    /// A host's content-free hint that executions journaled new module
    /// events (`g5.runtime.module-event-hints`). Advisory: a backend that
    /// does not stream live attempts ignores it.
    async fn module_events(
        &self,
        _host_id: Uuid,
        _hint: &g5_internal_api::host_ctl::VmModuleEvents,
    ) -> Result<(), HostCtlError> {
        Ok(())
    }

    async fn renew_lease(
        &self,
        host_id: Uuid,
        renewal: &LeaseRenew,
    ) -> Result<LeaseRenewResult, HostCtlError>;

    async fn host_drained(&self, host_id: Uuid, drained: &HostDrained) -> Result<(), HostCtlError>;
}

struct OutboundCommand {
    request: RpcRequest,
    response: oneshot::Sender<Result<RpcResponse, HostCtlError>>,
}

/// Cloneable command surface for the currently authenticated tunnel.
#[derive(Clone)]
pub struct HostCommandClient {
    tx: mpsc::Sender<OutboundCommand>,
}

/// A command enqueued on the connection whose reply is still outstanding.
/// The command is ordered before every command enqueued on the same
/// connection afterwards: the control stream writes requests in enqueue order
/// and the host reads them in that order.
pub(crate) struct PendingReply {
    response_rx: oneshot::Receiver<Result<RpcResponse, HostCtlError>>,
}

impl PendingReply {
    /// Wait for the host's reply until `deadline`. A timeout leaves the
    /// outcome unknown; the command stays enqueued or in flight.
    pub(crate) async fn wait(
        self,
        deadline: tokio::time::Instant,
    ) -> Result<CommandResult, HostCtlError> {
        let response = tokio::time::timeout_at(deadline, self.response_rx)
            .await
            .map_err(|_| HostCtlError::Timeout)?
            .map_err(|_| HostCtlError::Closed)??;
        if let Some(error) = response.error {
            return Err(HostCtlError::Protocol(format!(
                "remote error {}: {}",
                error.code, error.message
            )));
        }
        response
            .result_as()
            .map_err(|error| HostCtlError::Protocol(error.to_string()))
    }
}

impl HostCommandClient {
    /// Enqueue one command without waiting for its reply. On `Ok` the command
    /// is on the connection's outbound queue ahead of anything enqueued later;
    /// on `Err` nothing was enqueued. The lifecycle owner stamps a Start as
    /// sent and enqueues it inside one row-locked transaction, so a release
    /// that reads the stamp knows the host sees the Start before any command
    /// it sends afterwards (`g5.runtime.start.settlement`).
    pub(crate) async fn dispatch<T: Serialize>(
        &self,
        method: &str,
        params: &CommandParams<T>,
        enqueue_wait: Duration,
    ) -> Result<PendingReply, HostCtlError> {
        self.dispatch_until(method, params, tokio::time::Instant::now() + enqueue_wait)
            .await
    }

    async fn dispatch_until<T: Serialize>(
        &self,
        method: &str,
        params: &CommandParams<T>,
        deadline: tokio::time::Instant,
    ) -> Result<PendingReply, HostCtlError> {
        params.validate().map_err(|error| {
            HostCtlError::Protocol(format!("invalid command parameters: {error}"))
        })?;
        let request = RpcRequest::new(format!("command:{}", Uuid::new_v4()), method, params)
            .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
        let (response_tx, response_rx) = oneshot::channel();
        tokio::time::timeout_at(
            deadline,
            self.tx.send(OutboundCommand {
                request,
                response: response_tx,
            }),
        )
        .await
        .map_err(|_| HostCtlError::Timeout)?
        .map_err(|_| HostCtlError::Closed)?;
        Ok(PendingReply { response_rx })
    }

    pub(crate) async fn execute<T: Serialize>(
        &self,
        method: &str,
        params: &CommandParams<T>,
        wait: Duration,
    ) -> Result<CommandResult, HostCtlError> {
        let deadline = tokio::time::Instant::now() + wait;
        self.dispatch_until(method, params, deadline)
            .await?
            .wait(deadline)
            .await
    }

    /// Retry an ambiguous timeout with the same method, payload, and
    /// idempotency key. A definite host result is returned immediately.
    pub(crate) async fn execute_with_retry<T: Serialize>(
        &self,
        method: &str,
        params: &CommandParams<T>,
        wait: Duration,
        attempts: usize,
    ) -> Result<CommandResult, HostCtlError> {
        let mut last = HostCtlError::Timeout;
        for _ in 0..attempts.max(1) {
            match self.execute(method, params, wait).await {
                Err(HostCtlError::Timeout) => last = HostCtlError::Timeout,
                outcome => return outcome,
            }
        }
        Err(last)
    }

    /// Test support: send one command the way the placement and lifecycle
    /// owners in this crate send it.
    #[cfg(feature = "test-helpers")]
    pub async fn execute_for_test<T: Serialize>(
        &self,
        method: &str,
        params: &CommandParams<T>,
        wait: Duration,
    ) -> Result<CommandResult, HostCtlError> {
        self.execute(method, params, wait).await
    }
}

pub struct ControlSession {
    pub client: HostCommandClient,
    rx: mpsc::Receiver<OutboundCommand>,
    /// Publishes the client once the host's hello is accepted and its
    /// inventory reconciled, so no command can be enqueued on a connection
    /// whose hello snapshot predates it (`g5.runtime.start.settlement`).
    on_accept: Option<Box<dyn FnOnce(HostCommandClient) + Send>>,
}

impl ControlSession {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(MAX_INFLIGHT_COMMANDS);
        Self {
            client: HostCommandClient { tx },
            rx,
            on_accept: None,
        }
    }

    /// A session that hands its client to `publish` after hello acceptance.
    #[must_use]
    pub fn publishing_on_accept(publish: impl FnOnce(HostCommandClient) + Send + 'static) -> Self {
        let mut session = Self::new();
        session.on_accept = Some(Box::new(publish));
        session
    }
}

impl Default for ControlSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Serve one host-opened control stream. `connection_active` is cleared by the
/// tunnel registry when a newer authenticated connection replaces this one;
/// stale messages are then ignored before reaching the state owner.
pub async fn serve_control_stream(
    host_id: Uuid,
    stream: MuxStream,
    backend: Arc<dyn ControlPlaneHostState>,
    mut commands: ControlSession,
    connection_active: Arc<HostConnection>,
) -> Result<(), HostCtlError> {
    let (writer, mut reader) = stream.into_split();
    let hello_request = read_request(&mut reader).await?;
    if hello_request.method != method::HOST_HELLO {
        return Err(HostCtlError::Protocol(
            "first ctl message must be host.hello".to_string(),
        ));
    }
    let hello = hello_request
        .params::<HostHello>()
        .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
    if hello.host_id != host_id {
        send_hello_reject(
            &writer,
            hello_request.id,
            HostRejectReason::IdentityMismatch,
        )
        .await?;
        return Ok(());
    }
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&hello.protocol_version) {
        send_hello_reject(
            &writer,
            hello_request.id,
            HostRejectReason::UnsupportedVersion,
        )
        .await?;
        return Ok(());
    }
    if !process_host_admitted(
        &hello.capabilities,
        g5_contracts::env::is_env_flag_enabled(g5_internal_api::host_ctl::ALLOW_PROCESS_HOSTS_ENV),
    ) {
        tracing::warn!(
            %host_id,
            "refusing a process-isolation host: this control plane was not started for local development ({}=true)",
            g5_internal_api::host_ctl::ALLOW_PROCESS_HOSTS_ENV
        );
        send_hello_reject(
            &writer,
            hello_request.id,
            HostRejectReason::IsolationNotAdmitted,
        )
        .await?;
        return Ok(());
    }
    if !connection_active.is_active() {
        return Ok(());
    }
    let accepted = backend
        .accept_hello(host_id, &hello, &connection_active)
        .await?;
    if !accepted.accepted || accepted.protocol_version != hello.protocol_version {
        return Err(HostCtlError::Backend(
            "backend returned an invalid host.accept".to_string(),
        ));
    }
    send_response(
        &writer,
        &RpcResponse::result(hello_request.id, &HelloDisposition::Accept(accepted))
            .map_err(|error| HostCtlError::Protocol(error.to_string()))?,
    )
    .await?;
    // The hello's inventory is reconciled and committed: from here on, a
    // `starting` row this connection does not list is one whose Start the
    // host could not have observed, so commands may now be enqueued. Publish
    // under the publication guard and only while still active, so a
    // `terminate` that deactivated this tunnel during `accept_hello` cannot be
    // followed by this callback re-installing a client on the dead tunnel (a
    // Start could then be stamped sent that the host never received).
    if let Some(publish) = commands.on_accept.take() {
        let guard = connection_active.publication_guard().await;
        if !connection_active.is_active() {
            return Ok(());
        }
        publish(commands.client.clone());
        drop(guard);
    }

    let mut pending = HashMap::<String, oneshot::Sender<Result<RpcResponse, HostCtlError>>>::new();
    let mut reap_abandoned = tokio::time::interval(Duration::from_millis(100));
    reap_abandoned.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            line = reader.read_line_with_limit(MAX_CTL_MESSAGE_BYTES) => {
                let Some(line) = line else { return Ok(()); };
                if !connection_active.is_active() {
                    return Ok(());
                }
                let message: RpcMessage = serde_json::from_str(&line)
                    .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
                match message {
                    RpcMessage::Request(request) => {
                        handle_host_request(host_id, &writer, backend.as_ref(), request).await?;
                    }
                    RpcMessage::Notification(notification) => {
                        handle_host_notification(host_id, &writer, backend.as_ref(), notification).await?;
                    }
                    RpcMessage::Response(response) => {
                        if let Some(waiter) = pending.remove(&response.id) {
                            let _ = waiter.send(Ok(response));
                        }
                    }
                }
            }
            command = commands.rx.recv(), if pending.len() < MAX_INFLIGHT_COMMANDS => {
                let Some(command) = command else { return Ok(()); };
                if !connection_active.is_active() {
                    let _ = command.response.send(Err(HostCtlError::Closed));
                    return Ok(());
                }
                let id = command.request.id.clone();
                if pending.insert(id.clone(), command.response).is_some() {
                    return Err(HostCtlError::Protocol(format!("duplicate request id {id}")));
                }
                if let Err(error) = send_request(&writer, &command.request).await {
                    if let Some(waiter) = pending.remove(&id) {
                        let _ = waiter.send(Err(HostCtlError::Closed));
                    }
                    return Err(error);
                }
            }
            _ = reap_abandoned.tick() => {
                pending.retain(|_, waiter| !waiter.is_closed());
            }
        }
    }
}

async fn handle_host_request(
    host_id: Uuid,
    writer: &MuxWriter,
    backend: &dyn ControlPlaneHostState,
    request: RpcRequest,
) -> Result<(), HostCtlError> {
    let response = match request.method.as_str() {
        method::HOST_HEARTBEAT => {
            let heartbeat = request
                .params::<HostHeartbeat>()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            let ack = backend.heartbeat(host_id, &heartbeat).await?;
            RpcResponse::result(request.id, &ack)
        }
        method::LEASE_RENEW => {
            let renewal = request
                .params::<LeaseRenew>()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            let result = backend.renew_lease(host_id, &renewal).await?;
            RpcResponse::result(request.id, &result)
        }
        _ => Ok(RpcResponse::error(request.id, -32601, "method not found")),
    }
    .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
    send_response(writer, &response).await
}

async fn handle_host_notification(
    host_id: Uuid,
    writer: &MuxWriter,
    backend: &dyn ControlPlaneHostState,
    notification: RpcNotification,
) -> Result<(), HostCtlError> {
    match notification.method.as_str() {
        method::VM_EVENT => {
            let event = notification
                .params::<VmEvent>()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            event
                .validate()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            let ack = backend.ingest_vm_event(host_id, &event).await?;
            send_notification(
                writer,
                &RpcNotification::new(method::VM_EVENT_ACK, &ack)
                    .map_err(|error| HostCtlError::Protocol(error.to_string()))?,
            )
            .await
        }
        method::VM_FILE_CHANGED => {
            let event = notification
                .params::<VmFileChanged>()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            event
                .validate()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            backend.file_changed(host_id, &event).await
        }
        method::VM_MODULE_EVENTS => {
            let hint = notification
                .params::<g5_internal_api::host_ctl::VmModuleEvents>()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            hint.validate()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            backend.module_events(host_id, &hint).await
        }
        method::HOST_DRAINED => {
            let drained = notification
                .params::<HostDrained>()
                .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
            backend.host_drained(host_id, &drained).await
        }
        _ => Err(HostCtlError::Protocol(format!(
            "unexpected host notification {}",
            notification.method
        ))),
    }
}

/// `g5.runtime.boot.backend.process-local`: a host without a VM boundary is
/// admitted only by a control plane explicitly started for local development.
/// Hosts that advertise no isolation level are the microVM hosts from before
/// the capability existed.
fn process_host_admitted(capabilities: &[String], process_hosts_allowed: bool) -> bool {
    process_hosts_allowed
        || !capabilities
            .iter()
            .any(|capability| capability == g5_internal_api::host_ctl::CAPABILITY_ISOLATION_PROCESS)
}

async fn send_hello_reject(
    writer: &MuxWriter,
    id: String,
    reason: HostRejectReason,
) -> Result<(), HostCtlError> {
    let response = RpcResponse::result(
        id,
        &HelloDisposition::Reject(HostReject {
            accepted: false,
            reason,
            supported: SUPPORTED_PROTOCOL_VERSIONS.to_vec(),
        }),
    )
    .map_err(|error| HostCtlError::Protocol(error.to_string()))?;
    send_response(writer, &response).await
}

async fn read_request(reader: &mut MuxReader) -> Result<RpcRequest, HostCtlError> {
    let line = reader
        .read_line_with_limit(MAX_CTL_MESSAGE_BYTES)
        .await
        .ok_or(HostCtlError::Closed)?;
    match serde_json::from_str::<RpcMessage>(&line)
        .map_err(|error| HostCtlError::Protocol(error.to_string()))?
    {
        RpcMessage::Request(request) => Ok(request),
        _ => Err(HostCtlError::Protocol(
            "expected JSON-RPC request".to_string(),
        )),
    }
}

async fn send_request(writer: &MuxWriter, request: &RpcRequest) -> Result<(), HostCtlError> {
    send_message(writer, &RpcMessage::Request(request.clone())).await
}

async fn send_response(writer: &MuxWriter, response: &RpcResponse) -> Result<(), HostCtlError> {
    send_message(writer, &RpcMessage::Response(response.clone())).await
}

async fn send_notification(
    writer: &MuxWriter,
    notification: &RpcNotification,
) -> Result<(), HostCtlError> {
    send_message(writer, &RpcMessage::Notification(notification.clone())).await
}

async fn send_message(writer: &MuxWriter, message: &RpcMessage) -> Result<(), HostCtlError> {
    let mut encoded =
        serde_json::to_vec(message).map_err(|error| HostCtlError::Protocol(error.to_string()))?;
    encoded.push(b'\n');
    writer
        .send(&encoded)
        .await
        .map_err(|error| HostCtlError::Protocol(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_internal_api::host_ctl::{
        CurrentIncarnation, HostCtlConfig, HostDrain, LeaseGrant, StaleLease, VmEventKind,
        PROTOCOL_VERSION,
    };
    use g5_internal_api::stream_mux::{Channel, Mux};
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct RecordingBackend {
        events: Mutex<Vec<VmEvent>>,
        file_changes: Mutex<Vec<VmFileChanged>>,
        accepted_hellos: Mutex<usize>,
    }

    #[async_trait]
    impl ControlPlaneHostState for RecordingBackend {
        async fn accept_hello(
            &self,
            _host_id: Uuid,
            _hello: &HostHello,
            _connection_active: &HostConnection,
        ) -> Result<HostAccept, HostCtlError> {
            *self.accepted_hellos.lock().await += 1;
            Ok(HostAccept {
                accepted: true,
                protocol_version: _hello.protocol_version,
                incarnation_generation: 1,
                current_incarnations: Vec::new(),
                config: HostCtlConfig::default(),
            })
        }

        async fn heartbeat(
            &self,
            _host_id: Uuid,
            _heartbeat: &HostHeartbeat,
        ) -> Result<HeartbeatAck, HostCtlError> {
            Ok(HeartbeatAck {
                source_preparation: None,
                incarnation_generation: 2,
                current_incarnations: vec![CurrentIncarnation {
                    workspace_id: Uuid::new_v4(),
                    vm_id: Uuid::new_v4(),
                    epoch: 3,
                }],
            })
        }

        async fn ingest_vm_event(
            &self,
            _host_id: Uuid,
            event: &VmEvent,
        ) -> Result<VmEventAck, HostCtlError> {
            self.events.lock().await.push(event.clone());
            Ok(VmEventAck {
                vm_id: event.vm_id,
                through_seq: event.seq,
            })
        }

        async fn file_changed(
            &self,
            _host_id: Uuid,
            event: &VmFileChanged,
        ) -> Result<(), HostCtlError> {
            self.file_changes.lock().await.push(event.clone());
            Ok(())
        }

        async fn renew_lease(
            &self,
            _host_id: Uuid,
            renewal: &LeaseRenew,
        ) -> Result<LeaseRenewResult, HostCtlError> {
            if renewal.seq == 0 {
                Ok(LeaseRenewResult::Stale(StaleLease { stale: true }))
            } else {
                Ok(LeaseRenewResult::Granted(LeaseGrant {
                    stale: false,
                    seq: renewal.seq,
                    token: "token".to_string(),
                    lease_ttl_secs: 30,
                    incarnation_generation: 2,
                }))
            }
        }

        async fn host_drained(
            &self,
            _host_id: Uuid,
            _drained: &HostDrained,
        ) -> Result<(), HostCtlError> {
            Ok(())
        }
    }

    async fn send(writer: &MuxWriter, message: RpcMessage) {
        send_message(writer, &message).await.unwrap();
    }

    async fn recv(reader: &mut MuxReader) -> RpcMessage {
        serde_json::from_str(&reader.read_line().await.unwrap()).unwrap()
    }

    async fn negotiate(writer: &MuxWriter, reader: &mut MuxReader, host_id: Uuid) {
        send(
            writer,
            RpcMessage::Request(
                RpcRequest::new(
                    "hello",
                    method::HOST_HELLO,
                    &HostHello {
                        host_id,
                        protocol_version: PROTOCOL_VERSION,
                        capabilities: Vec::new(),
                        slots_total: 1,
                        slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                            cpus: 2,
                            mem_mib: 4096,
                            scratch_disk_mib: 10240,
                        }),
                        inventory: Vec::new(),
                        retained_roots: Vec::new(),
                        runtime: None,
                    },
                )
                .unwrap(),
            ),
        )
        .await;
        let response = recv(reader).await;
        assert!(matches!(response, RpcMessage::Response(_)));
    }

    #[tokio::test]
    async fn event_is_acked_only_after_backend_ingestion() {
        let host_id = Uuid::new_v4();
        let backend = Arc::new(RecordingBackend::default());
        let (api_io, host_io) = tokio::io::duplex(64 * 1024);
        let mut api_mux = Mux::new(api_io, true);
        let host_mux = Mux::new(host_io, false);
        let host_stream = host_mux.open(Channel::Ctl).await.unwrap();
        let api_stream = api_mux.accept().await.unwrap();
        let session = ControlSession::new();
        let active = Arc::new(HostConnection::new(true));
        let task = tokio::spawn(serve_control_stream(
            host_id,
            api_stream,
            backend.clone(),
            session,
            active,
        ));
        let (writer, mut reader) = host_stream.into_split();
        negotiate(&writer, &mut reader, host_id).await;
        let event = VmEvent {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            seq: 1,
            kind: VmEventKind::Booted,
            at: chrono::Utc::now(),
            log_tail: None,
        };
        send(
            &writer,
            RpcMessage::Notification(RpcNotification::new(method::VM_EVENT, &event).unwrap()),
        )
        .await;
        match recv(&mut reader).await {
            RpcMessage::Notification(notification) => {
                assert_eq!(notification.method, method::VM_EVENT_ACK);
                assert_eq!(
                    notification.params::<VmEventAck>().unwrap(),
                    VmEventAck {
                        vm_id: event.vm_id,
                        through_seq: 1,
                    }
                );
            }
            _ => panic!("expected event ack"),
        }
        assert_eq!(backend.events.lock().await.as_slice(), &[event]);
        task.abort();
    }

    #[tokio::test]
    async fn file_change_notification_uses_the_ephemeral_backend_path() {
        let host_id = Uuid::new_v4();
        let backend = Arc::new(RecordingBackend::default());
        let (api_io, host_io) = tokio::io::duplex(64 * 1024);
        let mut api_mux = Mux::new(api_io, true);
        let host_mux = Mux::new(host_io, false);
        let host_stream = host_mux.open(Channel::Ctl).await.unwrap();
        let api_stream = api_mux.accept().await.unwrap();
        let task = tokio::spawn(serve_control_stream(
            host_id,
            api_stream,
            backend.clone(),
            ControlSession::new(),
            Arc::new(HostConnection::new(true)),
        ));
        let (writer, mut reader) = host_stream.into_split();
        negotiate(&writer, &mut reader, host_id).await;
        let event = VmFileChanged {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            path: "src/main.rs".into(),
            rescan_required: false,
        };
        send(
            &writer,
            RpcMessage::Notification(
                RpcNotification::new(method::VM_FILE_CHANGED, &event).unwrap(),
            ),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if !backend.file_changes.lock().await.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("ephemeral file change did not reach the backend");
        assert_eq!(backend.file_changes.lock().await.as_slice(), &[event]);
        task.abort();
    }

    #[tokio::test]
    async fn command_timeout_is_ambiguous_and_retry_reuses_key() {
        let host_id = Uuid::new_v4();
        let backend = Arc::new(RecordingBackend::default());
        let (api_io, host_io) = tokio::io::duplex(64 * 1024);
        let mut api_mux = Mux::new(api_io, true);
        let host_mux = Mux::new(host_io, false);
        let host_stream = host_mux.open(Channel::Ctl).await.unwrap();
        let api_stream = api_mux.accept().await.unwrap();
        let session = ControlSession::new();
        let client = session.client.clone();
        let active = Arc::new(HostConnection::new(true));
        let task = tokio::spawn(serve_control_stream(
            host_id, api_stream, backend, session, active,
        ));
        let (writer, mut reader) = host_stream.into_split();
        negotiate(&writer, &mut reader, host_id).await;
        let params = CommandParams {
            idempotency_key: "drain-once".to_string(),
            budget_ms: 5_000,
            command: HostDrain {
                drain_id: Uuid::new_v4(),
            },
        };
        let first = tokio::spawn({
            let client = client.clone();
            let params = params.clone();
            async move {
                client
                    .execute(method::HOST_DRAIN, &params, Duration::from_millis(1))
                    .await
            }
        });
        let first_request = match recv(&mut reader).await {
            RpcMessage::Request(request) => request,
            _ => panic!("expected first command"),
        };
        assert!(matches!(first.await.unwrap(), Err(HostCtlError::Timeout)));

        let retry = tokio::spawn({
            let client = client.clone();
            let params = params.clone();
            async move {
                client
                    .execute(method::HOST_DRAIN, &params, Duration::from_secs(1))
                    .await
            }
        });
        let retry_request = match recv(&mut reader).await {
            RpcMessage::Request(request) => request,
            _ => panic!("expected retried command"),
        };
        assert_ne!(first_request.id, retry_request.id);
        assert_eq!(
            first_request.params["idempotency_key"],
            retry_request.params["idempotency_key"]
        );
        send(
            &writer,
            RpcMessage::Response(
                RpcResponse::result(retry_request.id, &CommandResult::Accepted).unwrap(),
            ),
        )
        .await;
        assert_eq!(retry.await.unwrap().unwrap(), CommandResult::Accepted);
        task.abort();
    }

    #[tokio::test]
    async fn replaced_connection_stops_before_backend_work() {
        let host_id = Uuid::new_v4();
        let backend = Arc::new(RecordingBackend::default());
        let (api_io, host_io) = tokio::io::duplex(64 * 1024);
        let mut api_mux = Mux::new(api_io, true);
        let host_mux = Mux::new(host_io, false);
        let host_stream = host_mux.open(Channel::Ctl).await.unwrap();
        let api_stream = api_mux.accept().await.unwrap();
        let active = Arc::new(HostConnection::new(false));
        let task = tokio::spawn(serve_control_stream(
            host_id,
            api_stream,
            backend.clone(),
            ControlSession::new(),
            active,
        ));
        let (writer, _reader) = host_stream.into_split();
        send(
            &writer,
            RpcMessage::Request(
                RpcRequest::new(
                    "hello",
                    method::HOST_HELLO,
                    &HostHello {
                        host_id,
                        protocol_version: PROTOCOL_VERSION,
                        capabilities: Vec::new(),
                        slots_total: 1,
                        slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                            cpus: 2,
                            mem_mib: 4096,
                            scratch_disk_mib: 10240,
                        }),
                        inventory: Vec::new(),
                        retained_roots: Vec::new(),
                        runtime: None,
                    },
                )
                .unwrap(),
            ),
        )
        .await;
        task.await.unwrap().unwrap();
        assert!(backend.events.lock().await.is_empty());
        assert_eq!(*backend.accepted_hellos.lock().await, 0);
    }

    #[test]
    fn process_hosts_need_the_local_development_switch() {
        let process = vec![
            "workspace_vm:v1".to_string(),
            g5_internal_api::host_ctl::CAPABILITY_ISOLATION_PROCESS.to_string(),
        ];
        let microvm = vec![
            "workspace_vm:v1".to_string(),
            g5_internal_api::host_ctl::CAPABILITY_ISOLATION_MICROVM.to_string(),
        ];
        assert!(!process_host_admitted(&process, false));
        assert!(process_host_admitted(&process, true));
        assert!(process_host_admitted(&microvm, false));
        assert!(process_host_admitted(&[], false));
    }

    /// A process-isolation host is refused before any backend work unless the
    /// control plane was started for local development; the test environment
    /// does not set that switch.
    #[tokio::test]
    async fn hello_from_a_process_host_is_rejected_before_backend_work() {
        if g5_contracts::env::is_env_flag_enabled(
            g5_internal_api::host_ctl::ALLOW_PROCESS_HOSTS_ENV,
        ) {
            eprintln!("SKIP: the test environment admits process hosts");
            return;
        }
        let host_id = Uuid::new_v4();
        let backend = Arc::new(RecordingBackend::default());
        let (api_io, host_io) = tokio::io::duplex(64 * 1024);
        let mut api_mux = Mux::new(api_io, true);
        let host_mux = Mux::new(host_io, false);
        let host_stream = host_mux.open(Channel::Ctl).await.unwrap();
        let api_stream = api_mux.accept().await.unwrap();
        let task = tokio::spawn(serve_control_stream(
            host_id,
            api_stream,
            backend.clone(),
            ControlSession::new(),
            Arc::new(HostConnection::new(true)),
        ));
        let (writer, mut reader) = host_stream.into_split();
        send(
            &writer,
            RpcMessage::Request(
                RpcRequest::new(
                    "hello",
                    method::HOST_HELLO,
                    &HostHello {
                        host_id,
                        protocol_version: PROTOCOL_VERSION,
                        capabilities: vec![
                            "workspace_vm:v1".to_string(),
                            g5_internal_api::host_ctl::CAPABILITY_ISOLATION_PROCESS.to_string(),
                        ],
                        slots_total: 1,
                        slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                            cpus: 2,
                            mem_mib: 4096,
                            scratch_disk_mib: 10240,
                        }),
                        inventory: Vec::new(),
                        retained_roots: Vec::new(),
                        runtime: None,
                    },
                )
                .unwrap(),
            ),
        )
        .await;
        let reject = match recv(&mut reader).await {
            RpcMessage::Response(response) => response
                .result_as::<HelloDisposition>()
                .expect("valid hello response"),
            _ => panic!("expected hello response"),
        };
        assert!(matches!(
            reject,
            HelloDisposition::Reject(HostReject {
                reason: HostRejectReason::IsolationNotAdmitted,
                ..
            })
        ));
        task.await.unwrap().unwrap();
        assert_eq!(*backend.accepted_hellos.lock().await, 0);
    }

    #[tokio::test]
    async fn hello_identity_mismatch_is_rejected_before_backend_work() {
        let authenticated_host_id = Uuid::new_v4();
        let claimed_host_id = Uuid::new_v4();
        let backend = Arc::new(RecordingBackend::default());
        let (api_io, host_io) = tokio::io::duplex(64 * 1024);
        let mut api_mux = Mux::new(api_io, true);
        let host_mux = Mux::new(host_io, false);
        let host_stream = host_mux.open(Channel::Ctl).await.unwrap();
        let api_stream = api_mux.accept().await.unwrap();
        let task = tokio::spawn(serve_control_stream(
            authenticated_host_id,
            api_stream,
            backend.clone(),
            ControlSession::new(),
            Arc::new(HostConnection::new(true)),
        ));
        let (writer, mut reader) = host_stream.into_split();
        send(
            &writer,
            RpcMessage::Request(
                RpcRequest::new(
                    "hello",
                    method::HOST_HELLO,
                    &HostHello {
                        host_id: claimed_host_id,
                        protocol_version: PROTOCOL_VERSION,
                        capabilities: Vec::new(),
                        slots_total: 1,
                        slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                            cpus: 2,
                            mem_mib: 4096,
                            scratch_disk_mib: 10240,
                        }),
                        inventory: Vec::new(),
                        retained_roots: Vec::new(),
                        runtime: None,
                    },
                )
                .unwrap(),
            ),
        )
        .await;
        let reject = match recv(&mut reader).await {
            RpcMessage::Response(response) => response
                .result_as::<HelloDisposition>()
                .expect("valid hello response"),
            _ => panic!("expected hello response"),
        };
        assert!(matches!(
            reject,
            HelloDisposition::Reject(HostReject {
                reason: HostRejectReason::IdentityMismatch,
                ..
            })
        ));
        task.await.unwrap().unwrap();
        assert_eq!(*backend.accepted_hellos.lock().await, 0);
    }
}
