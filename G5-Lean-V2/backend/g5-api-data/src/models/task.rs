use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A durable unit of workspace work, with execution attempts and dependency edges.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub task_type: String,
    pub status: String,
    pub base_commit: Option<String>,
    pub retry_count: i32,
    /// Read from `task_dependencies` (`TaskDependency`), not the legacy
    /// `tasks.depends_on` column.
    pub depends_on: Vec<Uuid>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Optional source context for the task
    pub component_id: Option<String>,
    pub src_files: Vec<String>,
    pub node_level: Option<String>,
    pub language: Option<String>,
    pub parent_component_id: Option<String>,
    pub child_component_ids: Vec<String>,
    /// Maximum wall-clock execution time in seconds. Exceeded → failed.
    #[sqlx(default)]
    pub max_duration_secs: Option<i32>,
    /// Shared agent session. Required by the final schema when kind=agent.
    #[sqlx(default)]
    pub chat_session_id: Option<Uuid>,
}
