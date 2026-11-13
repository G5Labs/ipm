//! `email_deliveries` access. Only this module reads or writes the table
//! (`g5.email.data`).

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::render::Rendered;
use super::{DeliveryState, DeliveryStatus, PurposeKind};

/// Statuses of a live request; the unique index shares this predicate.
const LIVE: &str = "status IN ('queued', 'sending')";

pub(super) struct NewDelivery<'a> {
    pub(super) id: Uuid,
    pub(super) org_id: Uuid,
    pub(super) kind: PurposeKind,
    pub(super) key: &'a str,
    pub(super) sender: &'a str,
    pub(super) recipient: &'a str,
    pub(super) rendered: &'a Rendered,
}

/// Insert a new request, or make the live one for the same purpose due now.
/// Returns the row id and whether it was inserted.
pub(super) async fn insert_or_coalesce(
    conn: &mut PgConnection,
    new: NewDelivery<'_>,
) -> Result<(Uuid, bool), sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO email_deliveries (
             id, org_id, purpose_kind, purpose_key, sender, recipient,
             subject, text_body, html_body, status, next_attempt_at
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'queued', now())
         ON CONFLICT (purpose_kind, purpose_key) WHERE {LIVE}
         DO UPDATE SET next_attempt_at = LEAST(email_deliveries.next_attempt_at, now()),
                       updated_at = now()
         RETURNING id, (xmax = 0) AS inserted"
    ))
    .bind(new.id)
    .bind(new.org_id)
    .bind(new.kind.as_str())
    .bind(new.key)
    .bind(new.sender)
    .bind(new.recipient)
    .bind(&new.rendered.subject)
    .bind(&new.rendered.text)
    .bind(&new.rendered.html)
    .fetch_one(conn)
    .await
}

/// Serialize volume-limited requests of one organization and kind until the
/// caller's transaction ends, so concurrent requests are counted one at a time.
pub(super) async fn lock_volume(
    conn: &mut PgConnection,
    org_id: Uuid,
    kind: PurposeKind,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("g5.email.volume:{org_id}:{}", kind.as_str()))
        .execute(conn)
        .await
        .map(|_| ())
}

/// Any request for this purpose, live or ended, still retained.
pub(super) async fn find_by_purpose(
    conn: &mut PgConnection,
    kind: PurposeKind,
    key: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM email_deliveries
          WHERE purpose_kind = $1 AND purpose_key = $2
          ORDER BY created_at DESC, id DESC
          LIMIT 1",
    )
    .bind(kind.as_str())
    .bind(key)
    .fetch_optional(conn)
    .await
}

/// Make the live request for this purpose due now, if one exists.
pub(super) async fn coalesce_live(
    conn: &mut PgConnection,
    kind: PurposeKind,
    key: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "UPDATE email_deliveries
            SET next_attempt_at = LEAST(next_attempt_at, now()), updated_at = now()
          WHERE purpose_kind = $1 AND purpose_key = $2 AND {LIVE}
          RETURNING id"
    ))
    .bind(kind.as_str())
    .bind(key)
    .fetch_optional(conn)
    .await
}

/// When the organization (or, given `recipient`, that recipient within it)
/// already had `limit` requests of this kind in the last 24 hours, how long
/// until the oldest of them leaves the window.
pub(super) async fn volume_exceeded(
    conn: &mut PgConnection,
    org_id: Uuid,
    kind: PurposeKind,
    recipient: Option<&str>,
    limit: u32,
) -> Result<Option<Duration>, sqlx::Error> {
    let (count, retry_after_secs): (i64, Option<f64>) = sqlx::query_as(
        "SELECT count(*),
                extract(epoch FROM (min(created_at) + interval '24 hours' - now()))::double precision
           FROM email_deliveries
          WHERE org_id = $1 AND purpose_kind = $2
            AND created_at > now() - interval '24 hours'
            AND ($3::text IS NULL OR recipient = $3)",
    )
    .bind(org_id)
    .bind(kind.as_str())
    .bind(recipient)
    .fetch_one(conn)
    .await?;
    Ok((count >= i64::from(limit))
        .then(|| Duration::from_secs(retry_after_secs.unwrap_or(1.0).ceil().max(1.0) as u64)))
}

#[derive(sqlx::FromRow)]
struct StatusRow {
    purpose_key: String,
    id: Uuid,
    status: String,
    outcome_uncertain: bool,
    terminal_reason: Option<String>,
    attempts: i32,
    updated_at: DateTime<Utc>,
}

pub(super) async fn latest_status(
    pool: &PgPool,
    org_id: Uuid,
    kind: PurposeKind,
    keys: &[String],
) -> Result<HashMap<String, DeliveryStatus>, sqlx::Error> {
    let rows: Vec<StatusRow> = sqlx::query_as(
        "SELECT DISTINCT ON (purpose_key)
                purpose_key, id, status, outcome_uncertain, terminal_reason,
                attempts, updated_at
           FROM email_deliveries
          WHERE org_id = $1 AND purpose_kind = $2 AND purpose_key = ANY($3)
          ORDER BY purpose_key, created_at DESC, id DESC",
    )
    .bind(org_id)
    .bind(kind.as_str())
    .bind(keys)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.purpose_key,
                DeliveryStatus {
                    id: row.id,
                    state: DeliveryState::parse(&row.status),
                    outcome_uncertain: row.outcome_uncertain,
                    terminal_reason: row.terminal_reason,
                    attempts: row.attempts,
                    updated_at: row.updated_at,
                },
            )
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

