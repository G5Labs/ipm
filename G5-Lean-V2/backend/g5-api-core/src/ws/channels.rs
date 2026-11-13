use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::db::scoped::{
    ensure_user_workspace_access, resolve_chat_session_context, resolve_task_context,
    resolve_workspace_context, scoped_db_for_user_orgs,
};
use crate::errors::{AppError, G5Error};
use crate::middleware::AuthContext;
use crate::services::AppState;
use g5_auth::JwtClaims;
// Channel enum
// ---------------------------------------------------------------------------

/// Represents the public and internal channel types supported by the WebSocket hub.
///
/// Each variant carries the entity identifier(s) needed to uniquely identify
/// the channel. `Channel` is used as a key in `DashMap<Channel, HashSet<ConnectionId>>`
/// (the reverse subscription index in the hub), which is why it derives `Hash`
/// and `Eq`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Workspace-level events: sync status, spend limits, settings, comments.
    Workspace(Uuid),
    /// Task-level events: status changes, timeline entries, diffs.
    Task(Uuid),
    /// Chat session events: streaming chunks, completion, errors.
    Chat(Uuid),
    /// User-level events: private mention notifications, direct messages.
    /// Each user can only subscribe to their own `user:<uuid>` channel.
    User(Uuid),
    /// Org-level broadcasts. Every org member of org X may subscribe to
    /// `Channel::Org(X)`.
    Org(Uuid),
}

/// Resource authority resolved at subscription admission, including indirect
/// workspace channels such as chat sessions and tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionScope {
    User,
    Organization(Uuid),
    Workspace { org_id: Uuid, workspace_id: Uuid },
}

/// Check live membership even when the browser presents an older valid ticket.
pub async fn authorize_org(
    state: &Arc<AppState>,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<(), AppError> {
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL)",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(&state.system_pool)
    .await?;
    if !member {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec![],
        }));
    }
    Ok(())
}

impl Channel {
    /// Parses a client-provided channel string into a typed `Channel` value.
    ///
    /// # Channel String Format
    ///
    /// - `"workspace:<uuid>"` → `Channel::Workspace(uuid)`
    /// - `"task:<uuid>"` → `Channel::Task(uuid)`
    /// - `"chat:<uuid>"` → `Channel::Chat(uuid)`
    ///
    /// # Errors
    ///
    /// - `ChannelParseError::InvalidFormat` — No `:` separator.
    /// - `ChannelParseError::UnknownType` — Unrecognized channel type prefix.
    /// - `ChannelParseError::InvalidId` — Entity ID is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self, ChannelParseError> {
        let (channel_type, rest) = s.split_once(':').ok_or(ChannelParseError::InvalidFormat)?;

        match channel_type {
            "workspace" => {
                let id = Uuid::parse_str(rest).map_err(|_| ChannelParseError::InvalidId)?;
                Ok(Channel::Workspace(id))
            }
            "task" => {
                let id = Uuid::parse_str(rest).map_err(|_| ChannelParseError::InvalidId)?;
                Ok(Channel::Task(id))
            }
            "chat" => {
                let id = Uuid::parse_str(rest).map_err(|_| ChannelParseError::InvalidId)?;
                Ok(Channel::Chat(id))
            }
            "user" => {
                let id = Uuid::parse_str(rest).map_err(|_| ChannelParseError::InvalidId)?;
                Ok(Channel::User(id))
            }
            "org" => {
                let id = Uuid::parse_str(rest).map_err(|_| ChannelParseError::InvalidId)?;
                Ok(Channel::Org(id))
            }
            _ => Err(ChannelParseError::UnknownType),
        }
    }

    /// Converts a typed `Channel` back to its string representation.
    ///
    /// Used in server messages (subscription confirmations, error messages).
    #[must_use]
    pub fn to_channel_string(&self) -> String {
        match self {
            Channel::Workspace(id) => format!("workspace:{}", id),
            Channel::Task(id) => format!("task:{}", id),
            Channel::Chat(id) => format!("chat:{}", id),
            Channel::User(id) => format!("user:{}", id),
            Channel::Org(id) => format!("org:{}", id),
        }
    }
}

impl std::fmt::Display for Channel {
    /// Delegates to [`Channel::to_channel_string()`] for convenient logging
    /// and tracing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_channel_string())
    }
}

