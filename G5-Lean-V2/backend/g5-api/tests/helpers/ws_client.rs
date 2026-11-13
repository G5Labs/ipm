//! WebSocket test client for asserting real-time event delivery.
//!
//! Provides `WsTestClient` which wraps a `tokio-tungstenite` WebSocket
//! connection with convenience methods for subscribing to channels,
//! expecting specific events with timeouts, and asserting silence.

use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// WebSocket client wrapper for integration test assertions.
///
/// Wraps a split WebSocket stream (read + write halves) and provides
/// typed methods for sending subscribe/unsubscribe messages and asserting
/// event delivery with timeouts.
pub struct WsTestClient {
    write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl WsTestClient {
    /// Establishes a WebSocket connection to the given URL.
    ///
    /// Uses `tokio_tungstenite::connect_async` to perform the WebSocket
    /// handshake, then splits the stream into read and write halves.
    ///
    /// # Panics
    /// Panics if the WebSocket connection fails (e.g., server not running,
    /// authentication rejected).
    pub async fn connect(url: &str) -> Self {
        let (ws_stream, _response) = connect_async(url)
            .await
            .expect("Failed to connect WebSocket");

        let (write, read) = ws_stream.split();

        Self { write, read }
    }

    /// Deadline for the subscription acknowledgement handshake.
    ///
    /// Nearly every caller discards the ack: it is a sync point proving the
    /// subscription is live before the test triggers the events it actually
    /// asserts on, not an assertion of its own. A short bound therefore fails
    /// a test for a reason unrelated to what it covers — under a loaded CI
    /// runner the handshake can exceed a few seconds while the subscription is
    /// perfectly healthy, which is how a five-second literal turned into an
    /// intermittent failure across API WebSocket and remote-advisory suites.
    ///
    /// Keep the sync point, drop the tight deadline. A subscription that never
    /// acks is a real hang, and the nextest harness guard attributes it (SLOW
    /// at each 60s period, killed after 720s) with per-test detail rather than
    /// a bare five-second panic.
    const SUBSCRIBE_ACK_TIMEOUT: Duration = Duration::from_secs(60);

    /// Waits for the subscription acknowledgement before the caller drives the
    /// events it asserts on.
    pub async fn expect_subscribed(&mut self) -> serde_json::Value {
        self.expect_event("subscribed", Self::SUBSCRIBE_ACK_TIMEOUT)
            .await
    }

    /// Require server closure without accepting any queued application data.
    pub async fn expect_closed(&mut self, timeout: Duration) {
        tokio::time::timeout(timeout, async {
            loop {
                match self.read.next().await {
                    Some(Ok(Message::Close(_))) | None => return,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = self.write.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {},
                    Some(Err(tokio_tungstenite::tungstenite::Error::Protocol(
                        tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                    ))) => return,
                    Some(Err(tokio_tungstenite::tungstenite::Error::Io(error)))
                        if error.kind() == std::io::ErrorKind::ConnectionReset => return,
                    other => panic!("Expected closure without application data, received {other:?}"),
                }
            }
        }).await.expect("revoked WebSocket did not close within the bound");
    }

    /// Waits for a specific event type with a timeout.
    ///
    /// Loops over incoming messages from the WebSocket stream. For each text
    /// message, parses it as JSON and checks if the `"type"` field matches
    /// `event_type`. Returns the first matching event.
    ///
    /// # Arguments
    /// * `event_type` — The expected event type string (e.g., `"task.status"`,
    ///   `"chat.chunk"`).
    /// * `timeout` — Maximum duration to wait for the event.
    ///
    /// # Returns
    /// The parsed JSON value of the matching event.
    ///
    /// # Panics
    /// Panics with a descriptive message if the timeout expires before a
    /// matching event is received:
    /// `"Timed out waiting for event '{event_type}' after {timeout:?}"`.
    pub async fn expect_event(&mut self, event_type: &str, timeout: Duration) -> serde_json::Value {
        let result = tokio::time::timeout(timeout, async {
            loop {
                match self.read.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if json.get("type").and_then(|v| v.as_str()) == Some(event_type) {
                                return json;
                            }
                        }
                        // Not the event we're looking for; keep waiting
                    }
                    Some(Ok(Message::Ping(_))) => {
                        // Ignore ping frames
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Ignore pong frames
                    }
                    Some(Ok(Message::Close(_))) => {
                        panic!("WebSocket closed while waiting for event '{}'", event_type);
                    }
                    Some(Err(e)) => {
                        panic!(
                            "WebSocket error while waiting for event '{}': {}",
                            event_type, e
                        );
                    }
                    None => {
                        panic!(
                            "WebSocket stream ended while waiting for event '{}'",
                            event_type
                        );
                    }
                    _ => {
                        // Ignore binary and other frame types
                    }
                }
            }
        })
        .await;

        match result {
            Ok(event) => event,
            Err(_) => panic!(
                "Timed out waiting for event '{}' after {:?}",
                event_type, timeout
            ),
        }
    }

    /// Counts events of `event_type` received within `window`, ignoring every
    /// other frame (heartbeats, other event types). Returns once the window
    /// elapses or the stream ends — never panics on unrelated traffic, so it
    /// can assert an EXACT count on a channel that also carries other events.
    pub async fn count_events(&mut self, event_type: &str, window: Duration) -> usize {
        let mut count = 0;
        let deadline = tokio::time::Instant::now() + window;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return count;
            }
            match tokio::time::timeout(remaining, self.read.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if json.get("type").and_then(|v| v.as_str()) == Some(event_type) {
                            count += 1;
                        }
                    }
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(e))) => {
                    panic!("WebSocket error while counting '{}': {}", event_type, e)
                }
                Ok(None) => return count,
                Err(_) => return count,
            }
        }
    }

    /// Asserts that no events are received within the given duration.
    ///
    /// Loops over incoming messages, ignoring Ping/Pong frames (heartbeats).
    /// Panics if any Text or Binary message is received. If the timeout
    /// expires (no event message), returns successfully.
    ///
    /// # Arguments
    /// * `duration` — How long to wait for silence.
    ///
    /// # Panics
    /// Panics if any event message is received within the duration.
    pub async fn expect_silence(&mut self, duration: Duration) {
        let result = tokio::time::timeout(duration, async {
            loop {
                match self.read.next().await {
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {
                        // Ignore heartbeat frames — not counted as events
                        continue;
                    }
                    Some(Ok(msg)) => {
                        return Some(msg);
                    }
                    Some(Err(e)) => {
                        panic!("Expected silence but got WebSocket error: {}", e);
                    }
                    None => {
                        // Stream ended — that counts as silence
                        return None;
                    }
                }
            }
        })
        .await;

        match result {
            Ok(Some(msg)) => {
                panic!("Expected silence but received message: {:?}", msg);
            }
            Ok(None) => {
                // Stream ended — silence
            }
            Err(_) => {
                // Timeout expired — no event message received. This is the
                // expected outcome.
            }
        }
    }

    /// Asserts that no event of one specific type is received within the
    /// duration while allowing unrelated events to pass through.
    pub async fn expect_no_event(&mut self, event_type: &str, duration: Duration) {
        let result = tokio::time::timeout(duration, async {
            loop {
                match self.read.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if json.get("type").and_then(|value| value.as_str()) == Some(event_type)
                            {
                                return Some(json);
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => return None,
                    Some(Err(error)) => {
                        panic!(
                            "WebSocket error while asserting no event '{}': {}",
                            event_type, error
                        );
                    }
                    _ => {}
                }
            }
        })
        .await;

        if let Ok(Some(event)) = result {
            panic!(
                "Expected no event '{}' but received: {:?}",
                event_type, event
            );
        }
    }

    /// Sends a raw text message through the WebSocket.
    ///
    /// # Panics
    /// Panics if the send fails.
    pub async fn send(&mut self, msg: &str) {
        self.write
            .send(Message::Text(msg.into()))
            .await
            .expect("Failed to send WebSocket message");
    }

    /// Serializes the value to JSON and sends it as a text message.
    ///
    /// # Panics
    /// Panics if serialization or sending fails.
    pub async fn send_json<T: Serialize>(&mut self, value: &T) {
        let json = serde_json::to_string(value).expect("Failed to serialize to JSON");
        self.send(&json).await;
    }

    /// Sends a subscribe message for the given channel.
    ///
    /// Sends: `{"type": "subscribe", "channel": "<channel>"}`
    pub async fn subscribe(&mut self, channel: &str) {
        let msg = serde_json::json!({
            "type": "subscribe",
            "channel": channel,
        });
        self.send_json(&msg).await;
    }

    /// Sends an unsubscribe message for the given channel.
    ///
    /// Sends: `{"type": "unsubscribe", "channel": "<channel>"}`
    pub async fn unsubscribe(&mut self, channel: &str) {
        let msg = serde_json::json!({
            "type": "unsubscribe",
            "channel": channel,
        });
        self.send_json(&msg).await;
    }

    /// Sends a close frame and drops the connection.
    pub async fn close(mut self) {
        self.write.send(Message::Close(None)).await.ok(); // Best-effort close
    }
}
