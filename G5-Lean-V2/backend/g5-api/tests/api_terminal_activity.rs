//! Owner-input activity, background recovery and idle reclamation over real
//! HTTP/WebSocket routes and persisted VM state.
use crate::helpers;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

async fn terminal_ticket(app: &TestApp, owner: &TestUser, workspace_id: Uuid) -> String {
    let response = app
        .authenticated_client(owner)
        .post_json(
            "/api/v1/auth/ws-ticket",
            &json!({"workspaceId": workspace_id}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"]["ticket"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn age_activity(app: &TestApp, workspace_id: Uuid) -> DateTime<Utc> {
    sqlx::query_scalar("UPDATE workspace_vms SET started_at = now() - interval '10 minutes',
        last_activity_at = now() - interval '10 minutes' WHERE workspace_id = $1 RETURNING last_activity_at")
        .bind(workspace_id).fetch_one(&app.db).await.unwrap()
}

async fn activity_at(app: &TestApp, workspace_id: Uuid) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT last_activity_at FROM workspace_vms WHERE workspace_id = $1")
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn terminal(
    app: &TestApp,
    owner: &TestUser,
    workspace: &TestWorkspace,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let ticket = terminal_ticket(app, owner, workspace.id).await;
    connect_async(format!(
        "{}/api/v1/workspaces/{}/terminal?token={ticket}&cols=80&rows=24",
        app.ws_url, workspace.id
    ))
    .await
    .unwrap()
    .0
}

async fn wait_for_input_activity(app: &TestApp, id: Uuid, previous: DateTime<Utc>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while activity_at(app, id).await == previous {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("accepted owner input renews the timestamp");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quiet_terminal_reconnect_control_output_and_invalid_frames_do_not_renew() {
    std::env::set_var("G5_TERMINAL_KEEPALIVE_SECS", "1");
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let previous = age_activity(&app, workspace.id).await;

    for _ in 0..2 {
        let socket = terminal(&app, &owner, &workspace).await;
        let mut guest = runtime.take_pty_stream().await;
        // Exact incarnation preamble, followed by the server's open frame.
        assert_eq!(guest.recv().await.unwrap(), runtime.vm_id().as_bytes());
        assert!(String::from_utf8(guest.recv().await.unwrap())
            .unwrap()
            .contains("\"type\":\"open\""));
        let (mut write, mut read) = socket.split();
        let reader = tokio::spawn(async move {
            let mut pings = 0;
            while let Some(Ok(message)) = read.next().await {
                if matches!(message, Message::Ping(_)) {
                    pings += 1;
                }
                if pings >= 2 {
                    break;
                }
            }
            pings
        });
        for frame in [
            Message::Text(r#"{"type":"resize","cols":120,"rows":40}"#.into()),
            Message::Text(r#"{"type":"input","data":""}"#.into()),
            Message::Text(r#"{"type":"input","data":42}"#.into()),
            Message::Text(r#"{"type":"input","data":"x","unexpected":true}"#.into()),
            Message::Text(r#"{"type":"response","data":"\u001b[0n"}"#.into()),
            Message::Text("garbage".into()),
            Message::Text("".into()),
            Message::Binary(vec![]),
            Message::Binary(vec![0xff]),
            Message::Ping(vec![]),
            Message::Pong(vec![]),
        ] {
            write.send(frame).await.unwrap();
        }
        guest.send(b"background terminal output\r\n").await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), reader)
                .await
                .unwrap()
                .unwrap(),
            2
        );
        assert_eq!(activity_at(&app, workspace.id).await, previous);
        let _ = write.close().await;
    }

    // An open idle tab does not block the real lifecycle reconciler.
    sqlx::query("UPDATE workspace_settings SET idle_timeout_secs = 60 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let _quiet = terminal(&app, &owner, &workspace).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(service.reconcile_idle(10).await.unwrap(), 1);
    assert_eq!(service.view(workspace.id).await.unwrap().status, "stopped");
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|m| m == "vm.stop"));
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminal_keyboard_and_paste_renew_but_revoked_owner_cannot() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    for frame in [
        Message::Text(r#"{"type":"input","data":"\u0003"}"#.into()),
        Message::Binary(b"pasted command\n".to_vec()),
    ] {
        let previous = age_activity(&app, workspace.id).await;
        let mut socket = terminal(&app, &owner, &workspace).await;
        assert_eq!(
            activity_at(&app, workspace.id).await,
            previous,
            "open is not input"
        );
        socket.send(frame).await.unwrap();
        wait_for_input_activity(&app, workspace.id, previous).await;
        assert!(activity_at(&app, workspace.id).await > previous);
        let _ = socket.close(None).await;
    }
    let mut stale = terminal(&app, &owner, &workspace).await;
    let successor = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(successor.id)
        .execute(&app.db)
        .await
        .unwrap();
    let previous = age_activity(&app, workspace.id).await;
    stale
        .send(Message::Text(r#"{"type":"input","data":"x"}"#.into()))
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), stale.next())
        .await
        .unwrap();
    assert_eq!(activity_at(&app, workspace.id).await, previous);
    app.cleanup().await;
}

#[tokio::test]
async fn background_reads_and_rpc_retries_do_not_renew_explicit_gestures_do() {
    let events = helpers::tracing_capture();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime =
        TestWorkspaceRuntimePeer::attach_dropping_replies(&app, &workspace, owner.id, "fs.read", 2)
            .await;
    runtime
        .seed_file(
            workspace.repo_id,
            "test.txt",
            "contents must never be logged",
        )
        .await;
    let previous = age_activity(&app, workspace.id).await;
    let handle = app
        .state
        .workspace_runtime
        .owner_handle(workspace.id, owner.id)
        .await
        .unwrap();
    for _ in 0..3 {
        let ticket = app
            .authenticated_client(&owner)
            .post_json("/api/v1/auth/ws-ticket", &json!({}))
            .await
            .json::<Value>()
            .await
            .unwrap();
        let mut ws = helpers::WsTestClient::connect(&format!(
            "{}/api/v1/ws?token={}",
            app.ws_url,
            ticket["data"]["ticket"].as_str().unwrap()
        ))
        .await;
        ws.subscribe(&format!("workspace:{}", workspace.id)).await;
        ws.expect_subscribed().await;
        let file = app
            .authenticated_client(&owner)
            .get(&format!(
                "/api/v1/workspaces/{}/files/test.txt?repoId={}",
                workspace.id, workspace.repo_id
            ))
            .await;
        let status = file.status();
        assert_eq!(status, StatusCode::OK, "{}", file.text().await.unwrap());
        assert_eq!(
            app.authenticated_client(&owner)
                .get(&format!("/api/v1/workspaces/{}", workspace.id))
                .await
                .status(),
            StatusCode::OK
        );
        let _ = terminal_ticket(&app, &owner, workspace.id).await;
        ws.close().await;
        assert_eq!(activity_at(&app, workspace.id).await, previous);
    }
    assert_eq!(
        runtime.dropped_method_keys().await.len(),
        5,
        "the first read retried twice"
    );
    let path = format!("/api/v1/workspaces/{}/user-activity", workspace.id);
    for _ in 0..3 {
        let clock = app.authenticated_client(&owner).get(&path).await;
        assert_eq!(clock.status(), StatusCode::OK);
        assert_eq!(clock.headers()["cache-control"], "no-store");
        let time = clock.json::<Value>().await.unwrap()["data"]["serverTime"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap();
        assert!((Utc::now() - time).num_seconds().abs() < 5);
        assert_eq!(
            activity_at(&app, workspace.id).await,
            previous,
            "clock samples are passive"
        );
    }
    sqlx::query("UPDATE workspace_settings SET idle_timeout_secs = 60 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(
        service.idle_candidates(10).await.unwrap(),
        vec![workspace.id]
    );
    let gesture = json!({"vmId":handle.vm_id, "epoch":handle.epoch, "observedAt":Utc::now(), "reason":"pointer"});
    let client = app.authenticated_client(&owner);
    assert_eq!(
        client.post_json(&path, &gesture).await.status(),
        StatusCode::OK
    );
    let renewed = activity_at(&app, workspace.id).await;
    assert!(renewed > previous);
    let outcome = service
        .stop_as_system_for_tests(
            workspace.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::Idle,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        outcome.status, "running",
        "Stop rechecks a gesture that arrived after candidate selection"
    );
    assert_eq!(
        client.post_json(&path, &gesture).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        activity_at(&app, workspace.id).await,
        renewed,
        "replay cannot create new activity"
    );
    let changes: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| {
            e.field("event") == Some("workspace_activity_changed")
                && e.field("workspace_id") == Some(workspace.id.to_string().as_str())
        })
        .cloned()
        .collect();
    assert_eq!(
        changes.len(),
        1,
        "background RPCs and coalesced reports emit no change logs"
    );
    let change = &changes[0];
    assert_eq!(
        change.field("vm_id"),
        Some(handle.vm_id.to_string().as_str())
    );
    assert_eq!(change.field("epoch"), Some("1"));
    assert_eq!(change.field("activity_kind"), Some("user_activity"));
    assert_eq!(change.field("activity_reason"), Some("pointer"));
    assert!(change.field("correlation_id").unwrap().starts_with("req_"));
    assert_eq!(
        change.field("previous_last_activity_at"),
        Some(format!("{:?}", Some(previous)).as_str())
    );
    assert_eq!(
        change.field("new_last_activity_at"),
        Some(format!("{:?}", Some(renewed)).as_str())
    );
    assert!(!format!("{changes:?}").contains("contents must never be logged"));

    let previous = age_activity(&app, workspace.id).await;
    for invalid in [
        json!({"vmId":Uuid::new_v4(), "epoch":handle.epoch, "observedAt":Utc::now(), "reason":"pointer"}),
        json!({"vmId":handle.vm_id, "epoch":handle.epoch + 1, "observedAt":Utc::now(), "reason":"pointer"}),
        json!({"vmId":handle.vm_id, "epoch":handle.epoch, "observedAt":Utc::now()-chrono::Duration::minutes(1), "reason":"keyboard"}),
        json!({"vmId":handle.vm_id, "epoch":handle.epoch, "observedAt":Utc::now()+chrono::Duration::minutes(1), "reason":"pointer"}),
    ] {
        assert_eq!(
            client.post_json(&path, &invalid).await.status(),
            StatusCode::OK
        );
        assert_eq!(activity_at(&app, workspace.id).await, previous);
    }
    let forged_preview = json!({"vmId":handle.vm_id, "epoch":handle.epoch, "observedAt":Utc::now(), "reason":"preview_traffic"});
    assert!(client
        .post_json(&path, &forged_preview)
        .await
        .status()
        .is_client_error());
    assert_eq!(activity_at(&app, workspace.id).await, previous);
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| {
                event.field("event") == Some("workspace_activity_rejected")
                    && event.field("workspace_id") == Some(workspace.id.to_string().as_str())
                    && event.field("rejection_reason") == Some("observation_outside_window")
            })
            .count(),
        2
    );
    assert_eq!(
        app.authenticated_client(&reader).get(&path).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.authenticated_client(&reader)
            .post_json(&path, &gesture)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(activity_at(&app, workspace.id).await, previous);
    app.cleanup().await;
}

#[tokio::test]
async fn preview_http_traffic_renews_then_idles_after_traffic_and_work_end() {
    let events = helpers::tracing_capture();
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    const HTML: &str = "<!doctype html><html><head><meta http-equiv=Content-Security-Policy content=\"script-src 'self'; connect-src 'none'\"></head><body>hello</body></html>";
    let runtime = TestWorkspaceRuntimePeer::attach_with_preview_response(
        &app,
        &workspace,
        owner.id,
        b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\nconnection: close\r\ncontent-security-policy: script-src 'self'; connect-src 'none'\r\ncache-control: public, max-age=60\r\netag: \"app-etag\"\r\ncontent-length: 148\r\n\r\n<!doctype html><html><head><meta http-equiv=Content-Security-Policy content=\"script-src 'self'; connect-src 'none'\"></head><body>hello</body></html>",
    )
    .await;
    let task =
        crate::api_task_lifecycle::running_preview(&app, &workspace, owner.id, runtime.vm_id())
            .await;
    let previous = age_activity(&app, workspace.id).await;
    let host = format!("{task}.preview.g5.example");

    // G5 preview-status polling and session recovery are not proxied traffic.
    for _ in 0..3 {
        let status = app
            .authenticated_client(&owner)
            .get(&format!("/api/v1/workspaces/{}/preview", workspace.id))
            .await;
        assert_eq!(status.status(), StatusCode::OK);
        assert_eq!(activity_at(&app, workspace.id).await, previous);
    }
    let mint = app
        .authenticated_client(&owner)
        .post_json(&format!("/api/v1/preview/session/{task}"), &json!({}))
        .await;
    assert_eq!(mint.status(), StatusCode::OK);
    assert_eq!(activity_at(&app, workspace.id).await, previous);
    let unauthorized = app
        .client
        .get(&app.base_url)
        .header("host", &host)
        .send()
        .await
        .unwrap();
    assert!(unauthorized.status().is_client_error());
    assert_eq!(activity_at(&app, workspace.id).await, previous);

    let mut renewed = previous;
    for _ in 0..3 {
        // No Sec-Fetch-User: automatic preview reloads intentionally count.
        let response = app
            .client
            .get(&app.base_url)
            .header("host", &host)
            .header("Authorization", format!("Bearer {}", owner.jwt))
            .header("accept-encoding", "identity")
            .header("sec-fetch-mode", "navigate")
            .header("sec-fetch-dest", "iframe")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-security-policy"],
            "script-src 'self'; connect-src 'none'"
        );
        assert_eq!(response.headers()["cache-control"], "public, max-age=60");
        assert_eq!(response.headers()["etag"], "\"app-etag\"");
        assert_eq!(response.headers()["content-length"], HTML.len().to_string());
        assert_eq!(
            response.text().await.unwrap(),
            HTML,
            "no script or meta-CSP rewriting"
        );
        let current = activity_at(&app, workspace.id).await;
        if renewed == previous {
            assert!(current > previous);
            renewed = current;
        } else {
            assert_eq!(current, renewed, "new streams share the SQL throttle");
        }
    }
    let changes: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| {
            e.field("event") == Some("workspace_activity_changed")
                && e.field("workspace_id") == Some(workspace.id.to_string().as_str())
        })
        .cloned()
        .collect();
    assert_eq!(changes.len(), 1, "only actual timestamp changes are logged");
    let change = &changes[0];
    assert_eq!(change.field("activity_kind"), Some("preview_traffic"));
    assert_eq!(change.field("activity_reason"), Some("preview_traffic"));
    assert_eq!(
        change.field("vm_id"),
        Some(runtime.vm_id().to_string().as_str())
    );
    assert_eq!(change.field("epoch"), Some("1"));
    assert!(change.field("correlation_id").unwrap().starts_with("req_"));
    assert_eq!(
        change.field("previous_last_activity_at"),
        Some(format!("{:?}", Some(previous)).as_str())
    );
    assert_eq!(
        change.field("new_last_activity_at"),
        Some(format!("{:?}", Some(renewed)).as_str())
    );
    assert!(!format!("{changes:?}").contains(HTML));

    let service = workspace_vm_service(&app, owner.id, org.id).await;
    sqlx::query("UPDATE workspace_settings SET idle_timeout_secs = 60 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        service.reconcile_idle(10).await.unwrap(),
        0,
        "recent preview traffic prevents idle Stop"
    );
    let outcome = service
        .stop_as_system_for_tests(
            workspace.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::Idle,
            None,
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, "running");
    let recent = events
        .lock()
        .unwrap()
        .iter()
        .find(|e| {
            e.field("event") == Some("workspace_idle_decision")
                && e.field("workspace_id") == Some(workspace.id.to_string().as_str())
                && e.field("stage") == Some("admission")
                && e.field("decision") == Some("exempt")
        })
        .cloned()
        .expect("locked Stop admission logs recent preview activity");
    assert_eq!(
        recent.field("exemptions"),
        Some("[\"recent_activity_or_initialization\"]"),
        "preview traffic must not be described as human interaction"
    );

    // This path is no longer a platform clock/report endpoint. Both methods
    // relay to the application unchanged, without report validation or metadata.
    for method in [reqwest::Method::GET, reqwest::Method::POST] {
        let previous = age_activity(&app, workspace.id).await;
        let response = app
            .client
            .request(
                method.clone(),
                format!("{}/.g5/user-activity", app.base_url),
            )
            .header("host", &host)
            .header("Authorization", format!("Bearer {}", owner.jwt))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), HTML);
        assert!(activity_at(&app, workspace.id).await > previous);
        assert!(runtime
            .preview_requests()
            .await
            .last()
            .unwrap()
            .starts_with(&format!("{method} /.g5/user-activity HTTP/1.1")));
    }

    // Revoked runtime authority cannot renew or open an upstream stream.
    let previous = age_activity(&app, workspace.id).await;
    let streams_before = runtime.preview_stream_count();
    let successor =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(successor.id)
        .execute(&app.db)
        .await
        .unwrap();
    let denied = app
        .client
        .get(&app.base_url)
        .header("host", &host)
        .header("Authorization", format!("Bearer {}", owner.jwt))
        .send()
        .await
        .unwrap();
    assert!(denied.status().is_client_error());
    assert_eq!(activity_at(&app, workspace.id).await, previous);
    assert_eq!(runtime.preview_stream_count(), streams_before);
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();

    // The same elapsed activity is exempt at 20 minutes, idle at 60 seconds.
    let previous = age_activity(&app, workspace.id).await;
    sqlx::query("UPDATE workspace_settings SET idle_timeout_secs = 1200 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(service.reconcile_idle(10).await.unwrap(), 0);
    sqlx::query("UPDATE workspace_settings SET idle_timeout_secs = 60 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let work_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks (id,org_id,workspace_id,name,kind,status,spec,attempt,created_by)
        VALUES ($1,$2,$3,'bounded work','command','running','{}',1,$4)",
    )
    .bind(work_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(service.reconcile_idle(10).await.unwrap(), 0);
    assert_eq!(activity_at(&app, workspace.id).await, previous);
    service
        .log_idle_exemptions(&mut Default::default(), 10)
        .await
        .unwrap();
    sqlx::query("UPDATE tasks SET status = 'succeeded' WHERE id = $1")
        .bind(work_id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(service.reconcile_idle(10).await.unwrap(), 1);
    assert_eq!(service.view(workspace.id).await.unwrap().status, "stopped");
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|m| m == "vm.stop"));
    let decisions: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| {
            e.field("event") == Some("workspace_idle_decision")
                && e.field("workspace_id") == Some(workspace.id.to_string().as_str())
        })
        .cloned()
        .collect();
    assert!(decisions
        .iter()
        .any(|e| e.field("decision") == Some("exempt")
            && e.field("exemptions").unwrap().contains("running_command")));
    assert!(decisions
        .iter()
        .any(|e| e.field("decision") == Some("stop") && e.field("stage") == Some("admission")));
    assert!(decisions.iter().all(|e| e.field("correlation_id").is_some()
        && e.field("vm_id") == Some(runtime.vm_id().to_string().as_str())
        && e.field("epoch") == Some("1")));
    app.cleanup().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preview_websocket_connections_renew_but_established_connections_do_not() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let events = helpers::tracing_capture();
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (runtime, mut streams) =
        TestWorkspaceRuntimePeer::attach_with_streams(&app, &workspace, owner.id).await;
    let task =
        crate::api_task_lifecycle::running_preview(&app, &workspace, owner.id, runtime.vm_id())
            .await;
    let host = format!("{task}.preview.g5.example");
    let previous = age_activity(&app, workspace.id).await;
    let denied = app
        .client
        .get(&app.base_url)
        .header("host", &host)
        .header("Authorization", format!("Bearer {}", owner.jwt))
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("origin", "https://another.preview.g5.example")
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(activity_at(&app, workspace.id).await, previous);
    assert!(
        streams.try_recv().is_err(),
        "cross-origin upgrade never reaches the guest"
    );

    for connection in 0..2 {
        let previous = age_activity(&app, workspace.id).await;
        let mut request = format!("{}/socket", app.ws_url)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert("host", host.parse().unwrap());
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", owner.jwt).parse().unwrap(),
        );
        request
            .headers_mut()
            .insert("origin", format!("https://{host}").parse().unwrap());
        request.headers_mut().insert(
            "sec-websocket-key",
            "dGhlIHNhbXBsZSBub25jZQ==".parse().unwrap(),
        );
        let accept = async {
            let mut guest = streams.recv().await.unwrap();
            assert_eq!(guest.recv().await.unwrap(), runtime.vm_id().as_bytes());
            assert_eq!(guest.recv().await.unwrap(), 4173_u16.to_be_bytes());
            let mut head = Vec::new();
            while !head.windows(4).any(|part| part == b"\r\n\r\n") {
                head.extend(guest.recv().await.unwrap());
                assert!(head.len() < 64 * 1024);
            }
            assert!(head.starts_with(b"GET /socket HTTP/1.1"));
            guest.send(b"HTTP/1.1 101 Switching Protocols\r\nconnection: Upgrade\r\nupgrade: websocket\r\nsec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n").await.unwrap();
            guest
        };
        let (connected, mut guest) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(connect_async(request), accept)
        })
        .await
        .unwrap();
        let (mut socket, _) = connected.unwrap();
        assert!(
            activity_at(&app, workspace.id).await > previous,
            "new connection and reconnect both count"
        );

        if connection == 0 {
            let previous = age_activity(&app, workspace.id).await;
            // Outlast the write throttle: it must not mask accidental renewals
            // from quiet presence or bidirectional application/control frames.
            tokio::time::sleep(Duration::from_millis(10_100)).await;
            assert_eq!(activity_at(&app, workspace.id).await, previous);
            socket
                .send(Message::Text("automatic app message".into()))
                .await
                .unwrap();
            assert!(!tokio::time::timeout(Duration::from_secs(5), guest.recv())
                .await
                .unwrap()
                .unwrap()
                .is_empty());
            guest.send(b"\x81\x02ok").await.unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), socket.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
                Message::Text("ok".into())
            );
            socket.send(Message::Ping(vec![])).await.unwrap();
            assert!(!tokio::time::timeout(Duration::from_secs(5), guest.recv())
                .await
                .unwrap()
                .unwrap()
                .is_empty());
            guest.send(b"\x8a\x00").await.unwrap();
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(5), socket.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
                Message::Pong(_)
            ));
            assert_eq!(activity_at(&app, workspace.id).await, previous);
        }
        socket.close(None).await.unwrap();
        guest.close().await;
    }
    let changes: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| {
            e.field("event") == Some("workspace_activity_changed")
                && e.field("workspace_id") == Some(workspace.id.to_string().as_str())
        })
        .cloned()
        .collect();
    assert_eq!(changes.len(), 2);
    assert!(changes
        .iter()
        .all(|e| e.field("activity_kind") == Some("preview_traffic")
            && e.field("activity_reason") == Some("preview_traffic")
            && e.field("vm_id") == Some(runtime.vm_id().to_string().as_str())
            && e.field("epoch") == Some("1")
            && e.field("correlation_id").is_some()));
    assert!(!format!("{changes:?}").contains("automatic app message"));
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn user_activity_and_stop_use_the_same_lock_order() {
    use g5_api::services::workspace_runtime::WorkspaceActivityReason;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let previous = age_activity(&app, workspace.id).await;
    let runtime = app.state.workspace_runtime.clone();
    let handle = runtime.owner_handle(workspace.id, owner.id).await.unwrap();
    let mut stop = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace.id)
        .fetch_one(&mut *stop)
        .await
        .unwrap();
    let activity = tokio::spawn(async move {
        runtime
            .record_user_activity(
                &handle,
                WorkspaceActivityReason::Keyboard,
                "lock-order-test",
                Utc::now(),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                WHERE datname = current_database() AND wait_event_type = 'Lock'
                  AND query LIKE '%authz.lock_workspace_member_activity%')",
            )
            .fetch_one(&app.db)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        sqlx::query("SELECT workspace_id FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
            .bind(workspace.id)
            .fetch_one(&mut *stop),
    )
    .await
    .expect("activity must not hold the VM while waiting for Stop's workspace lock")
    .unwrap();
    stop.rollback().await.unwrap();
    activity.await.unwrap().unwrap();
    assert!(activity_at(&app, workspace.id).await > previous);
    app.cleanup().await;
}

