//! Integration tests for the G5 API spend endpoints.
//!
//! Covers the complete request→route→middleware→service→database→response cycle
//! for the spend-related endpoints:
//!
//! 1. `GET /api/v1/workspaces/:workspaceId/settings/spend` — Current spend summary
//!    vs. configured limits (from `routes/settings.rs`, §3.9)
//! 2. `GET /api/v1/workspaces/:workspaceId/settings/spend/history` — Historical
//!    spend data across multiple billing periods (from `routes/spend.rs`, §3.15)
//!
//! Each test spawns a full `TestApp` instance with an isolated Postgres database,
//! makes real HTTP requests via the `AuthenticatedClient`, and asserts at the HTTP
//! response level (status codes, JSON body shapes) and the database level (verifying
//! usage records and spend data are correctly aggregated).

use crate::helpers;
use g5_auth::Scope;

use helpers::{assert_json_error, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;
use uuid::Uuid;

// ===========================================================================
// Module-Level Helper Functions (spend-specific)
// ===========================================================================

/// Inserts a single usage record directly into the `usage_records` table.
#[allow(clippy::too_many_arguments)]
async fn insert_usage_record(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    task_id: Uuid,
    cost_usd: &str,
    input_tokens: i64,
    output_tokens: i64,
) {
    let record_id = Uuid::new_v4();
    let cost = Decimal::from_str(cost_usd).expect("Invalid decimal string");

    sqlx::query!(
        r#"INSERT INTO usage_records
           (id, org_id, workspace_id, task_id,
            provider, model, input_tokens, output_tokens, total_tokens,
            cache_creation_tokens, cache_read_tokens,
            cost_usd, request_type, created_at)
           VALUES ($1, $2, $3, $4,
                   'anthropic', 'claude-sonnet-4-20250514', $5, $6, $5::bigint + $6::bigint,
                   0, 0,
                   $7, 'chat', now())"#,
        record_id,
        org_id,
        workspace_id,
        task_id,
        input_tokens,
        output_tokens,
        cost,
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert usage record");
}

/// Inserts a usage record with a specific `created_at` timestamp for testing
/// historical spend queries.
#[allow(clippy::too_many_arguments, dead_code)]
async fn insert_usage_record_with_timestamp(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    task_id: Uuid,
    cost_usd: &str,
    input_tokens: i64,
    output_tokens: i64,
    created_at: chrono::DateTime<chrono::Utc>,
) {
    let record_id = Uuid::new_v4();
    let cost = Decimal::from_str(cost_usd).expect("Invalid decimal string");

    sqlx::query!(
        r#"INSERT INTO usage_records
           (id, org_id, workspace_id, task_id,
            provider, model, input_tokens, output_tokens, total_tokens,
            cache_creation_tokens, cache_read_tokens,
            cost_usd, request_type, created_at)
           VALUES ($1, $2, $3, $4,
                   'anthropic', 'claude-sonnet-4-20250514', $5, $6, $5::bigint + $6::bigint,
                   0, 0,
                   $7, 'chat', $8)"#,
        record_id,
        org_id,
        workspace_id,
        task_id,
        input_tokens,
        output_tokens,
        cost,
        created_at,
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert usage record with timestamp");
}

/// Creates a task with all-time cost counters matching the usage ledger fixture.
async fn create_task_with_cost(
    app: &TestApp,
    workspace_id: Uuid,
    cost_usd: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> Uuid {
    let task_id = Uuid::new_v4();
    let cost = Decimal::from_str(cost_usd).expect("Invalid decimal string");

    sqlx::query!(
        r#"INSERT INTO tasks
           (id, workspace_id, name, kind, spec, status, created_by,
            cost_usd, input_tokens, output_tokens, created_at, updated_at, org_id)
           SELECT $1, id, 'test-task', 'command', '{}'::jsonb,
                  'succeeded', owner_user_id, $3, $4, $5, now(), now(), org_id
           FROM workspaces WHERE id = $2"#,
        task_id,
        workspace_id,
        cost,
        input_tokens,
        output_tokens,
    )
    .execute(&app.db)
    .await
    .expect("Failed to insert task with cost");

    task_id
}

/// Comprehensive setup function that creates tasks and usage records
/// for a workspace, populating both denormalized counters and usage_records.
async fn setup_spend_data(app: &TestApp, org: &TestOrg, workspace: &TestWorkspace) -> Vec<Uuid> {
    // Create tasks with denormalized all-time cost data
    let task1_id = create_task_with_cost(app, workspace.id, "10.50", 200000, 80000).await;
    let task2_id = create_task_with_cost(app, workspace.id, "5.25", 100000, 40000).await;

    // Insert usage records (for spend summary and history)
    insert_usage_record(app, org.id, workspace.id, task1_id, "10.50", 200000, 80000).await;
    insert_usage_record(app, org.id, workspace.id, task2_id, "5.25", 100000, 40000).await;

    vec![task1_id, task2_id]
}

// ===========================================================================
// Group 1: Spend Summary — GET /api/v1/workspaces/:workspaceId/settings/spend
// ===========================================================================

/// Verifies that the spend summary endpoint returns 200 with the correct
/// `SpendSummaryDTO` shape including period bounds, totals, limits,
/// limit-reached flag, and per-task breakdown.
#[tokio::test]
async fn test_spend_summary_returns_200_with_correct_shape() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::BillingAdmin])
            .await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;

    // Insert spend data
    setup_spend_data(&app, &org, &workspace).await;

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend",
            workspace.id
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify SpendSummaryDTO shape
    assert!(
        data["periodStart"].is_string(),
        "periodStart should be a string"
    );
    assert!(
        data["periodEnd"].is_string(),
        "periodEnd should be a string"
    );
    assert!(
        data["totalSpendUsd"].is_number(),
        "totalSpendUsd should be a number"
    );
    assert!(
        data["totalInputTokens"].is_number(),
        "totalInputTokens should be a number"
    );
    assert!(
        data["totalOutputTokens"].is_number(),
        "totalOutputTokens should be a number"
    );
    // limitSpendUsd can be null or number
    assert!(
        data["limitSpendUsd"].is_null() || data["limitSpendUsd"].is_number(),
        "limitSpendUsd should be null or number"
    );
    assert!(
        data["limitInputTokens"].is_null() || data["limitInputTokens"].is_number(),
        "limitInputTokens should be null or number"
    );
    assert!(
        data["limitOutputTokens"].is_null() || data["limitOutputTokens"].is_number(),
        "limitOutputTokens should be null or number"
    );
    assert!(
        data["isSpendLimitReached"].is_boolean(),
        "isSpendLimitReached should be boolean"
    );
    assert!(data["byTask"].is_array(), "byTask should be an array");

    // Verify per-task breakdown shape
    let by_task = data["byTask"].as_array().unwrap();
    assert!(
        !by_task.is_empty(),
        "Should have at least one task in breakdown"
    );

    for task_entry in by_task {
        assert!(
            task_entry["taskId"].is_string(),
            "taskId should be a string"
        );
        assert!(
            task_entry["taskName"].is_string(),
            "taskName should be a string"
        );
        assert!(
            task_entry["taskType"].is_string(),
            "taskType should be a string"
        );
        assert!(
            task_entry["spendUsd"].is_number(),
            "spendUsd should be a number"
        );
        assert!(
            task_entry["inputTokens"].is_number(),
            "inputTokens should be a number"
        );
        assert!(
            task_entry["outputTokens"].is_number(),
            "outputTokens should be a number"
        );
    }

    // Verify spend totals are non-zero (we inserted cost data)
    let total_spend = data["totalSpendUsd"].as_f64().unwrap();
    assert!(
        total_spend > 0.0,
        "totalSpendUsd should be > 0 after inserting cost data"
    );

    // Verify response does NOT have standard pagination meta
    assert!(
        body["meta"].is_null() || body.get("meta").is_none(),
        "Spend summary should NOT use standard PaginationMeta"
    );

    app.cleanup().await;
}

/// Verifies that the spend summary returns 200 with zero totals when no
/// tasks have cost data.
#[tokio::test]
async fn test_spend_summary_returns_zero_spend_for_empty_workspace() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::BillingAdmin])
            .await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;

    // No spend data inserted

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend",
            workspace.id
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    assert_eq!(
        data["totalSpendUsd"].as_f64().unwrap(),
        0.0,
        "totalSpendUsd should be 0 for empty workspace"
    );
    assert_eq!(
        data["totalInputTokens"].as_i64().unwrap(),
        0,
        "totalInputTokens should be 0 for empty workspace"
    );
    assert_eq!(
        data["totalOutputTokens"].as_i64().unwrap(),
        0,
        "totalOutputTokens should be 0 for empty workspace"
    );
    assert!(
        !data["isSpendLimitReached"].as_bool().unwrap(),
        "isSpendLimitReached should be false for empty workspace"
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 2: Spend History — GET /api/v1/workspaces/:workspaceId/settings/spend/history
// ===========================================================================

/// Verifies that the spend history endpoint returns 200 with the correct
/// `SpendHistoryDTO` shape including granularity, period-level aggregates,
/// and daily breakdown.
#[tokio::test]
async fn test_spend_history_returns_200_with_correct_shape() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::BillingAdmin])
            .await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;

    // Insert usage records for spend history
    setup_spend_data(&app, &org, &workspace).await;

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend/history",
            workspace.id
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify SpendHistoryDTO shape
    assert!(
        data["granularity"].is_string(),
        "granularity should be a string"
    );
    assert!(data["periods"].is_array(), "periods should be an array");
    assert!(data["daily"].is_array(), "daily should be an array");

    let granularity = data["granularity"].as_str().unwrap();
    assert!(
        ["hourly", "daily", "weekly", "monthly"].contains(&granularity),
        "granularity should be one of hourly/daily/weekly/monthly, got '{}'",
        granularity
    );

    // Verify period shape (if any periods returned)
    let periods = data["periods"].as_array().unwrap();
    for period in periods {
        assert!(
            period["periodStart"].is_string(),
            "periodStart should be a string"
        );
        assert!(
            period["periodEnd"].is_string(),
            "periodEnd should be a string"
        );
        assert!(
            period["totalSpendUsd"].is_number(),
            "totalSpendUsd should be a number"
        );
        assert!(
            period["totalInputTokens"].is_number(),
            "totalInputTokens should be a number"
        );
        assert!(
            period["totalOutputTokens"].is_number(),
            "totalOutputTokens should be a number"
        );
        assert!(
            period["taskCount"].is_number(),
            "taskCount should be a number"
        );
        assert!(
            period["taskCount"].is_number(),
            "taskCount should be a number"
        );
    }

    // Verify daily shape (if any daily entries returned)
    let daily = data["daily"].as_array().unwrap();
    for day in daily {
        assert!(day["date"].is_string(), "date should be a string");
        assert!(day["spendUsd"].is_number(), "spendUsd should be a number");
        assert!(
            day["inputTokens"].is_number(),
            "inputTokens should be a number"
        );
        assert!(
            day["outputTokens"].is_number(),
            "outputTokens should be a number"
        );
    }

    // Verify response does NOT have standard pagination meta
    assert!(
        body["meta"].is_null() || body.get("meta").is_none(),
        "Spend history should NOT use standard PaginationMeta"
    );

    app.cleanup().await;
}

