//! WebSocket hub — event broadcasting, channel management, connection handling.
//!
//! This module is the central coordinator of the real-time WebSocket hub for
//! the G5 API server. It provides:
//!
//! - [`WsHub`] — shared hub managing all active connections and channel subscriptions
//! - [`WsConnection`] — per-connection state tracked by the hub
//! - [`ConnectionId`] — unique identifier newtype for each WebSocket connection
//! - [`WsMessage`] — internal message type for the per-connection writer channel
//! - [`WsHubConfig`] — configuration values for the hub
//!
//! # Architecture
//!
//! Each WebSocket connection spawns three concurrent tokio tasks:
//! 1. **Reader task** — reads incoming frames, processes subscribe/unsubscribe
//! 2. **Writer task** — receives `WsMessage` from mpsc channel, sends over WebSocket
//! 3. **Heartbeat task** — renews current authority, sends pings, detects dead connections
//!
//! The hub uses `DashMap` for concurrent access to the connection registry and
//! channel subscriber index, avoiding global lock contention during event fan-out.

pub mod channels;
pub mod events;
pub mod host_ctl;
pub mod preview_tunnel;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Events admitted under one membership recheck: everything already queued
/// for the connection, at most this many (`g5.security.membership-streams`).
pub const WRITER_BATCH_MAX_EVENTS: usize = 256;
/// A positive membership verdict stands for at most this long; a trickle of
/// single events is not one query each.
pub const MEMBERSHIP_VERDICT_TTL: Duration = Duration::from_secs(1);

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tokio::time::Instant as Deadline;
use tracing::Instrument;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::errors::AppError;
use crate::services::identity::membership_activity::MembershipActivityGuard;
use crate::services::AppState;
use g5_auth::JwtClaims;
use g5_contracts::error::G5Error;

pub use channels::{Channel, ChannelParseError, ClientMessage, ServerMessage, SubscriptionScope};
pub use events::WsEvent;

#[derive(Debug, Deserialize)]
pub struct WsUpgradeParams {
    /// The signed data-channel JWT token.
    pub token: String,
}

// ---------------------------------------------------------------------------
// ConnectionId — unique connection identifier
// ---------------------------------------------------------------------------

/// Unique identifier for each WebSocket connection.
///
/// Generated as UUID v4 on connection establishment. Used as the key in the
/// connection registry and in tracing spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub Uuid);

