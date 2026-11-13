//! Current authority for bounded stream delivery. Never grants a future lease.
use g5_contracts::error::G5Error;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

/// Membership activity queries this process ran (tests observe the
/// per-batch recheck bound through it).
#[cfg(feature = "test-helpers")]
pub static ACTIVITY_CHECKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone)]
pub struct MembershipActivityGuard {
    pool: PgPool,
    org_id: Uuid,
    user_id: Uuid,
    revision: i64,
}

impl MembershipActivityGuard {
    /// Captured identity for combining a fresh membership check with resource
    /// predicates in one query. These values are not cached authorization.
    pub(crate) fn binding(&self) -> (Uuid, Uuid, i64) {
        (self.org_id, self.user_id, self.revision)
    }

    pub async fn capture_all(
        pool: &PgPool,
        user_id: Uuid,
    ) -> Result<std::collections::HashMap<Uuid, Self>, G5Error> {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "SELECT org_id, activity_revision FROM org_memberships WHERE user_id = $1 AND deactivated_at IS NULL")
            .bind(user_id).fetch_all(pool).await?;
        Ok(rows
            .into_iter()
            .map(|(org_id, revision)| {
                (
                    org_id,
                    Self {
                        pool: pool.clone(),
                        org_id,
                        user_id,
                        revision,
                    },
                )
            })
            .collect())
    }

    pub async fn capture(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> Result<Self, G5Error> {
        let revision = sqlx::query_scalar::<_, i64>(
            "SELECT activity_revision FROM org_memberships
             WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| G5Error::Forbidden {
            missing_scopes: Vec::new(),
        })?;
        Ok(Self {
            pool: pool.clone(),
            org_id,
            user_id,
            revision,
        })
    }

    pub async fn authorize(&self) -> bool {
        #[cfg(feature = "test-helpers")]
        ACTIVITY_CHECKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        matches!(
            tokio::time::timeout(
                Duration::from_secs(5),
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (SELECT 1 FROM org_memberships WHERE org_id = $1
                 AND user_id = $2 AND deactivated_at IS NULL AND activity_revision = $3)"
                )
                .bind(self.org_id)
                .bind(self.user_id)
                .bind(self.revision)
                .fetch_one(&self.pool)
            )
            .await,
            Ok(Ok(true))
        )
    }
}
