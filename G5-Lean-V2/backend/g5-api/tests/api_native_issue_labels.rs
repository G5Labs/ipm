//! Labels on G5's native issue tracker (`g5.issue.labels`,
//! `g5.issue.labels.native`): on a deployment without the GitHub App, a
//! domain's catalog, labels selected at creation, tagging by deltas,
//! `label:`/`no:label` search, Activity and export, tenant and viewer
//! isolation, the workspace agent as its owner, and racing writers.
//!
//! The real router, auth middleware and Postgres (RLS under `g5_tenant`) are
//! exercised; no tracker is mocked because none is called.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};
use uuid::Uuid;

// ─── Fixtures and request helpers ────────────────────────────────────────────

async fn json_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

async fn send(
    app: &TestApp,
    user: &TestUser,
    method: Method,
    path: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let client = app.authenticated_client(user);
    let response = match (method, body) {
        (Method::GET, _) => client.get(path).await,
        (Method::POST, Some(body)) => client.post_json(path, body).await,
        (Method::PATCH, Some(body)) => client.patch_json(path, body).await,
        (Method::DELETE, _) => client.delete(path).await,
        (method, _) => panic!("unsupported {method}"),
    };
    json_of(response).await
}

async fn get(app: &TestApp, user: &TestUser, path: &str) -> (StatusCode, Value) {
    send(app, user, Method::GET, path, None).await
}

async fn post(app: &TestApp, user: &TestUser, path: &str, body: &Value) -> (StatusCode, Value) {
    send(app, user, Method::POST, path, Some(body)).await
}

fn assert_code(status: StatusCode, body: &Value, expected: StatusCode, code: &str) {
    assert_eq!(status, expected, "{body}");
    assert_eq!(body["error"]["details"]["code"], code, "{body}");
}

async fn native_org(app: &TestApp, key: &str) -> (TestUser, TestOrg, String) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json, secret_keys) \
         VALUES ($1, '{\"external_issues_enabled\": true}'::jsonb, '{}') \
         ON CONFLICT (org_id) DO UPDATE \
         SET settings_json = org_configs.settings_json || EXCLUDED.settings_json",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, domain) = post(
        app,
        &owner,
        &format!("/api/v1/orgs/{}/issue-domains", org.id),
        &json!({"provider": "g5", "key": key}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{domain}");
    let domain_id = domain["data"]["id"].as_str().unwrap().to_string();
    (owner, org, domain_id)
}

fn catalog(org: &TestOrg, domain: &str) -> String {
    format!("/api/v1/orgs/{}/issue-domains/{domain}/labels", org.id)
}

async fn create_label(
    app: &TestApp,
    user: &TestUser,
    org: &TestOrg,
    domain: &str,
    name: &str,
    color: Option<&str>,
) -> Value {
    let mut request = json!({"name": name});
    if let Some(color) = color {
        request["color"] = color.into();
    }
    let (status, body) = post(app, user, &catalog(org, domain), &request).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["outcome"], "applied", "{body}");
    body["data"]["label"].clone()
}

async fn create_issue(
    app: &TestApp,
    user: &TestUser,
    org: &TestOrg,
    domain: &str,
    request: Value,
) -> (StatusCode, Value) {
    let mut request = request;
    request["domainId"] = domain.into();
    post(
        app,
        user,
        &format!("/api/v1/orgs/{}/issues", org.id),
        &request,
    )
    .await
}

async fn change_labels(
    app: &TestApp,
    user: &TestUser,
    org: &TestOrg,
    number: u64,
    add: &[&str],
    remove: &[&str],
) -> (StatusCode, Value) {
    post(
        app,
        user,
        &format!("/api/v1/orgs/{}/issues/g5/{number}/labels", org.id),
        &json!({"addLabels": add, "removeLabels": remove}),
    )
    .await
}

