//! Real-Postgres recovery proof for platform-report delivery idempotency.
//!
//! The only mocked boundary is GitHub itself. The outbox, claim transition,
//! recovery write, and final persisted status all run against an isolated
//! migrated Postgres database.

use crate::helpers;

use std::sync::Arc;

use helpers::TestDb;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use g5_api::services::{
    PlatformIssueGithubClient, PlatformIssueGithubConfig, PlatformIssueIntakeService,
    PlatformIssueReport, PlatformIssueScreenshot,
};

const TEST_GITHUB_APP_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQCqiVnFPs/rD57C\nOPHD6xoZZE9vn5sa05AFNwj/dwq7NTzF7gTlZ6HVBQm9CJrzOA6emE6F2TPL3JWG\nKS5BddbkxwAlUnOK+gSFonZwGJV4ddZQtYpxeueWG/Ah5DJRvWCr3XHM4nt/09ss\nmMPNxOZ4Mr/deEBNkpneOR8VFW+opqukxNJSCw/Q5ZWfI19M+KJi6GG1pgFir00a\nQavMgBm4Mv9Kx2J79329XCQpYWT4OqnE6LpYJ61AycDHUvm6xEbJ9FXifYxM3af0\nn5nKXzM/a3VrjUjGwDjGU+akYdHwWxsaupJu72QgB1orY6DMou3ZA+dCTQBh1uZ0\nK26K+1M5AgMBAAECggEACNM9zYrQGLKAvT1UKZbfjWPXBohEbxF2uQZdlVbUO9bn\n87g7EaIYylzN4bXxytdiTkvEeIHCf/DE3iIAbkvKWKGBvOy05QLGPCIprehfx9GV\nSyzx4qiBFpAQj7hu8DL0L0Cw9HDeAkH07CxlqZ7gbGT3796auUj2ZH0hQiaPVjNg\nHqaZvcTsMCF9Yg09q03iruNxinpzfIIASS4CLJNhH8Eqa7BQZwoaNMfN8cvrP/aC\n2v6A/etHfc1cZby1zSDGf6LBZ7F8t1rCjiBpqWQEI+WaNBou0kSb2xjhu8s3Bjzh\n1ilbrZNYUwLcrkV+moKWquz7+o9MIhJFkpmXYfz1jwKBgQDnV8UUjutPDdKsJ3td\n+jiHfzhn3eAzbA1583A9pLfvhSOK4QIJJ/odXCseHC6Bder1fEiBw2VCjqNkJ4Kq\nrAWzMoWOb23HDW031HR6e39WBF/npKpwRV8Df6db5htgyecFQxhGUmkwjPJc+/7a\ndNKUtn4VK1pzDS+YwcPzHINOqwKBgQC8tng4UYKQL02F+lht+jR2d5Yg35Xze0Gj\nNNXuu+mzrmpI+9mlnVIps4E1CDCB6yzOoHXzoiMd9iIepT2HkZaPoqkWJOhDFvHI\n3UFeMzv9F0kj5ZQVzRghoo5v5Vdm0Ahdn4kZILY3UmEPqxjF48hCEuLl8DQ1I48j\nXFdn1nVVqwKBgQCE9ZrdX+65trYYH3D16DPJHEMs3wZWR4nPRyyBIURdsuVkkkF7\nRbCoYyqc2ZGY8k6h3FZHzidKg3RAwbyFnux/u0+rNzuepYbaCY8QPh5HL6h3F9ql\nQBmFhz6GwM+HZ0MRcShockDmmHtLGcG1iGLm+aOpQwtvCGsMnoCiHwcQlQKBgQC0\ngNxREjYOcBB43FKFL1PLFOu2meY2NGk+EKWoanlJcsUa9jriXK5gUOB/t0uemTIM\nmhL4bNemKu3L3kYSTEF1NiMBmV/H/ERc2rVRwzPYC9Pb0BZrG/e/BFu1z+pf3xCB\nbL2YGV/xXoyUjhVuA0hjWulJAw8M3Oxm6EZ6fflRbQKBgQCMsAdlFb3PV56gh14V\nYtr+VG/wSj9rvsR7BgQ/sma3mY5Deqj2+Lw7bOBVZTfXQkBzOk6g9UvYtY1hr2Af\ne0IuUQQoAHYjz5t//9wYhkXYfvII4apAbfwGYFLhSbTCVXfymCZAGn7gUCkXZR4l\nigpSb+iNKO+xB1posjWgU4iFZA==\n-----END PRIVATE KEY-----\n";

