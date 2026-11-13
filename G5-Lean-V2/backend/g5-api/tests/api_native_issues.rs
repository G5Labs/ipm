//! G5's native issue tracker (`g5.issue.native`): an organization with no
//! external integration, on a deployment without the GitHub App, creates,
//! searches, links, comments on, closes and reopens issues through the same
//! routes as GitHub issues, and its agent through `g5d issue`.
//!
//! The real router, auth middleware and Postgres (RLS under `g5_tenant`) are
//! exercised; no tracker is mocked because none is called.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

// ─── Fixtures and request helpers ────────────────────────────────────────────

async fn json_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

async fn get(app: &TestApp, user: &TestUser, path: &str) -> (StatusCode, Value) {
    json_of(app.authenticated_client(user).get(path).await).await
}

async fn post(app: &TestApp, user: &TestUser, path: &str, body: &Value) -> (StatusCode, Value) {
    json_of(app.authenticated_client(user).post_json(path, body).await).await
}

async fn patch(app: &TestApp, user: &TestUser, path: &str, body: &Value) -> (StatusCode, Value) {
    json_of(app.authenticated_client(user).patch_json(path, body).await).await
}

fn assert_conflict_code(status: StatusCode, body: &Value, code: &str) {
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["details"]["code"], code, "{body}");
}

async fn enable_issues(app: &TestApp, org_id: Uuid) {
    sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json, secret_keys) \
         VALUES ($1, '{\"external_issues_enabled\": true}'::jsonb, '{}') \
         ON CONFLICT (org_id) DO UPDATE \
         SET settings_json = org_configs.settings_json || EXCLUDED.settings_json",
    )
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn insert_workspace(
    app: &TestApp,
    org_id: Uuid,
    owner: Uuid,
    name: &str,
    base: bool,
) -> Uuid {
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by) \
         VALUES ($1, $2, $3, $5, $4, $4)",
    )
    .bind(workspace_id)
    .bind(org_id)
    .bind(name)
    .bind(owner)
    .bind(base)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_settings (org_id, workspace_id) VALUES ($1, $2)")
        .bind(org_id)
        .bind(workspace_id)
        .execute(&app.db)
        .await
        .unwrap();
    workspace_id
}

/// Issues are on for an organization that never touched the switch
/// (`g5.issue.native.domain`); only an explicit `false` turns them off.
#[tokio::test]
async fn issues_are_on_until_the_switch_is_set_off() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    // An issue read, which the switch governs (domains stay listed while
    // off, for cleanup).
    let domains_path = format!("/api/v1/orgs/{}/issues/search?q=anything", org.id);

    // No organization configuration at all: on.
    let (status, body) = get(&app, &owner, &domains_path).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A configuration without the key: still on.
    sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json, secret_keys) VALUES ($1, '{}'::jsonb, '{}') \
         ON CONFLICT (org_id) DO UPDATE \
         SET settings_json = org_configs.settings_json - 'external_issues_enabled'",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = get(&app, &owner, &domains_path).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Switched off: refused.
    sqlx::query(
        "UPDATE org_configs SET settings_json = settings_json || '{\"external_issues_enabled\": false}' WHERE org_id = $1",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = get(&app, &owner, &domains_path).await;
    assert_conflict_code(status, &body, "external_issues_disabled");

    // Switched on again explicitly.
    enable_issues(&app, org.id).await;
    let (status, body) = get(&app, &owner, &domains_path).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// An organization with issues on and G5 issues keyed `key`.
async fn native_org(app: &TestApp, key: &str) -> (TestUser, TestOrg, Value) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    enable_issues(app, org.id).await;
    let (status, domain) = post(
        app,
        &owner,
        &format!("/api/v1/orgs/{}/issue-domains", org.id),
        &json!({"provider": "g5", "key": key}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{domain}");
    (owner, org, domain["data"].clone())
}

async fn create_issue(
    app: &TestApp,
    user: &TestUser,
    org_id: Uuid,
    domain_id: &Value,
    request: Value,
) -> (StatusCode, Value) {
    let mut request = request;
    request["domainId"] = domain_id.clone();
    post(
        app,
        user,
        &format!("/api/v1/orgs/{org_id}/issues"),
        &request,
    )
    .await
}

async fn agent_token(app: &TestApp, org_id: Uuid, workspace_id: Uuid, owner: Uuid) -> String {
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status, last_heartbeat, slot_size_reported) \
         VALUES ($1, $2, 'us-central1', 'ready', now(), true)",
    )
    .bind(host_id)
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms \
         (org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at) \
         VALUES ($1, $2, $3, $4, 1, 'running', now() + interval '1 hour')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .unwrap();
    app.state
        .jwt_minter
        .as_ref()
        .expect("test jwt minter")
        .mint_workspace_incarnation(org_id, workspace_id, owner, vm_id, 1, 3600)
        .unwrap()
}

async fn agent(
    app: &TestApp,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let request = reqwest::Client::new()
        .request(method, format!("{}{path}", app.base_url))
        .bearer_auth(token);
    let request = match body {
        Some(body) => request.json(body),
        None => request,
    };
    json_of(request.send().await.unwrap()).await
}

