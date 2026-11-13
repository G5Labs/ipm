//! Scripted worker-host protocol peer for control-plane integration tests.
//!
//! The peer speaks the production `host_ctl` wire types over the real stream
//! multiplexer. Its state survives control-stream reconnects, which lets tests
//! exercise inventory reconciliation and journal replay without inventing a
//! second lifecycle implementation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::host_ctl::{
    method, CommandResult, HeartbeatAck, HelloDisposition, HostHeartbeat, HostHello, HostVmState,
    RpcMessage, RpcNotification, RpcRequest, RpcResponse, VmEvent, VmEventAck, VmEventKind,
    VmInventory, MAX_CTL_MESSAGE_BYTES, PROTOCOL_VERSION,
};
use crate::stream_mux::{MuxReader, MuxStream, MuxWriter};

#[derive(Debug, Clone, Default)]
pub struct ScriptedHostFaults {
    /// Close the control stream after the first replayed event and before its
    /// acknowledgement can be consumed.
    pub disconnect_after_first_event: bool,
    /// Send each replayed event twice. The API must deduplicate before acking.
    pub duplicate_replayed_events: bool,
    /// Hold command replies long enough to trigger caller timeout/retry paths.
    pub command_response_delay: Duration,
    /// Optional definite result by command method. Commands without an entry
    /// retain the default `accepted` response used by reconciliation tests.
    pub command_results: HashMap<String, CommandResult>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScriptedHostError {
    #[error("scripted host control stream closed")]
    Closed,
    #[error("scripted host protocol error: {0}")]
    Protocol(String),
}

#[derive(Default)]
struct PeerState {
    inventory: Vec<VmInventory>,
    next_seq: HashMap<Uuid, u64>,
    pending_events: Vec<VmEvent>,
    received_commands: Vec<RpcRequest>,
    acknowledged: HashMap<Uuid, u64>,
}

/// Cloneable scripted peer whose inventory and event journal span reconnects.
#[derive(Clone)]
pub struct ScriptedHostPeer {
    host_id: Uuid,
    slots_total: u32,
    slot_size: crate::host_ctl::VmSizeSpec,
    runtime: Option<crate::host_ctl::HostRuntimeIdentity>,
    state: Arc<Mutex<PeerState>>,
}

impl ScriptedHostPeer {
    #[must_use]
    pub fn new(host_id: Uuid, slots_total: u32) -> Self {
        Self {
            host_id,
            slots_total,
            slot_size: crate::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 10240,
            },
            runtime: Some(crate::host_ctl::HostRuntimeIdentity::bundled()),
            state: Arc::new(Mutex::new(PeerState::default())),
        }
    }

    pub fn with_slot_size(mut self, size: crate::host_ctl::VmSizeSpec) -> Self {
        self.slot_size = size;
        self
    }

    /// Publish another runtime identity, or none, on every hello: a host
    /// from an older image, or from before identities were published.
    pub fn with_runtime_identity(
        mut self,
        runtime: Option<crate::host_ctl::HostRuntimeIdentity>,
    ) -> Self {
        self.runtime = runtime;
        self
    }

    pub async fn set_inventory(&self, inventory: Vec<VmInventory>) {
        self.state.lock().await.inventory = inventory;
    }

    pub async fn record_event(
        &self,
        workspace_id: Uuid,
        vm_id: Uuid,
        kind: VmEventKind,
    ) -> VmEvent {
        let mut state = self.state.lock().await;
        let seq = state.next_seq.entry(vm_id).or_default();
        *seq += 1;
        let event = VmEvent {
            workspace_id,
            vm_id,
            seq: *seq,
            kind,
            at: Utc::now(),
            log_tail: None,
        };
        state.pending_events.push(event.clone());
        event
    }

    pub async fn pending_events(&self) -> Vec<VmEvent> {
        self.state.lock().await.pending_events.clone()
    }

    pub async fn received_commands(&self) -> Vec<RpcRequest> {
        self.state.lock().await.received_commands.clone()
    }

    pub async fn acknowledged_through(&self, vm_id: Uuid) -> u64 {
        self.state
            .lock()
            .await
            .acknowledged
            .get(&vm_id)
            .copied()
            .unwrap_or_default()
    }