fn github_config(github: &MockServer) -> PlatformIssueGithubConfig {
    PlatformIssueGithubConfig {
        owner: "g5labs".to_string(),
        repo: "platform-bugs".to_string(),
        app_id: "123".to_string(),
        installation_id: "987".to_string(),
        private_key_pem: TEST_GITHUB_APP_PRIVATE_KEY.to_string(),
        api_base_url: github.uri(),
        web_base_url: "https://github.example".into(),
    }
}

fn intake_service(test_db: &TestDb, github: &MockServer) -> PlatformIssueIntakeService {
    PlatformIssueIntakeService::new(
        test_db.pool.clone(),
        Arc::new(PlatformIssueGithubClient::new(
            g5_scm::providers::ProviderClients::new().unwrap(),
            github_config(github),
        )),
        None,
    )
}

fn report(title: &str, screenshot: Option<PlatformIssueScreenshot>) -> PlatformIssueReport {
    PlatformIssueReport {
        reporter_user_id: Uuid::from_u128(0xBEEF),
        reporter_display_name: None,
        report_base_url: None,
        title: title.to_string(),
        body: "The worker must durably recover this report.".to_string(),
        kind: "bug".to_string(),
        severity: "high".to_string(),
        context: json!({ "route": "/workspaces/demo" }),
        screenshot,
    }
}

async fn mount_installation_token(github: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/app/installations/987/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "token": "test-token" })))
        .expect(1)
        .mount(github)
        .await;
}

#[tokio::test]
async fn marker_lookup_adopts_prior_issue_without_a_second_create() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report("Prior issue already exists", None))
        .await
        .expect("enqueue outbox row");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .and(query_param(
            "q",
            format!(
                "repo:g5labs/platform-bugs type:issue in:body \"<!-- g5-report:{submission_id} -->\""
            ),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {
                    "number": 7,
                    "html_url": "https://github.example/g5labs/platform-bugs/issues/7",
                    "body": "A search hit without the hidden outbox marker must not be adopted."
                },
                {
                    "number": 4242,
                    "html_url": "https://github.example/g5labs/platform-bugs/issues/4242",
                    "body": format!("Recovered report <!-- g5-report:{submission_id} -->")
                }
            ]
        })))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&github)
        .await;

    let result = service.process_due(1).await.expect("process due row");
    assert_eq!(result.claimed, 1);
    assert_eq!(result.completed, 1);
    assert_eq!(result.failed, 0);

    let row = sqlx::query(
        "SELECT outcome, github_issue_number, github_issue_url FROM platform_issue_delivery_tombstone WHERE submission_id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read recovered tombstone");
    assert_eq!(row.get::<String, _>("outcome"), "completed");
    assert_eq!(row.get::<Option<i64>, _>("github_issue_number"), Some(4242));
    assert_eq!(
        row.get::<Option<String>, _>("github_issue_url").as_deref(),
        Some("https://github.example/g5labs/platform-bugs/issues/4242")
    );

    test_db.drop().await;
}

