//! The delivery worker: claims due requests one at a time under a lease,
//! asks the consumer whether each is still wanted, sends it and records the
//! outcome (`g5.email.handoff`, `.validity`, `.retry`, `.ambiguity`).

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rand::Rng;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use super::store::{self, Claimed, Completion};
use super::transport::{Outgoing, SendOutcome, Transport};
use super::{EmailGates, PurposeKind, Validity, LOG_TARGET};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const BATCH_SIZE: usize = 10;
/// Covers the gate check and one provider call (30 s timeout).
const LEASE: Duration = Duration::from_secs(120);
const BACKOFF_BASE: Duration = Duration::from_secs(30);
const BACKOFF_CAP: Duration = Duration::from_secs(30 * 60);
/// Inside the provider's 24-hour idempotency window.
const RETRY_WINDOW: chrono::Duration = chrono::Duration::hours(23);
const RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const RETENTION_EVERY: u32 = 120;

/// Outcome counts for one batch.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EmailBatchResult {
    pub claimed: usize,
    pub sent: usize,
    pub retried: usize,
    pub failed: usize,
    pub cancelled: usize,
    /// Completions refused because another worker reclaimed the row.
    pub lost: usize,
}

pub struct EmailWorker {
    pool: PgPool,
    transport: Arc<dyn Transport>,
    gates: EmailGates,
}

impl EmailWorker {
    pub(super) fn new(pool: PgPool, transport: Arc<dyn Transport>, gates: EmailGates) -> Self {
        Self {
            pool,
            transport,
            gates,
        }
    }

