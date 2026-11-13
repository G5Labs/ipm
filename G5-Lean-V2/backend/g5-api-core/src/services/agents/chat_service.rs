//! Chat service — chat session lifecycle, message persistence, fork/resume,
//! session summarization, and session search per API_AND_STORAGE.md §3.6.
//!
//! # Overview
//! Chat sessions are the primary mechanism for human-agent interaction in G5.
//! They store the full conversational history shared by typed and templated
//! agent tasks.
//!
//! The service owns the writes to `chat_sessions`, `chat_messages` and
//! `chat_attachments`. Other services decide a delivery, an outcome or a
//! notice and call its transcript operations in their own transaction;
//! SpendService keeps the session usage counters.
use crate::errors::{AppError, G5Error};
use crate::services::{EventBus, PaginatedResult, PaginationParams};
use crate::ws::{Channel, WsEvent};
use base64::Engine;
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::types::{
    AgentAccountSelectionV1, ChatAgentProvider, ChatAttachmentContext, PersistedAgentLaunchV1,
    ResolvedAgentExecutionV1,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::str::FromStr;
use tracing::info;
use uuid::Uuid;
const MAX_CHAT_ATTACHMENT_BYTES: usize = 5 * 1024 * 1024;
/// Shared with upload routes so their pre-deserialization body limits track
/// the maximum encoded attachment accepted by the service.
pub const MAX_CHAT_ATTACHMENT_BASE64_CHARS: usize = MAX_CHAT_ATTACHMENT_BYTES.div_ceil(3) * 4;
const MAX_CHAT_ATTACHMENTS_PER_MESSAGE: usize = 8;
const MAX_CHAT_ATTACHMENT_TOTAL_BYTES_PER_MESSAGE: usize = 10 * 1024 * 1024;
const MAX_CHAT_ATTACHMENT_FILE_NAME_CHARS: usize = 255;
const MAX_CHAT_ATTACHMENT_MATERIALIZED_FILE_NAME_BYTES: usize = 180;
const MAX_CHAT_ATTACHMENT_MATERIALIZED_EXTENSION_BYTES: usize = 24;
const MAX_CHAT_ATTACHMENT_CONTENT_TYPE_CHARS: usize = 255;
/// Content types accepted for agent-produced screenshots. User-origin chat
/// attachments are deliberately unrestricted by MIME/extension (chat_service
/// D20); this allowlist does not gate their upload.
const AGENT_CHAT_ATTACHMENT_IMAGE_TYPES: [&str; 4] =
    ["image/png", "image/jpeg", "image/gif", "image/webp"];
/// PostgreSQL stores `timestamptz` values at microsecond precision. Normalize
/// before binding so an acknowledgement that echoes the bound value is also
/// identical to the value a later database read returns.
fn normalize_postgres_timestamp(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_micros(timestamp.timestamp_micros())
        .expect("current UTC timestamp must be representable at microsecond precision")
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChatDonePublish {
    pub session_id: Uuid,
    pub message_id: Uuid,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub abort_task_id: Option<Uuid>,
    #[sqlx(skip)]
    pub task_status_publish: Option<crate::services::agents::task_service::TaskStatusPublish>,
}
/// An attempt's assistant message in one conversation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AssistantMessage {
    pub id: Uuid,
    pub org_id: Uuid,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub attempt: i32,
}

/// An input the provider accepted, as the transcript records it.
pub(crate) struct AcceptedInput<'a> {
    pub message: AssistantMessage,
    pub input_id: Uuid,
    /// "user", or "system" for a continuation.
    pub role: &'a str,
    pub content: &'a str,
    pub artifacts: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Mark the platform notices an accepted input carries as delivered,
/// returning how many matched.
pub(crate) async fn deliver_reserved_notices(
    conn: &mut sqlx::PgConnection,
    notice_ids: &[Uuid],
    session_id: Uuid,
    task_id: Uuid,
    attempt: i32,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE chat_messages
         SET artifacts = jsonb_set(
                 jsonb_set(artifacts, '{platform_notice,delivery}', '\"delivered\"', false),
                 '{platform_notice,delivered_at}', to_jsonb(now()::text), true)
         WHERE id = ANY($1) AND session_id = $2 AND role = 'system'
           AND artifacts #>> '{platform_notice,delivery}' = 'reserved'
           AND artifacts #>> '{platform_notice,task_id}' = $3::text
           AND (artifacts #>> '{platform_notice,attempt}')::integer = $4",
    )
    .bind(notice_ids)
    .bind(session_id)
    .bind(task_id)
    .bind(attempt)
    .execute(conn)
    .await?
    .rows_affected())
}

/// Record an accepted input in the caller's transaction: its message, the
/// attempt's pending assistant placeholder, the input's attachments and the
/// session's counters and provisional title.
pub(crate) async fn record_accepted_input(
    conn: &mut sqlx::PgConnection,
    input: AcceptedInput<'_>,
) -> Result<(), sqlx::Error> {
    let message = input.message;
    let message_index = sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(MAX(message_index), -1) + 1
         FROM chat_messages WHERE session_id = $1",
    )
    .bind(message.session_id)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, artifacts, citations,
             input_tokens, output_tokens, status, message_index, created_at,
             input_id, task_id, attempt)
         VALUES ($1,$2,$3,$10,$4,$5,'[]'::jsonb,NULL,NULL,'complete',$6,$7,$1,$8,$9)",
    )
    .bind(input.input_id)
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(input.content)
    .bind(&input.artifacts)
    .bind(message_index)
    .bind(input.created_at)
    .bind(message.task_id)
    .bind(message.attempt)
    .bind(input.role)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, artifacts, citations,
             input_tokens, output_tokens, status, message_index, created_at,
             task_id, attempt)
         VALUES ($1,$2,$3,'assistant','',NULL,'[]'::jsonb,
                 NULL,NULL,'pending',$4,now(),$5,$6)",
    )
    .bind(message.id)
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(message_index + 1)
    .bind(message.task_id)
    .bind(message.attempt)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE chat_attachments SET message_id = $1
         WHERE org_id = $2 AND session_id = $3 AND input_id = $1
           AND message_id IS NULL",
    )
    .bind(input.input_id)
    .bind(message.org_id)
    .bind(message.session_id)
    .execute(&mut *conn)
    .await?;
    // A session that still has no name takes the provisional title derived
    // from the first delivered user message (the same rule
    // `POST /sessions/start` applies up front), so sessions created bare and
    // fed through `POST /sessions/:id/messages` are named as well. A title
    // the user cleared (`title_source = 'user'`) stays cleared.
    let provisional_title = (input.role == "user")
        .then(|| derive_session_title(input.content))
        .flatten();
    sqlx::query(
        "UPDATE chat_sessions
         SET message_count = message_count + 2, updated_at = now(),
             title = CASE WHEN title IS NULL AND title_source IS DISTINCT FROM 'user'
                          THEN $3 ELSE title END,
             title_source = CASE WHEN title IS NULL AND title_source IS DISTINCT FROM 'user'
                                 AND $3 IS NOT NULL
                                 THEN 'derived' ELSE title_source END
         WHERE org_id = $1 AND id = $2",
    )
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(provisional_title)
    .execute(conn)
    .await?;
    Ok(())
}

/// Apply a provider-generated summary title while the session's title is
/// still platform-owned. A malformed or empty title is ignored.
pub(crate) async fn apply_generated_title(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    session_id: Uuid,
    raw_title: &str,
) -> Result<(), sqlx::Error> {
    let Some(title) = sanitize_generated_title(raw_title) else {
        return Ok(());
    };
    sqlx::query(&format!(
        "UPDATE chat_sessions
         SET title = $3, title_source = $4
         WHERE org_id = $1 AND id = $2 AND {}",
        PLATFORM_OWNED_TITLE_SQL
    ))
    .bind(org_id)
    .bind(session_id)
    .bind(&title)
    .bind(TITLE_SOURCE_SUMMARY)
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether the session's title is still platform-owned, so a turn should
/// produce a summary title.
pub(crate) async fn session_title_wanted(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    session_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let wanted: Option<bool> = sqlx::query_scalar(&format!(
        "SELECT {} FROM chat_sessions WHERE org_id = $1 AND id = $2",
        PLATFORM_OWNED_TITLE_SQL
    ))
    .bind(org_id)
    .bind(session_id)
    .fetch_optional(conn)
    .await?;
    Ok(wanted.unwrap_or(false))
}

/// Record the provider session a running turn reports.
pub(crate) async fn record_provider_session(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    session_id: Uuid,
    provider_session_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_sessions
         SET provider_session_id = $3, updated_at = now()
         WHERE org_id = $1 AND id = $2",
    )
    .bind(org_id)
    .bind(session_id)
    .bind(provider_session_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Record the provider session state a settled turn reports: its session
/// id, when given, and the newest session-file generation.
pub(crate) async fn record_provider_session_state(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    session_id: Uuid,
    provider_session_id: Option<&str>,
    generation: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_sessions
         SET provider_session_id = COALESCE($3, provider_session_id),
             session_manifest_generation = GREATEST(
                 session_manifest_generation,
                 COALESCE($4, session_manifest_generation)),
             updated_at = now()
         WHERE org_id = $1 AND id = $2",
    )
    .bind(org_id)
    .bind(session_id)
    .bind(provider_session_id)
    .bind(generation)
    .execute(conn)
    .await?;
    Ok(())
}

/// Refresh an assistant message with the content streamed so far; a pending
/// message becomes streaming.
pub(crate) async fn refresh_assistant_message(
    conn: &mut sqlx::PgConnection,
    message: AssistantMessage,
    content: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_messages SET content = $4, is_error_notice = false,
             status = CASE WHEN status = 'pending' THEN 'streaming' ELSE status END
         WHERE id = $1 AND org_id = $2 AND session_id = $3
           AND task_id = $5 AND attempt = $6 AND role = 'assistant'",
    )
    .bind(message.id)
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(content)
    .bind(message.task_id)
    .bind(message.attempt)
    .execute(conn)
    .await?;
    Ok(())
}

/// Mark a pending assistant message streaming.
pub(crate) async fn mark_assistant_message_streaming(
    conn: &mut sqlx::PgConnection,
    message: AssistantMessage,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_messages SET status = 'streaming'
         WHERE id = $1 AND org_id = $2 AND session_id = $3
           AND task_id = $4 AND attempt = $5 AND role = 'assistant'
           AND status = 'pending'",
    )
    .bind(message.id)
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(message.task_id)
    .bind(message.attempt)
    .execute(conn)
    .await?;
    Ok(())
}

/// Set an assistant message's status from the settled attempt.
pub(crate) async fn set_assistant_message_status(
    conn: &mut sqlx::PgConnection,
    message: AssistantMessage,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_messages SET status = $4
         WHERE id = $1 AND org_id = $2 AND session_id = $3
           AND task_id = $5 AND attempt = $6 AND role = 'assistant'",
    )
    .bind(message.id)
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(status)
    .bind(message.task_id)
    .bind(message.attempt)
    .execute(conn)
    .await?;
    Ok(())
}

/// Settle an attempt's assistant message with its final content and status.
/// When the placeholder is missing (an attempt accepted before placeholders
/// were materialized), non-empty content is inserted as the message instead.
/// Returns whether that fallback ran.
pub(crate) async fn settle_final_assistant_message(
    conn: &mut sqlx::PgConnection,
    message: AssistantMessage,
    content: &str,
    status: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE chat_messages
         SET content = $4, status = $7, is_error_notice = false
         WHERE id = $1 AND org_id = $2 AND session_id = $3
           AND task_id = $5 AND attempt = $6 AND role = 'assistant'",
    )
    .bind(message.id)
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(content)
    .bind(message.task_id)
    .bind(message.attempt)
    .bind(status)
    .execute(&mut *conn)
    .await?;
    if updated.rows_affected() == 1 || content.is_empty() {
        return Ok(false);
    }
    // Rolling-upgrade fallback for an attempt accepted before assistant
    // placeholders were materialized at input.accepted.
    let message_index = sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(MAX(message_index), -1) + 1
         FROM chat_messages WHERE session_id = $1",
    )
    .bind(message.session_id)
    .fetch_one(&mut *conn)
    .await?;
    let inserted = sqlx::query(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, artifacts, citations,
             input_tokens, output_tokens, status, message_index, created_at,
             task_id, attempt)
         SELECT $1,$2,$3,'assistant',$4,NULL,'[]'::jsonb,NULL,NULL,$8,$5,now(),$6,$7
         WHERE NOT EXISTS (
             SELECT 1 FROM chat_messages
             WHERE org_id = $2 AND session_id = $3 AND task_id = $6
               AND attempt = $7 AND role = 'assistant')",
    )
    .bind(message.id)
    .bind(message.org_id)
    .bind(message.session_id)
    .bind(content)
    .bind(message_index)
    .bind(message.task_id)
    .bind(message.attempt)
    .bind(status)
    .execute(&mut *conn)
    .await?;
    if inserted.rows_affected() == 1 {
        sqlx::query(
            "UPDATE chat_sessions
             SET message_count = message_count + 1, updated_at = now()
             WHERE org_id = $1 AND id = $2",
        )
        .bind(message.org_id)
        .bind(message.session_id)
        .execute(conn)
        .await?;
    }
    Ok(true)
}

/// A reserved input whose turn ended before the provider accepted it, as
/// TaskService holds it.
pub(crate) struct UnacceptedInput {
    pub input_id: Uuid,
    pub content: String,
    pub artifacts: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Keep a turn whose provider never accepted its input visible: the
/// reserved user inputs as errored messages, their attachments, and the
/// authentication notice as the assistant's errored reply.
pub(crate) async fn record_auth_failure_turn(
    conn: &mut sqlx::PgConnection,
    message: AssistantMessage,
    inputs: &[UnacceptedInput],
    notice: &str,
) -> Result<(), sqlx::Error> {
    let index: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(message_index), -1) + 1 FROM chat_messages WHERE session_id=$1",
    )
    .bind(message.session_id)
    .fetch_one(&mut *conn)
    .await?;
    let mut inserted = 0;
    for input in inputs {
        inserted += sqlx::query(
            "INSERT INTO chat_messages(id,org_id,session_id,role,content,artifacts,citations,status,message_index,created_at,input_id,task_id,attempt)
             VALUES ($1,$2,$3,'user',$4,$5,'[]'::jsonb,'error',$6,$7,$1,$8,$9)
             ON CONFLICT (id) DO NOTHING"
        ).bind(input.input_id).bind(message.org_id).bind(message.session_id)
            .bind(&input.content).bind(&input.artifacts).bind(index).bind(input.created_at)
            .bind(message.task_id).bind(message.attempt)
            .execute(&mut *conn).await?.rows_affected();
    }
    if inserted != 0 {
        sqlx::query(
            "UPDATE chat_attachments SET message_id=input_id
             WHERE input_id IN (SELECT id FROM chat_messages WHERE task_id=$1 AND attempt=$2 AND role='user')
               AND message_id IS NULL"
        ).bind(message.task_id).bind(message.attempt).execute(&mut *conn).await?;
    }
    let notice_inserted = sqlx::query(
        "INSERT INTO chat_messages(id,org_id,session_id,role,content,artifacts,citations,status,message_index,created_at,task_id,attempt,is_error_notice)
         VALUES ($1,$2,$3,'assistant',$4,NULL,'[]'::jsonb,'error',$5,now(),$6,$7,true)
         ON CONFLICT (id) DO NOTHING"
    ).bind(message.id).bind(message.org_id).bind(message.session_id)
        .bind(notice).bind(index + inserted as i32).bind(message.task_id).bind(message.attempt)
        .execute(&mut *conn).await?.rows_affected();
    sqlx::query(
        "UPDATE chat_sessions SET message_count=message_count+$2,updated_at=now() WHERE id=$1",
    )
    .bind(message.session_id)
    .bind((inserted + notice_inserted) as i32)
    .execute(conn)
    .await?;
    Ok(())
}

