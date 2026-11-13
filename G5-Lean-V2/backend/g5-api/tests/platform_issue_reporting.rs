//! Route-level platform-report acceptance proof.
//!
//! These tests run the real API router, auth middleware, the Postgres-backed
//! shared quota (`rate_limit_windows`), and the Postgres outbox. GitHub delivery remains a separate concern of
//! `platform_issue_delivery_recovery.rs`; this file proves that the HTTP
//! boundary fails closed, accepts an authenticated user, and rejects malformed
//! screenshot content before enqueueing it.

use crate::helpers;

use std::time::Duration;

use helpers::{test_config, TestApp, TestUser, TEST_PRIVATE_KEY_PEM};
use reqwest::StatusCode;
use serde_json::json;
use sqlx::Row;

const PLATFORM_QUOTA_WINDOW_SECS: i64 = 60;
/// Filling a 100-request window takes a few seconds of real requests; never
/// start that close to a bucket boundary.
const PLATFORM_QUOTA_MIN_REMAINING_SECS: i64 = 25;

fn enabled_platform_issue_config() -> g5_api::config::AppConfig {
    let mut config = test_config();
    config.platform_issue.enabled = true;
    config.platform_issue.github_repo = Some("g5labs/platform-bugs".to_string());
    config.platform_issue.github_app_id = Some("123".to_string());
    config.platform_issue.github_app_installation_id = Some("987".to_string());
    // The route only builds the delivery client; it does not mint a GitHub App
    // token. The recovery test owns the real RSA signing/delivery proof.
    config.platform_issue.github_app_private_key = Some(TEST_PRIVATE_KEY_PEM.to_string());
    config.platform_issue.github_api_base_url = "http://127.0.0.1:9".to_string();
    config
}

fn multipart_report(
    report: serde_json::Value,
    screenshot: Option<(&str, &[u8])>,
) -> (String, Vec<u8>) {
    let boundary = "g5-platform-issue-test-boundary";
    let mut body = Vec::new();

    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"report\"\r\n");
    body.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    body.extend_from_slice(report.to_string().as_bytes());
    body.extend_from_slice(b"\r\n");

    if let Some((content_type, bytes)) = screenshot {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"screenshot\"; filename=\"screen.bin\"\r\n",
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }

    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn report_payload() -> serde_json::Value {
    json!({
        "title": "Report route accepts an authenticated user",
        "body": "The outbox must retain the report before delivery.",
        "kind": "bug",
        "severity": "high",
        "context": { "route": "/workspaces/demo", "appVersion": "test" }
    })
}

fn valid_2_to_5_mib_png() -> Vec<u8> {
    use std::io::Cursor;

    // Deterministic RGB noise keeps the PNG's wire size representative of a
    // real screenshot rather than letting DEFLATE collapse a uniform fixture.
    // The fixed alpha channel gives a stable ~3 MiB payload, within the route's
    // allowed 2–5 MiB range, while its decoded image remains only 4 MiB.
    let image = image::RgbaImage::from_fn(1024, 1024, |x, y| {
        let mut value = (u64::from(x) << 32) ^ u64::from(y) ^ 0x9E37_79B9_7F4A_7C15;
        value ^= value >> 30;
        value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^= value >> 31;
        image::Rgba([
            value as u8,
            (value >> 8) as u8,
            (value >> 16) as u8,
            u8::MAX,
        ])
    });
    let mut encoded = Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .expect("encode representative PNG fixture");
    let bytes = encoded.into_inner();
    assert!(
        (2 * 1024 * 1024..=5 * 1024 * 1024).contains(&bytes.len()),
        "fixture must exercise the accepted 2–5 MiB range; got {} bytes",
        bytes.len()
    );
    bytes
}

/// Every test app owns an isolated database, so the shared quota table starts
/// empty. Only the window boundary can leak between the filling requests and
/// the assertion; this wait, on the same database clock the store uses, rules
/// that out.
async fn wait_for_fresh_platform_quota_window(app: &TestApp) {
    loop {
        let seconds_remaining: i64 = sqlx::query_scalar(
            "SELECT $1::bigint - (floor(extract(epoch FROM now()))::bigint % $1::bigint)",
        )
        .bind(PLATFORM_QUOTA_WINDOW_SECS)
        .fetch_one(&app.db)
        .await
        .expect("read database time for deterministic quota test window");
        if seconds_remaining > PLATFORM_QUOTA_MIN_REMAINING_SECS {
            return;
        }
        tokio::time::sleep(Duration::from_secs((seconds_remaining + 1) as u64)).await;
    }
}

async fn quota_row_count(app: &TestApp, key: &str) -> Option<i32> {
    sqlx::query_scalar("SELECT count FROM rate_limit_windows WHERE key = $1")
        .bind(key)
        .fetch_optional(&app.db)
        .await
        .expect("read shared quota row")
}

#[tokio::test]
async fn platform_report_route_fails_closed_when_delivery_is_not_configured() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);
    let (content_type, body) = multipart_report(report_payload(), None);

    let response = client
        .post_body("/api/v1/platform/issues", &content_type, body)
        .await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    app.cleanup().await;
}