#[tokio::test]
async fn failed_delivery_transitions_from_claim_to_backoff_then_retry_completion() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report("Retry a pre-create GitHub failure", None))
        .await
        .expect("enqueue outbox row");

    // Installation-token failure occurs before marker lookup and before the
    // irreversible issue-create fence. It must follow the ordinary failed /
    // backoff path rather than reconciliation.
    let failed_token = Mock::given(method("POST"))
        .and(path("/app/installations/987/access_tokens"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({
            "message": "temporary GitHub App outage"
        })))
        .expect(1)
        .mount_as_scoped(&github)
        .await;

    let first = service
        .process_due(1)
        .await
        .expect("pre-create failure is persisted");
    assert_eq!((first.claimed, first.completed, first.failed), (1, 0, 1));

    let failed = sqlx::query(
        "SELECT status, attempts, locked_at IS NULL AS unlocked, \
                next_run_at > now() AS deferred, \
                github_create_started_at IS NULL AS unfenced \
         FROM platform_issue_outbox WHERE id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read failed row");
    assert_eq!(failed.get::<String, _>("status"), "failed");
    assert_eq!(failed.get::<i32, _>("attempts"), 1);
    assert!(failed.get::<bool, _>("unlocked"));
    assert!(failed.get::<bool, _>("deferred"));
    assert!(failed.get::<bool, _>("unfenced"));

    drop(failed_token);
    sqlx::query("UPDATE platform_issue_outbox SET next_run_at = now() WHERE id = $1")
        .bind(submission_id)
        .execute(&test_db.pool)
        .await
        .expect("make failed row due for retry");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "number": 5150,
            "html_url": "https://github.example/g5labs/platform-bugs/issues/5150"
        })))
        .expect(1)
        .mount(&github)
        .await;

    let second = service
        .process_due(1)
        .await
        .expect("retry completes delivery");
    assert_eq!((second.claimed, second.completed, second.failed), (1, 1, 0));

    let tombstone = sqlx::query(
        "SELECT outcome, attempts, github_issue_number, github_issue_url \
         FROM platform_issue_delivery_tombstone WHERE submission_id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read completed retry tombstone");
    assert_eq!(tombstone.get::<String, _>("outcome"), "completed");
    assert_eq!(tombstone.get::<i32, _>("attempts"), 2);
    assert_eq!(
        tombstone.get::<Option<i64>, _>("github_issue_number"),
        Some(5150)
    );
    assert_eq!(
        tombstone
            .get::<Option<String>, _>("github_issue_url")
            .as_deref(),
        Some("https://github.example/g5labs/platform-bugs/issues/5150")
    );

    test_db.drop().await;
}

#[tokio::test]
async fn create_error_enters_reconciliation_without_a_second_post() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report("Retry after a GitHub outage", None))
        .await
        .expect("enqueue outbox row");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .up_to_n_times(2)
        .expect(2)
        .mount(&github)
        .await;
    // The issue POST has no idempotency key. Its error is therefore ambiguous
    // after the durable create fence and must never permit another POST.
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({ "message": "outage" })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&github)
        .await;

    let first = service.process_due(1).await.expect("first drain completes");
    assert_eq!((first.claimed, first.completed, first.failed), (1, 0, 1));

    let failed = sqlx::query(
        "SELECT status, attempts, locked_at IS NULL AS unlocked, next_run_at > now() AS deferred, github_create_started_at IS NOT NULL AS fenced \
         FROM platform_issue_outbox WHERE id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read failed row");
    assert_eq!(failed.get::<String, _>("status"), "reconciling");
    assert_eq!(failed.get::<i32, _>("attempts"), 1);
    assert!(failed.get::<bool, _>("unlocked"));
    assert!(failed.get::<bool, _>("deferred"));
    assert!(failed.get::<bool, _>("fenced"));

    // Advance only this durable row to its scheduled retry point; the second
    // `process_due` call then reclaims the row through the production query.
    sqlx::query("UPDATE platform_issue_outbox SET next_run_at = now() WHERE id = $1")
        .bind(submission_id)
        .execute(&test_db.pool)
        .await
        .expect("make failed row due for retry");

    let second = service
        .process_due(1)
        .await
        .expect("reconciliation drain completes");
    assert_eq!((second.claimed, second.completed, second.failed), (1, 0, 1));

    let reconciling = sqlx::query(
        "SELECT status, attempts, github_issue_number, github_issue_url \
         FROM platform_issue_outbox WHERE id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read retained reconciliation row");
    assert_eq!(reconciling.get::<String, _>("status"), "reconciling");
    assert_eq!(reconciling.get::<i32, _>("attempts"), 2);
    assert_eq!(
        reconciling.get::<Option<i64>, _>("github_issue_number"),
        None
    );

    test_db.drop().await;
}

