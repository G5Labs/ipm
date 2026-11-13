//! Bounded retention for terminal approval-request coordination rows.

use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

#[derive(Debug, Clone)]
pub struct ApprovalRequestRetentionConfig {
    pub interval: Duration,
    pub retention_days: i64,
    pub batch_size: i64,
}

impl Default for ApprovalRequestRetentionConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(24 * 60 * 60),
            retention_days: 30,
            batch_size: 1000,
        }
    }
}

impl ApprovalRequestRetentionConfig {
    fn validate(&self) -> Result<(), ApprovalRequestRetentionSweepError> {
        if self.interval.is_zero() {
            return Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "interval must be nonzero",
            ));
        }
        if self.retention_days < 0 {
            return Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "retention_days must be nonnegative",
            ));
        }
        if self.retention_days > i64::from(i32::MAX) {
            return Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "retention_days must fit PostgreSQL integer range",
            ));
        }
        if self.batch_size <= 0 {
            return Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "batch_size must be positive",
            ));
        }
        Ok(())
    }
}

/// Typed failures that prevent a retention sweep from producing a report.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalRequestRetentionSweepError {
    #[error("invalid approval request retention configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Machine-readable result of one explicitly requested retention sweep.
///
/// The background loop emits the same bounded aggregate as structured logs;
/// this projection lets integration tests and operator tooling assert the
/// exact tenant-isolation and cancellation outcomes without scraping logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequestRetentionSweepReport {
    pub run_id: Uuid,
    pub orgs_attempted: u64,
    pub orgs_completed: u64,
    pub orgs_failed: u64,
    pub orgs_cancelled: u64,
    pub batches: u64,
    pub deleted: u64,
    pub disposition: RetentionDisposition,
}

/// Linked tasks retain their requests; immutable decisions are never deleted here.
const DELETE_BATCH_SQL: &str = r#"
    DELETE FROM approval_requests request
    WHERE request.id IN (
        SELECT candidate.id
        FROM approval_requests candidate
        WHERE candidate.org_id = $1
          AND candidate.state IN ('settled', 'cancelled', 'expired', 'superseded')
          AND candidate.closed_at < now() - make_interval(days => $2::int)
          AND NOT EXISTS (SELECT 1 FROM agent_approval_reviews review WHERE review.org_id=candidate.org_id AND review.approval_request_id=candidate.id)
          AND NOT EXISTS (
                SELECT 1 FROM approval_request_tasks link
                WHERE link.org_id = candidate.org_id AND link.approval_request_id = candidate.id
              )
        ORDER BY candidate.closed_at, candidate.seq, candidate.id
        LIMIT $3
    )
    "#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionDisposition {
    Completed,
    Failed,
    Cancelled,
}

impl RetentionDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetentionFailureStage {
    EnumerateOrganizations,
    Scope,
    Delete,
    Commit,
}

impl RetentionFailureStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::EnumerateOrganizations => "enumerate_organizations",
            Self::Scope => "scope",
            Self::Delete => "delete",
            Self::Commit => "commit",
        }
    }
}

#[derive(Debug, Clone)]
struct OrgRetentionAudit {
    run_id: Uuid,
    org_id: Uuid,
    batches: u64,
    deleted: u64,
    disposition: RetentionDisposition,
    failure_stage: Option<RetentionFailureStage>,
}

impl OrgRetentionAudit {
    fn new(run_id: Uuid, org_id: Uuid) -> Self {
        Self {
            run_id,
            org_id,
            batches: 0,
            deleted: 0,
            disposition: RetentionDisposition::Completed,
            failure_stage: None,
        }
    }

    fn record_batch(&mut self, deleted: u64) {
        self.batches += 1;
        self.deleted += deleted;
    }

    fn complete(&mut self) {
        self.disposition = RetentionDisposition::Completed;
        self.failure_stage = None;
    }

    fn fail(&mut self, stage: RetentionFailureStage) {
        self.disposition = RetentionDisposition::Failed;
        self.failure_stage = Some(stage);
    }

    fn cancel(&mut self) {
        self.disposition = RetentionDisposition::Cancelled;
        self.failure_stage = None;
    }

    fn emit(&self) {
        tracing::info!(
            target: "approval_request_retention",
            run_id = %self.run_id,
            org_id = %self.org_id,
            batches = self.batches,
            deleted = self.deleted,
            disposition = self.disposition.as_str(),
            failure_stage = self.failure_stage.map(RetentionFailureStage::as_str),
            "approval request retention organization audit"
        );
    }
}