/// A claimed request. `attempts` is the claim's fence.
#[derive(sqlx::FromRow)]
pub(super) struct Claimed {
    pub(super) id: Uuid,
    pub(super) org_id: Uuid,
    pub(super) purpose_kind: String,
    pub(super) purpose_key: String,
    pub(super) sender: String,
    pub(super) recipient: String,
    pub(super) subject: Option<String>,
    pub(super) text_body: Option<String>,
    pub(super) html_body: Option<String>,
    pub(super) attempts: i32,
    pub(super) first_attempt_at: DateTime<Utc>,
    pub(super) outcome_uncertain: bool,
    /// The claim reclaimed an expired lease.
    pub(super) reclaimed: bool,
}

/// Claim one due request: a queued row whose time has come, or a sending row
/// whose lease expired (its earlier attempt may have reached the provider).
pub(super) async fn claim_one(
    pool: &PgPool,
    lease: Duration,
) -> Result<Option<Claimed>, sqlx::Error> {
    sqlx::query_as(
        "WITH due AS (
             SELECT id, status = 'sending' AS reclaimed
               FROM email_deliveries
              WHERE (status = 'queued' AND next_attempt_at <= now())
                 OR (status = 'sending' AND lease_expires_at <= now())
              ORDER BY next_attempt_at
              LIMIT 1
              FOR UPDATE SKIP LOCKED
         )
         UPDATE email_deliveries d
            SET status = 'sending',
                attempts = d.attempts + 1,
                lease_expires_at = now() + make_interval(secs => $1),
                first_attempt_at = COALESCE(d.first_attempt_at, now()),
                outcome_uncertain = d.outcome_uncertain OR due.reclaimed,
                updated_at = now()
           FROM due
          WHERE d.id = due.id
          RETURNING d.id, d.org_id, d.purpose_kind, d.purpose_key, d.sender,
                    d.recipient, d.subject, d.text_body, d.html_body, d.attempts,
                    d.first_attempt_at, d.outcome_uncertain,
                    due.reclaimed",
    )
    .bind(lease.as_secs_f64())
    .fetch_optional(pool)
    .await
}

/// How a claimed attempt ended.
pub(super) enum Completion<'a> {
    Sent {
        provider_message_id: Option<&'a str>,
    },
    Failed {
        reason: &'a str,
        last_error: Option<&'a str>,
        uncertain: bool,
    },
    Cancelled {
        reason: &'a str,
    },
    Retry {
        delay: Duration,
        last_error: &'a str,
        uncertain: bool,
    },
}

/// Record an attempt's end if this claim still owns the row. Returns false
/// when another worker reclaimed it meanwhile.
pub(super) async fn complete(
    pool: &PgPool,
    claimed: &Claimed,
    completion: Completion<'_>,
) -> Result<bool, sqlx::Error> {
    const OWNED: &str = "WHERE id = $1 AND status = 'sending' AND attempts = $2";
    const ENDED: &str = "subject = NULL, text_body = NULL, html_body = NULL,
                         lease_expires_at = NULL, completed_at = now(), updated_at = now()";
    let done = match completion {
        Completion::Sent {
            provider_message_id,
        } => {
            sqlx::query(&format!(
                "UPDATE email_deliveries
                SET status = 'sent', provider_message_id = $3, terminal_reason = NULL, {ENDED}
              {OWNED}"
            ))
            .bind(claimed.id)
            .bind(claimed.attempts)
            .bind(provider_message_id.map(str::to_string))
            .execute(pool)
            .await?
        }
        Completion::Failed {
            reason,
            last_error,
            uncertain,
        } => {
            sqlx::query(&format!(
                "UPDATE email_deliveries
                SET status = 'failed', terminal_reason = $3,
                    last_error = COALESCE($4, last_error),
                    outcome_uncertain = outcome_uncertain OR $5, {ENDED}
              {OWNED}"
            ))
            .bind(claimed.id)
            .bind(claimed.attempts)
            .bind(reason.to_string())
            .bind(last_error.map(str::to_string))
            .bind(uncertain)
            .execute(pool)
            .await?
        }
        Completion::Cancelled { reason } => {
            sqlx::query(&format!(
                "UPDATE email_deliveries
                SET status = 'cancelled', terminal_reason = $3, {ENDED}
              {OWNED}"
            ))
            .bind(claimed.id)
            .bind(claimed.attempts)
            .bind(format!("cancelled:{reason}"))
            .execute(pool)
            .await?
        }
        Completion::Retry {
            delay,
            last_error,
            uncertain,
        } => {
            sqlx::query(&format!(
                "UPDATE email_deliveries
                SET status = 'queued', lease_expires_at = NULL,
                    next_attempt_at = now() + make_interval(secs => $3),
                    last_error = $4, outcome_uncertain = outcome_uncertain OR $5,
                    updated_at = now()
              {OWNED}"
            ))
            .bind(claimed.id)
            .bind(claimed.attempts)
            .bind(delay.as_secs_f64())
            .bind(last_error.to_string())
            .bind(uncertain)
            .execute(pool)
            .await?
        }
    };
    Ok(done.rows_affected() == 1)
}

/// Delete ended requests older than the retention period.
pub(super) async fn delete_ended_before(
    pool: &PgPool,
    retention: Duration,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "DELETE FROM email_deliveries
          WHERE completed_at IS NOT NULL
            AND completed_at < now() - make_interval(secs => $1)",
    )
    .bind(retention.as_secs_f64())
    .execute(pool)
    .await?
    .rows_affected())
}
