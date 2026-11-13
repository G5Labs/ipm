//! Fork names follow repository ownership. Live claims are refused before fork
//! work; completed deletion releases membership and refs so names can be reused.
//! Suggested names (`uniquifyName`) take the first free numbered alternative.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};

async fn fork(
    client: &helpers::AuthenticatedClient,
    base_id: uuid::Uuid,
    name: &str,
    key: &str,
) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{base_id}/clone"),
            &json!({"name": name, "idempotencyKey": key}),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_deleted_workspace_name_is_reusable_after_repository_cleanup() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let (status, body) = fork(&client, base.id, "issue-19-third", "fork-1").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let deleted = client
        .post_json(&format!("/api/v1/workspaces/{first}/delete"), &json!({}))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let memberships: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_repos WHERE workspace_id=$1::text::uuid",
    )
    .bind(&first)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(memberships, 0, "deletion releases repository ownership");
    let path = app.repo_path(base.repo_id).await;
    assert!(
        g5_scm::GitRepo::open(&path)
            .unwrap()
            .read_branch_oid("issue-19-third")
            .unwrap()
            .is_none(),
        "deletion must remove the old ref before its name is reusable"
    );
    let (status, body) = fork(&client, base.id, "issue-19-third", "fork-2").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_ne!(body["data"]["workspace"]["id"], first);
    assert_eq!(body["data"]["workspace"]["name"], "issue-19-third");

    // Another name forks as usual.
    let (status, body) = fork(&client, base.id, "issue-19-fourth", "fork-3").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    app.cleanup().await;
}

async fn fork_suggested(
    client: &helpers::AuthenticatedClient,
    base_id: uuid::Uuid,
    name: &str,
    key: &str,
) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{base_id}/clone"),
            &json!({"name": name, "idempotencyKey": key, "uniquifyName": true}),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_suggested_name_takes_the_first_free_numbered_alternative() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    // Free: the suggestion is used as is.
    let (status, body) = fork_suggested(&client, base.id, "issue-19-crash", "s-1").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["workspace"]["name"], "issue-19-crash");
    // The live workspace owns the name, so the next fork gets a suffix.
    let (status, body) = fork_suggested(&client, base.id, "issue-19-crash", "s-2").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["workspace"]["name"], "issue-19-crash-2");
    let branch = body["data"]["workspace"]["branches"][0]["branchName"].clone();
    assert_eq!(branch, "issue-19-crash-2", "{body}");
    // A live workspace holds the name too.
    let (status, body) = fork_suggested(&client, base.id, "issue-19-crash", "s-3").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["workspace"]["name"], "issue-19-crash-3");
    // A replay returns the same fork and name.
    let (status, replay) = fork_suggested(&client, base.id, "issue-19-crash", "s-3").await;
    assert_eq!(status, StatusCode::CREATED, "{replay}");
    assert_eq!(
        replay["data"]["workspace"]["id"],
        body["data"]["workspace"]["id"]
    );
    assert_eq!(replay["data"]["workspace"]["name"], "issue-19-crash-3");

    // A name typed exactly is never changed.
    let (status, body) = fork(&client, base.id, "issue-19-crash-2", "s-4").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["field"], "name", "{body}");

    app.cleanup().await;
}
