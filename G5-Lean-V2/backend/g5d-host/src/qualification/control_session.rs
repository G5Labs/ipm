//! Host loss, reconnect, journal replay, superseded-VM cleanup and drain over
//! the real `ctl` stream, with the workspace runtime holding scripted VMs.
//!
//! The control plane is a scripted peer on the far side of a real mux, so
//! every assertion below is about bytes the host actually sends and the
//! order it sends them in.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::ctl::{
    serve_control_stream, EventJournal, HostCommandProcessor, LeaseRenewalBroker, SessionOutcome,
};
use crate::runtime::WorkspaceHostState;
use crate::test_support::{materializer_available, FakeLauncher, HostFixture};
use g5_internal_api::host_ctl::{
    method, CommandParams, CommandResult, CurrentIncarnation, HelloDisposition, HostAccept,
    HostCtlConfig, HostDrain, HostDrained, HostHello, RootsPurge, RpcMessage, RpcNotification,
    RpcRequest, RpcResponse, StopMode, VmEvent, VmEventAck, VmEventKind, VmStatus, VmStop,
    PROTOCOL_VERSION,
};
use g5_internal_api::stream_mux::{Channel, Mux, MuxReader, MuxWriter};
use serde::Serialize;
use uuid::Uuid;

/// The control-plane end of one tunnel lifetime.
struct ScriptedControlPlane {
    _mux: Mux,
    writer: MuxWriter,
    reader: MuxReader,
}

impl ScriptedControlPlane {
    async fn read(&mut self) -> RpcMessage {
        // nextest owns the hang guard. Protocol/event assertions below prove
        // progress without a competing wall-clock budget under CI load.
        let line = self.reader.read_line().await.expect("ctl stream open");
        serde_json::from_str(&line).expect("ctl message decodes")
    }

    async fn send(&self, message: &RpcMessage) {
        let mut encoded = serde_json::to_vec(message).unwrap();
        encoded.push(b'\n');
        self.writer.send(&encoded).await.unwrap();
    }

    async fn expect_hello(&mut self) -> (String, HostHello) {
        match self.read().await {
            RpcMessage::Request(request) if request.method == method::HOST_HELLO => {
                let hello = request.params::<HostHello>().unwrap();
                (request.id, hello)
            }
            other => panic!("expected host.hello first, got {other:?}"),
        }
    }

    async fn accept(&self, hello_id: String, generation: u64, current: Vec<CurrentIncarnation>) {
        self.accept_with_heartbeat(hello_id, generation, current, 60)
            .await;
    }

    async fn accept_with_heartbeat(
        &self,
        hello_id: String,
        generation: u64,
        current: Vec<CurrentIncarnation>,
        heartbeat_secs: u64,
    ) {
        self.send(&RpcMessage::Response(
            RpcResponse::result(
                hello_id,
                &HelloDisposition::Accept(HostAccept {
                    accepted: true,
                    protocol_version: PROTOCOL_VERSION,
                    incarnation_generation: generation,
                    current_incarnations: current,
                    config: HostCtlConfig {
                        heartbeat_secs,
                        result_retention_secs: 600,
                        source_preparation_enabled: false,
                        module_event_hints_enabled: false,
                    },
                }),
            )
            .unwrap(),
        ))
        .await;
    }

    async fn command<T: Serialize>(&self, id: &str, method: &str, key: &str, command: T) {
        self.send(&RpcMessage::Request(
            RpcRequest::new(
                id,
                method,
                &CommandParams {
                    idempotency_key: key.to_string(),
                    budget_ms: 30_000,
                    command,
                },
            )
            .unwrap(),
        ))
        .await;
    }

    async fn ack(&self, event: &VmEvent) {
        self.send(&RpcMessage::Notification(
            RpcNotification::new(
                method::VM_EVENT_ACK,
                &VmEventAck {
                    vm_id: event.vm_id,
                    through_seq: event.seq,
                },
            )
            .unwrap(),
        ))
        .await;
    }