/// Keep a refusal that needs the owner's correction visible in the chat: one
/// assistant error message bound to no attempt (a turn refused before any
/// attempt existed has none). Idempotent on `id`, so a repeated settlement
/// records it once.
pub(crate) async fn record_refusal_notice(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    session_id: Uuid,
    id: Uuid,
    notice: &str,
) -> Result<(), sqlx::Error> {
    let inserted = sqlx::query(
        "INSERT INTO chat_messages(id,org_id,session_id,role,content,artifacts,citations,status,message_index,created_at,is_error_notice)
         SELECT $1,$2,$3,'assistant',$4,NULL,'[]'::jsonb,'error',
                COALESCE(MAX(message_index), -1) + 1, now(),true
         FROM chat_messages WHERE session_id = $3
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(org_id)
    .bind(session_id)
    .bind(notice)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if inserted == 1 {
        sqlx::query(
            "UPDATE chat_sessions SET message_count = message_count + 1, updated_at = now()
             WHERE org_id = $1 AND id = $2",
        )
        .bind(org_id)
        .bind(session_id)
        .execute(conn)
        .await?;
    }
    Ok(())
}

/// The live chat events an agent turn's boundary journal event produces,
/// published after the batch that stored it commits. Streamed text reaches
/// the browser as live timeline rows before persistence
/// (`g5.chat.live-delivery`), so no chunk is built here.
#[must_use]
pub(crate) fn agent_turn_live_events(
    message: AssistantMessage,
    kind: &str,
    payload: &serde_json::Value,
    task_status: &str,
) -> Vec<(Channel, WsEvent)> {
    let session_id = message.session_id;
    let message_id = message.id;
    let channel = Channel::Chat(session_id);
    let mut events = Vec::new();
    match kind {
        "input.accepted" | "input.shipped" => {
            if let Some(input_id) = payload["input_id"]
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
            {
                for updated in [input_id, message_id] {
                    events.push((
                        channel.clone(),
                        WsEvent::ChatMessageUpdated {
                            session_id,
                            message_id: updated,
                        },
                    ));
                }
            }
        }
        "input.steer_rejected" => {
            events.push((
                channel,
                WsEvent::ChatMessageUpdated {
                    session_id,
                    message_id,
                },
            ));
        }
        "invocation.outcome" => {
            let result = payload.get("result").unwrap_or(&serde_json::Value::Null);
            events.push((
                channel.clone(),
                WsEvent::ChatMessageUpdated {
                    session_id,
                    message_id,
                },
            ));
            if task_status == "succeeded" {
                let usage = result.get("usage").unwrap_or(&serde_json::Value::Null);
                let tokens = |snake: &str, camel: &str| {
                    usage[snake]
                        .as_i64()
                        .or_else(|| usage[camel].as_i64())
                        .unwrap_or(0)
                };
                events.push((
                    channel,
                    WsEvent::ChatDone {
                        session_id,
                        message_id,
                        usage: crate::ws::events::ChatUsagePayload {
                            input_tokens: tokens("input_tokens", "inputTokens"),
                            output_tokens: tokens("output_tokens", "outputTokens"),
                        },
                        citations: Vec::new(),
                    },
                ));
            } else if task_status == "failed" {
                events.push((
                    channel,
                    WsEvent::ChatError {
                        session_id,
                        message_id,
                        error: "agent_error".to_string(),
                        message: result["code"]
                            .as_str()
                            .unwrap_or("agent turn ended without a final message")
                            .to_string(),
                    },
                ));
            }
        }
        _ => {}
    }
    events
}

/// The session's queued platform notices, oldest first, locked for
/// reservation.
pub(crate) async fn lock_queued_notices(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
    limit: i64,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, content
         FROM chat_messages
         WHERE session_id = $1 AND role = 'system'
           AND artifacts #>> '{platform_notice,delivery}' = 'queued'
         ORDER BY message_index, id
         LIMIT $2
         FOR UPDATE",
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// Reserve queued platform notices for one attempt, returning how many
/// matched.
pub(crate) async fn reserve_notices(
    conn: &mut sqlx::PgConnection,
    notice_ids: &[Uuid],
    session_id: Uuid,
    task_id: Uuid,
    attempt: i32,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE chat_messages
         SET artifacts = jsonb_set(
                 jsonb_set(
                     jsonb_set(artifacts, '{platform_notice,delivery}', '\"reserved\"', false),
                     '{platform_notice,task_id}', to_jsonb($2::text), true),
                 '{platform_notice,attempt}', to_jsonb($3::integer), true)
         WHERE id = ANY($1) AND session_id = $4
           AND artifacts #>> '{platform_notice,delivery}' = 'queued'",
    )
    .bind(notice_ids)
    .bind(task_id)
    .bind(attempt)
    .bind(session_id)
    .execute(conn)
    .await?
    .rows_affected())
}

/// The contents of reserved or delivered platform notices, in the order of
/// `notice_ids`.
pub(crate) async fn reserved_notice_contents(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
    notice_ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT message.id, message.content
         FROM unnest($2::uuid[]) WITH ORDINALITY AS selected(id, ordinal)
         JOIN chat_messages AS message ON message.id = selected.id
         WHERE message.session_id = $1 AND message.role = 'system'
           AND message.artifacts #>> '{platform_notice,delivery}' IN ('reserved','delivered')
         ORDER BY selected.ordinal",
    )
    .bind(session_id)
    .bind(notice_ids)
    .fetch_all(conn)
    .await
}

/// The kind of notice telling the next turn about an edit on an ontology
/// changes card (`g5.chat.ontology-card.edit`). It concerns the files, not the
/// VM or the attempt, so it is queued again instead of withdrawn until a turn
/// receives it.
pub(crate) const CARD_EDIT_NOTICE_KIND: &str = "ontology_card_edit";

