//! Non-authoritative, expiring remote-tip observations shared by the API and
//! maintenance roles through the `remote_tip_observations` table.
//!
//! One row per remote integration, overwritten on every observation and
//! cascading with the policy row. Observations are hints: a reader ignores a
//! row older than the TTL or bound to another remote URL or branch, and
//! nothing here ever authorizes a push, import or conflict resolution. Every
//! caller already holds a tenant-scoped connection, so the hint rides the
//! same transaction as the operation that produced it.

use super::RemoteIntegration;
use chrono::{DateTime, Utc};
use g5_contracts::G5Error;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

/// How long an observation counts as current.
const TTL_SECONDS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTipObservation {
    pub oid: String,
    pub observed_at: DateTime<Utc>,
}

/// The current observation for `integration`: younger than the TTL, not
/// stamped in the future (a clock step backward must not keep a hint alive
/// past its TTL) and bound to the same remote URL and branch. A rebound,
/// expired or future-stamped row reads as no observation, which readers
/// treat as "behind".
pub async fn observed_tip(
    conn: &mut PgConnection,
    integration: &RemoteIntegration,
) -> Result<Option<RemoteTipObservation>, G5Error> {
    let row: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT oid, observed_at FROM remote_tip_observations
          WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3
            AND git_url = $4 AND remote_branch = $5
            AND observed_at > clock_timestamp() - make_interval(secs => $6)
            AND observed_at <= clock_timestamp()",
    )
    .bind(integration.workspace_id)
    .bind(integration.repo_id)
    .bind(integration.org_id)
    .bind(&integration.git_url)
    .bind(&integration.remote_branch)
    .bind(TTL_SECONDS as f64)
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(|(oid, observed_at)| {
        // Stored rows never hold the zero OID (`observe_tip` refuses it), so
        // this only guards against hand-edited rows.
        let is_zero = g5_scm::object_id_is_zero(&oid).ok()?;
        (!is_zero).then_some(RemoteTipObservation { oid, observed_at })
    }))
}

/// The current observation as a hint that may be missing: the read runs
/// under a savepoint, so a failed statement (a rollout whose migration has
/// not applied yet, a transient cancellation) neither aborts the caller's
/// transaction nor fails the caller's operation; it reads as no hint and is
/// logged. Every reader that treats the hint as optional uses this.
pub async fn observed_tip_or_none(
    conn: &mut PgConnection,
    integration: &RemoteIntegration,
) -> Option<RemoteTipObservation> {
    if let Err(error) = sqlx::query("SAVEPOINT g5_remote_tip_hint")
        .execute(&mut *conn)
        .await
    {
        tracing::warn!(%error, "Remote tip hint savepoint unavailable");
        return None;
    }
    match observed_tip(conn, integration).await {
        Ok(observation) => {
            if let Err(error) = sqlx::query("RELEASE SAVEPOINT g5_remote_tip_hint")
                .execute(&mut *conn)
                .await
            {
                tracing::warn!(%error, "Remote tip hint savepoint release failed");
            }
            observation
        }
        Err(error) => {
            tracing::warn!(%error, "Remote tip hint unavailable");
            // `ROLLBACK TO` keeps the savepoint open; release it, or a caller
            // that reads one hint per member in one transaction keeps one open
            // subtransaction per failed read.
            match sqlx::query("ROLLBACK TO SAVEPOINT g5_remote_tip_hint")
                .execute(&mut *conn)
                .await
            {
                Ok(_) => {
                    if let Err(error) = sqlx::query("RELEASE SAVEPOINT g5_remote_tip_hint")
                        .execute(&mut *conn)
                        .await
                    {
                        tracing::warn!(%error, "Remote tip hint savepoint release failed");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Remote tip hint savepoint rollback failed");
                }
            }
            None
        }
    }
}

/// Record `oid` as the tip observed for `integration`. Call only after
/// revalidating the policy following transport, inside the caller's
/// transaction. Refreshes the timestamp even when the tip is unchanged and
/// reports whether the observation changed: no current previous row (none, or
/// one older than the TTL), or a different OID, URL or branch.
///
/// Timestamps come from `clock_timestamp()`, the wall clock after the lock
/// wait, not the transaction start: an observer that waited across the TTL
/// boundary must classify the old row as expired and stamp its own row with a
/// time that is current when it commits.
///
/// Observers of one integration serialize on its `remote_integrations` row:
/// a missing observation row cannot be locked, so without the parent lock two
/// first observations would both report a change. The pinger already holds
/// that row (`load(.., for_update = true)`) when it observes, so every writer
/// takes locks in the same order (policy row, then observation row) and the
/// worker can never hold the observation row while waiting for the policy row
/// the pinger holds.
pub async fn observe_tip(
    conn: &mut PgConnection,
    integration: &RemoteIntegration,
    oid: &str,
) -> Result<bool, G5Error> {
    if g5_scm::object_id_is_zero(oid)? {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({"message": "Remote tip must be a commit OID"}),
        });
    }
    sqlx::query(
        "SELECT 1 FROM remote_integrations
          WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3
          FOR UPDATE",
    )
    .bind(integration.workspace_id)
    .bind(integration.repo_id)
    .bind(integration.org_id)
    .execute(&mut *conn)
    .await?;
    let previous: Option<(String, String, String, bool)> = sqlx::query_as(
        "SELECT oid, git_url, remote_branch,
                observed_at > clock_timestamp() - make_interval(secs => $4)
                    AND observed_at <= clock_timestamp() AS current
           FROM remote_tip_observations
          WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3
          FOR UPDATE",
    )
    .bind(integration.workspace_id)
    .bind(integration.repo_id)
    .bind(integration.org_id)
    .bind(TTL_SECONDS as f64)
    .fetch_optional(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO remote_tip_observations
            (org_id, workspace_id, repo_id, git_url, remote_branch, oid, observed_at)
         VALUES ($1, $2, $3, $4, $5, $6, clock_timestamp())
         ON CONFLICT (workspace_id, repo_id) DO UPDATE
            SET git_url = EXCLUDED.git_url,
                remote_branch = EXCLUDED.remote_branch,
                oid = EXCLUDED.oid,
                observed_at = clock_timestamp()",
    )
    .bind(integration.org_id)
    .bind(integration.workspace_id)
    .bind(integration.repo_id)
    .bind(&integration.git_url)
    .bind(&integration.remote_branch)
    .bind(oid)
    .execute(&mut *conn)
    .await?;
    Ok(
        previous.is_none_or(|(old_oid, old_url, old_branch, current)| {
            !current
                || old_oid != oid
                || old_url != integration.git_url
                || old_branch != integration.remote_branch
        }),
    )
}
