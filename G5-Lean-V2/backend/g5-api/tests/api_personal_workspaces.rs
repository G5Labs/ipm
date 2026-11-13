//! Personal workspaces (`g5.personal.*`, G5-Lean#624): creation through
//! `/orgs/:orgId/me` and membership changes, the registry-kind refusals, the
//! reserved names, read access and retirement on removal or demotion.
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints; assertions are at the HTTP, Git and
//! database levels.

use crate::helpers;

use g5_auth::Scope;
use helpers::{AuthenticatedClient, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn body_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn me(client: &AuthenticatedClient, org_id: Uuid) -> (StatusCode, Value) {
    body_of(client.get(&format!("/api/v1/orgs/{org_id}/me")).await).await
}

async fn ensure_mine(client: &AuthenticatedClient, org_id: Uuid) -> (Uuid, String) {
    let (status, body) = me(client, org_id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id =
        Uuid::parse_str(body["data"]["workspaceId"].as_str().expect("workspace id")).expect("uuid");
    (
        id,
        body["data"]["handle"].as_str().expect("handle").to_string(),
    )
}

async fn set_role(app: &TestApp, org_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("UPDATE org_memberships SET role = $3 WHERE org_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .execute(&app.db)
        .await
        .expect("set role");
}

async fn set_email(app: &TestApp, user_id: Uuid, email: &str) {
    sqlx::query("UPDATE users SET email = $2 WHERE id = $1")
        .bind(user_id)
        .bind(email)
        .execute(&app.db)
        .await
        .expect("set email");
}

async fn live_personal_count(app: &TestApp, org_id: Uuid, user_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM workspaces
         WHERE org_id = $1 AND personal_user_id = $2 AND deleted_at IS NULL",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("count personal workspaces")
}

fn member_scopes() -> Vec<Scope> {
    vec![
        Scope::WorkspacesRead,
        Scope::WorkspacesWrite,
        Scope::SessionsRead,
        Scope::SessionsWrite,
    ]
}

#[tokio::test]
async fn me_creates_one_private_personal_workspace_with_its_own_repository() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    set_email(&app, owner.id, "Alice.Smith@example.com").await;
    let client = app.authenticated_client(&owner);

    let (workspace_id, handle) = ensure_mine(&client, org.id).await;
    assert_eq!(handle, "alice-smith");
    // Idempotent: a second call returns the same workspace.
    let (again, _) = ensure_mine(&client, org.id).await;
    assert_eq!(again, workspace_id);
    assert_eq!(live_personal_count(&app, org.id, owner.id).await, 1);

    let row = sqlx::query_as::<_, (String, Option<Uuid>, Option<Uuid>, bool, Option<Uuid>)>(
        "SELECT name, owner_user_id, personal_user_id, is_base, docs_repo_id
         FROM workspaces WHERE id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("personal row");
    assert_eq!(row.0, "users/alice-smith");
    assert_eq!(row.1, Some(owner.id));
    assert_eq!(row.2, Some(owner.id));
    assert!(!row.3);
    assert_eq!(row.4, None, "no Project Docs folder in the docs repository");

    let members = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String)>(
        "SELECT member.repo_id, member.root_path, member.role, repo.provider, repo.git_url
         FROM workspace_repos AS member JOIN repos AS repo ON repo.id = member.repo_id
         WHERE member.workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(&app.db)
    .await
    .expect("members");
    assert_eq!(members.len(), 1, "exactly one private member");
    let (repo_id, root_path, role, provider, git_url) = members[0].clone();
    assert_eq!(root_path, "personal");
    assert_eq!(role, "code");
    assert_eq!(provider, None, "providerless");
    // `g5.repo.hosted-url`: the organization's and repository's slugs,
    // never a disk path.
    let org_slug: String = sqlx::query_scalar("SELECT slug FROM orgs WHERE id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .expect("org slug");
    assert_eq!(
        git_url,
        format!("g5-local://{org_slug}/personal-{workspace_id}")
    );
    let shared =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM workspace_repos WHERE repo_id = $1")
            .bind(repo_id)
            .fetch_one(&app.db)
            .await
            .expect("repo sharing");
    assert_eq!(
        shared, 1,
        "no other workspace includes the private repository"
    );

    let (idle, agent_opt_in) = sqlx::query_as::<_, (i32, bool)>(
        "SELECT idle_timeout_secs, agent_may_modify_other_workspaces
         FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("settings");
    assert_eq!(
        idle, 1800,
        "the personal idle default, not the ordinary one"
    );
    assert!(
        agent_opt_in,
        "a personal workspace's agent may modify other workspaces by default"
    );

    // The branch is initialized in the private repository.
    let path = app.repo_path(repo_id).await;
    let repo = g5_scm::GitRepo::open(&path).expect("open repo");
    assert!(
        repo.read_branch_oid("users/alice-smith")
            .expect("read branch")
            .is_some(),
        "personal branch has its first commit"
    );

    // Detail and list report the kind.
    let (status, detail) = body_of(
        client
            .get(&format!("/api/v1/workspaces/{workspace_id}"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["data"]["kind"], json!("personal"));
    let (_, list) = body_of(
        client
            .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
            .await,
    )
    .await;
    let item = list["data"]
        .as_array()
        .expect("list")
        .iter()
        .find(|item| item["id"] == json!(workspace_id.to_string()))
        .expect("listed for its owner")
        .clone();
    assert_eq!(item["kind"], json!("personal"));

    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_creations_make_one_workspace_and_colliding_handles_get_suffixes() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    set_email(&app, owner.id, "sam@one.example").await;
    let other = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    set_email(&app, other.id, "Sam@two.example").await;

    let owner_client = app.authenticated_client(&owner);
    let (first, second) = tokio::join!(me(&owner_client, org.id), me(&owner_client, org.id));
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second.0, StatusCode::OK, "{}", second.1);
    assert_eq!(
        first.1["data"]["workspaceId"],
        second.1["data"]["workspaceId"]
    );
    assert_eq!(live_personal_count(&app, org.id, owner.id).await, 1);
    assert_eq!(first.1["data"]["handle"], json!("sam"));

    let (_, other_handle) = ensure_mine(&app.authenticated_client(&other), org.id).await;
    assert_eq!(other_handle, "sam-2");

    app.cleanup().await;
}

#[tokio::test]
async fn viewers_have_no_personal_workspace() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let viewer = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    set_role(&app, org.id, viewer.id, "viewer").await;

    let (status, body) = me(&app.authenticated_client(&viewer), org.id).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(live_personal_count(&app, org.id, viewer.id).await, 0);

    app.cleanup().await;
}

#[tokio::test]
async fn personal_workspaces_refuse_fork_transfer_delete_phases_rename() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let other = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    let client = app.authenticated_client(&owner);
    let (workspace_id, _) = ensure_mine(&client, org.id).await;

    let refusals = [
        client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/clone"),
                &json!({ "name": "copy-of-personal", "idempotencyKey": "personal-fork" }),
            )
            .await,
        client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/transfer"),
                &json!({ "toUserId": other.id, "idempotencyKey": "personal-transfer" }),
            )
            .await,
        client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/delete"),
                &json!({}),
            )
            .await,
        client
            .patch_json(
                &format!("/api/v1/workspaces/{workspace_id}"),
                &json!({ "phase": "se" }),
            )
            .await,
        client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/rename"),
                &json!({ "name": "renamed-personal" }),
            )
            .await,
    ];
    for (index, response) in refusals.into_iter().enumerate() {
        let (status, body) = body_of(response).await;
        assert_eq!(status, StatusCode::CONFLICT, "refusal {index}: {body}");
        assert_eq!(
            body["error"]["code"],
            json!("WORKSPACE_IS_PERSONAL"),
            "refusal {index}: {body}"
        );
    }
    assert_eq!(live_personal_count(&app, org.id, owner.id).await, 1);

    // An ordinary workspace whose name looks personal stays a working
    // workspace: kind comes from the registry, never the name. Name admission
    // refuses `users/` names, so the legacy name is written directly.
    let lookalike = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET name = 'users/lookalike' WHERE id = $1")
        .bind(lookalike.id)
        .execute(&app.db)
        .await
        .expect("legacy name");
    let (status, body) = body_of(
        client
            .patch_json(
                &format!("/api/v1/workspaces/{}", lookalike.id),
                &json!({ "phase": "se" }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, detail) = body_of(
        client
            .get(&format!("/api/v1/workspaces/{}", lookalike.id))
            .await,
    )
    .await;
    assert_eq!(detail["data"]["kind"], json!("working"));

    app.cleanup().await;
}

#[tokio::test]
async fn ordinary_names_cannot_take_me_users_or_names_under_users() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    for (index, name) in ["me", "users", "users/alice", "Users/Alice", "USERS"]
        .iter()
        .enumerate()
    {
        let (status, body) = body_of(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{}/clone", source.id),
                    &json!({ "name": name, "idempotencyKey": format!("reserved-{index}") }),
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {body}");
        let (status, body) = body_of(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{}/rename", source.id),
                    &json!({ "name": name }),
                )
                .await,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "rename to {name}: {body}"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn only_the_owner_and_admins_read_a_personal_workspace() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    set_email(&app, member.id, "mona@example.com").await;
    let other = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    let member_client = app.authenticated_client(&member);
    let (workspace_id, handle) = ensure_mine(&member_client, org.id).await;
    assert_eq!(handle, "mona");
    let repo_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT repo_id FROM workspace_repos WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("repo");

    let owner_client = app.authenticated_client(&owner);
    let other_client = app.authenticated_client(&other);
    for (client, allowed) in [
        (&member_client, true),
        (&owner_client, true),
        (&other_client, false),
    ] {
        let (status, body) = body_of(
            client
                .get(&format!("/api/v1/orgs/{}/users/mona/workspace", org.id))
                .await,
        )
        .await;
        if allowed {
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["data"]["workspaceId"], json!(workspace_id.to_string()));
        } else {
            assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        }
        let status = client
            .get(&format!("/api/v1/workspaces/{workspace_id}"))
            .await
            .status();
        assert_eq!(status.is_success(), allowed, "detail read: {status}");
    }

    for (user, allowed) in [(&member, true), (&owner, true), (&other, false)] {
        let response = app
            .client
            .get(format!(
                "{}/git/{}/info/refs?service=git-upload-pack",
                app.base_url, repo_id
            ))
            .bearer_auth(&user.jwt)
            .send()
            .await
            .expect("advertisement");
        let status = response.status();
        assert_eq!(status.is_success(), allowed, "git read: {status}");
    }

    // Other members do not see it in the workspace list.
    let (_, list) = body_of(
        other_client
            .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
            .await,
    )
    .await;
    assert!(
        !list["data"]
            .as_array()
            .expect("list")
            .iter()
            .any(|item| item["id"] == json!(workspace_id.to_string())),
        "{list}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn removing_or_demoting_a_member_retires_their_personal_workspace() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    let removed = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    let (removed_ws, _) = ensure_mine(&app.authenticated_client(&removed), org.id).await;
    let status = owner_client
        .delete(&format!("/api/v1/orgs/{}/members/{}", org.id, removed.id))
        .await
        .status();
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "removal is never blocked by the personal workspace"
    );
    let deleted: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM workspaces WHERE id = $1")
            .bind(removed_ws)
            .fetch_one(&app.db)
            .await
            .expect("tombstone");
    assert!(
        deleted.is_some(),
        "removal tombstones the personal workspace"
    );

    let demoted = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    let (demoted_ws, _) = ensure_mine(&app.authenticated_client(&demoted), org.id).await;
    let (status, body) = body_of(
        owner_client
            .patch_json(
                &format!("/api/v1/orgs/{}/members/{}", org.id, demoted.id),
                &json!({ "role": "viewer" }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(live_personal_count(&app, org.id, demoted.id).await, 0);
    let generation_bumped: bool = sqlx::query_scalar(
        "SELECT deleted_at IS NOT NULL AND lifecycle_generation > 0 FROM workspaces WHERE id = $1",
    )
    .bind(demoted_ws)
    .fetch_one(&app.db)
    .await
    .expect("retired");
    assert!(generation_bumped, "retirement withdraws Start consent");

    // Promotion back creates a fresh personal workspace.
    let (status, body) = body_of(
        owner_client
            .patch_json(
                &format!("/api/v1/orgs/{}/members/{}", org.id, demoted.id),
                &json!({ "role": "member" }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(live_personal_count(&app, org.id, demoted.id).await, 1);
    let fresh = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM workspaces WHERE org_id = $1 AND personal_user_id = $2 AND deleted_at IS NULL",
    )
    .bind(org.id)
    .bind(demoted.id)
    .fetch_one(&app.db)
    .await
    .expect("fresh");
    assert_ne!(
        fresh, demoted_ws,
        "a rejoining member gets a fresh workspace"
    );

    // An owned working workspace still blocks removal.
    let blocked = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    let working = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(working.id)
        .bind(blocked.id)
        .execute(&app.db)
        .await
        .expect("owner");
    ensure_mine(&app.authenticated_client(&blocked), org.id).await;
    let status = owner_client
        .delete(&format!("/api/v1/orgs/{}/members/{}", org.id, blocked.id))
        .await
        .status();
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        live_personal_count(&app, org.id, blocked.id).await,
        1,
        "a refused removal leaves the personal workspace live"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn the_reconciler_backfills_eligible_members() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    let viewer = TestUser::create_with_scopes(&app, org.id, &member_scopes()).await;
    set_role(&app, org.id, viewer.id, "viewer").await;

    let created = g5_api::services::personal_workspace::reconcile_org(&app.state, org.id, 8)
        .await
        .expect("reconcile");
    assert_eq!(created, 2);
    assert_eq!(live_personal_count(&app, org.id, owner.id).await, 1);
    assert_eq!(live_personal_count(&app, org.id, member.id).await, 1);
    assert_eq!(live_personal_count(&app, org.id, viewer.id).await, 0);
    // Idempotent.
    let created = g5_api::services::personal_workspace::reconcile_org(&app.state, org.id, 8)
        .await
        .expect("reconcile again");
    assert_eq!(created, 0);

    app.cleanup().await;
}

#[tokio::test]
async fn admins_set_the_personal_idle_default_for_new_personal_workspaces() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    let (status, body) = body_of(
        client
            .get(&format!("/api/v1/orgs/{}/admin-settings", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["defaultPersonalIdleTimeoutSecs"], json!(1800));
    let (status, body) = body_of(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({ "defaultPersonalIdleTimeoutSecs": 10 }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = body_of(
        client
            .put_json(
                &format!("/api/v1/orgs/{}/admin-settings", org.id),
                &json!({ "defaultPersonalIdleTimeoutSecs": 900 }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["defaultPersonalIdleTimeoutSecs"], json!(900));

    let (workspace_id, _) = ensure_mine(&client, org.id).await;
    let idle = sqlx::query_scalar::<_, i32>(
        "SELECT idle_timeout_secs FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("settings");
    assert_eq!(idle, 900);

    app.cleanup().await;
}

/// The workbench reads a personal workspace through the ordinary endpoints;
/// a workspace with no docs repository must never make them fail as a
/// server error, and its files read from committed Git without a VM.
#[tokio::test]
async fn workbench_reads_of_a_personal_workspace_do_not_fail() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let (workspace_id, _) = ensure_mine(&client, org.id).await;
    let repo_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT repo_id FROM workspace_repos WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("repo");

    for path in [
        format!("/api/v1/workspaces/{workspace_id}"),
        format!("/api/v1/workspaces/{workspace_id}/vm"),
        format!("/api/v1/workspaces/{workspace_id}/branches"),
        format!("/api/v1/workspaces/{workspace_id}/changes"),
        format!("/api/v1/workspaces/{workspace_id}/stats"),
        format!("/api/v1/workspaces/{workspace_id}/members"),
        format!("/api/v1/workspaces/{workspace_id}/journey"),
        format!("/api/v1/workspaces/{workspace_id}/pull-requests"),
        format!("/api/v1/workspaces/{workspace_id}/sessions"),
        format!("/api/v1/workspaces/{workspace_id}/tasks"),
        format!("/api/v1/workspaces/{workspace_id}/docs"),
        format!("/api/v1/workspaces/{workspace_id}/questions"),
        format!("/api/v1/workspaces/{workspace_id}/workbench"),
        format!("/api/v1/workspaces/{workspace_id}/repos/{repo_id}/files"),
        format!("/api/v1/workspaces/{workspace_id}/repos/{repo_id}/files/README.md"),
        format!("/api/v1/orgs/{}/workspaces", org.id),
        format!("/api/v1/orgs/{}/home/attention", org.id),
    ] {
        let response = client.get(&path).await;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        assert!(
            !status.is_server_error(),
            "{path} returned {status}: {body}"
        );
        if path.ends_with("/files") || path.ends_with("README.md") {
            assert_eq!(status, StatusCode::OK, "{path}: {body}");
        }
    }

    app.cleanup().await;
}

/// Creating an organization and joining one both create the new eligible
/// owner's personal workspace (g5.personal.lifecycle); a viewer invitation
/// does not.
#[tokio::test]
async fn organization_creation_and_joining_create_personal_workspaces() {
    let app = TestApp::spawn().await;
    let creator = TestUser::create(&app).await;
    let client = app.authenticated_client(&creator);
    let created = helpers::platform::create_org_via_operator(
        &app,
        &creator,
        json!({ "name": "Personal Lifecycle" }),
    )
    .await;
    let status = created.status();
    let body: Value = created.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let org_id = Uuid::parse_str(body["data"]["org"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(live_personal_count(&app, org_id, creator.id).await, 1);

    // Membership in the path organization authorizes the invitation.
    let owner_client = client;
    let member = TestUser::create(&app).await;
    let viewer = TestUser::create(&app).await;
    for (user, role) in [(&member, "member"), (&viewer, "viewer")] {
        let (status, body) = body_of(
            owner_client
                .post_json(
                    &format!("/api/v1/orgs/{org_id}/members"),
                    &json!({ "email": user.email, "role": role }),
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    assert_eq!(live_personal_count(&app, org_id, member.id).await, 1);
    assert_eq!(live_personal_count(&app, org_id, viewer.id).await, 0);

    app.cleanup().await;
}