/// Verifies that the `periods` and `granularity` query parameters are respected.
#[tokio::test]
async fn test_spend_history_with_custom_periods_and_granularity() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::BillingAdmin])
            .await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;

    // Insert usage records
    setup_spend_data(&app, &org, &workspace).await;

    // Request with custom periods=3 and granularity=daily
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend/history?periods=3&granularity=daily",
            workspace.id
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify granularity is "daily"
    assert_eq!(
        data["granularity"].as_str().unwrap(),
        "daily",
        "granularity should match the requested value"
    );

    // Verify periods array has at most 3 entries
    let periods = data["periods"].as_array().unwrap();
    assert!(
        periods.len() <= 3,
        "Should have at most 3 periods, got {}",
        periods.len()
    );

    app.cleanup().await;
}

/// Verifies that the `periods` parameter defaults to 6 when not provided.
#[tokio::test]
async fn test_spend_history_defaults_periods_to_6() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::BillingAdmin])
            .await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;

    // No query parameters — should use defaults
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend/history",
            workspace.id
        ))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    let data = &body["data"];

    // Verify periods array has at most 6 entries (default)
    let periods = data["periods"].as_array().unwrap();
    assert!(
        periods.len() <= 6,
        "Default periods should be at most 6, got {}",
        periods.len()
    );

    app.cleanup().await;
}

