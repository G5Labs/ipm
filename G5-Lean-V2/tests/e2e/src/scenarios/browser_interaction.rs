//! Browser Interaction E2E Tests
//!
//! Tests workspace event streaming.

use crate::helpers::api_client::{api_base_url, TestApiClient};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

/// WebSocket event stream: subscribe to workspace events
#[tokio::test]
async fn test_workspace_event_stream() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    let org = api.create_org("ws-events-org").await?;
    let ws = api.create_workspace(org.id, "ws-events").await?;
    let ticket_response = api
        .authed_post(&format!("{}/api/v1/auth/ws-ticket", api.base_url()))
        .json(&serde_json::json!({}))
        .send()
        .await?
        .error_for_status()?;
    let ticket_body: serde_json::Value = ticket_response.json().await?;
    let token = ticket_body["data"]["ticket"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing platform WebSocket ticket"))?;

    // Connect to the authenticated event stream and subscribe to the workspace channel.
    let base = api_base_url().replace("http://", "ws://");
    let url = format!("{}/api/v1/ws?token={}", base, token);
    let (mut socket, _resp) = tokio_tungstenite::connect_async(&url).await?;

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "subscribe",
                "channel": format!("workspace:{}", ws.id),
            })
            .to_string()
            .into(),
        ))
        .await?;

    // Wait for the subscribe ack, skipping any non-text frames (e.g. server
    // heartbeat Ping that may arrive immediately after connection).
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let ack_text = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("timed out waiting for subscribe ack");
        }
        let msg = tokio::time::timeout(remaining, socket.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("expected subscribe ack message"))??;
        if let Message::Text(txt) = msg {
            break txt;
        }
        // Skip Ping/Pong/Binary frames.
    };
    let ack_json: serde_json::Value = serde_json::from_str(&ack_text)?;
    assert_eq!(
        ack_json.get("type").and_then(|v| v.as_str()),
        Some("subscribed"),
        "expected subscribed ack, got: {}",
        ack_json
    );

    socket.close(None).await?;

    Ok(())
}
