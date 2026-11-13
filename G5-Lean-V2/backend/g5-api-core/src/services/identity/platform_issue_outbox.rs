//! Platform bug-report delivery outbox worker.
//!
//! Drains `platform_issue_outbox` and delivers each pending report to the
//! configured G5-owned GitHub repository via [`PlatformIssueIntakeService`],
//! with retry/backoff. Only spawned when the `platform_issue_reporting` feature
//! is enabled AND the GitHub delivery config is fully present (fail-closed), so
//! a disabled or unconfigured deployment does no outbox work.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::services::platform_issue_github::PlatformIssueGithubClient;
use crate::services::platform_issue_intake::{
    PlatformIssueIntakeService, PlatformIssueOutboxBatchResult,
};

const PLATFORM_ISSUE_OUTBOX_POLL_INTERVAL: Duration = Duration::from_secs(10);
const PLATFORM_ISSUE_OUTBOX_BATCH_SIZE: i64 = 10;

pub async fn run_platform_issue_outbox_loop(
    pool: PgPool,
    github: Arc<PlatformIssueGithubClient>,
    cancel: CancellationToken,
) {
    let service = PlatformIssueIntakeService::new(pool, github, None);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Platform issue outbox worker shutting down");
                break;
            }
            _ = tokio::time::sleep(PLATFORM_ISSUE_OUTBOX_POLL_INTERVAL) => {
                match service.process_due(PLATFORM_ISSUE_OUTBOX_BATCH_SIZE).await {
                    Ok(PlatformIssueOutboxBatchResult { claimed: 0, .. }) => {
                        debug!("Platform issue outbox worker found no due reports");
                    }
                    Ok(result) => {
                        info!(
                            claimed = result.claimed,
                            completed = result.completed,
                            failed = result.failed,
                            "Platform issue outbox batch processed"
                        );
                    }
                    Err(err) => {
                        error!(error = %err, "Platform issue outbox batch failed");
                    }
                }
            }
        }
    }
}