    /// Run one control-stream connection until the peer or script closes it.
    pub async fn run_control_stream(
        &self,
        stream: MuxStream,
        faults: ScriptedHostFaults,
    ) -> Result<(), ScriptedHostError> {
        let (writer, mut reader) = stream.into_split();
        let inventory = self.state.lock().await.inventory.clone();
        let hello = HostHello {
            host_id: self.host_id,
            protocol_version: PROTOCOL_VERSION,
            capabilities: vec!["test:scripted-host".to_string()],
            slots_total: self.slots_total,
            slot_size: Some(self.slot_size.clone()),
            inventory,
            retained_roots: Vec::new(),
            runtime: self.runtime.clone(),
        };
        let hello_id = format!("hello:{}", Uuid::new_v4());
        send(
            &writer,
            &RpcMessage::Request(
                RpcRequest::new(hello_id.clone(), method::HOST_HELLO, &hello)
                    .map_err(protocol_error)?,
            ),
        )
        .await?;
        let response = read(&mut reader).await?;
        let RpcMessage::Response(response) = response else {
            return Err(ScriptedHostError::Protocol(
                "expected host.hello response".to_string(),
            ));
        };
        if response.id != hello_id {
            return Err(ScriptedHostError::Protocol(
                "host.hello response id mismatch".to_string(),
            ));
        }
        match response
            .result_as::<HelloDisposition>()
            .map_err(protocol_error)?
        {
            HelloDisposition::Accept(accept)
                if accept.accepted && accept.protocol_version == PROTOCOL_VERSION => {}
            HelloDisposition::Reject(reject) => {
                return Err(ScriptedHostError::Protocol(format!(
                    "host rejected: {:?}",
                    reject.reason
                )));
            }
            _ => {
                return Err(ScriptedHostError::Protocol(
                    "invalid host.accept response".to_string(),
                ));
            }
        }

        let replay = self.state.lock().await.pending_events.clone();
        for (index, event) in replay.iter().enumerate() {
            let message = RpcMessage::Notification(
                RpcNotification::new(method::VM_EVENT, event).map_err(protocol_error)?,
            );
            send(&writer, &message).await?;
            if faults.duplicate_replayed_events {
                send(&writer, &message).await?;
            }
            if faults.disconnect_after_first_event && index == 0 {
                writer.close().await;
                return Ok(());
            }
        }

        let heartbeat_id = format!("heartbeat:{}", Uuid::new_v4());
        send(
            &writer,
            &RpcMessage::Request(
                RpcRequest::new(
                    heartbeat_id,
                    method::HOST_HEARTBEAT,
                    &HostHeartbeat {
                        refresh_source_preparation: false,
                        slots_in_use: replay.len() as u32,
                        boots_in_progress: 0,
                        load: 0.0,
                        memory_free_mib: 1024,
                        disk_free_mib: 4096,
                        retained_roots: Vec::new(),
                        vms: replay
                            .iter()
                            .map(|event| crate::host_ctl::VmHeartbeat {
                                vm_id: event.vm_id,
                                state: HostVmState::Running,
                                cpu_pct: 0.0,
                                mem_mib: 0,
                            })
                            .collect(),
                    },
                )
                .map_err(protocol_error)?,
            ),
        )
        .await?;

        loop {
            match read(&mut reader).await? {
                RpcMessage::Notification(notification)
                    if notification.method == method::VM_EVENT_ACK =>
                {
                    let ack = notification
                        .params::<VmEventAck>()
                        .map_err(protocol_error)?;
                    let mut state = self.state.lock().await;
                    let previous = state.acknowledged.entry(ack.vm_id).or_default();
                    *previous = (*previous).max(ack.through_seq);
                    state
                        .pending_events
                        .retain(|event| event.vm_id != ack.vm_id || event.seq > ack.through_seq);
                }
                RpcMessage::Request(request) => {
                    let result = faults
                        .command_results
                        .get(&request.method)
                        .cloned()
                        .unwrap_or(CommandResult::Accepted);
                    self.state
                        .lock()
                        .await
                        .received_commands
                        .push(request.clone());
                    if !faults.command_response_delay.is_zero() {
                        tokio::time::sleep(faults.command_response_delay).await;
                    }
                    send(
                        &writer,
                        &RpcMessage::Response(
                            RpcResponse::result(request.id, &result).map_err(protocol_error)?,
                        ),
                    )
                    .await?;
                }
                RpcMessage::Response(response) if response.id.starts_with("heartbeat:") => {
                    response
                        .result_as::<HeartbeatAck>()
                        .map_err(protocol_error)?;
                }
                message => {
                    return Err(ScriptedHostError::Protocol(format!(
                        "unexpected message from control plane: {message:?}"
                    )));
                }
            }
        }
    }
}

fn protocol_error(error: impl std::fmt::Display) -> ScriptedHostError {
    ScriptedHostError::Protocol(error.to_string())
}

async fn read(reader: &mut MuxReader) -> Result<RpcMessage, ScriptedHostError> {
    let line = reader
        .read_line_with_limit(MAX_CTL_MESSAGE_BYTES)
        .await
        .ok_or(ScriptedHostError::Closed)?;
    serde_json::from_str(&line).map_err(protocol_error)
}

async fn send(writer: &MuxWriter, message: &RpcMessage) -> Result<(), ScriptedHostError> {
    let mut encoded = serde_json::to_vec(message).map_err(protocol_error)?;
    encoded.push(b'\n');
    writer.send(&encoded).await.map_err(protocol_error)
}

/// Convenience inventory row for tests that only care about reconciliation.
#[must_use]
pub fn running_inventory(workspace_id: Uuid, vm_id: Uuid) -> VmInventory {
    VmInventory {
        workspace_id,
        vm_id,
        state: HostVmState::Running,
        boot_at: Utc::now(),
    }
}
