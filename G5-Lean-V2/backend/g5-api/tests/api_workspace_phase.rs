//! Workbench phase (workbench `g5.workbench.phases.state`): explicit,
//! owner-moved workspace state through `PATCH /api/v1/workspaces/:id {phase}`,
//! reported on the detail and list DTOs, recorded durably in
//! `workspace_phase_events`, copied by a fork from an ordinary source and
//! reset to `be` for a fork of Base. Base itself never moves.
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints; assertions are at the HTTP and database
//! levels. No mocks.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn phase_of(app: &TestApp, workspace_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT phase FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .expect("Failed to load workspace phase")
}

async fn set_phase(app: &TestApp, workspace_id: Uuid, phase: &str) {
    sqlx::query("UPDATE workspaces SET phase = $2 WHERE id = $1")
        .bind(workspace_id)
        .bind(phase)
        .execute(&app.db)
        .await
        .expect("Failed to seed workspace phase");
}

/// `(from_phase, to_phase, actor_user_id)` rows, oldest first.
async fn phase_events(app: &TestApp, workspace_id: Uuid) -> Vec<(String, String, Option<Uuid>)> {
    sqlx::query_as::<_, (String, String, Option<Uuid>)>(
        "SELECT from_phase, to_phase, actor_user_id FROM workspace_phase_events \
         WHERE workspace_id = $1 ORDER BY created_at, id",
    )
    .bind(workspace_id)
    .fetch_all(&app.db)
    .await
    .expect("Failed to load phase events")
}

async fn patch_phase(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    phase: &str,
) -> (StatusCode, Value) {
    let response = client
        .patch_json(
            &format!("/api/v1/workspaces/{workspace_id}"),
            &json!({ "phase": phase }),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn owner_moves_the_phase_and_the_move_is_recorded_and_reported() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    // A new workspace starts in Business Engineering.
    let detail = client
        .get(&format!("/api/v1/workspaces/{}", workspace.id))
        .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let body: Value = detail.json().await.expect("json");
    assert_eq!(body["data"]["phase"], json!("be"));

    let (status, body) = patch_phase(&client, workspace.id, "se").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["phase"],
        json!("se"),
        "the update response reports the phase just written"
    );
    assert_eq!(phase_of(&app, workspace.id).await, "se");
    assert_eq!(
        phase_events(&app, workspace.id).await,
        vec![("be".to_string(), "se".to_string(), Some(owner.id))],
        "the move and its durable record commit together"
    );

    // Re-asserting the current phase is a no-op: no second event.
    let (status, _) = patch_phase(&client, workspace.id, "se").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(phase_events(&app, workspace.id).await.len(), 1);

    // Moving back is allowed and recorded.
    let (status, _) = patch_phase(&client, workspace.id, "be").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(phase_of(&app, workspace.id).await, "be");
    assert_eq!(phase_events(&app, workspace.id).await.len(), 2);

    // The list DTO carries the phase for every reader.
    let list = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let body: Value = list.json().await.expect("json");
    let item = body["data"]
        .as_array()
        .expect("list")
        .iter()
        .find(|item| item["id"] == json!(workspace.id.to_string()))
        .expect("workspace listed")
        .clone();
    assert_eq!(item["phase"], json!("be"));

    app.cleanup().await;
}

#[tokio::test]
async fn a_non_owner_member_cannot_move_the_phase() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&member);

    let (status, body) = patch_phase(&client, workspace.id, "se").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(phase_of(&app, workspace.id).await, "be");
    assert!(phase_events(&app, workspace.id).await.is_empty());

    app.cleanup().await;
}

#[tokio::test]
async fn an_unknown_phase_is_rejected() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    for phase in ["BE", "design", ""] {
        let (status, body) = patch_phase(&client, workspace.id, phase).await;
        // ValidationError is the route family's rejection for an invalid
        // body value (same as `visibility` and `push_mode`).
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "phase {phase:?}: {body}"
        );
    }
    assert_eq!(phase_of(&app, workspace.id).await, "be");
    assert!(phase_events(&app, workspace.id).await.is_empty());

    app.cleanup().await;
}

#[tokio::test]
async fn base_never_moves_its_phase() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let (status, body) = patch_phase(&client, base.id, "se").await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "Base rejects like the other Base content mutations: {body}"
    );
    assert_eq!(body["error"]["code"], json!("WORKSPACE_IS_BASE"), "{body}");
    assert_eq!(phase_of(&app, base.id).await, "be");
    assert!(phase_events(&app, base.id).await.is_empty());

    app.cleanup().await;
}

async fn fork(client: &helpers::AuthenticatedClient, source_id: Uuid, name: &str) -> Uuid {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{source_id}/clone"),
            &json!({ "name": name, "idempotencyKey": format!("phase-{name}") }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CREATED, "{body}");
    Uuid::parse_str(body["data"]["workspace"]["id"].as_str().expect("fork id"))
        .expect("fork id is a uuid")
}

#[tokio::test]
async fn a_fork_copies_an_ordinary_source_phase() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    set_phase(&app, source.id, "se").await;
    let client = app.authenticated_client(&owner);

    let target = fork(&client, source.id, "phase-fork-of-ordinary").await;
    assert_eq!(
        phase_of(&app, target).await,
        "se",
        "a fork continues where its ordinary source stands"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_fork_of_base_starts_in_business_engineering() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    // Base is always `be`; even a seeded value must not leak into a fork.
    set_phase(&app, base.id, "ie").await;
    let client = app.authenticated_client(&owner);

    let target = fork(&client, base.id, "phase-fork-of-base").await;
    assert_eq!(phase_of(&app, target).await, "be");

    app.cleanup().await;
}