/// Put a turn's ontology changes card on its reply's assistant message
/// (`chat_messages.artifacts.ontology_card`), keeping the message's other
/// artifacts. `chat_ontology` validates the card.
pub(crate) async fn attach_ontology_card(
    conn: &mut sqlx::PgConnection,
    message_id: Uuid,
    task_id: Uuid,
    card: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_messages
         SET artifacts = CASE WHEN jsonb_typeof(artifacts) = 'object'
                              THEN artifacts || jsonb_build_object('ontology_card', $2::jsonb)
                              ELSE jsonb_build_object('ontology_card', $2::jsonb) END
         WHERE id = $1 AND role = 'assistant' AND task_id = $3
           AND (artifacts IS NULL OR jsonb_typeof(artifacts) = 'object')",
    )
    .bind(message_id)
    .bind(card)
    .bind(task_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Record the owner's edit on a message's ontology changes card
/// (`g5.chat.ontology-card.edit`). `chat_ontology` holds the card's lock and
/// validates the edit.
pub(crate) async fn append_ontology_card_edit(
    conn: &mut sqlx::PgConnection,
    message_id: Uuid,
    edit: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_messages
         SET artifacts = jsonb_set(artifacts, '{ontology_card,edits}',
             COALESCE(artifacts #> '{ontology_card,edits}', '[]'::jsonb) || jsonb_build_array($2::jsonb), true)
         WHERE id = $1",
    )
    .bind(message_id)
    .bind(edit)
    .execute(conn)
    .await?;
    Ok(())
}

/// Return card-edit notices reserved by attempts that ended without
/// delivering them to the queue; `scope` limits them to one attempt, else to
/// one workspace.
async fn requeue_card_edit_notices(
    conn: &mut sqlx::PgConnection,
    attempt: Option<(Uuid, i32)>,
    workspace_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE chat_messages AS message
         SET artifacts = jsonb_set(
                 (message.artifacts #- '{platform_notice,task_id}') #- '{platform_notice,attempt}',
                 '{platform_notice,delivery}', '\"queued\"', false)
         WHERE message.role = 'system'
           AND message.artifacts #>> '{platform_notice,kind}' = $1
           AND message.artifacts #>> '{platform_notice,delivery}' = 'reserved'
           AND ($2::uuid IS NULL OR (message.artifacts #>> '{platform_notice,task_id}' = $2::text
                AND (message.artifacts #>> '{platform_notice,attempt}')::integer = $3))
           AND ($4::uuid IS NULL OR message.session_id IN (
                SELECT id FROM chat_sessions WHERE workspace_id = $4))",
    )
    .bind(CARD_EDIT_NOTICE_KIND)
    .bind(attempt.map(|(task, _)| task))
    .bind(attempt.map(|(_, attempt)| attempt))
    .bind(workspace_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Withdraw the platform notices reserved for one attempt.
pub(crate) async fn withdraw_task_notices(
    conn: &mut sqlx::PgConnection,
    task_id: Uuid,
    attempt: i32,
    reason: &str,
) -> Result<(), sqlx::Error> {
    requeue_card_edit_notices(&mut *conn, Some((task_id, attempt)), None).await?;
    sqlx::query(
        "UPDATE chat_messages
         SET artifacts = jsonb_set(
                 jsonb_set(artifacts,
                     '{platform_notice,delivery}', '\"withdrawn\"', false),
                 '{platform_notice,withdrawn_reason}', to_jsonb($3::text), true)
         WHERE role = 'system'
           AND artifacts #>> '{platform_notice,delivery}' = 'reserved'
           AND artifacts #>> '{platform_notice,task_id}' = $1::text
           AND (artifacts #>> '{platform_notice,attempt}')::integer = $2",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(reason)
    .execute(conn)
    .await?;
    Ok(())
}

/// Withdraw platform notices that can no longer be delivered by the current
/// workspace lifecycle. Delivered notices remain immutable history.
pub(crate) async fn withdraw_workspace_notices(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    requeue_card_edit_notices(&mut *conn, None, Some(workspace_id)).await?;
    sqlx::query(
        "UPDATE chat_messages AS message
         SET artifacts = jsonb_set(
                 jsonb_set(message.artifacts,
                     '{platform_notice,delivery}', '\"withdrawn\"', false),
                 '{platform_notice,withdrawn_reason}', to_jsonb($2::text), true)
         FROM chat_sessions AS session
         WHERE session.workspace_id = $1
           AND message.session_id = session.id
           AND message.role = 'system'
           AND COALESCE(message.artifacts #>> '{platform_notice,kind}', '') <> $3
           AND message.artifacts #>> '{platform_notice,delivery}' IN ('queued','reserved')",
    )
    .bind(workspace_id)
    .bind(reason)
    .bind(CARD_EDIT_NOTICE_KIND)
    .execute(conn)
    .await?;
    Ok(())
}

/// [`withdraw_workspace_notices`] for integration tests of the notices it
/// keeps.
#[cfg(feature = "test-helpers")]
pub async fn withdraw_workspace_notices_for_test(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    withdraw_workspace_notices(conn, workspace_id, reason).await
}

/// Append a lifecycle notice for one stop operation to a session, once.
///
/// Message ordinals share the turn-allocation authority (D53). The stop
/// transaction calling this already holds workspace rows that an enqueue
/// waits on while holding this session's allocation lock, so blocking on the
/// lock here could deadlock; instead the lock is tried, and any remaining
/// collision — an allocator holding the lock, or one that never takes it,
/// such as a draining pre-upgrade pod — resolves through the unique
/// `(session_id, message_index)` index under a savepoint, re-reading the
/// committed ordinal before the bounded retry.
pub(crate) async fn append_lifecycle_notice(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
    content: &str,
    operation_id: Uuid,
    stop_reason: &str,
    deadline_at: &str,
) -> Result<(), sqlx::Error> {
    append_platform_notice(
        conn,
        session_id,
        content,
        operation_id,
        serde_json::json!({"stop_reason": stop_reason, "deadline_at": deadline_at}),
    )
    .await
}

/// Tell the agents working in `workspace_id` about an operation: one notice
/// per chat with queued, running or waiting agent work, for its next turn.
/// Nothing wakes an idle chat.
pub async fn notify_working_agents(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    content: &str,
    operation_id: Uuid,
    details: serde_json::Value,
) -> Result<(), sqlx::Error> {
    let sessions = sqlx::query_scalar::<_, Uuid>(
        "SELECT session.id FROM chat_sessions AS session
          WHERE session.workspace_id = $1
            AND EXISTS (SELECT 1 FROM tasks AS task
                         WHERE task.chat_session_id = session.id AND task.kind = 'agent'
                           AND task.status IN ('queued', 'running', 'waiting'))
          ORDER BY session.id",
    )
    .bind(workspace_id)
    .fetch_all(&mut *conn)
    .await?;
    for session in sessions {
        append_platform_notice(&mut *conn, session, content, operation_id, details.clone()).await?;
    }
    Ok(())
}

/// Queue a notice for a chat's next turn, once per `operation_id`, whether or
/// not the chat has work right now: an edit on an ontology changes card
/// (`g5.chat.ontology-card.edit`) is made between turns.
pub(crate) async fn append_session_notice(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
    content: &str,
    operation_id: Uuid,
    details: serde_json::Value,
) -> Result<(), sqlx::Error> {
    append_platform_notice(conn, session_id, content, operation_id, details).await
}

/// Append a platform notice for one operation to a session, once, for the
/// agent's next turn. `details` joins the notice's artifact beside its
/// delivery state; ordinals are allocated as [`append_lifecycle_notice`]
/// describes, so a caller holding workspace rows cannot deadlock on them.
async fn append_platform_notice(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
    content: &str,
    operation_id: Uuid,
    details: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock($1, $2)")
        .bind(CHAT_TURN_ALLOCATION_LOCK_CLASS)
        .bind(session_turn_allocation_lock_key(session_id))
        .fetch_one(&mut *conn)
        .await?;
    const MAX_ALLOCATION_RETRIES: usize = 5;
    for retry in 0..=MAX_ALLOCATION_RETRIES {
        sqlx::query("SAVEPOINT lifecycle_notice")
            .execute(&mut *conn)
            .await?;
        match insert_platform_notice(&mut *conn, session_id, content, operation_id, &details).await
        {
            Ok(inserted) => {
                sqlx::query("RELEASE SAVEPOINT lifecycle_notice")
                    .execute(&mut *conn)
                    .await?;
                if inserted {
                    sqlx::query(
                        "UPDATE chat_sessions
                         SET message_count = message_count + 1, updated_at = now()
                         WHERE id = $1",
                    )
                    .bind(session_id)
                    .execute(conn)
                    .await?;
                }
                return Ok(());
            }
            Err(error)
                if retry < MAX_ALLOCATION_RETRIES
                    && error.as_database_error().is_some_and(|db_error| {
                        db_error.constraint() == Some("uq_chat_messages_session_message_index")
                            // A deadlock loser and an insert that crossed an
                            // uncommitted session delete (23503) both re-read
                            // committed state; the EXISTS guard then makes the
                            // deleted-session retry a no-op.
                            || matches!(db_error.code().as_deref(), Some("40P01" | "23503"))
                    }) =>
            {
                // The in-flight allocator we lost to has committed (the
                // unique-index wait ends with its transaction); the retry
                // reads its now-visible ordinal.
                sqlx::query("ROLLBACK TO SAVEPOINT lifecycle_notice")
                    .execute(&mut *conn)
                    .await?;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("lifecycle notice allocation loop returns before exhausting retries")
}

/// One allocation attempt: `MAX+1` and the deduplicated insert, `true` when a
/// row was inserted. The session-existence guard keeps a notice for a
/// concurrently deleted session a no-op instead of a NOT NULL violation now
/// that the caller no longer holds the session row lock.
async fn insert_platform_notice(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
    content: &str,
    operation_id: Uuid,
    details: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let message_index: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(message_index), -1) + 1
         FROM chat_messages WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_one(&mut *conn)
    .await?;
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO chat_messages(
             id, org_id, session_id, role, content, artifacts, citations,
             status, message_index)
         SELECT
             gen_random_uuid(),
             (SELECT org_id FROM chat_sessions WHERE id = $1),
             $1, 'system', $2,
             jsonb_build_object('platform_notice', $5::jsonb || jsonb_build_object(
                 'schema_version', 1,
                 'operation_id', $4::text,
                 'delivery', 'queued')),
             '[]'::jsonb, 'complete', $3
         WHERE EXISTS (SELECT 1 FROM chat_sessions WHERE id = $1)
           AND NOT EXISTS (
             SELECT 1 FROM chat_messages
             WHERE session_id = $1 AND role = 'system'
               AND artifacts #>> '{platform_notice,operation_id}' = $4::text)
         RETURNING id",
    )
    .bind(session_id)
    .bind(content)
    .bind(message_index)
    .bind(operation_id)
    .bind(details)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(inserted.is_some())
}

/// Advisory-lock class id namespacing per-session chat turn-row allocation
/// (chat_service D53). Postgres keeps the two-arg
/// `pg_advisory_xact_lock(classid, objid)` lock space entirely separate from
/// the one-arg `bigint` space that the maintenance sweeps use, so this class
/// only has to be unique among two-arg advisory locks — and it is currently
/// the only one. Value is the ASCII of "G5CT" (G5 Chat Turn).
const CHAT_TURN_ALLOCATION_LOCK_CLASS: i32 = 0x4735_4354;
/// Serialize per-session chat turn-row allocation (chat_service D53).
///
/// `send_message`, `dispatch_background_wake`, and task-session retry rebuild
/// take this session-scoped transaction advisory lock BEFORE scanning pending
/// turns, locking the task, or reading `MAX(message_index)`; `stop_generation`
/// takes the same lock before deciding queued-turn handoff. Two uncommitted
/// writers therefore cannot read the same max and interleave duplicate
/// `message_index` values or double-pending assistant turns. The loser blocks
/// until the winner's transaction commits, then observes its committed state.
/// The schema's unique `(session_id, message_index)` index is the database
/// backstop for any missed writer and rejects duplicate durable ordinals with
/// SQLSTATE 23505.
///
/// Transaction-scoped: the lock releases automatically on commit OR rollback,
/// so it MUST be taken on the SAME connection/transaction as the subsequent
/// writes (never a separate pool acquire), following the repo's xact-scoped
/// advisory pattern (`maintenance/preview_reaper`). The `objid` folds the
/// session uuid's first 8 bytes to an i32; a rare cross-session objid collision
/// only over-serializes two unrelated sessions and never breaks correctness.
pub async fn lock_session_turn_allocation(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
) -> Result<(), AppError> {
    let objid = session_turn_allocation_lock_key(session_id);
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(CHAT_TURN_ALLOCATION_LOCK_CLASS)
        .bind(objid)
        .execute(conn)
        .await?;
    Ok(())
}

/// Try to acquire the per-session turn-allocation lock without waiting.
///
/// Dispatch reservation has already locked a candidate task by the time it
/// learns the session identity. Waiting there would invert enqueue's
/// session-then-task order. A failed try lets that short reservation
/// transaction release its task row so enqueue can finish; a later sweep
/// reconsiders the still-queued task.
pub async fn try_lock_session_turn_allocation(
    conn: &mut sqlx::PgConnection,
    session_id: Uuid,
) -> Result<bool, AppError> {
    let acquired = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock($1, $2)")
        .bind(CHAT_TURN_ALLOCATION_LOCK_CLASS)
        .bind(session_turn_allocation_lock_key(session_id))
        .fetch_one(conn)
        .await?;
    Ok(acquired)
}

fn session_turn_allocation_lock_key(session_id: Uuid) -> i32 {
    let b = session_id.as_bytes();
    let folded = i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
    (folded ^ (folded >> 32)) as i32
}
/// Filters for listing sessions.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListFilters {
    /// Optional presentation scope retained for node-oriented history views.
    pub scope_type: Option<String>,
}
/// Request to create a new session.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub scope_type: Option<String>,
    pub scope_ref: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub agent_provider: ChatAgentProvider,
    /// `title` was derived by the platform (see [`derive_session_title`]),
    /// not chosen by the caller: the session stays platform-named and the
    /// agent's summary may replace it. Never set from a request body.
    #[serde(skip)]
    pub derived_title: bool,
    /// Persona the session starts under (`be` | `se` | `ie`; workbench
    /// `g5.workbench.persona`) with its instructions resolved once by the
    /// caller and frozen on the session. Never set from a request body: the
    /// start route resolves them after the request key is claimed.
    #[serde(skip)]
    pub persona: Option<String>,
    #[serde(skip)]
    pub persona_instructions: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAttachmentRequest {
    pub message_id: Uuid,
    pub file_name: String,
    pub content_type: Option<String>,
    pub content_base64: String,
}
/// One inline file attachment carried on a chat send/start request
/// (routes/chat D19, chat_service D20). Unlike `MessageContext.projectReferences`
/// (reference-only, no bytes), this path uploads external bytes and creates a
/// `chat_attachments` row bound to the same turn's user message.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineChatAttachment {
    pub file_name: String,
    #[serde(default)]
    pub content_type: Option<String>,
    pub content_base64: String,
}
#[derive(sqlx::FromRow)]
pub struct ChatAttachmentDTO {
    pub id: Uuid,
    pub session_id: Uuid,
    pub message_id: Option<Uuid>,
    pub file_name: String,
    pub content_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub scan_status: String,
    pub storage_key: String,
    pub provider_mount_path: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
/// Context attached to a message.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageContext {
    pub file_ranges: Option<Vec<FileRange>>,
    pub task_id: Option<String>,
    pub active_file_path: Option<String>,
    pub open_file_paths: Option<Vec<String>>,
    pub project_references: Option<Vec<ProjectReference>>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReference {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub path: Option<String>,
    pub doc_id: Option<String>,
    pub branch_id: Option<String>,
}
/// A file range reference within message context.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRange {
    pub path: String,
    pub start_line: i32,
    pub end_line: i32,
}
/// Request to fork a session.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkSessionRequest {
    pub from_message_index: i32,
    pub title: Option<String>,
    /// `(persona, instructions)` the fork continues under ("follow view",
    /// workbench `g5.workbench.persona`). `None` inherits the source persona
    /// and its frozen instructions without re-injecting them.
    #[serde(skip)]
    pub persona: Option<(String, String)>,
}
/// Response from forking a session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkSessionResponse {
    pub session_id: Uuid,
    pub messages_copied: i32,
}
/// A session in the list view (summary).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItem {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scope_type: String,
    pub scope_ref: Option<String>,
    pub model: Option<String>,
    pub agent_provider: ChatAgentProvider,
    pub provider_state: serde_json::Value,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub message_count: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_usd: f64,
    pub forked_from_id: Option<Uuid>,
    pub linked_commits: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub can_send: bool,
    /// Persona the session started with (workbench `g5.workbench.persona`).
    pub persona: Option<String>,
    /// Turn activity as the listing person sees it (chat `g5.chat.activity`);
    /// `None` for search results, which are ordered by relevance.
    pub activity: Option<super::chat_activity::SessionActivity>,
}
/// Detailed session view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scope_type: String,
    pub scope_ref: Option<String>,
    pub model: Option<String>,
    pub agent_provider: ChatAgentProvider,
    pub provider_state: serde_json::Value,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub message_count: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_usd: f64,
    pub forked_from_id: Option<Uuid>,
    pub forked_at_index: Option<i32>,
    pub linked_commits: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub can_send: bool,
    /// Persona the session started with (workbench `g5.workbench.persona`).
    pub persona: Option<String>,
    /// Turn activity for the reading person; filled by session reads that
    /// name a viewer, `None` on mutation responses.
    pub activity: Option<super::chat_activity::SessionActivity>,
}
/// A terminal login failure belongs to one attempt. This public identity lets
/// transcript readers reconcile the journal notice and persisted fallback
/// without using diagnostic text or exposing the private device challenge.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAuthFailureDTO {
    pub task_id: Uuid,
    pub attempt: i32,
}

/// A single chat message.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageDTO {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: String,
    pub artifacts: Option<serde_json::Value>,
    pub citations: serde_json::Value,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub status: String,
    pub message_index: i32,
    pub created_at: String,
    /// The content is a platform-authored failure explanation, not agent output.
    pub is_error_notice: bool,
    /// The execution attempt that produced this message, when it has one, so
    /// a reader can tell which finished result it displays
    /// (chat `g5.chat.inspection.acknowledge`).
    pub task_id: Option<Uuid>,
    pub attempt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex_auth_failure: Option<CodexAuthFailureDTO>,
}
/// Paginated message list response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageListResponse {
    pub messages: Vec<ChatMessageDTO>,
    pub cursor: Option<i32>,
    pub has_more: bool,
}
/// Summary of a sent message (returned by send_message).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummary {
    pub user_message_id: Uuid,
    pub assistant_message_id: Uuid,
    pub assistant_message_index: i32,
    /// Exact persisted timestamp bound to the pending assistant placeholder INSERT.
    pub assistant_created_at: DateTime<Utc>,
    /// The session this message belongs to.
    pub session_id: Uuid,
    /// The workspace this session belongs to.
    pub workspace_id: Uuid,
    pub agent_provider: ChatAgentProvider,
    pub status: String,
}
/// A single timeline entry scoped to a chat session (returned by get_session_timeline).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTimelineEntry {
    pub id: String,
    pub entry_type: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
}
/// Row returned by session list/detail queries.
#[derive(Debug, sqlx::FromRow)]
struct SessionRow {
    id: Uuid,
    workspace_id: Uuid,
    scope_type: String,
    scope_ref: Option<String>,
    model: Option<String>,
    agent_provider: String,
    provider_state: serde_json::Value,
    title: Option<String>,
    summary: Option<String>,
    tags: Vec<String>,
    message_count: i32,
    total_input_tokens: i64,
    total_output_tokens: i64,
    total_cost_usd: Decimal,
    forked_from_id: Option<Uuid>,
    forked_at_index: Option<i32>,
    persona: Option<String>,
    linked_commits: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}
impl SessionRow {
    fn into_list_item(self) -> SessionListItem {
        SessionListItem {
            id: self.id,
            workspace_id: self.workspace_id,
            scope_type: self.scope_type,
            scope_ref: self.scope_ref,
            model: self.model,
            agent_provider: parse_chat_agent_provider_or_default(&self.agent_provider),
            provider_state: self.provider_state,
            title: self.title,
            summary: self.summary,
            tags: self.tags,
            message_count: self.message_count,
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            total_cost_usd: decimal_to_f64(&self.total_cost_usd),
            forked_from_id: self.forked_from_id,
            linked_commits: self.linked_commits,
            created_at: self.created_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
            can_send: true,
            persona: self.persona,
            activity: None,
        }
    }
    fn into_detail(self) -> SessionDetail {
        SessionDetail {
            id: self.id,
            workspace_id: self.workspace_id,
            scope_type: self.scope_type,
            scope_ref: self.scope_ref,
            model: self.model,
            agent_provider: parse_chat_agent_provider_or_default(&self.agent_provider),
            provider_state: self.provider_state,
            title: self.title,
            summary: self.summary,
            tags: self.tags,
            message_count: self.message_count,
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            total_cost_usd: decimal_to_f64(&self.total_cost_usd),
            forked_from_id: self.forked_from_id,
            forked_at_index: self.forked_at_index,
            linked_commits: self.linked_commits,
            created_at: self.created_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
            can_send: true,
            persona: self.persona,
            activity: None,
        }
    }
}
/// A session row with its activity for one viewer (chat `g5.chat.activity`).
#[derive(Debug, sqlx::FromRow)]
struct SessionActivityRow {
    #[sqlx(flatten)]
    session: SessionRow,
    #[sqlx(flatten)]
    activity: super::chat_activity::ActivityColumns,
}
/// Session columns for an `s`-aliased `chat_sessions` row.
const SESSION_ROW_COLUMNS: &str = "s.id, s.workspace_id, s.scope_type, s.scope_ref, \
     s.model, s.agent_provider, s.provider_state, \
     s.title, s.summary, s.tags, s.message_count, \
     s.total_input_tokens, s.total_output_tokens, s.total_cost_usd, \
     s.forked_from_id, s.forked_at_index, s.persona, \
     s.linked_commits, s.created_at, s.updated_at";
/// Row returned by message queries.
#[derive(Debug, sqlx::FromRow)]
struct MessageRow {
    id: Uuid,
    session_id: Uuid,
    role: String,
    content: String,
    artifacts: Option<serde_json::Value>,
    citations: serde_json::Value,
    input_tokens: Option<i32>,
    output_tokens: Option<i32>,
    status: String,
    message_index: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    is_error_notice: bool,
    task_id: Option<Uuid>,
    attempt: Option<i32>,
}
impl MessageRow {
    fn into_dto(self) -> ChatMessageDTO {
        ChatMessageDTO {
            id: self.id,
            session_id: self.session_id,
            role: self.role,
            content: self.content,
            artifacts: self.artifacts,
            citations: self.citations,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            status: self.status,
            message_index: self.message_index,
            created_at: self.created_at.to_rfc3339(),
            is_error_notice: self.is_error_notice,
            task_id: self.task_id,
            attempt: self.attempt,
            codex_auth_failure: None,
        }
    }
}
/// Only errored rows from an attempt whose login ended carry this marker.
/// Do this once per error-containing page, including tail reads, so replay and
/// read-only views have the same identity even without the timeline loaded.
async fn annotate_codex_auth_failures(
    conn: &mut sqlx::PgConnection,
    messages: &mut [ChatMessageDTO],
) -> Result<(), sqlx::Error> {
    let ids: Vec<Uuid> = messages
        .iter()
        .filter(|message| message.status == "error")
        .map(|message| message.id)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let failures: Vec<(Uuid, Uuid, i32)> = sqlx::query_as(
        "SELECT m.id, m.task_id, m.attempt
         FROM chat_messages m
         JOIN task_attempts a ON a.task_id = m.task_id AND a.attempt = m.attempt
         WHERE m.id = ANY($1)
           AND a.codex_auth_state->>'phase' IN ('ended', 'superseded')",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?;
    for (id, task_id, attempt) in failures {
        if let Some(message) = messages.iter_mut().find(|message| message.id == id) {
            message.codex_auth_failure = Some(CodexAuthFailureDTO { task_id, attempt });
        }
    }
    Ok(())
}

/// Helper: convert rust_decimal::Decimal to f64 for API responses.
fn decimal_to_f64(d: &Decimal) -> f64 {
    use std::str::FromStr;
    f64::from_str(&d.to_string()).unwrap_or(0.0)
}
fn parse_chat_agent_provider_or_default(raw: &str) -> ChatAgentProvider {
    ChatAgentProvider::from_str(raw).unwrap_or_default()
}
fn normalize_attachment_file_name(file_name: &str) -> Result<String, AppError> {
    let trimmed = file_name.trim();
    let basename = trimmed
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .unwrap_or("");
    if basename.is_empty()
        || basename == "."
        || basename == ".."
        || basename.chars().any(|ch| ch.is_control())
        || basename.chars().count() > MAX_CHAT_ATTACHMENT_FILE_NAME_CHARS
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!(
                { "field" : "fileName", "message" :
                "fileName must be non-empty, <= 255 characters, and free of control path data",
                }
            ),
        }));
    }
    Ok(basename.to_string())
}
fn sanitize_attachment_path_segment(input: &str, max_bytes: usize) -> String {
    let mut out = String::new();
    let mut pending_separator = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            if pending_separator && !out.is_empty() {
                if out.len() + 1 > max_bytes {
                    break;
                }
                out.push('-');
            }
            pending_separator = false;
            if out.len() + 1 > max_bytes {
                break;
            }
            out.push(ch);
        } else {
            pending_separator = true;
        }
    }
    out.trim_matches(['.', '_', '-']).to_string()
}
fn materialized_attachment_file_name(file_name: &str) -> String {
    let (stem_input, extension) = file_name
        .rsplit_once('.')
        .and_then(|(stem, extension)| {
            let extension_is_safe = !stem.is_empty()
                && !extension.is_empty()
                && extension.len() <= MAX_CHAT_ATTACHMENT_MATERIALIZED_EXTENSION_BYTES
                && extension.chars().all(|ch| ch.is_ascii_alphanumeric());
            extension_is_safe.then_some((stem, extension))
        })
        .unwrap_or((file_name, ""));
    let extension_budget = if extension.is_empty() {
        0
    } else {
        extension.len() + 1
    };
    let stem_budget = MAX_CHAT_ATTACHMENT_MATERIALIZED_FILE_NAME_BYTES
        .saturating_sub(extension_budget)
        .max("attachment".len());
    let mut stem = sanitize_attachment_path_segment(stem_input, stem_budget);
    if stem.is_empty() {
        stem = "attachment".to_string();
    }
    if extension.is_empty() {
        stem
    } else {
        format!("{stem}.{extension}")
    }
}
fn normalize_attachment_content_type(
    content_type: Option<String>,
) -> Result<Option<String>, AppError> {
    let Some(raw) = content_type else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().any(|ch| ch.is_control())
        || trimmed.chars().count() > MAX_CHAT_ATTACHMENT_CONTENT_TYPE_CHARS
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!(
                { "field" : "contentType", "message" :
                "contentType must be <= 255 characters and free of control data", }
            ),
        }));
    }
    Ok(Some(trimmed.to_string()))
}
/// A validated + decoded chat-attachment payload shared by the standalone
/// upload endpoint and the inline send/start path.
/// Origin discriminator for the shared attachment INSERT (chat_service D63):
/// the storage prefix is the origin marker and `created_by` is NULL exactly
/// for agent-produced rows.
struct AttachmentOrigin {
    created_by: Option<Uuid>,
    storage_prefix: &'static str,
}
struct ValidatedChatAttachment {
    file_name: String,
    content_type: Option<String>,
    content_bytes: Vec<u8>,
}
/// Minimal metadata needed to enforce the user-origin, message-wide attachment
/// contract. Inline uploads derive these values from decoded payloads;
/// standalone uploads combine persisted rows with the decoded candidate.
struct ChatAttachmentSetMember {
    size_bytes: usize,
    sha256: String,
}
/// Normalize and validate an attachment payload: file name, content type, the
/// base64/decoded 5 MiB cap, and — when `require_agent_image` is set for an
/// agent-produced screenshot — the image content-type allowlist. User-origin
/// standalone and inline uploads intentionally apply no MIME/extension gate.
fn validate_chat_attachment_payload(
    file_name: &str,
    content_type: Option<String>,
    content_base64: &str,
    require_agent_image: bool,
) -> Result<ValidatedChatAttachment, AppError> {
    let file_name = normalize_attachment_file_name(file_name)?;
    let content_type = normalize_attachment_content_type(content_type)?;
    if require_agent_image {
        let is_allowed_image = content_type
            .as_deref()
            .map(|value| {
                AGENT_CHAT_ATTACHMENT_IMAGE_TYPES.contains(&value.to_ascii_lowercase().as_str())
            })
            .unwrap_or(false);
        if !is_allowed_image {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "contentType", "message" :
                    format!("agent screenshots must be an image ({})",
                    AGENT_CHAT_ATTACHMENT_IMAGE_TYPES.join(", ")), }
                ),
            }));
        }
    }
    let encoded = content_base64.trim();
    if encoded.len() > MAX_CHAT_ATTACHMENT_BASE64_CHARS {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!(
                { "field" : "contentBase64", "message" :
                format!("contentBase64 exceeds the maximum decoded size of {} bytes",
                MAX_CHAT_ATTACHMENT_BYTES), }
            ),
        }));
    }
    let content_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .map_err(|e| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "contentBase64", "message" :
                    format!("contentBase64 is not valid base64: {e}"), }
                ),
            })
        })?;
    if content_bytes.is_empty() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!(
                { "field" : "contentBase64", "message" :
                "attachment content must not be empty", }
            ),
        }));
    }
    if content_bytes.len() > MAX_CHAT_ATTACHMENT_BYTES {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!(
                { "field" : "contentBase64", "message" :
                format!("decoded content exceeds the maximum size of {} bytes",
                MAX_CHAT_ATTACHMENT_BYTES), }
            ),
        }));
    }
    Ok(ValidatedChatAttachment {
        file_name,
        content_type,
        content_bytes,
    })
}
fn chat_attachment_set_member(attachment: &ValidatedChatAttachment) -> ChatAttachmentSetMember {
    ChatAttachmentSetMember {
        size_bytes: attachment.content_bytes.len(),
        sha256: hex::encode(Sha256::digest(&attachment.content_bytes)),
    }
}
fn attachment_count_error() -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!(
            { "field" : "attachments", "message" :
            format!("a message can include at most {MAX_CHAT_ATTACHMENTS_PER_MESSAGE} attachments"),
            }
        ),
    })
}
/// Validate and decode the complete inline batch before any durable INSERT.
/// The early count check avoids decoding a request that cannot be accepted;
/// the shared message-set validator below remains authoritative for both the
/// inline and standalone persistence paths.
fn validate_inline_chat_attachment_set(
    attachments: Vec<InlineChatAttachment>,
) -> Result<Vec<ValidatedChatAttachment>, AppError> {
    if attachments.len() > MAX_CHAT_ATTACHMENTS_PER_MESSAGE {
        return Err(attachment_count_error());
    }
    let validated = attachments
        .into_iter()
        .map(|attachment| {
            validate_chat_attachment_payload(
                &attachment.file_name,
                attachment.content_type,
                &attachment.content_base64,
                false,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_inline_chat_attachment_set_guards(&validated)?;
    Ok(validated)
}

/// Bind a first-turn idempotency key to the exact validated attachment batch.
/// Empty batches retain the historical message digest for existing callers.
pub fn inline_attachment_request_digest(
    attachments: &[InlineChatAttachment],
) -> Result<Option<String>, AppError> {
    if attachments.is_empty() {
        return Ok(None);
    }
    let validated = validate_inline_chat_attachment_set(attachments.to_vec())?;
    let identity = validated
        .iter()
        .map(|attachment| {
            serde_json::json!({
                "fileName": attachment.file_name,
                "contentType": attachment.content_type,
                "sizeBytes": attachment.content_bytes.len(),
                "sha256": hex::encode(Sha256::digest(&attachment.content_bytes)),
            })
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_vec(&identity).map_err(|error| {
        AppError::from_error(G5Error::Internal {
            message: format!("attachment request digest: {error}"),
        })
    })?;
    Ok(Some(hex::encode(Sha256::digest(encoded))))
}
/// Adapt the decoded inline batch to the path-independent message-set guard.
/// Kept as the inline entry point so existing callers and grounding retain a
/// stable symbol while the authoritative policy is shared below.
fn validate_inline_chat_attachment_set_guards(
    attachments: &[ValidatedChatAttachment],
) -> Result<(), AppError> {
    let members = attachments
        .iter()
        .map(chat_attachment_set_member)
        .collect::<Vec<_>>();
    validate_chat_attachment_set(&members)
}
/// Enforce one message-wide set contract for every user-origin upload path.
/// Callers must provide the complete post-mutation set, never only the new
/// payload or batch.
fn validate_chat_attachment_set(attachments: &[ChatAttachmentSetMember]) -> Result<(), AppError> {
    if attachments.len() > MAX_CHAT_ATTACHMENTS_PER_MESSAGE {
        return Err(attachment_count_error());
    }
    let mut total_bytes = 0usize;
    let mut digests = HashSet::<&str>::with_capacity(attachments.len());
    for attachment in attachments {
        total_bytes = total_bytes
            .checked_add(attachment.size_bytes)
            .ok_or_else(|| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!(
                        { "field" : "attachments", "message" :
                        "combined attachment size is too large", }
                    ),
                })
            })?;
        if total_bytes > MAX_CHAT_ATTACHMENT_TOTAL_BYTES_PER_MESSAGE {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "attachments", "message" :
                    format!("combined attachment size exceeds {MAX_CHAT_ATTACHMENT_TOTAL_BYTES_PER_MESSAGE} bytes"),
                    }
                ),
            }));
        }
        if !digests.insert(attachment.sha256.as_str()) {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "attachments", "message" :
                    "duplicate attachment content is not allowed", }
                ),
            }));
        }
    }
    Ok(())
}
/// Build the attachment context persisted with the queued session input.
/// `download_path` is consumed by the agent turn after the input reservation
/// is accepted.
fn chat_attachment_context_from_dto(attachment: &ChatAttachmentDTO) -> ChatAttachmentContext {
    ChatAttachmentContext {
        id: attachment.id.to_string(),
        message_id: attachment.message_id.map(|id| id.to_string()),
        file_name: attachment.file_name.clone(),
        content_type: attachment.content_type.clone(),
        size_bytes: attachment.size_bytes,
        sha256: attachment.sha256.clone(),
        scan_status: attachment.scan_status.clone(),
        storage_key: attachment.storage_key.clone(),
        provider_mount_path: attachment.provider_mount_path.clone(),
        download_path: format!("/api/internal/chat-attachments/{}/content", attachment.id),
        read_only: true,
    }
}
/// Structured blocked reason — same taxonomy and JSON payload as the
/// start-preflight 409 reasons (`worker_starting` / `missing_capability`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionBlockedReason {
    pub kind: String,
    pub detail: serde_json::Value,
}
impl ChatSessionBlockedReason {
    /// The start-preflight-format conflict reason string (`<kind>: {json}`).
    #[must_use]
    pub fn conflict_reason(&self) -> String {
        format!("{}: {}", self.kind, self.detail)
    }
}
/// LLM model info returned by the chat service.
pub struct ChatModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub supports_streaming: bool,
    pub cost_per_1k_input: Option<f64>,
    pub cost_per_1k_output: Option<f64>,
    pub agent_provider: ChatAgentProvider,
    pub route: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
    pub modes: Vec<String>,
    pub cli_version: String,
}

