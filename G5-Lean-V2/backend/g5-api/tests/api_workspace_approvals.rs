//! Integration tests for the workspace approvals endpoints
//! (`/api/v1/workspaces/:workspaceId/approvals`, B6 governance UI PR-2).
//!
//! Scenario ownership: `test_contract/workspace_approvals_management` D1–D4, D9.
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! exercises the real request→route→middleware→service→database→response
//! cycle. No mocks, stubs, or fake persistence.

use crate::helpers;
use crate::helpers::review_flow;
use review_flow::{body, Flow};

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn approvals_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals")
}

fn requirements_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/requirements")
}

fn retire_path(workspace_id: Uuid, definition_id: &str) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/requirements/{definition_id}/retire")
}

fn decisions_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/decisions")
}

fn requests_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/requests")
}

fn cancel_request_path(workspace_id: Uuid, request_id: &str) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/requests/{request_id}/cancel")
}

/// Create a definition in the library and return its id.
async fn create_definition(
    app: &TestApp,
    owner: &TestUser,
    org_id: Uuid,
    name: &str,
    human_ids: &[Uuid],
) -> String {
    let client = app.authenticated_client(owner);
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{org_id}/approval-definitions"),
            &json!({ "name": name, "humanUserIds": human_ids }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED, "create definition");
    let body: Value = response.json().await.expect("parse JSON");
    body["data"]["id"].as_str().expect("id").to_string()
}

/// Create a definition bound to the given phases and return its id.
async fn create_phase_definition(
    app: &TestApp,
    owner: &TestUser,
    org_id: Uuid,
    name: &str,
    human_ids: &[Uuid],
    phases: &[&str],
) -> String {
    let client = app.authenticated_client(owner);
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{org_id}/approval-definitions"),
            &json!({
                "name": name,
                "humanUserIds": human_ids,
                "phases": phases,
            }),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "create phase definition"
    );
    let body: Value = response.json().await.expect("parse JSON");
    body["data"]["id"].as_str().expect("id").to_string()
}

/// Mark `fork` as cloned from `target` so the merge target resolves.
async fn set_cloned_from(app: &TestApp, fork: Uuid, target: Uuid) {
    sqlx::query("UPDATE workspaces SET cloned_from_id = $1 WHERE id = $2")
        .bind(target)
        .bind(fork)
        .execute(&app.db)
        .await
        .expect("set cloned_from_id");
}

/// Set an explicit merge target on `fork` (beats `cloned_from_id`).
async fn set_merge_target(app: &TestApp, fork: Uuid, target: Uuid) {
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $1 WHERE id = $2")
        .bind(target)
        .bind(fork)
        .execute(&app.db)
        .await
        .expect("set merge_target_workspace_id");
}

/// The `retired` flags of a target's config events, in `seq` order.
async fn event_history(app: &TestApp, target: Uuid) -> Vec<bool> {
    sqlx::query_scalar(
        "SELECT retired FROM approval_configs WHERE target_workspace_id = $1 ORDER BY seq",
    )
    .bind(target)
    .fetch_all(&app.db)
    .await
    .expect("read event history")
}