    /// Poll until cancelled.
    pub async fn run(self, cancel: CancellationToken) {
        let mut ticks: u32 = 0;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!(target: LOG_TARGET, "Email delivery worker shutting down");
                    break;
                }
                _ = tokio::time::sleep(POLL_INTERVAL) => {
                    match self.process_due(BATCH_SIZE).await {
                        Ok(result) if result.claimed > 0 => tracing::info!(
                            target: LOG_TARGET,
                            claimed = result.claimed,
                            sent = result.sent,
                            retried = result.retried,
                            failed = result.failed,
                            cancelled = result.cancelled,
                            "Email delivery batch processed"
                        ),
                        Ok(_) => {}
                        Err(error) => tracing::error!(
                            target: LOG_TARGET, %error, "Email delivery batch failed"
                        ),
                    }
                    ticks = ticks.wrapping_add(1);
                    if ticks.is_multiple_of(RETENTION_EVERY) {
                        self.purge_ended().await;
                    }
                }
            }
        }
    }

    /// Delete ended requests past the retention period (`g5.email.data`).
    pub async fn purge_ended(&self) {
        match store::delete_ended_before(&self.pool, RETENTION).await {
            Ok(0) => {}
            Ok(deleted) => {
                tracing::info!(target: LOG_TARGET, deleted, "Deleted ended email deliveries")
            }
            Err(error) => {
                tracing::error!(target: LOG_TARGET, %error, "Email delivery retention failed")
            }
        }
    }

    /// Claim and process up to `limit` due requests, one at a time.
    pub async fn process_due(&self, limit: usize) -> Result<EmailBatchResult, sqlx::Error> {
        let mut result = EmailBatchResult::default();
        for _ in 0..limit {
            let Some(claimed) = store::claim_one(&self.pool, LEASE).await? else {
                break;
            };
            result.claimed += 1;
            match self.attempt(&claimed).await? {
                Some(Tally::Sent) => result.sent += 1,
                Some(Tally::Retried) => result.retried += 1,
                Some(Tally::Failed) => result.failed += 1,
                Some(Tally::Cancelled) => result.cancelled += 1,
                None => result.lost += 1,
            }
        }
        Ok(result)
    }

    async fn attempt(&self, claimed: &Claimed) -> Result<Option<Tally>, sqlx::Error> {
        let Some(kind) = PurposeKind::parse(&claimed.purpose_kind) else {
            // Only after a rollback to a build that predates the kind.
            return self
                .finish(
                    claimed,
                    Completion::Failed {
                        reason: "unknown_kind",
                        last_error: None,
                        uncertain: false,
                    },
                    Tally::Failed,
                )
                .await;
        };
        if Utc::now() > claimed.first_attempt_at + RETRY_WINDOW {
            // A retry claimed late (worker outage or backlog) would fall outside
            // the provider's idempotency window: end it instead of sending.
            tracing::error!(
                target: LOG_TARGET,
                delivery_id = %claimed.id,
                purpose_kind = kind.as_str(),
                org_id = %claimed.org_id,
                attempt = claimed.attempts,
                outcome_uncertain = claimed.outcome_uncertain,
                "Email delivery failed: retry window exhausted before the attempt"
            );
            return self
                .finish(
                    claimed,
                    Completion::Failed {
                        reason: "retry_window",
                        last_error: None,
                        uncertain: false,
                    },
                    Tally::Failed,
                )
                .await;
        }
        if claimed.reclaimed {
            tracing::warn!(
                target: LOG_TARGET,
                delivery_id = %claimed.id,
                attempt = claimed.attempts,
                "Email delivery lease expired; retrying with the same identity"
            );
        }

        match self
            .gates
            .for_kind(kind)
            .still_wanted(&self.pool, &claimed.purpose_key, &claimed.recipient)
            .await
        {
            Ok(Validity::Wanted) => {}
            Err(error) => {
                // Nothing was sent: put the request back without marking it
                // uncertain, instead of leaving it to an expired lease.
                tracing::error!(
                    target: LOG_TARGET,
                    delivery_id = %claimed.id,
                    %error,
                    "Email validity check failed; will retry"
                );
                return self
                    .finish(
                        claimed,
                        Completion::Retry {
                            delay: retry_delay(claimed.attempts, None),
                            last_error: "validity_check",
                            uncertain: false,
                        },
                        Tally::Retried,
                    )
                    .await;
            }
            Ok(Validity::Cancelled(reason)) => {
                tracing::info!(
                    target: LOG_TARGET,
                    delivery_id = %claimed.id,
                    purpose_kind = kind.as_str(),
                    purpose_key = %claimed.purpose_key,
                    reason,
                    "Email delivery cancelled; no longer wanted"
                );
                return self
                    .finish(claimed, Completion::Cancelled { reason }, Tally::Cancelled)
                    .await;
            }
        }

        let (Some(subject), Some(text), Some(html)) = (
            claimed.subject.as_deref(),
            claimed.text_body.as_deref(),
            claimed.html_body.as_deref(),
        ) else {
            return self
                .finish(
                    claimed,
                    Completion::Failed {
                        reason: "missing_content",
                        last_error: None,
                        uncertain: false,
                    },
                    Tally::Failed,
                )
                .await;
        };
        let outcome = self
            .transport
            .send(&Outgoing {
                delivery_id: claimed.id,
                sender: &claimed.sender,
                recipient: &claimed.recipient,
                subject,
                text,
                html,
            })
            .await;

        match outcome {
            SendOutcome::Accepted {
                provider_message_id,
            } => {
                tracing::info!(
                    target: LOG_TARGET,
                    delivery_id = %claimed.id,
                    purpose_kind = kind.as_str(),
                    org_id = %claimed.org_id,
                    attempt = claimed.attempts,
                    provider_message_id = provider_message_id.as_deref().unwrap_or(""),
                    "Email accepted by provider"
                );
                self.finish(
                    claimed,
                    Completion::Sent {
                        provider_message_id: provider_message_id.as_deref(),
                    },
                    Tally::Sent,
                )
                .await
            }
            SendOutcome::Rejected { code } => {
                tracing::error!(
                    target: LOG_TARGET,
                    delivery_id = %claimed.id,
                    purpose_kind = kind.as_str(),
                    org_id = %claimed.org_id,
                    attempt = claimed.attempts,
                    error = %code,
                    "Email permanently rejected by provider"
                );
                self.finish(
                    claimed,
                    Completion::Failed {
                        reason: "rejected",
                        last_error: Some(&code),
                        uncertain: false,
                    },
                    Tally::Failed,
                )
                .await
            }
            SendOutcome::Retry {
                code,
                uncertain,
                configuration,
                retry_after,
            } => {
                let delay = retry_delay(claimed.attempts, retry_after);
                let deadline = claimed.first_attempt_at + RETRY_WINDOW;
                let next = Utc::now()
                    + chrono::Duration::from_std(delay).unwrap_or(chrono::Duration::zero());
                if next > deadline {
                    tracing::error!(
                        target: LOG_TARGET,
                        delivery_id = %claimed.id,
                        purpose_kind = kind.as_str(),
                        org_id = %claimed.org_id,
                        attempt = claimed.attempts,
                        error = %code,
                        outcome_uncertain = claimed.outcome_uncertain || uncertain,
                        "Email delivery failed: retry window exhausted"
                    );
                    return self
                        .finish(
                            claimed,
                            Completion::Failed {
                                reason: "retry_window",
                                last_error: Some(&code),
                                uncertain,
                            },
                            Tally::Failed,
                        )
                        .await;
                }
                if configuration {
                    tracing::error!(
                        target: LOG_TARGET,
                        delivery_id = %claimed.id,
                        attempt = claimed.attempts,
                        error = %code,
                        retry_in_secs = delay.as_secs(),
                        "Email provider refused G5's credentials or sender; check the email configuration"
                    );
                } else {
                    tracing::warn!(
                        target: LOG_TARGET,
                        delivery_id = %claimed.id,
                        attempt = claimed.attempts,
                        error = %code,
                        outcome_uncertain = uncertain,
                        retry_in_secs = delay.as_secs(),
                        "Email delivery attempt failed; will retry"
                    );
                }
                self.finish(
                    claimed,
                    Completion::Retry {
                        delay,
                        last_error: &code,
                        uncertain,
                    },
                    Tally::Retried,
                )
                .await
            }
        }
    }

    async fn finish(
        &self,
        claimed: &Claimed,
        completion: Completion<'_>,
        tally: Tally,
    ) -> Result<Option<Tally>, sqlx::Error> {
        if store::complete(&self.pool, claimed, completion).await? {
            Ok(Some(tally))
        } else {
            tracing::warn!(
                target: LOG_TARGET,
                delivery_id = %claimed.id,
                attempt = claimed.attempts,
                "Email delivery was reclaimed by another worker; outcome not recorded"
            );
            Ok(None)
        }
    }
}