impl ChatModelInfo {
    fn from_catalog(
        agent: ChatAgentProvider,
        id: &str,
        label: &str,
        route: &str,
        efforts: &[&str],
        default_effort: &str,
    ) -> Self {
        use g5_contracts::{claude_models, codex_models};
        let (modes, version) = match agent {
            ChatAgentProvider::ClaudeCode => (
                claude_models::CLAUDE_CHAT_MODES,
                claude_models::CLAUDE_CODE_CLI_VERSION,
            ),
            ChatAgentProvider::Codex => (
                codex_models::CODEX_CHAT_MODES,
                codex_models::CODEX_CLI_VERSION,
            ),
        };
        Self {
            id: id.into(),
            name: label.into(),
            provider: if route == "openrouter" {
                "openrouter"
            } else if agent == ChatAgentProvider::Codex {
                "openai"
            } else {
                "anthropic"
            }
            .into(),
            // These are route/account dependent and not owned by the catalog.
            context_window: None,
            max_output_tokens: None,
            supports_streaming: true,
            cost_per_1k_input: None,
            cost_per_1k_output: None,
            agent_provider: agent,
            route: route.into(),
            efforts: efforts.iter().map(|value| (*value).into()).collect(),
            default_effort: default_effort.into(),
            modes: modes.iter().map(|value| (*value).into()).collect(),
            cli_version: version.trim().into(),
        }
    }
}
/// Service implementing the chat session lifecycle.
///
/// Holds the event publisher and response policy for chat operations.
#[derive(Clone)]
pub struct ChatService {
    event_bus: EventBus,
    /// How long to wait before timing out a pending assistant message.
    response_timeout: std::time::Duration,
}
impl ChatService {
    /// Construct a new `ChatService`.
    pub fn new(event_bus: EventBus, response_timeout: std::time::Duration) -> Self {
        Self {
            event_bus,
            response_timeout,
        }
    }
    /// Get the configured response timeout.
    #[must_use]
    pub fn response_timeout(&self) -> std::time::Duration {
        self.response_timeout
    }
    pub fn publish_chat_done(&self, publish: &ChatDonePublish) {
        self.event_bus.publish(
            &Channel::Chat(publish.session_id),
            &WsEvent::ChatDone {
                session_id: publish.session_id,
                message_id: publish.message_id,
                usage: crate::ws::events::ChatUsagePayload {
                    input_tokens: publish.input_tokens,
                    output_tokens: publish.output_tokens,
                },
                citations: vec![],
            },
        );
    }
    /// Lists workspace sessions in turn-activity order (chat
    /// `g5.chat.activity.order`), with each chat's activity as `viewer` sees
    /// it. The whole workspace is ordered before the page limit applies.
    pub async fn list_sessions(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        viewer: Uuid,
        filters: SessionListFilters,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<SessionListItem>, AppError> {
        use super::chat_activity::{activity_joins, ACTIVITY_COLUMNS, ACTIVITY_ORDER};
        let limit = pagination.limit();
        let offset = pagination.offset();
        let rows: Vec<SessionActivityRow> = sqlx::query_as::<_, SessionActivityRow>(&format!(
            "SELECT {SESSION_ROW_COLUMNS}, {ACTIVITY_COLUMNS}
             FROM chat_sessions AS s
             {joins}
             WHERE s.workspace_id = $1
               AND ($2::text IS NULL OR s.scope_type = $2)
             ORDER BY {ACTIVITY_ORDER}
             LIMIT $3 OFFSET $4",
            joins = activity_joins("$5"),
        ))
        .bind(workspace_id)
        .bind(filters.scope_type.as_deref())
        .bind(limit)
        .bind(offset)
        .bind(viewer)
        .fetch_all(db.conn())
        .await?;
        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM chat_sessions
            WHERE workspace_id = $1
              AND ($2::text IS NULL OR scope_type = $2)
            "#,
        )
        .bind(workspace_id)
        .bind(filters.scope_type.as_deref())
        .fetch_one(db.conn())
        .await?;
        let mut items: Vec<_> = rows
            .into_iter()
            .map(|row| SessionListItem {
                activity: Some(row.activity.into_activity()),
                ..row.session.into_list_item()
            })
            .collect();
        self.enrich_session_list_items(db, workspace_id, &mut items)
            .await?;
        Ok(PaginatedResult::new(items, total, pagination))
    }
    /// Creates a new chat session with the specified scope.
    pub async fn create_session(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        created_by: Uuid,
        req: CreateSessionRequest,
    ) -> Result<SessionDetail, AppError> {
        let session_id = Uuid::new_v4();
        let now = normalize_postgres_timestamp(Utc::now());
        let scope_type = req.scope_type.unwrap_or_else(|| "workspace".to_string());
        if scope_type == "task" {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "scopeType": "task-scoped sessions were replaced by tasks linked to workspace sessions"
                }),
            }));
        }
        let empty_tags: Vec<String> = vec![];
        let empty_commits: Vec<String> = vec![];
        let normalized_title = Self::normalize_session_title(req.title.as_deref());
        if req.persona.is_some() != req.persona_instructions.is_some() {
            return Err(AppError::from_error(G5Error::Internal {
                message: "a persona session needs its frozen instructions".to_string(),
            }));
        }
        sqlx::query(
            r#"
            INSERT INTO chat_sessions (
                id, org_id, workspace_id, created_by, scope_type, scope_ref,
                model, agent_provider, title, title_source, summary, tags,
                linked_commits,
                total_input_tokens, total_output_tokens, total_cost_usd,
                message_count, forked_from_id, forked_at_index,
                created_at, updated_at, persona, persona_instructions
            ) VALUES (
                $1, (SELECT org_id FROM workspaces WHERE id = $2), $2, $3, $4, $5,
                $6, $7, $8, $12, NULL, $9,
                $10,
                0, 0, 0,
                0, NULL, NULL,
                $11, $11, $13, $14
            )
            "#,
        )
        .bind(session_id)
        .bind(workspace_id)
        .bind(created_by)
        .bind(&scope_type)
        .bind(req.scope_ref.as_deref())
        .bind(req.model.as_deref())
        .bind(req.agent_provider.as_str())
        .bind(normalized_title.as_deref())
        .bind(&empty_tags)
        .bind(&empty_commits)
        .bind(now)
        .bind(if normalized_title.is_none() || req.derived_title {
            TITLE_SOURCE_DERIVED
        } else {
            TITLE_SOURCE_USER
        })
        .bind(req.persona.as_deref())
        .bind(req.persona_instructions.as_deref())
        .execute(db.conn())
        .await?;
        info!(
            session_id = %session_id,
            workspace_id = %workspace_id,
            "Created workspace agent session"
        );
        self.get_session(db, session_id).await
    }
    /// Updates the user-editable session title.
    ///
    /// Blank or whitespace-only values clear the title so the UI can fall back
    /// to its presentation default. Non-empty values are trimmed before
    /// persistence.
    pub async fn update_session_title(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        title: Option<String>,
    ) -> Result<SessionDetail, AppError> {
        let normalized_title = Self::normalize_session_title(title.as_deref());
        let result = sqlx::query(
            r#"
            UPDATE chat_sessions
            SET title = $2,
                title_source = $4,
                updated_at = $3
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .bind(normalized_title.as_deref())
        .bind(Utc::now())
        .bind(TITLE_SOURCE_USER)
        .execute(db.conn())
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::from_error(G5Error::NotFound {
                resource: format!("chat session {}", session_id),
            }));
        }
        self.get_session(db, session_id).await
    }
    /// Fetches a single session with all metadata.
    pub async fn get_session(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
    ) -> Result<SessionDetail, AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT id, workspace_id, scope_type, scope_ref,
                   model, agent_provider, provider_state,
                   title, summary, tags, message_count,
                   total_input_tokens, total_output_tokens, total_cost_usd,
                   forked_from_id, forked_at_index, persona,
                   linked_commits,
                   created_at, updated_at
            FROM chat_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("chat session {}", session_id),
            })
        })?;
        Ok(row.into_detail())
    }
    pub async fn create_attachment(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        org_id: Uuid,
        user_id: Uuid,
        req: CreateAttachmentRequest,
    ) -> Result<ChatAttachmentDTO, AppError> {
        lock_session_turn_allocation(db.conn(), session_id).await?;
        #[derive(sqlx::FromRow)]
        struct AttachmentMessageRow {
            id: Uuid,
            role: String,
        }
        let message = sqlx::query_as::<_, AttachmentMessageRow>(
            r#"
            SELECT id, role
            FROM chat_messages
            WHERE session_id = $1
              AND id = $2
            "#,
        )
        .bind(session_id)
        .bind(req.message_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "messageId", "message" :
                    "messageId must reference a message in this session", }
                ),
            })
        })?;
        if message.role != "user" {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "messageId", "message" :
                    "chat attachments must be scoped to a user message", }
                ),
            }));
        }
        let validated = validate_chat_attachment_payload(
            &req.file_name,
            req.content_type,
            &req.content_base64,
            false,
        )?;
        #[derive(sqlx::FromRow)]
        struct PersistedAttachmentSetMember {
            size_bytes: i64,
            sha256: String,
        }
        let persisted = sqlx::query_as::<_, PersistedAttachmentSetMember>(
            r#"
            SELECT size_bytes, sha256
            FROM chat_attachments
            WHERE session_id = $1
              AND message_id = $2
              AND storage_key NOT LIKE 'agent-screenshots/%'
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .bind(session_id)
        .bind(message.id)
        .fetch_all(db.conn())
        .await?;
        let mut complete_set = persisted
            .into_iter()
            .map(|row| {
                let size_bytes = usize::try_from(row.size_bytes).map_err(|_| {
                    AppError::from(G5Error::Internal {
                        message: format!(
                            "chat attachment for message {} has invalid size_bytes {}",
                            message.id, row.size_bytes
                        ),
                    })
                })?;
                Ok(ChatAttachmentSetMember {
                    size_bytes,
                    sha256: row.sha256,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        complete_set.push(chat_attachment_set_member(&validated));
        validate_chat_attachment_set(&complete_set)?;
        let attachment = self
            .insert_chat_attachment_row(db, session_id, org_id, user_id, message.id, validated)
            .await?;
        Ok(attachment)
    }
    /// Persist one validated attachment row for `message_id`, reusing the exact
    /// INSERT (and storage-key / provider-mount-path derivation) shared by the
    /// standalone upload endpoint and the inline send/start path. Keeping a
    /// single `sqlx::query_as!`-free INSERT means no new prepared-query cache
    /// entry is required for `SQLX_OFFLINE` builds.
    async fn insert_chat_attachment_row(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        org_id: Uuid,
        user_id: Uuid,
        message_id: Uuid,
        validated: ValidatedChatAttachment,
    ) -> Result<ChatAttachmentDTO, AppError> {
        self.insert_attachment_row_with_origin(
            db,
            session_id,
            org_id,
            message_id,
            AttachmentOrigin {
                created_by: Some(user_id),
                storage_prefix: "chat-attachments",
            },
            validated,
        )
        .await
    }
    /// Shared INSERT + key derivation for user- and agent-origin attachments
    /// (chat_service D63): the two origins differ ONLY in storage prefix and
    /// `created_by`, and a schema change to `chat_attachments` must not have
    /// two copies to update in lockstep.
    async fn insert_attachment_row_with_origin(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        org_id: Uuid,
        message_id: Uuid,
        origin: AttachmentOrigin,
        validated: ValidatedChatAttachment,
    ) -> Result<ChatAttachmentDTO, AppError> {
        let AttachmentOrigin {
            created_by,
            storage_prefix,
        } = origin;
        let ValidatedChatAttachment {
            file_name,
            content_type,
            content_bytes,
        } = validated;
        let digest = format!("{:x}", Sha256::digest(&content_bytes));
        let storage_key = format!(
            "{}/{}/{}/{}/{}",
            storage_prefix, session_id, message_id, digest, file_name
        );
        let materialized_file_name = materialized_attachment_file_name(&file_name);
        let provider_mount_path = format!(
            "/mnt/g5/{}/{}/{}/{}/{}",
            storage_prefix, session_id, message_id, digest, materialized_file_name
        );
        let attachment = sqlx::query_as::<_, ChatAttachmentDTO>(
            r#"
            INSERT INTO chat_attachments (
                org_id,
                session_id,
                message_id,
                created_by,
                file_name,
                content_type,
                size_bytes,
                sha256,
                storage_key,
                provider_mount_path,
                scan_status,
                content_bytes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', $11)
            ON CONFLICT (storage_key)
            DO UPDATE SET provider_mount_path = EXCLUDED.provider_mount_path
            RETURNING id, session_id, message_id, file_name, content_type,
                      size_bytes, sha256, scan_status, storage_key,
                      provider_mount_path, created_at
            "#,
        )
        .bind(org_id)
        .bind(session_id)
        .bind(message_id)
        .bind(created_by)
        .bind(&file_name)
        .bind(content_type.as_deref())
        .bind(content_bytes.len() as i64)
        .bind(&digest)
        .bind(&storage_key)
        .bind(&provider_mount_path)
        .bind(&content_bytes)
        .fetch_one(db.conn())
        .await
        .map_err(g5_contracts::error::G5Error::from)
        .map_err(AppError::from)?;
        Ok(attachment)
    }
    /// Validate and persist one agent-produced screenshot (chat_service D63):
    /// the public entry the internal worker upload route calls. Validation runs
    /// the shared payload validator with `require_image = true` — agent
    /// evidence is always an inline-image type.
    pub async fn create_agent_attachment(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        message_id: Uuid,
        file_name: &str,
        content_type: Option<String>,
        content_base64: &str,
    ) -> Result<ChatAttachmentDTO, AppError> {
        let validated =
            validate_chat_attachment_payload(file_name, content_type, content_base64, true)?;
        let org_id: Uuid = sqlx::query_scalar("SELECT org_id FROM chat_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(db.conn())
            .await
            .map_err(g5_contracts::error::G5Error::from)
            .map_err(AppError::from)?;
        self.insert_agent_chat_attachment_row(db, session_id, org_id, message_id, validated)
            .await
    }
    /// Agent-origin attachment insert (chat_service D63): `created_by` is NULL
    /// (no user authored the bytes), the owning message must be the session's
    /// ASSISTANT message (agent evidence binds to the agent's own turn; the
    /// user-upload `role = 'user'` invariant is untouched), and the storage key
    /// carries the reserved `agent-screenshots/` prefix — the origin marker the
    /// re-feed exclusions key on, chosen over a schema column so no migration
    /// rides this feature.
    async fn insert_agent_chat_attachment_row(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        org_id: Uuid,
        message_id: Uuid,
        validated: ValidatedChatAttachment,
    ) -> Result<ChatAttachmentDTO, AppError> {
        #[derive(sqlx::FromRow)]
        struct AgentAttachmentMessageRow {
            id: Uuid,
            role: String,
        }
        let message = sqlx::query_as::<_, AgentAttachmentMessageRow>(
            r#"
            SELECT id, role
            FROM chat_messages
            WHERE session_id = $1
              AND id = $2
            "#,
        )
        .bind(session_id)
        .bind(message_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "messageId", "message" :
                    "messageId must reference a message in this session", }
                ),
            })
        })?;
        if message.role != "assistant" {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "field" : "messageId", "message" :
                    "agent attachments must be scoped to an assistant message", }
                ),
            }));
        }
        self.insert_attachment_row_with_origin(
            db,
            session_id,
            org_id,
            message.id,
            AttachmentOrigin {
                created_by: None,
                storage_prefix: "agent-screenshots",
            },
            validated,
        )
        .await
    }
    /// Session-fenced attachment byte read (chat_service D63): the row must
    /// belong to the scoped session; serves user-origin and agent-origin rows
    /// alike for direct browser rendering (routes/chat D30).
    pub async fn get_attachment_content(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        attachment_id: Uuid,
    ) -> Result<(String, Option<String>, Vec<u8>), AppError> {
        #[derive(sqlx::FromRow)]
        struct AttachmentContentRow {
            file_name: String,
            content_type: Option<String>,
            content_bytes: Vec<u8>,
        }
        let row = sqlx::query_as::<_, AttachmentContentRow>(
            r#"
            SELECT file_name, content_type, content_bytes
            FROM chat_attachments
            WHERE id = $1
              AND session_id = $2
            "#,
        )
        .bind(attachment_id)
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("chat attachment {attachment_id}"),
            })
        })?;
        Ok((row.file_name, row.content_type, row.content_bytes))
    }
    /// Persist attachments for a queued inbox row. They remain linked to the
    /// input authority with no transcript `message_id` until the matching
    /// journaled `input.accepted` event materializes that message.
    pub async fn persist_inline_session_input_attachments(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        org_id: Uuid,
        user_id: Uuid,
        input_id: Uuid,
        attachments: Vec<InlineChatAttachment>,
    ) -> Result<Vec<ChatAttachmentContext>, AppError> {
        let validated_attachments = validate_inline_chat_attachment_set(attachments)?;
        let mut contexts = Vec::with_capacity(validated_attachments.len());
        for validated in validated_attachments {
            let ValidatedChatAttachment {
                file_name,
                content_type,
                content_bytes,
            } = validated;
            let digest = format!("{:x}", Sha256::digest(&content_bytes));
            let storage_key =
                format!("chat-attachments/{session_id}/{input_id}/{digest}/{file_name}");
            let materialized_file_name = materialized_attachment_file_name(&file_name);
            let provider_mount_path = format!(
                "/mnt/g5/chat-attachments/{session_id}/{input_id}/{digest}/{materialized_file_name}"
            );
            let row = sqlx::query_as::<_, ChatAttachmentDTO>(
                "INSERT INTO chat_attachments(
                     org_id, session_id, message_id, input_id, created_by,
                     file_name, content_type, size_bytes, sha256, storage_key,
                     provider_mount_path, scan_status, content_bytes)
                 VALUES ($1,$2,NULL,$3,$4,$5,$6,$7,$8,$9,$10,'pending',$11)
                 ON CONFLICT (storage_key)
                 DO UPDATE SET provider_mount_path = EXCLUDED.provider_mount_path
                 RETURNING id, session_id, message_id, file_name, content_type,
                           size_bytes, sha256, scan_status, storage_key,
                           provider_mount_path, created_at",
            )
            .bind(org_id)
            .bind(session_id)
            .bind(input_id)
            .bind(user_id)
            .bind(&file_name)
            .bind(content_type.as_deref())
            .bind(content_bytes.len() as i64)
            .bind(&digest)
            .bind(&storage_key)
            .bind(&provider_mount_path)
            .bind(&content_bytes)
            .fetch_one(db.conn())
            .await?;
            contexts.push(chat_attachment_context_from_dto(&row));
        }
        let artifacts = serde_json::to_value(&contexts)?;
        let updated = crate::services::agents::agent_dispatch::set_queued_input_artifacts(
            db.conn(),
            org_id,
            session_id,
            input_id,
            artifacts,
        )
        .await?;
        if !updated {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: "queued input changed before attachments were persisted".to_string(),
            }));
        }
        Ok(contexts)
    }
    /// Fetches paginated messages for a session, ordered by `message_index`.
    /// Uses cursor-based pagination where the cursor is the `message_index`
    /// of the last message in the previous page. When `tail` is true and no
    /// cursor is supplied, returns the LAST `limit` rows in the same ascending
    /// order (latest-window read, chat_service D2 / routes/chat I6).
    pub async fn get_messages(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        cursor: Option<i32>,
        limit: i32,
        tail: bool,
    ) -> Result<MessageListResponse, AppError> {
        let _session =
            sqlx::query_scalar!("SELECT id FROM chat_sessions WHERE id = $1", session_id,)
                .fetch_optional(db.conn())
                .await?
                .ok_or_else(|| {
                    AppError::from_error(G5Error::NotFound {
                        resource: format!("chat session {}", session_id),
                    })
                })?;
        if tail && cursor.is_none() {
            let mut rows = sqlx::query_as::<_, MessageRow>(
                r#"
                SELECT id, session_id, role, content, artifacts, citations,
                       input_tokens, output_tokens, status, message_index, created_at, is_error_notice,
                       task_id, attempt
                FROM chat_messages
                WHERE session_id = $1
                ORDER BY message_index DESC
                LIMIT $2
                "#,
            )
            .bind(session_id)
            .bind((limit + 1) as i64)
            .fetch_all(db.conn())
            .await?;
            let has_more = rows.len() as i32 > limit;
            if has_more {
                rows.truncate(limit as usize);
            }
            rows.reverse();
            let next_cursor = rows.last().map(|r| r.message_index);
            let mut messages: Vec<ChatMessageDTO> =
                rows.into_iter().map(|r| r.into_dto()).collect();
            annotate_codex_auth_failures(db.conn(), &mut messages).await?;
            return Ok(MessageListResponse {
                messages,
                cursor: next_cursor,
                has_more,
            });
        }
        let rows = sqlx::query_as!(
            MessageRow,
            r#"
            SELECT id, session_id, role, content, artifacts, citations,
                   input_tokens, output_tokens, status, message_index, created_at, is_error_notice,
                   task_id, attempt
            FROM chat_messages
            WHERE session_id = $1
              AND ($2::int IS NULL OR message_index > $2)
            ORDER BY message_index ASC
            LIMIT $3
            "#,
            session_id,
            cursor,
            (limit + 1) as i64,
        )
        .fetch_all(db.conn())
        .await?;
        let has_more = rows.len() as i32 > limit;
        let rows: Vec<MessageRow> = if has_more {
            rows.into_iter().take(limit as usize).collect()
        } else {
            rows
        };
        let next_cursor = rows.last().map(|r| r.message_index);
        let mut messages: Vec<ChatMessageDTO> = rows.into_iter().map(|r| r.into_dto()).collect();
        annotate_codex_auth_failures(db.conn(), &mut messages).await?;
        Ok(MessageListResponse {
            messages,
            cursor: next_cursor,
            has_more,
        })
    }
    /// Creates a new session with messages copied from the source session
    /// up to `from_message_index` (inclusive).
    /// Where a "follow view" fork cuts the source transcript (workbench
    /// `g5.workbench.persona`): the requested index, clamped to the last
    /// completed assistant unit while a turn is active (an assistant row still
    /// pending/streaming, or an input queued/reserved). `Ok(None)` means the
    /// source has no delivered transcript yet — empty or queued-only — so the
    /// caller starts a new session instead of forking.
    pub async fn persona_fork_boundary(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        requested_index: i32,
    ) -> Result<Option<i32>, AppError> {
        let last_index: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(message_index) FROM chat_messages WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_one(db.conn())
        .await?;
        let Some(last_index) = last_index else {
            return Ok(None);
        };
        let turn_active: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM chat_messages
                 WHERE session_id = $1 AND role = 'assistant'
                   AND status IN ('pending', 'streaming'))
             OR EXISTS (
                 SELECT 1 FROM session_inputs
                 WHERE session_id = $1
                   AND delivery IN ('queued', 'reserved', 'delivery_unknown'))",
        )
        .bind(session_id)
        .fetch_one(db.conn())
        .await?;
        let mut boundary = requested_index.clamp(0, last_index);
        if turn_active {
            let last_completed: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(message_index) FROM chat_messages
                 WHERE session_id = $1 AND role = 'assistant' AND status = 'complete'",
            )
            .bind(session_id)
            .fetch_one(db.conn())
            .await?;
            let Some(last_completed) = last_completed else {
                return Ok(None);
            };
            boundary = boundary.min(last_completed);
        }
        Ok(Some(boundary))
    }

    pub async fn fork_session(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        req: ForkSessionRequest,
    ) -> Result<ForkSessionResponse, AppError> {
        let source = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT id, workspace_id, scope_type, scope_ref,
                   model, agent_provider, provider_state,
                   title, summary, tags, message_count,
                   total_input_tokens, total_output_tokens, total_cost_usd,
                   forked_from_id, forked_at_index, persona,
                   linked_commits,
                   created_at, updated_at
            FROM chat_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("chat session {}", session_id),
            })
        })?;
        if req.from_message_index < 0 {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "from_message_index" : "Must be non-negative" }
                ),
            }));
        }
        if req.from_message_index >= source.message_count {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!(
                    { "from_message_index" :
                    format!("Exceeds session message count ({})", source
                    .message_count) }
                ),
            }));
        }
        let new_session_id = Uuid::new_v4();
        let now = Utc::now();
        let fork_title = req
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| {
                format!("Fork of {}", source.title.as_deref().unwrap_or("Untitled"))
            });
        let empty_commits: Vec<String> = vec![];
        let source_created_by: Uuid =
            sqlx::query_scalar("SELECT created_by FROM chat_sessions WHERE id = $1")
                .bind(session_id)
                .fetch_one(db.conn())
                .await?;
        // Copy composer defaults, never the native provider's continuation.
        // Old workflow chats may only have their mode in the first attempt.
        let initial_modes =
            super::attempt_snapshot::initial_session_permission_modes(db.conn(), &[session_id])
                .await?;
        let mut provider_state = serde_json::json!({});
        let source_provider = parse_chat_agent_provider_or_default(&source.agent_provider);
        let namespace = super::workspace_messages::provider_state_namespace(source_provider);
        if let Some(settings) = source
            .provider_state
            .get(namespace)
            .and_then(|v| v.get("settings"))
        {
            provider_state[namespace] = serde_json::json!({ "settings": settings });
        }
        apply_session_permission_mode(
            &mut provider_state,
            source_provider,
            initial_modes.get(&session_id).map(String::as_str),
        );
        // Persona (workbench g5.workbench.persona): a "follow view" fork
        // records the new persona with its instructions in the same INSERT
        // (no persona-labelled fork exists without them); an ordinary fork
        // inherits the source's persona and frozen instructions as they are.
        let (persona, persona_instructions) = match &req.persona {
            Some((persona, instructions)) => {
                if g5_api_foundation::contracts::workbench::WorkbenchPhase::parse(persona).is_none()
                {
                    return Err(AppError::from_error(G5Error::ValidationError {
                        details: serde_json::json!({ "persona": "must be one of be, se, ie" }),
                    }));
                }
                (Some(persona.as_str()), Some(instructions.as_str()))
            }
            None => (None, None),
        };
        sqlx::query(
            r#"
            INSERT INTO chat_sessions (
                id, org_id, workspace_id, created_by, scope_type, scope_ref,
                model, agent_provider, title, summary, tags,
                linked_commits,
                total_input_tokens, total_output_tokens, total_cost_usd,
                message_count, forked_from_id, forked_at_index,
                created_at, updated_at, persona, persona_instructions, provider_state
            ) VALUES (
                $1, (SELECT org_id FROM workspaces WHERE id = $2), $2, $3, $4, $5,
                $6, $7, $8, NULL, $9,
                $10,
                0, 0, 0,
                0, $11, $12,
                $13, $13,
                COALESCE($14, (SELECT persona FROM chat_sessions WHERE id = $11)),
                COALESCE($15, (SELECT persona_instructions FROM chat_sessions WHERE id = $11)), $16
            )
            "#,
        )
        .bind(new_session_id)
        .bind(source.workspace_id)
        .bind(source_created_by)
        .bind(&source.scope_type)
        .bind(source.scope_ref.as_deref())
        .bind(source.model.as_deref())
        .bind(source.agent_provider.as_str())
        .bind(&fork_title)
        .bind(&source.tags)
        .bind(&empty_commits)
        .bind(session_id)
        .bind(req.from_message_index)
        .bind(now)
        .bind(persona)
        .bind(persona_instructions)
        .bind(provider_state)
        .execute(db.conn())
        .await?;
        let copied_messages = sqlx::query_as!(
            MessageRow,
            r#"
            SELECT id, session_id, role, content, artifacts, citations,
                   input_tokens, output_tokens, status, message_index, created_at, is_error_notice,
                   task_id, attempt
            FROM chat_messages
            WHERE session_id = $1 AND message_index <= $2
            ORDER BY message_index ASC
            "#,
            session_id,
            req.from_message_index,
        )
        .fetch_all(db.conn())
        .await?;
        let messages_copied = copied_messages.len() as i32;
        let mut total_input_tokens: i64 = 0;
        let mut total_output_tokens: i64 = 0;
        let mut message_id_pairs = Vec::with_capacity(copied_messages.len());
        for (new_index, msg) in copied_messages.iter().enumerate() {
            let new_msg_id = Uuid::new_v4();
            let forked_status = match msg.status.as_str() {
                "pending" | "streaming" if msg.role == "assistant" => "stopped",
                _ => msg.status.as_str(),
            };
            sqlx::query!(
                r#"
                INSERT INTO chat_messages (
                    id, org_id, session_id, role, content, artifacts, citations,
                    input_tokens, output_tokens, status, message_index, created_at, is_error_notice
                ) VALUES (
                    $1, (SELECT org_id FROM chat_sessions WHERE id = $2), $2, $3, $4, $5, $6,
                    $7, $8, $9, $10, $11, $12
                )
                "#,
                new_msg_id,
                new_session_id,
                &msg.role,
                &msg.content,
                msg.artifacts,
                &msg.citations,
                msg.input_tokens,
                msg.output_tokens,
                forked_status,
                new_index as i32,
                msg.created_at,
                msg.is_error_notice,
            )
            .execute(db.conn())
            .await?;
            total_input_tokens += msg.input_tokens.unwrap_or(0) as i64;
            total_output_tokens += msg.output_tokens.unwrap_or(0) as i64;
            message_id_pairs.push((msg.id, new_msg_id));
        }
        Self::copy_session_attachments(db.conn(), session_id, new_session_id, &message_id_pairs)
            .await?;
        sqlx::query!(
            r#"
            UPDATE chat_sessions
            SET message_count = $2,
                total_input_tokens = $3,
                total_output_tokens = $4
            WHERE id = $1
            "#,
            new_session_id,
            messages_copied,
            total_input_tokens,
            total_output_tokens,
        )
        .execute(db.conn())
        .await?;
        info!(
            session_id = % new_session_id, forked_from = % session_id, messages_copied =
            % messages_copied, "Forked session"
        );
        Ok(ForkSessionResponse {
            session_id: new_session_id,
            messages_copied,
        })
    }
    /// Copies every `chat_messages` row of `source_session_id` into
    /// `new_session_id` with dense `message_index` from 0, normalizing
    /// pending/streaming assistant rows to a terminal `stopped` state (the
    /// copy has no live worker to finish them), then rolls the copied
    /// counters (`message_count`, token totals, `forked_at_index`,
    /// `updated_at`) onto the new session row.
    ///
    /// Used by [`Self::copy_sessions_to_workspace`] so status normalization
    /// and counter math cannot drift between workspace-fork copies. Returns
    /// the `(source_message_id, new_message_id)` pairs in
    /// `message_index` order; callers use the length as the copied count and
    /// the carry path uses the id mapping to re-bind attachments.
    async fn copy_session_messages_with_counters(
        conn: &mut sqlx::PgConnection,
        source_session_id: Uuid,
        new_session_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Vec<(Uuid, Uuid)>, AppError> {
        let copied_messages = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT id, session_id, role, content, artifacts, citations,
                   input_tokens, output_tokens, status, message_index, created_at, is_error_notice,
                   task_id, attempt
            FROM chat_messages
            WHERE session_id = $1
            ORDER BY message_index ASC
            "#,
        )
        .bind(source_session_id)
        .fetch_all(&mut *conn)
        .await?;
        let mut total_input_tokens: i64 = 0;
        let mut total_output_tokens: i64 = 0;
        let mut id_pairs: Vec<(Uuid, Uuid)> = Vec::with_capacity(copied_messages.len());
        for (new_index, msg) in copied_messages.iter().enumerate() {
            let copied_status = match msg.status.as_str() {
                "pending" | "streaming" if msg.role == "assistant" => "stopped",
                _ => msg.status.as_str(),
            };
            let new_message_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO chat_messages (
                    id, org_id, session_id, role, content, artifacts, citations,
                    input_tokens, output_tokens, status, message_index, created_at, is_error_notice
                ) VALUES (
                    $1, (SELECT org_id FROM chat_sessions WHERE id = $2), $2, $3, $4, $5, $6,
                    $7, $8, $9, $10, $11, $12
                )
                "#,
            )
            .bind(new_message_id)
            .bind(new_session_id)
            .bind(&msg.role)
            .bind(&msg.content)
            .bind(&msg.artifacts)
            .bind(&msg.citations)
            .bind(msg.input_tokens)
            .bind(msg.output_tokens)
            .bind(copied_status)
            .bind(new_index as i32)
            .bind(msg.created_at)
            .bind(msg.is_error_notice)
            .execute(&mut *conn)
            .await?;
            total_input_tokens += msg.input_tokens.unwrap_or(0) as i64;
            total_output_tokens += msg.output_tokens.unwrap_or(0) as i64;
            id_pairs.push((msg.id, new_message_id));
        }
        let message_count = id_pairs.len() as i32;
        let forked_at_index = if message_count > 0 {
            Some(message_count - 1)
        } else {
            None
        };
        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET message_count = $2,
                total_input_tokens = $3,
                total_output_tokens = $4,
                forked_at_index = $5,
                updated_at = $6
            WHERE id = $1
            "#,
        )
        .bind(new_session_id)
        .bind(message_count)
        .bind(total_input_tokens)
        .bind(total_output_tokens)
        .bind(forked_at_index)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        Ok(id_pairs)
    }
    /// Re-binds the source session's message-bound `chat_attachments` rows to
    /// the carried session (chat_service D56): one row per source attachment
    /// whose message was copied, with `session_id`/`message_id` pointing at
    /// the new rows and `storage_key`/`provider_mount_path` re-derived from
    /// the new ids (both embed the session/message ids, and `storage_key` is
    /// globally unique). Uploader, file name, content type, bytes, sha256,
    /// scan status, and upload `created_at` are preserved. Attachments
    /// orphaned by message deletion (`message_id IS NULL`) are not carried.
    ///
    /// The mount-path file name keeps the source row's already-materialized
    /// tail segment, so the SQL-side rebuild stays consistent with
    /// `materialized_attachment_file_name` without re-running it.
    async fn copy_session_attachments(
        conn: &mut sqlx::PgConnection,
        source_session_id: Uuid,
        new_session_id: Uuid,
        message_id_pairs: &[(Uuid, Uuid)],
    ) -> Result<(), AppError> {
        if message_id_pairs.is_empty() {
            return Ok(());
        }
        let (old_ids, new_ids): (Vec<Uuid>, Vec<Uuid>) = message_id_pairs.iter().copied().unzip();
        sqlx::query(
            r#"
            INSERT INTO chat_attachments (
                org_id, session_id, message_id, created_by, file_name,
                content_type, size_bytes, sha256, storage_key,
                provider_mount_path, scan_status, content_bytes, created_at
            )
            SELECT
                (SELECT org_id FROM chat_sessions WHERE id = $1),
                $1,
                map.new_id,
                a.created_by,
                a.file_name,
                a.content_type,
                a.size_bytes,
                a.sha256,
                split_part(a.storage_key, '/', 1) || '/' || $1::text || '/' || map.new_id::text
                    || '/' || a.sha256 || '/' || a.file_name,
                '/mnt/g5/' || split_part(a.storage_key, '/', 1) || '/' || $1::text
                    || '/' || map.new_id::text
                    || '/' || a.sha256 || '/'
                    || split_part(a.provider_mount_path, '/', -1),
                a.scan_status,
                a.content_bytes,
                a.created_at
            FROM chat_attachments a
            JOIN UNNEST($2::uuid[], $3::uuid[]) AS map(old_id, new_id)
              ON a.message_id = map.old_id
            WHERE a.session_id = $4
            ON CONFLICT (storage_key) DO NOTHING
            "#,
        )
        .bind(new_session_id)
        .bind(&old_ids)
        .bind(&new_ids)
        .bind(source_session_id)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }
    /// Carries selected sessions into a workspace created by the workspace
    /// fork service. It runs in the caller's transaction.
    ///
    /// Sessions for either native agent provider are carried. Wrong-workspace
    /// or unsupported-provider ids are skipped. Returns the number of sessions
    /// actually carried.
    ///
    /// Each copied session re-derives `org_id` from the target workspace,
    /// resets commit/node links, and points `forked_from_id` at its source.
    pub async fn copy_sessions_to_workspace(
        conn: &mut sqlx::PgConnection,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        session_ids: &[Uuid],
        created_by: Uuid,
    ) -> Result<i32, AppError> {
        if session_ids.is_empty() {
            return Ok(0);
        }
        #[derive(sqlx::FromRow)]
        struct CarrySource {
            id: Uuid,
            model: Option<String>,
            agent_provider: String,
            provider_state: serde_json::Value,
            title: Option<String>,
            summary: Option<String>,
            tags: Vec<String>,
        }
        let sources = sqlx::query_as::<_, CarrySource>(
            r#"
            SELECT id, model, agent_provider, provider_state, title, summary, tags
            FROM chat_sessions
            WHERE workspace_id = $1
              AND id = ANY($2)
              AND agent_provider IN ('claude_code', 'codex')
            ORDER BY created_at ASC
            "#,
        )
        .bind(source_workspace_id)
        .bind(session_ids)
        .fetch_all(&mut *conn)
        .await?;
        let initial_modes = super::attempt_snapshot::initial_session_permission_modes(
            conn,
            &sources.iter().map(|source| source.id).collect::<Vec<_>>(),
        )
        .await?;
        let now = Utc::now();
        let empty_commits: Vec<String> = Vec::new();
        let mut carried: i32 = 0;
        for source in &sources {
            let new_session_id = Uuid::new_v4();
            let mut provider_state = source.provider_state.clone();
            apply_session_permission_mode(
                &mut provider_state,
                parse_chat_agent_provider_or_default(&source.agent_provider),
                initial_modes.get(&source.id).map(String::as_str),
            );
            sqlx::query(
                r#"
                INSERT INTO chat_sessions (
                    id, org_id, workspace_id, created_by, scope_type, scope_ref,
                    model, agent_provider, provider_state, title, summary, tags,
                    linked_commits,
                    total_input_tokens, total_output_tokens, total_cost_usd,
                    message_count, forked_from_id, forked_at_index,
                    created_at, updated_at, persona, persona_instructions
                ) VALUES (
                    $1, (SELECT org_id FROM workspaces WHERE id = $2), $2, $3, 'workspace', NULL,
                    $4, $5, $6, $7, $8, $9,
                    $10,
                    0, 0, 0,
                    0, $11, NULL,
                    $12, $12,
                    (SELECT persona FROM chat_sessions WHERE id = $11),
                    (SELECT persona_instructions FROM chat_sessions WHERE id = $11)
                )
                "#,
            )
            .bind(new_session_id)
            .bind(target_workspace_id)
            .bind(created_by)
            .bind(source.model.as_deref())
            .bind(&source.agent_provider)
            .bind(&provider_state)
            .bind(source.title.as_deref())
            .bind(source.summary.as_deref())
            .bind(&source.tags)
            .bind(&empty_commits)
            .bind(source.id)
            .bind(now)
            .execute(&mut *conn)
            .await?;
            let message_id_pairs =
                Self::copy_session_messages_with_counters(conn, source.id, new_session_id, now)
                    .await?;
            Self::copy_session_attachments(conn, source.id, new_session_id, &message_id_pairs)
                .await?;
            let message_count = message_id_pairs.len() as i32;
            carried += 1;
            info!(
                source_session_id = % source.id, new_session_id = % new_session_id,
                source_workspace_id = % source_workspace_id, target_workspace_id = %
                target_workspace_id, messages_copied = message_count,
                "Carried chat session into forked workspace"
            );
        }
        Ok(carried)
    }
    /// Full-text search over session titles and summaries using Postgres
    /// GIN index with `ts_rank_cd` for relevance ordering.
    pub async fn search_sessions(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        query: &str,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<SessionListItem>, AppError> {
        let limit = pagination.limit();
        let offset = pagination.offset();
        let rows: Vec<SessionRow> = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT id, workspace_id, scope_type, scope_ref,
                   model, agent_provider, provider_state,
                   title, summary, tags, message_count,
                   total_input_tokens, total_output_tokens, total_cost_usd,
                   forked_from_id, forked_at_index, persona,
                   linked_commits,
                   created_at, updated_at
            FROM chat_sessions
            WHERE workspace_id = $1
              AND to_tsvector('english', COALESCE(title, '') || ' ' || COALESCE(summary, ''))
                  @@ websearch_to_tsquery('english', $2)
            ORDER BY
                ts_rank_cd(
                    to_tsvector('english', COALESCE(title, '') || ' ' || COALESCE(summary, '')),
                    websearch_to_tsquery('english', $2)
                ) DESC,
                updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(workspace_id)
        .bind(query)
        .bind(limit)
        .bind(offset)
        .fetch_all(db.conn())
        .await?;
        let total = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM chat_sessions
            WHERE workspace_id = $1
              AND to_tsvector('english', COALESCE(title, '') || ' ' || COALESCE(summary, ''))
                  @@ websearch_to_tsquery('english', $2)
            "#,
        )
        .bind(workspace_id)
        .bind(query)
        .fetch_one(db.conn())
        .await?;
        let mut items: Vec<_> = rows.into_iter().map(SessionRow::into_list_item).collect();
        self.enrich_session_list_items(db, workspace_id, &mut items)
            .await?;
        Ok(PaginatedResult::new(items, total, pagination))
    }
    /// Return the newest historical launch snapshot for a chat session.
    /// Turn allocation uses this only as a compatibility fallback when an old
    /// session lacks one of the persisted mutable defaults; every new turn
    /// resolves and stores its own immutable launch contract (routes/chat D27).
    /// TaskService decodes the attempts' snapshots, accepting both persisted
    /// shapes ([`super::attempt_snapshot::session_agent_launch`]).
    pub async fn agent_launch_for_session(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
    ) -> Result<Option<PersistedAgentLaunchV1>, AppError> {
        crate::services::agents::attempt_snapshot::session_agent_launch(db, session_id, true).await
    }
    /// Return the first persisted launch snapshot for a chat session.
    /// Its provider-native permission Mode is immutable session policy; later
    /// Model/Effort snapshots must never turn it into a mutable default.
    pub async fn initial_agent_launch_for_session(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
    ) -> Result<Option<PersistedAgentLaunchV1>, AppError> {
        crate::services::agents::attempt_snapshot::session_agent_launch(db, session_id, false).await
    }
    /// Get available LLM models for chat.
    ///
    /// Returns the set of models the instance is configured to offer.
    pub async fn get_available_models(&self) -> Result<Vec<ChatModelInfo>, AppError> {
        Ok(g5_contracts::model_catalog::offered_entries()
            .map(|entry| {
                ChatModelInfo::from_catalog(
                    entry.provider,
                    entry.id,
                    entry.label,
                    entry.route,
                    entry.controls.efforts,
                    entry.controls.default_effort,
                )
            })
            .collect())
    }
    /// Get all timeline entries for a chat session, across all tasks.
    ///
    /// Joins `task_events` through the task's shared session identity. Returns
    /// entries in chronological order with cursor-based pagination.
    pub async fn get_session_timeline(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        cursor: Option<String>,
        limit: i64,
    ) -> Result<(Vec<SessionTimelineEntry>, Option<String>, bool), AppError> {
        let limit = limit.clamp(1, 200);
        let fetch_limit = limit + 1;
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            entry_type: String,
            content: String,
            metadata: serde_json::Value,
            created_at: chrono::DateTime<Utc>,
        }
        let rows: Vec<Row> = if let Some(ref cursor_str) = cursor {
            let parts: Vec<&str> = cursor_str.splitn(2, '|').collect();
            if parts.len() != 2 {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!(
                        { "cursor" :
                        "Invalid cursor format — expected timestamp|uuid" }
                    ),
                }
                .into());
            }
            let cursor_ts: chrono::DateTime<Utc> =
                parts[0].parse().map_err(|_| G5Error::ValidationError {
                    details: serde_json::json!({ "cursor" : "Invalid cursor timestamp" }),
                })?;
            let cursor_id: Uuid = parts[1].parse().map_err(|_| G5Error::ValidationError {
                details: serde_json::json!({ "cursor" : "Invalid cursor id" }),
            })?;
            sqlx::query_as::<_, Row>(
                r#"
                SELECT te.id, te.entry_type, te.content,
                    CASE WHEN request.id IS NULL THEN te.metadata ELSE
                        te.metadata || jsonb_build_object('nativeRequestStatus', request.status,
                            'nativeRequestEndReason', request.end_reason,
                            'nativeRequestAnswer', request.response -> 'body') END AS metadata,
                    te.created_at
                FROM task_events te
                JOIN tasks t ON t.id = te.task_id
                LEFT JOIN provider_requests request ON request.task_id = te.task_id
                    AND request.attempt = te.attempt
                    AND request.provider_request_id = te.metadata #>> '{payload,provider_request_id}'
                    AND te.metadata ->> 'kind' = 'provider_request.opened'
                    AND te.metadata ->> 'projection_error' IS NULL
                WHERE t.chat_session_id = $1
                  AND (te.created_at < $2 OR (te.created_at = $2 AND te.id < $3))
                ORDER BY te.created_at DESC, te.id DESC
                LIMIT $4
                "#,
            )
            .bind(session_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(db.conn())
            .await?
        } else {
            sqlx::query_as::<_, Row>(
                r#"
                SELECT te.id, te.entry_type, te.content,
                    CASE WHEN request.id IS NULL THEN te.metadata ELSE
                        te.metadata || jsonb_build_object('nativeRequestStatus', request.status,
                            'nativeRequestEndReason', request.end_reason,
                            'nativeRequestAnswer', request.response -> 'body') END AS metadata,
                    te.created_at
                FROM task_events te
                JOIN tasks t ON t.id = te.task_id
                LEFT JOIN provider_requests request ON request.task_id = te.task_id
                    AND request.attempt = te.attempt
                    AND request.provider_request_id = te.metadata #>> '{payload,provider_request_id}'
                    AND te.metadata ->> 'kind' = 'provider_request.opened'
                    AND te.metadata ->> 'projection_error' IS NULL
                WHERE t.chat_session_id = $1
                ORDER BY te.created_at DESC, te.id DESC
                LIMIT $2
                "#,
            )
            .bind(session_id)
            .bind(fetch_limit)
            .fetch_all(db.conn())
            .await?
        };
        let has_more = rows.len() as i64 > limit;
        let mut entries: Vec<Row> = rows.into_iter().take(limit as usize).collect();
        let next_cursor = if has_more {
            entries.last().map(|e| {
                format!(
                    "{}|{}",
                    e.created_at
                        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                    e.id
                )
            })
        } else {
            None
        };
        entries.reverse();
        let dtos = entries
            .into_iter()
            .map(|r| SessionTimelineEntry {
                id: r.id.to_string(),
                entry_type: r.entry_type,
                content: r.content,
                metadata: r.metadata,
                created_at: r.created_at.to_rfc3339(),
            })
            .collect();
        Ok((dtos, next_cursor, has_more))
    }
    /// Fetch a session with its send capability and canonical first-launch
    /// mode. Internal callers may still request the plain stored row.
    pub async fn get_session_enriched(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        viewer: Uuid,
    ) -> Result<SessionDetail, AppError> {
        let mut detail = self.get_session(db, session_id).await?;
        self.enrich_session_detail(db, &mut detail).await?;
        detail.activity =
            Some(super::chat_activity::session_activity(db, session_id, viewer).await?);
        Ok(detail)
    }
    /// Recover the fixed mode even when an older launch omitted saved settings.
    pub async fn enrich_session_detail(
        &self,
        db: &mut ScopedDb,
        detail: &mut SessionDetail,
    ) -> Result<(), AppError> {
        detail.can_send = true;
        let initial_modes =
            super::attempt_snapshot::initial_session_permission_modes(db.conn(), &[detail.id])
                .await?;
        apply_session_permission_mode(
            &mut detail.provider_state,
            detail.agent_provider,
            initial_modes.get(&detail.id).map(String::as_str),
        );
        Ok(())
    }
    /// List/search entries can hydrate the composer before detail arrives.
    /// Recover their modes in one database query for the whole page.
    async fn enrich_session_list_items(
        &self,
        db: &mut ScopedDb,
        _workspace_id: Uuid,
        items: &mut [SessionListItem],
    ) -> Result<(), AppError> {
        let initial_modes = super::attempt_snapshot::initial_session_permission_modes(
            db.conn(),
            &items.iter().map(|item| item.id).collect::<Vec<_>>(),
        )
        .await?;
        for item in items.iter_mut() {
            item.can_send = true;
            apply_session_permission_mode(
                &mut item.provider_state,
                item.agent_provider,
                initial_modes.get(&item.id).map(String::as_str),
            );
        }
        Ok(())
    }
    /// Atomically patches one provider namespace without replacing sibling
    /// namespaces, unknown keys, runtime continuation data, or queued-turn
    /// snapshots. Setting defaults fill only absent keys; explicit setting
    /// updates win over both existing values and defaults.
    pub async fn patch_provider_state_namespace(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
        namespace: &str,
        runtime_patch: serde_json::Value,
        settings_patch: serde_json::Value,
        settings_defaults: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        for (field, value) in [
            ("runtimePatch", &runtime_patch),
            ("settingsPatch", &settings_patch),
            ("settingsDefaults", &settings_defaults),
        ] {
            if !value.is_object() {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!(
                        { (field) : format!("{field} must be a JSON object") }
                    ),
                }));
            }
        }
        let updated: Option<serde_json::Value> = sqlx::query_scalar(
                r#"
            UPDATE chat_sessions
            SET provider_state =
                (CASE
                    WHEN jsonb_typeof(provider_state) = 'object' THEN provider_state
                    ELSE '{}'::jsonb
                END)
                || jsonb_build_object(
                    $2::text,
                    (CASE
                        WHEN jsonb_typeof(provider_state -> $2::text) = 'object'
                            THEN provider_state -> $2::text
                        ELSE '{}'::jsonb
                    END)
                    || $3::jsonb
                    || (CASE
                        WHEN $4::jsonb = '{}'::jsonb AND $5::jsonb = '{}'::jsonb
                            THEN '{}'::jsonb
                        ELSE jsonb_build_object(
                            'settings',
                            $5::jsonb
                            || (CASE
                                WHEN jsonb_typeof(provider_state -> $2::text -> 'settings') = 'object'
                                    THEN provider_state -> $2::text -> 'settings'
                                ELSE '{}'::jsonb
                            END)
                            || $4::jsonb
                        )
                    END)
                ),
                updated_at = now()
            WHERE id = $1
            RETURNING provider_state
            "#,
            )
            .bind(session_id)
            .bind(namespace)
            .bind(runtime_patch)
            .bind(settings_patch)
            .bind(settings_defaults)
            .fetch_optional(db.conn())
            .await?;
        updated.ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("chat session {}", session_id),
            })
        })
    }
    fn normalize_session_title(title: Option<&str>) -> Option<String> {
        title
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    }
}

