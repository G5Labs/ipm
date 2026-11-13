//! Notification service — CRUD for persistent user notifications,
//! mention extraction, and real-time delivery via WebSocket.
//!
//! The notification service provides:
//! - Persistent notification storage (mentions, task events, spend alerts)
//! - Mention detection from comment/reply content
//! - Real-time notification delivery to `user:<uuid>` channels
//! - Read/dismiss state management

use std::{collections::HashMap, sync::LazyLock};

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use g5_contracts::utils::safe_truncate;

use crate::errors::{AppError, G5Error};
use crate::services::EventBus;
use crate::ws::events::{NotificationAction, NotificationPayload};
use crate::ws::{Channel, WsEvent};

/// Pre-compiled regex for @mention extraction, avoiding per-call recompilation.
static MENTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@(\w[\w.-]{0,38})").unwrap());

// ---------------------------------------------------------------------------
// DTO Types
// ---------------------------------------------------------------------------

/// Notification as returned by the REST API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDTO {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub task_id: Option<Uuid>,
    pub comment_id: Option<Uuid>,
    pub reply_id: Option<Uuid>,
    pub actions: serde_json::Value, // JSONB
    pub read_at: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    /// Actor info (who triggered this notification)
    pub actor_name: Option<String>,
    pub actor_avatar: Option<String>,
}

