use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a row in the `comment_replies` table (API_AND_STORAGE.md §2.6).
///
/// A threaded reply to a top-level comment, enabling discussion threads on any
/// comment. Replies are ordered by `created_at` via the `idx_reply_comment`
/// index on `(comment_id, created_at)`.
///
/// This is a passive data container — no business logic, no database queries.
/// All reply management lives in the service layer.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CommentReply {
    pub id: Uuid,
    pub comment_id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub created_at: DateTime<Utc>,
}