/// Claude's retired `auto` spelling always executed with bypass permissions.
/// Keep compatibility scoped to Claude; Codex and formal review modes retain
/// their provider-native policy values.
pub fn canonical_chat_permission_mode(provider: ChatAgentProvider, mode: &str) -> &str {
    match (provider, mode.trim()) {
        (ChatAgentProvider::ClaudeCode, "auto") => "bypassPermissions",
        (_, mode) => mode,
    }
}

/// Present/copy the immutable first launch mode over mutable saved state.
/// With no attempt yet, the saved first-turn settings are authoritative.
fn apply_session_permission_mode(
    state: &mut serde_json::Value,
    provider: ChatAgentProvider,
    initial_mode: Option<&str>,
) {
    let namespace = super::workspace_messages::provider_state_namespace(provider);
    let mode = initial_mode.or_else(|| {
        state
            .get(namespace)?
            .get("settings")?
            .get("permissionMode")?
            .as_str()
    });
    let Some(mode) = mode
        .map(|mode| canonical_chat_permission_mode(provider, mode).to_string())
        .filter(|mode| !mode.is_empty())
    else {
        return;
    };
    // Recover malformed legacy containers without touching sibling settings
    // or native runtime state when those containers are objects.
    for path in [&[][..], &[namespace][..], &[namespace, "settings"][..]] {
        let mut value = &mut *state;
        for key in path {
            value = &mut value[*key];
        }
        if !value.is_object() {
            *value = serde_json::json!({});
        }
    }
    state[namespace]["settings"]["permissionMode"] = serde_json::json!(mode);
}