/// Request to create a notification.
#[derive(Debug, Clone)]
pub struct CreateNotificationRequest {
    pub workspace_id: Uuid,
    pub recipient_user_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub task_id: Option<Uuid>,
    pub comment_id: Option<Uuid>,
    pub reply_id: Option<Uuid>,
    pub actions: Vec<NotificationAction>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Query parameters for listing notifications.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotificationFilters {
    pub unread_only: Option<bool>,
    pub limit: Option<i64>,
}

/// Mention extraction result.
#[derive(Debug, Clone)]
pub struct ExtractedMention {
    pub user_id: Uuid,
    pub display_name: String,
}

// ---------------------------------------------------------------------------
// Internal row types
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct NotificationRecord {
    id: Uuid,
    workspace_id: Uuid,
    category: String,
    severity: String,
    title: String,
    message: String,
    task_id: Option<Uuid>,
    comment_id: Option<Uuid>,
    reply_id: Option<Uuid>,
    actions: serde_json::Value,
    read_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    actor_user_id: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct UserSummaryRow {
    id: Uuid,
    display_name: String,
    avatar_url: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkspaceMentionScope {
    org_id: Uuid,
    personal_user_id: Option<Uuid>,
    agent_approval_review_id: Option<Uuid>,
    owner_user_id: Option<Uuid>,
}

impl NotificationRecord {
    fn into_dto(self, actors: &HashMap<Uuid, UserSummaryRow>) -> NotificationDTO {
        let actor = self.actor_user_id.and_then(|user_id| actors.get(&user_id));
        NotificationDTO {
            id: self.id,
            workspace_id: self.workspace_id,
            category: self.category,
            severity: self.severity,
            title: self.title,
            message: self.message,
            task_id: self.task_id,
            comment_id: self.comment_id,
            reply_id: self.reply_id,
            actions: self.actions,
            read_at: self.read_at.map(|dt| dt.to_rfc3339()),
            expires_at: self.expires_at.map(|dt| dt.to_rfc3339()),
            created_at: self.created_at.to_rfc3339(),
            actor_name: actor.map(|summary| summary.display_name.clone()),
            actor_avatar: actor.and_then(|summary| summary.avatar_url.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// NotificationService
// ---------------------------------------------------------------------------

/// Notification CRUD, mention extraction, and real-time delivery.
pub struct NotificationService {
    event_bus: EventBus,
    system_pool: PgPool,
}

impl NotificationService {
    #[must_use]
    pub fn new(event_bus: EventBus, system_pool: PgPool) -> Self {
        Self {
            event_bus,
            system_pool,
        }
    }

    // -----------------------------------------------------------------------
    // create_notification
    // -----------------------------------------------------------------------

    /// Creates a notification on the caller's transaction and publishes its
    /// WS event to the recipient once that transaction commits, so a
    /// delivered notice always names a stored row.
    pub async fn create_notification(
        &self,
        db: &mut ScopedDb,
        req: CreateNotificationRequest,
    ) -> Result<NotificationDTO, AppError> {
        let actions_json = serde_json::to_value(&req.actions).unwrap_or_default();

        let row = sqlx::query_as!(
            NotificationRecord,
            r#"
            INSERT INTO notifications (
                workspace_id, recipient_user_id, actor_user_id,
                category, severity, title, message,
                task_id, comment_id, reply_id,
                actions, expires_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING
                id,
                workspace_id,
                category,
                severity,
                title,
                message,
                task_id,
                comment_id,
                reply_id,
                actions,
                read_at,
                expires_at,
                created_at,
                actor_user_id
            "#,
            req.workspace_id,
            req.recipient_user_id,
            req.actor_user_id,
            req.category,
            req.severity,
            req.title,
            req.message,
            req.task_id,
            req.comment_id,
            req.reply_id,
            actions_json,
            req.expires_at,
        )
        .fetch_one(db.conn())
        .await
        .map_err(|e| {
            error!(
                recipient = %req.recipient_user_id,
                error = %e,
                "Failed to create notification"
            );
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to create notification: {}", e),
            })
        })?;

        let actor_summaries = self.load_user_summaries(row.actor_user_id).await?;
        let dto = row.into_dto(&actor_summaries);

        // Publish to the recipient's user channel for real-time delivery.
        let payload = NotificationPayload {
            id: dto.id,
            severity: dto.severity.clone(),
            category: dto.category.clone(),
            title: dto.title.clone(),
            message: dto.message.clone(),
            task_id: dto.task_id,
            comment_id: dto.comment_id,
            reply_id: dto.reply_id,
            actions: req.actions,
            created_at: Utc::now(),
            expires_at: req.expires_at,
        };

        let ws_event = WsEvent::Notification {
            workspace_id: req.workspace_id,
            notification: payload,
        };

        // After commit, publish to the recipient's user channel (private)
        // and, for clients that only subscribe to the workspace channel, to
        // that channel but filtered to the recipient's own connections: a
        // notification is never broadcast to everyone viewing the workspace.
        let event_bus = self.event_bus.clone();
        let recipient_user_id = req.recipient_user_id;
        let workspace_id = req.workspace_id;
        db.on_commit(move || {
            event_bus.publish(&Channel::User(recipient_user_id), &ws_event);
            event_bus.publish_to_user(
                &Channel::Workspace(workspace_id),
                recipient_user_id,
                &ws_event,
            );
        });

        info!(
            notification_id = %dto.id,
            recipient = %req.recipient_user_id,
            category = %dto.category,
            "Notification created; delivery follows commit"
        );

        Ok(dto)
    }

    // -----------------------------------------------------------------------
    // list_notifications
    // -----------------------------------------------------------------------

    /// Lists notifications for a specific user in a workspace.
    pub async fn list_notifications(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        user_id: Uuid,
        filters: NotificationFilters,
    ) -> Result<Vec<NotificationDTO>, AppError> {
        let limit = filters.limit.unwrap_or(50).clamp(1, 100);
        let unread_only = filters.unread_only.unwrap_or(false);

        let rows = sqlx::query_as!(
            NotificationRecord,
            r#"
            SELECT
                id,
                workspace_id,
                category,
                severity,
                title,
                message,
                task_id,
                comment_id,
                reply_id,
                actions,
                read_at,
                expires_at,
                created_at,
                actor_user_id
            FROM notifications
            WHERE workspace_id = $1
              AND recipient_user_id = $2
              AND ($3::BOOLEAN = FALSE OR read_at IS NULL)
            ORDER BY created_at DESC
            LIMIT $4
            "#,
            workspace_id,
            user_id,
            unread_only,
            limit,
        )
        .fetch_all(db.conn())
        .await
        .map_err(|e| {
            error!(
                workspace_id = %workspace_id,
                user_id = %user_id,
                error = %e,
                "Failed to list notifications"
            );
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to list notifications: {}", e),
            })
        })?;

        let actor_summaries = self
            .load_user_summaries(
                rows.iter()
                    .filter_map(|row| row.actor_user_id)
                    .collect::<Vec<_>>(),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.into_dto(&actor_summaries))
            .collect())
    }

    // -----------------------------------------------------------------------
    // mark_read
    // -----------------------------------------------------------------------

    /// Marks a single notification as read.
    pub async fn mark_read(
        &self,
        db: &mut ScopedDb,
        notification_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let result = sqlx::query!(
            r#"
            UPDATE notifications
            SET read_at = now()
            WHERE id = $1 AND recipient_user_id = $2 AND read_at IS NULL
            "#,
            notification_id,
            user_id,
        )
        .execute(db.conn())
        .await
        .map_err(|e| {
            error!(
                notification_id = %notification_id,
                error = %e,
                "Failed to mark notification read"
            );
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to mark notification read: {}", e),
            })
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::from_error(G5Error::NotFound {
                resource: format!("Notification {}", notification_id),
            }));
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // mark_all_read
    // -----------------------------------------------------------------------

    /// Marks a user's unread notifications in a workspace as read: all of
    /// them, or with `ids` only that set, so a client clearing what it showed
    /// never marks a row that arrived unseen in the meantime
    /// (g5.user.notifications). An empty set marks nothing.
    pub async fn mark_all_read(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        user_id: Uuid,
        ids: Option<&[Uuid]>,
    ) -> Result<u64, AppError> {
        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET read_at = now()
            WHERE workspace_id = $1
              AND recipient_user_id = $2
              AND read_at IS NULL
              AND ($3::uuid[] IS NULL OR id = ANY($3))
            "#,
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(ids)
        .execute(db.conn())
        .await
        .map_err(|e| {
            error!(
                workspace_id = %workspace_id,
                user_id = %user_id,
                error = %e,
                "Failed to mark all notifications read"
            );
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to mark notifications read: {}", e),
            })
        })?;

        Ok(result.rows_affected())
    }

    // -----------------------------------------------------------------------
    // Mention extraction + notification creation
    // -----------------------------------------------------------------------

    /// Extract `@mention` targets from content text and resolve them against
    /// users who can access the workspace. Returns a deduplicated list of
    /// user IDs.
    ///
    /// Primary path: trust explicit `mention_user_ids` from the client (already
    /// resolved by the mention picker). Fallback: parse `@token` patterns
    /// from content and match them against the same workspace-visible audience.
    pub async fn resolve_mentions(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        author_id: Uuid,
        content: &str,
        explicit_mention_ids: &[Uuid],
    ) -> Result<Vec<ExtractedMention>, AppError> {
        let workspace = sqlx::query_as::<_, WorkspaceMentionScope>(
            "SELECT org_id, personal_user_id, owner_user_id, agent_approval_review_id
             FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("workspace {workspace_id}"),
            })
        })?;

        let handles: Vec<String> = MENTION_RE
            .captures_iter(content)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_lowercase()))
            .collect();
        if explicit_mention_ids.is_empty() && handles.is_empty() {
            return Ok(Vec::new());
        }
        // Membership is system-owned. Pass only the already resolved workspace
        // identity; use the same audience predicate as direct workspace reads.
        let rows = sqlx::query_as::<_, UserSummaryRow>(
            r#"
            SELECT u.id, COALESCE(u.display_name, u.email) AS display_name, u.avatar_url
            FROM users u JOIN org_memberships om ON om.user_id = u.id
            WHERE om.org_id = $1 AND om.deactivated_at IS NULL AND u.id <> $2
              AND authz.workspace_audience_allows($3, $4, $5, u.id, om.role = 'admin')
              AND ((cardinality($6::uuid[]) > 0 AND u.id = ANY($6))
                   OR (cardinality($6::uuid[]) = 0
                       AND LOWER(COALESCE(u.display_name, u.email)) = ANY($7::text[])))
            "#,
        )
        .bind(workspace.org_id)
        .bind(author_id)
        .bind(workspace.personal_user_id)
        .bind(workspace.owner_user_id)
        .bind(workspace.agent_approval_review_id)
        .bind(explicit_mention_ids)
        .bind(&handles)
        .fetch_all(&self.system_pool)
        .await?;

        let mut mentions = rows
            .into_iter()
            .map(|row| ExtractedMention {
                user_id: row.id,
                display_name: row.display_name,
            })
            .collect::<Vec<_>>();
        mentions.sort_by_key(|mention| mention.user_id);
        mentions.dedup_by_key(|mention| mention.user_id);

        Ok(mentions)
    }

    /// Persist mention edges and create notifications for each mentioned user.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_mention_notifications(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        author_id: Uuid,
        author_name: &str,
        comment_id: Option<Uuid>,
        reply_id: Option<Uuid>,
        mentions: &[ExtractedMention],
        content_preview: &str,
    ) -> Result<(), AppError> {
        // Check feature flag
        let enabled = std::env::var("G5_ENABLE_MENTION_NOTIFICATIONS")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        if !enabled {
            return Ok(());
        }

        for mention in mentions {
            // A failed write leaves the transaction aborted until we roll back
            // to the savepoint in `process_mentions_best_effort`.
            sqlx::query(
                r#"
                INSERT INTO comment_mentions (
                    org_id, workspace_id, comment_id, reply_id,
                    mentioned_user_id, mentioned_by_user_id
                ) VALUES ((SELECT org_id FROM workspaces WHERE id = $1), $1, $2, $3, $4, $5)
                ON CONFLICT ON CONSTRAINT uq_mention_per_content DO NOTHING
                "#,
            )
            .bind(workspace_id)
            .bind(comment_id)
            .bind(reply_id)
            .bind(mention.user_id)
            .bind(author_id)
            .execute(db.conn())
            .await
            .map_err(|e| {
                warn!(
                    mention_user = %mention.user_id,
                    error = %e,
                    "Failed to persist mention edge"
                );
                AppError::from_error(G5Error::Internal {
                    message: format!("Failed to persist mention edge: {}", e),
                })
            })?;

            // 2. Create notification
            let title = format!("{} mentioned you", author_name);
            let preview = if content_preview.len() > 120 {
                format!("{}…", safe_truncate(content_preview, 120))
            } else {
                content_preview.to_string()
            };

            self.create_notification(
                db,
                CreateNotificationRequest {
                    workspace_id,
                    recipient_user_id: mention.user_id,
                    actor_user_id: Some(author_id),
                    category: "mention".to_string(),
                    severity: "info".to_string(),
                    title,
                    message: preview,
                    task_id: None,
                    comment_id,
                    reply_id,
                    actions: vec![],
                    expires_at: None,
                },
            )
            .await
            .map_err(|e| {
                warn!(
                    mention_user = %mention.user_id,
                    error = %e,
                    "Failed to create mention notification"
                );
                e
            })?;
        }

        info!(
            workspace_id = %workspace_id,
            mention_count = mentions.len(),
            "Mention notifications created"
        );

        Ok(())
    }

    /// Resolve and fan out comment/reply mentions without poisoning the
    /// caller's outer transaction if mention-specific work fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn process_mentions_best_effort(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        author_id: Uuid,
        author_name: &str,
        comment_id: Option<Uuid>,
        reply_id: Option<Uuid>,
        content_preview: &str,
        mention_user_ids: &[Uuid],
    ) {
        if mention_user_ids.is_empty() && !content_preview.contains('@') {
            return;
        }

        // Notices created inside the savepoint publish after commit; a
        // rolled-back savepoint must also withdraw them.
        let commit_actions_mark = db.commit_actions_mark();
        if let Err(error) = sqlx::query("SAVEPOINT g5_comment_mentions")
            .execute(db.conn())
            .await
        {
            warn!(
                workspace_id = %workspace_id,
                error = %error,
                "Failed to create mention savepoint; skipping mention processing"
            );
            return;
        }

        let mention_result: Result<(), AppError> = async {
            let mentions = self
                .resolve_mentions(
                    db,
                    workspace_id,
                    author_id,
                    content_preview,
                    mention_user_ids,
                )
                .await?;

            if mentions.is_empty() {
                return Ok(());
            }

            self.create_mention_notifications(
                db,
                workspace_id,
                author_id,
                author_name,
                comment_id,
                reply_id,
                &mentions,
                content_preview,
            )
            .await
        }
        .await;

        match mention_result {
            Ok(()) => {
                if let Err(error) = sqlx::query("RELEASE SAVEPOINT g5_comment_mentions")
                    .execute(db.conn())
                    .await
                {
                    warn!(
                        workspace_id = %workspace_id,
                        error = %error,
                        "Failed to release mention savepoint after success"
                    );
                }
            }
            Err(error) => {
                warn!(
                    workspace_id = %workspace_id,
                    error = %error,
                    "Mention processing failed; rolling back best-effort work"
                );
                db.discard_commit_actions_since(commit_actions_mark);
                if let Err(rollback_error) =
                    sqlx::query("ROLLBACK TO SAVEPOINT g5_comment_mentions")
                        .execute(db.conn())
                        .await
                {
                    warn!(
                        workspace_id = %workspace_id,
                        error = %rollback_error,
                        "Failed to roll back mention savepoint"
                    );
                }
                if let Err(release_error) = sqlx::query("RELEASE SAVEPOINT g5_comment_mentions")
                    .execute(db.conn())
                    .await
                {
                    warn!(
                        workspace_id = %workspace_id,
                        error = %release_error,
                        "Failed to release mention savepoint after rollback"
                    );
                }
            }
        }
    }

    async fn load_user_summaries<I>(
        &self,
        user_ids: I,
    ) -> Result<HashMap<Uuid, UserSummaryRow>, AppError>
    where
        I: IntoIterator<Item = Uuid>,
    {
        let mut user_ids: Vec<Uuid> = user_ids.into_iter().collect();
        user_ids.sort();
        user_ids.dedup();

        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as!(
            UserSummaryRow,
            r#"
            SELECT
                id AS "id!",
                COALESCE(display_name, email) AS "display_name!",
                avatar_url AS "avatar_url?"
            FROM users
            WHERE id = ANY($1::UUID[])
            "#,
            &user_ids,
        )
        .fetch_all(&self.system_pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to load notification actor summaries");
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to load notification actors: {}", e),
            })
        })?;

        Ok(rows.into_iter().map(|row| (row.id, row)).collect())
    }
}