async fn member_user(app: &TestApp, org_id: Uuid) -> TestUser {
    TestUser::create_with_scopes(
        app,
        org_id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await
}

// ===========================================================================
// test_contract/workspace_approvals_management D1
// ===========================================================================

/// A target requires one workspace-wide type, with independently addressed
/// recipients and immutable request context across phase moves and retries.
#[tokio::test]
async fn workspace_wide_review_notes_and_retries_survive_phase_moves() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let initial = f.state(&app, &f.reviewer).await;
    assert_eq!(
        initial["outgoing"]["requirements"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let owner = app.authenticated_client(&f.owner);
    let payload = json!({
        "clientRequestId": Uuid::new_v4(), "definitionId": f.definition,
        "definitionVersion": 1, "actorUserIds": [f.reviewer.id, f.second.id],
        "note": "  Please check the migration and rollback plan.  "
    });
    let created = body(
        owner.post_json(&requests_path(f.source), &payload).await,
        StatusCode::CREATED,
    )
    .await;
    body(
        owner
            .patch_json(
                &format!("/api/v1/workspaces/{}", f.source),
                &json!({"phase":"se"}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let view = f.overview(&app, &f.reviewer).await;
    let requests = view["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|r| r["note"] == "Please check the migration and rollback plan."));
    let addressed = requests
        .iter()
        .find(|r| r["actorUserId"] == f.reviewer.id.to_string())
        .unwrap();
    body(
        f.decision(&app, &f.reviewer, addressed, "approved").await,
        StatusCode::OK,
    )
    .await;
    let state = f.state(&app, &f.owner).await;
    assert_eq!(
        state["outgoing"]["requirements"].as_array().unwrap().len(),
        1
    );
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], true);
    let replay = body(
        owner.post_json(&requests_path(f.source), &payload).await,
        StatusCode::OK,
    )
    .await;
    let request_ids = |value: &Value| {
        value["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["requestId"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(request_ids(&created), request_ids(&replay));
    assert!(replay["requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["reused"] == true));
    let mut changed = payload.clone();
    changed["note"] = json!("Different instructions");
    body(
        owner.post_json(&requests_path(f.source), &changed).await,
        StatusCode::CONFLICT,
    )
    .await;
    let fresh = json!({
        "clientRequestId": Uuid::new_v4(), "definitionId": f.definition,
        "definitionVersion": 1, "actorUserIds": [f.reviewer.id],
        "note": "Please check the revised rollback plan."
    });
    body(
        owner.post_json(&requests_path(f.source), &fresh).await,
        StatusCode::CREATED,
    )
    .await;
    let after = f.overview(&app, &f.reviewer).await;
    assert_eq!(after["requests"].as_array().unwrap().len(), 3);
    assert_eq!(
        after["requests"][0]["note"],
        "Please check the revised rollback plan."
    );
    assert_ne!(after["requests"][0]["id"], addressed["id"]);
    assert_eq!(after["requests"][0]["state"], "pending");
    assert!(after["requests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["actorUserId"] == f.second.id.to_string() && r["state"] == "pending"));
    app.cleanup().await;
}

/// Attach → active + member-visible; retire → inactive but in history;
/// re-attach → active again with three events, all inserts.
#[tokio::test]
async fn attach_retire_reattach_lifecycle() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let member = member_user(&app, org.id).await;
    let definition_id = create_definition(&app, &owner, org.id, "Prod sign-off", &[owner.id]).await;
    let owner_client = app.authenticated_client(&owner);
    let member_client = app.authenticated_client(&member);

    // Attach.
    let response = owner_client
        .post_json(
            &requirements_path(target.id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    let required = body["data"]["requiredInto"].as_array().expect("array");
    assert_eq!(required.len(), 1);
    assert_eq!(required[0]["definition"]["id"], definition_id.as_str());
    assert_eq!(required[0]["attachedBy"]["userId"], owner.id.to_string());

    // Member sees the same read-only state.
    let response = member_client.get(&approvals_path(target.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    assert_eq!(body["data"]["requiredInto"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["data"]["attachmentHistory"].as_array().unwrap().len(),
        1
    );

    // Retire: leaves the active set, history has both events.
    let response = owner_client
        .post_json(&retire_path(target.id, &definition_id), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    assert_eq!(body["data"]["requiredInto"].as_array().unwrap().len(), 0);
    let history = body["data"]["attachmentHistory"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["retired"], true);
    assert_eq!(history[1]["retired"], false);

    // Re-attach restores it with a third event.
    let response = owner_client
        .post_json(
            &requirements_path(target.id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    assert_eq!(body["data"]["requiredInto"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["data"]["attachmentHistory"].as_array().unwrap().len(),
        3
    );

    // Every event is an insert-only row.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM approval_configs WHERE target_workspace_id = $1")
            .bind(target.id)
            .fetch_one(&app.db)
            .await
            .expect("count events");
    assert_eq!(count, 3);

    app.cleanup().await;
}

/// Attaching an already-active requirement is an idempotent no-op.
#[tokio::test]
async fn attach_is_idempotent_for_active_requirement() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let definition_id = create_definition(&app, &owner, org.id, "Idem", &[owner.id]).await;
    let client = app.authenticated_client(&owner);

    for _ in 0..2 {
        let response = client
            .post_json(
                &requirements_path(target.id),
                &json!({ "definitionId": definition_id }),
            )
            .await;
        let status = response.status();
        if status != StatusCode::OK {
            let body = response.text().await.unwrap_or_default();
            panic!("attach failed: {status} body={body}");
        }
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM approval_configs WHERE target_workspace_id = $1")
            .bind(target.id)
            .fetch_one(&app.db)
            .await
            .expect("count events");
    assert_eq!(count, 1, "double attach must not create a second event");

    app.cleanup().await;
}

/// Contract D1: lifecycle writes serialize (service advisory lock) — racing
/// requests never append an event repeating the governing state.
#[tokio::test]
async fn concurrent_lifecycle_writes_serialize() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let definition_id = create_definition(&app, &owner, org.id, "Race", &[owner.id]).await;

    // Burst of parallel attaches → exactly one attach event.
    let attaches = (0..4).map(|_| {
        let client = app.authenticated_client(&owner);
        let path = requirements_path(target.id);
        let body = json!({ "definitionId": definition_id });
        async move { client.post_json(&path, &body).await.status() }
    });
    for status in futures::future::join_all(attaches).await {
        assert_eq!(status, StatusCode::OK, "concurrent attach");
    }
    assert_eq!(
        event_history(&app, target.id).await,
        vec![false],
        "racing attaches must land exactly one active event"
    );

    // Burst of parallel retires → exactly one retire event appended.
    let retires = (0..4).map(|_| {
        let client = app.authenticated_client(&owner);
        let path = retire_path(target.id, &definition_id);
        async move { client.post_json(&path, &json!({})).await.status() }
    });
    for status in futures::future::join_all(retires).await {
        assert_eq!(status, StatusCode::OK, "concurrent retire");
    }
    assert_eq!(
        event_history(&app, target.id).await,
        vec![false, true],
        "racing retires must land exactly one retire event"
    );

    // Mixed attach-versus-retire burst: whatever interleaving wins, the
    // stored history must strictly alternate attach/retire.
    let mixed = (0..6).map(|i| {
        let client = app.authenticated_client(&owner);
        let attach_target = requirements_path(target.id);
        let retire_target = retire_path(target.id, &definition_id);
        let body = json!({ "definitionId": definition_id });
        async move {
            if i % 2 == 0 {
                client.post_json(&attach_target, &body).await.status()
            } else {
                client.post_json(&retire_target, &json!({})).await.status()
            }
        }
    });
    for status in futures::future::join_all(mixed).await {
        assert_eq!(status, StatusCode::OK, "mixed lifecycle burst");
    }
    let history = event_history(&app, target.id).await;
    for pair in history.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "history must alternate attach/retire, got {history:?}"
        );
    }

    app.cleanup().await;
}

// ===========================================================================
// test_contract/workspace_approvals_management D2
// ===========================================================================

/// A listed approver's decision records version, comment, actor, and the
/// platform head vector; state reflects it.
#[tokio::test]
async fn decision_records_version_comment_and_head_vector() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = f.primary_request(&app, "ie").await;
    let data = body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let req = data["outgoing"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["phase"] == "ie")
        .unwrap();
    assert_eq!(req["callerCanDecide"], false);
    let actor = req["approvers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["userId"] == f.reviewer.id.to_string())
        .unwrap();
    assert_eq!(actor["state"], "approved");
    assert_eq!(actor["comment"], "Billing scope confirmed.");
    let row:(i32,String,Option<Uuid>,Value,Uuid)=sqlx::query_as("SELECT approval_definition_version,actor_type,actor_user_id,reviewed_git_heads,pull_request_id FROM approval_statuses WHERE source_workspace_id=$1").bind(f.source).fetch_one(&app.db).await.unwrap();
    let view = f.overview(&app, &f.owner).await;
    assert_eq!(row.0, 1);
    assert_eq!(row.1, "human");
    assert_eq!(row.2, Some(f.reviewer.id));
    assert_eq!(row.3, view["heads"]);
    assert_eq!(row.3.as_array().unwrap().len(), 2);
    assert_eq!(row.4.to_string(), view["pr"]["id"]);
    app.cleanup().await;
}

/// Any rejection wins the requirement state while both rows stay in the log.
#[tokio::test]
async fn rejection_wins_requirement_state() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let view = f.overview(&app, &f.second).await;
    let second = view["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actorUserId"] == f.second.id.to_string())
        .unwrap();
    body(
        f.decision(&app, &f.second, second, "rejected").await,
        StatusCode::OK,
    )
    .await;
    let data = f.state(&app, &f.owner).await;
    let req = data["outgoing"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["phase"] == "ie")
        .unwrap();
    assert_eq!(req["state"], "rejected");
    assert_eq!(req["decisionLog"].as_array().unwrap().len(), 2);
    app.cleanup().await;
}

// ===========================================================================
// test_contract/workspace_approvals_management D9
// ===========================================================================

/// Durable Undo settles the withdrawn request, preserves both status rows,
/// and atomically re-arms the actor with one replacement pending request.
#[tokio::test]
async fn settled_decision_requires_explicit_rerequest_without_automatic_successor() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let view = f.overview(&app, &f.reviewer).await;
    assert_eq!(
        view["requests"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["actorUserId"] == f.reviewer.id.to_string() && r["state"] == "pending")
            .count(),
        0
    );
    body(
        f.decision(&app, &f.reviewer, &request, "rejected").await,
        StatusCode::CONFLICT,
    )
    .await;
    let next = f.primary_request(&app, "ie").await;
    assert_ne!(next["id"], request["id"]);
    body(
        f.decision(&app, &f.reviewer, &next, "rejected").await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        f.overview(&app, &f.owner).await["decisions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    app.cleanup().await;
}

/// Only the requester may cancel the pending human successor. Cancellation
/// closes coordination without changing the existing approval decision.
#[tokio::test]
async fn requester_only_human_request_cancellation_preserves_decision() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let first = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &first, "approved").await,
        StatusCode::OK,
    )
    .await;
    let next = f.primary_request(&app, "ie").await;
    let owner = app.authenticated_client(&f.owner);
    let id = next["id"].as_str().unwrap();
    body(
        owner
            .post_json(
                &cancel_request_path(f.source, first["id"].as_str().unwrap()),
                &json!({}),
            )
            .await,
        StatusCode::CONFLICT,
    )
    .await;
    body(
        owner
            .post_json(&cancel_request_path(f.parent.id, id), &json!({}))
            .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    body(
        app.authenticated_client(&f.reviewer)
            .post_json(&cancel_request_path(f.source, id), &json!({}))
            .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    for _ in 0..2 {
        body(
            owner
                .post_json(&cancel_request_path(f.source, id), &json!({}))
                .await,
            StatusCode::OK,
        )
        .await;
    }
    body(
        f.decision(&app, &f.reviewer, &next, "approved").await,
        StatusCode::CONFLICT,
    )
    .await;
    let view = f.overview(&app, &f.owner).await;
    assert_eq!(view["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(view["decisions"][0]["outcome"], "approved");
    assert_eq!(
        view["requests"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["state"] == "cancelled")
            .count(),
        1
    );
    app.cleanup().await;
}

#[tokio::test]
async fn inactive_approvers_cannot_receive_new_assignments_but_keep_history() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let owner = &f.owner;
    let member = &f.reviewer;
    let org = &f.org;
    let fork_id = f.source;
    let definition_id = &f.definition;
    let client = app.authenticated_client(owner);
    let request = policy_request(&app, &f, 1, member.id).await;
    assert_eq!(
        app.authenticated_client(member)
            .post_json(
                &decisions_path(fork_id),
                &json!({
                    "definitionId": definition_id, "phase": "ie", "requestId": request["id"],
                    "outcome": "approved", "comment": "Historical sign-off", "reviewedGitHeads": f.overview(&app, member).await["heads"]
                })
            )
            .await
            .status(),
        StatusCode::OK
    );
    let before: Value = client
        .get(&approvals_path(fork_id))
        .await
        .json()
        .await
        .unwrap();
    let before = &before["data"]["outgoing"]["requirements"][0];

    assert_eq!(
        client
            .post_json(
                &format!("/api/v1/orgs/{}/members/{}/deactivate", org.id, member.id),
                &json!({"activityRevision": 0})
            )
            .await
            .status(),
        StatusCode::OK
    );

    let definitions = format!("/api/v1/orgs/{}/approval-definitions", org.id);
    let versions = format!("{definitions}/{definition_id}/versions");
    for path in [&definitions, &versions] {
        let response = client
            .post_json(
                path,
                &json!({
                    "name": "Unavailable approver", "humanUserIds": [member.id]
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value = response.json().await.unwrap();
        assert!(body["error"]["details"]["humanUserIds"]
            .as_str()
            .unwrap()
            .contains("not active"));
    }
    // Both explicit recipients and recipients derived from an old definition
    // must be active. Mixed requests are rejected without partial assignment.
    for recipients in [
        None,
        Some(json!([member.id])),
        Some(json!([f.second.id, member.id])),
    ] {
        let mut request =
            json!({"definitionId": definition_id, "definitionVersion": 1, "phase": "ie"});
        if let Some(recipients) = recipients {
            request["actorUserIds"] = recipients;
        }
        assert_eq!(
            client
                .post_json(&requests_path(fork_id), &request)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    let after: Value = client
        .get(&approvals_path(fork_id))
        .await
        .json()
        .await
        .unwrap();
    let after = &after["data"]["outgoing"]["requirements"][0];
    assert_eq!(after["definition"]["latestVersion"], 1);
    let historical_human = after["definition"]["humans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|human| human["userId"] == member.id.to_string())
        .unwrap();
    assert_eq!(historical_human["stillMember"], true);
    assert!(historical_human["deactivatedAt"].is_string());
    let historical_approver = after["approvers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|human| human["userId"] == member.id.to_string())
        .unwrap();
    assert_eq!(historical_approver["state"], "approved");
    assert_eq!(historical_approver["comment"], "Historical sign-off");
    assert!(historical_approver["deactivatedAt"].is_string());
    assert_eq!(after["decisionLog"], before["decisionLog"]);
    assert_eq!(after["requests"], before["requests"]);

    assert_eq!(client.post_json(&requests_path(fork_id), &json!({
        "definitionId": definition_id, "definitionVersion": 1, "phase": "ie", "actorUserIds": [f.second.id]
    })).await.status(), StatusCode::CREATED);
    assert_eq!(
        client
            .post_json(
                &format!("/api/v1/orgs/{}/members/{}/reactivate", org.id, member.id),
                &json!({"activityRevision": 1})
            )
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .post_json(
                &versions,
                &json!({
                    "name": "Active approver again", "humanUserIds": [member.id]
                })
            )
            .await
            .status(),
        StatusCode::OK
    );
    app.cleanup().await;
}

// test_contract/workspace_approvals_management D10
#[tokio::test]
async fn request_addressed_decision_keeps_exact_version_independent_of_recommendation() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = f.primary_request(&app, "ie").await;
    body(app.authenticated_client(&f.owner).post_json(&format!("/api/v1/orgs/{}/approval-definitions/{}/versions",f.org.id,f.definition),&json!({"name":"Security v2","humanUserIds":[f.reviewer.id],"phases":["be"],"invalidatesPrevious":false})).await,StatusCode::OK).await;
    for _ in 0..2 {
        body(
            f.decision(&app, &f.reviewer, &request, "approved").await,
            StatusCode::OK,
        )
        .await;
    }
    for (phase, status) in [
        ("bad", StatusCode::UNPROCESSABLE_ENTITY),
        ("be", StatusCode::OK),
    ] {
        let mut wrong = request.clone();
        wrong["phase"] = json!(phase);
        body(
            f.decision(&app, &f.reviewer, &wrong, "approved").await,
            status,
        )
        .await;
    }
    let rows:Vec<(i32,String)>=sqlx::query_as("SELECT approval_definition_version,phase FROM approval_statuses WHERE source_workspace_id=$1").bind(f.source).fetch_all(&app.db).await.unwrap();
    assert_eq!(rows, vec![(1, "ie".into())]);
    app.cleanup().await;
}

// test_contract/workspace_approvals_management D10
#[tokio::test]
async fn explicit_human_request_admission_and_settlement() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let owner = app.authenticated_client(&f.owner);
    let request = json!({"definitionId":f.definition,"definitionVersion":1,"phase":"se","actorUserIds":[f.reviewer.id]});
    // Requested before any PR (g5.governance.approval.request-pr); the same
    // request after the PR opens reuses it rather than adding a duplicate.
    let first = body(
        owner.post_json(&requests_path(f.source), &request).await,
        StatusCode::CREATED,
    )
    .await;
    f.submit(&app).await;
    let reused = body(
        owner.post_json(&requests_path(f.source), &request).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        reused["requests"][0]["requestId"],
        first["requests"][0]["requestId"]
    );
    assert_eq!(reused["requests"][0]["reused"], true);
    for actors in [
        json!([]),
        json!([f.owner.id]),
        json!([f.reviewer.id, f.reviewer.id]),
        json!([Uuid::new_v4()]),
    ] {
        let mut wrong = request.clone();
        wrong["actorUserIds"] = actors;
        body(
            owner.post_json(&requests_path(f.source), &wrong).await,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    let mut stale = request.clone();
    stale["definitionVersion"] = json!(999);
    assert!(owner
        .post_json(&requests_path(f.source), &stale)
        .await
        .status()
        .is_client_error());
    body(
        app.authenticated_client(&f.reviewer)
            .post_json(&requests_path(f.source), &request)
            .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let view = f.overview(&app, &f.reviewer).await;
    let task = &view["requests"][0];
    let decision = json!({"definitionId":f.definition,"requestId":task["id"],"phase":"se","outcome":"approved","reviewedGitHeads":view["heads"]});
    let a = app.authenticated_client(&f.reviewer);
    let b = app.authenticated_client(&f.reviewer);
    let path = decisions_path(f.source);
    let (one, two) = tokio::join!(a.post_json(&path, &decision), b.post_json(&path, &decision));
    body(one, StatusCode::OK).await;
    body(two, StatusCode::OK).await;
    let after = f.overview(&app, &f.owner).await;
    assert_eq!(after["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(after["requests"].as_array().unwrap().len(), 1);
    assert_eq!(after["requests"][0]["state"], "settled");
    app.cleanup().await;
}

/// Contract D2: an explicit `merge_target_workspace_id` beats the clone
/// parent — outgoing requirements and decisions resolve against it.
#[tokio::test]
async fn decision_uses_explicit_merge_target_over_clone_parent() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let alternate = TestWorkspace::create(&app, &f.org).await;
    let parent_only =
        create_definition(&app, &f.owner, f.org.id, "Parent only", &[f.reviewer.id]).await;
    body(
        app.authenticated_client(&f.owner)
            .post_json(
                &requirements_path(alternate.id),
                &json!({"definitionId":parent_only}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    set_merge_target(&app, f.source, f.parent.id).await;
    set_cloned_from(&app, f.source, alternate.id).await;
    let request = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let state = f.state(&app, &f.reviewer).await;
    assert_eq!(
        state["outgoing"]["targetWorkspaceId"],
        f.parent.id.to_string()
    );
    let reqs = state["outgoing"]["requirements"].as_array().unwrap();
    assert!(reqs.iter().all(|r| r["definition"]["id"] == f.definition));
    assert!(reqs
        .iter()
        .any(|r| r["decisionLog"].as_array().unwrap().len() == 1));
    app.cleanup().await;
}

// ===========================================================================
// test_contract/workspace_approvals_management D3
// ===========================================================================

/// An invalidating publish supersedes prior decisions in state but keeps
/// them, flagged, in the log; the boundary is one-way.
#[tokio::test]
async fn invalidation_supersedes_state_not_log() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let owner = app.authenticated_client(&f.owner);
    for (version, invalidates) in [(2, true), (3, false)] {
        body(owner.post_json(&format!("/api/v1/orgs/{}/approval-definitions/{}/versions",f.org.id,f.definition),&json!({"name":"Security","humanUserIds":[f.reviewer.id],"phases":["ie"],"invalidatesPrevious":invalidates})).await,StatusCode::OK).await;
        body(owner.post_json(&requests_path(f.source),&json!({"definitionId":f.definition,"definitionVersion":version,"phase":"ie","actorUserIds":[f.reviewer.id]})).await,StatusCode::CREATED).await;
        let state = f.state(&app, &f.reviewer).await;
        let req = &state["outgoing"]["requirements"][0];
        assert_eq!(req["state"], "pending");
        assert_eq!(req["callerCanDecide"], true);
        assert_eq!(req["decisionLog"].as_array().unwrap().len(), 1);
        assert_eq!(req["decisionLog"][0]["superseded"], true);
    }
    app.cleanup().await;
}

// ===========================================================================
// test_contract/workspace_approvals_management D11
// ===========================================================================

/// A recommendation never duplicates a target requirement or its pending request.
#[tokio::test]
async fn phase_recommendations_share_one_requirement_and_request() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let be = f.primary_request(&app, "be").await;
    let ie = f.primary_request(&app, "ie").await;
    assert_eq!(be["id"], ie["id"]);
    let initial = f.state(&app, &f.reviewer).await;
    assert_eq!(
        initial["outgoing"]["requirements"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(initial["outgoing"]["mergeAdmission"]["admitted"], false);
    body(
        app.authenticated_client(&f.owner)
            .patch_json(
                &format!("/api/v1/workspaces/{}", f.source),
                &json!({"phase":"ie"}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    body(
        f.decision(&app, &f.reviewer, &be, "approved").await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        f.state(&app, &f.owner).await["outgoing"]["mergeAdmission"]["admitted"],
        true
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE source_workspace_id=$1")
            .bind(f.source)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(rows, 1);
    app.cleanup().await;
}

/// A renewed request is independent of the prior settled task and other people.
#[tokio::test]
async fn request_retries_across_phases_do_not_create_another_chain() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let be = f.primary_request(&app, "be").await;
    assert_eq!(be["id"], f.primary_request(&app, "ie").await["id"]);
    body(
        f.decision(&app, &f.reviewer, &be, "approved").await,
        StatusCode::OK,
    )
    .await;
    let renewed = f.primary_request(&app, "se").await;
    assert_ne!(renewed["id"], be["id"]);
    body(
        app.authenticated_client(&f.owner)
            .post_json(
                &cancel_request_path(f.source, renewed["id"].as_str().unwrap()),
                &json!({}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let view = f.overview(&app, &f.owner).await;
    let requests = view["requests"].as_array().unwrap();
    assert_eq!(
        requests.iter().find(|r| r["id"] == be["id"]).unwrap()["state"],
        "settled"
    );
    assert_eq!(
        requests.iter().find(|r| r["id"] == renewed["id"]).unwrap()["state"],
        "cancelled"
    );
    assert_eq!(
        requests
            .iter()
            .find(|r| r["actorUserId"] == f.second.id.to_string())
            .unwrap()["state"],
        "pending"
    );
    assert_eq!(view["decisions"].as_array().unwrap().len(), 1);
    app.cleanup().await;
}

// ===========================================================================
// test_contract/workspace_approvals_management D12
// ===========================================================================

/// All attached target types gate merge, including unassociated and BE-only types.
#[tokio::test]
async fn unassociated_and_be_recommended_requirements_both_gate_merge() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let owner = app.authenticated_client(&f.owner);
    body(
        owner
            .post_json(&retire_path(f.parent.id, &f.definition), &json!({}))
            .await,
        StatusCode::OK,
    )
    .await;
    let unassociated = create_phase_definition(
        &app,
        &f.owner,
        f.org.id,
        "Unassociated",
        &[f.reviewer.id],
        &[],
    )
    .await;
    let be = create_phase_definition(
        &app,
        &f.owner,
        f.org.id,
        "BE recommended",
        &[f.reviewer.id],
        &["be"],
    )
    .await;
    for id in [&unassociated, &be] {
        body(
            owner
                .post_json(&requirements_path(f.parent.id), &json!({"definitionId":id}))
                .await,
            StatusCode::OK,
        )
        .await;
        body(owner.post_json(&requests_path(f.source),&json!({"definitionId":id,"definitionVersion":1,"phase":"se","actorUserIds":[f.reviewer.id]})).await,StatusCode::CREATED).await;
    }
    let state = f.state(&app, &f.owner).await;
    assert_eq!(
        state["outgoing"]["requirements"].as_array().unwrap().len(),
        2
    );
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], false);
    let view = f.overview(&app, &f.reviewer).await;
    for (index, id) in [&unassociated, &be].iter().enumerate() {
        let request = view["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["definitionId"] == **id)
            .unwrap();
        body(
            f.decision(&app, &f.reviewer, request, "approved").await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(
            f.state(&app, &f.owner).await["outgoing"]["mergeAdmission"]["admitted"],
            index == 1
        );
        let mut db = app.scoped_db_for_org(f.org.id).await;
        g5_api::services::approval_config_service::lock_merge_admission(
            &mut db,
            f.org.id,
            f.parent.id,
        )
        .await
        .unwrap();
        let heads: Vec<g5_contracts::types::MergeSourceHead> =
            serde_json::from_value(view["heads"].clone()).unwrap();
        let admission = g5_api::services::approval_config_service::evaluate_merge_admission(
            &mut db,
            f.org.id,
            f.source,
            f.parent.id,
            &heads,
        )
        .await
        .unwrap();
        assert_eq!(admission.admitted, index == 1);
        assert_eq!(admission.requirements.len(), 2);
        db.rollback().await.unwrap();
    }
    app.cleanup().await;
}

// ===========================================================================
// test_contract/workspace_approvals_management D4
// ===========================================================================

/// Admin-only attach/retire; approver-only decisions; typed 4xxs, no rows.
#[tokio::test]
async fn authorization_and_validation_boundaries() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let task = f.primary_request(&app, "ie").await;
    let owner = app.authenticated_client(&f.owner);
    let reviewer = app.authenticated_client(&f.reviewer);
    body(
        reviewer
            .post_json(
                &requirements_path(f.parent.id),
                &json!({"definitionId":f.definition}),
            )
            .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    body(
        reviewer
            .post_json(&retire_path(f.parent.id, &f.definition), &json!({}))
            .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    body(
        owner
            .post_json(
                &requirements_path(f.parent.id),
                &json!({"definitionId":Uuid::new_v4()}),
            )
            .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let heads = f.overview(&app, &f.reviewer).await["heads"].clone();
    let decision = json!({"definitionId":f.definition,"requestId":task["id"],"phase":"ie","outcome":"approved","reviewedGitHeads":heads});
    body(
        owner.post_json(&decisions_path(f.source), &decision).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    for patch in [
        json!({"outcome":"maybe"}),
        json!({"comment":"  "}),
        json!({"outcome":"rejected"}),
        json!({"phase":"bad"}),
        json!({"reviewedGitHeads":[]}),
    ] {
        let mut wrong = decision.clone();
        wrong
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        body(
            reviewer.post_json(&decisions_path(f.source), &wrong).await,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    let mut wrong = decision.clone();
    wrong["requestId"] = json!(Uuid::new_v4());
    body(
        reviewer.post_json(&decisions_path(f.source), &wrong).await,
        StatusCode::CONFLICT,
    )
    .await;
    wrong.as_object_mut().unwrap().remove("requestId");
    body(
        reviewer.post_json(&decisions_path(f.source), &wrong).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert!(f.overview(&app, &f.owner).await["decisions"]
        .as_array()
        .unwrap()
        .is_empty());
    app.cleanup().await;
}

/// Requirement administration is organization authority, not workspace
/// ownership (issue #278): a non-owner org admin attaches and retires
/// requirements on Base owned by another identity, while members and other
/// tenants stay denied and the admin gains no owner-only workspace content,
/// sharing or VM authority.
#[tokio::test]
async fn org_admin_administers_requirements_on_base_owned_by_another_identity() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let base_owner: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id = $1")
        .bind(base.id)
        .fetch_one(&app.db)
        .await
        .expect("read base owner");
    assert_eq!(base_owner, owner.id);

    let admin = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::OrgAdmin,
        ],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(admin.id)
        .execute(&app.db)
        .await
        .expect("promote non-owner organization administrator");
    let member = member_user(&app, org.id).await;
    let mut other_tenant = TestUser::create(&app).await;
    TestOrg::create_with_admin(&app, &mut other_tenant).await;

    let definition_id = create_definition(&app, &owner, org.id, "Base sign-off", &[owner.id]).await;
    // Requirement administration is exercised with an `org:admin`-only bearer;
    // the role-shaped bearer (with workspace scopes) probes the owner-only
    // boundaries below, so those denials come from ownership, not scope.
    let admin_only = TestUser {
        id: admin.id,
        email: admin.email.clone(),
        display_name: admin.display_name.clone(),
        jwt: helpers::test_jwt_minter()
            .mint(admin.id, org.id, vec![Scope::OrgAdmin], 3600)
            .expect("mint org-admin-only JWT"),
    };
    let org_admin_client = app.authenticated_client(&admin_only);
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&member);
    let other_client = app.authenticated_client(&other_tenant);

    // Callers without organization administration of Base's org are denied.
    let response = member_client
        .post_json(
            &requirements_path(base.id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = other_client
        .post_json(
            &requirements_path(base.id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert!(
        matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ),
        "cross-tenant attach must be denied, got {}",
        response.status()
    );
    assert!(event_history(&app, base.id).await.is_empty());

    // The non-owner admin attaches; the saved requirement is attributed and visible.
    let response = org_admin_client
        .post_json(
            &requirements_path(base.id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("parse JSON");
    assert_eq!(status, StatusCode::OK, "admin attach: {body}");
    let required = body["data"]["requiredInto"].as_array().expect("array");
    assert_eq!(required.len(), 1);
    assert_eq!(required[0]["definition"]["id"], definition_id.as_str());
    assert_eq!(required[0]["attachedBy"]["userId"], admin.id.to_string());
    let response = member_client.get(&approvals_path(base.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    assert_eq!(body["data"]["requiredInto"].as_array().unwrap().len(), 1);

    // Retire is denied to the member and other tenant, then admitted for the admin.
    let response = member_client
        .post_json(&retire_path(base.id, &definition_id), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = other_client
        .post_json(&retire_path(base.id, &definition_id), &json!({}))
        .await;
    assert!(
        matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ),
        "cross-tenant retire must be denied, got {}",
        response.status()
    );
    let response = org_admin_client
        .post_json(&retire_path(base.id, &definition_id), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    assert_eq!(body["data"]["requiredInto"].as_array().unwrap().len(), 0);
    let history = body["data"]["attachmentHistory"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["retired"], true);
    assert_eq!(history[1]["retired"], false);
    assert_eq!(event_history(&app, base.id).await, vec![false, true]);

    // Requirement administration confers no owner-only workspace authority.
    let response = admin_client
        .post_json(
            &requests_path(base.id),
            &json!({ "definitionId": definition_id, "definitionVersion": 1, "phase": "ie" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = admin_client
        .post_json(
            &format!("/api/v1/workspaces/{}/members", base.id),
            &json!({ "userId": member.id.to_string(), "role": "editor" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = admin_client
        .patch_json(
            &format!("/api/v1/workspaces/{}", base.id),
            &json!({ "description": "admin edit" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = admin_client
        .post_json(&format!("/api/v1/workspaces/{}/stop", base.id), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
}

/// A pending request addressed to a non-owner is theirs to decide (issue #278
/// follow-up): they reject and withdraw through the request without owning the
/// source, every successor in the chain stays the requester's to cancel, and a
/// token without `workspaces:read` cannot record the decision.
#[tokio::test]
async fn addressed_nonowner_decides_while_requester_keeps_the_chain() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = f.primary_request(&app, "se").await;
    body(
        f.decision(&app, &f.owner, &request, "approved").await,
        StatusCode::FORBIDDEN,
    )
    .await;
    body(
        f.decision(&app, &f.visitor, &request, "approved").await,
        StatusCode::CONFLICT,
    )
    .await;
    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let next = f.primary_request(&app, "se").await;
    assert_ne!(next["id"], request["id"]);
    assert_eq!(next["requestedBy"], f.owner.id.to_string());
    assert_eq!(next["actorUserId"], f.reviewer.id.to_string());
    app.cleanup().await;
}

/// An addressed human needs active organization membership to decide;
/// deactivation refuses the write until they are reactivated.
#[tokio::test]
async fn addressed_human_needs_current_read_access() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = f.primary_request(&app, "ie").await;
    let heads = f.overview(&app, &f.reviewer).await["heads"].clone();
    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let decision = json!({"definitionId":f.definition,"requestId":request["id"],"outcome":"approved","phase":"ie","reviewedGitHeads":heads});
    let client = app.authenticated_client(&f.reviewer);
    body(
        client.post_json(&decisions_path(f.source), &decision).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert!(f.overview(&app, &f.owner).await["decisions"]
        .as_array()
        .unwrap()
        .is_empty());
    sqlx::query("UPDATE org_memberships SET deactivated_at=NULL WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    body(
        client.post_json(&decisions_path(f.source), &decision).await,
        StatusCode::OK,
    )
    .await;
    app.cleanup().await;
}

/// Requirement administration needs organization-level administration: a
/// workspace-scoped `org:admin` grant held by a plain member, even with an
/// `org:admin` token from another organization they administer, is refused.
#[tokio::test]
async fn workspace_scoped_org_admin_grant_cannot_administer_requirements() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let definition_id = create_definition(&app, &owner, org.id, "Override", &[owner.id]).await;
    let mut outsider = TestUser::create(&app).await;
    TestOrg::create_with_admin(&app, &mut outsider).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(outsider.id)
        .execute(&app.db)
        .await
        .expect("add plain membership");
    sqlx::query(
        "INSERT INTO oauth_scopes (scope, description, category) \
         VALUES ('org:admin', 'test', 'test') ON CONFLICT (scope) DO NOTHING",
    )
    .execute(&app.db)
    .await
    .expect("seed scope catalog");
    sqlx::query(
        "INSERT INTO workspace_permission_overrides \
         (workspace_id, user_id, scope, granted_by, granted_at) \
         VALUES ($1, $2, 'org:admin', $3, now())",
    )
    .bind(base.id)
    .bind(outsider.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("grant workspace-scoped org:admin");
    let client = app.authenticated_client(&outsider);

    let response = client
        .post_json(
            &requirements_path(base.id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(event_history(&app, base.id).await.is_empty());

    // Seed an active requirement as the owner; the grant cannot retire it.
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &requirements_path(base.id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = client
        .post_json(&retire_path(base.id, &definition_id), &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(event_history(&app, base.id).await, vec![false]);

    app.cleanup().await;
}

/// Lost acknowledgements cannot create another task after a fast reviewer
/// settles the original. A new explicit operation may request another review.
#[tokio::test]
async fn request_operation_replay_preserves_settled_identity() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let owner = app.authenticated_client(&f.owner);
    let path = requests_path(f.source);
    let mut payload = json!({
        "clientRequestId": Uuid::new_v4(), "definitionId":f.definition,
        "definitionVersion":1, "phase":"se", "actorUserIds":[f.reviewer.id]
    });
    let first = body(owner.post_json(&path, &payload).await, StatusCode::CREATED).await;
    let id = &first["requests"][0]["requestId"];
    let pending_retry = body(owner.post_json(&path, &payload).await, StatusCode::OK).await;
    assert_eq!(&pending_retry["requests"][0]["requestId"], id);
    let mut duplicate = payload.clone();
    duplicate["clientRequestId"] = json!(Uuid::new_v4());
    assert_eq!(
        owner.post_json(&path, &duplicate).await.status(),
        StatusCode::CONFLICT
    );
    let overview = f.overview(&app, &f.reviewer).await;
    let request = overview["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| &r["id"] == id)
        .unwrap();
    body(
        f.decision(&app, &f.reviewer, request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let replay = body(owner.post_json(&path, &payload).await, StatusCode::OK).await;
    assert_eq!(&replay["requests"][0]["requestId"], id);
    let after = f.overview(&app, &f.owner).await;
    assert_eq!(after["requests"].as_array().unwrap().len(), 1);
    assert_eq!(after["requests"][0]["state"], "settled");
    let mut changed = payload.clone();
    changed["note"] = json!("Different request context");
    assert_eq!(
        owner.post_json(&path, &changed).await.status(),
        StatusCode::CONFLICT
    );
    payload["clientRequestId"] = json!(Uuid::new_v4());
    let fresh = body(owner.post_json(&path, &payload).await, StatusCode::CREATED).await;
    assert_ne!(&fresh["requests"][0]["requestId"], id);
    app.cleanup().await;
}

/// Closing a PR keeps a settled decision's effect and its audit trail, but
/// gives an undecided request no authority; legacy rows remain history
/// (g5.governance.approval.request-pr).
#[tokio::test]
async fn closed_pr_keeps_settled_decisions_and_legacy_rows_remain_history() {
    for legacy in [false, true] {
        let app = TestApp::spawn().await;
        let f = Flow::new(&app).await;
        f.submit(&app).await;
        let request = f.primary_request(&app, "se").await;
        let undecided = f.overview(&app, &f.second).await["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["actorUserId"] == f.second.id.to_string() && r["state"] == "pending")
            .unwrap()
            .clone();
        body(
            f.decision(&app, &f.reviewer, &request, "approved").await,
            StatusCode::OK,
        )
        .await;
        let overview = f.overview(&app, &f.owner).await;
        if legacy {
            sqlx::query(
                "UPDATE approval_statuses SET pull_request_id=NULL WHERE source_workspace_id=$1",
            )
            .bind(f.source)
            .execute(&app.db)
            .await
            .unwrap();
        }
        let pr_id = overview["pr"]["id"].as_str().unwrap();
        body(
            app.authenticated_client(&f.owner)
                .post_json(&f.path(&format!("pull-requests/{pr_id}/close")), &json!({}))
                .await,
            StatusCode::OK,
        )
        .await;
        let state = f.state(&app, &f.reviewer).await;
        let requirement = state["outgoing"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["definition"]["id"] == json!(f.definition))
            .unwrap();
        assert_eq!(requirement["decisionLog"].as_array().unwrap().len(), 1);
        assert_eq!(requirement["callerCanDecide"], false);
        let admitted = state["outgoing"]["mergeAdmission"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["definitionId"] == json!(f.definition))
            .unwrap()["state"]
            .clone();
        if legacy {
            assert_ne!(requirement["state"], "approved");
            assert_ne!(admitted, "approved");
        } else {
            assert_eq!(requirement["state"], "approved", "{requirement}");
            assert_eq!(admitted, "approved");
        }
        // The closed PR's undecided request gains no authority.
        let other = f.state(&app, &f.second).await;
        let other = other["outgoing"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["definition"]["id"] == json!(f.definition))
            .unwrap()
            .clone();
        assert_eq!(other["callerCanDecide"], false);
        assert_eq!(
            f.decision(&app, &f.second, &undecided, "rejected")
                .await
                .status(),
            StatusCode::CONFLICT
        );
        let current = f.overview(&app, &f.reviewer).await;
        assert!(current["pr"].is_null());
        assert_eq!(current["decisions"].as_array().unwrap().len(), 0);
        if legacy {
            // Decisions recorded before requests existed carry no request.
            sqlx::query(
                "UPDATE approval_statuses SET request_id=NULL WHERE source_workspace_id=$1",
            )
            .bind(f.source)
            .execute(&app.db)
            .await
            .unwrap();
        }
        let activity = body(
            app.authenticated_client(&f.visitor)
                .get(&f.path("activity"))
                .await,
            StatusCode::OK,
        )
        .await;
        let approval = activity["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == "approval_decision")
            .unwrap();
        assert_eq!(
            approval.get("pullRequestId").unwrap(),
            &if legacy { Value::Null } else { json!(pr_id) }
        );
        // A human decision exposes its comment and no agent summary;
        // `historical` marks only legacy rows recorded without a request.
        assert_eq!(approval["historical"], json!(legacy), "{approval}");
        assert_eq!(approval["summary"], Value::Null, "{approval}");
        assert_eq!(approval["actorType"], "human");
        app.cleanup().await;
    }
}

fn policy_ie(state: &Value) -> &Value {
    state["outgoing"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["phase"] == "ie")
        .unwrap()
}

async fn publish_reviewer_policy(
    app: &TestApp,
    f: &Flow,
    all: bool,
    self_approval: bool,
    humans: &[Uuid],
) -> Value {
    body(
        app.authenticated_client(&f.owner)
            .post_json(
                &format!(
                    "/api/v1/orgs/{}/approval-definitions/{}/versions",
                    f.org.id, f.definition
                ),
                &json!({"name":"Reviewer policy", "humanUserIds":humans, "phases":["be","se","ie"],
            "requireAllReviewers":all, "allowSelfApproval":self_approval}),
            )
            .await,
        StatusCode::OK,
    )
    .await
}

async fn policy_request(app: &TestApp, f: &Flow, version: i32, actor: Uuid) -> Value {
    body(app.authenticated_client(&f.owner).post_json(&f.path("approvals/requests"),
        &json!({"definitionId":f.definition,"definitionVersion":version,"phase":"ie","actorUserIds":[actor]})
    ).await, StatusCode::CREATED).await;
    f.overview(app, &f.owner).await["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actorUserId"] == actor.to_string() && r["state"] == "pending")
        .unwrap()
        .clone()
}

#[tokio::test]
async fn reviewer_policy_any_approval_passes_then_denial_vetoes_without_settling_others() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let definition = body(
        app.authenticated_client(&f.owner)
            .get(&format!("/api/v1/orgs/{}/approval-definitions", f.org.id))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(definition[0]["requireAllReviewers"], false);
    assert_eq!(definition[0]["allowSelfApproval"], false);
    f.submit(&app).await;
    f.request(&app, "ie").await;
    let view = f.overview(&app, &f.reviewer).await;
    let request = view["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actorUserId"] == f.reviewer.id.to_string())
        .unwrap();
    body(
        f.decision(&app, &f.reviewer, request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let state = f.state(&app, &f.owner).await;
    assert_eq!(policy_ie(&state)["state"], "approved");
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], true);
    let view = f.overview(&app, &f.second).await;
    let other = view["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actorUserId"] == f.second.id.to_string())
        .unwrap();
    assert_eq!(other["state"], "pending");
    body(
        f.decision(&app, &f.second, other, "rejected").await,
        StatusCode::OK,
    )
    .await;
    let state = f.state(&app, &f.owner).await;
    assert_eq!(policy_ie(&state)["state"], "rejected");
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], false);
}

#[tokio::test]
async fn reviewer_policy_all_waits_for_every_eligible_reviewer() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let def = publish_reviewer_policy(
        &app,
        &f,
        true,
        false,
        &[f.owner.id, f.reviewer.id, f.second.id],
    )
    .await;
    assert_eq!(def["requireAllReviewers"], true);
    assert_eq!(def["allowSelfApproval"], false);
    f.submit(&app).await;
    let first = policy_request(&app, &f, 2, f.reviewer.id).await;
    let second = policy_request(&app, &f, 2, f.second.id).await;
    body(
        f.decision(&app, &f.reviewer, &first, "approved").await,
        StatusCode::OK,
    )
    .await;
    let state = f.state(&app, &f.owner).await;
    assert_eq!(policy_ie(&state)["state"], "pending");
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], false);
    body(
        f.decision(&app, &f.second, &second, "approved").await,
        StatusCode::OK,
    )
    .await;
    let state = f.state(&app, &f.owner).await;
    assert_eq!(policy_ie(&state)["state"], "approved");
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], true);
}

#[tokio::test]
async fn reviewer_policy_self_requires_requested_and_current_permission_and_survives_retry() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let refused = app.authenticated_client(&f.owner).post_json(&f.path("approvals/requests"),
        &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie","actorUserIds":[f.owner.id]})).await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    publish_reviewer_policy(&app, &f, false, true, &[f.owner.id, f.reviewer.id]).await;
    // An old immutable version cannot acquire self authority from a newer version.
    let refused = app.authenticated_client(&f.owner).post_json(&f.path("approvals/requests"),
        &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie","actorUserIds":[f.owner.id]})).await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let request = policy_request(&app, &f, 2, f.owner.id).await;
    let home = body(
        app.authenticated_client(&f.owner)
            .get(&format!("/api/v1/orgs/{}/home/attention", f.org.id))
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(home["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["kind"] == "approval" && i["workspaceId"] == f.source.to_string()));
    assert_eq!(request["allowSelfApproval"], true);
    let state = f.state(&app, &f.owner).await;
    assert_eq!(policy_ie(&state)["callerCanDecide"], true);
    body(
        f.decision(&app, &f.owner, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    body(
        f.decision(&app, &f.owner, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let view = f.overview(&app, &f.owner).await;
    assert_eq!(view["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(
        f.state(&app, &f.owner).await["outgoing"]["mergeAdmission"]["admitted"],
        true
    );
    let next = policy_request(&app, &f, 2, f.owner.id).await;
    publish_reviewer_policy(&app, &f, false, false, &[f.owner.id, f.reviewer.id]).await;
    assert_eq!(
        f.decision(&app, &f.owner, &next, "approved").await.status(),
        StatusCode::FORBIDDEN
    );
    let state = f.state(&app, &f.owner).await;
    assert_eq!(policy_ie(&state)["callerCanDecide"], false);
    let home = body(
        app.authenticated_client(&f.owner)
            .get(&format!("/api/v1/orgs/{}/home/attention", f.org.id))
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(!home["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["kind"] == "approval" && i["workspaceId"] == f.source.to_string()));
    assert_eq!(policy_ie(&state)["state"], "pending");
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], false);
    let view = f.overview(&app, &f.owner).await;
    assert!(view["requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["state"] == "pending")
        .all(|r| r["allowSelfApproval"] == false));
}

#[tokio::test]
async fn reviewer_policy_transfer_does_not_add_self_authority_to_an_old_request() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let request = policy_request(&app, &f, 1, f.reviewer.id).await;
    publish_reviewer_policy(&app, &f, false, true, &[f.owner.id, f.reviewer.id]).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.source)
        .bind(f.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        f.decision(&app, &f.reviewer, &request, "approved")
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let state = f.state(&app, &f.reviewer).await;
    assert_eq!(policy_ie(&state)["callerCanDecide"], false);
    let view = f.overview(&app, &f.reviewer).await;
    assert_eq!(view["requests"][0]["state"], "pending");
    assert_eq!(view["decisions"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn reviewer_policy_withdrawal_without_comment_uses_neutral_timeline_text() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "ie").await;
    let view = f.overview(&app, &f.reviewer).await;
    let request = view["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actorUserId"] == f.reviewer.id.to_string())
        .unwrap();
    body(
        app.authenticated_client(&f.reviewer)
            .post_json(
                &f.path("approvals/decisions"),
                &json!({"definitionId":f.definition,"requestId":request["id"],"phase":"ie",
          "outcome":"deferred","reviewedGitHeads":view["heads"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let content: String = sqlx::query_scalar("SELECT c.content FROM approval_statuses s JOIN comment_index c ON c.id=s.comment_id WHERE s.source_workspace_id=$1 AND s.outcome='deferred'")
        .bind(f.source).fetch_one(&app.db).await.unwrap();
    assert_eq!(content, "Decision withdrawn");
    let state = f.state(&app, &f.owner).await;
    assert_eq!(policy_ie(&state)["state"], "pending");
    assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], false);
}

/// `PR_REQUIRED` applies one rule to human and agent requests
/// (g5.governance.approval.request-pr). With `false`, a reviewer requested
/// before any PR decides, and that no-PR evidence keeps counting once a PR
/// opens; with `true`, both kinds wait for the PR.
#[tokio::test]
async fn request_pr_rule_applies_to_both_kinds() {
    for pr_required in [false, true] {
        let mut config = helpers::test_config();
        config.approval_policy.pr_required = pr_required;
        let app = TestApp::spawn_with_config(config).await;
        let f = Flow::new(&app).await;
        let owner = app.authenticated_client(&f.owner);
        let agent_definition = body(
            owner
                .post_json(
                    &format!("/api/v1/orgs/{}/approval-definitions", f.org.id),
                    &json!({"name":"Architecture","humanUserIds":[],
                        "agentType":"simple","agentPrompt":"Review the exact cut."}),
                )
                .await,
            StatusCode::CREATED,
        )
        .await["id"]
            .as_str()
            .unwrap()
            .to_owned();
        body(
            owner
                .post_json(
                    &requirements_path(f.parent.id),
                    &json!({"definitionId": agent_definition}),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        let post = |path: String, payload: Value| {
            let owner = &owner;
            async move { owner.post_json(&path, &payload).await }
        };
        let request_human = || {
            post(
                requests_path(f.source),
                json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie",
                    "actorUserIds":[f.reviewer.id],"clientRequestId":Uuid::new_v4()}),
            )
        };
        let request_agent = || {
            post(
                f.path("approvals/reviews"),
                json!({"definitionId":agent_definition,"definitionVersion":1,"phase":"ie",
                    "clientRequestId":Uuid::new_v4()}),
            )
        };
        let state = f.state(&app, &f.owner).await;
        assert_eq!(
            state["outgoing"]["requestPolicy"]["prRequired"],
            json!(pr_required)
        );

        let human = request_human().await;
        let agent = request_agent().await;
        if pr_required {
            assert_eq!(human.status(), StatusCode::CONFLICT);
            assert_eq!(agent.status(), StatusCode::CONFLICT);
            f.submit(&app).await;
            assert!(request_human().await.status().is_success());
            assert!(request_agent().await.status().is_success());
            app.cleanup().await;
            continue;
        }
        assert_eq!(human.status(), StatusCode::CREATED);
        assert!(agent.status().is_success(), "agent request without a PR");

        // The reviewer decides the no-PR request.
        let request = f.overview(&app, &f.reviewer).await["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["actorUserId"] == f.reviewer.id.to_string() && r["state"] == "pending")
            .expect("no-PR request is visible to its reviewer")
            .clone();
        body(
            f.decision(&app, &f.reviewer, &request, "approved").await,
            StatusCode::OK,
        )
        .await;
        let requirement = |state: &Value| {
            state["outgoing"]["requirements"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["definition"]["id"] == json!(f.definition))
                .unwrap()
                .clone()
        };
        assert_eq!(
            requirement(&f.state(&app, &f.owner).await)["state"],
            "approved"
        );

        // A duplicate active request is refused, PR or not.
        let second = || {
            post(
                requests_path(f.source),
                json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie",
                    "actorUserIds":[f.second.id],"clientRequestId":Uuid::new_v4()}),
            )
        };
        assert_eq!(second().await.status(), StatusCode::CREATED);
        f.submit(&app).await;
        assert_eq!(second().await.status(), StatusCode::CONFLICT);

        // Activity records the no-PR request and its decision as current, not
        // historical, evidence (g5.workbench.landing.approval-events).
        let activity = body(
            app.authenticated_client(&f.visitor)
                .get(&f.path("activity"))
                .await,
            StatusCode::OK,
        )
        .await;
        let entries = activity["entries"].as_array().unwrap();
        assert!(
            entries
                .iter()
                .filter(|e| e["kind"] == "approval_requested")
                .count()
                >= 2,
            "no-PR requests appear in Activity: {activity}"
        );
        let decision = entries
            .iter()
            .find(|e| e["kind"] == "approval_decision")
            .expect("decision in Activity");
        assert_eq!(decision["historical"], false, "{decision}");

        // Opening the PR does not retire the no-PR evidence.
        let state = f.state(&app, &f.owner).await;
        assert_eq!(requirement(&state)["state"], "approved");
        assert_eq!(state["outgoing"]["mergeAdmission"]["admitted"], false);
        assert_eq!(
            state["outgoing"]["mergeAdmission"]["reason"], "Approval(s) required before merging",
            "the agent-only requirement still waits"
        );

        // Neither the flag becoming true nor the PR closing retires it.
        let definition = Uuid::parse_str(&f.definition).unwrap();
        let flipped = || {
            crate::api_agent_approval_evaluation::state_with_pr_required(
                &app, f.org.id, f.source, f.owner.id, definition, true,
            )
        };
        assert_eq!(flipped().await, "approved");
        let pr_id = f.overview(&app, &f.owner).await["pr"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        body(
            owner
                .post_json(&f.path(&format!("pull-requests/{pr_id}/close")), &json!({}))
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(
            requirement(&f.state(&app, &f.owner).await)["state"],
            "approved"
        );
        assert_eq!(flipped().await, "approved");
        app.cleanup().await;
    }
}