    /// Read until the response with `id` arrives, returning it plus every
    /// `vm.event` notification seen on the way (in order).
    async fn response(&mut self, id: &str) -> (CommandResult, Vec<VmEvent>) {
        let mut events = Vec::new();
        loop {
            match self.read().await {
                RpcMessage::Response(response) if response.id == id => {
                    return (response.result_as::<CommandResult>().unwrap(), events);
                }
                RpcMessage::Notification(notification)
                    if notification.method == method::VM_EVENT =>
                {
                    events.push(notification.params::<VmEvent>().unwrap());
                }
                other => panic!("unexpected ctl traffic while waiting for {id}: {other:?}"),
            }
        }
    }

    /// Read exactly `count` `vm.event` notifications, refusing any other traffic.
    async fn events(&mut self, count: usize) -> Vec<VmEvent> {
        let mut events = Vec::new();
        while events.len() < count {
            match self.read().await {
                RpcMessage::Notification(notification)
                    if notification.method == method::VM_EVENT =>
                {
                    events.push(notification.params::<VmEvent>().unwrap());
                }
                other => panic!("expected only vm.event replay, got {other:?}"),
            }
        }
        events
    }
}

struct Host {
    fixture: HostFixture,
    launcher: Arc<FakeLauncher>,
    journal: Arc<EventJournal>,
    state: Arc<WorkspaceHostState>,
    commands: Arc<HostCommandProcessor>,
    renewals: Arc<LeaseRenewalBroker>,
}

impl Host {
    /// Dial one tunnel lifetime: the host opens `ctl`, the scripted control
    /// plane accepts it.
    async fn connect(
        &self,
    ) -> (
        ScriptedControlPlane,
        tokio::task::JoinHandle<anyhow::Result<SessionOutcome>>,
    ) {
        let (host_io, api_io) = tokio::io::duplex(256 * 1024);
        let host_mux = Mux::new(host_io, false);
        let mut api_mux = Mux::new(api_io, true);
        let control = host_mux.open(Channel::Ctl).await.unwrap();
        let session = tokio::spawn({
            let state: Arc<dyn crate::ctl::HostControlState> = self.state.clone();
            let journal = self.journal.clone();
            let commands = self.commands.clone();
            let renewals = self.renewals.clone();
            async move {
                // The host mux must outlive the session, as the carrier's does.
                let _host_mux = host_mux;
                serve_control_stream(control, state, journal, commands, renewals).await
            }
        });
        let peer = api_mux.accept().await.expect("host opens ctl");
        let (writer, reader) = peer.into_split();
        (
            ScriptedControlPlane {
                _mux: api_mux,
                writer,
                reader,
            },
            session,
        )
    }
}

