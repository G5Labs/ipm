//! Content-free activity observability shared by host and workspace layers.
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Call only after the owning transaction commits. Reasons must be fixed;
/// never pass RPC parameters, terminal input, file paths or document contents.
#[allow(clippy::too_many_arguments)]
pub fn log_activity_change(
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    activity_kind: &str,
    activity_reason: &str,
    correlation_id: &str,
    previous: Option<DateTime<Utc>>,
    new: Option<DateTime<Utc>>,
) {
    if previous == new {
        return;
    }
    tracing::info!(
        target: "g5_api::workspace_activity",
        event = "workspace_activity_changed",
        %workspace_id, %vm_id, epoch, activity_kind, activity_reason, correlation_id,
        previous_last_activity_at = ?previous,
        new_last_activity_at = ?new,
        "workspace activity timestamp changed"
    );
}
