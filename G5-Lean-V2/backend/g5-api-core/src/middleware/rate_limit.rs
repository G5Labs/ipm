//! Rate limit middleware for the g5-api HTTP processing pipeline.
//!
//! This layer runs after client IP resolution and before authentication.
//!
//! It uses the [`governor`] crate's keyed GCRA rate limiter (token-bucket
//! equivalent: burst capacity plus a sustained refill rate) to throttle
//! excessive request volume before requests reach the authentication layer.
//! This protects JWT signature verification from brute-force attacks —
//! without rate limiting before auth, an attacker could flood the server
//! with invalid tokens, consuming CPU on cryptographic operations.
//!
//! The module provides:
//! - A [`RateLimitStore`] trait abstracting the storage backend.
//! - An [`InMemoryRateLimitStore`] default implementation wrapping a
//!   `governor` keyed rate limiter.
//! - A [`PostgresRateLimitStore`] for quotas shared by every API replica
//!   (one `rate_limit_windows` row per key on the database clock).
//! - [`RateLimitLayer`] / [`RateLimitMiddleware`] Tower Layer/Service types.
//! - Proper `429 RATE_LIMITED` error responses with `Retry-After` header and
//!   request ID correlation.

use std::future::Future;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use chrono::{DateTime, Utc};
use governor::clock::{Clock, DefaultClock};
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use http::{Request, Response, StatusCode};
use sqlx::{PgConnection, PgPool};
use tokio_util::sync::CancellationToken;
use tower::{Layer, Service};

use super::request_id::RequestId;

/// The governor keyed rate limiter type used by [`InMemoryRateLimitStore`]:
/// per-IP GCRA state in governor's default keyed store (DashMap-backed,
/// per-shard locking) on the default monotonic clock.
type KeyedLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

// ---------------------------------------------------------------------------
// RateLimitConfig
// ---------------------------------------------------------------------------

/// Configuration for the rate limiter.
///
/// Controls the per-IP request rate and burst size. When `enabled` is `false`,
/// the middleware passes all requests through without rate checking — useful
/// for development and testing.
///
/// # Defaults
///
/// - `requests_per_minute`: 100
/// - `burst_size`: 100 (same as `requests_per_minute`)
/// - `enabled`: true
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Per-IP requests per minute limit.
    pub requests_per_minute: u32,
    /// Maximum burst size (token bucket capacity).
    pub burst_size: u32,
    /// Master switch — if `false`, all requests pass through.
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 100,
            burst_size: 100,
            enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// RateLimitStore trait
// ---------------------------------------------------------------------------

/// Trait abstracting the rate limit storage backend.
///
/// Allows swapping between in-memory and Postgres-backed implementations for
/// distributed rate limiting in multi-instance deployments.
///
/// The trait is object-safe and requires `Send + Sync + 'static` for use in
/// async middleware across threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitError {
    /// The key has exhausted its configured quota. The duration is advisory
    /// and is emitted as `Retry-After` by HTTP callers.
    Limited(Duration),
    /// A shared backend could not make an authoritative decision. Callers that
    /// protect a shared external credential must fail closed.
    Unavailable,
}

#[async_trait::async_trait]
pub trait RateLimitStore: Send + Sync + 'static {
    /// Try to acquire a token for the given key.
    ///
    /// Returns `Ok(())` if the request should proceed.
    /// Returns `Limited(retry_after)` when the request should be rate limited,
    /// or `Unavailable` when a shared backend cannot decide safely.
    async fn try_acquire(&self, key: &str, config: &RateLimitConfig) -> Result<(), RateLimitError>;

    /// Atomically acquire two independent keys when the backend supports it.
    /// A backend must opt into this explicitly: sequential acquisition can
    /// charge the first bucket while the second rejects and is unsafe for a
    /// shared external credential.
    async fn try_acquire_pair(
        &self,
        _first_key: &str,
        _first_config: &RateLimitConfig,
        _second_key: &str,
        _second_config: &RateLimitConfig,
    ) -> Result<(), RateLimitError> {
        Err(RateLimitError::Unavailable)
    }
}

// ---------------------------------------------------------------------------
// InMemoryRateLimitStore
// ---------------------------------------------------------------------------