#[derive(Debug, Clone)]
struct RetentionSweepAudit {
    run_id: Uuid,
    orgs_attempted: u64,
    orgs_completed: u64,
    orgs_failed: u64,
    orgs_cancelled: u64,
    batches: u64,
    deleted: u64,
    disposition: RetentionDisposition,
    failure_stage: Option<RetentionFailureStage>,
}

impl RetentionSweepAudit {
    fn from_orgs(run_id: Uuid, orgs: &[OrgRetentionAudit], cancelled: bool) -> Self {
        let orgs_failed = orgs
            .iter()
            .filter(|org| org.disposition == RetentionDisposition::Failed)
            .count() as u64;
        let orgs_cancelled = orgs
            .iter()
            .filter(|org| org.disposition == RetentionDisposition::Cancelled)
            .count() as u64;
        let disposition = if orgs_failed > 0 {
            RetentionDisposition::Failed
        } else if cancelled || orgs_cancelled > 0 {
            RetentionDisposition::Cancelled
        } else {
            RetentionDisposition::Completed
        };
        Self {
            run_id,
            orgs_attempted: orgs.len() as u64,
            orgs_completed: orgs
                .iter()
                .filter(|org| org.disposition == RetentionDisposition::Completed)
                .count() as u64,
            orgs_failed,
            orgs_cancelled,
            batches: orgs.iter().map(|org| org.batches).sum(),
            deleted: orgs.iter().map(|org| org.deleted).sum(),
            disposition,
            failure_stage: None,
        }
    }

    fn enumeration_failed(run_id: Uuid) -> Self {
        Self {
            run_id,
            orgs_attempted: 0,
            orgs_completed: 0,
            orgs_failed: 0,
            orgs_cancelled: 0,
            batches: 0,
            deleted: 0,
            disposition: RetentionDisposition::Failed,
            failure_stage: Some(RetentionFailureStage::EnumerateOrganizations),
        }
    }

    fn emit(&self) {
        tracing::info!(
            target: "approval_request_retention",
            run_id = %self.run_id,
            orgs_attempted = self.orgs_attempted,
            orgs_completed = self.orgs_completed,
            orgs_failed = self.orgs_failed,
            orgs_cancelled = self.orgs_cancelled,
            batches = self.batches,
            deleted = self.deleted,
            disposition = self.disposition.as_str(),
            failure_stage = self.failure_stage.map(RetentionFailureStage::as_str),
            "approval request retention sweep audit"
        );
    }
}

pub async fn run_approval_request_retention_loop(
    deps: TenantMaintenanceDeps,
    config: ApprovalRequestRetentionConfig,
    cancel: CancellationToken,
) {
    if let Err(reason) = config.validate() {
        tracing::error!(
            reason = %reason,
            "invalid approval request retention config; loop not started"
        );
        return;
    }
    tracing::info!(
        interval_secs = config.interval.as_secs(),
        retention_days = config.retention_days,
        batch_size = config.batch_size,
        "starting approval request retention loop"
    );
    let mut interval = tokio::time::interval(config.interval);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let _ = run_approval_request_retention_sweep(&deps, &config, &cancel).await;
            }
            _ = cancel.cancelled() => {
                tracing::info!("approval request retention task shutting down");
                break;
            }
        }
    }
}