#[derive(Clone, Copy)]
enum Tally {
    Sent,
    Retried,
    Failed,
    Cancelled,
}

/// Capped exponential backoff with ±20% jitter; a provider `Retry-After` is
/// honored up to the cap.
fn retry_delay(attempts: i32, retry_after: Option<Duration>) -> Duration {
    let exponent = u32::try_from(attempts.saturating_sub(1).clamp(0, 16)).unwrap_or(16);
    let base = BACKOFF_BASE
        .saturating_mul(2u32.saturating_pow(exponent))
        .min(BACKOFF_CAP);
    let jitter = rand::thread_rng().gen_range(0.8..=1.2);
    let jittered = base.mul_f64(jitter).min(BACKOFF_CAP);
    match retry_after {
        Some(provider) => provider.min(BACKOFF_CAP),
        None => jittered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_is_capped_and_honors_retry_after() {
        let first = retry_delay(1, None);
        assert!(first >= Duration::from_secs(24) && first <= Duration::from_secs(36));
        let fourth = retry_delay(4, None);
        assert!(fourth >= Duration::from_secs(192) && fourth <= Duration::from_secs(288));
        assert!(retry_delay(40, None) <= BACKOFF_CAP);
        assert_eq!(
            retry_delay(1, Some(Duration::from_secs(7))),
            Duration::from_secs(7)
        );
        assert_eq!(
            retry_delay(1, Some(Duration::from_secs(10 * 60 * 60))),
            BACKOFF_CAP
        );
    }
}
