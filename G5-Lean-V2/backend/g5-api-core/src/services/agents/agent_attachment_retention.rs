//! Retention for agent-produced screenshot attachments.
//!
//! Agent-origin chat attachments (chat_service D63: the reserved
//! `agent-screenshots/` storage-key prefix) are timeline evidence, not user
//! content: they accumulate per browser tool call and their bytes live in
//! Postgres, so they age out on a fixed horizon. User-origin attachments are
//! never touched — the prefix predicate IS the origin marker. A timeline
//! entry whose `imageArtifacts` reference has aged out renders as a hidden
//! thumbnail client-side; the textual tool result remains.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

#[derive(Debug, Clone)]
pub struct AgentAttachmentRetentionConfig {
    pub interval: Duration,
    pub retention_days: i64,
    pub batch_size: i64,
}

impl Default for AgentAttachmentRetentionConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(24 * 60 * 60),
            retention_days: 30,
            batch_size: 500,
        }
    }
}

impl AgentAttachmentRetentionConfig {
    /// A zero interval panics tokio's timer, a zero batch size can never
    /// terminate the batch loop, and a negative horizon moves the cutoff into
    /// the future and deletes fresh evidence.
    fn validate(&self) -> Result<(), &'static str> {
        if self.interval.is_zero() {
            return Err("interval must be nonzero");
        }
        if self.retention_days < 0 {
            return Err("retention_days must be nonnegative");
        }
        // DELETE_BATCH_SQL casts $2 to Postgres `int`; an oversized horizon
        // would fail every batch and quietly stop retention (same bound as
        // ApprovalRequestRetentionConfig).
        if self.retention_days > i64::from(i32::MAX) {
            return Err("retention_days must fit in a Postgres integer");
        }
        if self.batch_size <= 0 {
            return Err("batch_size must be positive");
        }
        Ok(())
    }
}

/// One bounded, tenant-scoped delete batch. Kept as a named constant so the
/// destructive predicate is pinned by source-text regression tests: it must
/// only ever match the reserved agent-origin storage-key prefix.
const DELETE_BATCH_SQL: &str = r#"
    DELETE FROM chat_attachments a
    WHERE a.id IN (
        SELECT candidate.id
        FROM chat_attachments candidate
        WHERE candidate.org_id = $1
          AND candidate.storage_key LIKE 'agent-screenshots/%'
          AND candidate.created_at < now() - make_interval(days => $2::int)
        ORDER BY candidate.created_at, candidate.id
        LIMIT $3
    )
    "#;

pub async fn run_agent_attachment_retention_loop(
    deps: TenantMaintenanceDeps,
    config: AgentAttachmentRetentionConfig,
    cancel: CancellationToken,
) {
    if let Err(reason) = config.validate() {
        tracing::error!(
            reason,
            interval_secs = config.interval.as_secs(),
            retention_days = config.retention_days,
            batch_size = config.batch_size,
            "invalid agent attachment retention config; retention loop not started"
        );
        return;
    }
    tracing::info!(
        interval_secs = config.interval.as_secs(),
        retention_days = config.retention_days,
        batch_size = config.batch_size,
        "starting agent attachment retention loop"
    );
    let mut interval = tokio::time::interval(config.interval);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                match sweep(&deps, &config, &cancel).await {
                    Ok(deleted) => tracing::info!(deleted, "agent attachment retention complete"),
                    Err(error) => tracing::error!(%error, "agent attachment retention failed"),
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("agent attachment retention task shutting down");
                break;
            }
        }
    }
}

async fn sweep(
    deps: &TenantMaintenanceDeps,
    config: &AgentAttachmentRetentionConfig,
    cancel: &CancellationToken,
) -> Result<u64, sqlx::Error> {
    let org_ids = maintenance_org_ids(&deps.system_pool).await?;
    let mut total = 0;
    'orgs: for org_id in org_ids {
        loop {
            if cancel.is_cancelled() {
                tracing::info!(total, "agent attachment retention cancelled mid-sweep");
                break 'orgs;
            }
            // Each batch runs in its own tenant transaction so a large
            // backlog never holds one long-running transaction open.
            let mut db = match scoped_db_for_maintenance_org(deps, org_id).await {
                Ok(db) => db,
                Err(error) => {
                    tracing::warn!(%org_id, %error, "failed to scope agent attachment retention org");
                    continue 'orgs;
                }
            };
            let result = match sqlx::query(DELETE_BATCH_SQL)
                .bind(org_id)
                .bind(config.retention_days)
                .bind(config.batch_size)
                .execute(db.conn())
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    tracing::warn!(%org_id, %error, "agent attachment retention delete batch failed");
                    continue 'orgs;
                }
            };
            let deleted = result.rows_affected();
            if let Err(error) = db.commit().await {
                tracing::warn!(%org_id, %error, "failed to commit agent attachment retention batch");
                continue 'orgs;
            }
            total += deleted;
            if deleted < config.batch_size as u64 {
                break;
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_predicate_is_pinned_to_the_agent_origin_prefix() {
        // The destructive predicate must only ever match agent-origin rows
        // (chat_service D63): user uploads are never retention candidates.
        assert!(DELETE_BATCH_SQL.contains("storage_key LIKE 'agent-screenshots/%'"));
        assert!(DELETE_BATCH_SQL.contains("candidate.org_id = $1"));
        assert!(DELETE_BATCH_SQL.contains("LIMIT $3"));
    }

    #[test]
    fn config_validation_rejects_degenerate_values() {
        let valid = AgentAttachmentRetentionConfig::default();
        assert!(valid.validate().is_ok());
        assert!(AgentAttachmentRetentionConfig {
            interval: Duration::ZERO,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(AgentAttachmentRetentionConfig {
            retention_days: -1,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(AgentAttachmentRetentionConfig {
            retention_days: i64::from(i32::MAX) + 1,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(AgentAttachmentRetentionConfig {
            batch_size: 0,
            ..valid
        }
        .validate()
        .is_err());
    }
}
