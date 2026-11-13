//! `PostgresRateLimitStore` on the `rate_limit_windows` table: the backend of
//! the shared platform bug-report quota since Redis was removed
//! (G5Labs/G5-Lean#405). Every test owns an isolated database.

use crate::helpers;

use std::time::Duration;

use g5_api::middleware::{PostgresRateLimitStore, RateLimitConfig, RateLimitError, RateLimitStore};
use helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

const WINDOW_SECONDS: i64 = 60;

fn limit(requests_per_minute: u32) -> RateLimitConfig {
    RateLimitConfig {
        requests_per_minute,
        burst_size: requests_per_minute,
        enabled: true,
    }
}

async fn window_count(pool: &PgPool, key: &str) -> Option<i32> {
    sqlx::query_scalar("SELECT count FROM rate_limit_windows WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .expect("read quota row")
}

/// The store buckets on the database clock; a test that fills a window and
/// then expects a rejection must not straddle a bucket boundary.
async fn wait_for_fresh_window(pool: &PgPool) {
    loop {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT $1::bigint - (floor(extract(epoch FROM now()))::bigint % $1::bigint)",
        )
        .bind(WINDOW_SECONDS)
        .fetch_one(pool)
        .await
        .expect("read database clock");
        if remaining > 15 {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::test]
async fn single_key_admits_the_limit_then_rejects_without_charging() {
    let db = TestDb::create().await;
    wait_for_fresh_window(&db.pool).await;
    let store = PostgresRateLimitStore::new(db.pool.clone());

    for _ in 0..3 {
        store.try_acquire("k:single", &limit(3)).await.unwrap();
    }
    match store.try_acquire("k:single", &limit(3)).await {
        Err(RateLimitError::Limited(retry_after)) => assert!(
            retry_after >= Duration::from_secs(1)
                && retry_after <= Duration::from_secs(WINDOW_SECONDS as u64),
            "retry-after must point inside the current window, got {retry_after:?}"
        ),
        other => panic!("expected a window-full rejection, got {other:?}"),
    }
    assert_eq!(
        window_count(&db.pool, "k:single").await,
        Some(3),
        "a rejection never charges the key"
    );
    db.drop().await;
}

#[tokio::test]
async fn a_new_window_resets_the_key_in_place() {
    let db = TestDb::create().await;
    wait_for_fresh_window(&db.pool).await;
    let store = PostgresRateLimitStore::new(db.pool.clone());

    for _ in 0..2 {
        store.try_acquire("k:rollover", &limit(2)).await.unwrap();
    }
    assert!(matches!(
        store.try_acquire("k:rollover", &limit(2)).await,
        Err(RateLimitError::Limited(_))
    ));
    // Age the row into the previous bucket instead of waiting a minute.
    sqlx::query(
        "UPDATE rate_limit_windows SET window_start = window_start - make_interval(secs => $1) WHERE key = $2",
    )
    .bind(WINDOW_SECONDS as f64)
    .bind("k:rollover")
    .execute(&db.pool)
    .await
    .unwrap();
    store.try_acquire("k:rollover", &limit(2)).await.unwrap();
    assert_eq!(
        window_count(&db.pool, "k:rollover").await,
        Some(1),
        "the first charge of a new window starts the count over"
    );
    db.drop().await;
}

#[tokio::test]
async fn pair_rejection_on_the_second_key_leaves_the_first_uncharged() {
    let db = TestDb::create().await;
    wait_for_fresh_window(&db.pool).await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    // "pair:a" sorts first, so it is charged before "pair:b" rejects.
    for _ in 0..2 {
        store
            .try_acquire_pair("pair:a", &limit(10), "pair:b", &limit(2))
            .await
            .unwrap();
    }
    assert!(matches!(
        store
            .try_acquire_pair("pair:a", &limit(10), "pair:b", &limit(2))
            .await,
        Err(RateLimitError::Limited(_))
    ));
    assert_eq!(
        window_count(&db.pool, "pair:a").await,
        Some(2),
        "the first key's charge must roll back when the second key rejects"
    );
    assert_eq!(window_count(&db.pool, "pair:b").await, Some(2));
    // The same pair in the other argument order takes the same lock order.
    assert!(matches!(
        store
            .try_acquire_pair("pair:b", &limit(2), "pair:a", &limit(10))
            .await,
        Err(RateLimitError::Limited(_))
    ));
    assert_eq!(window_count(&db.pool, "pair:a").await, Some(2));
    db.drop().await;
}

#[tokio::test]
async fn a_disabled_config_is_skipped_and_the_other_key_still_counts() {
    let db = TestDb::create().await;
    wait_for_fresh_window(&db.pool).await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    let mut disabled = limit(1);
    disabled.enabled = false;
    store.try_acquire("k:disabled", &disabled).await.unwrap();
    assert_eq!(window_count(&db.pool, "k:disabled").await, None);
    store
        .try_acquire_pair("k:disabled", &disabled, "k:enabled", &limit(1))
        .await
        .unwrap();
    assert!(matches!(
        store
            .try_acquire_pair("k:disabled", &disabled, "k:enabled", &limit(1))
            .await,
        Err(RateLimitError::Limited(_))
    ));
    assert_eq!(window_count(&db.pool, "k:disabled").await, None);
    assert_eq!(window_count(&db.pool, "k:enabled").await, Some(1));
    db.drop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_acquirers_on_one_key_admit_exactly_the_limit() {
    let db = TestDb::create().await;
    wait_for_fresh_window(&db.pool).await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let store = store.clone();
        tasks.spawn(async move { store.try_acquire("k:concurrent", &limit(10)).await });
    }
    let mut admitted = 0;
    let mut limited = 0;
    while let Some(result) = tasks.join_next().await {
        match result.unwrap() {
            Ok(()) => admitted += 1,
            Err(RateLimitError::Limited(_)) => limited += 1,
            Err(RateLimitError::Unavailable) => panic!("the backend must stay available"),
        }
    }
    assert_eq!((admitted, limited), (10, 22));
    assert_eq!(window_count(&db.pool, "k:concurrent").await, Some(10));
    db.drop().await;
}

#[tokio::test]
async fn an_unreachable_database_reports_unavailable_so_callers_fail_closed() {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(3))
        .connect_lazy("postgres://g5:nobody@127.0.0.1:9/nowhere")
        .expect("lazy pool");
    let store = PostgresRateLimitStore::new(pool);
    assert_eq!(
        store
            .try_acquire_pair("k:a", &limit(10), "k:b", &limit(10))
            .await,
        Err(RateLimitError::Unavailable)
    );
    assert_eq!(
        store.try_acquire("k:a", &limit(10)).await,
        Err(RateLimitError::Unavailable)
    );
}

#[tokio::test]
async fn trim_removes_only_keys_idle_for_a_day() {
    let db = TestDb::create().await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    store.try_acquire("k:fresh", &limit(5)).await.unwrap();
    sqlx::query(
        "INSERT INTO rate_limit_windows (key, window_start, count) VALUES ('k:idle', now() - interval '2 days', 4)",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        PostgresRateLimitStore::trim_stale(&db.pool).await.unwrap(),
        1
    );
    assert_eq!(window_count(&db.pool, "k:idle").await, None);
    assert_eq!(window_count(&db.pool, "k:fresh").await, Some(1));
    db.drop().await;
}

/// A request that formed its bucket before waiting on the row lock resumes
/// after a newer window was opened. It must be charged
/// to the stored newer window, never reset it backward to a count of one.
#[tokio::test]
async fn a_stale_bucket_never_resets_a_newer_window_backward() {
    let db = TestDb::create().await;
    wait_for_fresh_window(&db.pool).await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    let next_bucket = "to_timestamp((floor(extract(epoch FROM clock_timestamp()) / 60) * 60)::double precision) + interval '60 seconds'";

    // The stored window is one bucket ahead of this process's clock and full:
    // exactly what a lock-wait across the boundary leaves behind.
    sqlx::query(&format!(
        "INSERT INTO rate_limit_windows (key, window_start, count) VALUES ('k:stale-full', {next_bucket}, 3)"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    match store.try_acquire("k:stale-full", &limit(3)).await {
        Err(RateLimitError::Limited(retry_after)) => assert!(
            retry_after > Duration::from_secs(WINDOW_SECONDS as u64)
                && retry_after <= Duration::from_secs(2 * WINDOW_SECONDS as u64),
            "retry-after must point at the end of the stored (newer) window, got {retry_after:?}"
        ),
        other => panic!("a full newer window must reject, got {other:?}"),
    }
    let (window_in_future, count): (bool, i32) = sqlx::query_as(
        "SELECT window_start > clock_timestamp(), count FROM rate_limit_windows WHERE key = 'k:stale-full'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(
        window_in_future,
        "the newer window must survive an older-bucket request"
    );
    assert_eq!(count, 3, "a rejected older-bucket request charges nothing");

    // With room left, the older-bucket request is charged to the newer window.
    sqlx::query(&format!(
        "INSERT INTO rate_limit_windows (key, window_start, count) VALUES ('k:stale-room', {next_bucket}, 2)"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    store.try_acquire("k:stale-room", &limit(3)).await.unwrap();
    let (window_in_future, count): (bool, i32) = sqlx::query_as(
        "SELECT window_start > clock_timestamp(), count FROM rate_limit_windows WHERE key = 'k:stale-room'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(window_in_future);
    assert_eq!(count, 3, "the admission counts against the newer window");
    assert!(matches!(
        store.try_acquire("k:stale-room", &limit(3)).await,
        Err(RateLimitError::Limited(_))
    ));
    db.drop().await;
}

/// The window is sampled only after every row lock is
/// held, so a pair that waits for a lock across a minute boundary is charged,
/// for both keys, to the window current at admission. The test drives a real
/// boundary while the pair is provably waiting on the row lock, so it can
/// take a minute, or a few when a boundary slips past the setup.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pair_blocked_across_a_boundary_is_charged_to_the_window_current_at_admission() {
    let db = TestDb::create().await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    // The lock holder and the clock polling use their own connections so the
    // store's pool is never exhausted while the pair is blocked.
    let aux = PgPoolOptions::new()
        .max_connections(3)
        .connect(&TestDb::url_for_database(&db.db_name).await)
        .await
        .expect("auxiliary pool");
    let bucket_number = |pool: &PgPool| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, f64>(
                "SELECT floor(extract(epoch FROM clock_timestamp()) / 60)::double precision",
            )
            .fetch_one(&pool)
            .await
            .unwrap() as i64
        }
    };
    // Seed both keys in the current window with room to spare.
    wait_for_fresh_window(&db.pool).await;
    store
        .try_acquire_pair("pb:global", &limit(100), "pb:reporter", &limit(10))
        .await
        .unwrap();
    let mut seeded = bucket_number(&aux).await;
    // A backend other than this poller waiting on a lock in a statement on
    // the quota table: the pair has reached the row lock.
    let pair_waits_on_the_lock = |pool: &PgPool| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM pg_stat_activity
                  WHERE datname = current_database() AND pid <> pg_backend_pid()
                    AND wait_event_type = 'Lock' AND query LIKE '%rate_limit_windows%'",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
                > 0
        }
    };
    // Hold the platform key's row lock from another connection inside the
    // last second of the seeded window, so the blocked pair waits well
    // under the store's lock bound, then spawn the pair and watch it reach
    // the lock before the boundary. A pair that only starts after the
    // boundary would be charged to the new window without ever waiting and
    // prove nothing, so if the boundary slips past the lock or the pair's
    // start, release, re-seed in a fresh window and try the next boundary.
    let mut attempts = 0;
    let (holder, blocked) = loop {
        attempts += 1;
        assert!(
            attempts <= 3,
            "the pair never reached the row lock inside a seeded window in {} attempts",
            attempts - 1
        );
        loop {
            let remaining: i64 = sqlx::query_scalar(
                "SELECT 60 - (floor(extract(epoch FROM clock_timestamp()))::bigint % 60)",
            )
            .fetch_one(&aux)
            .await
            .unwrap();
            if remaining <= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let mut holder = aux.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM rate_limit_windows WHERE key = 'pb:global' FOR UPDATE")
            .execute(&mut *holder)
            .await
            .unwrap();
        let mut reached_the_lock = false;
        let mut blocked = None;
        if bucket_number(&aux).await == seeded {
            blocked = Some(tokio::spawn({
                let store = store.clone();
                async move {
                    store
                        .try_acquire_pair("pb:global", &limit(100), "pb:reporter", &limit(10))
                        .await
                }
            }));
            // Let the boundary pass while the pair waits on the lock. An
            // observation counts only if the window is still the seeded one
            // after it: an activity query that completes past the boundary
            // could see a pair that began waiting in the new window.
            while bucket_number(&aux).await == seeded {
                if !reached_the_lock
                    && pair_waits_on_the_lock(&aux).await
                    && bucket_number(&aux).await == seeded
                {
                    reached_the_lock = true;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        if reached_the_lock {
            break (holder, blocked.unwrap());
        }
        // The boundary passed before the pair waited on the lock (or before
        // the lock was taken): release, let the pair finish in the new
        // window, re-seed in a fresh window and retry.
        holder.rollback().await.unwrap();
        if let Some(blocked) = blocked {
            blocked.await.unwrap().unwrap();
        }
        wait_for_fresh_window(&db.pool).await;
        store
            .try_acquire_pair("pb:global", &limit(100), "pb:reporter", &limit(10))
            .await
            .unwrap();
        seeded = bucket_number(&aux).await;
    };
    assert!(
        !blocked.is_finished(),
        "the pair must still be waiting on the row lock"
    );
    holder.rollback().await.unwrap();
    blocked.await.unwrap().unwrap();

    let rows: Vec<(String, f64, i32)> = sqlx::query_as(
        "SELECT key, floor(extract(epoch FROM window_start) / 60)::double precision, count
           FROM rate_limit_windows WHERE key IN ('pb:global', 'pb:reporter') ORDER BY key",
    )
    .fetch_all(&aux)
    .await
    .unwrap();
    let windows: Vec<i64> = rows.iter().map(|(_, w, _)| *w as i64).collect();
    let counts: Vec<i32> = rows.iter().map(|(_, _, c)| *c).collect();
    assert_eq!(
        windows,
        vec![seeded + 1, seeded + 1],
        "both keys belong to the window current at admission, not the one the pair started in"
    );
    assert_eq!(
        counts,
        vec![1, 1],
        "the admission opened the new window for both keys"
    );
    db.drop().await;
}

/// The row is created and locked in one statement, so an
/// acquisition that meets a concurrent trim of its idle row waits for that
/// delete and then opens a fresh window, instead of answering without a row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_acquisition_racing_the_trim_of_its_idle_row_still_admits() {
    let db = TestDb::create().await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    let aux = PgPoolOptions::new()
        .max_connections(3)
        .connect(&TestDb::url_for_database(&db.db_name).await)
        .await
        .expect("auxiliary pool");
    sqlx::query(
        "INSERT INTO rate_limit_windows (key, window_start, count) VALUES ('k:idle-race', now() - interval '2 days', 7)",
    )
    .execute(&aux)
    .await
    .unwrap();
    // The trim's delete of the idle row is in flight while the acquisition starts.
    let mut trimming = aux.begin().await.unwrap();
    sqlx::query("DELETE FROM rate_limit_windows WHERE key = 'k:idle-race'")
        .execute(&mut *trimming)
        .await
        .unwrap();
    let racing = tokio::spawn({
        let store = store.clone();
        async move { store.try_acquire("k:idle-race", &limit(3)).await }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !racing.is_finished(),
        "the acquisition waits for the in-flight delete instead of answering without a row"
    );
    trimming.commit().await.unwrap();
    racing.await.unwrap().unwrap();
    assert_eq!(
        window_count(&aux, "k:idle-race").await,
        Some(1),
        "the acquisition recreated the row and opened a fresh window"
    );
    db.drop().await;
}

/// The trim never waits for a row an acquisition holds, so
/// cleanup and a pair that locks its keys in order can never deadlock. A held
/// idle row is skipped and removed on a later pass.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_trim_skips_rows_held_by_an_acquisition_instead_of_waiting() {
    let db = TestDb::create().await;
    let aux = PgPoolOptions::new()
        .max_connections(3)
        .connect(&TestDb::url_for_database(&db.db_name).await)
        .await
        .expect("auxiliary pool");
    sqlx::query(
        "INSERT INTO rate_limit_windows (key, window_start, count)
         VALUES ('t:held', now() - interval '2 days', 1), ('t:free', now() - interval '2 days', 1)",
    )
    .execute(&aux)
    .await
    .unwrap();
    let mut holder = aux.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM rate_limit_windows WHERE key = 't:held' FOR UPDATE")
        .execute(&mut *holder)
        .await
        .unwrap();
    let trimmed = tokio::time::timeout(
        Duration::from_secs(5),
        PostgresRateLimitStore::trim_stale(&db.pool),
    )
    .await
    .expect("the trim must not wait for the held row")
    .unwrap();
    assert_eq!(
        trimmed, 1,
        "only the free idle row is removed while the held one stays"
    );
    holder.rollback().await.unwrap();
    assert_eq!(window_count(&aux, "t:held").await, Some(1));
    assert_eq!(window_count(&aux, "t:free").await, None);
    assert_eq!(
        PostgresRateLimitStore::trim_stale(&db.pool).await.unwrap(),
        1,
        "the next pass removes the released idle row"
    );
    db.drop().await;
}

/// A quota row held by a stalled transaction
/// must not pin an acquisition (and its pool connection) indefinitely. The
/// lock wait is bounded and the acquisition fails closed as `Unavailable`;
/// once the holder releases, the next acquisition proceeds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_held_quota_row_fails_closed_within_the_lock_bound_instead_of_hanging() {
    let db = TestDb::create().await;
    // The final count assertion assumes one window; keep the ~4 s of waits
    // away from a minute boundary.
    wait_for_fresh_window(&db.pool).await;
    let store = PostgresRateLimitStore::new(db.pool.clone());
    let aux = PgPoolOptions::new()
        .max_connections(3)
        .connect(&TestDb::url_for_database(&db.db_name).await)
        .await
        .expect("auxiliary pool");
    store
        .try_acquire("k:held-timeout", &limit(3))
        .await
        .unwrap();
    let mut holder = aux.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM rate_limit_windows WHERE key = 'k:held-timeout' FOR UPDATE")
        .execute(&mut *holder)
        .await
        .unwrap();

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        store.try_acquire("k:held-timeout", &limit(3)),
    )
    .await
    .expect("the acquisition must give up within the lock bound, never hang");
    let waited = started.elapsed();
    assert_eq!(
        outcome,
        Err(RateLimitError::Unavailable),
        "a held row fails closed"
    );
    assert!(
        waited >= Duration::from_secs(2) && waited <= Duration::from_secs(12),
        "the wait is bounded by lock_timeout, got {waited:?}"
    );
    assert_eq!(
        window_count(&aux, "k:held-timeout").await,
        Some(1),
        "nothing was charged"
    );

    holder.rollback().await.unwrap();
    store
        .try_acquire("k:held-timeout", &limit(3))
        .await
        .unwrap();
    assert_eq!(window_count(&aux, "k:held-timeout").await, Some(2));
    db.drop().await;
}
