//! Workspace rename (`g5.workspace.rename`, `g5.workspace.former-names`):
//! the owner changes the name without touching Git; former names stay
//! reserved by the workspace and resolve to it until its deletion completes.
//! Every fixture here has a name that differs from its branch after the first
//! rename, so a reader that still derives the branch from the name fails.

use crate::helpers::{fixtures::*, TestApp};
use axum::http::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn rename(
    client: &crate::helpers::AuthenticatedClient,
    workspace_id: Uuid,
    name: &str,
) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/rename"),
            &json!({ "name": name }),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn resolve(
    client: &crate::helpers::AuthenticatedClient,
    org: &str,
    workspace: &str,
) -> (StatusCode, Value) {
    let response = client
        .get(&format!("/api/v1/resolve?org={org}&workspace={workspace}"))
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn workspace_detail(
    client: &crate::helpers::AuthenticatedClient,
    workspace_id: Uuid,
) -> Value {
    let response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

async fn branch_of(app: &TestApp, workspace_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT branch_name FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_owner_renames_and_the_former_name_keeps_resolving() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_named(&app, &org, "foo").await;
    let client = app.authenticated_client(&owner);

    let (status, body) = rename(&client, workspace.id, "bar").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["name"], "bar");
    assert_eq!(body["data"]["formerNames"], json!(["foo"]));

    // Only the name changed: the branch the workspace owns is the one it was
    // created with, in the detail and in the row.
    let detail = workspace_detail(&client, workspace.id).await;
    assert_eq!(detail["name"], "bar");
    assert_eq!(detail["formerNames"], json!(["foo"]));
    assert_eq!(detail["branches"][0]["branchName"], "foo");
    assert_eq!(branch_of(&app, workspace.id).await.as_deref(), Some("foo"));

    // The former name resolves to the workspace and reports the current name;
    // the resolver takes the organization id as well as its slug.
    let (status, body) = resolve(&client, &org.slug, "foo").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspaceId"], workspace.id.to_string());
    assert_eq!(body["data"]["workspaceName"], "bar");
    let (status, body) = resolve(&client, &org.id.to_string(), "FOO").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspaceId"], workspace.id.to_string());
    let (status, body) = resolve(&client, &org.slug, "bar").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspaceName"], "bar");

    // The rename is on record, and the workspace list shows the new name.
    let log: Vec<(String, String)> = sqlx::query_as(
        "SELECT previous_name, name FROM workspace_renames WHERE workspace_id = $1 ORDER BY seq",
    )
    .bind(workspace.id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(log, vec![("foo".to_string(), "bar".to_string())]);
    let list = client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let names: Vec<String> = list.json::<Value>().await.unwrap()["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"bar".to_string()) && !names.contains(&"foo".to_string()));

    app.cleanup().await;
}

#[tokio::test]
async fn renaming_through_former_names_keeps_every_name_and_the_order() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_named(&app, &org, "foo").await;
    let client = app.authenticated_client(&owner);

    let mut seen = vec!["foo"];
    for name in ["bar", "abc", "bar", "foo"] {
        let (status, body) = rename(&client, workspace.id, name).await;
        assert_eq!(status, StatusCode::OK, "{name}: {body}");
        assert_eq!(body["data"]["name"], name);
        if !seen.contains(&name) {
            seen.push(name);
        }
        // Every name the workspace has held resolves to it.
        for former in seen.iter().copied() {
            let (status, body) = resolve(&client, &org.slug, former).await;
            assert_eq!(status, StatusCode::OK, "{former} after {name}");
            assert_eq!(body["data"]["workspaceId"], workspace.id.to_string());
            assert_eq!(body["data"]["workspaceName"], name);
        }
    }
    let (_, body) = rename(&client, workspace.id, "foo").await;
    assert_eq!(
        body["data"]["name"], "foo",
        "renaming to the current name is a no-op"
    );
    assert_eq!(body["data"]["formerNames"], json!(["bar", "abc"]));
    assert_eq!(branch_of(&app, workspace.id).await.as_deref(), Some("foo"));
    let log: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM workspace_renames WHERE workspace_id = $1 ORDER BY seq",
    )
    .bind(workspace.id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(log, vec!["bar", "abc", "bar", "foo"]);

    // A case-only change creates no former name; resolution ignores case.
    let (status, body) = rename(&client, workspace.id, "Foo").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["name"], "Foo");
    assert_eq!(body["data"]["formerNames"], json!(["bar", "abc"]));
    let (_, body) = resolve(&client, &org.slug, "foo").await;
    assert_eq!(body["data"]["workspaceName"], "Foo");

    app.cleanup().await;
}

#[tokio::test]
async fn rename_is_refused_for_non_owners_base_and_invalid_names() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_named(&app, &org, "foo").await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;

    let (status, _) = rename(&app.authenticated_client(&member), workspace.id, "bar").await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a member who is not the owner"
    );

    let client = app.authenticated_client(&owner);
    let (status, body) = rename(&client, base.id, "root").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "Base: {body}");
    for invalid in ["bad name", "settings", "fork", "a..b", "", "@"] {
        let (status, body) = rename(&client, workspace.id, invalid).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{invalid:?}: {body}"
        );
    }
    assert_eq!(workspace_detail(&client, workspace.id).await["name"], "foo");

    // A pending ownership transfer refuses the rename like other owner writes.
    let other = TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    sqlx::query(
        "INSERT INTO workspace_ownership_transfers
             (org_id, workspace_id, from_user_id, to_user_id, requested_by,
              idempotency_key, request_digest)
         VALUES ($1, $2, $3, $4, $3, 'transfer-1', repeat('b', 64))",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(other.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = rename(&client, workspace.id, "bar").await;
    assert_eq!(status, StatusCode::CONFLICT, "pending transfer: {body}");
    assert_eq!(workspace_detail(&client, workspace.id).await["name"], "foo");

    app.cleanup().await;
}