impl ConnectionId {
    /// Create a new random connection ID.
    #[must_use]
    pub fn new() -> Self {
        ConnectionId(Uuid::new_v4())
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// WsConnection — per-connection state
// ---------------------------------------------------------------------------

/// Per-connection state tracked by the hub.
pub struct WsConnection {
    /// Unique connection identifier (UUID v4).
    pub id: ConnectionId,
    /// Authenticated user's UUID, extracted from the JWT `sub` claim.
    pub user_id: Uuid,
    /// User's current org UUID, extracted from the JWT `org` claim.
    pub org_id: Uuid,
    /// Channels and their resolved resource authority.
    pub subscriptions: HashMap<Channel, SubscriptionScope>,
    /// Sender half of a bounded mpsc channel connected to the writer task.
    pub tx: mpsc::Sender<WsMessage>,
    /// When the connection was established.
    pub connected_at: Instant,
    /// When the last pong frame was received from the client.
    pub last_pong: Instant,
    /// Counter of events dropped because the writer channel was full.
    pub dropped_events: AtomicU64,
    /// Start of the current dropped-event tracking window.
    pub dropped_events_window_start: Instant,
    authority: watch::Sender<ConnectionAuthority>,
    authorization_gate: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct ConnectionAuthority {
    deadline: Deadline,
    close: Option<(u16, String)>,
}

impl ConnectionAuthority {
    fn permits_events(&self) -> bool {
        self.close.is_none() && Deadline::now() < self.deadline
    }
}

/// Scope whose authority changed. Workspace scope includes indirect channels.
#[derive(Debug, Clone, Copy)]
pub enum RevocationScope {
    Organization(Uuid),
    Workspace(Uuid),
}

impl RevocationScope {
    fn matches(self, scope: &SubscriptionScope) -> bool {
        match (self, scope) {
            (Self::Organization(id), SubscriptionScope::Organization(org_id))
            | (Self::Organization(id), SubscriptionScope::Workspace { org_id, .. }) => {
                id == *org_id
            }
            (Self::Workspace(id), SubscriptionScope::Workspace { workspace_id, .. }) => {
                id == *workspace_id
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// WsMessage — internal message type for per-connection writer
// ---------------------------------------------------------------------------

/// Internal message type sent through the per-connection mpsc channel to
/// the writer task.
pub enum WsMessage {
    /// A JSON event to send to the client.
    Event(Channel, Box<WsEvent>),
    /// A control message (subscription confirmation, error).
    Control(ServerMessage),
    /// Close the connection with the given code and reason.
    Close(u16, String),
    /// Send a ping frame.
    Ping,
}

// ---------------------------------------------------------------------------
// WsHubConfig — hub configuration
// ---------------------------------------------------------------------------

/// Configuration values for the WebSocket hub.
#[derive(Debug, Clone)]
pub struct WsHubConfig {
    /// Ping interval and maximum authorization lifetime. Default: 30 seconds.
    /// Authorization renewal starts halfway through this interval.
    pub heartbeat_interval: Duration,
    /// Maximum time to wait for a pong after sending a ping. Default: 10 seconds.
    pub heartbeat_timeout: Duration,
    /// Maximum number of concurrent WebSocket connections. Default: 10,000.
    pub max_connections: usize,
    /// Capacity of the per-connection writer channel. Default: 2,048, so one
    /// published journal page (at most 1,024 live rows) fits twice.
    pub writer_channel_capacity: usize,
    /// Maximum dropped events before force-closing a connection. Default: 1000.
    pub max_dropped_events: u64,
    /// Window duration for dropped event tracking. Default: 60 seconds.
    pub dropped_events_window: Duration,
    /// Graceful shutdown drain timeout. Default: 5 seconds.
    pub shutdown_drain_timeout: Duration,
}

impl Default for WsHubConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(30),
            heartbeat_timeout: Duration::from_secs(10),
            max_connections: 10_000,
            // One live journal page (`g5.chat.live-delivery`, up to 1,024
            // rows) is published in a burst; a queue that holds two of them
            // keeps a healthy browser from dropping any of it, while a slow
            // one still loses events and is closed as before.
            writer_channel_capacity: 2048,
            max_dropped_events: 1000,
            dropped_events_window: Duration::from_secs(60),
            shutdown_drain_timeout: Duration::from_secs(5),
        }
    }
}

// ---------------------------------------------------------------------------
// WsHub — main hub struct
// ---------------------------------------------------------------------------

/// The main hub struct that manages all active WebSocket connections and
/// handles event routing.
///
/// Uses `DashMap` for concurrent access from multiple tokio tasks without
/// global lock contention.
pub struct WsHub {
    /// Bumped when this API changes a membership's activity, so every
    /// connection's standing membership verdict is rechecked at its next
    /// batch instead of at the verdict's expiry (`g5.security.membership-streams`).
    membership_generation: std::sync::atomic::AtomicU64,
    /// Registry of all active connections, keyed by ConnectionId.
    connections: DashMap<ConnectionId, WsConnection>,
    /// Reverse index: channel → set of connection IDs subscribed to that channel.
    channel_subscribers: DashMap<Channel, HashSet<ConnectionId>>,
    /// Configuration values.
    config: WsHubConfig,
}

impl WsHub {
    /// Create a new hub with the given configuration.
    #[must_use]
    pub fn new(config: WsHubConfig) -> Self {
        Self {
            membership_generation: std::sync::atomic::AtomicU64::new(0),
            connections: DashMap::new(),
            channel_subscribers: DashMap::new(),
            config,
        }
    }

    /// Create a new hub from the application configuration.
    ///
    /// Extracts WS-related config from `AppConfig`. Currently uses defaults
    /// for all values not explicitly in `AppConfig`.
    #[must_use]
    pub fn from_app_config(_config: &AppConfig) -> Self {
        Self::new(WsHubConfig::default())
    }

    /// Publish an event to all connections subscribed to the given channel.
    ///
    /// This is the in-process fan-out path. For each subscribed
    /// connection:
    /// - Attempts `tx.try_send(WsMessage::Event(channel.clone(), Box::new(event.clone())))`
    /// - If the channel is full, increments the connection's dropped_events counter
    /// - If dropped_events exceeds max_dropped_events within the window, closes
    ///   the connection through its authority signal (ahead of the full queue)
    pub fn publish(&self, channel: &Channel, event: &WsEvent) {
        self.publish_matching(channel, event, |_| true);
    }

    /// Publish an event only to the specified user's connections on a shared
    /// channel. This keeps owner-only live workspace payloads off channels
    /// that readers may legitimately subscribe to for durable events.
    pub fn publish_to_user(&self, channel: &Channel, user_id: Uuid, event: &WsEvent) {
        self.publish_matching(channel, event, |connection| connection.user_id == user_id);
    }

    fn publish_matching(
        &self,
        channel: &Channel,
        event: &WsEvent,
        include: impl Fn(&WsConnection) -> bool,
    ) {
        // Release the reverse-index lock before taking a connection lock.
        // Subscription changes take these locks in the opposite order.
        let subscribers: Vec<_> = self
            .channel_subscribers
            .get(channel)
            .map(|subscribers| subscribers.iter().copied().collect())
            .unwrap_or_default();
        let mut too_slow = Vec::new();
        for conn_id in &subscribers {
            if let Some(conn) = self.connections.get(conn_id) {
                if !include(&conn)
                    || !conn.subscriptions.contains_key(channel)
                    || !conn.authority.borrow().permits_events()
                {
                    continue;
                }
                match conn
                    .tx
                    .try_send(WsMessage::Event(channel.clone(), Box::new(event.clone())))
                {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        let dropped = conn.dropped_events.fetch_add(1, Ordering::Relaxed) + 1;
                        tracing::warn!(
                            connection_id = %conn_id,
                            channel = %channel,
                            dropped_events = dropped,
                            "Event dropped for slow client"
                        );
                        // A client this far behind is closed. The signal goes
                        // through the connection's authority, which the writer
                        // observes ahead of its queue, because the queue is the
                        // very thing that is full: a close frame queued behind it
                        // would be dropped like the events.
                        if dropped > self.config.max_dropped_events
                            && conn.dropped_events_window_start.elapsed()
                                < self.config.dropped_events_window
                        {
                            conn.authority.send_modify(|authority| {
                                authority.close.get_or_insert((
                                    1008,
                                    "Too many dropped events: client too slow".into(),
                                ));
                            });
                            too_slow.push(*conn_id);
                        }
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        // Connection is already closing, ignore
                    }
                }
            }
        }
        // Unregister after the connection reference is released: removal
        // takes the registry's write lock.
        for conn_id in too_slow {
            self.disconnect(conn_id);
        }
    }

    /// Add a subscription for a connection.
    ///
    /// Returns `Ok(())` on success, or an error if the connection is not found.
    /// Does NOT perform authorization — that is the caller's responsibility
    /// (handled in the reader task using `channels::authorize_subscribe`).
    fn subscribe(
        &self,
        conn_id: ConnectionId,
        channel: Channel,
        scope: SubscriptionScope,
    ) -> Result<(), AppError> {
        // Add channel to connection's subscription set
        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            if !conn.authority.borrow().permits_events() {
                return Err(AppError::from_error(G5Error::Forbidden {
                    missing_scopes: vec![],
                }));
            }
            conn.subscriptions.insert(channel.clone(), scope);
            // Keep the connection lock through index insertion so disconnect
            // cannot leave a subscription behind or revive an evicted socket.
            self.channel_subscribers
                .entry(channel)
                .or_default()
                .insert(conn_id);
        } else {
            return Err(AppError::from_error(G5Error::Internal {
                message: format!("Connection {} not found", conn_id),
            }));
        }

        Ok(())
    }

    /// Remove a subscription for a connection.
    pub fn unsubscribe(&self, conn_id: ConnectionId, channel: &Channel) {
        // Remove channel from connection's subscription set
        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            conn.subscriptions.remove(channel);
            self.remove_subscriber(channel, conn_id);
        }
    }

    fn remove_subscriber(&self, channel: &Channel, conn_id: ConnectionId) {
        if let dashmap::mapref::entry::Entry::Occupied(mut entry) =
            self.channel_subscribers.entry(channel.clone())
        {
            entry.get_mut().remove(&conn_id);
            if entry.get().is_empty() {
                entry.remove();
            }
        }
    }

    /// Remove a connection and all its subscriptions.
    ///
    /// Called on WebSocket close (normal or abnormal).
    pub fn disconnect(&self, conn_id: ConnectionId) {
        // Remove the connection and get its subscription list
        if let Some((_, conn)) = self.connections.remove(&conn_id) {
            conn.authority.send_modify(|authority| {
                authority
                    .close
                    .get_or_insert((1000, "Connection closed".into()));
            });
            // Remove connection from all channel subscriber sets
            for channel in conn.subscriptions.keys() {
                self.remove_subscriber(channel, conn_id);
            }
            tracing::info!(
                connection_id = %conn_id,
                user_id = %conn.user_id,
                subscriptions = conn.subscriptions.len(),
                duration_secs = conn.connected_at.elapsed().as_secs(),
                "WebSocket connection closed"
            );
        }
    }

    /// A membership's activity changed in this API: standing verdicts are
    /// stale from now on.
    pub fn note_membership_change(&self) {
        self.membership_generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }

    fn membership_generation(&self) -> u64 {
        self.membership_generation
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn close_connection(&self, conn_id: ConnectionId, code: u16, reason: &str) {
        if let Some(conn) = self.connections.get(&conn_id) {
            // This signal cannot be lost behind a full event queue.
            conn.authority.send_modify(|authority| {
                authority.close = Some((code, reason.into()));
            });
        }
        self.disconnect(conn_id);
    }

    /// Evict every affected socket, including user-only sockets bound to an
    /// organization and workspace subscriptions reached via chat/task IDs.
    pub fn evict(&self, user_id: Uuid, scope: RevocationScope) {
        let ids: Vec<_> = self
            .connections
            .iter()
            .filter(|conn| {
                conn.user_id == user_id
                    && (matches!(scope, RevocationScope::Organization(id) if id == conn.org_id)
                        || conn
                            .subscriptions
                            .values()
                            .any(|subscription| scope.matches(subscription)))
            })
            .map(|conn| conn.id)
            .collect();
        for id in ids {
            self.close_connection(id, 1008, "Access revoked");
        }
    }

    /// Called after a workspace authority transaction commits. Retained owners,
    /// organization readers and admins keep their sockets. A failure
    /// to establish current access fails closed without undoing the mutation.
    pub async fn revoke_workspace_access(
        &self,
        system_pool: &sqlx::PgPool,
        scope: g5_authz::ScopedDbReopener,
        workspace_id: Uuid,
        user_id: Option<Uuid>,
    ) {
        let readers: HashSet<_> = self
            .connections
            .iter()
            .filter(|conn| user_id.is_none_or(|id| id == conn.user_id))
            .flat_map(|conn| {
                conn.subscriptions
                    .values()
                    .filter_map(|subscription| match subscription {
                        SubscriptionScope::Workspace {
                            org_id,
                            workspace_id: id,
                        } if *id == workspace_id => Some((conn.user_id, *org_id)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        // Bound parallel database work for large audiences; each socket's writer
        // deadline still applies while this hook processes successive batches.
        let scope = &scope;
        futures_util::stream::iter(readers)
            .for_each_concurrent(16, |(user_id, org_id)| async move {
                let access = tokio::time::timeout(self.config.heartbeat_interval / 2, async {
                    let mut db = scope.open().await.map_err(AppError::from)?;
                    crate::db::scoped::ensure_user_workspace_access(
                        system_pool,
                        &mut db,
                        user_id,
                        org_id,
                        workspace_id,
                    )
                    .await
                })
                .await;
                if !matches!(access, Ok(Ok(()))) {
                    self.evict(user_id, RevocationScope::Workspace(workspace_id));
                }
            })
            .await;
    }

    async fn reauthorize_connection(
        &self,
        state: &Arc<AppState>,
        conn_id: ConnectionId,
        claims: &JwtClaims,
    ) {
        let Some(gate) = self
            .connections
            .get(&conn_id)
            .map(|conn| conn.authorization_gate.clone())
        else {
            return;
        };
        let _guard = gate.lock().await;
        let Some((user_id, org_id, subscriptions, deadline)) =
            self.connections.get(&conn_id).map(|conn| {
                (
                    conn.user_id,
                    conn.org_id,
                    conn.subscriptions.keys().cloned().collect::<Vec<_>>(),
                    conn.authority.borrow().deadline,
                )
            })
        else {
            return;
        };
        let started = Deadline::now();
        let authorized = tokio::time::timeout_at(deadline, async {
            if !org_id.is_nil() {
                channels::authorize_org(state, user_id, org_id).await?;
            }
            let mut resolved = HashMap::new();
            for channel in subscriptions {
                let scope = channels::authorize_subscribe(state, user_id, claims, &channel).await?;
                resolved.insert(channel, scope);
            }
            Ok::<_, AppError>(resolved)
        })
        .await;
        if let Ok(Ok(resolved)) = authorized {
            if let Some(mut conn) = self.connections.get_mut(&conn_id) {
                conn.subscriptions = resolved;
                conn.authority.send_modify(|authority| {
                    if authority.permits_events() {
                        // Database time consumes the lease; it never extends it.
                        authority.deadline = started + self.config.heartbeat_interval;
                    }
                });
            }
        } else {
            self.close_connection(conn_id, 1008, "Access revoked or authorization unavailable");
        }
    }

    /// Returns the number of active connections.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Returns the number of subscribers for a given channel.
    #[must_use]
    pub fn channel_subscriber_count(&self, channel: &Channel) -> usize {
        self.channel_subscribers
            .get(channel)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Register a new connection in the hub.
    ///
    /// Returns the `ConnectionId` and the receiver half of the writer channel.
    fn register_connection(
        &self,
        user_id: Uuid,
        org_id: Uuid,
    ) -> Result<(ConnectionId, mpsc::Receiver<WsMessage>), AppError> {
        // Check connection limit
        if self.connections.len() >= self.config.max_connections {
            return Err(AppError::from_error(G5Error::Internal {
                message: "Maximum WebSocket connections reached".into(),
            }));
        }

        let conn_id = ConnectionId::new();
        let (tx, rx) = mpsc::channel(self.config.writer_channel_capacity);
        let now = Instant::now();

        let connection = WsConnection {
            id: conn_id,
            user_id,
            org_id,
            subscriptions: HashMap::new(),
            tx,
            connected_at: now,
            last_pong: now,
            dropped_events: AtomicU64::new(0),
            dropped_events_window_start: now,
            authority: watch::channel(ConnectionAuthority {
                deadline: Deadline::now() + self.config.heartbeat_interval,
                close: None,
            })
            .0,
            authorization_gate: Arc::new(Mutex::new(())),
        };

        self.connections.insert(conn_id, connection);

        tracing::info!(
            connection_id = %conn_id,
            user_id = %user_id,
            org_id = %org_id,
            total_connections = self.connections.len(),
            "WebSocket connection established"
        );

        Ok((conn_id, rx))
    }

    /// Initiate graceful shutdown: send close frames to all connections
    /// and wait for drain.
    pub async fn shutdown(&self) {
        tracing::info!(
            connections = self.connections.len(),
            "Initiating WebSocket hub shutdown"
        );

        // Send close frame to all connections
        for entry in self.connections.iter() {
            if let Err(e) = entry
                .value()
                .tx
                .try_send(WsMessage::Close(1001, "Server shutting down".into()))
            {
                tracing::debug!(connection_id = %entry.key(), error = %e, "ws_message_dropped: failed to send shutdown close frame");
            }
        }

        // Wait for drain timeout
        tokio::time::sleep(self.config.shutdown_drain_timeout).await;

        // Force-close remaining connections
        let remaining = self.connections.len();
        if remaining > 0 {
            tracing::warn!(
                remaining_connections = remaining,
                "Force-closing remaining WebSocket connections after drain timeout"
            );
            self.connections.clear();
            self.channel_subscribers.clear();
        }
    }
}

struct MembershipDelivery {
    state: Arc<AppState>,
    user_id: Uuid,
    claims: JwtClaims,
    selected: Option<MembershipActivityGuard>,
    memberships: HashMap<Uuid, MembershipActivityGuard>,
    /// When each organization's membership was last confirmed, and under
    /// which membership generation; a verdict stands for
    /// [`MEMBERSHIP_VERDICT_TTL`] unless a membership changed meanwhile
    /// (`g5.security.membership-streams`).
    confirmed: HashMap<Uuid, (Instant, u64)>,
}

impl MembershipDelivery {
    fn confirmed_recently(&self, org: Uuid, now: Instant, generation: u64) -> bool {
        self.confirmed.get(&org).is_some_and(|(at, seen)| {
            *seen == generation && now.duration_since(*at) < MEMBERSHIP_VERDICT_TTL
        })
    }

    /// Admit a batch: one membership check per organization the batch
    /// touches, reused within the verdict's life. A selected-organization
    /// denial closes the connection; a notification for another organization
    /// is discarded alone, as before.
    async fn authorize_batch(
        &mut self,
        messages: &[WsMessage],
        hub: &WsHub,
        conn_id: ConnectionId,
    ) -> Result<Vec<bool>, ()> {
        let now = Instant::now();
        let generation = hub.membership_generation();
        if let Some(guard) = &self.selected {
            let org = guard.binding().0;
            if !self.confirmed_recently(org, now, generation) {
                if !guard.authorize().await {
                    return Err(());
                }
                self.confirmed.insert(org, (now, generation));
            }
        }
        let mut admitted = Vec::with_capacity(messages.len());
        // An organization denied once in this batch is not asked again for
        // the batch's later events.
        let mut denied: HashSet<Uuid> = HashSet::new();
        for message in messages {
            admitted.push(
                self.authorize_event(message, hub, conn_id, now, generation, &mut denied)
                    .await?,
            );
        }
        Ok(admitted)
    }

    // The resource organization of one event, checked once per verdict life.
    async fn authorize_event(
        &mut self,
        message: &WsMessage,
        hub: &WsHub,
        conn_id: ConnectionId,
        now: Instant,
        generation: u64,
        denied: &mut HashSet<Uuid>,
    ) -> Result<bool, ()> {
        let WsMessage::Event(channel, event) = message else {
            return Ok(true);
        };
        // Subscription scopes already carry the resource's organization and
        // remain subject to the writer's independently renewed read lease.
        // Only account-wide notifications need a per-event resource lookup.
        let scope = hub
            .connections
            .get(&conn_id)
            .and_then(|conn| conn.subscriptions.get(channel).copied());
        let org = match scope {
            Some(SubscriptionScope::Organization(org_id))
            | Some(SubscriptionScope::Workspace { org_id, .. }) => org_id,
            Some(SubscriptionScope::User) => {
                let Ok(Ok(org)) = tokio::time::timeout(
                    Duration::from_secs(5),
                    channels::delivery_org(&self.state, self.user_id, &self.claims, channel, event),
                )
                .await
                else {
                    return Ok(false);
                };
                org
            }
            None => return Ok(false),
        };
        if self
            .selected
            .as_ref()
            .is_some_and(|guard| guard.binding().0 == org)
        {
            return Ok(true); // Checked for this batch above.
        }
        if self.confirmed_recently(org, now, generation) {
            return Ok(true);
        }
        if denied.contains(&org) {
            return Ok(false);
        }
        if let Some(guard) = self.memberships.get(&org) {
            let allowed = guard.authorize().await;
            if allowed {
                self.confirmed.insert(org, (now, generation));
            } else {
                denied.insert(org);
            }
            return Ok(allowed);
        }
        // A membership can become available after the connection opens. Bind it
        // on first delivery, but never replace a known revision: reactivation
        // must not restore an old stream's authority.
        let Ok(Ok(guard)) = tokio::time::timeout(
            Duration::from_secs(5),
            MembershipActivityGuard::capture(&self.state.system_pool, org, self.user_id),
        )
        .await
        else {
            denied.insert(org);
            return Ok(false);
        };
        self.memberships.insert(org, guard);
        self.confirmed.insert(org, (now, generation));
        Ok(true)
    }
}

// The writer independently enforces authority even when a database check,
// subscription handler or network send is stalled.
async fn write_connection(
    mut ws_sender: impl futures_util::Sink<Message> + Unpin,
    mut rx: mpsc::Receiver<WsMessage>,
    mut authority: watch::Receiver<ConnectionAuthority>,
    writer_hub: Arc<WsHub>,
    conn_id: ConnectionId,
    mut membership: Option<MembershipDelivery>,
) {
    let mut interrupted_send = false;
    'writer: loop {
        let current = authority.borrow_and_update().clone();
        if !current.permits_events() {
            break;
        }
        let first = tokio::select! {
            biased;
            changed = authority.changed() => {
                    if changed.is_err() { break; }
                    continue;
                },
            _ = tokio::time::sleep_until(current.deadline) => break,
            msg = rx.recv() => match msg { Some(msg) => msg, None => break },
        };
        // Everything already queued is admitted as one bounded batch under
        // one membership recheck (`g5.security.membership-streams`); a
        // denial discards the whole unsent batch with the connection.
        let mut batch = vec![first];
        while batch.len() < WRITER_BATCH_MAX_EVENTS {
            match rx.try_recv() {
                Ok(msg) => batch.push(msg),
                Err(_) => break,
            }
        }
        let batch: Vec<WsMessage> = if let Some(membership) = &mut membership {
            let admitted = {
                let admission = membership.authorize_batch(&batch, &writer_hub, conn_id);
                tokio::pin!(admission);
                loop {
                    let current = authority.borrow_and_update().clone();
                    if !current.permits_events() {
                        break 'writer;
                    }
                    tokio::select! {
                        biased;
                        changed = authority.changed() => {
                            if changed.is_err() { break 'writer; }
                        },
                        _ = tokio::time::sleep_until(current.deadline) => break 'writer,
                        allowed = &mut admission => match allowed {
                            Ok(admitted) => break admitted,
                            Err(()) => {
                                writer_hub.close_connection(conn_id, 4003, "Organization access changed");
                                break 'writer;
                            }
                        },
                    }
                }
            };
            batch
                .into_iter()
                .zip(admitted)
                .filter_map(|(msg, allowed)| allowed.then_some(msg))
                .collect()
        } else {
            batch
        };
        for msg in batch {
            let ws_msg = match msg {
                WsMessage::Event(_, event) => match serde_json::to_string(&event) {
                    Ok(json) => Message::Text(json),
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to serialize WsEvent");
                        continue;
                    }
                },
                WsMessage::Control(ctrl) => match serde_json::to_string(&ctrl) {
                    Ok(json) => Message::Text(json),
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to serialize ServerMessage");
                        continue;
                    }
                },
                WsMessage::Close(code, reason) => {
                    writer_hub.close_connection(conn_id, code, &reason);
                    break 'writer;
                }
                WsMessage::Ping => Message::Ping(vec![]),
            };
            let send = ws_sender.send(ws_msg);
            tokio::pin!(send);
            loop {
                let current = authority.borrow_and_update().clone();
                if !current.permits_events() {
                    interrupted_send = true;
                    break 'writer;
                }
                tokio::select! {
                    biased;
                    changed = authority.changed() => {
                            if changed.is_err() {
                                interrupted_send = true;
                                break 'writer;
                            }
                        },
                    _ = tokio::time::sleep_until(current.deadline) => {
                        interrupted_send = true;
                        break 'writer;
                    },
                    result = &mut send => {
                        if result.is_err() {
                            interrupted_send = true;
                            break 'writer;
                        }
                        break;
                    },
                }
            }
        }
    }
    let (code, reason) = authority
        .borrow()
        .close
        .clone()
        .unwrap_or((1008, "Authorization expired".into()));
    writer_hub.disconnect(conn_id);
    // A cancelled send may have a partially buffered data frame. Drop
    // that transport instead of flushing revoked data with Close.
    if !interrupted_send {
        // A slow close handshake cannot extend an expired authorization lease.
        let close_deadline = authority
            .borrow()
            .deadline
            .min(Deadline::now() + Duration::from_secs(1));
        let _ = tokio::time::timeout_at(
            close_deadline,
            ws_sender.send(Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            }))),
        )
        .await;
    }
}

// ---------------------------------------------------------------------------
// handle_ws_connection — per-connection task
// ---------------------------------------------------------------------------

/// Async function that runs for each WebSocket connection after upgrade.
///
/// Registers the connection in the hub, splits the WebSocket, and spawns
/// reader, writer, and heartbeat tasks.
#[tracing::instrument(
    name = "ws_connection",
    skip_all,
    fields(connection_id = tracing::field::Empty, %user_id, %org_id)
)]
pub async fn handle_ws_connection(
    socket: WebSocket,
    hub: Arc<WsHub>,
    state: Arc<AppState>,
    user_id: Uuid,
    org_id: Uuid,
    claims: JwtClaims,
) {
    let admitted_at = Deadline::now();
    let Ok(Ok(memberships)) = tokio::time::timeout(
        hub.config.heartbeat_interval / 2,
        crate::services::identity::membership_activity::MembershipActivityGuard::capture_all(
            &state.system_pool,
            user_id,
        ),
    )
    .await
    else {
        return;
    };
    let connection_membership = memberships.get(&org_id).cloned();
    if !org_id.is_nil() && connection_membership.is_none() {
        return;
    }
    // 1. Register the connection in the hub
    let (conn_id, rx) = match hub.register_connection(user_id, org_id) {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(error = %e.inner, "Failed to register WebSocket connection");
            return;
        }
    };

