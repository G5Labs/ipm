//! Platform bug-report intake service.
//!
//! Owns the durable, one-directional delivery outbox for platform bug reports
//! (bugs about the G5 platform itself). The intake route enqueues a report and
//! returns immediately; a background worker drains the outbox and delivers each
//! report to a G5-owned GitHub repository via [`PlatformIssueGithubClient`],
//! with retry/backoff so a transient GitHub outage never drops a report.
//!
//! This is a single-sink, single-direction simplification of the prior design's
//! bidirectional sync engine: no watermark, no loop prevention, no inbound
//! consumer, no read-back. It never touches the internal Issue domain.

use std::sync::Arc;

use g5_contracts::error::G5Error;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::{RateLimitConfig, RateLimitError, RateLimitStore};
use crate::services::platform_issue_github::PlatformIssueGithubClient;

/// A captured platform bug report handed to [`PlatformIssueIntakeService::enqueue`].
pub struct PlatformIssueReport {
    /// The reporting G5 user, recorded as issue-body metadata and never as
    /// the GitHub author.
    pub reporter_user_id: Uuid,
    /// Snapshot of the reporter's human-readable display name (from the `users`
    /// table), resolved by the intake route so the issue body shows a name
    /// rather than a bare UUID. `None` when the user has no name or is absent.
    pub reporter_display_name: Option<String>,
    /// Snapshot of the deployment browser base URL (`server.external_url`)
    /// captured at intake, so the delivered issue can render the captured route
    /// as a clickable absolute link. `None`/blank degrades to plain-text route.
    pub report_base_url: Option<String>,
    pub title: String,
    pub body: String,
    pub kind: String,
    pub severity: String,
    /// Structured client context (route, appVersion, userAgent, viewport,
    /// workspaceSlug, orgSlug, breadcrumbs) rendered into the issue body.
    pub context: Value,
    /// Optional kept screenshot (PNG); committed via the Contents API on delivery.
    pub screenshot: Option<PlatformIssueScreenshot>,
}

/// A kept screenshot attached to a report.
pub struct PlatformIssueScreenshot {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// Outcome counts for one outbox drain batch.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlatformIssueOutboxBatchResult {
    pub claimed: i64,
    pub completed: i64,
    pub failed: i64,
}

/// Maximum delivery attempts before a row stops being retried.
const MAX_ATTEMPTS: i32 = 6;

/// Shared Postgres-backed quota (`rate_limit_windows`) for the reporting user
/// and the platform's one GitHub App credential. It is deliberately not a
/// process-local fallback:
/// accepting a report without a cluster-wide decision would let replicas evade
/// the configured external-service quota.
#[derive(Clone)]
pub struct PlatformIssueThrottle {
    store: Arc<dyn RateLimitStore>,
}

impl PlatformIssueThrottle {
    #[must_use]
    pub fn new(store: Arc<dyn RateLimitStore>) -> Self {
        Self { store }
    }

    /// Atomically reserve the reporter's 10/minute allowance and the one
    /// platform-wide 100/minute GitHub allowance.
    pub async fn check(&self, user_id: Uuid) -> Result<(), G5Error> {
        let reporter_key = format!("g5:rate-limit:{{platform-issues}}:reporter:{user_id}");
        let platform_key = "g5:rate-limit:{platform-issues}:global";
        let reporter_limit = RateLimitConfig {
            requests_per_minute: 10,
            burst_size: 10,
            enabled: true,
        };
        let platform_limit = RateLimitConfig {
            requests_per_minute: 100,
            burst_size: 100,
            enabled: true,
        };

        match self
            .store
            .try_acquire_pair(
                &reporter_key,
                &reporter_limit,
                platform_key,
                &platform_limit,
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(RateLimitError::Limited(retry_after)) => Err(G5Error::RateLimited {
                retry_after_secs: Some(retry_after.as_secs().max(1)),
            }),
            Err(RateLimitError::Unavailable) => Err(G5Error::ServiceUnavailable {
                message: "Platform bug reporting is temporarily unavailable".to_string(),
            }),
        }
    }
}