/// The account a chat's next turn uses when the sender names none
/// (routes/chat D33): the account `resolved` ran on, or the provider's
/// Runtime Config variables when it ran on none. A chat begun on a named
/// account keeps that account until the sender picks another.
pub fn next_turn_account(resolved: &ResolvedAgentExecutionV1) -> AgentAccountSelectionV1 {
    match resolved.account() {
        Some(account) => AgentAccountSelectionV1::Account {
            account_id: account.id,
        },
        None => AgentAccountSelectionV1::RuntimeConfig,
    }
}

/// The settings a new chat saves from its first turn (chat_service D65,
/// routes/chat D33): the resolved model, permission mode and effort, and the
/// account the chat keeps for later turns.
pub fn first_turn_settings(
    resolved: &ResolvedAgentExecutionV1,
) -> Result<serde_json::Value, AppError> {
    Ok(serde_json::json!({
        "model": resolved.model.value,
        "permissionMode": resolved.permission_mode.value,
        "effort": resolved.effort.value,
        "account": encode_next_turn_account(&next_turn_account(resolved))?,
    }))
}

/// The settings patch that saves `account` as the chat's next-turn account
/// (routes/chat D33).
pub fn next_turn_account_setting(
    account: &AgentAccountSelectionV1,
) -> Result<serde_json::Value, AppError> {
    Ok(serde_json::json!({ "account": encode_next_turn_account(account)? }))
}

