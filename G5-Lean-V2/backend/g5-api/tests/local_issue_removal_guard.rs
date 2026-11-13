//! Boundary regressions for retiring the local issue tracker.
//!
//! External issue integration replaces local issues and hotlists. This guard
//! proves the retired public routes (the org issues path serves only tracker
//! issue creation), instance feature and schema objects are
//! absent, that the system-pool lookup grants which carried issue-discovery
//! names survive under neutral names, and that platform bug reporting is
//! untouched.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::Value;

const RETIRED_TABLES: &[&str] = &[
    "hotlist_events",
    "hotlist_memberships",
    "hotlists",
    "hotlist_slug_sequence",
    "issue_attachments",
    "issue_comments",
    "issue_components",
    "issue_events",
    "issue_inference_jobs",
    "issue_job_links",
    "issue_labels",
    "issue_links",
    "issue_slug_sequence",
    "issues",
];

#[tokio::test]
async fn retired_issue_routes_and_feature_are_absent() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);

    // The path now creates tracker issues (POST, #478); the retired local
    // issue list must stay unrouted. Write scope passes the POST route's scope
    // layer, so the unmatched method is answered by the method router.
    let removed = client.get(&format!("/api/v1/orgs/{}/issues", org.id)).await;
    assert_eq!(
        removed.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /orgs/:orgId/issues must not be routed"
    );

    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .expect("info request")
        .json()
        .await
        .expect("info JSON");
    let features = info["data"]["features"]
        .as_array()
        .expect("instance feature list");
    assert!(
        !features.iter().any(|feature| feature == "issue_tracking"),
        "instance info must not advertise the retired issue_tracking feature: {info}"
    );
}

#[tokio::test]
async fn retired_issue_schema_is_dropped_and_platform_issue_schema_is_retained() {
    let app = TestApp::spawn().await;

    for table in RETIRED_TABLES {
        let present: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('public.' || $1)::text")
                .bind(table)
                .fetch_one(&app.db)
                .await
                .expect("regclass lookup");
        assert!(present.is_none(), "retired table {table} still exists");
    }

    for table in ["platform_issue_outbox", "platform_issue_delivery_tombstone"] {
        let present: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('public.' || $1)::text")
                .bind(table)
                .fetch_one(&app.db)
                .await
                .expect("regclass lookup");
        assert!(
            present.is_some(),
            "platform issue table {table} must be retained"
        );
    }

    let discovery_policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_policies \
         WHERE schemaname = 'public' AND policyname LIKE 'issue_discovery_%'",
    )
    .fetch_one(&app.db)
    .await
    .expect("policy lookup");
    assert_eq!(
        discovery_policies, 0,
        "issue discovery policies must be retired"
    );
}

#[tokio::test]
async fn retired_node_and_branch_lookup_policies_are_gone() {
    let app = TestApp::spawn().await;

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) \
         FROM pg_policies \
         WHERE schemaname = 'public' \
           AND policyname IN ( \
               'system_ontology_node_lookup', \
               'system_workspace_branch_lookup', \
               'system_workspace_branch_repo_lookup')",
    )
    .fetch_one(&app.db)
    .await
    .expect("policy lookup");

    assert_eq!(count, 0, "retired lookup surfaces must have no policies");
}
