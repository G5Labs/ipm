//! Two API replicas share PostgreSQL; only one has the host's reverse tunnel.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use g5_api::services::AppState;
use g5_internal_api::stream_mux::{Channel, Mux};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::helpers::{self, TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};

/// Controlled guest: acceptance is keyed by execution identity and produces a
/// durable journal even when its invoke response is lost. Native process
/// deduplication itself is covered by g5d's real-dispatcher regression.
#[derive(Default)]
struct Guest {
    executions: HashMap<String, Value>,
    invokes: Vec<Value>,
    drop_invoke_replies: usize,
}

struct TaskPeer(Vec<JoinHandle<()>>);

impl Drop for TaskPeer {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

impl TaskPeer {
    async fn connect(
        state: &Arc<AppState>,
        host_id: Uuid,
        vm_id: Uuid,
        guest: Arc<Mutex<Guest>>,
    ) -> Self {
        let (api_io, host_io) = tokio::io::duplex(256 * 1024);
        let api_mux = Mux::new(api_io, true);
        let mut host_mux = Mux::new(host_io, false);
        let registry = state.preview_tunnels.clone();
        let attached = registry.clone();
        let old = registry.get(&host_id);
        let api = tokio::spawn(async move { attached.attach_test_mux(host_id, api_mux).await });
        let host = tokio::spawn(async move {
            while let Some(mut stream) = host_mux.accept().await {
                assert_eq!(stream.channel(), Channel::Rpc);
                assert_eq!(stream.recv().await.unwrap(), vm_id.as_bytes());
                let (writer, mut reader) = stream.into_split();
                let request: Value = serde_json::from_str(
                    &reader
                        .read_line_with_limit(g5d_dispatch::MAX_RPC_BYTES)
                        .await
                        .unwrap(),
                )
                .unwrap();
                let params = &request["params"];
                let exec = params["exec_id"].as_str().unwrap_or_default();
                let mut guest = guest.lock().await;
                let result = match request["method"].as_str().unwrap() {
                    "modules.invoke" => {
                        guest.invokes.push(params.clone());
                        let existed = guest.executions.contains_key(exec);
                        if !existed {
                            guest.executions.insert(exec.into(), params.clone());
                        }
                        if guest.drop_invoke_replies > 0 {
                            guest.drop_invoke_replies -= 1;
                            writer.close().await;
                            continue;
                        }
                        json!({"disposition": if existed { "already_running" } else { "started" }})
                    }
                    "events.replay" => {
                        let from = params["from_seq"].as_u64().unwrap();
                        let events: Vec<Value> = guest.executions.get(exec).map(|invoke| {
                            let mut events = Vec::new();
                            let initial = &invoke["input"]["task"]["initial_input"];
                            if initial.is_object() {
                                events.push(json!({"seq": 1, "kind": "input.accepted", "payload": initial}));
                            }
                            events.push(json!({"seq": events.len() + 1, "kind": "invocation.outcome",
                                "payload": {"status":"succeeded", "result":{"status":"succeeded", "provider_started":true}},
                                "terminal":true}));
                            events.into_iter().filter(|event| event["seq"].as_u64().unwrap() >= from).collect()
                        }).unwrap_or_default();
                        json!({"events":events})
                    }
                    "events.ack" => json!({"acked":params["up_to_seq"]}),
                    "modules.status" => {
                        json!({"status": if guest.executions.contains_key(exec) { "running" } else { "unknown" }})
                    }
                    other => panic!("unexpected RPC: {other}"),
                };
                drop(guest);
                let reply = json!({"jsonrpc":"2.0", "id":request["id"], "result":result});
                writer.send(format!("{reply}\n").as_bytes()).await.unwrap();
                writer.close().await;
            }
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if registry.get(&host_id).is_some_and(|current| {
                    old.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &current))
                }) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        Self(vec![api, host])
    }
}

fn replica(app: &TestApp) -> Arc<AppState> {
    let config = helpers::test_config();
    let mut state = AppState::new(
        app.state.system_pool.clone(),
        app.state.tenant_pool.clone(),
        &config,
        Arc::new(g5_api::ws::WsHub::from_app_config(&config)),
        app.state.authz.clone(),
        app.repo_storage.clone(),
    );
    state.jwt_minter = app.state.jwt_minter.clone();
    Arc::new(state)
}

async fn vm_service(
    app: &TestApp,
    state: &Arc<AppState>,
    owner: Uuid,
    org: Uuid,
) -> g5_api::services::WorkspaceVmService {
    let db = app.scoped_db_for_user_org(owner, org).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
        system_pool: state.system_pool.clone(),
        scope,
        hosts: state.preview_tunnels.clone(),
        minter: app.state.jwt_minter.clone().unwrap(),
        runtime: state.workspace_runtime.clone(),
        agent_work: state.agent_work.clone(),
        repo_storage: state.repo_storage.clone(),
        git_external_url: state.git_external_url.clone(),
        ws_hub: state.event_bus.ws_hub().clone(),
    })
}