/// Default in-memory rate limit store wrapping a [`governor`] keyed rate
/// limiter (GCRA — token-bucket-equivalent admission: full burst available to
/// a new key, quota consumed per request, deterministic retry-after).
///
/// Governor's default keyed state store is DashMap-backed, so different IP
/// addresses are rate-checked concurrently without global contention.
///
/// # Memory Management
///
/// Stale per-key state should be periodically cleaned up via [`cleanup_stale`]
/// to prevent unbounded memory growth. This cleanup is NOT called
/// automatically by the middleware — it should be triggered by a background
/// task.
///
/// [`cleanup_stale`]: InMemoryRateLimitStore::cleanup_stale
pub struct InMemoryRateLimitStore {
    /// Lazily initialized from the first `try_acquire` call's config — the
    /// config is process-constant (derived from `AppConfig`), so the first
    /// config wins. A later call with a DIFFERING config is served by the
    /// initialized limiter and logged once at `warn` (it would otherwise be
    /// silently ignored — codex review of chat 2d6874de, finding 3).
    state: OnceLock<LimiterState>,
    /// Warn-once guard for the differing-config log.
    config_mismatch_warned: std::sync::Once,
    /// Clock used to convert governor's `NotUntil` into a retry-after
    /// duration. Same monotonic source as the limiter's internal clock.
    clock: DefaultClock,
}

/// The lazily-initialized limiter plus the config values it was built from,
/// kept so later calls can detect a differing config.
struct LimiterState {
    limiter: KeyedLimiter,
    requests_per_minute: u32,
    burst_size: u32,
}

impl Default for InMemoryRateLimitStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the governor quota from a [`RateLimitConfig`]: sustained
/// `requests_per_minute` with `burst_size` burst capacity. Zero config values
/// are clamped to 1 to satisfy `NonZeroU32`.
fn quota_from_config(config: &RateLimitConfig) -> Quota {
    let per_minute = NonZeroU32::new(config.requests_per_minute).unwrap_or(NonZeroU32::MIN);
    let burst = NonZeroU32::new(config.burst_size).unwrap_or(NonZeroU32::MIN);
    Quota::per_minute(per_minute).allow_burst(burst)
}

impl InMemoryRateLimitStore {
    /// Create a new empty in-memory rate limit store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: OnceLock::new(),
            config_mismatch_warned: std::sync::Once::new(),
            clock: DefaultClock::default(),
        }
    }

    /// Remove stale per-key state to prevent unbounded memory growth.
    ///
    /// Delegates to governor's `retain_recent()` (drops key state old enough
    /// to no longer affect admission decisions) and `shrink_to_fit()`. Should
    /// be called periodically by a background task (e.g., every 5 minutes).
    ///
    /// If an IP returns after its state is cleaned up, it starts fresh with
    /// the full burst available.
    pub fn cleanup_stale(&self) {
        if let Some(state) = self.state.get() {
            state.limiter.retain_recent();
            state.limiter.shrink_to_fit();
        }
    }
}

#[async_trait::async_trait]
impl RateLimitStore for InMemoryRateLimitStore {
    async fn try_acquire(&self, key: &str, config: &RateLimitConfig) -> Result<(), RateLimitError> {
        if !config.enabled {
            return Ok(());
        }

        let state = self.state.get_or_init(|| LimiterState {
            limiter: RateLimiter::keyed(quota_from_config(config)),
            requests_per_minute: config.requests_per_minute,
            burst_size: config.burst_size,
        });

        if state.requests_per_minute != config.requests_per_minute
            || state.burst_size != config.burst_size
        {
            self.config_mismatch_warned.call_once(|| {
                tracing::warn!(
                    initialized_rpm = state.requests_per_minute,
                    initialized_burst = state.burst_size,
                    requested_rpm = config.requests_per_minute,
                    requested_burst = config.burst_size,
                    "InMemoryRateLimitStore received a config differing from the one it \
                     was initialized with; the initialized quota stays in effect. Use a \
                     separate store instance per distinct RateLimitConfig."
                );
            });
        }

        state
            .limiter
            .check_key(&key.to_string())
            .map_err(|not_until| {
                RateLimitError::Limited(not_until.wait_time_from(self.clock.now()))
            })
    }
}

// ---------------------------------------------------------------------------
// PostgresRateLimitStore
// ---------------------------------------------------------------------------

