use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Represents a row in the `docs` table.
///
/// A markdown document stored in the `.g5/docs/` directory of a workspace's
/// docs repository. Document reference semantics come from canonical
/// frontmatter at an exact repository cut.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Doc {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Option<Uuid>,
    pub org_id: Uuid,
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub document_source: Option<String>,
    pub last_uplifted_at: Option<DateTime<Utc>>,
    pub last_downlifted_at: Option<DateTime<Utc>>,
    pub content_updated_at: Option<DateTime<Utc>>,
    pub content_hash: Option<String>,
    pub last_uplifted_commit_sha: Option<String>,
    pub last_downlifted_commit_sha: Option<String>,
    pub linked_doc_paths: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub content: Option<String>,
}
