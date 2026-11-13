//! Authz session pruning background task.
//!
//! Periodically deletes:
//! 1. Expired `authz.user_sessions` rows (where `expires_at < now()`)
//! 2. Stale `authz.session_ctx` rows (orphaned by disconnected connections)
//!
//! This task is spawned by `MaintenanceRunner::spawn_all()` in `mod.rs` and
//! runs every 60 seconds. It uses the same pool as other maintenance tasks
//! (the main app pool has GRANT DELETE on these tables).
//!
//! The two SQL operations are independent and idempotent — if one fails,
//! the other still runs on the next tick.

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// How often to run the session pruning pass.
const PRUNE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// session_ctx rows older than this are considered stale and removed.
/// A healthy request completes in well under 5 minutes.
const STALE_CTX_THRESHOLD_SECS: i64 = 300;

// ---------------------------------------------------------------------------
// Public API — pruning loop
// ---------------------------------------------------------------------------

/// Runs the authz session pruning loop.
///
/// Respects the cancellation token for graceful shutdown.
/// Errors are logged but do NOT crash the task.
pub async fn run_session_pruning(pool: PgPool, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(PRUNE_INTERVAL);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match do_session_prune(&pool).await {
                    Ok((expired, stale)) => {
                        if expired > 0 || stale > 0 {
                            tracing::info!(
                                expired_sessions = expired,
                                stale_ctx_rows = stale,
                                "session_pruning: completed"
                            );
                        } else {
                            tracing::debug!("session_pruning: nothing to prune");
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "session_pruning: failed");
                    }
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("session_pruning: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Private — single prune pass
// ---------------------------------------------------------------------------

/// Performs one round of session pruning.
///
/// Returns `(expired_sessions_count, stale_ctx_count)` on success.
async fn do_session_prune(pool: &PgPool) -> Result<(u64, u64), sqlx::Error> {
    // 1. Delete expired user_sessions
    let expired = sqlx::query!(
        r#"
        DELETE FROM authz.user_sessions
        WHERE expires_at < now()
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();

    // 2. Delete stale session_ctx rows.
    //    A session_ctx row is stale when:
    //    - The backend PID that created it is no longer connected, OR
    //    - It was set more than STALE_CTX_THRESHOLD_SECS ago (hung query)
    //
    //    pg_stat_activity tracks active PIDs — any session_ctx.pid NOT in
    //    pg_stat_activity is from a disconnected client.
    let stale = sqlx::query!(
        r#"
        DELETE FROM authz.session_ctx
        WHERE pid NOT IN (SELECT pid FROM pg_stat_activity WHERE pid IS NOT NULL)
           OR set_at < now() - make_interval(secs => $1::double precision)
        "#,
        STALE_CTX_THRESHOLD_SECS as f64,
    )
    .execute(pool)
    .await?
    .rows_affected();

    Ok((expired, stale))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_interval_is_reasonable() {
        assert_eq!(PRUNE_INTERVAL.as_secs(), 60);
    }

    #[test]
    fn stale_threshold_is_five_minutes() {
        assert_eq!(STALE_CTX_THRESHOLD_SECS, 300);
    }
}
