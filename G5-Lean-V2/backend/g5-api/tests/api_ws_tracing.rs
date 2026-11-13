//! Exercise tracing isolation through the mounted, authenticated WebSocket route.

use crate::helpers;

use helpers::{TestApp, TestUser, WsTestClient};
use serde_json::{json, Value};
use std::time::Duration;

/// Pending and closing sockets must not leave their tracing spans on unrelated tasks.
#[tokio::test(flavor = "current_thread")]
async fn websocket_spans_do_not_leak_across_await_or_close() {
    // One runtime thread makes the poll boundary deterministic: a guard held
    // across await leaks into the next unrelated task on this same thread.
    helpers::tracing_capture();
    // The checks below only mean something if server INFO spans are enabled:
    // a disabled span never becomes current, so a leak could not be seen.
    assert!(
        !tracing::info_span!(target: "g5_api_core::ws", "ws_tracing_probe").is_disabled(),
        "INFO spans are disabled under the test subscriber; the leak checks would be vacuous"
    );
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;

    for _ in 0..3 {
        let mut sockets = Vec::new();
        for _ in 0..2 {
            let response = app
                .authenticated_client(&user)
                .post_json("/api/v1/auth/ws-ticket", &json!({}))
                .await;
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            let body: Value = response.json().await.unwrap();
            let ticket = body["data"]["ticket"].as_str().unwrap();
            sockets.push(
                WsTestClient::connect(&format!("{}/api/v1/ws?token={ticket}", app.ws_url)).await,
            );
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while app.state.event_bus.ws_hub().connection_count() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        for socket in &mut sockets {
            socket.send_json(&json!({"type": "invalid-message"})).await;
            socket.expect_event("error", Duration::from_secs(10)).await;
            assert!(
                tracing::Span::current().metadata().is_none(),
                "a pending WebSocket span leaked into an unrelated task"
            );
            assert_eq!(
                app.client
                    .get(format!("{}/healthz", app.base_url))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                reqwest::StatusCode::OK
            );
        }
        for socket in sockets.into_iter().rev() {
            socket.close().await;
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while app.state.event_bus.ws_hub().connection_count() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            tracing::Span::current().metadata().is_none(),
            "a closed WebSocket left a stale span on an unrelated task"
        );
    }
    app.cleanup().await;
}