/// Run one bounded sweep using the production tenant-scoped transactions.
/// Cancellation is checked before every organization and every delete batch.
pub async fn run_approval_request_retention_sweep(
    deps: &TenantMaintenanceDeps,
    config: &ApprovalRequestRetentionConfig,
    cancel: &CancellationToken,
) -> Result<ApprovalRequestRetentionSweepReport, ApprovalRequestRetentionSweepError> {
    config.validate()?;
    let run_id = Uuid::new_v4();
    let org_ids = match maintenance_org_ids(&deps.system_pool).await {
        Ok(org_ids) => org_ids,
        Err(error) => {
            RetentionSweepAudit::enumeration_failed(run_id).emit();
            return Err(error.into());
        }
    };
    let mut org_audits = Vec::new();

    'orgs: for org_id in org_ids {
        if cancel.is_cancelled() {
            break;
        }
        let mut audit = OrgRetentionAudit::new(run_id, org_id);
        loop {
            if cancel.is_cancelled() {
                audit.cancel();
                audit.emit();
                org_audits.push(audit);
                break 'orgs;
            }
            let mut db = match scoped_db_for_maintenance_org(deps, org_id).await {
                Ok(db) => db,
                Err(_) => {
                    audit.fail(RetentionFailureStage::Scope);
                    audit.emit();
                    org_audits.push(audit);
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
                Err(_) => {
                    audit.fail(RetentionFailureStage::Delete);
                    audit.emit();
                    org_audits.push(audit);
                    continue 'orgs;
                }
            };
            let deleted = result.rows_affected();
            if db.commit().await.is_err() {
                audit.fail(RetentionFailureStage::Commit);
                audit.emit();
                org_audits.push(audit);
                continue 'orgs;
            }
            audit.record_batch(deleted);
            if deleted < config.batch_size as u64 {
                audit.complete();
                audit.emit();
                org_audits.push(audit);
                break;
            }
            // A full batch means more work may remain. Yield after its commit
            // so shutdown can be observed at the documented between-batch
            // boundary even when PostgreSQL keeps returning immediately.
            tokio::task::yield_now().await;
        }
    }

    let summary = RetentionSweepAudit::from_orgs(run_id, &org_audits, cancel.is_cancelled());
    summary.emit();
    Ok(ApprovalRequestRetentionSweepReport {
        run_id: summary.run_id,
        orgs_attempted: summary.orgs_attempted,
        orgs_completed: summary.orgs_completed,
        orgs_failed: summary.orgs_failed,
        orgs_cancelled: summary.orgs_cancelled,
        batches: summary.batches,
        deleted: summary.deleted,
        disposition: summary.disposition,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_contract() {
        let config = ApprovalRequestRetentionConfig::default();
        assert_eq!(config.interval, Duration::from_secs(86_400));
        assert_eq!(config.retention_days, 30);
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn invalid_configurations_are_rejected() {
        assert!(ApprovalRequestRetentionConfig::default().validate().is_ok());
        assert!(matches!(
            ApprovalRequestRetentionConfig {
                interval: Duration::ZERO,
                ..ApprovalRequestRetentionConfig::default()
            }
            .validate(),
            Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "interval must be nonzero"
            ))
        ));
        assert!(matches!(
            ApprovalRequestRetentionConfig {
                retention_days: -1,
                ..ApprovalRequestRetentionConfig::default()
            }
            .validate(),
            Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "retention_days must be nonnegative"
            ))
        ));
        assert!(matches!(
            ApprovalRequestRetentionConfig {
                retention_days: i64::from(i32::MAX) + 1,
                ..ApprovalRequestRetentionConfig::default()
            }
            .validate(),
            Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "retention_days must fit PostgreSQL integer range"
            ))
        ));
        assert!(matches!(
            ApprovalRequestRetentionConfig {
                batch_size: 0,
                ..ApprovalRequestRetentionConfig::default()
            }
            .validate(),
            Err(ApprovalRequestRetentionSweepError::InvalidConfiguration(
                "batch_size must be positive"
            ))
        ));
    }

    #[test]
    fn destructive_predicate_retains_pending_and_task_referenced_requests() {
        for required in [
            "candidate.org_id = $1",
            "candidate.state IN ('settled', 'cancelled', 'expired', 'superseded')",
            "candidate.closed_at < now() - make_interval(days => $2::int)",
            "FROM approval_request_tasks link",
            "ORDER BY candidate.closed_at, candidate.seq, candidate.id",
            "LIMIT $3",
        ] {
            assert!(
                DELETE_BATCH_SQL.contains(required),
                "missing clause: {required}"
            );
        }
        assert!(!DELETE_BATCH_SQL.contains("DELETE FROM approval_statuses"));
        assert!(!DELETE_BATCH_SQL.contains("FROM approval_statuses"));
        assert!(!DELETE_BATCH_SQL.contains("candidate.state = 'pending'"));
    }

    #[test]
    fn sweep_audit_aggregates_one_bounded_record_per_org() {
        let run_id = Uuid::new_v4();
        let mut completed = OrgRetentionAudit::new(run_id, Uuid::new_v4());
        completed.record_batch(100);
        completed.record_batch(7);
        completed.complete();
        let mut failed = OrgRetentionAudit::new(run_id, Uuid::new_v4());
        failed.fail(RetentionFailureStage::Commit);

        let summary = RetentionSweepAudit::from_orgs(run_id, &[completed, failed], false);
        assert_eq!(summary.orgs_attempted, 2);
        assert_eq!(summary.orgs_completed, 1);
        assert_eq!(summary.orgs_failed, 1);
        assert_eq!(summary.orgs_cancelled, 0);
        assert_eq!(summary.batches, 2);
        assert_eq!(summary.deleted, 107);
        assert_eq!(summary.disposition, RetentionDisposition::Failed);
    }
}