#[tokio::test]
async fn reconnect_replays_the_journal_kills_superseded_vms_and_drains_to_completion() {
    let temp = tempfile::Builder::new()
        .prefix("g5-host-session-")
        .tempdir_in(
            std::env::var_os("G5_TEST_REFLINK_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap()),
        )
        .unwrap();
    let fixture = HostFixture::new(temp.path());
    if !materializer_available(&fixture).await {
        return;
    }
    let launcher = Arc::new(FakeLauncher::default());
    let journal = fixture.journal().await;
    let renewals = Arc::new(LeaseRenewalBroker::default());
    let runtime = fixture.runtime(launcher.clone(), journal.clone(), renewals.clone(), 2);
    let host_id = Uuid::new_v4();
    let host = Host {
        state: Arc::new(WorkspaceHostState::new(
            host_id,
            2,
            fixture.workspaces_root.clone(),
            runtime.clone(),
        )),
        commands: Arc::new(HostCommandProcessor::default().with_workspace_runtime(runtime.clone())),
        fixture,
        launcher,
        journal,
        renewals,
    };
    let ws_a = Uuid::new_v4();
    let vm_a = Uuid::new_v4();
    let ws_b = Uuid::new_v4();
    let vm_b = Uuid::new_v4();

    // --- Tunnel lifetime 1: start A, observe its events, lose the tunnel. ---
    let (mut cp, session) = host.connect().await;
    let (hello_id, hello) = cp.expect_hello().await;
    assert_eq!(hello.host_id, host_id);
    assert!(hello.inventory.is_empty());
    cp.accept(hello_id, 1, Vec::new()).await;

    let start_a = host.fixture.start(ws_a, vm_a, 1);
    cp.command("start-a", method::VM_START, "start-a", start_a.clone())
        .await;
    let (result, mut events) = cp.response("start-a").await;
    assert_eq!(result, CommandResult::Started);
    // Booted and Ready are journaled before the result is sent, so either
    // both already arrived or they follow immediately.
    if events.len() < 2 {
        events.extend(cp.events(2 - events.len()).await);
    }
    assert!(matches!(events[0].kind, VmEventKind::Booted));
    assert!(matches!(events[1].kind, VmEventKind::Ready { .. }));
    assert!(events[1]
        .log_tail
        .as_deref()
        .is_some_and(|tail| tail.contains("boot_to_ready_ms=")));
    cp.ack(&events[1]).await;
    while !host.journal.pending().await.is_empty() {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    // A retry after a lost reply gets the retained result, not a second boot.
    cp.command(
        "start-a-retry",
        method::VM_START,
        "start-a",
        start_a.clone(),
    )
    .await;
    assert_eq!(cp.response("start-a-retry").await.0, CommandResult::Started);
    assert_eq!(host.launcher.started().await.len(), 1);
    let mut conflicting = start_a.clone();
    conflicting.spec.incarnation.epoch = 2;
    cp.command("start-a-conflict", method::VM_START, "start-a", conflicting)
        .await;
    assert_eq!(
        cp.response("start-a-conflict").await.0,
        CommandResult::IdempotencyConflict
    );

    drop(cp);
    let outcome = session.await.unwrap().unwrap();
    assert_eq!(outcome, SessionOutcome::Disconnected);
    assert_eq!(
        runtime.active_count().await,
        1,
        "a tunnel drop keeps the VM running"
    );

    // --- Disconnected: B boots from a retried command; its events wait in
    //     the durable journal. ---
    let start_b = RpcRequest::new(
        "start-b",
        method::VM_START,
        &CommandParams {
            idempotency_key: "start-b".to_string(),
            budget_ms: 30_000,
            command: host.fixture.start(ws_b, vm_b, 1),
        },
    )
    .unwrap();
    assert_eq!(
        host.commands
            .execute(&start_b, Instant::now(), Duration::from_secs(600))
            .await
            .unwrap(),
        CommandResult::Started
    );
    let pending = host.journal.pending().await;
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().all(|event| event.vm_id == vm_b));

    // --- Tunnel lifetime 2: inventory carries both VMs; the control plane
    //     only knows B, so A is stale. Replay precedes every other message. ---
    let (mut cp, session) = host.connect().await;
    let (hello_id, hello) = cp.expect_hello().await;
    let mut inventory = hello
        .inventory
        .iter()
        .map(|vm| vm.vm_id)
        .collect::<Vec<_>>();
    inventory.sort();
    let mut expected = vec![vm_a, vm_b];
    expected.sort();
    assert_eq!(inventory, expected);
    cp.accept(
        hello_id,
        2,
        vec![CurrentIncarnation {
            workspace_id: ws_b,
            vm_id: vm_b,
            epoch: 1,
        }],
    )
    .await;
    let replay = cp.events(4).await;
    let b_kinds = replay
        .iter()
        .filter(|event| event.vm_id == vm_b)
        .map(|event| std::mem::discriminant(&event.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        b_kinds,
        vec![
            std::mem::discriminant(&VmEventKind::Booted),
            std::mem::discriminant(&VmEventKind::Ready {
                epoch: 0,
                module_catalog_version: String::new(),
                dispatcher_contract_version: 0
            }),
        ]
    );
    let a_kinds = replay
        .iter()
        .filter(|event| event.vm_id == vm_a)
        .map(|event| std::mem::discriminant(&event.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        a_kinds,
        vec![
            std::mem::discriminant(&VmEventKind::Exited {
                code: None,
                signal: None
            }),
            std::mem::discriminant(&VmEventKind::StaleKilled),
        ],
        "the superseded VM is killed on reconnect and journaled as stale"
    );
    assert!(replay
        .windows(2)
        .filter(|pair| pair[0].vm_id == pair[1].vm_id)
        .all(|pair| pair[1].seq == pair[0].seq + 1));
    let kills = host.launcher.stopped().await;
    assert_eq!(kills.len(), 1);
    assert_eq!(kills[0].mode, StopMode::Kill);
    for event in &replay {
        cp.ack(event).await;
    }
    assert_eq!(runtime.active_count().await, 1);
    assert!(runtime.endpoint(vm_a).await.is_none());

    cp.command(
        "status-a",
        method::VM_STATUS,
        "status-a",
        VmStatus {
            workspace_id: ws_a,
            vm_id: vm_a,
        },
    )
    .await;
    assert_eq!(cp.response("status-a").await.0, CommandResult::NotFound);
    cp.command(
        "status-b",
        method::VM_STATUS,
        "status-b",
        VmStatus {
            workspace_id: ws_b,
            vm_id: vm_b,
        },
    )
    .await;
    assert!(matches!(
        cp.response("status-b").await.0,
        CommandResult::Status { .. }
    ));

    // --- Drain: no new starts, existing VM stays until the control plane
    //     stops it, then host.drained names the drain. ---
    let drain_id = Uuid::new_v4();
    cp.command("drain", method::HOST_DRAIN, "drain", HostDrain { drain_id })
        .await;
    assert_eq!(cp.response("drain").await.0, CommandResult::Accepted);
    cp.command(
        "start-c",
        method::VM_START,
        "start-c",
        host.fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1),
    )
    .await;
    assert_eq!(cp.response("start-c").await.0, CommandResult::Draining);
    assert_eq!(
        runtime.active_count().await,
        1,
        "draining keeps running VMs"
    );

    cp.command(
        "stop-b",
        method::VM_STOP,
        "stop-b",
        VmStop {
            workspace_id: ws_b,
            vm_id: vm_b,
            mode: StopMode::Graceful,
            grace_secs: 30,
        },
    )
    .await;
    let mut drained = None;
    let mut stop_result = None;
    let mut exited = None;
    while drained.is_none() || stop_result.is_none() || exited.is_none() {
        match cp.read().await {
            RpcMessage::Response(response) if response.id == "stop-b" => {
                stop_result = Some(response.result_as::<CommandResult>().unwrap());
            }
            RpcMessage::Notification(notification) if notification.method == method::VM_EVENT => {
                let event = notification.params::<VmEvent>().unwrap();
                assert_eq!(event.vm_id, vm_b);
                assert!(matches!(
                    event.kind,
                    VmEventKind::Exited { code: Some(0), .. }
                ));
                cp.ack(&event).await;
                exited = Some(event);
            }
            RpcMessage::Notification(notification)
                if notification.method == method::HOST_DRAINED =>
            {
                drained = Some(notification.params::<HostDrained>().unwrap());
            }
            other => panic!("unexpected traffic during drain: {other:?}"),
        }
    }
    assert!(matches!(stop_result, Some(CommandResult::Stopped { .. })));
    assert_eq!(drained.unwrap().drain_id, drain_id);
    assert_eq!(runtime.active_count().await, 0);
    // The stale kill and the graceful stop both deleted their roots.
    let workspaces_root = host.fixture.workspaces_root.clone();
    let deadline = Instant::now() + Duration::from_secs(5);
    while [ws_a, ws_b]
        .iter()
        .any(|workspace_id| workspaces_root.join(workspace_id.to_string()).exists())
        || std::fs::read_dir(&workspaces_root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".trash-"))
        })
    {
        assert!(Instant::now() < deadline, "released roots were not deleted");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // A control plane from before root deletion still purges before a
    // transfer or delete; the host answers that nothing is left.
    cp.command(
        "purge-a",
        method::ROOTS_PURGE,
        "purge-a",
        RootsPurge {
            workspace_id: ws_a,
            cache_generation: 1,
        },
    )
    .await;
    let (purged, events) = cp.response("purge-a").await;
    assert_eq!(purged, CommandResult::NotFound);
    assert!(events.is_empty());
    session.abort();
}