/// Postgres-backed rate-limit store for routes that need a quota shared by
/// every API replica, on the `rate_limit_windows` table: one row per key, a
/// fixed window measured on the database clock (so replicas agree), reset in
/// place when a new window starts. The platform-report route uses
/// `try_acquire_pair` to reserve the reporter key and the single
/// GitHub-credential key in one transaction; a rejection charges neither, and
/// both keys are charged to the same window (the one current once every row
/// lock is held).
#[derive(Clone)]
pub struct PostgresRateLimitStore {
    pool: PgPool,
}

/// Window length of the shared counters; `RateLimitConfig::requests_per_minute`
/// is the limit per window.
const WINDOW_SECONDS: i64 = 60;

/// Keys that have not been charged for this long are dead weight and are
/// removed by [`run_rate_limit_window_trim`]. Hygiene only: the store resets a
/// key's window in place, so a stale row never admits or rejects wrongly.
const STALE_KEY_AGE_SECONDS: i64 = 24 * 60 * 60;

/// Cadence of [`run_rate_limit_window_trim`].
const TRIM_INTERVAL: Duration = Duration::from_secs(600);

/// Longest wait for a quota row another transaction holds. A quota row is
/// held for the few milliseconds of an acquisition, so a longer wait means a
/// stalled holder (maintenance, manual repair, DDL); the acquisition then
/// fails closed as `Unavailable` instead of pinning a pool connection, the
/// bound the Redis store had through its two-second timeouts.
const LOCK_WAIT: &str = "3s";

impl PostgresRateLimitStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Lock `key`'s row, creating it with an epoch-zero window (so the first
    /// charge opens a fresh window) when it does not exist yet. One statement:
    /// the no-op `DO UPDATE` takes the row lock, waits out a concurrent trim's
    /// delete and inserts afresh when that delete commits, so an acquisition
    /// can never end up without a row. Locks are taken before any window is
    /// sampled: see [`Self::acquire_all`].
    async fn lock_row(conn: &mut PgConnection, key: &str) -> Result<(), sqlx::Error> {
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO rate_limit_windows (key, window_start, count)
             VALUES ($1, to_timestamp(0), 0)
             ON CONFLICT (key) DO UPDATE SET count = rate_limit_windows.count
             RETURNING count",
        )
        .bind(key)
        .fetch_one(&mut *conn)
        .await?;
        Ok(())
    }

    /// The start of the window current on the database clock right now.
    async fn current_bucket(conn: &mut PgConnection) -> Result<DateTime<Utc>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT to_timestamp((floor(extract(epoch FROM clock_timestamp()) / $1::int) * $1::int)::double precision)",
        )
        .bind(WINDOW_SECONDS as i32)
        .fetch_one(conn)
        .await
    }

    /// Charge `key` once in `bucket`, whose row the caller has locked.
    /// `Ok(None)` admits; `Ok(Some(retry_after))` means the window is full and
    /// nothing was charged. A stored window never moves backward: when the row
    /// already holds a newer window (opened by another acquirer while this one
    /// waited for its locks) the charge counts against that newer window.
    async fn charge(
        conn: &mut PgConnection,
        key: &str,
        config: &RateLimitConfig,
        bucket: DateTime<Utc>,
    ) -> Result<Option<Duration>, sqlx::Error> {
        let limit = i32::try_from(config.requests_per_minute).unwrap_or(i32::MAX);
        let admitted: Option<i32> = sqlx::query_scalar(
            "UPDATE rate_limit_windows
                SET count = CASE WHEN $2 > window_start THEN 1 ELSE count + 1 END,
                    window_start = GREATEST(window_start, $2)
              WHERE key = $1 AND ($2 > window_start OR count < $3::int)
              RETURNING count",
        )
        .bind(key)
        .bind(bucket)
        .bind(limit)
        .fetch_optional(&mut *conn)
        .await?;
        if admitted.is_some() {
            return Ok(None);
        }
        // Rejected by the stored window: wait until that window ends. The
        // stored window is never older than `bucket`, so the wait is under
        // two windows; anything beyond that is a hand-edited row.
        let remaining: Option<f64> = sqlx::query_scalar(
            "SELECT extract(epoch FROM (window_start + make_interval(secs => $2) - clock_timestamp()))::double precision
               FROM rate_limit_windows WHERE key = $1",
        )
        .bind(key)
        .bind(WINDOW_SECONDS as f64)
        .fetch_optional(&mut *conn)
        .await?;
        let remaining = remaining.unwrap_or(1.0).ceil() as i64;
        Ok(Some(Duration::from_secs(
            remaining.clamp(1, 2 * WINDOW_SECONDS) as u64,
        )))
    }

    /// Charge every entry in one transaction and roll everything back on the
    /// first rejection. Every row is locked first, in one global key order
    /// (so two pairs can never deadlock) and within [`LOCK_WAIT`], and only
    /// then is the window sampled, once for the whole acquisition: an
    /// acquirer that waited for a lock across a window boundary is charged,
    /// for every key, to the window current at admission, never partly to the
    /// window it started in.
    async fn acquire_all(
        &self,
        mut entries: Vec<(&str, &RateLimitConfig)>,
    ) -> Result<(), RateLimitError> {
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut tx = self.pool.begin().await.map_err(unavailable)?;
        // Transaction-local: a lock wait past the bound raises an error that
        // maps to `Unavailable` and rolls the transaction back.
        sqlx::query(&format!("SET LOCAL lock_timeout = '{LOCK_WAIT}'"))
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        for (key, _) in &entries {
            Self::lock_row(&mut tx, key).await.map_err(unavailable)?;
        }
        let bucket = Self::current_bucket(&mut tx).await.map_err(unavailable)?;
        for (key, config) in entries {
            if let Some(retry_after) = Self::charge(&mut tx, key, config, bucket)
                .await
                .map_err(unavailable)?
            {
                // Undo any charge already made in this transaction.
                if let Err(err) = tx.rollback().await {
                    tracing::warn!(error = %err, "Postgres rate-limit rollback failed");
                }
                return Err(RateLimitError::Limited(retry_after));
            }
        }
        tx.commit().await.map_err(unavailable)?;
        Ok(())
    }

    /// Delete keys that have not been charged for [`STALE_KEY_AGE_SECONDS`].
    /// Rows an acquisition holds are skipped, never waited for: the trim takes
    /// no lock another transaction is waiting on, so it can never close a lock
    /// cycle with a pair that locks its keys in order. A skipped row is still
    /// idle on the next pass.
    pub async fn trim_stale(pool: &PgPool) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query(
            "DELETE FROM rate_limit_windows
              WHERE key IN (SELECT key FROM rate_limit_windows
                             WHERE window_start < now() - make_interval(secs => $1)
                             ORDER BY key
                             FOR UPDATE SKIP LOCKED)",
        )
        .bind(STALE_KEY_AGE_SECONDS as f64)
        .execute(pool)
        .await?
        .rows_affected())
    }
}

