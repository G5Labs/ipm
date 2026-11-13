//! Approval policy lock protocol (g5.governance.approval.requirements,
//! g5.governance.merge): a merge admission holding the target's policy
//! locks serializes every writer of the policy it reads — definition
//! publishing, decisions and withdrawals, requirement attachment — so an
//! admission sees the state before or after a writer, never a mix.
use crate::helpers;
use crate::helpers::review_flow;

use g5_api::services::approval_config_service::lock_merge_admission;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

struct Fixture {
    app: TestApp,
    owner: TestUser,
    reviewer: TestUser,
    org: TestOrg,
    source: TestWorkspace,
    target: TestWorkspace,
    definition_id: String,
}

impl Fixture {
    async fn create() -> Self {
        let app = TestApp::spawn().await;
        let flow = review_flow::Flow::new(&app).await;
        flow.submit(&app).await;
        let state = flow.overview(&app, &flow.owner).await;
        let head = state["heads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["repoId"] == flow.parent.repo_id.to_string())
            .unwrap();
        let source = TestWorkspace {
            id: flow.source,
            org_id: flow.org.id,
            repo_id: flow.parent.repo_id,
            name: head["branchName"].as_str().unwrap().to_owned(),
            head_commit: head["headCommit"].as_str().unwrap().to_owned(),
        };
        let owner = flow.owner;
        let org = flow.org;
        let target = flow.parent;
        let reviewer = flow.reviewer;
        let client = app.authenticated_client(&owner);
        let response = client
            .post_json(
                &format!("/api/v1/orgs/{}/approval-definitions", org.id),
                &json!({ "name": "Merge sign-off", "humanUserIds": [reviewer.id], "phases": ["ie"] }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.unwrap();
        let definition_id = body["data"]["id"].as_str().unwrap().to_string();
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/approvals/requirements", target.id),
                &json!({ "definitionId": definition_id }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        Self {
            app,
            owner,
            reviewer,
            org,
            source,
            target,
            definition_id,
        }
    }
}

/// Run `writer` while an admission holds the target's policy locks: the
/// writer must still be pending after `grace`, and complete once the
/// admission commits.
async fn assert_serialized<F>(fixture: &Fixture, label: &str, writer: F)
where
    F: std::future::Future<Output = (StatusCode, Value)> + Send + 'static,
{
    let mut admission = fixture.app.scoped_db_for_org(fixture.org.id).await;
    lock_merge_admission(&mut admission, fixture.org.id, fixture.target.id)
        .await
        .unwrap();
    let mut pending = tokio::spawn(writer);
    let grace = Duration::from_millis(400);
    assert!(
        tokio::time::timeout(grace, &mut pending).await.is_err(),
        "{label} must wait for the admission's policy locks"
    );
    admission.commit().await.unwrap();
    let (status, body) = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("writer completes once the admission commits")
        .unwrap();
    assert_eq!(status, StatusCode::OK, "{label}: {body}");
}

#[tokio::test]
async fn admission_serializes_definition_publishing() {
    let fixture = Fixture::create().await;
    let client = fixture.app.authenticated_client(&fixture.owner);
    let url = format!(
        "/api/v1/orgs/{}/approval-definitions/{}/versions",
        fixture.org.id, fixture.definition_id
    );
    let owner_id = fixture.owner.id;
    assert_serialized(&fixture, "publish", async move {
        let response = client
            .post_json(
                &url,
                &json!({ "name": "Merge sign-off v2", "humanUserIds": [owner_id], "phases": ["ie"] }),
            )
            .await;
        let status = response.status();
        (
            if status == StatusCode::CREATED {
                StatusCode::OK
            } else {
                status
            },
            response.json().await.unwrap(),
        )
    })
    .await;
    let versions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_definitions WHERE id = $1::uuid")
            .bind(&fixture.definition_id)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
    assert_eq!(versions, 2);
    fixture.app.cleanup().await;
}

#[tokio::test]
async fn admission_serializes_independent_requested_decisions() {
    let fixture = Fixture::create().await;
    let url = format!(
        "/api/v1/workspaces/{}/approvals/decisions",
        fixture.source.id
    );
    for outcome in ["approved", "rejected"] {
        let response=fixture.app.authenticated_client(&fixture.owner).post_json(&format!("/api/v1/workspaces/{}/approvals/requests",fixture.source.id),&json!({"definitionId":fixture.definition_id,"definitionVersion":1,"phase":"ie","actorUserIds":[fixture.reviewer.id]})).await;
        let request = review_flow::body(response, StatusCode::CREATED).await;
        let request_id = request["requests"][0]["requestId"].clone();
        let client = fixture.app.authenticated_client(&fixture.reviewer);
        let url = url.clone();
        let definition_id = fixture.definition_id.clone();
        let heads = review_flow::heads(&fixture.app, &fixture.reviewer, fixture.source.id).await;
        assert_serialized(&fixture, outcome, async move {
            let response = client
                .post_json(
                    &url,
                    &json!({ "definitionId": definition_id,"requestId":request_id, "outcome": outcome,"comment":"Reviewed this scope", "phase": "ie", "reviewedGitHeads": heads }),
                )
                .await;
            (response.status(), response.json().await.unwrap())
        })
        .await;
    }
    let decisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE source_workspace_id = $1")
            .bind(fixture.source.id)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
    assert_eq!(decisions, 2);
    fixture.app.cleanup().await;
}

#[tokio::test]
async fn admission_serializes_requirement_attachment() {
    let fixture = Fixture::create().await;
    let client = fixture.app.authenticated_client(&fixture.owner);
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/approval-definitions", fixture.org.id),
            &json!({ "name": "Second sign-off", "humanUserIds": [fixture.owner.id], "phases": ["ie"] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let second: Uuid = body["data"]["id"].as_str().unwrap().parse().unwrap();
    let url = format!(
        "/api/v1/workspaces/{}/approvals/requirements",
        fixture.target.id
    );
    assert_serialized(&fixture, "attach", async move {
        let response = client
            .post_json(&url, &json!({ "definitionId": second }))
            .await;
        (response.status(), response.json().await.unwrap())
    })
    .await;
    fixture.app.cleanup().await;
}