#[tokio::test]
async fn reconciliation_at_the_retry_ceiling_finalizes_the_payload() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report(
            "Reconciliation must not outlive its retry ceiling",
            None,
        ))
        .await
        .expect("enqueue outbox row");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(6)
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({ "message": "outage" })))
        .expect(1)
        .mount(&github)
        .await;

    for attempt in 1..=6 {
        let result = service
            .process_due(1)
            .await
            .expect("reconciliation result is persisted");
        assert_eq!((result.claimed, result.completed, result.failed), (1, 0, 1));
        if attempt < 6 {
            sqlx::query("UPDATE platform_issue_outbox SET next_run_at = now() WHERE id = $1")
                .bind(submission_id)
                .execute(&test_db.pool)
                .await
                .expect("make reconciliation retry due");
        }
    }

    let tombstone = sqlx::query(
        "SELECT outcome, attempts FROM platform_issue_delivery_tombstone WHERE submission_id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read terminal reconciliation tombstone");
    assert_eq!(tombstone.get::<String, _>("outcome"), "terminal_failed");
    assert_eq!(tombstone.get::<i32, _>("attempts"), 6);

    test_db.drop().await;
}

#[tokio::test]
async fn terminal_failure_moves_only_delivery_metadata_to_a_tombstone() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report(
            "Terminal lookup failure retains no report payload",
            None,
        ))
        .await
        .expect("enqueue outbox row");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({ "message": "outage" })))
        .expect(6)
        .mount(&github)
        .await;

    for attempt in 1..=6 {
        let result = service
            .process_due(1)
            .await
            .expect("failed lookup is persisted without aborting the drain");
        assert_eq!((result.claimed, result.completed, result.failed), (1, 0, 1));
        if attempt < 6 {
            sqlx::query("UPDATE platform_issue_outbox SET next_run_at = now() WHERE id = $1")
                .bind(submission_id)
                .execute(&test_db.pool)
                .await
                .expect("make retry due");
        }
    }

    let tombstone = sqlx::query(
        "SELECT outcome, attempts, last_error, github_issue_number \
         FROM platform_issue_delivery_tombstone WHERE submission_id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read terminal tombstone");
    assert_eq!(tombstone.get::<String, _>("outcome"), "terminal_failed");
    assert_eq!(tombstone.get::<i32, _>("attempts"), 6);
    assert!(tombstone.get::<Option<String>, _>("last_error").is_some());
    assert_eq!(tombstone.get::<Option<i64>, _>("github_issue_number"), None);

    let active_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM platform_issue_outbox WHERE id = $1")
            .bind(submission_id)
            .fetch_one(&test_db.pool)
            .await
            .expect("confirm personal active row was deleted");
    assert_eq!(active_count, 0);

    test_db.drop().await;
}

#[tokio::test]
async fn contents_api_failure_delivers_a_text_only_issue() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report(
            "Contents failure must not drop the report",
            Some(PlatformIssueScreenshot {
                bytes: b"not-relevant-to-the-delivery-boundary".to_vec(),
                mime: "image/png".to_string(),
            }),
        ))
        .await
        .expect("enqueue screenshot report");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/g5labs/platform-bugs/contents/assets/issues/{submission_id}.png"
        )))
        .respond_with(ResponseTemplate::new(404))
        .expect(2)
        .mount(&github)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "/repos/g5labs/platform-bugs/contents/assets/issues/{submission_id}.png"
        )))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({ "message": "contents down" })),
        )
        .expect(1)
        .mount(&github)
        .await;
    // A screenshot embed would match this first mock and violate its zero-use
    // expectation. The succeeding generic create therefore proves the body
    // was reduced to text-only after the Contents API failure.
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .and(body_string_contains("![screenshot]"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "number": 2718,
            "html_url": "https://github.example/g5labs/platform-bugs/issues/2718"
        })))
        .expect(1)
        .mount(&github)
        .await;

    let result = service
        .process_due(1)
        .await
        .expect("deliver text-only issue");
    assert_eq!((result.claimed, result.completed, result.failed), (1, 1, 0));

    let row = sqlx::query(
        "SELECT outcome, github_issue_number FROM platform_issue_delivery_tombstone WHERE submission_id = $1",
    )
            .bind(submission_id)
            .fetch_one(&test_db.pool)
            .await
            .expect("read text-only delivery tombstone");
    assert_eq!(row.get::<String, _>("outcome"), "completed");
    assert_eq!(row.get::<Option<i64>, _>("github_issue_number"), Some(2718));

    test_db.drop().await;
}