fn encode_next_turn_account(
    account: &AgentAccountSelectionV1,
) -> Result<serde_json::Value, AppError> {
    serde_json::to_value(account).map_err(|error| {
        AppError::from_error(G5Error::Internal {
            message: format!("failed to encode the next-turn account: {error}"),
        })
    })
}

/// Longest title either derivation produces, in bytes (also the wire cap
/// `PATCH /sessions/:id` enforces for user renames).
pub const MAX_DERIVED_TITLE_BYTES: usize = 80;

/// `chat_sessions.title_source`: who owns the current title. `derived` and
/// `summary` are platform-owned; `user` is a rename or an explicit clear and
/// is never replaced automatically. Legacy rows carry NULL and count as
/// user-owned when a title is present, platform-owned when it is NULL (see
/// [`PLATFORM_OWNED_TITLE_SQL`]).
pub const TITLE_SOURCE_DERIVED: &str = "derived";
pub const TITLE_SOURCE_SUMMARY: &str = "summary";
pub const TITLE_SOURCE_USER: &str = "user";
/// Row predicate for "the platform may still rename this session". Written
/// with `IS NOT DISTINCT FROM` so a legacy NULL source yields false, never
/// NULL (it is also selected as a bare boolean).
const PLATFORM_OWNED_TITLE_SQL: &str = "(title_source IS NOT DISTINCT FROM 'derived' \
     OR (title_source IS NULL AND title IS NULL))";

