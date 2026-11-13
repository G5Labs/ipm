//! Guided-journey marks (workbench `g5.workbench.journey.marks`): the owner's
//! durable "Mark done" / "Not done" per step through
//! `PUT /api/v1/workspaces/:id/journey/steps/:stepId {done}`, read back as the
//! latest mark per step from `GET /api/v1/workspaces/:id/journey`, idempotent,
//! owner-only, refused on Base, and projected on the timeline as
//! `journey_step` entries (`g5.workbench.landing`).
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints. No mocks.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn journey(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Vec<Value> {
    let response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}/journey"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    body["data"]["marks"].as_array().expect("marks").clone()
}

async fn mark(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    step_id: &str,
    done: bool,
) -> (StatusCode, Value) {
    mark_with(client, workspace_id, step_id, json!({ "done": done })).await
}

async fn mark_with(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    step_id: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{workspace_id}/journey/steps/{step_id}"),
            &body,
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// `(step_id, done, actor_user_id)` rows, oldest first.
async fn journey_events(app: &TestApp, workspace_id: Uuid) -> Vec<(String, bool, Option<Uuid>)> {
    sqlx::query_as::<_, (String, bool, Option<Uuid>)>(
        "SELECT step_id, done, actor_user_id FROM workspace_journey_events \
         WHERE workspace_id = $1 ORDER BY seq",
    )
    .bind(workspace_id)
    .fetch_all(&app.db)
    .await
    .expect("Failed to load journey events")
}

#[tokio::test]
async fn owner_marks_a_step_done_and_not_done_and_the_latest_mark_is_reported() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    assert!(journey(&client, workspace.id).await.is_empty());

    let (status, body) = mark(&client, workspace.id, "se-diff-review", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let marks = body["data"]["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0]["stepId"], json!("se-diff-review"));
    assert_eq!(marks[0]["done"], json!(true));
    assert_eq!(marks[0]["actorUserId"], json!(owner.id.to_string()));

    // Marking the same state again records no second event.
    let (status, _) = mark(&client, workspace.id, "se-diff-review", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(journey_events(&app, workspace.id).await.len(), 1);

    // "Not done" is a new event; the read reports the latest one.
    let (status, _) = mark(&client, workspace.id, "se-diff-review", false).await;
    assert_eq!(status, StatusCode::OK);
    let marks = journey(&client, workspace.id).await;
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0]["done"], json!(false));
    assert_eq!(
        journey_events(&app, workspace.id).await,
        vec![
            ("se-diff-review".to_string(), true, Some(owner.id)),
            ("se-diff-review".to_string(), false, Some(owner.id)),
        ]
    );

    // "Not done" on a never-marked step records nothing.
    let (status, _) = mark(&client, workspace.id, "be-intent", false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(journey_events(&app, workspace.id).await.len(), 2);

    // The timeline projects every event as a journey_step entry.
    let response = client
        .get(&format!("/api/v1/workspaces/{}/activity", workspace.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    let entries = body["data"]["entries"].as_array().expect("entries");
    let mut steps: Vec<&Value> = entries
        .iter()
        .filter(|entry| entry["kind"] == json!("journey_step"))
        .collect();
    assert_eq!(steps.len(), 2, "{entries:?}");
    steps.sort_by_key(|entry| entry["at"].as_str().unwrap_or("").to_string());
    assert_eq!(steps[0]["stepId"], json!("se-diff-review"));
    assert_eq!(steps[0]["done"], json!(true), "the first mark, oldest");
    assert_eq!(steps[1]["done"], json!(false), "then its undo");
    assert_eq!(steps[1]["actor"]["userId"], json!(owner.id.to_string()));

    app.cleanup().await;
}

#[tokio::test]
async fn a_non_owner_member_cannot_mark_but_can_read() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let owner_client = app.authenticated_client(&owner);
    let (status, _) = mark(&owner_client, workspace.id, "be-mock", true).await;
    assert_eq!(status, StatusCode::OK);

    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&member);

    let (status, body) = mark(&client, workspace.id, "be-mock", false).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(journey_events(&app, workspace.id).await.len(), 1);

    let marks = journey(&client, workspace.id).await;
    assert_eq!(marks.len(), 1, "a reader sees the owner's marks");
    assert_eq!(marks[0]["done"], json!(true));

    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_marks_on_different_and_same_steps_all_land_without_a_deadlock() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    // Two requests for different steps overlap: both commit (the route's
    // FOR SHARE plus the service's advisory lock never upgrade a row lock).
    let (a, b) = tokio::join!(
        mark(&client, workspace.id, "se-plan", true),
        mark(&client, workspace.id, "se-code", true),
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(b.0, StatusCode::OK, "{}", b.1);
    let marks = journey(&client, workspace.id).await;
    assert_eq!(marks.len(), 2);
    assert!(marks.iter().all(|mark| mark["done"] == json!(true)));

    // Two requests for one step with opposite marks overlap: exactly one
    // event per effective change, and the read reports the last one landed.
    let (c, d) = tokio::join!(
        mark(&client, workspace.id, "se-plan", false),
        mark(&client, workspace.id, "se-plan", true),
    );
    assert_eq!(c.0, StatusCode::OK, "{}", c.1);
    assert_eq!(d.0, StatusCode::OK, "{}", d.1);
    let events = journey_events(&app, workspace.id).await;
    let plan_events: Vec<bool> = events
        .iter()
        .filter(|(step, _, _)| step == "se-plan")
        .map(|(_, done, _)| *done)
        .collect();
    let last_event = *plan_events.last().expect("plan events");
    let reported = journey(&client, workspace.id)
        .await
        .into_iter()
        .find(|mark| mark["stepId"] == json!("se-plan"))
        .expect("plan mark")["done"]
        .clone();
    assert_eq!(reported, json!(last_event), "the latest event is the state");
    for pair in plan_events.windows(2) {
        assert_ne!(pair[0], pair[1], "no event repeats the state it found");
    }
    // The timeline's `created_at` order agrees with the write order (`seq`).
    let stamped: Vec<(i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT seq, created_at FROM workspace_journey_events \
         WHERE workspace_id = $1 ORDER BY seq",
    )
    .bind(workspace.id)
    .fetch_all(&app.db)
    .await
    .expect("events");
    for pair in stamped.windows(2) {
        assert!(
            pair[0].1 <= pair[1].1,
            "created_at follows seq: {stamped:?}"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn a_read_only_or_freezing_workspace_refuses_marks() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    for seed in [
        "UPDATE workspaces SET mutation_policy = 'read_only' WHERE id = $1",
        "UPDATE workspaces SET mutation_policy = 'read_write', mutation_policy_transition = 'freezing' WHERE id = $1",
    ] {
        sqlx::query(seed)
            .bind(workspace.id)
            .execute(&app.db)
            .await
            .expect("seed policy");
        let (status, body) = mark(&client, workspace.id, "se-plan", true).await;
        assert_ne!(status, StatusCode::OK, "{seed}: {body}");
        assert!(journey_events(&app, workspace.id).await.is_empty(), "{seed}");
    }

    app.cleanup().await;
}

#[tokio::test]
async fn an_invalid_step_id_is_refused_and_base_has_no_journey_marks() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    for bad in [
        "Bad-Step",
        "-leading",
        "step_with_underscore",
        "x".repeat(65).as_str(),
    ] {
        let (status, body) = mark(&client, workspace.id, bad, true).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}: {status} {body}"
        );
    }
    assert!(journey_events(&app, workspace.id).await.is_empty());

    // Base is read-only Business Engineering: no marks.
    let base = TestWorkspace::create_base(&app, &org).await;
    let (status, body) = mark(&client, base.id, "be-mock", true).await;
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(journey_events(&app, base.id).await.is_empty());

    app.cleanup().await;
}

/// A skip is the owner's third mark (ui g5.ui.guided-panel): not done, left
/// out of the count, reversible like a mark, never together with done.
#[tokio::test]
async fn owner_skips_a_step_and_undoes_the_skip_like_a_mark() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let (status, body) = mark_with(
        &client,
        workspace.id,
        "se-explainer",
        json!({ "done": false, "skipped": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let marks = body["data"]["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0]["done"], json!(false));
    assert_eq!(marks[0]["skipped"], json!(true));

    // Skipping again records nothing; a plain mark without the field reads
    // as not skipped, so older clients keep working.
    let (status, _) = mark_with(
        &client,
        workspace.id,
        "se-explainer",
        json!({ "done": false, "skipped": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(journey_events(&app, workspace.id).await.len(), 1);

    // Done and skipped together is refused before anything is written.
    let (status, body) = mark_with(
        &client,
        workspace.id,
        "se-explainer",
        json!({ "done": true, "skipped": true }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(journey_events(&app, workspace.id).await.len(), 1);

    // Undoing the skip is a new event; marking done afterwards another.
    let (status, _) = mark(&client, workspace.id, "se-explainer", false).await;
    assert_eq!(status, StatusCode::OK);
    let marks = journey(&client, workspace.id).await;
    assert_eq!(marks[0]["skipped"], json!(false));
    assert_eq!(marks[0]["done"], json!(false));
    let (status, _) = mark(&client, workspace.id, "se-explainer", true).await;
    assert_eq!(status, StatusCode::OK);
    let marks = journey(&client, workspace.id).await;
    assert_eq!(marks[0]["done"], json!(true));
    assert_eq!(marks[0]["skipped"], json!(false));
    assert_eq!(journey_events(&app, workspace.id).await.len(), 3);

    // The timeline carries the flag on every entry.
    let response = client
        .get(&format!("/api/v1/workspaces/{}/activity", workspace.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    let entries = body["data"]["entries"].as_array().expect("entries");
    let skipped: Vec<&Value> = entries
        .iter()
        .filter(|entry| entry["kind"] == json!("journey_step") && entry["skipped"] == json!(true))
        .collect();
    assert_eq!(skipped.len(), 1, "{entries:?}");

    app.cleanup().await;
}