#[tokio::test]
async fn existing_screenshot_asset_is_recovered_without_a_second_contents_put() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report(
            "Crash after screenshot asset commit",
            Some(PlatformIssueScreenshot {
                bytes: b"already-committed-screenshot".to_vec(),
                mime: "image/png".to_string(),
            }),
        ))
        .await
        .expect("enqueue screenshot report");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(1)
        .mount(&github)
        .await;
    let asset_path = format!("assets/issues/{submission_id}.png");
    let commit_sha = "d0c0ffee";
    let screenshot_url =
        format!("https://github.example/g5labs/platform-bugs/raw/{commit_sha}/{asset_path}");
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/g5labs/platform-bugs/contents/assets/issues/{submission_id}.png"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html_url": format!(
                "https://github.example/g5labs/platform-bugs/blob/main/{asset_path}"
            )
        })))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/g5labs/platform-bugs/commits"))
        .and(query_param("path", &asset_path))
        .and(query_param("per_page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "sha": commit_sha }])))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "/repos/g5labs/platform-bugs/contents/assets/issues/{submission_id}.png"
        )))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .and(body_string_contains(&screenshot_url))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "number": 3141,
            "html_url": "https://github.example/g5labs/platform-bugs/issues/3141"
        })))
        .expect(1)
        .mount(&github)
        .await;

    let result = service
        .process_due(1)
        .await
        .expect("recover existing asset and create issue");
    assert_eq!((result.claimed, result.completed, result.failed), (1, 1, 0));

    test_db.drop().await;
}

#[tokio::test]
async fn newly_committed_screenshot_uses_a_commit_pinned_raw_url() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report(
            "New screenshot uses its Contents commit",
            Some(PlatformIssueScreenshot {
                bytes: b"new-screenshot".to_vec(),
                mime: "image/png".to_string(),
            }),
        ))
        .await
        .expect("enqueue screenshot report");

    mount_installation_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(1)
        .mount(&github)
        .await;
    let asset_path = format!("assets/issues/{submission_id}.png");
    let commit_sha = "c0ffee00";
    let screenshot_url =
        format!("https://github.example/g5labs/platform-bugs/raw/{commit_sha}/{asset_path}");
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/g5labs/platform-bugs/contents/assets/issues/{submission_id}.png"
        )))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "/repos/g5labs/platform-bugs/contents/assets/issues/{submission_id}.png"
        )))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "content": {
                "html_url": format!(
                    "https://github.example/g5labs/platform-bugs/blob/main/{asset_path}"
                )
            },
            "commit": { "sha": commit_sha }
        })))
        .expect(1)
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .and(body_string_contains(&screenshot_url))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "number": 2719,
            "html_url": "https://github.example/g5labs/platform-bugs/issues/2719"
        })))
        .expect(1)
        .mount(&github)
        .await;

    let result = service
        .process_due(1)
        .await
        .expect("deliver screenshot issue");
    assert_eq!((result.claimed, result.completed, result.failed), (1, 1, 0));

    test_db.drop().await;
}