fn unavailable(err: sqlx::Error) -> RateLimitError {
    tracing::warn!(error = %err, "Postgres rate-limit backend is unavailable");
    RateLimitError::Unavailable
}

#[async_trait::async_trait]
impl RateLimitStore for PostgresRateLimitStore {
    async fn try_acquire(&self, key: &str, config: &RateLimitConfig) -> Result<(), RateLimitError> {
        if !config.enabled {
            return Ok(());
        }
        self.acquire_all(vec![(key, config)]).await
    }

    async fn try_acquire_pair(
        &self,
        first_key: &str,
        first_config: &RateLimitConfig,
        second_key: &str,
        second_config: &RateLimitConfig,
    ) -> Result<(), RateLimitError> {
        let entries: Vec<(&str, &RateLimitConfig)> =
            [(first_key, first_config), (second_key, second_config)]
                .into_iter()
                .filter(|(_, config)| config.enabled)
                .collect();
        if entries.is_empty() {
            return Ok(());
        }
        self.acquire_all(entries).await
    }
}

/// Maintenance loop spawned by the daemon role: trims idle quota keys every
/// [`TRIM_INTERVAL`]. Errors are logged and never stop the loop.
pub async fn run_rate_limit_window_trim(pool: PgPool, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(TRIM_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = interval.tick() => match PostgresRateLimitStore::trim_stale(&pool).await {
                Ok(0) => tracing::debug!("rate_limit_window_trim: nothing to trim"),
                Ok(rows) => tracing::info!(rows, "rate_limit_window_trim: completed"),
                Err(error) => tracing::error!(error = %error, "rate_limit_window_trim: failed"),
            },
            _ = cancel.cancelled() => {
                tracing::info!("rate_limit_window_trim: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rate Limit Error Response
// ---------------------------------------------------------------------------

/// Build a 429 Too Many Requests response with the standard error envelope.
///
/// The response includes:
/// - HTTP status `429 Too Many Requests`
/// - `Retry-After` header with integer seconds (minimum 1)
/// - `content-type: application/json` header
/// - JSON body following the standard error envelope format:
///   `{"error":{"code":"RATE_LIMITED","message":"...","details":{},"requestId":"..."}}`
fn build_rate_limit_response(request_id: String, retry_after_secs: u64) -> Response<Body> {
    let body = serde_json::json!({
        "error": {
            "code": "RATE_LIMITED",
            "message": format!("Too many requests. Please retry after {} seconds.", retry_after_secs),
            "details": {},
            "requestId": request_id
        }
    });

    let body_bytes = serde_json::to_vec(&body).unwrap_or_else(|e| {
        tracing::error!(error = %e, "Failed to serialize rate-limit response body");
        Vec::new()
    });

    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header("content-type", "application/json")
        .header("retry-after", retry_after_secs.to_string())
        .body(Body::from(body_bytes))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap()
        })
}

/// Build a fail-closed rate-limit-backend response with the same envelope
/// contract as quota exhaustion. A short retry delay prevents immediate
/// client retry loops while the shared backend reconnects.
fn build_rate_limit_unavailable_response(request_id: String) -> Response<Body> {
    let body = serde_json::json!({
        "error": {
            "code": "RATE_LIMIT_UNAVAILABLE",
            "message": "Rate-limit service is temporarily unavailable. Please retry after 1 second.",
            "details": {},
            "requestId": request_id
        }
    });

    let body_bytes = serde_json::to_vec(&body).unwrap_or_else(|err| {
        tracing::error!(error = %err, "Failed to serialize rate-limit unavailable response body");
        Vec::new()
    });

    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header("content-type", "application/json")
        .header("retry-after", "1")
        .body(Body::from(body_bytes))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap()
        })
}