fn label_names(issue: &Value) -> Vec<String> {
    issue["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|label| label["name"].as_str().unwrap().to_string())
        .collect()
}

async fn issue_labels(app: &TestApp, user: &TestUser, org: &TestOrg, number: u64) -> Vec<String> {
    let (status, body) = get(
        app,
        user,
        &format!("/api/v1/orgs/{}/issues/g5/{number}", org.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    label_names(&body["data"])
}

async fn search(app: &TestApp, user: &TestUser, org: &TestOrg, query: &str) -> (StatusCode, Value) {
    let mut url = reqwest::Url::parse("http://placeholder/").unwrap();
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("state", "all");
    get(
        app,
        user,
        &format!(
            "/api/v1/orgs/{}/issues/search?{}",
            org.id,
            url.query().unwrap()
        ),
    )
    .await
}

async fn search_keys(app: &TestApp, user: &TestUser, org: &TestOrg, query: &str) -> Vec<String> {
    let (status, body) = search(app, user, org, query).await;
    assert_eq!(status, StatusCode::OK, "{query}: {body}");
    let mut keys: Vec<String> = body["data"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["displayKey"].as_str().unwrap().to_string())
        .collect();
    keys.sort();
    keys
}

/// `(kind, label_name)` of the organization's label events, oldest first.
async fn label_events(app: &TestApp, org: &TestOrg) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT kind, label_name FROM native_issue_events \
         WHERE org_id = $1 AND label_name IS NOT NULL ORDER BY id",
    )
    .bind(org.id)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

async fn insert_workspace(app: &TestApp, org_id: Uuid, owner: Uuid, name: &str) -> Uuid {
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by) \
         VALUES ($1, $2, $3, false, $4, $4)",
    )
    .bind(workspace_id)
    .bind(org_id)
    .bind(name)
    .bind(owner)
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
    method: Method,
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

// ─── Tests ───────────────────────────────────────────────────────────────────

/// The catalog: create, list a page at a time, edit, delete; names unique
/// without regard to case; a retried create is harmless; invalid input is
/// refused before anything is stored (`g5.issue.labels.contract`).
#[tokio::test]
async fn a_g5_domain_keeps_a_label_catalog() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let path = catalog(&org, &domain);

    let bug = create_label(&app, &owner, &org, &domain, "bug", Some("#D73A4A")).await;
    assert_eq!(bug["name"], "bug");
    assert_eq!(bug["color"], "d73a4a", "colours are normalized: {bug}");
    assert!(bug["id"].is_string(), "{bug}");
    let (status, body) = post(
        &app,
        &owner,
        &path,
        &json!({"name": " priority:p0 ", "description": "Drop everything"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["label"]["name"], "priority:p0");
    assert!(body["data"]["label"]["color"].is_null());
    assert_eq!(body["data"]["label"]["description"], "Drop everything");
    assert!(body["data"]["githubLogin"].is_null());

    // A retried create returns the same label; a different label with the
    // name, in any case, is a conflict carrying the existing one.
    let (status, retry) = post(
        &app,
        &owner,
        &path,
        &json!({"name": "bug", "color": "d73a4a"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{retry}");
    assert_eq!(retry["data"]["label"]["id"], bug["id"]);
    let (status, conflict) = post(
        &app,
        &owner,
        &path,
        &json!({"name": "BUG", "color": "000000"}),
    )
    .await;
    assert_code(status, &conflict, StatusCode::CONFLICT, "label_exists");
    assert_eq!(
        conflict["error"]["details"]["label"]["id"], bug["id"],
        "{conflict}"
    );

    // Validation is central and stores nothing.
    for (request, code) in [
        (json!({"name": "  "}), "invalid_label"),
        (json!({"name": "a\nb"}), "invalid_label"),
        (json!({"name": "x".repeat(51)}), "invalid_label"),
        (json!({"name": "ok", "color": "red"}), "invalid_color"),
        (
            json!({"name": "ok", "description": "d".repeat(101)}),
            "invalid_label",
        ),
    ] {
        let (status, body) = post(&app, &owner, &path, &request).await;
        assert_code(status, &body, StatusCode::UNPROCESSABLE_ENTITY, code);
    }

    // Listing is by name, a page of 100 at a time.
    let (status, page) = get(&app, &owner, &path).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["data"]["displayKey"], "ACME");
    assert_eq!(
        label_names(&page["data"]),
        vec!["bug".to_string(), "priority:p0".to_string()]
    );
    assert!(page["data"]["nextPage"].is_null(), "{page}");
    let domain_id = Uuid::parse_str(&domain).unwrap();
    for n in 0..100 {
        sqlx::query(
            "INSERT INTO native_issue_labels (org_id, domain_id, name) VALUES ($1, $2, $3)",
        )
        .bind(org.id)
        .bind(domain_id)
        .bind(format!("zz-{n:03}"))
        .execute(&app.db)
        .await
        .unwrap();
    }
    let (_, first) = get(&app, &owner, &path).await;
    assert_eq!(first["data"]["labels"].as_array().unwrap().len(), 100);
    assert_eq!(
        first["data"]["nextPage"], 2,
        "{}",
        first["data"]["nextPage"]
    );
    let (_, second) = get(&app, &owner, &format!("{path}?page=2")).await;
    assert_eq!(
        label_names(&second["data"]),
        vec!["zz-098".to_string(), "zz-099".to_string()]
    );
    assert!(second["data"]["nextPage"].is_null());

    // Edit: rename, recolour, clear the description; taking another
    // label's name is refused; an edit naming nothing is refused.
    let label_path = |label: &Value| format!("{path}/{}", label["id"].as_str().unwrap());
    let (status, renamed) = send(
        &app,
        &owner,
        Method::PATCH,
        &label_path(&bug),
        Some(&json!({"name": "defect", "color": "0E8A16", "description": ""})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed["data"]["label"]["name"], "defect");
    assert_eq!(renamed["data"]["label"]["color"], "0e8a16");
    assert_eq!(renamed["data"]["label"]["id"], bug["id"]);
    let (status, body) = send(
        &app,
        &owner,
        Method::PATCH,
        &label_path(&bug),
        Some(&json!({"name": "Priority:P0"})),
    )
    .await;
    assert_code(status, &body, StatusCode::CONFLICT, "label_exists");
    let (status, body) = send(
        &app,
        &owner,
        Method::PATCH,
        &label_path(&bug),
        Some(&json!({})),
    )
    .await;
    assert_code(status, &body, StatusCode::UNPROCESSABLE_ENTITY, "no_change");
    // Changing only the case of its own name is allowed.
    let (status, body) = send(
        &app,
        &owner,
        Method::PATCH,
        &label_path(&bug),
        Some(&json!({"name": "Defect"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Delete, then it is gone.
    let (status, deleted) = send(&app, &owner, Method::DELETE, &label_path(&bug), None).await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["data"]["label"]["name"], "Defect");
    let (status, body) = send(&app, &owner, Method::DELETE, &label_path(&bug), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = send(
        &app,
        &owner,
        Method::PATCH,
        &format!("{path}/not-a-label"),
        Some(&json!({"name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    app.cleanup().await;
}

/// Creation selects existing labels in its exactly-once transaction;
/// tagging adds and removes by deltas, recording one event per actual
/// change; search, rename, deletion, Activity and export follow
/// (`g5.issue.labels.native`).
#[tokio::test]
async fn g5_issues_carry_labels_through_creation_tagging_search_and_export() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    for (name, color) in [
        ("bug", Some("d73a4a")),
        ("priority:p0", None),
        ("priority:p1", None),
        ("good first issue", None),
    ] {
        create_label(&app, &owner, &org, &domain, name, color).await;
    }

    // Creation: unknown labels are refused and nothing is created.
    let (status, body) = create_issue(
        &app,
        &owner,
        &org,
        &domain,
        json!({"title": "Crash", "labels": ["bug", "nope"], "idempotencyKey": "c1"}),
    )
    .await;
    assert_code(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown_label",
    );
    assert!(body["error"]["details"]["error"]
        .as_str()
        .unwrap()
        .contains("nope"));
    let issues: i64 = sqlx::query_scalar("SELECT count(*) FROM native_issues WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(issues, 0);
    let create =
        json!({"title": "Crash", "labels": ["BUG", "priority:p0"], "idempotencyKey": "c1"});
    let (status, created) = create_issue(&app, &owner, &org, &domain, create.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(
        created["data"]["labels"],
        json!(["bug", "priority:p0"]),
        "the catalog's names: {created}"
    );
    assert!(
        created["data"].get("labelsNotApplied").is_none(),
        "{created}"
    );
    // A replay returns the first issue; the same key with other labels is
    // refused.
    let (status, replay) = create_issue(&app, &owner, &org, &domain, create).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"]["sent"], false);
    let (status, body) = create_issue(
        &app,
        &owner,
        &org,
        &domain,
        json!({"title": "Crash", "labels": ["bug"], "idempotencyKey": "c1"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = create_issue(
        &app,
        &owner,
        &org,
        &domain,
        json!({"title": "Plain", "idempotencyKey": "c2"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        issue_labels(&app, &owner, &org, 1).await,
        vec!["bug", "priority:p0"]
    );
    assert!(
        label_events(&app, &org).await.is_empty(),
        "creation records no events"
    );

    // Tagging: replace a priority by removing the old and adding the new.
    let (status, changed) =
        change_labels(&app, &owner, &org, 1, &["priority:p1"], &["Priority:P0"]).await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["data"]["outcome"], "applied", "{changed}");
    assert_eq!(
        changed["data"]["changes"],
        json!([
            {"label": "priority:p1", "change": "add", "outcome": "applied"},
            {"label": "Priority:P0", "change": "remove", "outcome": "applied"}
        ])
    );
    assert_eq!(
        label_names(&changed["data"]["issue"]),
        vec!["bug", "priority:p1"]
    );
    assert_eq!(
        label_events(&app, &org).await,
        vec![
            ("unlabeled".to_string(), "priority:p0".to_string()),
            ("labeled".to_string(), "priority:p1".to_string()),
        ]
    );
    // Adding a label the issue has, or removing one it lacks, changes
    // nothing and records nothing; the issue's version stays.
    let (_, before) = get(
        &app,
        &owner,
        &format!("/api/v1/orgs/{}/issues/g5/1", org.id),
    )
    .await;
    let (status, same) =
        change_labels(&app, &owner, &org, 1, &["bug"], &["good first issue"]).await;
    assert_eq!(status, StatusCode::OK, "{same}");
    assert_eq!(same["data"]["outcome"], "applied");
    assert_eq!(label_events(&app, &org).await.len(), 2);
    assert_eq!(
        same["data"]["issue"]["updatedAt"],
        before["data"]["updatedAt"]
    );
    // Refusals change nothing.
    for (add, remove, code) in [
        (&["missing"][..], &[][..], "unknown_label"),
        (&["bug"][..], &["BUG"][..], "label_conflict"),
        (&["bug", "Bug"][..], &[][..], "duplicate_label"),
        (&[][..], &[][..], "no_change"),
    ] {
        let (status, body) = change_labels(&app, &owner, &org, 1, add, remove).await;
        assert_code(status, &body, StatusCode::UNPROCESSABLE_ENTITY, code);
    }
    let (status, body) = change_labels(&app, &owner, &org, 99, &["bug"], &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(label_events(&app, &org).await.len(), 2);

    // Search: `label:` (any case, quoted, a,b for either), `-label:` and
    // `no:label`.
    change_labels(&app, &owner, &org, 2, &["good first issue"], &[]).await;
    for (query, expected) in [
        ("label:bug", vec!["ACME-1"]),
        ("label:BUG", vec!["ACME-1"]),
        (r#"label:"good first issue""#, vec!["ACME-2"]),
        ("label:priority:p1", vec!["ACME-1"]),
        ("label:bug,missing", vec!["ACME-1"]),
        ("-label:bug", vec!["ACME-2"]),
        ("label:missing", vec![]),
        ("crash label:bug", vec!["ACME-1"]),
        ("no:label", vec![]),
    ] {
        let expected: Vec<String> = expected.into_iter().map(String::from).collect();
        assert_eq!(
            search_keys(&app, &owner, &org, query).await,
            expected,
            "{query}"
        );
    }
    change_labels(&app, &owner, &org, 2, &[], &["good first issue"]).await;
    assert_eq!(
        search_keys(&app, &owner, &org, "no:label").await,
        vec!["ACME-2"]
    );
    let (status, body) = search(&app, &owner, &org, "label:").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a domain error, not a failure: {body}"
    );
    assert_eq!(
        body["data"]["domainErrors"][0]["issueError"], "invalid_query",
        "{body}"
    );

    // Rename keeps assignments: issues show the new name; history keeps the
    // old one.
    let (_, page) = get(&app, &owner, &catalog(&org, &domain)).await;
    let id_of = |name: &str| {
        page["data"]["labels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|label| label["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let p1 = id_of("priority:p1");
    let (status, body) = send(
        &app,
        &owner,
        Method::PATCH,
        &format!("{}/{p1}", catalog(&org, &domain)),
        Some(&json!({"name": "priority:high"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        issue_labels(&app, &owner, &org, 1).await,
        vec!["bug", "priority:high"]
    );
    assert_eq!(
        search_keys(&app, &owner, &org, "label:priority:high").await,
        vec!["ACME-1"]
    );
    assert!(label_events(&app, &org)
        .await
        .contains(&("labeled".to_string(), "priority:p1".to_string())));

    // Deleting a catalog label removes it from every issue, recording the
    // removal under its name then.
    let bug = id_of("bug");
    let (status, body) = send(
        &app,
        &owner,
        Method::DELETE,
        &format!("{}/{bug}", catalog(&org, &domain)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        issue_labels(&app, &owner, &org, 1).await,
        vec!["priority:high"]
    );
    assert_eq!(
        label_events(&app, &org).await.last().cloned(),
        Some(("unlabeled".to_string(), "bug".to_string()))
    );
    let assignments: i64 =
        sqlx::query_scalar("SELECT count(*) FROM native_issue_label_assignments WHERE org_id = $1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(assignments, 1);

    // Activity shows label events as GitHub names them.
    let workspace = insert_workspace(&app, org.id, owner.id, "labels").await;
    let (status, body) = post(
        &app,
        &owner,
        &format!("/api/v1/workspaces/{workspace}/issue-links"),
        &json!({"ref": "ACME-1", "role": "primary"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, page) = get(
        &app,
        &owner,
        &format!("/api/v1/workspaces/{workspace}/activity"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let shown: Vec<(String, String)> = page["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["source"] == "g5" && entry["kind"] == "issue_event")
        .map(|entry| {
            (
                entry["event"].as_str().unwrap().to_string(),
                entry["detail"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    for expected in [("labeled", "priority:p1"), ("unlabeled", "bug")] {
        assert!(
            shown.contains(&(expected.0.to_string(), expected.1.to_string())),
            "{expected:?} in {shown:?}"
        );
    }

    // Export carries the catalog, each issue's labels and the label events.
    let (status, export) = get(
        &app,
        &owner,
        &format!("/api/v1/orgs/{}/issue-domains/{domain}/export", org.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{export}");
    let exported: Vec<&str> = export["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|label| label["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        exported,
        vec!["good first issue", "priority:high", "priority:p0"]
    );
    assert_eq!(export["issues"][0]["labels"], json!(["priority:high"]));
    assert!(export["issues"][0]["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["event"] == "unlabeled" && event["label"] == "bug"));

    app.cleanup().await;
}

/// Labels stay in their organization and domain; viewers read the catalog
/// and labels but change neither.
#[tokio::test]
async fn g5_labels_stay_in_their_organization_and_viewers_only_read() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let (other_owner, other_org, other_domain) = native_org(&app, "BETA").await;
    let mine = create_label(&app, &owner, &org, &domain, "secret", None).await;
    create_label(
        &app,
        &other_owner,
        &other_org,
        &other_domain,
        "theirs",
        None,
    )
    .await;
    let (status, body) = create_issue(
        &app,
        &owner,
        &org,
        &domain,
        json!({"title": "Plans", "labels": ["secret"], "idempotencyKey": "k"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // Another organization's member can neither read nor change them, even
    // naming this organization's domain and label.
    let (status, _) = get(&app, &other_owner, &catalog(&org, &domain)).await;
    assert!(
        matches!(status, StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "{status}"
    );
    let (status, _) = post(
        &app,
        &other_owner,
        &catalog(&org, &domain),
        &json!({"name": "x"}),
    )
    .await;
    assert!(
        matches!(status, StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "{status}"
    );
    let (status, _) = send(
        &app,
        &other_owner,
        Method::DELETE,
        &format!(
            "{}/{}",
            catalog(&org, &domain),
            mine["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert!(
        matches!(status, StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "{status}"
    );
    // Its own domain does not know this organization's label.
    let (status, body) = send(
        &app,
        &other_owner,
        Method::DELETE,
        &format!(
            "{}/{}",
            catalog(&other_org, &other_domain),
            mine["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = change_labels(&app, &other_owner, &other_org, 1, &["secret"], &[]).await;
    assert!(status.is_client_error(), "{body}");
    // The database refuses an assignment across organizations or domains.
    let issue_id: i64 = sqlx::query_scalar("SELECT id FROM native_issues WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let their_label: i64 =
        sqlx::query_scalar("SELECT id FROM native_issue_labels WHERE org_id = $1")
            .bind(other_org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let crossed = sqlx::query(
        "INSERT INTO native_issue_label_assignments (org_id, domain_id, issue_id, label_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(org.id)
    .bind(Uuid::parse_str(&domain).unwrap())
    .bind(issue_id)
    .bind(their_label)
    .execute(&app.db)
    .await;
    assert!(
        crossed.is_err(),
        "a cross-organization assignment must fail"
    );
    for table in ["native_issue_labels", "native_issue_label_assignments"] {
        let forced: bool =
            sqlx::query_scalar("SELECT relforcerowsecurity FROM pg_class WHERE relname = $1")
                .bind(table)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(forced, "{table}");
    }

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
    let (status, page) = get(&app, &viewer, &catalog(&org, &domain)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(label_names(&page["data"]), vec!["secret"]);
    assert_eq!(issue_labels(&app, &viewer, &org, 1).await, vec!["secret"]);
    let (status, body) = post(
        &app,
        &viewer,
        &catalog(&org, &domain),
        &json!({"name": "v"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = send(
        &app,
        &viewer,
        Method::DELETE,
        &format!(
            "{}/{}",
            catalog(&org, &domain),
            mine["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = change_labels(&app, &viewer, &org, 1, &[], &["secret"]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(issue_labels(&app, &owner, &org, 1).await, vec!["secret"]);

    app.cleanup().await;
}

/// The workspace agent manages the catalog and tags any readable issue as
/// its owner (`g5d issue label`, `g5d issue edit --add-label`,
/// `g5d issue create --label`); a review copy changes nothing.
#[tokio::test]
async fn the_workspace_agent_keeps_labels_as_its_owner() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let workspace = insert_workspace(&app, org.id, owner.id, "agent-labels").await;
    let token = agent_token(&app, org.id, workspace, owner.id).await;
    let route = format!("/api/internal/workspace-agent/{workspace}");
    let internal = format!("/api/internal/workspaces/{workspace}");

    // `g5d issue label create` in the creation domain, and by key; a retry
    // returns the label.
    for _ in 0..2 {
        let (status, body) = agent(
            &app,
            &token,
            Method::POST,
            &format!("{route}/issue-labels"),
            Some(&json!({"name": "priority:p2", "color": "fbca04"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["outcome"], "applied");
        assert_eq!(body["data"]["label"]["name"], "priority:p2");
    }
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-labels"),
        Some(&json!({"domain": "acme", "name": "source:formal-verification"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-labels"),
        Some(&json!({"domain": "acme/web", "name": "x"})),
    )
    .await;
    assert_code(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown_domain",
    );
    let (status, page) = agent(
        &app,
        &token,
        Method::GET,
        &format!("{route}/issue-labels?domain=ACME"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        label_names(&page["data"]),
        vec!["priority:p2", "source:formal-verification"]
    );

    // `g5d issue create --label`, then `g5d issue edit --add-label
    // --remove-label` with a title, then labels alone.
    let (status, created) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{internal}/issues"),
        Some(&json!({"title": "Proof gap", "labels": ["source:formal-verification"], "idempotencyKey": "a1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(
        created["data"]["labels"],
        json!(["source:formal-verification"])
    );
    let (status, edited) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-edit"),
        Some(
            &json!({"ref": "ACME-1", "title": "Proof gap in merge", "addLabels": ["priority:p2"]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["outcome"], "applied", "{edited}");
    assert_eq!(edited["data"]["issue"]["title"], "Proof gap in merge");
    assert_eq!(edited["data"]["labels"]["outcome"], "applied", "{edited}");
    assert_eq!(
        label_names(&edited["data"]["issue"]),
        vec!["priority:p2", "source:formal-verification"]
    );
    let (status, edited) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-edit"),
        Some(&json!({"ref": "1", "removeLabels": ["PRIORITY:P2"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["outcome"], "applied", "{edited}");
    assert_eq!(
        label_names(&edited["data"]["issue"]),
        vec!["source:formal-verification"]
    );
    // An unknown label refuses the whole edit before anything changes.
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-edit"),
        Some(&json!({"ref": "1", "title": "Never", "addLabels": ["nope"]})),
    )
    .await;
    assert_code(
        status,
        &body,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown_label",
    );
    let (_, issue) = get(
        &app,
        &owner,
        &format!("/api/v1/orgs/{}/issues/g5/1", org.id),
    )
    .await;
    assert_eq!(issue["data"]["title"], "Proof gap in merge");
    // So does a field the edit cannot make: fields and labels change in one
    // transaction, so the labels stay as they were.
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-edit"),
        Some(&json!({"ref": "1", "addLabels": ["priority:p2"], "addAssignees": ["stranger@example.com"]})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        issue_labels(&app, &owner, &org, 1).await,
        vec!["source:formal-verification"]
    );
    // A field edit and a label change together apply together.
    let (status, edited) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-edit"),
        Some(&json!({"ref": "1", "title": "Proof gap, triaged", "addLabels": ["priority:p2"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["data"]["outcome"], "applied", "{edited}");
    assert_eq!(edited["data"]["issue"]["title"], "Proof gap, triaged");
    assert_eq!(edited["data"]["labels"]["outcome"], "applied", "{edited}");
    assert_eq!(
        label_names(&edited["data"]["issue"]),
        vec!["priority:p2", "source:formal-verification"]
    );
    let (status, _) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-edit"),
        Some(&json!({"ref": "1", "removeLabels": ["priority:p2"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Events are the owner's.
    let actors: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT actor_user_id FROM native_issue_events WHERE org_id = $1",
    )
    .bind(org.id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(actors, vec![owner.id]);

    // `g5d issue label edit` and `delete` name the label.
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-labels/edit"),
        Some(&json!({"label": "Priority:P2", "name": "priority:p3"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["label"]["name"], "priority:p3");
    let (status, body) = agent(
        &app,
        &token,
        Method::POST,
        &format!("{route}/issue-labels/delete"),
        Some(&json!({"label": "source:formal-verification"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(issue_labels(&app, &owner, &org, 1).await.is_empty());

    // A review copy's agent changes no label.
    let review_copy = insert_workspace(&app, org.id, owner.id, "agent-labels-review").await;
    let repo_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) \
         VALUES ($1, $2, 'labels', 'labels', 'file:///unused', 'main', 'github')",
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
    .bind("review-workspace:agent-labels:1")
    .bind("d".repeat(64))
    .bind("agent-labels-review")
    .bind(json!([{"repo_id": repo_id}]))
    .execute(&app.db)
    .await
    .unwrap();
    let review_token = agent_token(&app, org.id, review_copy, owner.id).await;
    let review_route = format!("/api/internal/workspace-agent/{review_copy}");
    for (leaf, request) in [
        (
            "issue-edit",
            json!({"ref": "ACME-1", "addLabels": ["priority:p3"]}),
        ),
        ("issue-labels", json!({"name": "from-review"})),
        ("issue-labels/delete", json!({"label": "priority:p3"})),
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
    let (_, page) = get(&app, &owner, &catalog(&org, &domain)).await;
    assert_eq!(label_names(&page["data"]), vec!["priority:p3"]);
    assert!(issue_labels(&app, &owner, &org, 1).await.is_empty());

    app.cleanup().await;
}

/// Concurrent writers: additions and removals of different labels on one
/// issue all survive, and deleting a label while issues are being tagged
/// with it leaves no assignment behind and fails nothing with a server
/// error (`g5.issue.labels.native`).
#[tokio::test]
async fn racing_label_changes_and_deletions_keep_every_other_change() {
    let app = TestApp::spawn().await;
    let (owner, org, domain) = native_org(&app, "ACME").await;
    let names: Vec<String> = (0..8).map(|n| format!("l{n}")).collect();
    for name in &names {
        create_label(&app, &owner, &org, &domain, name, None).await;
    }
    for n in 0..4 {
        let (status, body) = create_issue(
            &app,
            &owner,
            &org,
            &domain,
            json!({"title": format!("Issue {n}"), "labels": ["l7"], "idempotencyKey": format!("k{n}")}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    // Eight writers add a different label each to issue 1 while another
    // removes l7: every change lands.
    let singles: Vec<[&str; 1]> = names.iter().take(7).map(|name| [name.as_str()]).collect();
    let writers: Vec<_> = singles
        .iter()
        .map(|single| change_labels(&app, &owner, &org, 1, single, &[]))
        .collect();
    let results = futures::future::join_all(writers).await;
    let (status, body) = change_labels(&app, &owner, &org, 1, &[], &["l7"]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for (status, body) in results {
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    assert_eq!(
        issue_labels(&app, &owner, &org, 1).await,
        names[..7].to_vec()
    );

    // Delete l7 while tagging issues with it and with others.
    let (_, page) = get(&app, &owner, &catalog(&org, &domain)).await;
    let l7 = page["data"]["labels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|label| label["name"] == "l7")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let delete_path = format!("{}/{l7}", catalog(&org, &domain));
    let deletion = send(&app, &owner, Method::DELETE, &delete_path, None);
    let tagging = futures::future::join_all(
        (1..=4).map(|number| change_labels(&app, &owner, &org, number, &["l7", "l0"], &[])),
    );
    let ((status, body), tagged) = tokio::join!(deletion, tagging);
    assert_eq!(status, StatusCode::OK, "{body}");
    for (status, body) in tagged {
        // Either the change came first (and the deletion then removed l7),
        // or the label was already gone and the change refused whole.
        assert!(
            status == StatusCode::OK
                || (status == StatusCode::UNPROCESSABLE_ENTITY
                    && body["error"]["details"]["code"] == "unknown_label"),
            "{status}: {body}"
        );
    }
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM native_issue_label_assignments a \
         JOIN native_issue_labels l ON l.id = a.label_id WHERE a.org_id = $1 AND l.name = 'l7'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(left, 0);
    for number in 1..=4 {
        assert!(!issue_labels(&app, &owner, &org, number)
            .await
            .contains(&"l7".to_string()));
    }

    app.cleanup().await;
}