#[tokio::test]
async fn github_201_followed_by_completion_write_failure_is_recovered_without_duplicate_create() {
    let test_db = TestDb::create().await;
    let github = MockServer::start().await;
    let service = intake_service(&test_db, &github);
    let submission_id = service
        .enqueue(report(
            "Persisted completion write fails after GitHub 201",
            None,
        ))
        .await
        .expect("enqueue outbox row");

    mount_installation_token(&github).await;
    // The first lookup permits the create and the first stale retry observes an
    // empty, index-lagging search result. Remove it before mounting the later
    // exact-marker response so the test controls the external sequence.
    let empty_search = Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(2)
        .mount_as_scoped(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/g5labs/platform-bugs/issues"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "number": 808,
            "html_url": "https://github.example/g5labs/platform-bugs/issues/808"
        })))
        .expect(1)
        .mount(&github)
        .await;

    sqlx::query(
        "CREATE FUNCTION fail_platform_issue_completion_write() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
         RAISE EXCEPTION 'injected completion write failure'; \
         RETURN NEW; \
         END; $$",
    )
    .execute(&test_db.pool)
    .await
    .expect("install completion-write failure function");
    sqlx::query(
        "CREATE TRIGGER fail_platform_issue_completion_write \
         BEFORE INSERT ON platform_issue_delivery_tombstone FOR EACH ROW \
         EXECUTE FUNCTION fail_platform_issue_completion_write()",
    )
    .execute(&test_db.pool)
    .await
    .expect("install completion-write failure trigger");

    let first = service
        .process_due(1)
        .await
        .expect("row-level finalization failure must not abort the batch");
    assert_eq!((first.claimed, first.completed, first.failed), (1, 0, 0));

    let interrupted = sqlx::query(
        "SELECT status, attempts, github_issue_number, github_create_started_at IS NOT NULL AS fenced FROM platform_issue_outbox WHERE id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read row after injected completion failure");
    assert_eq!(interrupted.get::<String, _>("status"), "processing");
    assert_eq!(interrupted.get::<i32, _>("attempts"), 1);
    assert_eq!(
        interrupted.get::<Option<i64>, _>("github_issue_number"),
        None,
        "the failed completion write must leave the crash window observable"
    );
    assert!(interrupted.get::<bool, _>("fenced"));

    sqlx::query(
        "DROP TRIGGER fail_platform_issue_completion_write ON platform_issue_delivery_tombstone",
    )
    .execute(&test_db.pool)
    .await
    .expect("remove injected trigger before retry");
    sqlx::query("DROP FUNCTION fail_platform_issue_completion_write()")
        .execute(&test_db.pool)
        .await
        .expect("remove injected function before retry");
    // Model the worker's stale-claim recovery path instead of modifying status
    // or outbox payload: the next drain must reclaim this processing row.
    sqlx::query(
        "UPDATE platform_issue_outbox \
         SET locked_at = now() - interval '11 minutes' WHERE id = $1",
    )
    .bind(submission_id)
    .execute(&test_db.pool)
    .await
    .expect("make interrupted processing row reclaimable");

    let reconciling = service
        .process_due(1)
        .await
        .expect("empty index-lag search enters reconciliation");
    assert_eq!(
        (
            reconciling.claimed,
            reconciling.completed,
            reconciling.failed
        ),
        (1, 0, 1)
    );

    let retained = sqlx::query("SELECT status, attempts FROM platform_issue_outbox WHERE id = $1")
        .bind(submission_id)
        .fetch_one(&test_db.pool)
        .await
        .expect("index-lag row remains retained for reconciliation");
    assert_eq!(retained.get::<String, _>("status"), "reconciling");
    assert_eq!(retained.get::<i32, _>("attempts"), 2);

    drop(empty_search);
    Mock::given(method("GET"))
        .and(path("/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "number": 808,
                "html_url": "https://github.example/g5labs/platform-bugs/issues/808",
                "body": format!("Saved report <!-- g5-report:{submission_id} -->")
            }]
        })))
        .expect(1)
        .mount(&github)
        .await;

    sqlx::query("UPDATE platform_issue_outbox SET next_run_at = now() WHERE id = $1")
        .bind(submission_id)
        .execute(&test_db.pool)
        .await
        .expect("make reconciliation lookup due");
    let recovered = service
        .process_due(1)
        .await
        .expect("recover prior GitHub issue after marker indexes");
    assert_eq!(
        (recovered.claimed, recovered.completed, recovered.failed),
        (1, 1, 0)
    );

    let row = sqlx::query(
        "SELECT outcome, attempts, github_issue_number, github_issue_url \
         FROM platform_issue_delivery_tombstone WHERE submission_id = $1",
    )
    .bind(submission_id)
    .fetch_one(&test_db.pool)
    .await
    .expect("read recovered tombstone");
    assert_eq!(row.get::<String, _>("outcome"), "completed");
    assert_eq!(row.get::<i32, _>("attempts"), 3);
    assert_eq!(row.get::<Option<i64>, _>("github_issue_number"), Some(808));
    assert_eq!(
        row.get::<Option<String>, _>("github_issue_url").as_deref(),
        Some("https://github.example/g5labs/platform-bugs/issues/808")
    );

    test_db.drop().await;
}