#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: Uuid,
    attempts: i32,
    reporter_user_id: Uuid,
    reporter_display_name: Option<String>,
    report_base_url: Option<String>,
    title: String,
    body: String,
    #[allow(dead_code)]
    kind: String,
    #[allow(dead_code)]
    severity: String,
    labels: Vec<String>,
    context: Value,
    screenshot_bytes: Option<Vec<u8>>,
    #[allow(dead_code)]
    screenshot_mime: Option<String>,
    github_issue_number: Option<i64>,
    github_issue_url: Option<String>,
    github_create_started_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Durable GitHub-issue intake service for platform bug reports.
#[derive(Clone)]
pub struct PlatformIssueIntakeService {
    db: PgPool,
    github: Arc<PlatformIssueGithubClient>,
    throttle: Option<PlatformIssueThrottle>,
}

impl PlatformIssueIntakeService {
    #[must_use]
    pub fn new(
        db: PgPool,
        github: Arc<PlatformIssueGithubClient>,
        rate_limits: Option<Arc<dyn RateLimitStore>>,
    ) -> Self {
        Self {
            db,
            github,
            throttle: rate_limits.map(PlatformIssueThrottle::new),
        }
    }

    /// Enforce the per-user submission throttle (design §5.7). Call before
    /// [`enqueue`](Self::enqueue) in the intake route.
    pub async fn check_rate(&self, user_id: Uuid) -> Result<(), G5Error> {
        let throttle = self
            .throttle
            .as_ref()
            .ok_or_else(|| G5Error::ServiceUnavailable {
                message: "Platform bug reporting is temporarily unavailable".to_string(),
            })?;
        throttle.check(user_id).await
    }