/// Derive a provisional session title from the first user message: the first
/// non-empty line outside a code fence, cut at the first sentence end, capped
/// at [`MAX_DERIVED_TITLE_BYTES`] on a word boundary with an ellipsis.
///
/// This is the "first several words" placeholder that names a session the
/// moment it starts. The workspace agent later replaces it with a one-line
/// summary (`session.title` turn event) as long as the title still equals the
/// value this function derives, so a user rename is never overwritten.
pub fn derive_session_title(content: &str) -> Option<String> {
    let mut in_fence = false;
    let cleaned = content
        .lines()
        .map(str::trim)
        .filter(|line| {
            if line.starts_with("```") {
                in_fence = !in_fence;
                return false;
            }
            !in_fence
        })
        .find(|line| !line.is_empty())?;
    let sentence_end = cleaned
        .find(['.', '?', '!'])
        .map(|i| i + 1)
        .unwrap_or(cleaned.len());
    let truncate_at = sentence_end.min(MAX_DERIVED_TITLE_BYTES);
    let safe_end = (0..=truncate_at)
        .rev()
        .find(|&i| cleaned.is_char_boundary(i))
        .unwrap_or(0);
    let title = &cleaned[..safe_end];
    if safe_end >= MAX_DERIVED_TITLE_BYTES
        && !title.ends_with(|c: char| c.is_whitespace() || c == '.' || c == '?' || c == '!')
    {
        if let Some(last_space) = title.rfind(' ') {
            return Some(format!("{}…", title[..last_space].trim_end()));
        }
    }
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// Normalize a model-written summary title: first non-empty line, wrapping
/// quotes and a trailing period removed, collapsed whitespace, capped at
/// [`MAX_DERIVED_TITLE_BYTES`] on a character boundary. `None` when nothing
/// usable remains, so callers keep the provisional title.
pub fn sanitize_generated_title(raw: &str) -> Option<String> {
    let collapsed = raw
        .lines()
        .map(|line| {
            let line = line.trim();
            // Models that ignore "title only" tend to label the answer.
            let line = ["title:", "Title:", "TITLE:"]
                .iter()
                .find_map(|prefix| line.strip_prefix(prefix))
                .unwrap_or(line);
            line.trim_start_matches(['"', '\'', '“', '‘', '`', '#', '*', '-'])
                .trim_end_matches(['"', '\'', '”', '’', '`', '*', '.'])
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .find(|line| !line.is_empty())?;
    if collapsed.len() <= MAX_DERIVED_TITLE_BYTES {
        return Some(collapsed);
    }
    let cut = (0..=MAX_DERIVED_TITLE_BYTES)
        .rev()
        .find(|&i| collapsed.is_char_boundary(i))
        .unwrap_or(0);
    let head = collapsed[..cut].trim_end();
    let head = head.rfind(' ').map_or(head, |space| &head[..space]);
    Some(format!("{}…", head.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_session_title_uses_the_first_sentence_outside_code_fences() {
        assert_eq!(
            derive_session_title(
                "```rust\nfn main() {}\n```\n  Fix the login bug. Then add tests."
            ),
            Some("Fix the login bug.".to_string())
        );
        assert_eq!(
            derive_session_title("can you check the preview\nsecond line"),
            Some("can you check the preview".to_string())
        );
        assert_eq!(derive_session_title("   \n\n"), None);
        assert_eq!(derive_session_title("```\nonly code\n```"), None);
        assert_eq!(
            derive_session_title("```\ncode\n```\ntrailing question?"),
            Some("trailing question?".to_string())
        );
    }

    #[test]
    fn derive_session_title_caps_long_requests_on_a_word_boundary() {
        let long = "please refactor the workspace preview dialog so that a failed preview task explains itself instead of silently returning";
        let title = derive_session_title(long).expect("title");
        assert!(title.ends_with('…'), "{title}");
        assert!(
            title.len() <= MAX_DERIVED_TITLE_BYTES + '…'.len_utf8(),
            "{title}"
        );
        assert!(!title.trim_end_matches('…').ends_with(' '), "{title}");
        // Multi-byte input never splits a character.
        let cjk = "请帮我检查预览功能为什么打不开，前端一直显示启动预览，点击之后地址是空白的，这个问题从昨天开始出现";
        let title = derive_session_title(cjk).expect("title");
        assert!(
            title.len() <= MAX_DERIVED_TITLE_BYTES + '…'.len_utf8(),
            "{title}"
        );
    }

    #[test]
    fn sanitize_generated_title_strips_model_decoration() {
        assert_eq!(
            sanitize_generated_title("\"Preview dialog fails silently.\"\n"),
            Some("Preview dialog fails silently".to_string())
        );
        assert_eq!(
            sanitize_generated_title("Title:   \n**Fix   preview   readiness**"),
            Some("Fix preview readiness".to_string())
        );
        assert_eq!(
            sanitize_generated_title("Title: Diagnose blank preview tab"),
            Some("Diagnose blank preview tab".to_string())
        );
        assert_eq!(sanitize_generated_title("\n  \"\"  \n"), None);
        let long = "a ".repeat(60);
        let title = sanitize_generated_title(&long).expect("title");
        assert!(title.ends_with('…') && title.len() <= MAX_DERIVED_TITLE_BYTES + '…'.len_utf8());
    }
    fn validated_attachment(file_name: &str, content_bytes: Vec<u8>) -> ValidatedChatAttachment {
        ValidatedChatAttachment {
            file_name: file_name.to_string(),
            content_type: None,
            content_bytes,
        }
    }

    #[test]
    fn validate_inline_attachment_rejects_oversize() {
        let oversize = "A".repeat(MAX_CHAT_ATTACHMENT_BASE64_CHARS + 4);
        let result = validate_chat_attachment_payload(
            "big.png",
            Some("image/png".to_string()),
            &oversize,
            false,
        );
        assert!(matches!(
            result.map_err(|error| error.inner),
            Err(G5Error::ValidationError { .. })
        ));
    }

    #[test]
    fn validate_inline_attachment_set_rejects_duplicate_bytes() {
        let attachments = [
            validated_attachment("one", b"same".to_vec()),
            validated_attachment("two", b"same".to_vec()),
        ];
        let result = validate_inline_chat_attachment_set_guards(&attachments);
        assert!(matches!(
            result.map_err(|error| error.inner),
            Err(G5Error::ValidationError { .. })
        ));
    }

    #[test]
    fn first_turn_replay_digest_binds_exact_attachment_bytes() {
        use base64::Engine as _;
        let attachment = |bytes: &[u8]| InlineChatAttachment {
            file_name: "image.png".into(),
            content_type: Some("image/png".into()),
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        };
        let first = inline_attachment_request_digest(&[attachment(b"first")]).unwrap();
        assert_eq!(
            first,
            inline_attachment_request_digest(&[attachment(b"first")]).unwrap()
        );
        assert_ne!(
            first,
            inline_attachment_request_digest(&[attachment(b"second")]).unwrap()
        );
        assert_ne!(first, inline_attachment_request_digest(&[]).unwrap());
    }

    #[test]
    fn validate_inline_attachment_set_enforces_ten_mibibyte_raw_total() {
        let within_limit = vec![
            validated_attachment("one", vec![0; MAX_CHAT_ATTACHMENT_BYTES]),
            validated_attachment("two", vec![1; MAX_CHAT_ATTACHMENT_BYTES]),
        ];
        validate_inline_chat_attachment_set_guards(&within_limit)
            .expect("exactly ten MiB is accepted");

        let over_limit = vec![
            validated_attachment("one", vec![0; MAX_CHAT_ATTACHMENT_BYTES]),
            validated_attachment("two", vec![1; MAX_CHAT_ATTACHMENT_BYTES]),
            validated_attachment("three", vec![2]),
        ];
        let result = validate_inline_chat_attachment_set_guards(&over_limit);
        assert!(matches!(
            result.map_err(|error| error.inner),
            Err(G5Error::ValidationError { .. })
        ));
    }
}