// ---------------------------------------------------------------------------
// ChannelParseError
// ---------------------------------------------------------------------------

/// Error type for channel string parsing failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelParseError {
    /// The channel string has no `:` separator or is otherwise malformed.
    InvalidFormat,
    /// The channel type prefix is not recognized.
    UnknownType,
    /// The entity ID portion is not a valid UUID.
    InvalidId,
}

impl std::fmt::Display for ChannelParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "invalid channel format"),
            Self::UnknownType => write!(f, "unknown channel type"),
            Self::InvalidId => write!(f, "invalid channel entity ID"),
        }
    }
}

impl std::error::Error for ChannelParseError {}

// ---------------------------------------------------------------------------
// ClientMessage — client-to-server messages
// ---------------------------------------------------------------------------

/// Represents the two message types a client can send over the WebSocket:
/// subscribe and unsubscribe.
///
/// Deserialized from JSON using serde's internally-tagged representation.
/// The `channel` field is a raw string because deserialization happens before
/// validation — the hub's message handler calls [`Channel::parse()`] on the
/// string after deserialization, which allows it to return a structured
/// [`ServerMessage::Error`] to the client if parsing fails.
///
/// # JSON Format
///
/// ```json
/// { "type": "subscribe",   "channel": "workspace:550e8400-..." }
/// { "type": "unsubscribe", "channel": "workspace:550e8400-..." }
/// ```
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Subscribe to a channel.
    #[serde(rename = "subscribe")]
    Subscribe {
        /// Raw channel string (e.g., `"workspace:<uuid>"`).
        channel: String,
    },
    /// Unsubscribe from a channel.
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        /// Raw channel string (e.g., `"workspace:<uuid>"`).
        channel: String,
    },
}

// ---------------------------------------------------------------------------
// ServerMessage — server-to-client control messages
// ---------------------------------------------------------------------------