fn keys_of(results: &Value) -> Vec<String> {
    results
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["displayKey"].as_str().unwrap().to_string())
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_organization_without_integrations_creates_searches_and_reads_g5_issues() {
    let app = TestApp::spawn().await;
    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let features = info["data"]["features"].as_array().unwrap();
    assert!(features.contains(&json!("issues")), "{info}");
    assert!(!features.contains(&json!("external_issues")), "{info}");

    let (owner, org, domain) = native_org(&app, "acme").await;
    assert_eq!(domain["provider"], "g5", "{domain}");
    assert_eq!(domain["containerKind"], "g5_project");
    assert_eq!(domain["displayKey"], "ACME");
    assert_eq!(domain["available"], true);
    assert_eq!(domain["isDefaultCreateTarget"], true);
    assert_eq!(domain["contentTrusted"], true);
    let domains_path = format!("/api/v1/orgs/{}/issue-domains", org.id);
    let (status, body) = post(
        &app,
        &owner,
        &domains_path,
        &json!({"provider": "g5", "key": "OTHER"}),
    )
    .await;
    assert_conflict_code(status, &body, "domain_exists");
    let (status, body) = post(
        &app,
        &owner,
        &domains_path,
        &json!({"provider": "g5", "key": "a-b"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // A GitHub repository needs the App this deployment does not have.
    let (status, body) = post(
        &app,
        &owner,
        &domains_path,
        &json!({"githubRepoId": "1001"}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    // Native domains have no publication switch.
    let domain_path = format!("{domains_path}/{}", domain["id"].as_str().unwrap());
    let (status, body) = patch(
        &app,
        &owner,
        &domain_path,
        &json!({"commentsEnabled": true}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let issues = format!("/api/v1/orgs/{}/issues", org.id);
    let (status, target) = get(&app, &owner, &format!("{issues}/create-target")).await;
    assert_eq!(status, StatusCode::OK, "{target}");
    assert_eq!(target["data"]["state"], "ready", "{target}");
    assert_eq!(target["data"]["destination"]["provider"], "g5");
    assert!(target["data"]["githubLogin"].is_null());
    assert_eq!(
        target["data"]["assignableUsers"][0]["login"],
        owner.email.as_str()
    );

    // Creation is keyed: a replay returns the first issue, a changed one is refused.
    let request = json!({
        "title": "Crash on save", "body": "Steps to reproduce", "assignees": ["@me"],
        "idempotencyKey": "create-1",
    });
    let (status, created) =
        create_issue(&app, &owner, org.id, &domain["id"], request.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let created = &created["data"];
    assert_eq!(created["state"], "created");
    assert_eq!(created["sent"], true);
    assert!(created["githubLogin"].is_null());
    assert_eq!(created["issue"]["displayKey"], "ACME-1");
    assert_eq!(created["issue"]["repositoryId"], "g5");
    assert_eq!(created["issue"]["number"], 1);
    assert_eq!(created["assignees"], json!([owner.email]));
    assert!(created["issue"]["url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/{}/issues/g5/1", org.slug)));
    let (status, replay) = create_issue(&app, &owner, org.id, &domain["id"], request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["sent"], false);
    assert_eq!(replay["data"]["issue"]["number"], 1);
    let mut changed = request.clone();
    changed["title"] = json!("Another title");
    let (status, body) = create_issue(&app, &owner, org.id, &domain["id"], changed).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // A status check only reads.
    let (status, check) = create_issue(
        &app,
        &owner,
        org.id,
        &domain["id"],
        json!({"title": "x", "idempotencyKey": "never-sent", "checkOnly": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{check}");
    assert_eq!(check["data"]["state"], "not_found");

    // A sub-issue: the parent is set in the creating transaction.
    let (status, child) = create_issue(
        &app,
        &owner,
        org.id,
        &domain["id"],
        json!({"title": "Flush the buffer", "parent": "1", "idempotencyKey": "create-2"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{child}");
    assert_eq!(child["data"]["issue"]["displayKey"], "ACME-2");
    assert_eq!(child["data"]["parent"]["displayKey"], "ACME-1");
    assert_eq!(child["data"]["parent"]["state"], "attached");
    for (parent, code) in [
        ("ACME-99", "parent_not_found"),
        ("acme/web#1", "parent_outside_destination"),
    ] {
        let (status, body) = create_issue(
            &app,
            &owner,
            org.id,
            &domain["id"],
            json!({"title": "Orphan", "parent": parent, "idempotencyKey": format!("orphan-{code}")}),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error"]["details"]["code"], code, "{body}");
    }
    // An assignee who is not a member is refused, never dropped.
    let (status, body) = create_issue(
        &app,
        &owner,
        org.id,
        &domain["id"],
        json!({"title": "Who", "assignees": ["stranger@example.com"], "idempotencyKey": "who"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Search: text, key, state and assignee, needing no GitHub connection.
    let search = |query: &str| format!("{issues}/search?{query}");
    let (status, body) = get(&app, &owner, &search("q=")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(keys_of(&body["data"]["results"]), vec!["ACME-2", "ACME-1"]);
    assert_eq!(body["data"]["domainErrors"], json!([]));
    assert_eq!(body["data"]["results"][1]["provider"], "g5");
    assert_eq!(
        body["data"]["results"][1]["author"]["userId"],
        owner.id.to_string()
    );
    let (_, body) = get(&app, &owner, &search("q=crashes")).await;
    assert_eq!(
        keys_of(&body["data"]["results"]),
        vec!["ACME-1"],
        "stemmed text"
    );
    let (_, body) = get(&app, &owner, &search("q=ACME-2")).await;
    assert_eq!(keys_of(&body["data"]["results"]), vec!["ACME-2"]);
    let (_, body) = get(&app, &owner, &search("q=buff")).await;
    assert_eq!(
        keys_of(&body["data"]["results"]),
        vec!["ACME-2"],
        "title fragment"
    );
    let (_, body) = get(&app, &owner, &search("assignee=me")).await;
    assert_eq!(keys_of(&body["data"]["results"]), vec!["ACME-1"]);
    let (_, body) = get(&app, &owner, &search("assignee=none")).await;
    assert_eq!(keys_of(&body["data"]["results"]), vec!["ACME-2"]);
    let (_, body) = get(&app, &owner, &search("state=closed")).await;
    assert_eq!(body["data"]["results"], json!([]));

    // Words also match the state and an assignee's name ("Test User"), and
    // the native qualifiers narrow (g5.issue.native.search).
    for (query, expected) in [
        ("q=test&state=all", vec!["ACME-1"]),
        ("q=Test+crash", vec!["ACME-1"]),
        ("q=test+buffer", vec![]),
        ("q=open&state=all", vec!["ACME-2", "ACME-1"]),
        ("q=closed&state=all", vec![]),
        ("q=assignee:%40me", vec!["ACME-1"]),
        ("q=assignee:test", vec!["ACME-1"]),
        ("q=assignee:nobody", vec![]),
        ("q=-assignee:%40me", vec!["ACME-2"]),
        ("q=no:assignee", vec!["ACME-2"]),
        ("q=author:%40me", vec!["ACME-2", "ACME-1"]),
        ("q=author:%22test+user%22+buffer", vec!["ACME-2"]),
        ("q=buffer+OR+crash", vec!["ACME-2", "ACME-1"]),
        ("q=%22flush+the+buffer%22", vec!["ACME-2"]),
        ("q=%22buffer+the+flush%22", vec![]),
        ("q=-crash", vec!["ACME-2"]),
        // A negated word still filters when the title fragment or the key
        // would otherwise match.
        ("q=buff+-flush", vec![]),
        ("q=ACME-2+-buffer", vec![]),
        ("q=the+crash", vec!["ACME-1"]),
        // State belongs to the State control, as on GitHub domains.
        ("q=is:closed", vec!["ACME-2", "ACME-1"]),
    ] {
        let (status, body) = get(&app, &owner, &search(query)).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(body["data"]["domainErrors"], json!([]), "{query}");
        assert_eq!(keys_of(&body["data"]["results"]), expected, "{query}");
    }
    // A GitHub qualifier G5 issues cannot honour is refused, never ignored.
    let (status, body) = get(&app, &owner, &search("q=milestone:v1")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["results"], json!([]));
    assert_eq!(
        body["data"]["domainErrors"][0]["issueError"], "invalid_query",
        "{body}"
    );

    // References by key (any case) and by page URL; other keys are not ours.
    for reference in ["ACME-1", "acme-1", &format!("/{}/issues/g5/1", org.slug)] {
        let (status, body) = get(&app, &owner, &format!("{issues}/resolve?ref={reference}")).await;
        assert_eq!(status, StatusCode::OK, "{reference}: {body}");
        assert_eq!(body["data"]["displayKey"], "ACME-1");
    }
    let (status, body) = get(&app, &owner, &format!("{issues}/resolve?ref=OTHER-1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // The issue page reads G5's store.
    let (status, page) = get(&app, &owner, &format!("{issues}/g5/1")).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["data"]["title"], "Crash on save");
    assert_eq!(page["data"]["body"], "Steps to reproduce");
    assert_eq!(page["data"]["comments"], json!([]));
    assert_eq!(page["data"]["assignees"][0]["displayName"], "Test User");
    let (status, body) = get(&app, &owner, &format!("{issues}/g5/40")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    app.cleanup().await;
}

#[tokio::test]
async fn g5_issues_link_to_workspaces_and_carry_activity_comments_and_state() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let issues = format!("/api/v1/orgs/{}/issues", org.id);
    for (key, title) in [("one", "Primary work"), ("two", "Side work")] {
        let (status, body) = create_issue(
            &app,
            &owner,
            org.id,
            &domain["id"],
            json!({"title": title, "idempotencyKey": key}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let workspace = insert_workspace(&app, org.id, owner.id, "fix-crash", false).await;
    let other = insert_workspace(&app, org.id, owner.id, "other-work", false).await;
    let base = insert_workspace(&app, org.id, owner.id, "base", true).await;
    let links = |workspace: Uuid| format!("/api/v1/workspaces/{workspace}/issue-links");

    // Links keep the primary rules: one primary per issue, none on Base.
    let (status, linked) = post(
        &app,
        &owner,
        &links(workspace),
        &json!({"ref": "ACME-1", "role": "primary"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{linked}");
    assert_eq!(linked["data"]["provider"], "g5");
    assert_eq!(
        linked["data"]["locator"],
        json!({"repositoryId": "g5", "number": 1})
    );
    let link_id = linked["data"]["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &app,
        &owner,
        &links(other),
        &json!({"ref": "ACME-1", "role": "primary"}),
    )
    .await;
    assert_conflict_code(status, &body, "issue_has_primary");
    let (status, body) = post(
        &app,
        &owner,
        &links(other),
        &json!({"ref": "acme-1", "role": "related"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = post(
        &app,
        &owner,
        &links(base),
        &json!({"ref": "ACME-2", "role": "related"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_READ_ONLY", "{body}");
    let (status, list) = get(&app, &owner, &links(workspace)).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(
        list["data"]["links"][0]["issue"]["title"], "Primary work",
        "{list}"
    );
    assert!(list["data"]["links"][0]["issueError"].is_null(), "{list}");

    // Activity follows the G5 issue; the composer needs no GitHub account.
    let activity = format!("/api/v1/workspaces/{workspace}/activity");
    let (status, page) = get(&app, &owner, &activity).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["data"]["primaryIssue"]["provider"], "g5", "{page}");
    assert_eq!(page["data"]["composer"]["state"], "ready", "{page}");
    assert_eq!(page["data"]["composer"]["provider"], "g5");
    assert!(page["data"]["composer"]["githubLogin"].is_null());
    assert!(page["data"]["issueError"].is_null(), "{page}");
    let opened: Vec<&Value> = page["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["source"] == "g5")
        .collect();
    assert_eq!(opened.len(), 1, "{page}");
    assert_eq!(opened[0]["kind"], "issue_opened");

    let destination = page["data"]["composer"]["destination"].clone();
    let comment =
        json!({"body": "Found the cause", "idempotencyKey": "c-1", "destination": destination});
    let (status, posted) = post(&app, &owner, &format!("{activity}/comments"), &comment).await;
    assert_eq!(status, StatusCode::CREATED, "{posted}");
    assert_eq!(posted["data"]["state"], "published");
    assert_eq!(posted["data"]["provider"], "g5");
    assert_eq!(posted["data"]["displayKey"], "ACME-1");
    let comment_id = posted["data"]["comment"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let seen = posted["data"]["comment"]["updatedAt"].clone();
    let (status, replay) = post(&app, &owner, &format!("{activity}/comments"), &comment).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["comment"]["id"], comment_id.as_str());

    // Edits apply only to the version the editor saw, and only the author's.
    let edit_path = format!("{activity}/comments/g5/{comment_id}");
    let (status, edited) = patch(
        &app,
        &owner,
        &edit_path,
        &json!({"body": "Found the real cause", "destination": destination, "expectedUpdatedAt": seen}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["kind"], "edit");
    assert_eq!(edited["data"]["body"], "Found the real cause");
    let (status, body) = patch(
        &app,
        &owner,
        &edit_path,
        &json!({"body": "Stale write", "expectedUpdatedAt": seen}),
    )
    .await;
    assert_conflict_code(status, &body, "comment_changed");
    let colleague = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let (status, body) = patch(
        &app,
        &colleague,
        &format!("{issues}/g5/1/comments/{comment_id}"),
        &json!({"body": "Not mine", "expectedUpdatedAt": edited["data"]["comment"]["updatedAt"]}),
    )
    .await;
    assert_conflict_code(status, &body, "not_your_comment");

    // Close and reopen from the workspace; repeating a close records one event.
    let state_path = format!("{}/{link_id}/state", links(workspace));
    for _ in 0..2 {
        let (status, body) = post(&app, &owner, &state_path, &json!({"state": "closed"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["outcome"], "applied");
        assert_eq!(body["data"]["issue"]["state"], "closed");
        assert_eq!(body["data"]["issue"]["stateReason"], "completed");
        assert!(body["data"]["githubLogin"].is_null());
    }
    let (status, body) = post(&app, &owner, &state_path, &json!({"state": "open"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["issue"]["state"], "open");
    assert_eq!(body["data"]["issue"]["stateReason"], "reopened");
    let (_, page) = get(&app, &owner, &activity).await;
    let events: Vec<String> = page["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["source"] == "g5" && entry["kind"] == "issue_event")
        .map(|entry| entry["event"].as_str().unwrap().to_string())
        .collect();
    let mut events = events;
    events.sort();
    assert_eq!(events, vec!["closed", "reopened"], "{page}");
    let comments = page["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["source"] == "g5" && entry["kind"] == "issue_comment")
        .count();
    assert_eq!(comments, 1, "{page}");

    // Issue page actions: comment, conditional edit, assignees, state.
    let (status, body) = post(
        &app,
        &colleague,
        &format!("{issues}/g5/2/comments"),
        &json!({"body": "I can take this", "idempotencyKey": "page-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, page) = get(&app, &owner, &format!("{issues}/g5/2")).await;
    let version = page["data"]["updatedAt"].clone();
    let (status, body) = patch(
        &app,
        &owner,
        &format!("{issues}/g5/2"),
        &json!({"title": "Side work, renamed", "assignees": [colleague.email], "expectedUpdatedAt": version}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["title"], "Side work, renamed");
    assert_eq!(
        body["data"]["assignees"][0]["login"],
        colleague.email.as_str()
    );
    let (status, body) = patch(
        &app,
        &owner,
        &format!("{issues}/g5/2"),
        &json!({"body": "Stale", "expectedUpdatedAt": version}),
    )
    .await;
    assert_conflict_code(status, &body, "issue_changed");
    let (status, body) = post(
        &app,
        &owner,
        &format!("{issues}/g5/2/state"),
        &json!({"state": "closed"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["issue"]["state"], "closed");

    // Export, and a domain with issues is not removed.
    let domain_path = format!(
        "/api/v1/orgs/{}/issue-domains/{}",
        org.id,
        domain["id"].as_str().unwrap()
    );
    let (status, export) = get(&app, &owner, &format!("{domain_path}/export")).await;
    assert_eq!(status, StatusCode::OK, "{export}");
    assert_eq!(export["format"], "g5-native-issues/1");
    assert_eq!(export["issues"].as_array().unwrap().len(), 2);
    assert_eq!(
        export["issues"][1]["assignees"][0]["email"],
        colleague.email.as_str()
    );
    assert_eq!(export["issues"][1]["events"][0]["event"], "assigned");
    assert_eq!(
        export["issues"][0]["comments"][0]["body"],
        "Found the real cause"
    );
    let (status, body) = get(&app, &colleague, &format!("{domain_path}/export")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let response = app.authenticated_client(&owner).delete(&domain_path).await;
    let (status, body) = json_of(response).await;
    assert_conflict_code(status, &body, "domain_has_issues");

    // Unlinking ends the claim and leaves the issue alone.
    let (status, body) = post(
        &app,
        &owner,
        &format!("{}/{link_id}/unlink", links(workspace)),
        &json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &app,
        &owner,
        &links(other),
        &json!({"ref": "ACME-2", "role": "primary"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    app.cleanup().await;
}

#[tokio::test]
async fn g5_issues_stay_in_their_organization_and_viewers_only_read() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let (other_owner, other_org, other_domain) = native_org(&app, "BETA").await;
    for (user, org_id, domain_id) in [
        (&owner, org.id, &domain["id"]),
        (&other_owner, other_org.id, &other_domain["id"]),
    ] {
        let (status, body) = create_issue(
            &app,
            user,
            org_id,
            domain_id,
            json!({"title": "Secret plans", "idempotencyKey": "k"}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        // Numbering is per domain.
        assert_eq!(body["data"]["issue"]["number"], 1, "{body}");
    }
    // Another organization's member sees none of it.
    let (status, _) = get(
        &app,
        &other_owner,
        &format!("/api/v1/orgs/{}/issues/g5/1", org.id),
    )
    .await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "{status}"
    );
    let (status, _) = get(
        &app,
        &other_owner,
        &format!("/api/v1/orgs/{}/issues/search?q=", org.id),
    )
    .await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "{status}"
    );
    // An outsider cannot probe the org's members through assignee errors:
    // the org is authorized before any assignee is looked up.
    let (status, body) = patch(
        &app,
        &other_owner,
        &format!("/api/v1/orgs/{}/issues/g5/1", org.id),
        &json!({"assignees": ["someone@example.com"], "expectedUpdatedAt": "2026-01-01T00:00:00Z"}),
    )
    .await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "{status}: {body}"
    );
    // Row-level security, not just the route, keeps the rows apart.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM native_issues WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class WHERE relname = 'native_issues'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(forced);

    // A viewer reads but does not write.
    let viewer = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role = 'viewer' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(viewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = get(
        &app,
        &viewer,
        &format!("/api/v1/orgs/{}/issues/g5/1", org.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &app,
        &viewer,
        &format!("/api/v1/orgs/{}/issues/g5/1/comments", org.id),
        &json!({"body": "Drive-by", "idempotencyKey": "v"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = create_issue(
        &app,
        &viewer,
        org.id,
        &domain["id"],
        json!({"title": "Viewer issue", "idempotencyKey": "v"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // Viewers are not assignable.
    let (status, body) = create_issue(
        &app,
        &owner,
        org.id,
        &domain["id"],
        json!({"title": "Assign viewer", "assignees": [viewer.email], "idempotencyKey": "av"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // With issues off, reads are refused and nothing is lost.
    sqlx::query(
        "UPDATE org_configs SET settings_json = settings_json || '{\"external_issues_enabled\": false}' WHERE org_id = $1",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = get(
        &app,
        &owner,
        &format!("/api/v1/orgs/{}/issues/g5/1", org.id),
    )
    .await;
    assert_conflict_code(status, &body, "external_issues_disabled");

    app.cleanup().await;
}

#[tokio::test]
async fn the_workspace_agent_edits_closes_and_reopens_any_g5_issue_as_its_owner() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let (status, body) = create_issue(
        &app,
        &owner,
        org.id,
        &domain["id"],
        json!({"title": "Existing", "body": "Old", "idempotencyKey": "existing"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let colleague = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-edits", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let agent_route = format!("/api/internal/workspace-agent/{workspace}");
    use reqwest::Method;
    let edit = |body: Value| {
        let (app, token, url) = (&app, &token, format!("{agent_route}/issue-edit"));
        async move { agent(app, token, Method::POST, &url, Some(&body)).await }
    };
    let events = || async {
        let kinds: Vec<String> = sqlx::query_scalar(
            "SELECT kind FROM native_issue_events WHERE org_id = $1 ORDER BY id",
        )
        .bind(org.id)
        .fetch_all(&app.db)
        .await
        .unwrap();
        kinds
    };
    let issue_row = || async {
        let row: (String, String, i64) = sqlx::query_as(
            "SELECT title, body, (SELECT count(*) FROM native_issue_assignees a WHERE a.issue_id = i.id) \
             FROM native_issues i WHERE org_id = $1 AND number = 1",
        )
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        row
    };

    // Without a reference and without a primary issue, nothing changes.
    let (status, body) = edit(json!({"title": "Nowhere"})).await;
    assert_conflict_code(status, &body, "no_primary_issue");

    // `g5d issue edit ACME-1 -t … -b … --add-assignee @me,<email>`: an
    // unlinked issue, as the owner, one event per assignment.
    let change = json!({"ref": "ACME-1", "title": "Renamed", "body": "New",
        "addAssignees": ["@me", colleague.email.to_uppercase()]});
    let (status, edited) = edit(change.clone()).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["outcome"], "applied", "{edited}");
    assert!(edited["data"]["githubLogin"].is_null(), "{edited}");
    assert_eq!(edited["data"]["issue"]["title"], "Renamed");
    let mut assigned: Vec<&str> = edited["data"]["issue"]["assignees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|person| person["login"].as_str().unwrap())
        .collect();
    assigned.sort_unstable();
    let mut expected = vec![owner.email.as_str(), colleague.email.as_str()];
    expected.sort_unstable();
    assert_eq!(assigned, expected);
    assert_eq!(events().await, vec!["assigned", "assigned"]);
    // Running the same edit again changes nothing and records nothing.
    let (status, again) = edit(change).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["outcome"], "applied");
    assert_eq!(events().await.len(), 2);

    // `--remove-assignee <email>`, on the issue by its bare number.
    let (status, removed) = edit(json!({"ref": "1", "removeAssignees": [colleague.email]})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(
        removed["data"]["issue"]["assignees"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "{removed}"
    );
    assert_eq!(events().await, vec!["assigned", "assigned", "unassigned"]);

    // Refusals change nothing: an unknown person, someone both added and
    // removed, an edit naming no change, an issue that does not exist.
    for (request, expected) in [
        (
            json!({"ref": "ACME-1", "title": "No", "addAssignees": ["stranger@example.com"]}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"ref": "ACME-1", "addAssignees": ["@me"], "removeAssignees": [owner.email]}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (json!({"ref": "ACME-1"}), StatusCode::UNPROCESSABLE_ENTITY),
        (
            json!({"ref": "ACME-77", "title": "No"}),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (status, body) = edit(request.clone()).await;
        assert_eq!(status, expected, "{request}: {body}");
    }
    assert_eq!(
        issue_row().await,
        ("Renamed".to_string(), "New".to_string(), 1)
    );
    assert_eq!(events().await.len(), 3);

    // `g5d issue close ACME-1` and `reopen`: any readable issue, not only
    // linked ones.
    for (state, event) in [("closed", "closed"), ("open", "reopened")] {
        let (status, changed) = agent(
            &app,
            &token,
            Method::POST,
            &format!("{agent_route}/issue-state"),
            Some(&json!({"ref": "ACME-1", "state": state})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{changed}");
        assert_eq!(changed["data"]["outcome"], "applied", "{changed}");
        assert_eq!(changed["data"]["issue"]["state"], state, "{changed}");
        assert_eq!(events().await.last().map(String::as_str), Some(event));
    }

    // A review copy's agent changes nothing.
    let review_copy = insert_workspace(&app, org.id, owner.id, "agent-edits-review", false).await;
    let repo_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) \
         VALUES ($1, $2, 'edits', 'edits', 'file:///unused', 'main', 'github')",
    )
    .bind(repo_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_forks(
             id, org_id, source_workspace_id, target_workspace_id, requested_by, owner_user_id,
             idempotency_key, request_digest, proposed_name, repo_manifest, status, completed_at)
         VALUES ($1,$2,$3,$4,$5,$5,$6,$7,$8,$9,'completed',now())",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(workspace)
    .bind(review_copy)
    .bind(owner.id)
    .bind("review-workspace:agent-edits:1")
    .bind("c".repeat(64))
    .bind("agent-edits-review")
    .bind(json!([{"repo_id": repo_id}]))
    .execute(&app.db)
    .await
    .unwrap();
    let review_token = agent_token(&app, org.id, review_copy, owner.id).await;
    let review_route = format!("/api/internal/workspace-agent/{review_copy}");
    for (leaf, request) in [
        (
            "issue-edit",
            json!({"ref": "ACME-1", "title": "From review"}),
        ),
        ("issue-state", json!({"ref": "ACME-1", "state": "closed"})),
    ] {
        let (status, body) = agent(
            &app,
            &review_token,
            Method::POST,
            &format!("{review_route}/{leaf}"),
            Some(&request),
        )
        .await;
        assert!(status.is_client_error(), "{leaf}: {status}: {body}");
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{leaf}: {body}");
    }
    assert_eq!(issue_row().await.0, "Renamed");
    assert_eq!(events().await.last().map(String::as_str), Some("reopened"));

    app.cleanup().await;
}

#[tokio::test]
async fn the_workspace_agent_creates_reads_links_and_comments_on_any_g5_issue_as_its_owner() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let (status, body) = create_issue(
        &app,
        &owner,
        org.id,
        &domain["id"],
        json!({"title": "Existing", "idempotencyKey": "existing"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-work", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let internal = format!("/api/internal/workspaces/{workspace}");
    use reqwest::Method;

    // `g5d issue create`: the owner's issue, recorded with agent origin.
    let (status, created) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{internal}/issues"),
        Some(&json!({"title": "Follow-up", "assignees": ["@me"], "parent": "1", "idempotencyKey": "a-1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["data"]["issue"]["displayKey"], "ACME-2");
    assert_eq!(created["data"]["parent"]["displayKey"], "ACME-1");
    let (author, origin): (Uuid, String) = sqlx::query_as(
        "SELECT author_user_id, origin FROM native_issues WHERE org_id = $1 AND number = 2",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!((author, origin.as_str()), (owner.id, "agent"));

    // `g5d issue view 1`: a bare number names the creation domain's issue.
    let (status, viewed) = agent(
        &app,
        &token,
        Method::GET,
        &format!("{internal}/issues/view?ref=1&comments=true"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{viewed}");
    assert_eq!(viewed["data"]["issue"]["displayKey"], "ACME-1");
    assert!(viewed["data"]["workspaceLink"].is_null());

    // `g5d issue attachments 1`: nothing uploaded yet, so none are listed,
    // and a handle names nothing (`g5.attachments`).
    let (status, attachments) = agent(
        &app,
        &token,
        Method::GET,
        &format!("{internal}/issues/attachments?ref=1&comments=true"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{attachments}");
    assert_eq!(attachments["data"]["issue"]["displayKey"], "ACME-1");
    assert_eq!(attachments["data"]["attachments"], json!([]));
    let (status, content) = agent(
        &app,
        &token,
        Method::GET,
        &format!("{internal}/issues/attachments/content?ref=1&handle=g5:1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{content}");

    // `g5d issue link ACME-1 --primary`, then `g5d issue comment` defaults to it.
    let (status, linked) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{internal}/issue-links"),
        Some(&json!({"ref": "1", "role": "primary"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{linked}");
    assert_eq!(linked["data"]["displayKey"], "ACME-1");
    let (status, comment) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{internal}/issue-comments"),
        Some(&json!({"body": "Started", "idempotencyKey": "c-1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{comment}");
    assert_eq!(comment["data"]["displayKey"], "ACME-1");
    assert_eq!(comment["data"]["origin"], "agent");

    // `g5d issue comment ACME-2`: any issue, linked or not.
    let (status, comment) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{internal}/issue-comments"),
        Some(&json!({"body": "Noted", "idempotencyKey": "c-2", "ref": "ACME-2"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{comment}");
    assert_eq!(comment["data"]["displayKey"], "ACME-2");
    let (status, replay) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{internal}/issue-comments"),
        Some(&json!({"body": "Noted", "idempotencyKey": "c-2", "ref": "ACME-2"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{internal}/issue-comments"),
        Some(&json!({"body": "Nowhere", "idempotencyKey": "c-3", "ref": "ACME-77"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // `g5d issue search` and `list` read G5's store, spending no GitHub budget.
    let (status, found) = agent(
        &app,
        &token,
        Method::GET,
        &format!("{internal}/issues/search?q=follow&repo=ACME"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(keys_of(&found["data"]["results"]), vec!["ACME-2"]);
    let budget: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM rate_limit_windows WHERE key LIKE '%agent-issue-search%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(budget, 0);
    let (status, list) = agent(
        &app,
        &token,
        Method::GET,
        &format!("{internal}/issue-links"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(
        list["data"]["links"][0]["issue"]["displayKey"], "ACME-1",
        "{list}"
    );

    app.cleanup().await;
}

/// `g5.issue.native.attachments`: members upload images to a G5 issue,
/// exactly once per key and only recognized images within the bounds; the
/// listing derives where each appears; readers get the bytes with their
/// recognized type; the uploader or an administrator removes one; the
/// agent lists and fetches them; another organization sees none; export
/// carries them. The bytes live in the attachment store under the
/// organization's id and a random id, never in the database, and go with
/// their removal (`g5.issue.native.attachment-store`).
#[tokio::test]
async fn members_attach_images_to_g5_issues_and_the_agent_reads_them() {
    use base64::Engine as _;
    use reqwest::Method;
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let (status, created) = create_issue(
        &app,
        &member,
        org.id,
        &domain["id"],
        json!({"title": "Mocks", "idempotencyKey": "issue-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let issue_path = format!("/api/v1/orgs/{}/issues/g5/1", org.id);
    let uploads = format!("{issue_path}/attachments");
    let png = b"\x89PNG\r\n\x1a\nmock popup".to_vec();
    let gif = b"GIF89a dropdown".to_vec();

    // An upload returns the Markdown that shows it; a retry with the key
    // returns the same attachment, and the key with other content is refused.
    let upload = |name: &str, bytes: &[u8], key: &str| json!({"fileName": name, "contentBase64": b64(bytes), "idempotencyKey": key});
    let (_, before) = get(&app, &member, &issue_path).await;
    let (status, first) = post(&app, &member, &uploads, &upload("popup.png", &png, "u-1")).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let first = first["data"].clone();
    assert_eq!(first["contentType"], "image/png");
    assert_eq!(first["sizeBytes"], png.len());
    let url = first["url"].as_str().unwrap().to_string();
    assert!(
        url.ends_with(&format!(
            "/issues/g5/1/attachments/{}",
            first["id"].as_str().unwrap()
        )),
        "{url}"
    );
    assert_eq!(first["markdown"], format!("![popup.png]({url})"));
    // Its bytes are a file named by the organization and a random id.
    // A keyed repository path is `<base>/<org>/<repo>`.
    let attachment_root = app
        .repo_storage
        .test_path(Uuid::nil(), Uuid::nil())
        .parent()
        .and_then(|org| org.parent())
        .map(|base| base.join(".attachments"))
        .expect("test storage is local");
    let stored_file = |id: &str| {
        let pool = app.db.clone();
        let id: i64 = id.parse().unwrap();
        let root = attachment_root.clone();
        async move {
            let (org_id, storage_id): (Uuid, Uuid) = sqlx::query_as(
                "SELECT org_id, storage_id FROM native_issue_attachments WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
            let name = storage_id.to_string();
            root.join(org_id.to_string()).join(&name[..2]).join(name)
        }
    };
    let first_file = stored_file(first["id"].as_str().unwrap()).await;
    assert!(first_file.starts_with(attachment_root.join(org.id.to_string())));
    assert_eq!(std::fs::read(&first_file).unwrap(), png);
    let content_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE table_name = 'native_issue_attachments' AND data_type = 'bytea'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        content_columns, 0,
        "attachment bytes stay out of the database"
    );
    let (status, replay) = post(&app, &member, &uploads, &upload("popup.png", &png, "u-1")).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["id"], first["id"]);
    let (status, body) = post(&app, &member, &uploads, &upload("other.png", &png, "u-1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // Uploading leaves the issue's edit version alone.
    let (_, after) = get(&app, &member, &issue_path).await;
    assert_eq!(after["data"]["updatedAt"], before["data"]["updatedAt"]);
    // Only recognized images within 10 MiB.
    for (name, bytes) in [
        (
            "page.svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(),
        ),
        ("notes.txt", b"plain text".to_vec()),
        ("empty.png", Vec::new()),
    ] {
        let (status, body) = post(&app, &member, &uploads, &upload(name, &bytes, name)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {body}");
    }
    let mut huge = png.clone();
    huge.resize(10 * 1024 * 1024 + 1, 0);
    let (status, body) = post(&app, &member, &uploads, &upload("huge.png", &huge, "huge")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // A second image, shown by a comment; the first by the description.
    let (status, second) = post(&app, &owner, &uploads, &upload("mode.gif", &gif, "u-2")).await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    let second = second["data"].clone();
    let (status, body) = patch(
        &app,
        &member,
        &issue_path,
        &json!({"body": format!("Popup:\n\n{}", first["markdown"].as_str().unwrap()),
                "expectedUpdatedAt": before["data"]["updatedAt"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &app,
        &owner,
        &format!("{issue_path}/comments"),
        &json!({"body": format!("Mode: {}", second["markdown"].as_str().unwrap()), "idempotencyKey": "c-1"}),
    )
    .await;
    assert!(status.is_success(), "{body}");
    let (status, listed) = get(&app, &member, &uploads).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let places: Vec<&str> = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["place"].as_str().unwrap())
        .collect();
    assert_eq!(places, ["description", "comment"], "{listed}");

    // A reader gets the bytes with their recognized type, never sniffed.
    let response = app
        .authenticated_client(&member)
        .get(&format!("{uploads}/{}", first["id"].as_str().unwrap()))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(response.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("inline"));
    assert_eq!(response.bytes().await.unwrap().to_vec(), png);

    // The workspace's agent lists and fetches them as its owner.
    let workspace =
        insert_workspace(&app, org.id, owner.id, "agent-reads-attachments", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let internal = format!("/api/internal/workspaces/{workspace}");
    let (status, listing) = agent(
        &app,
        &token,
        Method::GET,
        &format!("{internal}/issues/attachments?ref=ACME-1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    let found = listing["data"]["attachments"].as_array().unwrap();
    assert_eq!(found.len(), 2, "{listing}");
    assert_eq!(found[0]["kind"], "image");
    assert_eq!(found[0]["name"], "popup.png");
    assert_eq!(found[1]["place"], "comment");
    let response = reqwest::Client::new()
        .get(format!(
            "{}{internal}/issues/attachments/content?ref=ACME-1&handle={}",
            app.base_url,
            found[1]["handle"].as_str().unwrap()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/gif");
    assert_eq!(response.bytes().await.unwrap().to_vec(), gif);

    // Another organization's member sees none of it.
    let (other_owner, _, _) = native_org(&app, "BETA").await;
    for path in [
        uploads.clone(),
        format!("{uploads}/{}", first["id"].as_str().unwrap()),
    ] {
        let status = app
            .authenticated_client(&other_owner)
            .get(&path)
            .await
            .status();
        assert!(
            status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
            "{path}: {status}"
        );
    }
    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class WHERE relname = 'native_issue_attachments'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(forced);

    // Export carries them with their bytes.
    let (status, exported) = get(
        &app,
        &owner,
        &format!(
            "/api/v1/orgs/{}/issue-domains/{}/export",
            org.id,
            domain["id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{exported}");
    let exported_attachments = &exported["issues"][0]["attachments"];
    assert_eq!(
        exported_attachments.as_array().map(Vec::len),
        Some(2),
        "{exported}"
    );
    assert_eq!(exported_attachments[0]["contentBase64"], b64(&png));
    assert_eq!(exported_attachments[1]["contentBase64"], b64(&gif));
    assert_eq!(exported_attachments[1]["fileName"], "mode.gif");
    assert_eq!(exported_attachments[1]["sha256"], second["sha256"]);
    assert_eq!(exported["issues"][0]["number"], 1, "{exported}");

    // Only the uploader or an administrator removes one, with its bytes.
    let second_file = stored_file(second["id"].as_str().unwrap()).await;
    assert_eq!(std::fs::read(&second_file).unwrap(), gif);
    let second_path = format!("{uploads}/{}", second["id"].as_str().unwrap());
    let status = app
        .authenticated_client(&member)
        .delete(&second_path)
        .await
        .status();
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(second_file.exists(), "a refused removal keeps the bytes");
    // Bytes lost from the store read as a missing attachment, and its
    // removal still succeeds.
    std::fs::remove_file(&second_file).unwrap();
    let status = app
        .authenticated_client(&member)
        .get(&second_path)
        .await
        .status();
    assert_eq!(status, StatusCode::NOT_FOUND);
    // The export still completes, carrying that record without bytes.
    let (status, exported) = get(
        &app,
        &owner,
        &format!(
            "/api/v1/orgs/{}/issue-domains/{}/export",
            org.id,
            domain["id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{exported}");
    let exported_attachments = &exported["issues"][0]["attachments"];
    assert_eq!(exported_attachments[0]["contentBase64"], b64(&png));
    assert_eq!(exported_attachments[1]["contentBase64"], Value::Null);
    assert_eq!(exported_attachments[1]["fileName"], "mode.gif");
    let first_path = format!("{uploads}/{}", first["id"].as_str().unwrap());
    let status = app
        .authenticated_client(&member)
        .delete(&first_path)
        .await
        .status();
    assert_eq!(status, StatusCode::NO_CONTENT);
    let status = app
        .authenticated_client(&owner)
        .delete(&second_path)
        .await
        .status();
    assert_eq!(status, StatusCode::NO_CONTENT);
    let status = app
        .authenticated_client(&member)
        .get(&first_path)
        .await
        .status();
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, listed) = get(&app, &member, &uploads).await;
    assert_eq!(listed["data"], json!([]));
    assert!(!first_file.exists() && !second_file.exists());

    // At most 50 per issue.
    for index in 0..50 {
        let (status, body) = post(
            &app,
            &member,
            &uploads,
            &upload("n.png", &png, &format!("n-{index}")),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{index}: {body}");
    }
    let (status, body) = post(&app, &member, &uploads, &upload("n.png", &png, "n-50")).await;
    assert_conflict_code(status, &body, "attachment_limit");
    app.cleanup().await;
}

/// `g5.attachments.cli.attach`: the workspace's agent adds images to a G5
/// issue as the owner, recorded as the agent's; the same file on the same
/// issue is added once; a description edit conditional on what the agent
/// created is refused once someone changed it.
#[tokio::test]
async fn the_agent_attaches_images_to_g5_issues_once_and_shows_them_conditionally() {
    use base64::Engine as _;
    use reqwest::Method;
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let (status, created) = create_issue(
        &app,
        &owner,
        org.id,
        &domain["id"],
        json!({"title": "Mocks", "body": "Start", "idempotencyKey": "issue-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-attaches", false).await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let uploads = format!("/api/internal/workspaces/{workspace}/issues/attachments");
    let png = b"\x89PNG\r\n\x1a\nagent shot".to_vec();
    let upload = |name: &str, bytes: &[u8]| json!({"ref": "ACME-1", "fileName": name, "contentBase64": b64(bytes)});

    let (status, first) = agent(
        &app,
        &token,
        Method::POST,
        &uploads,
        Some(&upload("shot.png", &png)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let data = &first["data"];
    assert_eq!(data["displayKey"], "ACME-1");
    assert!(
        data["issueUrl"].as_str().unwrap().ends_with("/issues/g5/1"),
        "{first}"
    );
    let markdown = data["attachment"]["markdown"].as_str().unwrap().to_string();
    let (origin, uploader): (String, Uuid) = sqlx::query_as(
        "SELECT origin, uploader_user_id FROM native_issue_attachments WHERE id = $1",
    )
    .bind(
        data["attachment"]["id"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap(),
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!((origin.as_str(), uploader), ("agent", owner.id));

    // The same file again is the same attachment; another name is another.
    let (status, again) = agent(
        &app,
        &token,
        Method::POST,
        &uploads,
        Some(&upload("shot.png", &png)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["attachment"]["id"], data["attachment"]["id"]);
    let (status, other) = agent(
        &app,
        &token,
        Method::POST,
        &uploads,
        Some(&upload("copy.png", &png)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{other}");
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &uploads,
        Some(&upload("notes.txt", b"plain")),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // The description edit applies only to the description it was written against.
    let edit = format!("/api/internal/workspace-agent/{workspace}/issue-edit");
    let shown = format!("Start\n\n{markdown}");
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &edit,
        Some(&json!({"ref": "ACME-1", "body": shown, "expectedBody": "Something else"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &edit,
        Some(&json!({"ref": "ACME-1", "body": shown, "expectedBody": "Start"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, issue) = get(
        &app,
        &owner,
        &format!("/api/v1/orgs/{}/issues/g5/1", org.id),
    )
    .await;
    assert_eq!(issue["data"]["body"], shown, "{issue}");

    // Creating where new issues are G5's accepts images.
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("/api/internal/workspaces/{workspace}/issues"),
        Some(&json!({"title": "With images", "body": "B", "idempotencyKey": "agent-create-1", "attachments": true})),
    )
    .await;
    assert!(status.is_success(), "{body}");
    app.cleanup().await;
}