    /// Durably persist a report and return its submission id. Performs no GitHub
    /// call — delivery is the outbox worker's job.
    pub async fn enqueue(&self, report: PlatformIssueReport) -> Result<Uuid, G5Error> {
        let id = Uuid::new_v4();
        let labels = compose_labels(&report.kind, &report.severity);
        let (screenshot_bytes, screenshot_mime) = match report.screenshot {
            Some(shot) => (Some(shot.bytes), Some(shot.mime)),
            None => (None, None),
        };

        sqlx::query(
            "INSERT INTO platform_issue_outbox
                (id, status, reporter_user_id, reporter_display_name, report_base_url, title, body,
                 kind, severity, labels, context, screenshot_bytes, screenshot_mime)
             VALUES ($1, 'pending', $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(id)
        .bind(report.reporter_user_id)
        .bind(&report.reporter_display_name)
        .bind(&report.report_base_url)
        .bind(&report.title)
        .bind(&report.body)
        .bind(&report.kind)
        .bind(&report.severity)
        .bind(&labels)
        .bind(&report.context)
        .bind(screenshot_bytes)
        .bind(screenshot_mime)
        .execute(&self.db)
        .await
        .map_err(db_internal)?;

        Ok(id)
    }

    /// Claim and deliver up to `limit` due reports. Mirrors the durable-queue
    /// drain used by the issue-inference worker (claim under `FOR UPDATE SKIP
    /// LOCKED`, exponential backoff on failure).
    pub async fn process_due(&self, limit: i64) -> Result<PlatformIssueOutboxBatchResult, G5Error> {
        let claim_limit = limit.clamp(1, 100);

        let mut tx = self.db.begin().await.map_err(db_internal)?;
        let rows = sqlx::query_as::<_, OutboxRow>(
            "UPDATE platform_issue_outbox q
             SET status = 'processing',
                 locked_at = now(),
                 attempts = attempts + 1,
                 updated_at = now()
             WHERE q.id IN (
                 SELECT id
                 FROM platform_issue_outbox
                 WHERE (
                       (status IN ('pending', 'failed', 'reconciling') AND next_run_at <= now())
                   OR (status = 'processing' AND locked_at < now() - interval '10 minutes')
                   )
                   AND attempts < $2
                 ORDER BY created_at ASC
                 LIMIT $1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, attempts, reporter_user_id, reporter_display_name, report_base_url,
                       title, body, kind, severity, labels, context, screenshot_bytes,
                       screenshot_mime, github_issue_number, github_issue_url,
                       github_create_started_at",
        )
        .bind(claim_limit)
        .bind(MAX_ATTEMPTS)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_internal)?;
        tx.commit().await.map_err(db_internal)?;

        let mut result = PlatformIssueOutboxBatchResult {
            claimed: rows.len() as i64,
            ..Default::default()
        };

        for row in rows {
            // Idempotency: a persisted issue number is authoritative — a row that
            // was already delivered (worker crashed between GitHub 201 and the DB
            // write, then reclaimed) is completed without re-creating the issue.
            if let Some(number) = row.github_issue_number {
                if let Err(err) = self
                    .mark_completed(row.id, row.attempts, number, row.github_issue_url.clone())
                    .await
                {
                    tracing::error!(submission_id = %row.id, error = %err, "platform issue completion persistence failed");
                } else {
                    result.completed += 1;
                }
                continue;
            }

            // A prior worker can have created the GitHub issue but crashed
            // before persisting its number. Search for the exact marker before
            // deciding whether this row can make its first create attempt.
            match self.github.find_issue_by_marker(row.id).await {
                Ok(Some(existing)) => {
                    if let Err(err) = self
                        .mark_completed(
                            row.id,
                            row.attempts,
                            existing.number,
                            Some(existing.html_url),
                        )
                        .await
                    {
                        tracing::error!(submission_id = %row.id, error = %err, "platform issue adoption persistence failed");
                    } else {
                        result.completed += 1;
                    }
                    continue;
                }
                // Once the external-create fence has been committed, an empty
                // GitHub search cannot prove the issue was not created because
                // GitHub Search is eventually consistent. Retain the report for
                // operator-visible reconciliation rather than risking a second
                // POST.
                Ok(None) if row.github_create_started_at.is_some() => {
                    if let Err(err) = self
                        .mark_reconciling(
                            row.id,
                            row.attempts,
                            "GitHub marker not visible yet after issue-create fence",
                        )
                        .await
                    {
                        tracing::error!(submission_id = %row.id, error = %err, "platform issue reconciliation persistence failed");
                    } else {
                        result.failed += 1;
                    }
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    if let Err(persist_err) = self
                        .mark_failed(row.id, row.attempts, &err.to_string())
                        .await
                    {
                        tracing::error!(submission_id = %row.id, error = %persist_err, "platform issue failure persistence failed");
                    } else {
                        result.failed += 1;
                    }
                    continue;
                }
            }

            let body = self.delivery_body(&row).await;
            if let Err(err) = self.mark_create_started(row.id).await {
                tracing::error!(submission_id = %row.id, error = %err, "platform issue create-fence persistence failed");
                continue;
            }

            match self
                .github
                .create_issue(&row.title, &body, &row.labels)
                .await
            {
                Ok(created) => {
                    if row.attempts > 1 {
                        tracing::warn!(
                            submission_id = %row.id,
                            attempts = row.attempts,
                            github_issue_number = created.number,
                            "platform issue created after retry; review delivery history"
                        );
                    }
                    if let Err(err) = self
                        .mark_completed(
                            row.id,
                            row.attempts,
                            created.number,
                            Some(created.html_url),
                        )
                        .await
                    {
                        tracing::error!(submission_id = %row.id, error = %err, "platform issue completion persistence failed after GitHub create");
                    } else {
                        result.completed += 1;
                    }
                }
                Err(err) => {
                    if let Err(persist_err) = self
                        .mark_reconciling(row.id, row.attempts, &err.to_string())
                        .await
                    {
                        tracing::error!(submission_id = %row.id, error = %persist_err, "platform issue reconciliation persistence failed after create error");
                    } else {
                        result.failed += 1;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Compose the issue body and, when kept, recover or commit the screenshot
    /// before the external-create fence is crossed. The deterministic Contents
    /// path is write-once, so a retry never overwrites an asset without a SHA.
    async fn delivery_body(&self, row: &OutboxRow) -> String {
        let mut body = compose_body(row);

        if let Some(bytes) = row.screenshot_bytes.as_ref() {
            match self.github.find_screenshot_url(row.id).await {
                Ok(Some(url)) => body.push_str(&format!("\n\n![screenshot]({url})")),
                Ok(None) => match self.github.put_screenshot(row.id, bytes).await {
                    Ok(url) => body.push_str(&format!("\n\n![screenshot]({url})")),
                    Err(err) => match self.github.find_screenshot_url(row.id).await {
                        Ok(Some(url)) => body.push_str(&format!("\n\n![screenshot]({url})")),
                        Ok(None) | Err(_) => {
                            // A confirmed missing or unavailable Contents asset still permits a
                            // single text-only issue, but a successful prior commit is recovered.
                            tracing::warn!(
                                submission_id = %row.id,
                                error = %err,
                                "platform-issue screenshot commit failed; filing text-only"
                            );
                        }
                    },
                },
                Err(err) => {
                    // The contents lookup itself is unavailable; do not block the user report.
                    tracing::warn!(
                        submission_id = %row.id,
                        error = %err,
                        "platform-issue screenshot lookup failed; filing text-only"
                    );
                }
            }
        }

        body
    }

    /// Durably fence the irreversible GitHub create call. Any later worker
    /// observes this marker and reconciles rather than issuing another create.
    async fn mark_create_started(&self, id: Uuid) -> Result<(), G5Error> {
        sqlx::query(
            "UPDATE platform_issue_outbox
             SET github_create_started_at = COALESCE(github_create_started_at, now()),
                 updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(db_internal)?;
        Ok(())
    }

    async fn mark_completed(
        &self,
        id: Uuid,
        attempts: i32,
        number: i64,
        url: Option<String>,
    ) -> Result<(), G5Error> {
        let mut tx = self.db.begin().await.map_err(db_internal)?;
        sqlx::query(
            "INSERT INTO platform_issue_delivery_tombstone
                (submission_id, outcome, attempts, github_issue_number, github_issue_url, created_at)
             SELECT id, 'completed', $2, $3, COALESCE($4, github_issue_url), created_at
             FROM platform_issue_outbox
             WHERE id = $1",
        )
        .bind(id)
        .bind(attempts)
        .bind(number)
        .bind(url)
        .execute(&mut *tx)
        .await
        .map_err(db_internal)?;
        sqlx::query("DELETE FROM platform_issue_outbox WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_internal)?;
        tx.commit().await.map_err(db_internal)?;
        Ok(())
    }

    async fn mark_failed(&self, id: Uuid, attempts: i32, error: &str) -> Result<(), G5Error> {
        // Exponential backoff, capped at 60 minutes.
        let delay_minutes = (i64::from(attempts.max(1))).pow(2).min(60);
        let terminal = attempts >= MAX_ATTEMPTS;
        if terminal {
            return self.finalize_terminal_failure(id, attempts, error).await;
        }
        sqlx::query(
            "UPDATE platform_issue_outbox
             SET status = 'failed',
                 locked_at = NULL,
                 last_error = $2,
                 next_run_at = now() + make_interval(mins => $3),
                 updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .bind(delay_minutes as i32)
        .execute(&self.db)
        .await
        .map_err(db_internal)?;
        Ok(())
    }

    async fn finalize_terminal_failure(
        &self,
        id: Uuid,
        attempts: i32,
        error: &str,
    ) -> Result<(), G5Error> {
        let mut tx = self.db.begin().await.map_err(db_internal)?;
        sqlx::query(
            "INSERT INTO platform_issue_delivery_tombstone
                (submission_id, outcome, attempts, last_error, created_at)
             SELECT id, 'terminal_failed', $2, $3, created_at
             FROM platform_issue_outbox
             WHERE id = $1",
        )
        .bind(id)
        .bind(attempts)
        .bind(error)
        .execute(&mut *tx)
        .await
        .map_err(db_internal)?;
        sqlx::query("DELETE FROM platform_issue_outbox WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_internal)?;
        tx.commit().await.map_err(db_internal)?;
        Ok(())
    }

    async fn mark_reconciling(&self, id: Uuid, attempts: i32, error: &str) -> Result<(), G5Error> {
        if attempts >= MAX_ATTEMPTS {
            return self.finalize_terminal_failure(id, attempts, error).await;
        }
        let delay_minutes = (i64::from(attempts.max(1))).pow(2).min(60);
        sqlx::query(
            "UPDATE platform_issue_outbox
             SET status = 'reconciling',
                 locked_at = NULL,
                 last_error = $2,
                 next_run_at = now() + make_interval(mins => $3),
                 updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .bind(delay_minutes as i32)
        .execute(&self.db)
        .await
        .map_err(db_internal)?;
        Ok(())
    }
}

/// Build the GitHub label set for a report. A fixed `platform-report` label
/// makes reports filterable; `bug`/`<kind>` and `severity:<severity>` mirror the
/// report classification.
fn compose_labels(kind: &str, severity: &str) -> Vec<String> {
    let mut labels = vec!["platform-report".to_string()];
    let kind = kind.trim();
    if !kind.is_empty() {
        labels.push(kind.to_string());
    }
    let severity = severity.trim();
    if !severity.is_empty() {
        labels.push(format!("severity:{severity}"));
    }
    labels
}

/// Conservatively validate that a captured route is a plain absolute path that
/// is safe to embed as a Markdown link label and target: a leading `/` and only
/// URL-path characters. This rejects anything that could break out of the
/// `[label](target)` syntax or redirect to a foreign origin (spaces, parens,
/// brackets, backticks, `:`, control characters, protocol-relative `//host`).
fn is_safe_route_path(route: &str) -> bool {
    route.starts_with('/')
        && !route.starts_with("//")
        && route.len() <= 2048
        && route.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~' | b'%')
        })
}

/// Backslash-escape every character that could give an untrusted string
/// Markdown or GitHub-autolink meaning, so a client-supplied value renders as
/// literal, non-clickable, single-line text in the issue body and can never
/// form a link, emphasis, code span, raw HTML, bare-URL autolink, or a forged
/// extra list item / metadata line.
///
/// - Control characters (CR, LF, tab, and any other C0/C1 control) are rendered
///   as visible escapes (`\n`, `\r`, `\t`, or `\u{XX}`) so the value stays on a
///   single line and cannot inject a new Markdown block (e.g. a `\n- forged`
///   payload becoming a second list item).
/// - The unreserved URL-path characters that `is_safe_route_path` already
///   accepts (`-` `.` `_` `~` `/` `%`) are left as-is so a legitimate pathname
///   stays readable; every other ASCII punctuation byte is prefixed with a
///   backslash. GFM renders `\<punct>` as the literal punctuation, so the
///   escaped form is visually identical to the original while inert — in
///   particular the `:` in `https://` is escaped, defeating the bare-URL
///   autolinker.
fn escape_markdown_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:04x}}}", c as u32)),
            c if c.is_ascii_punctuation() && !matches!(c, '-' | '.' | '_' | '~' | '/' | '%') => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// Render the issue body: the user's text, a structured "Environment / context"
/// section, reporter metadata, and a hidden idempotency marker.
fn compose_body(row: &OutboxRow) -> String {
    let mut out = String::new();
    out.push_str(row.body.trim());
    out.push_str("\n\n---\n### Environment / context\n");

    // Route: render as a clickable absolute link so a G5 developer can open the
    // exact customer view in one click. Only linkify a conservatively validated
    // path against a snapshotted base URL; otherwise Markdown-escape the raw
    // route so a client-supplied value renders as literal, non-clickable text
    // and can never inject Markdown or a foreign link target (including a
    // complete `[label](url)` payload or a bare-URL autolink).
    if let Some(route) = row.context.get("route").and_then(Value::as_str) {
        if !route.is_empty() {
            match row.report_base_url.as_deref().map(str::trim) {
                Some(base) if !base.is_empty() && is_safe_route_path(route) => {
                    let url = format!("{}{}", base.trim_end_matches('/'), route);
                    out.push_str(&format!("- Route: [{route}]({url})\n"));
                }
                _ => out.push_str(&format!("- Route: {}\n", escape_markdown_text(route))),
            }
        }
    }

    for (label, key) in [
        ("App version", "appVersion"),
        ("User agent", "userAgent"),
        ("Viewport", "viewport"),
        ("Workspace slug", "workspaceSlug"),
        ("Org slug", "orgSlug"),
    ] {
        if let Some(value) = row.context.get(key).and_then(Value::as_str) {
            if !value.is_empty() {
                out.push_str(&format!("- {label}: {value}\n"));
            }
        }
    }

    if let Some(breadcrumbs) = row.context.get("breadcrumbs").and_then(Value::as_array) {
        let lines: Vec<String> = breadcrumbs
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if !lines.is_empty() {
            out.push_str(&format!("- Breadcrumbs: {}\n", lines.join(" | ")));
        }
    }

    // Prefer a human-readable name for triagers; always keep the exact user id
    // for deduplication. Fall back to the id-only form when no name snapshot
    // was captured (name-less or absent user).
    match row.reporter_display_name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => out.push_str(&format!(
            "- Reporter: {name} (G5 user id: {})\n",
            row.reporter_user_id
        )),
        _ => out.push_str(&format!(
            "- Reporter (G5 user id): {}\n",
            row.reporter_user_id
        )),
    }

    // Hidden idempotency marker (design §5.5): lets a human or a future
    // reconcile pass identify the outbox row that produced an issue.
    out.push_str(&format!("\n<!-- g5-report:{} -->\n", row.id));
    out
}

fn db_internal(err: sqlx::Error) -> G5Error {
    G5Error::Internal {
        message: format!("Database error: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row_with(context: Value, reporter: Uuid) -> OutboxRow {
        OutboxRow {
            id: Uuid::nil(),
            attempts: 1,
            reporter_user_id: reporter,
            reporter_display_name: None,
            report_base_url: None,
            title: "Tab bar loses state".to_string(),
            body: "It resets on reload.".to_string(),
            kind: "bug".to_string(),
            severity: "high".to_string(),
            labels: vec![],
            context,
            screenshot_bytes: None,
            screenshot_mime: None,
            github_issue_number: None,
            github_issue_url: None,
            github_create_started_at: None,
        }
    }

    #[test]
    fn labels_include_fixed_kind_and_severity() {
        let labels = compose_labels("bug", "high");
        assert_eq!(
            labels,
            vec![
                "platform-report".to_string(),
                "bug".to_string(),
                "severity:high".to_string()
            ]
        );
    }

    #[test]
    fn body_renders_context_reporter_and_marker() {
        let reporter = Uuid::from_u128(0x1234);
        let row = row_with(
            json!({
                "route": "/workspaces/abc",
                "appVersion": "1.2.3",
                "userAgent": "Mozilla/5.0",
                "viewport": "1280x720",
                "workspaceSlug": "ws-1",
                "orgSlug": "org-1",
                "breadcrumbs": ["opened merge modal", "clicked confirm"]
            }),
            reporter,
        );
        let body = compose_body(&row);
        assert!(body.contains("It resets on reload."));
        assert!(body.contains("### Environment / context"));
        assert!(body.contains("- Route: /workspaces/abc"));
        assert!(body.contains("- App version: 1.2.3"));
        assert!(body.contains("- Viewport: 1280x720"));
        assert!(body.contains("- Breadcrumbs: opened merge modal | clicked confirm"));
        assert!(body.contains(&format!("- Reporter (G5 user id): {reporter}")));
        assert!(body.contains(&format!("<!-- g5-report:{} -->", row.id)));
    }

    #[test]
    fn body_omits_absent_context_fields() {
        let reporter = Uuid::from_u128(0x5678);
        let row = row_with(json!({ "route": "/x" }), reporter);
        let body = compose_body(&row);
        assert!(body.contains("- Route: /x"));
        assert!(!body.contains("- App version:"));
        assert!(body.contains(&format!("- Reporter (G5 user id): {reporter}")));
    }

    #[test]
    fn body_renders_reporter_display_name_with_user_id_when_present() {
        let reporter = Uuid::from_u128(0x9abc);
        let mut row = row_with(json!({ "route": "/x" }), reporter);
        row.reporter_display_name = Some("Alex Chen".to_string());
        let body = compose_body(&row);
        // Human-readable name plus the exact id for dedup; not the id-only form.
        assert!(body.contains(&format!("- Reporter: Alex Chen (G5 user id: {reporter})")));
        assert!(!body.contains(&format!("- Reporter (G5 user id): {reporter}")));
    }

    #[test]
    fn body_falls_back_to_id_only_for_blank_display_name() {
        let reporter = Uuid::from_u128(0xdef0);
        let mut row = row_with(json!({ "route": "/x" }), reporter);
        row.reporter_display_name = Some("   ".to_string());
        let body = compose_body(&row);
        assert!(body.contains(&format!("- Reporter (G5 user id): {reporter}")));
        assert!(!body.contains("- Reporter: "));
    }

    #[test]
    fn route_renders_as_clickable_link_when_base_url_present() {
        let mut row = row_with(
            json!({ "route": "/acme/demo/spec/architecture" }),
            Uuid::from_u128(0x11),
        );
        // Trailing slash on the base is normalized so the URL is well-formed.
        row.report_base_url = Some("https://app.g5labs.ai/".to_string());
        let body = compose_body(&row);
        assert!(body.contains(
            "- Route: [/acme/demo/spec/architecture](https://app.g5labs.ai/acme/demo/spec/architecture)"
        ));
    }

    #[test]
    fn route_stays_plain_text_without_a_base_url() {
        let row = row_with(json!({ "route": "/acme/demo" }), Uuid::from_u128(0x22));
        let body = compose_body(&row);
        assert!(body.contains("- Route: /acme/demo\n"));
        assert!(!body.contains("- Route: ["));
    }

    #[test]
    fn unsafe_route_is_not_linkified() {
        // A hostile route that tries to break out of the Markdown link or point
        // at a foreign origin must degrade to Markdown-escaped literal text,
        // never a link (neither our own `[label](url)` nor the injected one).
        let mut row = row_with(
            json!({ "route": "/x](https://evil.example.com)" }),
            Uuid::from_u128(0x33),
        );
        row.report_base_url = Some("https://app.g5labs.ai".to_string());
        let body = compose_body(&row);
        // The raw route is escaped so its `](...)` cannot form a Markdown link
        // and its `https://` cannot bare-URL autolink.
        assert!(body.contains("- Route: /x\\]\\(https\\://evil.example.com\\)\n"));
        // The unescaped injected link payload must not survive anywhere.
        assert!(!body.contains("](https://evil.example.com)"));
        assert!(!body.contains("- Route: [/x]"));
        assert!(!body.contains("(https://app.g5labs.ai/x]"));
    }

    #[test]
    fn unsafe_route_fallback_neutralizes_control_characters() {
        // A newline-plus-list payload must not forge a second Markdown list
        // item: the route stays on one line with the newline rendered inert.
        let mut row = row_with(
            json!({ "route": "/x\n- forged context" }),
            Uuid::from_u128(0x55),
        );
        row.report_base_url = Some("https://app.g5labs.ai".to_string());
        let body = compose_body(&row);
        // The CR/LF is rendered as visible `\n`, keeping the value single-line.
        assert!(body.contains("- Route: /x\\n- forged context\n"));
        // No raw newline injection, so no forged second list item.
        assert!(!body.contains("/x\n- forged context"));
    }

    #[test]
    fn unsafe_route_fallback_escapes_full_markdown_link_payload() {
        // Regression for a complete injected Markdown link: the payload
        // `/[open](https://evil.example)` must render non-clickably.
        let mut row = row_with(
            json!({ "route": "/[open](https://evil.example)" }),
            Uuid::from_u128(0x44),
        );
        row.report_base_url = Some("https://app.g5labs.ai".to_string());
        let body = compose_body(&row);
        // Backslash-escaped so neither a `[label](url)` link nor a bare-URL
        // autolink can render.
        assert!(body.contains("- Route: /\\[open\\]\\(https\\://evil.example\\)\n"));
        // The clickable payload must not appear unescaped.
        assert!(!body.contains("[open](https://evil.example)"));
    }

    #[test]
    fn is_safe_route_path_accepts_paths_and_rejects_injection() {
        assert!(is_safe_route_path("/acme/demo/task/456"));
        assert!(is_safe_route_path("/acme/demo/issues/bug-123"));
        assert!(!is_safe_route_path("no-leading-slash"));
        assert!(!is_safe_route_path("//evil.example.com")); // protocol-relative
        assert!(!is_safe_route_path("/a b")); // space
        assert!(!is_safe_route_path("/a](x)")); // markdown break-out
        assert!(!is_safe_route_path("/a\nb")); // control character
    }
}