/// `workspace_phase_events` is keyed to its workspace's organization through
/// the composite same-org foreign key (migration 059, like
/// `fk_wpre_pull_request_same_org`): a row claiming another organization is
/// refused by the database itself, whatever role writes it.
#[tokio::test]
async fn a_phase_event_for_another_organization_is_rejected() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let other_org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let mut tx = app.db.begin().await.expect("begin test transaction");
    let error = sqlx::query(
        "INSERT INTO workspace_phase_events \
         (org_id, workspace_id, from_phase, to_phase, actor_user_id) \
         VALUES ($1, $2, 'be', 'se', $3)",
    )
    .bind(other_org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&mut *tx)
    .await
    .expect_err("a phase event must not cross organizations");
    let database_error = error
        .as_database_error()
        .expect("foreign key violation is a database error");
    assert_eq!(
        database_error.code().as_deref(),
        Some("23503"),
        "expected foreign_key_violation, got {database_error}"
    );
    assert_eq!(
        database_error.constraint(),
        Some("fk_workspace_phase_events_workspace_same_org"),
        "{database_error}"
    );
    tx.rollback().await.expect("rollback test transaction");

    // The matching organization is accepted.
    sqlx::query(
        "INSERT INTO workspace_phase_events \
         (org_id, workspace_id, from_phase, to_phase, actor_user_id) \
         VALUES ($1, $2, 'be', 'se', $3)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("same-org phase event is accepted");
    assert_eq!(phase_events(&app, workspace.id).await.len(), 1);

    app.cleanup().await;
}

/// The inherited phase is resolved once, at admission, and frozen in the fork
/// manifest (g5.workbench.phases.state, g5.fork.replay): a first attempt that
/// admits the operation but cannot finalize (unreachable provider, as in
/// `api_workspaces::identical_fork_retry_with_a_new_key_resumes_the_pending_operation`),
/// a source that moves on meanwhile, and a retry that resumes from the manifest
/// must yield a fork in the phase admitted the first time.
#[tokio::test]
async fn a_resumed_fork_keeps_the_phase_admitted_before_the_source_moved() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    set_phase(&app, source.id, "se").await;

    // A provider identity whose remote does not exist yet: the repository
    // identity (git_url) stays constant across both attempts — a resume
    // refuses a changed repository — while the remote's availability toggles.
    let remote_dir = tempfile::tempdir().expect("temp remote dir");
    let remote_path = remote_dir.path().join("remote.git");
    sqlx::query("UPDATE repos SET provider = 'github', git_url = $2 WHERE id = $1")
        .bind(source.repo_id)
        .bind(format!("file://{}", remote_path.display()))
        .execute(&app.db)
        .await
        .expect("point provider verification at a not-yet-existing remote");
    // Reach the unavailable remote after credential resolution, so the first
    // attempt fails on retryable Git verification rather than credentials.
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .expect("test SCM token manager")
        .encrypt_token("fork-provider-test-token")
        .expect("encrypt test SCM token");
    sqlx::query(
        "INSERT INTO user_scm_connections
         (id, user_id, provider, provider_user_id, provider_username, access_token_enc, scopes)
         VALUES ($1, $2, 'github', 'fork-owner', 'fork-owner', $3, ARRAY['repo'])",
    )
    .bind(Uuid::new_v4())
    .bind(owner.id)
    .bind(encrypted)
    .execute(&app.db)
    .await
    .expect("seed provider verification credentials");

    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/clone", source.id);
    let body = json!({
        "name": "phase-fork-resumed",
        "idempotencyKey": "phase-fork-resumed-key",
        "source": "durable"
    });
    let first = client.post_json(&path, &body).await;
    assert_eq!(first.status(), StatusCode::SERVICE_UNAVAILABLE);
    let status: String = sqlx::query_scalar(
        "SELECT status FROM workspace_forks WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind("phase-fork-resumed")
    .fetch_one(&app.db)
    .await
    .expect("load the admitted fork operation");
    assert_eq!(
        status, "pending",
        "the first attempt admits but cannot finalize"
    );

    // The admitted manifest carries the phase of that moment.
    let manifest: Value = sqlx::query_scalar(
        "SELECT repo_manifest FROM workspace_forks WHERE org_id = $1 AND proposed_name = $2",
    )
    .bind(org.id)
    .bind("phase-fork-resumed")
    .fetch_one(&app.db)
    .await
    .expect("load the admitted fork manifest");
    assert_eq!(manifest[0]["target_phase"], json!("se"), "{manifest}");

    // The source moves on while the operation is pending; the remote now
    // exists (same identity) so the retry can verify and finalize.
    set_phase(&app, source.id, "ie").await;
    let init = std::process::Command::new("git")
        .args(["init", "--bare", "--quiet"])
        .arg(&remote_path)
        .status()
        .expect("run git init --bare");
    assert!(init.success(), "git init --bare failed: {init}");

    let second = client.post_json(&path, &body).await;
    let second_status = second.status();
    let second_body: Value = second.json().await.unwrap_or(Value::Null);
    assert_eq!(second_status, StatusCode::CREATED, "{second_body}");
    let target = Uuid::parse_str(
        second_body["data"]["workspace"]["id"]
            .as_str()
            .expect("fork id"),
    )
    .expect("fork id is a uuid");

    assert_eq!(
        phase_of(&app, target).await,
        "se",
        "the resumed fork keeps the phase admitted before the source moved"
    );
    assert_eq!(phase_of(&app, source.id).await, "ie");

    app.cleanup().await;
}