/// Represents control messages sent from the server to the client in response
/// to subscribe/unsubscribe requests or when errors occur.
///
/// These are NOT event messages (events are defined in `ws/events.rs` as
/// `WsEvent`). Server messages are used for subscription lifecycle management
/// and error reporting.
///
/// # JSON Format
///
/// ```json
/// { "type": "subscribed",   "channel": "workspace:550e8400-..." }
/// { "type": "unsubscribed", "channel": "workspace:550e8400-..." }
/// { "type": "error",        "code": "INVALID_CHANNEL", "message": "..." }
/// ```
///
/// # Error Codes
///
/// - `"INVALID_CHANNEL"` — Channel string could not be parsed.
/// - `"FORBIDDEN"` — User does not have access to the entity.
/// - `"INVALID_MESSAGE"` — Client sent invalid JSON or unrecognized message type.
/// - `"ALREADY_SUBSCRIBED"` — Client is already subscribed (informational, idempotent).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Subscription confirmed.
    #[serde(rename = "subscribed")]
    Subscribed {
        /// Channel string that was subscribed to.
        channel: String,
    },
    /// Unsubscription confirmed.
    #[serde(rename = "unsubscribed")]
    Unsubscribed {
        /// Channel string that was unsubscribed from.
        channel: String,
    },
    /// Error response.
    #[serde(rename = "error")]
    Error {
        /// Machine-readable error code (e.g., `"INVALID_CHANNEL"`, `"FORBIDDEN"`).
        code: String,
        /// Human-readable error message.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// ProcessedMessage — result of message processing
// ---------------------------------------------------------------------------

/// The result of successfully processing a client message.
///
/// Returned by [`process_client_message`] when the message is valid and
/// authorized. The hub's reader task uses this to perform the actual
/// subscription/unsubscription operation.
pub enum ProcessedMessage {
    /// Client wants to subscribe to the given channel (already authorized).
    Subscribe(Channel, SubscriptionScope),
    /// Client wants to unsubscribe from the given channel.
    Unsubscribe(Channel),
}

// ---------------------------------------------------------------------------
// Subscription Authorization
// ---------------------------------------------------------------------------

/// Authorizes subscriptions on admission, delivery and bounded live reauthorization.
/// Workspace access is resolved directly for task and chat channels; organization
/// and user channels remain bounded by the ticket and current membership.
#[tracing::instrument(skip(state, claims), fields(%user_id, channel = %channel))]
pub async fn authorize_subscribe(
    state: &Arc<AppState>,
    user_id: Uuid,
    claims: &JwtClaims,
    channel: &Channel,
) -> Result<SubscriptionScope, AppError> {
    match channel {
        Channel::User(target_user_id) => {
            // Users can only subscribe to their own user channel.
            if *target_user_id != user_id {
                return Err(AppError::from_error(G5Error::Forbidden {
                    missing_scopes: vec![format!(
                        "subscribe to user:{} (you are {})",
                        target_user_id, user_id
                    )],
                }));
            }
            let org_id = Uuid::parse_str(&claims.org).map_err(|_| {
                AppError::from_error(G5Error::Forbidden {
                    missing_scopes: vec![],
                })
            })?;
            if !org_id.is_nil() {
                authorize_org(state, user_id, org_id).await?;
            }
            return Ok(SubscriptionScope::User);
        }
        Channel::Org(target_org_id) => {
            // The claim bounds the selected organization; current database
            // membership supplies authority, including after offboarding.
            let user_org_id = Uuid::parse_str(&claims.org).map_err(|_| {
                AppError::from_error(G5Error::Internal {
                    message: "Invalid org ID in JWT claims".into(),
                })
            })?;
            if *target_org_id != user_org_id {
                return Err(AppError::from_error(G5Error::Forbidden {
                    missing_scopes: vec![format!(
                        "subscribe to org:{} (you are in org:{})",
                        target_org_id, user_org_id
                    )],
                }));
            }
            authorize_org(state, user_id, *target_org_id).await?;
            return Ok(SubscriptionScope::Organization(*target_org_id));
        }
        _ => {}
    }

    let auth = AuthContext::User(claims.clone());
    let mut db = scoped_db_for_user_orgs(state, &auth, &[]).await?;

    let context = match channel {
        Channel::Workspace(id) => resolve_workspace_context(&mut db, *id).await?,
        Channel::Task(id) => resolve_task_context(&mut db, *id).await?,
        Channel::Chat(session_id) => resolve_chat_session_context(&mut db, *session_id).await?,
        Channel::User(_) | Channel::Org(_) => {
            unreachable!("handled above")
        }
    };

    ensure_user_workspace_access(
        &state.system_pool,
        &mut db,
        user_id,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    Ok(SubscriptionScope::Workspace {
        org_id: context.org_id,
        workspace_id: context.workspace_id,
    })
}

/// Resolve each event's tenant, including account-wide user notifications.
pub async fn delivery_org(
    state: &Arc<AppState>,
    user_id: Uuid,
    claims: &JwtClaims,
    channel: &Channel,
    event: &super::WsEvent,
) -> Result<Uuid, AppError> {
    let protected = match (channel, event) {
        (Channel::User(_), super::WsEvent::Notification { workspace_id, .. }) => {
            Channel::Workspace(*workspace_id)
        }
        (Channel::User(_), _) => {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec![],
            }))
        }
        _ => channel.clone(),
    };
    match authorize_subscribe(state, user_id, claims, &protected).await? {
        SubscriptionScope::Organization(org_id) | SubscriptionScope::Workspace { org_id, .. } => {
            Ok(org_id)
        }
        SubscriptionScope::User => unreachable!("user notifications resolve their workspace"),
    }
}

/// Reusable fixed workspace audience check (`g5.workspace.audience`).
/// Requires active organization membership, admits ordinary workspaces,
/// restricts personal workspaces to their owner and organization admins,
/// and excludes formal review runtimes before applying either exception.
///
/// # Errors
///
/// - `AppError` with `G5Error::NotFound` if the workspace does not exist or
///   belongs to a different org.
/// - `AppError` with `G5Error::Forbidden` if the user does not have access.
/// - `AppError` with `G5Error::Internal` for database errors.
pub async fn check_workspace_access(
    system_pool: &sqlx::PgPool,
    db: &mut g5_authz::ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), AppError> {
    ensure_user_workspace_access(system_pool, db, user_id, org_id, workspace_id).await
}

// ---------------------------------------------------------------------------
// Message Processing Helper
// ---------------------------------------------------------------------------

