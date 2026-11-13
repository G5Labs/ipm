//! Invitation garbage collection background task.
//!
//! Periodically expires pending org invitations whose `expires_at` timestamp
//! has passed, and permanently deletes old expired/declined invitations after
//! a 90-day retention period.
//!
//! This task is spawned by `MaintenanceRunner::spawn_all()` in `mod.rs` and
//! runs on a configurable interval (default: hourly, via
//! `MaintenanceConfig.invitation_gc_interval`).
//!
//! See `designs/API_AND_STORAGE.md` §3.12.2 for the invitation lifecycle
//! state machine and GC rules.

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::config::MaintenanceConfig;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Expired and declined invitations are permanently deleted after this many days.
/// Matches API_AND_STORAGE.md §3.12.2: "retained for 90 days (audit trail), then deleted."
const RETENTION_DAYS: i32 = 90;

// ---------------------------------------------------------------------------
// Public API — GC loop
// ---------------------------------------------------------------------------

/// Runs the invitation garbage collection loop.
///
/// This task periodically:
/// 1. Expires pending invitations whose `expires_at` has passed
/// 2. Deletes expired/declined invitations older than 90 days
///
/// The loop respects the cancellation token for graceful shutdown.
/// Errors are logged but do NOT crash the task — the loop continues
/// on the next interval tick.
pub async fn run_invitation_gc(pool: PgPool, config: MaintenanceConfig, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(config.invitation_gc_interval);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match do_invitation_gc(&pool).await {
                    Ok((expired, deleted)) => {
                        if expired > 0 || deleted > 0 {
                            tracing::info!(
                                expired_count = expired,
                                deleted_count = deleted,
                                "invitation_gc: completed"
                            );
                        } else {
                            tracing::debug!("invitation_gc: no invitations to process");
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "invitation_gc: failed");
                    }
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("invitation_gc: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Private — single GC pass
// ---------------------------------------------------------------------------

/// Performs one round of invitation garbage collection.
///
/// Returns `(expired_count, deleted_count)` on success.
///
/// The two SQL operations are independent and idempotent — they are NOT
/// wrapped in a transaction. If step 1 succeeds but step 2 fails, the
/// expired invitations from step 1 are still correctly transitioned.
async fn do_invitation_gc(pool: &PgPool) -> Result<(u64, u64), sqlx::Error> {
    // Step 1: Expire pending invitations whose expiry time has passed.
    //
    // Transitions pending → expired. Sets `updated_at = now()` so the
    // 90-day retention clock starts from the moment of expiration, not
    // from the original creation time.
    //
    // Uses the `idx_invitation_gc` partial index:
    //   CREATE INDEX idx_invitation_gc ON org_invitations(expires_at)
    //       WHERE status = 'pending';
    let expired_result = sqlx::query!(
        r#"
        UPDATE org_invitations
        SET status = 'expired', updated_at = now()
        WHERE status = 'pending'
          AND expires_at < now()
        "#
    )
    .execute(pool)
    .await?;

    let expired_count = expired_result.rows_affected();

    // Step 2: Delete old expired/declined invitations past the retention period.
    //
    // Permanently removes invitations that have been in a terminal state
    // (expired or declined) for more than 90 days. Accepted invitations
    // are NEVER deleted — they serve as a permanent record of how a user
    // joined the org.
    //
    // Uses `make_interval(days => $1)` with a bind parameter for the
    // retention period rather than string interpolation.
    let deleted_result = sqlx::query!(
        r#"
        DELETE FROM org_invitations
        WHERE status IN ('expired', 'declined')
          AND updated_at < now() - make_interval(days => $1)
        "#,
        RETENTION_DAYS
    )
    .execute(pool)
    .await?;

    let deleted_count = deleted_result.rows_affected();

    Ok((expired_count, deleted_count))
}