async fn progress_host(
    slots: u32,
) -> Option<(
    tempfile::TempDir,
    Host,
    Arc<crate::workspace_vm::WorkspaceVmRuntime>,
)> {
    let temp = tempfile::Builder::new()
        .prefix("g5-host-progress-")
        .tempdir_in(
            std::env::var_os("G5_TEST_REFLINK_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap()),
        )
        .unwrap();
    let fixture = HostFixture::new(temp.path());
    let launcher = Arc::new(FakeLauncher::default());
    let journal = fixture.journal().await;
    let renewals = Arc::new(LeaseRenewalBroker::default());
    let runtime = fixture.runtime(launcher.clone(), journal.clone(), renewals.clone(), slots);
    let host = Host {
        state: Arc::new(WorkspaceHostState::new(
            Uuid::new_v4(),
            slots,
            fixture.workspaces_root.clone(),
            runtime.clone(),
        )),
        commands: Arc::new(HostCommandProcessor::default().with_workspace_runtime(runtime.clone())),
        fixture,
        launcher,
        journal,
        renewals,
    };
    Some((temp, host, runtime))
}

#[tokio::test]
async fn reconnect_and_newer_rosters_progress_while_one_stale_stop_has_no_exit_proof() {
    use g5_internal_api::host_ctl::{HeartbeatAck, HostVmState};
    let Some((_temp, host, runtime)) = progress_host(3).await else {
        return;
    };
    let workspaces = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    // The unproven stop is first in the ordered active map: sequential
    // reconciliation cannot reach the second stale VM on the old path.
    let vms = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
    for i in 0..3 {
        runtime
            .seed_running_vm(host.fixture.start(workspaces[i], vms[i], 1).spec)
            .await;
        host.journal
            .record(workspaces[i], vms[i], VmEventKind::Booted, None)
            .await
            .unwrap();
    }
    let stuck_handle = format!("test-{}", vms[0]);
    host.launcher.fail_stop_for(&stuck_handle, true).await;
    let (mut cp, session) = host.connect().await;
    let (hello_id, _) = cp.expect_hello().await;
    cp.accept_with_heartbeat(
        hello_id,
        1,
        vec![CurrentIncarnation {
            workspace_id: workspaces[2],
            vm_id: vms[2],
            epoch: 1,
        }],
        1,
    )
    .await;
    cp.command(
        "healthy-status",
        method::VM_STATUS,
        "healthy-status",
        VmStatus {
            workspace_id: workspaces[2],
            vm_id: vms[2],
        },
    )
    .await;
    let mut status = false;
    let mut heartbeat = false;
    let mut cleaned = [false; 3];
    tokio::time::timeout(Duration::from_secs(10), async {
        while !status || !heartbeat || !cleaned[1] || !cleaned[2] {
            match cp.read().await {
                RpcMessage::Response(response) if response.id == "healthy-status" => {
                    assert!(matches!(
                        response.result_as::<CommandResult>().unwrap(),
                        CommandResult::Status { .. }
                    ));
                    status = true;
                }
                RpcMessage::Notification(notification)
                    if notification.method == method::VM_EVENT =>
                {
                    let event = notification.params::<VmEvent>().unwrap();
                    if matches!(event.kind, VmEventKind::StaleKilled) {
                        cleaned[vms.iter().position(|id| *id == event.vm_id).unwrap()] = true;
                    }
                    assert!(
                        !(event.vm_id == vms[0]
                            && matches!(
                                event.kind,
                                VmEventKind::Exited { .. } | VmEventKind::StaleKilled
                            ))
                    );
                    cp.ack(&event).await;
                }
                RpcMessage::Request(request) if request.method == method::HOST_HEARTBEAT => {
                    heartbeat = true;
                    cp.send(&RpcMessage::Response(
                        RpcResponse::result(
                            request.id,
                            &HeartbeatAck {
                                incarnation_generation: 2,
                                current_incarnations: Vec::new(),
                                source_preparation: None,
                            },
                        )
                        .unwrap(),
                    ))
                    .await;
                }
                other => panic!("unexpected traffic: {other:?}"),
            }
        }
    })
    .await
    .expect("cleanup, commands, event replay and heartbeats progress without A's exit proof");
    assert_eq!(runtime.inventory().await.len(), 1);
    assert_eq!(runtime.inventory().await[0].state, HostVmState::Stopping);
    assert!(host
        .fixture
        .workspaces_root
        .join(workspaces[0].to_string())
        .is_dir());
    drop(cp);
    assert_eq!(
        session.await.unwrap().unwrap(),
        SessionOutcome::Disconnected
    );
    let (mut cp, session) = host.connect().await;
    let (hello_id, hello) = cp.expect_hello().await;
    assert_eq!(hello.inventory[0].vm_id, vms[0]);
    cp.accept(hello_id, 3, Vec::new()).await;
    cp.command(
        "reconnected-status",
        method::VM_STATUS,
        "reconnected-status",
        VmStatus {
            workspace_id: workspaces[0],
            vm_id: vms[0],
        },
    )
    .await;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), cp.response("reconnected-status"))
            .await
            .unwrap()
            .0,
        CommandResult::Status { .. }
    ));
    host.launcher.fail_stop_for(&stuck_handle, false).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while runtime.active_count().await != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    drop(cp);
    assert_eq!(
        session.await.unwrap().unwrap(),
        SessionOutcome::Disconnected
    );
}