// ---------------------------------------------------------------------------
// RateLimitLayer
// ---------------------------------------------------------------------------

/// Tower [`Layer`] for rate limiting.
///
/// Holds a shared reference to the rate limit store and configuration.
/// Apply this as the fourth layer in the middleware stack (after Compression,
/// before Auth).
///
/// # Usage
///
/// ```ignore
/// use std::sync::Arc;
/// use crate::middleware::rate_limit::{RateLimitLayer, InMemoryRateLimitStore, RateLimitConfig};
///
/// let store = Arc::new(InMemoryRateLimitStore::new());
/// let config = RateLimitConfig::default();
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(RateLimitLayer::new(store, config));
/// ```
#[derive(Clone)]
pub struct RateLimitLayer {
    store: Arc<dyn RateLimitStore>,
    config: RateLimitConfig,
}

impl RateLimitLayer {
    /// Create a new rate limit layer with the given store and configuration.
    pub fn new(store: Arc<dyn RateLimitStore>, config: RateLimitConfig) -> Self {
        Self { store, config }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitMiddleware {
            inner,
            store: self.store.clone(),
            config: self.config.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// RateLimitMiddleware
// ---------------------------------------------------------------------------

/// Tower [`Service`] that enforces rate limits.
///
/// Wraps the inner service and checks rate limits before forwarding requests.
/// When rate limiting is disabled (`config.enabled == false`), all requests
/// pass through without rate checking.
///
/// On rate limit violation, returns a `429 Too Many Requests` response with
/// the standard error envelope and `Retry-After` header.
#[derive(Clone)]
pub struct RateLimitMiddleware<S> {
    inner: S,
    store: Arc<dyn RateLimitStore>,
    config: RateLimitConfig,
}

impl<S> Service<Request<Body>> for RateLimitMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // If rate limiting is disabled, pass through
        if !self.config.enabled {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(req).await });
        }

        // Extract client IP
        let client_ip = super::client_ip::ClientIp::from_extensions(req.extensions()).to_string();