#[tokio::test]
async fn wrong_replica_cannot_reserve_a_vm_or_exclude_its_connected_host() {
    use g5_api::services::workspace_vm_service::{ResumeOutcome, StartKind, StopReason};
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let connected = vm_service(&app, &app.state, owner.id, org.id).await;
    connected
        .stop(
            workspace.id,
            owner.id,
            Uuid::new_v4(),
            StopReason::User,
            None,
        )
        .await
        .unwrap();
    let epoch: i64 = sqlx::query_scalar("SELECT epoch FROM workspace_vms WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let foreign = replica(&app);
    let disconnected = vm_service(&app, &foreign, owner.id, org.id).await;
    let intent = Uuid::new_v4();
    assert!(matches!(
        disconnected
            .resume(workspace.id, owner.id, intent, StartKind::Resume)
            .await
            .unwrap(),
        ResumeOutcome::QueuedForCapacity { .. }
    ));
    let (after_epoch, consent): (i64, Value) =
        sqlx::query_as("SELECT epoch, start_intent FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        after_epoch, epoch,
        "a foreign API cannot allocate a VM incarnation"
    );
    assert_eq!(consent["id"], intent.to_string());
    assert!(
        consent["host_refusals"].is_null(),
        "foreign API excluded a healthy host: {consent}"
    );
    assert!(matches!(
        connected
            .resume_queued(workspace.id, owner.id, intent, StartKind::Resume)
            .await
            .unwrap(),
        ResumeOutcome::Starting { .. }
    ));
    assert_eq!(peer.received_host_command_params("vm.start").await.len(), 1);
    app.cleanup().await;
}

#[tokio::test]
async fn wrong_replica_cannot_postpone_starting_vm_status_reconciliation() {
    use g5_api::services::workspace_vm_service::{ResumeOutcome, StartKind, StopReason};
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let connected = vm_service(&app, &app.state, owner.id, org.id).await;
    connected
        .stop(
            workspace.id,
            owner.id,
            Uuid::new_v4(),
            StopReason::User,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(
        connected
            .resume(workspace.id, owner.id, Uuid::new_v4(), StartKind::Resume)
            .await
            .unwrap(),
        ResumeOutcome::Starting { .. }
    ));
    let deadline: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "UPDATE workspace_vms SET start_deadline_at=now()-interval '1 second'
         WHERE workspace_id=$1 RETURNING start_deadline_at",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let foreign = replica(&app);
    let disconnected = vm_service(&app, &foreign, owner.id, org.id).await;
    disconnected.reconcile_start_queue(1).await.unwrap();
    let after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT start_deadline_at FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        after, deadline,
        "a foreign API cannot postpone the capable API's status check"
    );
    let before = peer.received_host_command_params("vm.status").await.len();
    connected.reconcile_start_queue(1).await.unwrap();
    assert_eq!(
        peer.received_host_command_params("vm.status").await.len(),
        before + 1
    );
    app.cleanup().await;
}