#[tokio::test]
async fn duplicate_unproven_stops_leave_command_capacity_across_reconnect() {
    use g5_internal_api::host_ctl::MAX_INFLIGHT_COMMANDS;
    let Some((_temp, host, runtime)) = progress_host(1).await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    runtime
        .seed_running_vm(host.fixture.start(workspace_id, vm_id, 1).spec)
        .await;
    host.launcher.fail_stops(true);
    let current = vec![CurrentIncarnation {
        workspace_id,
        vm_id,
        epoch: 1,
    }];
    let stop = VmStop {
        workspace_id,
        vm_id,
        mode: StopMode::Kill,
        grace_secs: 0,
    };
    let (mut cp, mut session) = host.connect().await;
    let (hello_id, _) = cp.expect_hello().await;
    cp.accept(hello_id, 1, current.clone()).await;
    cp.command("original-stop", method::VM_STOP, "stop-key", stop.clone())
        .await;
    while host.launcher.stopped().await.is_empty() {
        tokio::task::yield_now().await;
    }
    for round in 0..2 {
        for i in 0..MAX_INFLIGHT_COMMANDS * 2 {
            let id = format!("retry-{round}-{i}");
            cp.command(&id, method::VM_STOP, "stop-key", stop.clone())
                .await;
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), cp.response(&id))
                    .await
                    .expect("duplicate does not wait for exit")
                    .0,
                CommandResult::Busy
            );
        }
        let mut changed = stop.clone();
        changed.grace_secs = 1;
        cp.command("changed", method::VM_STOP, "stop-key", changed)
            .await;
        assert_eq!(
            cp.response("changed").await.0,
            CommandResult::IdempotencyConflict
        );
        cp.command(
            "status",
            method::VM_STATUS,
            &format!("status-{round}"),
            VmStatus {
                workspace_id,
                vm_id,
            },
        )
        .await;
        assert!(matches!(
            cp.response("status").await.0,
            CommandResult::Status { .. }
        ));
        assert_eq!(runtime.active_count().await, 1);
        if round == 0 {
            drop(cp);
            assert_eq!(
                session.await.unwrap().unwrap(),
                SessionOutcome::Disconnected
            );
            (cp, session) = host.connect().await;
            let (hello_id, _) = cp.expect_hello().await;
            cp.accept(hello_id, 2, current.clone()).await;
        }
    }
    host.launcher.fail_stops(false);
    tokio::time::timeout(Duration::from_secs(10), async {
        while runtime.active_count().await != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    // The original command completes on its old session and retains its result.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            cp.command("completed", method::VM_STOP, "stop-key", stop.clone())
                .await;
            match cp.response("completed").await.0 {
                CommandResult::Busy => tokio::task::yield_now().await,
                CommandResult::Stopped { .. } => break,
                other => panic!("expected the retained stop result, got {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        host.journal
            .pending()
            .await
            .iter()
            .filter(|e| matches!(e.kind, VmEventKind::Exited { .. }))
            .count(),
        1
    );
    drop(cp);
    assert_eq!(
        session.await.unwrap().unwrap(),
        SessionOutcome::Disconnected
    );
}

/// Distinct admitted commands keep their capacity when the session disappears;
/// reconnect cannot spend a fresh budget while the old executions still run.
#[tokio::test]
async fn reconnect_preserves_the_budget_of_distinct_unproven_stops() {
    use g5_internal_api::host_ctl::MAX_INFLIGHT_COMMANDS;
    let Some((_temp, host, runtime)) = progress_host(MAX_INFLIGHT_COMMANDS as u32).await else {
        return;
    };
    let mut current = Vec::new();
    for _ in 0..MAX_INFLIGHT_COMMANDS {
        let incarnation = CurrentIncarnation {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            epoch: 1,
        };
        runtime
            .seed_running_vm(
                host.fixture
                    .start(incarnation.workspace_id, incarnation.vm_id, 1)
                    .spec,
            )
            .await;
        current.push(incarnation);
    }
    host.launcher.fail_stops(true);
    let (mut cp, session) = host.connect().await;
    let (hello_id, _) = cp.expect_hello().await;
    cp.accept(hello_id, 1, current.clone()).await;
    for (i, vm) in current.iter().enumerate() {
        cp.command(
            &format!("stop-{i}"),
            method::VM_STOP,
            &format!("stop-{i}"),
            VmStop {
                workspace_id: vm.workspace_id,
                vm_id: vm.vm_id,
                mode: StopMode::Kill,
                grace_secs: 0,
            },
        )
        .await;
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while host.launcher.stopped().await.len() < MAX_INFLIGHT_COMMANDS {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all distinct commands execute before disconnect");
    drop(cp);
    assert_eq!(
        session.await.unwrap().unwrap(),
        SessionOutcome::Disconnected
    );
    let (mut cp, session) = host.connect().await;
    let (hello_id, _) = cp.expect_hello().await;
    cp.accept(hello_id, 2, current).await;
    // This absent VM would return NotFound if admission ran it. Busy proves
    // the old session still occupies the processor's entire execution budget.
    let extra = VmStop {
        workspace_id: Uuid::new_v4(),
        vm_id: Uuid::new_v4(),
        mode: StopMode::Kill,
        grace_secs: 0,
    };
    cp.command("extra", method::VM_STOP, "extra", extra.clone())
        .await;
    assert_eq!(cp.response("extra").await.0, CommandResult::Busy);
    host.launcher.fail_stops(false);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            cp.command(
                "extra-after-release",
                method::VM_STOP,
                "extra",
                extra.clone(),
            )
            .await;
            match cp.response("extra-after-release").await.0 {
                CommandResult::Busy => tokio::time::sleep(Duration::from_millis(5)).await,
                CommandResult::NotFound => break,
                other => panic!("unexpected command admission result: {other:?}"),
            }
        }
        while runtime.active_count().await != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("released stops restore capacity on the new session");
    drop(cp);
    assert_eq!(
        session.await.unwrap().unwrap(),
        SessionOutcome::Disconnected
    );
}