        let store = self.store.clone();
        let config = self.config.clone();
        let mut inner = self.inner.clone();
        Box::pin(async move {
            match store.try_acquire(&client_ip, &config).await {
                Ok(()) => inner.call(req).await,
                Err(RateLimitError::Limited(retry_after)) => {
                    let request_id = req
                        .extensions()
                        .get::<RequestId>()
                        .map(|r| r.0.clone())
                        .unwrap_or_default();
                    Ok(build_rate_limit_response(
                        request_id,
                        retry_after.as_secs().max(1),
                    ))
                }
                Err(RateLimitError::Unavailable) => {
                    let request_id = req
                        .extensions()
                        .get::<RequestId>()
                        .map(|request_id| request_id.0.clone())
                        .unwrap_or_default();
                    tracing::error!(%request_id, "API rate-limit store became unavailable");
                    Ok(build_rate_limit_unavailable_response(request_id))
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use governor::clock::FakeRelativeClock;
    use governor::middleware::NoOpMiddleware;
    use governor::nanos::Nanos;
    use governor::state::keyed::DashMapStateStore;
    use http::{Request, Response, StatusCode};
    use tower::{ServiceBuilder, ServiceExt};

    // -----------------------------------------------------------------------
    // Helper services
    // -----------------------------------------------------------------------

    /// Helper service that echoes back a 200 OK with an empty body.
    #[derive(Clone)]
    struct EchoService;

    struct UnavailableRateLimitStore;

    struct RejectingRateLimitStore(Duration);

    /// Governor-backed test store whose monotonic clock advances only when a
    /// test asks it to. Multi-acquire assertions must not depend on how long a
    /// loaded CI worker deschedules the test process between two calls.
    struct DeterministicRateLimitStore {
        limiter: RateLimiter<
            String,
            DashMapStateStore<String>,
            FakeRelativeClock,
            NoOpMiddleware<Nanos>,
        >,
        clock: FakeRelativeClock,
    }

    impl DeterministicRateLimitStore {
        fn new(config: &RateLimitConfig) -> Self {
            let clock = FakeRelativeClock::default();
            let limiter = RateLimiter::dashmap_with_clock(quota_from_config(config), clock.clone());
            Self { limiter, clock }
        }

        fn advance(&self, duration: Duration) {
            self.clock.advance(duration);
        }

        fn cleanup_stale(&self) {
            self.limiter.retain_recent();
            self.limiter.shrink_to_fit();
        }
    }

    #[async_trait::async_trait]
    impl RateLimitStore for UnavailableRateLimitStore {
        async fn try_acquire(
            &self,
            _key: &str,
            _config: &RateLimitConfig,
        ) -> Result<(), RateLimitError> {
            Err(RateLimitError::Unavailable)
        }
    }

    #[async_trait::async_trait]
    impl RateLimitStore for RejectingRateLimitStore {
        async fn try_acquire(
            &self,
            _key: &str,
            _config: &RateLimitConfig,
        ) -> Result<(), RateLimitError> {
            Err(RateLimitError::Limited(self.0))
        }
    }

    #[async_trait::async_trait]
    impl RateLimitStore for DeterministicRateLimitStore {
        async fn try_acquire(
            &self,
            key: &str,
            config: &RateLimitConfig,
        ) -> Result<(), RateLimitError> {
            if !config.enabled {
                return Ok(());
            }

            self.limiter
                .check_key(&key.to_string())
                .map_err(|not_until| {
                    RateLimitError::Limited(not_until.wait_time_from(self.clock.now()))
                })
        }
    }

    impl Service<Request<Body>> for EchoService {
        type Response = Response<Body>;
        type Error = Box<dyn std::error::Error + Send + Sync>;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            Box::pin(async {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::empty())
                    .unwrap())
            })
        }
    }

    // -----------------------------------------------------------------------
    // RateLimitConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_minute, 100);
        assert_eq!(config.burst_size, 100);
        assert!(config.enabled);
    }

