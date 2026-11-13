//! Integration tests for the retained document/view storage contract.
//!
//! View classification is derived from repository files; document metadata
//! still accepts their doc types and retains the view index. All tests run
//! against a real Postgres database (no mocks).

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use sqlx::Row;
use uuid::Uuid;

// ===========================================================================
// TC-010: doc_type storage preserves unknown committed metadata
// ===========================================================================

#[tokio::test]
async fn test_doc_type_storage_accepts_unknown_committed_metadata() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let doc_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO docs (id, workspace_id, name, doc_type, org_id) VALUES ($1, $2, 'test', 'instruction', (SELECT org_id FROM workspaces WHERE id = $2))",
    )
    .bind(doc_id)
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("committed metadata must not be rejected by docs storage");

    let persisted: String = sqlx::query_scalar("SELECT doc_type FROM docs WHERE id = $1")
        .bind(doc_id)
        .fetch_one(&app.db)
        .await
        .expect("inserted doc must be readable");
    assert_eq!(persisted, "instruction");

    app.cleanup().await;
}

// ===========================================================================
// TC-011: doc_type storage accepts all view persona types
// ===========================================================================

#[tokio::test]
async fn test_doc_type_storage_accepts_all_view_types() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let view_types = vec![
        "pm_view",
        "arch_view",
        "security_view",
        "performance_view",
        "test_view",
        "ui_view",
        "customized_view",
    ];

    for doc_type in &view_types {
        let result = sqlx::query(
            "INSERT INTO docs (id, workspace_id, name, doc_type, org_id) VALUES ($1, $2, $3, $4, (SELECT org_id FROM workspaces WHERE id = $2))",
        )
        .bind(Uuid::new_v4())
        .bind(workspace.id)
        .bind(format!("test-{doc_type}"))
        .bind(doc_type)
        .execute(&app.db)
        .await;

        assert!(
            result.is_ok(),
            "doc_type '{}' should be accepted by docs storage, got: {:?}",
            doc_type,
            result.err()
        );
    }

    // Also verify existing types still work.
    let existing_types = vec!["spec", "design", "api", "guide", "adr", "note"];
    for doc_type in &existing_types {
        let result = sqlx::query(
            "INSERT INTO docs (id, workspace_id, name, doc_type, org_id) VALUES ($1, $2, $3, $4, (SELECT org_id FROM workspaces WHERE id = $2))",
        )
        .bind(Uuid::new_v4())
        .bind(workspace.id)
        .bind(format!("test-{doc_type}"))
        .bind(doc_type)
        .execute(&app.db)
        .await;

        assert!(
            result.is_ok(),
            "Existing doc_type '{}' should still be accepted",
            doc_type
        );
    }

    app.cleanup().await;
}

// ===========================================================================
// TC-014: Index existence verification
// ===========================================================================

#[tokio::test]
async fn test_view_indices_exist() {
    let app = TestApp::spawn().await;

    // Verify expected indices exist.
    let expected_indices = vec!["idx_docs_workspace_view_types"];

    for idx_name in &expected_indices {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM pg_indexes WHERE indexname = $1")
            .bind(idx_name)
            .fetch_one(&app.db)
            .await
            .expect("query pg_indexes");

        let count: i64 = row.get("cnt");
        assert_eq!(
            count, 1,
            "Index '{}' should exist in the database",
            idx_name
        );
    }

    app.cleanup().await;
}