#[tokio::test]
async fn a_name_held_or_reserved_by_another_workspace_is_refused() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create_named(&app, &org, "foo").await;
    let other = TestWorkspace::create_named(&app, &org, "taken").await;
    let client = app.authenticated_client(&owner);

    // Another live workspace's current name, in any letter case.
    let (status, body) = rename(&client, workspace.id, "TAKEN").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("another workspace"));

    // Another workspace's former name stays reserved while it exists.
    let (status, _) = rename(&client, other.id, "moved").await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = rename(&client, workspace.id, "taken").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("former name"));

    // A pending fork's proposed name.
    sqlx::query(
        "INSERT INTO workspace_forks (org_id, source_workspace_id, requested_by, owner_user_id,
                                      idempotency_key, request_digest, proposed_name, repo_manifest)
         VALUES ($1, $2, $3, $3, 'claim-1', repeat('a', 64), 'claimed', $4)",
    )
    .bind(org.id)
    .bind(other.id)
    .bind(owner.id)
    .bind(json!([{ "repo_id": other.repo_id }]))
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = rename(&client, workspace.id, "Claimed").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("being forked"));

    assert_eq!(workspace_detail(&client, workspace.id).await["name"], "foo");
    app.cleanup().await;
}

#[tokio::test]
async fn a_fork_cannot_take_another_workspaces_former_name() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let forked = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", base.id),
            &json!({"name": "foo", "idempotencyKey": "fork-foo"}),
        )
        .await;
    assert_eq!(forked.status(), StatusCode::CREATED);
    let foo_id: Uuid = forked.json::<Value>().await.unwrap()["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (status, body) = rename(&client, foo_id, "bar").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(branch_of(&app, foo_id).await.as_deref(), Some("foo"));

    // The name `foo` is reserved by `bar`, and its branch is still owned.
    let refused = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", base.id),
            &json!({"name": "foo", "idempotencyKey": "fork-foo-again"}),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let refused = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", base.id),
            &json!({"name": "Foo", "idempotencyKey": "fork-foo-case"}),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // A fork of the renamed workspace captures its branch, not its name.
    let child = client
        .post_json(
            &format!("/api/v1/workspaces/{foo_id}/clone"),
            &json!({"name": "bar-child", "idempotencyKey": "fork-child"}),
        )
        .await;
    assert_eq!(child.status(), StatusCode::CREATED);
    let child_id: Uuid = child.json::<Value>().await.unwrap()["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let child_detail = workspace_detail(&client, child_id).await;
    assert_eq!(child_detail["branches"][0]["branchName"], "bar-child");
    assert_eq!(child_detail["clonedFromId"], foo_id.to_string());

    app.cleanup().await;
}

#[tokio::test]
async fn completed_deletion_releases_the_former_names() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let forked = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", base.id),
            &json!({"name": "foo", "idempotencyKey": "fork-foo"}),
        )
        .await;
    assert_eq!(forked.status(), StatusCode::CREATED);
    let foo_id: Uuid = forked.json::<Value>().await.unwrap()["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (status, _) = rename(&client, foo_id, "bar").await;
    assert_eq!(status, StatusCode::OK);
    let keeper = TestWorkspace::create_named(&app, &org, "keeper").await;
    let (status, _) = rename(&client, keeper.id, "foo").await;
    assert_eq!(status, StatusCode::CONFLICT, "reserved while bar exists");

    let deleted = client
        .post_json(&format!("/api/v1/workspaces/{foo_id}/delete"), &json!({}))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let reserved: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_former_names WHERE workspace_id = $1")
            .bind(foo_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reserved, 0, "completed deletion retires the former names");
    let (status, _) = resolve(&client, &org.slug, "foo").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = rename(&client, keeper.id, "foo").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    app.cleanup().await;
}