    // The async instrumentation enters the span only while this task is polled.
    tracing::Span::current().record("connection_id", tracing::field::display(conn_id));

    // 3. Split the WebSocket into sender and receiver
    let (ws_sender, mut ws_receiver) = socket.split();

    // A separate authority signal preempts queued data and a stalled writer.
    let authority = {
        let Some(conn) = hub.connections.get(&conn_id) else {
            // A concurrent revocation can evict immediately after registration.
            return;
        };
        conn.authority.send_modify(|authority| {
            authority.deadline = admitted_at + hub.config.heartbeat_interval;
        });
        conn.authority.subscribe()
    };
    let mut writer_task = tokio::spawn(
        write_connection(
            ws_sender,
            rx,
            authority,
            hub.clone(),
            conn_id,
            Some(MembershipDelivery {
                state: state.clone(),
                user_id,
                claims: claims.clone(),
                selected: connection_membership.clone(),
                memberships,
                confirmed: HashMap::new(),
            }),
        )
        .in_current_span(),
    );

    // 5. Spawn the heartbeat task
    let heartbeat_hub = hub.clone();
    let heartbeat_conn_id = conn_id;
    let heartbeat_interval = hub.config.heartbeat_interval;
    let heartbeat_timeout = hub.config.heartbeat_timeout;
    let heartbeat_state = state.clone();
    let heartbeat_claims = claims.clone();
    let heartbeat_membership = connection_membership.clone();
    let heartbeat_task = tokio::spawn(async move {
        // Delay the first tick so we don't send a Ping immediately on
        // connection — that would race with the subscribe ack and confuse
        // clients expecting a text frame first.
        let start = tokio::time::Instant::now() + heartbeat_interval;
        let mut interval = tokio::time::interval_at(start, heartbeat_interval);
        let renewal_period = heartbeat_interval / 2;
        let mut renewal = tokio::time::interval_at(Deadline::now() + renewal_period, renewal_period);
        renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = renewal.tick() => {
                    if let Some(guard) = &heartbeat_membership {
                        if !guard.authorize().await {
                            heartbeat_hub.close_connection(heartbeat_conn_id, 4003, "Organization access changed");
                            break;
                        }
                    }
                    heartbeat_hub.reauthorize_connection(&heartbeat_state, heartbeat_conn_id, &heartbeat_claims).await;
                    if !heartbeat_hub.connections.contains_key(&heartbeat_conn_id) { break; }
                    continue;
                },
                _ = interval.tick() => {},
            }

            // Check if last pong is within timeout
            if let Some(conn) = heartbeat_hub.connections.get(&heartbeat_conn_id) {
                let elapsed = conn.last_pong.elapsed();
                if elapsed > heartbeat_interval + heartbeat_timeout {
                    tracing::warn!(
                        connection_id = %heartbeat_conn_id,
                        elapsed_secs = elapsed.as_secs(),
                        "Heartbeat timeout — closing connection"
                    );
                    if let Err(e) = conn
                        .tx
                        .try_send(WsMessage::Close(1001, "Heartbeat timeout".into()))
                    {
                        tracing::warn!(connection_id = %heartbeat_conn_id, error = %e, "Failed to send heartbeat close frame");
                    }
                    break;
                }
                // Send ping
                if let Err(e) = conn.tx.try_send(WsMessage::Ping) {
                    tracing::warn!(connection_id = %heartbeat_conn_id, error = %e, "Failed to send heartbeat ping");
                }
            } else {
                // Connection no longer exists
                break;
            }
        }
    }.in_current_span());

    // 6. Reader loop — process incoming client messages
    loop {
        let msg_result = tokio::select! {
            biased;
            _ = &mut writer_task => break,
            next = ws_receiver.next() => match next { Some(msg) => msg, None => break },
        };
        match msg_result {
            Ok(Message::Text(text)) => {
                if let Some(guard) = &connection_membership {
                    if !guard.authorize().await {
                        hub.close_connection(conn_id, 4003, "Organization access changed");
                        let _ = (&mut writer_task).await;
                        break;
                    }
                }
                handle_client_message(&hub, conn_id, state.clone(), user_id, &claims, &text).await;
            }
            Ok(Message::Pong(_)) => {
                // Update last_pong timestamp
                if let Some(mut conn) = hub.connections.get_mut(&conn_id) {
                    conn.last_pong = Instant::now();
                }
            }
            Ok(Message::Ping(_data)) => {
                // §1.22 fix: axum/tungstenite auto-responds with a proper
                // RFC 6455 Pong frame at the transport layer. The previous
                // handler incorrectly sent a JSON text frame with
                // {"type":"error","code":"PONG"} which violated the spec
                // and confused standards-compliant clients/load-balancers.
                // No application-level action is needed.
            }
            Ok(Message::Close(_)) => {
                // Client initiated close
                break;
            }
            Ok(Message::Binary(_)) => {
                // Binary messages are not supported on the event WebSocket.
                if let Some(conn) = hub.connections.get(&conn_id) {
                    if let Err(e) = conn.tx.try_send(WsMessage::Control(ServerMessage::Error {
                        code: "UNSUPPORTED_MESSAGE".into(),
                        message: "Binary messages are not supported on this endpoint".into(),
                    })) {
                        tracing::warn!(connection_id = %conn_id, error = %e, "ws_message_dropped: failed to send UNSUPPORTED_MESSAGE error");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    connection_id = %conn_id,
                    error = %e,
                    "WebSocket receive error"
                );
                break;
            }
        }
    }

    // 7. Cleanup: abort heartbeat and writer tasks, disconnect from hub
    heartbeat_task.abort();
    writer_task.abort();
    hub.disconnect(conn_id);
}