#[tokio::test]
async fn unsent_placement_loss_allows_immediate_control_transfer_without_host_exclusion() {
    use g5_api::services::worker_host_service::{PlacementOutcome, PlacementRequest};
    use g5_api::services::workspace_vm_service::{ResumeOutcome, StartKind, StopReason};
    use g5_internal_api::host_ctl::{method, CommandResult};
    use g5_internal_api::host_ctl_test_support::{ScriptedHostFaults, ScriptedHostPeer};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    workspace.attach_parent(&app).await;
    let original_peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let host_id = original_peer.host_id();
    let connected = vm_service(&app, &app.state, owner.id, org.id).await;
    connected
        .stop(
            workspace.id,
            owner.id,
            Uuid::new_v4(),
            StopReason::User,
            None,
        )
        .await
        .unwrap();
    let foreign = replica(&app);
    let next = vm_service(&app, &foreign, owner.id, org.id).await;
    let intent = Uuid::new_v4();
    assert!(matches!(
        next.resume(workspace.id, owner.id, intent, StartKind::Resume)
            .await
            .unwrap(),
        ResumeOutcome::QueuedForCapacity { .. }
    ));

    // Stop at the real placement boundary, then lose control before sending
    // Start. The release must not invent a refusal from a host never called.
    let mut db = app.scoped_db_for_org(org.id).await;
    let PlacementOutcome::Reserved(reservation) = app
        .state
        .worker_host_service
        .place(
            &mut db,
            PlacementRequest {
                org_id: org.id,
                workspace_id: workspace.id,
                region: "us-central1",
                reachable_host_ids: &[host_id],
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .unwrap()
    else {
        panic!("connected host has capacity")
    };
    db.commit().await.unwrap();
    assert!(app.state.preview_tunnels.terminate(&host_id).await);
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(app
        .state
        .worker_host_service
        .release_placement(
            &mut db,
            &reservation,
            org.id,
            workspace.id,
            false,
            "host_tunnel_unavailable",
            None,
        )
        .await
        .unwrap());
    db.commit().await.unwrap();
    let consent: Value =
        sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(consent["id"], intent.to_string());
    assert!(consent["host_refusals"].is_null(), "{consent}");

    // A tunnel alone is not sufficient for placement: its control hello
    // must have installed a command client before a Start can be admitted.
    let _rpc = TaskPeer::connect(&foreign, host_id, reservation.vm_id, Arc::default()).await;
    assert!(matches!(
        next.resume_queued(workspace.id, owner.id, intent, StartKind::Resume)
            .await
            .unwrap(),
        ResumeOutcome::QueuedForCapacity { .. }
    ));
    let epoch: i64 = sqlx::query_scalar("SELECT epoch FROM workspace_vms WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(epoch as u64, reservation.epoch);

    assert!(foreign.install_host_control_backend());
    let peer = ScriptedHostPeer::new(host_id, 1);
    let (api_io, host_io) = tokio::io::duplex(256 * 1024);
    let api_mux = Mux::new(api_io, true);
    let host_mux = Mux::new(host_io, false);
    let control = host_mux.open(Channel::Ctl).await.unwrap();
    let attached = foreign.preview_tunnels.clone();
    let api = tokio::spawn(async move { attached.attach_test_mux(host_id, api_mux).await });
    let host = tokio::spawn({
        let peer = peer.clone();
        async move {
            let _host_mux = host_mux;
            let _ = peer
                .run_control_stream(
                    control,
                    ScriptedHostFaults {
                        command_results: HashMap::from([(
                            method::VM_START.to_string(),
                            CommandResult::Started,
                        )]),
                        ..Default::default()
                    },
                )
                .await;
        }
    });
    let _control = TaskPeer(vec![api, host]);
    tokio::time::timeout(Duration::from_secs(2), async {
        while foreign
            .preview_tunnels
            .get(&host_id)
            .and_then(|host| host.command_client())
            .is_none()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        next.resume_queued(workspace.id, owner.id, intent, StartKind::Resume)
            .await
            .unwrap(),
        ResumeOutcome::Starting { .. }
    ));
    let starts: Vec<_> = peer
        .received_commands()
        .await
        .into_iter()
        .filter(|command| command.method == method::VM_START)
        .collect();
    assert_eq!(starts.len(), 1);
    assert!(original_peer
        .received_host_command_params("vm.start")
        .await
        .is_empty());
    let (epoch, vm_id): (i64, String) =
        sqlx::query_as("SELECT epoch, vm_id FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(epoch as u64, reservation.epoch + 1);
    assert_ne!(vm_id, reservation.vm_id.to_string());
    app.cleanup().await;
}

async fn personal(app: &TestApp, owner: &TestUser, org: &TestOrg) -> TestWorkspace {
    let response = app
        .authenticated_client(owner)
        .get(&format!("/api/v1/orgs/{}/me", org.id))
        .await;
    assert!(response.status().is_success());
    let body: Value = response.json().await.unwrap();
    let data = &body["data"];
    TestWorkspace {
        id: data["workspaceId"].as_str().unwrap().parse().unwrap(),
        org_id: org.id,
        repo_id: data["repoId"].as_str().unwrap().parse().unwrap(),
        name: data["name"].as_str().unwrap().into(),
        head_commit: String::new(),
    }
}

async fn queue(app: &TestApp, owner: &TestUser, workspace: &TestWorkspace) -> (Uuid, Uuid) {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/sessions/start", workspace.id),
            &json!({"content": "E2E: say hello from my personal workspace", "startCompute": true}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert!(status.is_success(), "{status}: {body}");
    (
        body["data"]["taskId"].as_str().unwrap().parse().unwrap(),
        body["data"]["message"]["inputId"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    )
}

async fn dispatch_state(app: &TestApp, task_id: Uuid) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('task', to_jsonb(t), 'input', to_jsonb(i),
                                  'attempt', to_jsonb(a))
         FROM tasks t JOIN session_inputs i ON i.task_id = t.id
         LEFT JOIN task_attempts a ON a.task_id = t.id AND a.attempt = t.attempt
         WHERE t.id = $1",
    )
    .bind(task_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn age_retry(app: &TestApp, task_id: Uuid) {
    sqlx::query("UPDATE tasks SET updated_at = now() - interval '3 seconds' WHERE id = $1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .unwrap();
}

/// Journal application is asynchronous since `g5.chat.live-delivery`: the
/// replica that claimed the attempt stores its journal in batches after the
/// pass. Wait for the attempt to settle in the database, whichever replica's
/// follower does it, before judging the outcome.
async fn await_follower(app: &TestApp, task_id: Uuid) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let settled: Option<bool> = sqlx::query_scalar(
            "SELECT a.ended_at IS NOT NULL FROM tasks t
             JOIN task_attempts a ON a.task_id = t.id AND a.attempt = t.attempt
             WHERE t.id = $1",
        )
        .bind(task_id)
        .fetch_optional(&app.db)
        .await
        .unwrap();
        if settled == Some(true) || settled.is_none() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the follower stores the journal and settles the attempt: {}",
            sqlx::query_scalar::<_, Value>("SELECT to_jsonb(t) FROM tasks t WHERE id = $1")
                .bind(task_id)
                .fetch_one(&app.db)
                .await
                .unwrap()
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn assert_published_once(app: &TestApp, task_id: Uuid, input_id: Uuid) {
    await_follower(app, task_id).await;
    let state = dispatch_state(app, task_id).await;
    assert_eq!(state["input"]["delivery"], "delivered", "{state}");
    assert_eq!(state["task"]["status"], "succeeded", "{state}");
    let messages: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM chat_messages WHERE task_id = $1 AND role = 'user' AND content = $2",
    )
    .bind(task_id)
    .bind("E2E: say hello from my personal workspace")
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(messages, 1, "input {input_id} publishes exactly once");
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE task_id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(attempts, 1);
}

#[tokio::test]
async fn wrong_replica_first_cannot_reserve_personal_input_or_delay_its_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = personal(&app, &owner, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let guest = Arc::new(Mutex::new(Guest::default()));
    let peer = TaskPeer::connect(
        &app.state,
        runtime.host_id(),
        runtime.vm_id(),
        guest.clone(),
    )
    .await;
    let foreign = replica(&app);
    let (task_id, input_id) = queue(&app, &owner, &workspace).await;
    let before = dispatch_state(&app, task_id).await;

    g5_api::maintenance::agent_tasks::run_once(&foreign)
        .await
        .unwrap();
    assert_eq!(
        dispatch_state(&app, task_id).await,
        before,
        "an API without the host route must not reserve input or advance shared retry state"
    );

    g5_api::maintenance::agent_tasks::run_once(&app.state)
        .await
        .unwrap();
    assert_eq!(
        guest.lock().await.invokes.len(),
        1,
        "the owner dispatches in its first pass"
    );
    assert_eq!(
        dispatch_state(&app, task_id).await["task"]["start_confirmed"],
        true
    );
    let reserved = dispatch_state(&app, task_id).await;
    g5_api::maintenance::agent_tasks::run_once(&foreign)
        .await
        .unwrap();
    assert_eq!(
        dispatch_state(&app, task_id).await,
        reserved,
        "a foreign replica must not claim a live attempt for reconciliation"
    );
    for _ in 0..2 {
        g5_api::maintenance::agent_tasks::run_once(&app.state)
            .await
            .unwrap();
    }
    assert_published_once(&app, task_id, input_id).await;
    drop(peer);
    drop(runtime);
    drop(foreign);
    app.cleanup().await;
}

#[tokio::test]
async fn wrong_replica_cannot_renew_an_unconfirmed_retry_and_competing_passes_publish_once() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = personal(&app, &owner, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let guest = Arc::new(Mutex::new(Guest::default()));
    let peer = TaskPeer::connect(
        &app.state,
        runtime.host_id(),
        runtime.vm_id(),
        guest.clone(),
    )
    .await;
    let foreign = replica(&app);
    let (task_id, input_id) = queue(&app, &owner, &workspace).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    age_retry(&app, task_id).await;
    let before = dispatch_state(&app, task_id).await;
    for _ in 0..3 {
        g5_api::maintenance::agent_tasks::run_once(&foreign)
            .await
            .unwrap();
    }
    assert_eq!(
        dispatch_state(&app, task_id).await,
        before,
        "the wrong replica cannot renew retry or reconciliation timestamps"
    );
    let (a, b, c) = tokio::join!(
        g5_api::maintenance::agent_tasks::run_once(&app.state),
        g5_api::maintenance::agent_tasks::run_once(&foreign),
        g5_api::maintenance::agent_tasks::run_once(&app.state),
    );
    a.unwrap();
    b.unwrap();
    c.unwrap();
    g5_api::maintenance::agent_tasks::run_once(&app.state)
        .await
        .unwrap();
    assert_published_once(&app, task_id, input_id).await;
    let g = guest.lock().await;
    assert_eq!(g.executions.len(), 1);
    assert!(g
        .invokes
        .iter()
        .all(|params| params["exec_id"] == dispatch.exec_id));
    drop(g);
    drop(peer);
    drop(runtime);
    drop(foreign);
    app.cleanup().await;
}

#[tokio::test]
async fn connection_loss_after_reservation_recovers_on_the_new_replica_with_the_same_identity() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = personal(&app, &owner, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let foreign = replica(&app);
    let (task_id, input_id) = queue(&app, &owner, &workspace).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    app.state
        .preview_tunnels
        .terminate(&runtime.host_id())
        .await;
    assert!(app
        .state
        .task_service
        .invoke_agent_dispatch(&dispatch)
        .await
        .is_err());
    age_retry(&app, task_id).await;
    let before = dispatch_state(&app, task_id).await;
    g5_api::maintenance::agent_tasks::run_once(&app.state)
        .await
        .unwrap();
    g5_api::maintenance::agent_tasks::run_once(&foreign)
        .await
        .unwrap();
    assert_eq!(
        dispatch_state(&app, task_id).await,
        before,
        "disconnection must not consume input or establish VM loss"
    );
    let guest = Arc::new(Mutex::new(Guest::default()));
    let peer = TaskPeer::connect(&foreign, runtime.host_id(), runtime.vm_id(), guest.clone()).await;
    g5_api::maintenance::agent_tasks::run_once(&app.state)
        .await
        .unwrap();
    for _ in 0..2 {
        g5_api::maintenance::agent_tasks::run_once(&foreign)
            .await
            .unwrap();
    }
    assert_published_once(&app, task_id, input_id).await;
    assert_eq!(guest.lock().await.invokes[0]["exec_id"], dispatch.exec_id);
    drop(peer);
    drop(runtime);
    drop(foreign);
    app.cleanup().await;
}

#[tokio::test]
async fn accepted_invoke_with_lost_replies_is_reconciled_once_by_the_connected_replica() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = personal(&app, &owner, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let guest = Arc::new(Mutex::new(Guest {
        drop_invoke_replies: 5,
        ..Guest::default()
    }));
    let peer = TaskPeer::connect(
        &app.state,
        runtime.host_id(),
        runtime.vm_id(),
        guest.clone(),
    )
    .await;
    let foreign = replica(&app);
    let (task_id, input_id) = queue(&app, &owner, &workspace).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    db.commit().await.unwrap();
    assert!(app
        .state
        .task_service
        .invoke_agent_dispatch(&dispatch)
        .await
        .is_err());
    let before = dispatch_state(&app, task_id).await;
    assert_eq!(before["task"]["start_confirmed"], false);
    assert_eq!(before["input"]["delivery"], "reserved");
    g5_api::maintenance::agent_tasks::run_once(&foreign)
        .await
        .unwrap();
    assert_eq!(dispatch_state(&app, task_id).await, before);
    for _ in 0..2 {
        g5_api::maintenance::agent_tasks::run_once(&app.state)
            .await
            .unwrap();
    }
    assert_published_once(&app, task_id, input_id).await;
    let g = guest.lock().await;
    assert_eq!(g.executions.len(), 1);
    assert_eq!(
        g.invokes.len(),
        5,
        "runtime retries the lost transport response with the same identity"
    );
    assert!(g
        .invokes
        .windows(2)
        .all(|pair| pair[0]["exec_id"] == pair[1]["exec_id"]));
    drop(g);
    drop(peer);
    drop(runtime);
    drop(foreign);
    app.cleanup().await;
}

#[tokio::test]
async fn command_and_preview_claims_also_require_the_host_route() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let guest = Arc::new(Mutex::new(Guest::default()));
    let peer = TaskPeer::connect(
        &app.state,
        runtime.host_id(),
        runtime.vm_id(),
        guest.clone(),
    )
    .await;
    let foreign = replica(&app);
    for kind in ["command", "preview"] {
        let task_id =
            crate::api_task_lifecycle::insert_queued_preview(&app, &workspace, owner.id).await;
        if kind == "command" {
            // The current executable command envelope is workspace.preview;
            // both persisted task kinds go through command reservation/retry.
            sqlx::query("UPDATE tasks SET kind = 'command' WHERE id = $1")
                .bind(task_id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        g5_api::maintenance::agent_tasks::run_once(&foreign)
            .await
            .unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(
            status, "queued",
            "{kind} remains available for its host's API"
        );
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_command_dispatch(&mut db)
            .await
            .unwrap()
            .unwrap();
        db.commit().await.unwrap();
        age_retry(&app, task_id).await;
        let before: Value = sqlx::query_scalar("SELECT to_jsonb(t) FROM tasks t WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
        g5_api::maintenance::agent_tasks::run_once(&foreign)
            .await
            .unwrap();
        let after: Value = sqlx::query_scalar("SELECT to_jsonb(t) FROM tasks t WHERE id = $1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(after, before, "{kind} retry must not be postponed");
        for _ in 0..2 {
            g5_api::maintenance::agent_tasks::run_once(&app.state)
                .await
                .unwrap();
        }
        await_follower(&app, task_id).await;
        let state: (String, i64) = sqlx::query_as("SELECT status, (SELECT count(*) FROM task_attempts WHERE task_id = $1) FROM tasks WHERE id = $1")
            .bind(task_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(state, ("succeeded".into(), 1));
        assert!(guest
            .lock()
            .await
            .executions
            .contains_key(&dispatch.exec_id));
    }
    drop(peer);
    drop(runtime);
    drop(foreign);
    app.cleanup().await;
}

#[tokio::test]
async fn disconnected_replicas_still_settle_stopped_replaced_and_expired_incarnations() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let foreign = replica(&app);
    for mode in ["stopped", "replaced", "expired"] {
        let workspace = TestWorkspace::create(&app, &org).await;
        let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
        let (task_id, _) = queue(&app, &owner, &workspace).await;
        let mut db = app.scoped_db_for_org(org.id).await;
        let dispatch = app
            .state
            .task_service
            .reserve_agent_dispatch(&mut db)
            .await
            .unwrap()
            .unwrap();
        db.commit().await.unwrap();
        if mode == "stopped" {
            sqlx::query("UPDATE workspace_vms SET status = 'stopping' WHERE workspace_id = $1")
                .bind(workspace.id)
                .execute(&app.db)
                .await
                .unwrap();
            assert!(
                app.state
                    .task_service
                    .invoke_agent_dispatch(&dispatch)
                    .await
                    .is_err(),
                "Stop fences invocation"
            );
            sqlx::query("UPDATE workspace_vms SET status = 'stopped', vm_id = NULL, host_id = NULL, lease_expires_at = NULL WHERE workspace_id = $1")
                .bind(workspace.id)
                .execute(&app.db)
                .await
                .unwrap();
        } else if mode == "replaced" {
            sqlx::query(
                "UPDATE workspace_vms SET vm_id = $2, epoch = epoch + 1 WHERE workspace_id = $1",
            )
            .bind(workspace.id)
            .bind(Uuid::new_v4().to_string())
            .execute(&app.db)
            .await
            .unwrap();
        } else {
            sqlx::query("UPDATE workspace_vms SET lease_expires_at = now() - interval '1 second' WHERE workspace_id = $1")
                .bind(workspace.id).execute(&app.db).await.unwrap();
        }
        assert!(app
            .state
            .task_service
            .invoke_agent_dispatch(&dispatch)
            .await
            .is_err());
        g5_api::maintenance::agent_tasks::run_once(&foreign)
            .await
            .unwrap();
        let state = dispatch_state(&app, task_id).await;
        assert!(!state["attempt"]["ended_at"].is_null(), "{mode}: {state}");
        assert_ne!(state["input"]["delivery"], "reserved", "{mode}: {state}");
        assert_eq!(
            runtime.invoke_params().await.len(),
            0,
            "no stale-epoch execution"
        );
        drop(runtime);
    }
    drop(foreign);
    app.cleanup().await;
}
