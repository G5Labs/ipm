//! Task, timeline, checkpoint, and diff response contracts.

use serde::{Deserialize, Serialize};

use g5_contracts::types::TaskLifecyclePosture;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDTO {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub status: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub node_level: Option<String>,
    pub spec_context: Option<SpecContextDTO>,
    pub depends_on: Vec<String>,
    pub files_changed: Vec<String>,
    pub cost_usd: f64,
    pub session_id: Option<String>,
    pub assigned_at: Option<String>,
    pub completed_at: Option<String>,
    /// Public lifecycle posture derived from the active/latest attempt.
    pub lifecycle_posture: TaskLifecyclePosture,
    /// Latest durable execution attempt, when the task has ever ACKed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_attempt: Option<TaskExecutionAttemptDTO>,
    /// Claude's live background set for the running attempt (the latest
    /// `background_tasks.replaced` payload), when it is non-empty. Set only
    /// on the session's live-task view, so a reload keeps the waiting panel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_wait: Option<serde_json::Value>,
    /// Eligibility hint for the session composer; send rechecks admission.
    pub native_steering_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionAttemptDTO {
    pub id: uuid::Uuid,
    pub attempt_number: i32,
    pub provider: Option<String>,
    pub provider_session_id: Option<String>,
    pub disposition: Option<String>,
    pub reason: Option<String>,
    pub retry_policy: Option<String>,
    pub lifecycle_posture: TaskLifecyclePosture,
    pub started_at: String,
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_summary: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecContextDTO {
    pub component_id: String,
    pub src_files: Vec<String>,
    pub parent_spec: Option<String>,
    pub child_specs: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntryDTO {
    pub id: String,
    pub entry_type: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskFileDiff {
    pub repo_id: uuid::Uuid,
    pub repo_name: String,
    pub file: String,
    pub additions: i64,
    pub deletions: i64,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub start_line: i64,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    #[serde(rename = "type")]
    pub line_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line: Option<i64>,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_line_preserves_type_name_and_omits_missing_numbers() {
        let line = DiffLine {
            line_type: "addition".to_string(),
            old_line: None,
            new_line: Some(4),
            content: "+new".to_string(),
        };
        assert_eq!(
            serde_json::to_value(line).unwrap(),
            serde_json::json!({"type":"addition","newLine":4,"content":"+new"})
        );
    }
}

/// A bounded workspace history row; execution detail is fetched by task ID.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListEntry {
    pub id: uuid::Uuid,
    pub name: String,
    pub task_type: String,
    pub status: String,
    pub workspace_id: uuid::Uuid,
    pub session_id: Option<uuid::Uuid>,
    pub cost_usd: f64,
    pub created_at: String,
    pub updated_at: String,
    /// Why a failed task failed, from its recorded outcome; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<TaskFailure>,
}

/// A failed task's recorded reason code and, when the outcome has one, its
/// text detail (for example a dispatch admission refusal).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskFailure {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl TaskFailure {
    /// The failure a task's `status` and `outcome` record, if it failed.
    pub fn from_outcome(status: &str, outcome: Option<&serde_json::Value>) -> Option<Self> {
        if status != "failed" {
            return None;
        }
        let outcome = outcome?;
        Some(Self {
            reason: outcome.get("reason")?.as_str()?.to_string(),
            detail: outcome
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }
}