#[tokio::test]
async fn platform_report_route_persists_valid_user_reports_and_rejects_non_png_screenshots() {
    let app = TestApp::spawn_with_config(enabled_platform_issue_config()).await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);
    let valid_png = valid_2_to_5_mib_png();

    let mut report_with_raw_route = report_payload();
    report_with_raw_route["context"]["route"] =
        serde_json::Value::String("/workspaces/demo?token=secret#fragment".to_string());
    let (content_type, body) =
        multipart_report(report_with_raw_route, Some(("image/png", &valid_png)));
    let response = client
        .post_body("/api/v1/platform/issues", &content_type, body)
        .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let accepted = sqlx::query(
        "SELECT reporter_user_id, reporter_display_name, report_base_url, screenshot_mime, \
                octet_length(screenshot_bytes) AS screenshot_size, \
                context ->> 'route' AS route \
         FROM platform_issue_outbox",
    )
    .fetch_one(&app.db)
    .await
    .expect("read accepted report");
    assert_eq!(accepted.get::<uuid::Uuid, _>("reporter_user_id"), user.id);
    // The reporter's human-readable name is snapshotted for issue-body triage.
    assert_eq!(
        accepted
            .get::<Option<String>, _>("reporter_display_name")
            .as_deref(),
        Some(user.display_name.as_str()),
    );
    // The deployment base URL is snapshotted so the issue can linkify the route.
    let base_url = accepted
        .get::<Option<String>, _>("report_base_url")
        .expect("report_base_url snapshotted at intake");
    assert!(
        base_url.starts_with("http"),
        "expected an absolute base URL snapshot, got {base_url:?}",
    );
    assert_eq!(
        accepted
            .get::<Option<String>, _>("screenshot_mime")
            .as_deref(),
        Some("image/png")
    );
    assert_eq!(
        accepted.get::<Option<i32>, _>("screenshot_size"),
        Some(valid_png.len() as i32)
    );
    assert_eq!(
        accepted.get::<Option<String>, _>("route").as_deref(),
        Some("/workspaces/demo"),
        "the server must redact raw query and fragment data from direct callers"
    );

    let (content_type, body) = multipart_report(
        report_payload(),
        Some(("image/jpeg", b"\x89PNG\r\n\x1a\nnot-a-jpeg")),
    );
    let response = client
        .post_body("/api/v1/platform/issues", &content_type, body)
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_issue_outbox")
        .fetch_one(&app.db)
        .await
        .expect("count outbox reports");
    assert_eq!(count, 1, "invalid screenshot must not enqueue a report");

    let over_limit = vec![b'x'; 6 * 1024 * 1024];
    let response = client
        .post_body(
            "/api/v1/platform/issues",
            "multipart/form-data; boundary=g5-platform-issue-test-boundary",
            over_limit,
        )
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_issue_outbox")
        .fetch_one(&app.db)
        .await
        .expect("count outbox reports after body-limit rejection");
    assert_eq!(count, 1, "over-limit payload must not enqueue a report");

    let mut oversized_context = report_payload();
    oversized_context["context"]["route"] = serde_json::Value::String("x".repeat(64 * 1024));
    let (content_type, body) = multipart_report(oversized_context, None);
    let response = client
        .post_body("/api/v1/platform/issues", &content_type, body)
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_issue_outbox")
        .fetch_one(&app.db)
        .await
        .expect("count outbox reports after context-size rejection");
    assert_eq!(count, 1, "oversized context must not enqueue a report");

    app.cleanup().await;
}

#[tokio::test]
async fn platform_report_route_enforces_the_shared_global_quota_across_users() {
    let app = TestApp::spawn_with_config(enabled_platform_issue_config()).await;
    wait_for_fresh_platform_quota_window(&app).await;

    for _ in 0..10 {
        let user = TestUser::create(&app).await;
        let client = app.authenticated_client(&user);
        for _ in 0..10 {
            let (content_type, body) = multipart_report(report_payload(), None);
            let response = client
                .post_body("/api/v1/platform/issues", &content_type, body)
                .await;
            assert_eq!(response.status(), StatusCode::ACCEPTED);
        }
    }

    // This reporter has not consumed its own 10/minute allowance. Its 429
    // therefore proves the single platform-global 100/minute quota, decided by
    // the shared `rate_limit_windows` row every replica charges.
    let next_user = TestUser::create(&app).await;
    let next_client = app.authenticated_client(&next_user);
    let (content_type, body) = multipart_report(report_payload(), None);
    let response = next_client
        .post_body("/api/v1/platform/issues", &content_type, body)
        .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_issue_outbox")
        .fetch_one(&app.db)
        .await
        .expect("count accepted reports after quota exhaustion");
    assert_eq!(count, 100, "the globally limited request must not enqueue");
    assert_eq!(
        quota_row_count(&app, "g5:rate-limit:{platform-issues}:global").await,
        Some(100),
        "the rejected request must not charge the global key"
    );
    assert_eq!(
        quota_row_count(
            &app,
            &format!(
                "g5:rate-limit:{{platform-issues}}:reporter:{}",
                next_user.id
            )
        )
        .await,
        None,
        "a pair rejected on the global key must not charge the reporter key"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn platform_report_route_enforces_the_per_reporter_quota() {
    let app = TestApp::spawn_with_config(enabled_platform_issue_config()).await;
    wait_for_fresh_platform_quota_window(&app).await;
    let user = TestUser::create(&app).await;
    let client = app.authenticated_client(&user);

    for _ in 0..10 {
        let (content_type, body) = multipart_report(report_payload(), None);
        let response = client
            .post_body("/api/v1/platform/issues", &content_type, body)
            .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    let (content_type, body) = multipart_report(report_payload(), None);
    let response = client
        .post_body("/api/v1/platform/issues", &content_type, body)
        .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        quota_row_count(&app, "g5:rate-limit:{platform-issues}:global").await,
        Some(10),
        "a pair rejected on the reporter key must not charge the global key"
    );

    app.cleanup().await;
}