/// Processes a raw client message through the full lifecycle: deserialize,
/// parse channel, authorize, and produce the appropriate result.
///
/// This is called by the hub's per-connection reader task.
///
/// # Returns
///
/// - `Ok(ProcessedMessage::Subscribe(channel, scope))` — Message is valid, channel
///   parsed, and authorization succeeded. The hub should add the subscription.
/// - `Ok(ProcessedMessage::Unsubscribe(channel))` — Message is valid, channel
///   parsed. The hub should remove the subscription.
/// - `Err(ServerMessage::Error { .. })` — An error occurred at some stage.
///   The hub should send this error to the client. The connection is NOT closed.
///
/// # Processing Steps
///
/// 1. Deserialize `raw_text` as `ClientMessage`.
/// 2. Extract the channel string.
/// 3. Parse the channel string with `Channel::parse()`.
/// 4. For `Subscribe`: call `authorize_subscribe()`.
/// 5. Return the processed message or an error.
pub async fn process_client_message(
    raw_text: &str,
    user_id: Uuid,
    claims: &JwtClaims,
    state: &Arc<AppState>,
) -> Result<ProcessedMessage, ServerMessage> {
    // Step 1: Deserialize the client message.
    let client_msg: ClientMessage =
        serde_json::from_str(raw_text).map_err(|e| ServerMessage::Error {
            code: "INVALID_MESSAGE".to_string(),
            message: format!("Failed to parse message: {}", e),
        })?;

    // Step 2 & 3: Extract and parse the channel string.
    match client_msg {
        ClientMessage::Subscribe { ref channel } => {
            let parsed_channel = Channel::parse(channel).map_err(|e| ServerMessage::Error {
                code: "INVALID_CHANNEL".to_string(),
                message: format!("Invalid channel '{}': {}", channel, e),
            })?;

            // Step 4: Authorize the subscription.
            let scope = authorize_subscribe(state, user_id, claims, &parsed_channel)
                .await
                .map_err(|e| ServerMessage::Error {
                    code: "FORBIDDEN".to_string(),
                    message: format!(
                        "Not authorized to subscribe to channel '{}': {}",
                        channel, e.inner
                    ),
                })?;

            Ok(ProcessedMessage::Subscribe(parsed_channel, scope))
        }
        ClientMessage::Unsubscribe { ref channel } => {
            let parsed_channel = Channel::parse(channel).map_err(|e| ServerMessage::Error {
                code: "INVALID_CHANNEL".to_string(),
                message: format!("Invalid channel '{}': {}", channel, e),
            })?;

            // No authorization needed for unsubscribe.
            Ok(ProcessedMessage::Unsubscribe(parsed_channel))
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Channel parsing tests --

    #[test]
    fn test_parse_workspace_channel() {
        let uuid = Uuid::new_v4();
        let input = format!("workspace:{}", uuid);
        let channel = Channel::parse(&input).unwrap();
        assert_eq!(channel, Channel::Workspace(uuid));
    }

    #[test]
    fn test_parse_task_channel() {
        let uuid = Uuid::new_v4();
        let input = format!("task:{}", uuid);
        let channel = Channel::parse(&input).unwrap();
        assert_eq!(channel, Channel::Task(uuid));
    }

    #[test]
    fn test_parse_chat_channel() {
        let uuid = Uuid::new_v4();
        let input = format!("chat:{}", uuid);
        let channel = Channel::parse(&input).unwrap();
        assert_eq!(channel, Channel::Chat(uuid));
    }

    #[test]
    fn test_parse_unknown_type() {
        let result = Channel::parse("unknown:123");
        assert_eq!(result, Err(ChannelParseError::UnknownType));
    }

    #[test]
    fn test_parse_invalid_uuid() {
        let result = Channel::parse("workspace:not-a-uuid");
        assert_eq!(result, Err(ChannelParseError::InvalidId));
    }

    #[test]
    fn test_parse_no_colon() {
        let result = Channel::parse("nocolon");
        assert_eq!(result, Err(ChannelParseError::InvalidFormat));
    }

    // -- Channel serialization (round-trip) tests --

    #[test]
    fn test_roundtrip_workspace() {
        let uuid = Uuid::new_v4();
        let channel = Channel::Workspace(uuid);
        let s = channel.to_channel_string();
        let parsed = Channel::parse(&s).unwrap();
        assert_eq!(parsed, channel);
    }

    #[test]
    fn test_roundtrip_task() {
        let uuid = Uuid::new_v4();
        let channel = Channel::Task(uuid);
        let s = channel.to_channel_string();
        let parsed = Channel::parse(&s).unwrap();
        assert_eq!(parsed, channel);
    }

    #[test]
    fn test_roundtrip_chat() {
        let uuid = Uuid::new_v4();
        let channel = Channel::Chat(uuid);
        let s = channel.to_channel_string();
        let parsed = Channel::parse(&s).unwrap();
        assert_eq!(parsed, channel);
    }

    // -- Display tests --

    #[test]
    fn test_display_delegates_to_channel_string() {
        let uuid = Uuid::new_v4();
        let channel = Channel::Workspace(uuid);
        assert_eq!(format!("{}", channel), channel.to_channel_string());
    }

    // -- ChannelParseError Display tests --

    #[test]
    fn test_channel_parse_error_display() {
        assert_eq!(
            ChannelParseError::InvalidFormat.to_string(),
            "invalid channel format"
        );
        assert_eq!(
            ChannelParseError::UnknownType.to_string(),
            "unknown channel type"
        );
        assert_eq!(
            ChannelParseError::InvalidId.to_string(),
            "invalid channel entity ID"
        );
    }

    #[test]
    fn test_channel_parse_error_is_error() {
        // Verify ChannelParseError implements std::error::Error
        let err: &dyn std::error::Error = &ChannelParseError::InvalidFormat;
        let _ = err.to_string();
    }

    // -- ClientMessage deserialization tests --

    #[test]
    fn test_deserialize_subscribe() {
        let json =
            r#"{"type":"subscribe","channel":"workspace:550e8400-e29b-41d4-a716-446655440000"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Subscribe { channel } => {
                assert_eq!(channel, "workspace:550e8400-e29b-41d4-a716-446655440000");
            }
            _ => panic!("Expected Subscribe variant"),
        }
    }

    #[test]
    fn test_deserialize_unsubscribe() {
        let json =
            r#"{"type":"unsubscribe","channel":"workspace:550e8400-e29b-41d4-a716-446655440000"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Unsubscribe { channel } => {
                assert_eq!(channel, "workspace:550e8400-e29b-41d4-a716-446655440000");
            }
            _ => panic!("Expected Unsubscribe variant"),
        }
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let result = serde_json::from_str::<ClientMessage>("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_unknown_type() {
        let json = r#"{"type":"ping","channel":"workspace:550e8400-e29b-41d4-a716-446655440000"}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }

    // -- ServerMessage serialization tests --

    #[test]
    fn test_serialize_subscribed() {
        let msg = ServerMessage::Subscribed {
            channel: "workspace:550e8400-e29b-41d4-a716-446655440000".to_string(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "subscribed");
        assert_eq!(
            json["channel"],
            "workspace:550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_serialize_unsubscribed() {
        let msg = ServerMessage::Unsubscribed {
            channel: "workspace:550e8400-e29b-41d4-a716-446655440000".to_string(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "unsubscribed");
        assert_eq!(
            json["channel"],
            "workspace:550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_serialize_error() {
        let msg = ServerMessage::Error {
            code: "INVALID_CHANNEL".to_string(),
            message: "Channel not found".to_string(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "error");
        assert_eq!(json["code"], "INVALID_CHANNEL");
        assert_eq!(json["message"], "Channel not found");
    }

    // -- Channel Hash/Eq tests (for DashMap usage) --

    #[test]
    fn test_channel_hash_eq() {
        use std::collections::HashSet;

        let uuid = Uuid::new_v4();
        let mut set = HashSet::new();
        set.insert(Channel::Workspace(uuid));
        set.insert(Channel::Workspace(uuid)); // duplicate
        assert_eq!(set.len(), 1);

        set.insert(Channel::Task(uuid));
        assert_eq!(set.len(), 2);
    }

    // -- Channel::parse no heap allocation for simple variants --

    #[test]
    fn test_parse_simple_variants_no_extra_allocation() {
        // Simple variants contain only a UUID and allocate no payload string.
        let uuid = Uuid::new_v4();
        let ws = Channel::parse(&format!("workspace:{}", uuid)).unwrap();
        assert!(matches!(ws, Channel::Workspace(_)));
    }
}