    #[test]
    fn test_rate_limit_config_clone_and_debug() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 10,
            enabled: false,
        };
        let cloned = config.clone();
        assert_eq!(cloned.requests_per_minute, 60);
        assert_eq!(cloned.burst_size, 10);
        assert!(!cloned.enabled);

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("RateLimitConfig"));
    }

    // -----------------------------------------------------------------------
    // InMemoryRateLimitStore tests (governor-backed)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_in_memory_store_first_request_allowed() {
        let store = InMemoryRateLimitStore::new();
        let config = RateLimitConfig::default();

        let result = store.try_acquire("192.168.1.1", &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_in_memory_store_new_key_gets_full_burst() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 5,
            enabled: true,
        };
        let store = DeterministicRateLimitStore::new(&config);

        // A new key must be able to burst up to burst_size immediately.
        for i in 0..5 {
            assert!(
                store.try_acquire("10.0.0.9", &config).await.is_ok(),
                "burst request {i} should be admitted"
            );
        }
        // The next request exceeds the burst and must be limited.
        assert!(store.try_acquire("10.0.0.9", &config).await.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_store_exhaustion_returns_positive_retry_after() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: true,
        };
        let store = DeterministicRateLimitStore::new(&config);

        assert!(store.try_acquire("10.0.0.3", &config).await.is_ok());
        let retry_after = store
            .try_acquire("10.0.0.3", &config)
            .await
            .expect_err("second request must be limited");
        assert!(
            matches!(retry_after, RateLimitError::Limited(duration) if duration > Duration::ZERO)
        );
    }

    #[tokio::test]
    async fn test_in_memory_store_per_key_isolation() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: true,
        };
        let store = DeterministicRateLimitStore::new(&config);

        // First request from IP A — should succeed
        assert!(store.try_acquire("10.0.0.1", &config).await.is_ok());
        // First request from IP B — should also succeed (separate key state)
        assert!(store.try_acquire("10.0.0.2", &config).await.is_ok());
        // Second request from IP A — should be rate limited (burst_size=1)
        let result = store.try_acquire("10.0.0.1", &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_store_quota_refills_over_time() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: true,
        };
        let store = DeterministicRateLimitStore::new(&config);

        assert!(store.try_acquire("10.0.0.4", &config).await.is_ok());
        let retry_after = store
            .try_acquire("10.0.0.4", &config)
            .await
            .expect_err("second immediate request must be limited");
        let RateLimitError::Limited(retry_after) = retry_after else {
            panic!("second request must be limited");
        };
        store.advance(retry_after + Duration::from_nanos(1));
        assert!(
            store.try_acquire("10.0.0.4", &config).await.is_ok(),
            "quota must replenish after the monotonic clock reaches its retry instant"
        );
    }

    #[tokio::test]
    async fn test_in_memory_store_cleanup_stale_keeps_admission_correct() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: true,
        };
        let store = DeterministicRateLimitStore::new(&config);

        // Exhaust the burst for a key, then clean up: fresh (still-relevant)
        // state must survive so cleanup cannot be used to reset limits.
        assert!(store.try_acquire("10.0.0.5", &config).await.is_ok());
        assert!(store.try_acquire("10.0.0.5", &config).await.is_err());
        store.cleanup_stale();
        assert!(
            store.try_acquire("10.0.0.5", &config).await.is_err(),
            "cleanup must not reset a hot key's rate limit state"
        );
    }

    #[tokio::test]
    async fn test_in_memory_store_cleanup_stale_on_empty_store_is_noop() {
        let store = InMemoryRateLimitStore::new();
        // No limiter initialized yet — must not panic.
        store.cleanup_stale();
    }

    // -----------------------------------------------------------------------
    // build_rate_limit_response tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rate_limit_response_status_429() {
        let response = build_rate_limit_response("req_test123".to_string(), 5);
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_rate_limit_response_headers() {
        let response = build_rate_limit_response("req_test123".to_string(), 42);

        assert_eq!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        assert_eq!(
            response
                .headers()
                .get("retry-after")
                .unwrap()
                .to_str()
                .unwrap(),
            "42"
        );
    }

    #[tokio::test]
    async fn test_rate_limit_response_body_format() {
        let response = build_rate_limit_response("req_abc123".to_string(), 10);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["error"]["code"], "RATE_LIMITED");
        assert_eq!(
            body["error"]["message"],
            "Too many requests. Please retry after 10 seconds."
        );
        assert_eq!(body["error"]["details"], serde_json::json!({}));
        assert_eq!(body["error"]["requestId"], "req_abc123");
    }

    #[tokio::test]
    async fn unavailable_rate_limit_response_preserves_error_envelope_and_backoff() {
        let layer = RateLimitLayer::new(
            Arc::new(UnavailableRateLimitStore),
            RateLimitConfig::default(),
        );
        let svc = ServiceBuilder::new().layer(layer).service(EchoService);
        let mut req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(RequestId("req_rate_limit_unavailable".to_string()));

        let response = svc.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get("retry-after").unwrap(), "1");

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body["error"]["code"], "RATE_LIMIT_UNAVAILABLE");
        assert_eq!(body["error"]["requestId"], "req_rate_limit_unavailable");
    }

    // -----------------------------------------------------------------------
    // RateLimitLayer / RateLimitMiddleware integration tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_middleware_allows_request_within_limit() {
        let store = Arc::new(InMemoryRateLimitStore::new());
        let config = RateLimitConfig::default();
        let layer = RateLimitLayer::new(store, config);

        let svc = ServiceBuilder::new().layer(layer).service(EchoService);

        let req = Request::builder()
            .uri("/test")
            .extension(axum::extract::ConnectInfo(
                "10.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
            ))
            .body(Body::empty())
            .unwrap();

        let response = svc.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_middleware_returns_429_when_rate_limited() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: true,
        };
        let store = Arc::new(DeterministicRateLimitStore::new(&config));

        // First request should succeed
        {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.99:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();

            let response = svc.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // Second request from same IP should be rate limited
        {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.99:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();

            let response = svc.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

            // Verify Retry-After header is present
            assert!(response.headers().contains_key("retry-after"));

            // Verify response body
            let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
            assert_eq!(body["error"]["code"], "RATE_LIMITED");
        }
    }

    #[tokio::test]
    async fn test_middleware_disabled_passes_all_requests() {
        let store = Arc::new(InMemoryRateLimitStore::new());
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: false, // Disabled
        };

        // Even with burst_size=1, all requests should pass when disabled
        for _ in 0..5 {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();

            let response = svc.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn test_middleware_extracts_request_id_for_error_response() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: true,
        };
        let store = Arc::new(DeterministicRateLimitStore::new(&config));

        // Exhaust the bucket
        {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.50:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();

            svc.oneshot(req).await.unwrap();
        }

        // Second request — rate limited. Include a RequestId in extensions.
        {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let mut req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.50:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();
            req.extensions_mut()
                .insert(RequestId("req_test_correlation".to_string()));

            let response = svc.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

            let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
            assert_eq!(body["error"]["requestId"], "req_test_correlation");
        }
    }

    #[tokio::test]
    async fn test_middleware_different_ips_get_separate_buckets() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 1,
            enabled: true,
        };
        let store = Arc::new(DeterministicRateLimitStore::new(&config));

        // Exhaust bucket for IP A
        {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();

            svc.oneshot(req).await.unwrap();
        }

        // IP A should be rate limited
        {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();

            let response = svc.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }

        // IP B should still be allowed (separate bucket)
        {
            let layer = RateLimitLayer::new(store.clone(), config.clone());
            let svc = ServiceBuilder::new().layer(layer).service(EchoService);

            let req = Request::builder()
                .uri("/test")
                .extension(axum::extract::ConnectInfo(
                    "10.0.0.2:1234".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::empty())
                .unwrap();

            let response = svc.oneshot(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn test_retry_after_header_minimum_1() {
        // Drive the sub-second rejection deterministically. A real high-rate
        // governor bucket can refill between requests when CI deschedules this
        // test, turning the expected 429 into a legitimate 200.
        let store = Arc::new(RejectingRateLimitStore(Duration::from_millis(10)));
        let layer = RateLimitLayer::new(store, RateLimitConfig::default());
        let svc = ServiceBuilder::new().layer(layer).service(EchoService);

        let req = Request::builder()
            .uri("/test")
            .extension(axum::extract::ConnectInfo(
                "10.0.0.77:1234".parse::<std::net::SocketAddr>().unwrap(),
            ))
            .body(Body::empty())
            .unwrap();

        let response = svc.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        let retry_after: u64 = response
            .headers()
            .get("retry-after")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(retry_after, 1);
    }

    #[test]
    fn test_rate_limit_layer_is_clone() {
        let store = Arc::new(InMemoryRateLimitStore::new());
        let config = RateLimitConfig::default();
        let layer = RateLimitLayer::new(store, config);
        let _cloned = layer.clone();
    }
}