// ---------------------------------------------------------------------------
// handle_client_message — client message processing
// ---------------------------------------------------------------------------

/// Processes a single text message from the client.
async fn handle_client_message(
    hub: &Arc<WsHub>,
    conn_id: ConnectionId,
    state: Arc<AppState>,
    user_id: Uuid,
    claims: &JwtClaims,
    text: &str,
) {
    let Some(gate) = hub
        .connections
        .get(&conn_id)
        .map(|conn| conn.authorization_gate.clone())
    else {
        return;
    };
    let _guard = gate.lock().await;
    let Some(deadline) = hub
        .connections
        .get(&conn_id)
        .map(|conn| conn.authority.borrow().deadline)
    else {
        return;
    };
    // 1. Parse the JSON message
    let client_msg: ClientMessage = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(e) => {
            tracing::debug!(
                connection_id = %conn_id,
                error = %e,
                "Invalid client message"
            );
            if let Some(conn) = hub.connections.get(&conn_id) {
                if let Err(e) = conn.tx.try_send(WsMessage::Control(ServerMessage::Error {
                    code: "INVALID_MESSAGE".into(),
                    message: format!("Failed to parse message: {}", e),
                })) {
                    tracing::warn!(connection_id = %conn_id, error = %e, "ws_message_dropped: failed to send INVALID_MESSAGE error");
                }
            }
            return;
        }
    };

    match client_msg {
        ClientMessage::Subscribe {
            channel: channel_str,
        } => {
            // 2a. Parse the channel string
            let channel = match Channel::parse(&channel_str) {
                Ok(ch) => ch,
                Err(e) => {
                    if let Some(conn) = hub.connections.get(&conn_id) {
                        if let Err(send_err) =
                            conn.tx.try_send(WsMessage::Control(ServerMessage::Error {
                                code: "INVALID_CHANNEL".into(),
                                message: format!("Invalid channel format: {}", e),
                            }))
                        {
                            tracing::warn!(connection_id = %conn_id, error = %send_err, "ws_message_dropped: failed to send INVALID_CHANNEL error");
                        }
                    }
                    return;
                }
            };

            // A blocked admission cannot outlive the connection's authority.
            let scope = match tokio::time::timeout_at(
                deadline,
                channels::authorize_subscribe(&state, user_id, claims, &channel),
            )
            .await
            {
                Ok(Ok(scope)) => scope,
                Ok(Err(e)) => {
                    if let Some(conn) = hub.connections.get(&conn_id) {
                        let _ = conn.tx.try_send(WsMessage::Control(ServerMessage::Error {
                            code: "FORBIDDEN".into(),
                            message: format!(
                                "Not authorized to subscribe to {}: {}",
                                channel_str, e.inner
                            ),
                        }));
                    }
                    return;
                }
                Err(_) => {
                    hub.close_connection(conn_id, 1008, "Authorization expired");
                    return;
                }
            };

            // 2c. Add the subscription
            if let Err(e) = hub.subscribe(conn_id, channel, scope) {
                tracing::error!(error = %e.inner, "Failed to add subscription");
                return;
            }

            // 2d. Send confirmation
            if let Some(conn) = hub.connections.get(&conn_id) {
                if let Err(e) = conn
                    .tx
                    .try_send(WsMessage::Control(ServerMessage::Subscribed {
                        channel: channel_str.clone(),
                    }))
                {
                    tracing::warn!(connection_id = %conn_id, error = %e, "ws_message_dropped: failed to send Subscribed confirmation");
                }
            }

            tracing::debug!(
                connection_id = %conn_id,
                channel = %channel_str,
                "Client subscribed to channel"
            );
        }

        ClientMessage::Unsubscribe {
            channel: channel_str,
        } => {
            // 3a. Parse the channel string
            let channel = match Channel::parse(&channel_str) {
                Ok(ch) => ch,
                Err(e) => {
                    if let Some(conn) = hub.connections.get(&conn_id) {
                        if let Err(send_err) =
                            conn.tx.try_send(WsMessage::Control(ServerMessage::Error {
                                code: "INVALID_CHANNEL".into(),
                                message: format!("Invalid channel format: {}", e),
                            }))
                        {
                            tracing::warn!(connection_id = %conn_id, error = %send_err, "ws_message_dropped: failed to send INVALID_CHANNEL error on unsubscribe");
                        }
                    }
                    return;
                }
            };

            // 3b. Remove the subscription
            hub.unsubscribe(conn_id, &channel);

            // 3c. Send confirmation
            if let Some(conn) = hub.connections.get(&conn_id) {
                if let Err(e) = conn
                    .tx
                    .try_send(WsMessage::Control(ServerMessage::Unsubscribed {
                        channel: channel_str.clone(),
                    }))
                {
                    tracing::warn!(connection_id = %conn_id, error = %e, "ws_message_dropped: failed to send Unsubscribed confirmation");
                }
            }

            tracing::debug!(
                connection_id = %conn_id,
                channel = %channel_str,
                "Client unsubscribed from channel"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn test_ws_upgrade_params_deserialize() {
        let json = r#"{"token":"my-jwt-token"}"#;
        let params: WsUpgradeParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.token, "my-jwt-token");
    }

    use super::*;

    fn make_hub() -> WsHub {
        WsHub::new(WsHubConfig {
            max_connections: 100,
            writer_channel_capacity: 16,
            ..WsHubConfig::default()
        })
    }

    #[tokio::test]
    async fn eviction_preempts_a_full_queue_and_cleans_indirect_subscriptions() {
        let hub = Arc::new(make_hub());
        let user = Uuid::new_v4();
        let org = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let channel = Channel::Chat(Uuid::new_v4());
        let (id, rx) = hub.register_connection(user, org).unwrap();
        hub.subscribe(
            id,
            channel.clone(),
            SubscriptionScope::Workspace {
                org_id: org,
                workspace_id: workspace,
            },
        )
        .unwrap();
        let authority = hub.connections.get(&id).unwrap().authority.subscribe();
        for _ in 0..hub.config.writer_channel_capacity {
            hub.publish(
                &channel,
                &WsEvent::TaskStatus {
                    task_id: Uuid::nil(),
                    status: "queued private data".into(),
                },
            );
        }
        assert_eq!(rx.len(), hub.config.writer_channel_capacity);
        hub.evict(user, RevocationScope::Workspace(workspace));
        assert_eq!(hub.connection_count(), 0);
        assert_eq!(hub.channel_subscriber_count(&channel), 0);
        assert!(hub.subscribe(id, channel, SubscriptionScope::User).is_err());
        let (sink, mut output) = futures::channel::mpsc::unbounded();
        write_connection(sink, rx, authority, hub, id, None).await;
        assert!(
            matches!(output.next().await, Some(Message::Close(Some(frame))) if frame.code == 1008)
        );
        assert!(
            output.next().await.is_none(),
            "queued data must be discarded"
        );
    }

    /// A client that falls more than `max_dropped_events` behind is closed
    /// even though its queue is full: the close travels through the
    /// connection's authority, not the queue, and the queued data is
    /// discarded with it.
    #[tokio::test]
    async fn a_client_too_slow_is_closed_ahead_of_its_full_queue() {
        let hub = Arc::new(make_hub());
        let user = Uuid::new_v4();
        let org = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let channel = Channel::Chat(Uuid::new_v4());
        let (id, rx) = hub.register_connection(user, org).unwrap();
        hub.subscribe(
            id,
            channel.clone(),
            SubscriptionScope::Workspace {
                org_id: org,
                workspace_id: workspace,
            },
        )
        .unwrap();
        let authority = hub.connections.get(&id).unwrap().authority.subscribe();
        let publishes =
            hub.config.writer_channel_capacity as u64 + hub.config.max_dropped_events + 1;
        for _ in 0..publishes {
            hub.publish(
                &channel,
                &WsEvent::TaskStatus {
                    task_id: Uuid::nil(),
                    status: "running".into(),
                },
            );
        }
        assert_eq!(rx.len(), hub.config.writer_channel_capacity);
        assert_eq!(hub.connection_count(), 0, "the slow client is unregistered");
        assert_eq!(hub.channel_subscriber_count(&channel), 0);
        let (sink, mut output) = futures::channel::mpsc::unbounded();
        write_connection(sink, rx, authority, hub, id, None).await;
        assert!(matches!(
            output.next().await,
            Some(Message::Close(Some(frame)))
                if frame.code == 1008 && frame.reason.contains("client too slow")
        ));
        assert!(
            output.next().await.is_none(),
            "queued data is discarded with the close"
        );
    }

    #[test]
    fn eviction_matches_resolved_org_and_preserves_unaffected_connections() {
        let hub = make_hub();
        let user = Uuid::new_v4();
        let org = Uuid::new_v4();
        let other_org = Uuid::new_v4();
        let (bound, _bound_rx) = hub.register_connection(user, org).unwrap();
        let (indirect, _indirect_rx) = hub.register_connection(user, other_org).unwrap();
        let (unaffected, _unaffected_rx) = hub.register_connection(user, other_org).unwrap();
        let (other_user, _other_user_rx) = hub.register_connection(Uuid::new_v4(), org).unwrap();
        hub.subscribe(
            indirect,
            Channel::Task(Uuid::new_v4()),
            SubscriptionScope::Workspace {
                org_id: org,
                workspace_id: Uuid::new_v4(),
            },
        )
        .unwrap();
        hub.evict(user, RevocationScope::Organization(org));
        assert!(!hub.connections.contains_key(&bound));
        assert!(!hub.connections.contains_key(&indirect));
        assert!(hub.connections.contains_key(&unaffected));
        assert!(hub.connections.contains_key(&other_user));
    }

    #[tokio::test(start_paused = true)]
    async fn expired_authority_blocks_both_fanout_paths_and_new_subscriptions() {
        let hub = make_hub();
        let user = Uuid::new_v4();
        let (id, mut rx) = hub.register_connection(user, Uuid::new_v4()).unwrap();
        let channel = Channel::User(user);
        hub.subscribe(id, channel.clone(), SubscriptionScope::User)
            .unwrap();
        tokio::time::advance(hub.config.heartbeat_interval).await;
        let event = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "private".into(),
        };
        hub.publish(&channel, &event);
        hub.publish_to_user(&channel, user, &event);
        assert!(rx.try_recv().is_err());
        assert!(hub.subscribe(id, channel, SubscriptionScope::User).is_err());
    }

    struct StalledSink {
        started: Option<tokio::sync::oneshot::Sender<()>>,
        close_attempted: Arc<std::sync::atomic::AtomicBool>,
    }

    impl futures_util::Sink<Message> for StalledSink {
        type Error = std::convert::Infallible;
        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn start_send(
            mut self: std::pin::Pin<&mut Self>,
            item: Message,
        ) -> Result<(), Self::Error> {
            if matches!(item, Message::Close(_)) {
                self.close_attempted.store(true, Ordering::SeqCst);
            }
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            Ok(())
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Pending
        }
        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Pending
        }
    }

    #[tokio::test(start_paused = true)]
    async fn revocation_or_expiry_interrupts_a_stalled_send_without_flushing_data() {
        for expire in [false, true] {
            let hub = Arc::new(make_hub());
            let user = Uuid::new_v4();
            let org = Uuid::new_v4();
            let (id, rx) = hub.register_connection(user, org).unwrap();
            let channel = Channel::User(user);
            hub.subscribe(id, channel.clone(), SubscriptionScope::User)
                .unwrap();
            let authority = hub.connections.get(&id).unwrap().authority.subscribe();
            hub.publish(
                &channel,
                &WsEvent::TaskStatus {
                    task_id: Uuid::nil(),
                    status: "in flight".into(),
                },
            );
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let close_attempted = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let writer = tokio::spawn(write_connection(
                StalledSink {
                    started: Some(started_tx),
                    close_attempted: close_attempted.clone(),
                },
                rx,
                authority,
                hub.clone(),
                id,
                None,
            ));
            started_rx.await.unwrap();
            if expire {
                tokio::time::advance(hub.config.heartbeat_interval).await;
            } else {
                hub.evict(user, RevocationScope::Organization(org));
            }
            tokio::time::timeout(Duration::from_secs(1), writer)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(hub.connection_count(), 0);
            assert!(
                !close_attempted.load(Ordering::SeqCst),
                "Close must not flush an interrupted data frame"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn expired_socket_does_not_wait_for_a_blocked_close_handshake() {
        let hub = Arc::new(make_hub());
        let (id, rx) = hub
            .register_connection(Uuid::new_v4(), Uuid::nil())
            .unwrap();
        let authority = hub.connections.get(&id).unwrap().authority.subscribe();
        tokio::time::advance(hub.config.heartbeat_interval).await;
        let expired_at = Deadline::now();
        write_connection(
            StalledSink {
                started: None,
                close_attempted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            rx,
            authority,
            hub.clone(),
            id,
            None,
        )
        .await;
        assert_eq!(
            Deadline::now(),
            expired_at,
            "close must not add a grace period to the authority bound"
        );
        assert_eq!(hub.connection_count(), 0);
    }

    #[test]
    fn concurrent_subscription_and_disconnect_cannot_leave_orphan_subscribers() {
        let hub = Arc::new(make_hub());
        for _ in 0..32 {
            let (id, _rx) = hub
                .register_connection(Uuid::new_v4(), Uuid::new_v4())
                .unwrap();
            let channel = Channel::Chat(Uuid::new_v4());
            let barrier = Arc::new(std::sync::Barrier::new(2));
            std::thread::scope(|threads| {
                threads.spawn(|| {
                    barrier.wait();
                    let _ = hub.subscribe(id, channel.clone(), SubscriptionScope::User);
                });
                barrier.wait();
                hub.disconnect(id);
            });
            assert_eq!(hub.channel_subscriber_count(&channel), 0);
        }
    }

    #[test]
    fn test_connection_id_new_unique() {
        let id1 = ConnectionId::new();
        let id2 = ConnectionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_connection_id_display() {
        let id = ConnectionId(Uuid::nil());
        assert_eq!(id.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn test_connection_id_hash_eq() {
        use std::collections::HashSet;
        let id = ConnectionId::new();
        let mut set = HashSet::new();
        set.insert(id);
        set.insert(id); // duplicate
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_ws_hub_config_default() {
        let config = WsHubConfig::default();
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
        assert_eq!(config.heartbeat_timeout, Duration::from_secs(10));
        assert_eq!(config.max_connections, 10_000);
        assert_eq!(config.writer_channel_capacity, 2048);
        assert_eq!(config.max_dropped_events, 1000);
        assert_eq!(config.dropped_events_window, Duration::from_secs(60));
        assert_eq!(config.shutdown_drain_timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_hub_new_empty() {
        let hub = make_hub();
        assert_eq!(hub.connection_count(), 0);
    }

    #[test]
    fn test_hub_register_connection() {
        let hub = make_hub();
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();

        let result = hub.register_connection(user_id, org_id);
        assert!(result.is_ok());

        let (conn_id, _rx) = result.unwrap();
        assert_eq!(hub.connection_count(), 1);

        // Verify connection state
        let conn = hub.connections.get(&conn_id).unwrap();
        assert_eq!(conn.user_id, user_id);
        assert_eq!(conn.org_id, org_id);
        assert!(conn.subscriptions.is_empty());
    }

    #[test]
    fn test_hub_register_connection_limit() {
        let hub = WsHub::new(WsHubConfig {
            max_connections: 2,
            writer_channel_capacity: 4,
            ..WsHubConfig::default()
        });

        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();

        let _r1 = hub.register_connection(user_id, org_id).unwrap();
        let _r2 = hub.register_connection(user_id, org_id).unwrap();
        let r3 = hub.register_connection(user_id, org_id);
        assert!(r3.is_err());
    }

    #[test]
    fn test_hub_subscribe_unsubscribe() {
        let hub = make_hub();
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();

        let (conn_id, _rx) = hub.register_connection(user_id, org_id).unwrap();
        let channel = Channel::Workspace(Uuid::new_v4());

        // Subscribe
        hub.subscribe(conn_id, channel.clone(), SubscriptionScope::User)
            .unwrap();
        assert_eq!(hub.channel_subscriber_count(&channel), 1);

        // Verify connection has the subscription
        let conn = hub.connections.get(&conn_id).unwrap();
        assert!(conn.subscriptions.contains_key(&channel));
        drop(conn);

        // Unsubscribe
        hub.unsubscribe(conn_id, &channel);
        assert_eq!(hub.channel_subscriber_count(&channel), 0);
    }

    #[test]
    fn test_hub_subscribe_nonexistent_connection() {
        let hub = make_hub();
        let fake_conn_id = ConnectionId::new();
        let channel = Channel::Workspace(Uuid::new_v4());

        let result = hub.subscribe(fake_conn_id, channel, SubscriptionScope::User);
        assert!(result.is_err());
    }

    #[test]
    fn test_hub_disconnect_cleans_subscriptions() {
        let hub = make_hub();
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();

        let (conn_id, _rx) = hub.register_connection(user_id, org_id).unwrap();
        let ch1 = Channel::Workspace(Uuid::new_v4());
        let ch2 = Channel::Task(Uuid::new_v4());

        hub.subscribe(conn_id, ch1.clone(), SubscriptionScope::User)
            .unwrap();
        hub.subscribe(conn_id, ch2.clone(), SubscriptionScope::User)
            .unwrap();

        assert_eq!(hub.channel_subscriber_count(&ch1), 1);
        assert_eq!(hub.channel_subscriber_count(&ch2), 1);

        hub.disconnect(conn_id);

        assert_eq!(hub.connection_count(), 0);
        assert_eq!(hub.channel_subscriber_count(&ch1), 0);
        assert_eq!(hub.channel_subscriber_count(&ch2), 0);
    }

    #[test]
    fn test_hub_publish_to_subscriber() {
        let hub = make_hub();
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();

        let (conn_id, mut rx) = hub.register_connection(user_id, org_id).unwrap();
        let channel = Channel::Workspace(Uuid::new_v4());
        hub.subscribe(conn_id, channel.clone(), SubscriptionScope::User)
            .unwrap();

        let event = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "running".to_string(),
        };

        hub.publish(&channel, &event);

        // Verify the event was received on the mpsc channel
        let msg = rx.try_recv();
        assert!(msg.is_ok());
    }

    #[test]
    fn owner_only_publish_filters_readers_on_the_workspace_channel() {
        let hub = make_hub();
        let owner_id = Uuid::new_v4();
        let reader_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let (owner_connection, mut owner_rx) = hub.register_connection(owner_id, org_id).unwrap();
        let (reader_connection, mut reader_rx) =
            hub.register_connection(reader_id, org_id).unwrap();
        let channel = Channel::Workspace(Uuid::new_v4());
        hub.subscribe(owner_connection, channel.clone(), SubscriptionScope::User)
            .unwrap();
        hub.subscribe(reader_connection, channel.clone(), SubscriptionScope::User)
            .unwrap();
        let event = WsEvent::WorkspaceFileChanged {
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            path: "src/main.rs".to_string(),
            rescan_required: false,
        };

        hub.publish_to_user(&channel, owner_id, &event);

        assert!(owner_rx.try_recv().is_ok());
        assert!(matches!(
            reader_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn test_hub_publish_no_subscribers() {
        let hub = make_hub();
        let channel = Channel::Workspace(Uuid::new_v4());
        let event = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "running".to_string(),
        };

        // Should not panic
        hub.publish(&channel, &event);
    }

    #[test]
    fn test_hub_multiple_subscribers() {
        let hub = make_hub();
        let channel = Channel::Workspace(Uuid::new_v4());

        let (conn_id1, mut rx1) = hub
            .register_connection(Uuid::new_v4(), Uuid::new_v4())
            .unwrap();
        let (conn_id2, mut rx2) = hub
            .register_connection(Uuid::new_v4(), Uuid::new_v4())
            .unwrap();

        hub.subscribe(conn_id1, channel.clone(), SubscriptionScope::User)
            .unwrap();
        hub.subscribe(conn_id2, channel.clone(), SubscriptionScope::User)
            .unwrap();

        assert_eq!(hub.channel_subscriber_count(&channel), 2);

        let event = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "succeeded".to_string(),
        };

        hub.publish(&channel, &event);

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_hub_channel_subscriber_count_empty() {
        let hub = make_hub();
        let channel = Channel::Workspace(Uuid::new_v4());
        assert_eq!(hub.channel_subscriber_count(&channel), 0);
    }

    #[test]
    fn test_hub_disconnect_nonexistent() {
        let hub = make_hub();
        // Should not panic
        hub.disconnect(ConnectionId::new());
    }

    #[test]
    fn test_connection_id_default() {
        let id = ConnectionId::default();
        // Just verify it creates a valid ID
        assert_ne!(id.0, Uuid::nil());
    }

    #[test]
    fn test_reexports_available() {
        // Verify that re-exports are accessible
        let _channel_str = Channel::Workspace(Uuid::nil()).to_channel_string();
        let _event_type = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "queued".to_string(),
        }
        .event_type();
    }
}