#[tokio::test]
async fn idle_selection_is_sql_bounded_and_exemption_diagnostics_are_rate_limited() {
    let events = helpers::tracing_capture();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut peers = Vec::new();
    let mut workspace_ids = Vec::new();
    let mut eligible = Vec::new();
    for (index, exemption) in ["command", "agent", "preview", "retry", "", "", ""]
        .into_iter()
        .enumerate()
    {
        let workspace = TestWorkspace::create(&app, &org).await;
        peers.push(TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await);
        workspace_ids.push(workspace.id.to_string());
        age_activity(&app, workspace.id).await;
        sqlx::query("UPDATE workspace_settings SET idle_timeout_secs = 60 WHERE workspace_id = $1")
            .bind(workspace.id)
            .execute(&app.db)
            .await
            .unwrap();
        // Put all exempt workspaces before candidates in activity order.
        sqlx::query("UPDATE workspace_vms SET last_activity_at = now() - interval '2 hours' + make_interval(secs => $2), idle_retry_after = CASE WHEN $3 THEN now() + interval '5 minutes' END WHERE workspace_id = $1")
            .bind(workspace.id).bind(index as f64).bind(exemption == "retry")
            .execute(&app.db).await.unwrap();
        if ["command", "agent", "preview"].contains(&exemption) {
            let chat_session_id = if exemption == "agent" {
                let id = Uuid::new_v4();
                sqlx::query("INSERT INTO chat_sessions(id, org_id, workspace_id, created_by) VALUES($1,$2,$3,$4)")
                    .bind(id).bind(org.id).bind(workspace.id).bind(owner.id)
                    .execute(&app.db).await.unwrap();
                Some(id)
            } else {
                None
            };
            sqlx::query("INSERT INTO tasks(id, org_id, workspace_id, name, kind, status, spec, attempt, created_by, chat_session_id) VALUES($1,$2,$3,'work',$4,'running','{}',1,$5,$6)")
                .bind(Uuid::new_v4()).bind(org.id).bind(workspace.id).bind(exemption).bind(owner.id)
                .bind(chat_session_id)
                .execute(&app.db).await.unwrap();
        } else if exemption.is_empty() {
            eligible.push(workspace.id);
        }
    }
    let decisions = || {
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| {
                e.field("event") == Some("workspace_idle_decision")
                    && e.field("workspace_id")
                        .is_some_and(|id| workspace_ids.iter().any(|v| v == id))
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(service.idle_candidates(2).await.unwrap(), eligible[..2]);
    let selection = decisions();
    assert_eq!(
        selection.len(),
        2,
        "selection must not fetch/log the full expired population"
    );
    assert!(selection
        .iter()
        .all(|e| e.field("decision") == Some("candidate")));

    let mut diagnostics = Default::default();
    assert_eq!(
        service
            .log_idle_exemptions(&mut diagnostics, 2)
            .await
            .unwrap(),
        2,
        "diagnostics also have a SQL result bound, including exempt rows"
    );
    let after_sample = decisions().len();
    assert!(after_sample <= 4);
    // Production constructs a fresh service each two-second sweep. The
    // lifecycle-owned diagnostic state must continue to suppress the scan.
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    for _ in 0..3 {
        assert_eq!(
            service
                .log_idle_exemptions(&mut diagnostics, 2)
                .await
                .unwrap(),
            0
        );
    }
    assert_eq!(decisions().len(), after_sample);
    assert_eq!(
        service
            .log_idle_exemptions(&mut Default::default(), 100)
            .await
            .unwrap(),
        7
    );
    let logged = decisions();
    for reason in [
        "running_command",
        "running_agent",
        "preview_startup",
        "stop_retry_backoff",
    ] {
        assert!(
            logged.iter().any(|e| e.field("stage") == Some("diagnostic")
                && e.field("exemptions")
                    .is_some_and(|value| value.contains(reason))),
            "missing {reason}"
        );
    }
    assert_eq!(
        service.idle_candidates(2).await.unwrap(),
        eligible[..2],
        "sampling does not change candidate eligibility or let older exemptions starve it"
    );
    drop(peers);
    app.cleanup().await;
}

async fn workspace_vm_service(
    app: &TestApp,
    actor_user_id: Uuid,
    org_id: Uuid,
) -> g5_api::services::WorkspaceVmService {
    let db = app.scoped_db_for_user_org(actor_user_id, org_id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
        system_pool: app.state.system_pool.clone(),
        scope,
        hosts: app.state.preview_tunnels.clone(),
        minter: app
            .state
            .jwt_minter
            .clone()
            .expect("test app configures a lifecycle JWT minter"),
        runtime: app.state.workspace_runtime.clone(),
        agent_work: app.state.agent_work.clone(),
        repo_storage: app.state.repo_storage.clone(),
        git_external_url: app.state.git_external_url.clone(),
        ws_hub: app.state.event_bus.ws_hub().clone(),
    })
}