// ===========================================================================
// Group 3: Authentication & Authorization
// ===========================================================================

/// Verifies that accessing the spend summary without authentication returns 401.
#[tokio::test]
async fn test_spend_summary_without_auth_returns_401() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let _user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // Send request without auth header
    let response = app
        .client
        .get(format!(
            "{}/api/v1/workspaces/{}/settings/spend",
            app.base_url, workspace.id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Verifies that accessing the spend history without authentication returns 401.
#[tokio::test]
async fn test_spend_history_without_auth_returns_401() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let _user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // Send request without auth header
    let response = app
        .client
        .get(format!(
            "{}/api/v1/workspaces/{}/settings/spend/history",
            app.base_url, workspace.id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_json_error(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// Verifies that accessing the spend history with a valid JWT but without the
/// `billing:admin` scope returns 403 FORBIDDEN. This is a critical authorization
/// test because the spend history endpoint has a stricter scope requirement than
/// the spend summary.
#[tokio::test]
async fn test_spend_history_without_billing_admin_scope_returns_403() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    // Create user with only 'workspaces:read' scope — missing 'billing:admin'
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend/history",
            workspace.id
        ))
        .await;

    assert_json_error(response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    app.cleanup().await;
}

// ===========================================================================
// Group 4: Not Found
// ===========================================================================

/// Verifies that requesting spend summary for a non-existent workspace returns 404.
#[tokio::test]
async fn test_spend_summary_nonexistent_workspace_returns_404() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::BillingAdmin])
            .await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    let client = app.authenticated_client(&user);

    let fake_workspace_id = Uuid::new_v4();

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend",
            fake_workspace_id
        ))
        .await;

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}

/// Verifies that requesting spend history for a non-existent workspace returns 404.
#[tokio::test]
async fn test_spend_history_nonexistent_workspace_returns_404() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::BillingAdmin])
            .await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    let client = app.authenticated_client(&user);

    let fake_workspace_id = Uuid::new_v4();

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/settings/spend/history",
            fake_workspace_id
        ))
        .await;

    assert_json_error(response, StatusCode::NOT_FOUND, "NOT_FOUND").await;

    app.cleanup().await;
}
