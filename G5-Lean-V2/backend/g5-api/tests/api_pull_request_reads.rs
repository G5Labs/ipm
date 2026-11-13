//! PR reads exercise real public/internal routes and restricted tenant DB roles.
use crate::helpers;
use helpers::{test_config, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{
    body_json, body_partial_json, body_string_contains, header, method, path,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn get(app: &TestApp, token: &str, path: &str) -> (StatusCode, Value) {
    let response = app
        .client
        .get(format!("{}{path}", app.base_url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}
fn route(w: Uuid) -> String {
    format!("/api/v1/workspaces/{w}/pull-requests/info")
}
async fn fixture(app: &TestApp) -> (TestUser, TestOrg, TestWorkspace) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let workspace = TestWorkspace::create(app, &org).await;
    (owner, org, workspace)
}
async fn bind_github(app: &TestApp, owner: &TestUser, org: &TestOrg, workspace: &TestWorkspace) {
    sqlx::query("UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github',provider_full_name='untrusted/elsewhere' WHERE id=$1")
        .bind(workspace.repo_id).execute(&app.db).await.unwrap();
    let identity = Uuid::new_v4();
    let binding = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO github_app_identities(id,org_id,kind) VALUES($1,$2,'platform_managed')",
    )
    .bind(identity)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO github_app_installation_bindings(id,org_id,app_identity_id,version,disposition,installation_id,account_id,account_login,permissions,provenance,created_by) VALUES($1,$2,$3,1,'active',555,44,'acme',$4,'coverage_change',$5)")
        .bind(binding).bind(org.id).bind(identity).bind(json!({"pull_requests":"read","contents":"read"})).bind(owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO github_app_installation_binding_repositories(org_id,binding_id,github_repo_id,full_name) VALUES($1,$2,1002,'acme/api')")
        .bind(org.id).bind(binding).execute(&app.db).await.unwrap();
}
pub(crate) async fn github_app(github: &MockServer) -> TestApp {
    let mut config = test_config();
    let mut app_config = helpers::github_app::github_app_config(github);
    // These fixtures model public-GitHub browser links with a local API stub.
    app_config.web_base_url = "https://github.com".into();
    config.github_app = Some(app_config);
    config.auth.scm_github_api_base_url = github.uri();
    TestApp::spawn_with_config(config).await
}
async fn mount_token(github: &MockServer) {
    Mock::given(method("POST")).and(path("/app/installations/555/access_tokens"))
        .and(body_json(json!({"repository_ids":[1002],"permissions":{"pull_requests":"read","contents":"read"}})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"installation-read-only","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
        .expect(1).mount(github).await;
}

#[tokio::test]
async fn github_lookup_uses_attached_identity_and_narrow_read_credentials_without_mutation() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    bind_github(&app, &owner, &org, &w).await;
    mount_token(&github).await;
    Mock::given(method("GET")).and(path("/repos/acme/api/pulls/42"))
        .and(header("authorization","Bearer installation-read-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":123456,"node_id":"PR_node42","number":42,"title":"Fix it","body":"Details","state":"closed","merged":true,"user":{"login":"author"},"head":{"ref":"feature","sha":"abc"},"base":{"ref":"main","sha":"def"}})))
        .expect(4).mount(&github).await;
    Mock::given(method("GET")).and(path("/repos/acme/api/issues/42/comments"))
        .and(header("authorization","Bearer installation-read-only"))
        .respond_with(ResponseTemplate::new(200).insert_header("Link","<https://api.github.com/repos/acme/api/issues/42/comments?page=2>; rel=\"next\"")
            .set_body_json(json!([{"id":1,"user":{"login":"reviewer"},"body":"Please test","created_at":"2026-09-30"}])))
        .expect(1).mount(&github).await;
    // Inline review threads and reviews come from GraphQL, which alone reports resolution.
    Mock::given(method("POST")).and(path("/graphql"))
        .and(header("authorization","Bearer installation-read-only"))
        .and(body_partial_json(json!({"variables":{"id":"PR_node42"}})))
        .and(body_string_contains("reviewThreads("))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"node":{"reviewThreads":{
            "pageInfo":{"hasNextPage":false,"endCursor":null},
            "nodes":[{"path":"src/lib.rs","line":3,"isResolved":true,"isOutdated":false,"resolvedBy":{"login":"author"},
                "comments":{"totalCount":1,"nodes":[{"databaseId":7,"author":{"login":"coderabbitai"},"body":"Major: bounds"}]}}]}}}})))
        .expect(1).mount(&github).await;
    Mock::given(method("POST")).and(path("/graphql"))
        .and(header("authorization","Bearer installation-read-only"))
        .and(body_string_contains("reviews("))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"node":{"reviews":{
            "pageInfo":{"hasNextPage":false,"endCursor":null},
            "nodes":[{"databaseId":8,"author":{"login":"claude"},"state":"CHANGES_REQUESTED","body":"See threads"}]}}}})))
        .expect(1).mount(&github).await;
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(w) FROM workspaces w WHERE id=$1")
        .bind(w.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?selector=42&comments=true", route(w.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["source"], "github");
    let p = &body["data"]["pullRequest"];
    assert_eq!(p["state"], "MERGED");
    assert_eq!(p["id"], "PR_node42");
    assert_eq!(p["headRefOid"], "abc");
    assert_eq!(p["comments"][0]["body"], "Please test");
    assert_eq!(p["commentsHasMore"], true);
    assert_eq!(p["reviewThreads"][0]["path"], "src/lib.rs");
    assert_eq!(p["reviewThreads"][0]["isResolved"], true);
    assert_eq!(
        p["reviewThreads"][0]["comments"][0]["body"],
        "Major: bounds"
    );
    assert_eq!(p["reviewThreadsHasMore"], false);
    assert_eq!(p["reviews"][0]["state"], "CHANGES_REQUESTED");
    assert!(p.get("reviewDecision").is_none());
    assert!(!body.to_string().contains("installation-read-only"));
    // The review summary is a separate, comment-free read.
    Mock::given(method("POST")).and(path("/graphql"))
        .and(header("authorization","Bearer installation-read-only"))
        .and(body_string_contains("latestReviews("))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"node":{
            "reviewDecision":"APPROVED","latestReviews":{"pageInfo":{"hasNextPage":false},
            "nodes":[{"id":"PRR_8","author":{"login":"claude"},"state":"APPROVED","body":"","commit":{"oid":"abc"}}]}}}})))
        .expect(1).mount(&github).await;
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?selector=42&reviews=true", route(w.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let p = &body["data"]["pullRequest"];
    assert_eq!(p["reviewDecision"], "APPROVED");
    assert_eq!(p["latestReviews"][0]["commit"]["oid"], "abc");
    assert!(p.get("comments").is_none());
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!(
            "{}?selector=https://github.com/acme/api/pull/42",
            route(w.id)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["pullRequest"].get("comments").is_none());
    assert_eq!(body["data"]["pullRequest"]["id"], "PR_node42");
    let vm_token = mint_vm(&app, org.id, w.id, owner.id).await;
    let (status, body) = get(
        &app,
        &vm_token,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/info?selector=42",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["pullRequest"]["number"], 42);
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(w) FROM workspaces w WHERE id=$1")
        .bind(w.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(before, after);
    let prs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_pull_requests WHERE org_id=$1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(prs, 0);
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn lookup_rejects_ambiguous_unattached_urls_and_redacts_provider_failures() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    bind_github(&app, &owner, &org, &w).await;
    for query in [
        "selector=https://evil.test/acme/api/pull/1",
        "selector=42&repo=other/repo",
        "selector=0",
        "repo=acme/api",
        "comments=true",
        "reviews=true",
        "selector=https://github.com/acme/api/pull/1&repo=other/repo",
    ] {
        let (status, body) = get(&app, &owner.jwt, &format!("{}?{query}", route(w.id))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
    }
    let sibling = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE repos SET git_url='https://github.com/acme/other.git',provider='github' WHERE id=$1").bind(sibling.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT $1,$2,$3,'other',role FROM workspace_repos WHERE workspace_id=$4 AND repo_id=$3")
        .bind(org.id).bind(w.id).bind(sibling.repo_id).bind(sibling.id).execute(&app.db).await.unwrap();
    let (status, body) = get(&app, &owner.jwt, &format!("{}?selector=42", route(w.id))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(github.received_requests().await.unwrap().is_empty());
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?selector=42&repo=acme/other", route(w.id)),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "uncovered attached repository: {body}"
    );
    assert!(github.received_requests().await.unwrap().is_empty());
    mount_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(403).set_body_string("installation-read-only secret"))
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?selector=42&repo=acme/api", route(w.id)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("403"));
    assert!(!body.to_string().contains("secret"));
    // A redirect to another origin never receives the token.
    let other = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/43"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/unchecked", other.uri())),
        )
        .mount(&github)
        .await;
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?selector=43&repo=acme/api", route(w.id)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(other.received_requests().await.unwrap().is_empty());
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn stored_reads_enforce_org_and_current_workspace_incarnation() {
    let app = TestApp::spawn().await;
    let (owner, org, w) = fixture(&app).await;
    let other = TestWorkspace::create(&app, &org).await;
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let (status, body) = get(&app, &reader.jwt, &route(w.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"], json!({"source":"g5","pullRequests":[]}));
    let stored_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,title,local_merge_status,last_preflight_fingerprint) VALUES($1,$2,$3,$4,'Stored PR','ready','test')")
        .bind(stored_id).bind(org.id).bind(w.id).bind(other.id).execute(&app.db).await.unwrap();
    let (status, body) = get(&app, &reader.jwt, &route(w.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["pullRequests"][0]["id"], stored_id.to_string());
    let (status, body) = get(&app, &owner.jwt, &route(other.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequests"][0]["id"],
        stored_id.to_string(),
        "incoming PR read"
    );
    // Ordinary readership remains available without a configurable grant.
    let (status, body) = get(&app, &reader.jwt, &route(w.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let outsider = TestUser::create(&app).await;
    let (status, body) = get(&app, &outsider.jwt, &route(w.id)).await;
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "{body}"
    );
    let token = mint_vm(&app, org.id, w.id, owner.id).await;
    let internal = |id| format!("/api/internal/workspaces/{id}/pull-requests/info");
    let (status, body) = get(&app, &token, &internal(w.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = get(&app, &token, &internal(other.id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = get(&app, &owner.jwt, &internal(w.id)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a user JWT is not a VM token: {body}"
    );
    sqlx::query("UPDATE workspace_vms SET epoch=2 WHERE workspace_id=$1")
        .bind(w.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = get(&app, &token, &internal(w.id)).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "replaced incarnation: {body}"
    );
    sqlx::query("UPDATE workspace_vms SET epoch=1 WHERE workspace_id=$1")
        .bind(w.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at=now()-interval '1 minute' WHERE workspace_id=$1",
    )
    .bind(w.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = get(&app, &token, &internal(w.id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    app.cleanup().await;
}

/// A lease that runs out while the incarnation check waits for the VM row is
/// refused. PostgreSQL rechecks a waited-for row only when it changed, and a
/// lock held without a change does not change it, so the reader checks the
/// rule again once it holds the rows.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lease_that_expires_during_the_row_lock_wait_is_refused() {
    let app = TestApp::spawn().await;
    let (owner, org, w) = fixture(&app).await;
    let token = mint_vm(&app, org.id, w.id, owner.id).await;
    let internal = format!("/api/internal/workspaces/{}/pull-requests/info", w.id);
    let (status, body) = get(&app, &token, &internal).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at = clock_timestamp() + interval '3 seconds'
         WHERE workspace_id = $1",
    )
    .bind(w.id)
    .execute(&app.db)
    .await
    .unwrap();
    let mut holder = app.db.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
        .bind(w.id)
        .execute(&mut *holder)
        .await
        .unwrap();
    let release = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    // The reader's statement; pg_stat_activity cuts long
                    // query text, so match its start.
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'
                       AND query LIKE 'SELECT w.org_id, w.owner_user_id%')",
                )
                .fetch_one(&app.db)
                .await
                .unwrap();
                let expired: bool = sqlx::query_scalar(
                    "SELECT lease_expires_at <= clock_timestamp() FROM workspace_vms
                     WHERE workspace_id = $1",
                )
                .bind(w.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
                if waiting && expired {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the read waits for the VM row until its lease ends");
        holder.rollback().await.unwrap();
    };
    let ((status, body), ()) = tokio::join!(get(&app, &token, &internal), release);
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn non_app_lookup_uses_the_readers_personal_connection() {
    let github = MockServer::start().await;
    let mut config = test_config();
    config.auth.scm_github_api_base_url = github.uri();
    let app = TestApp::spawn_with_config(config).await;
    let (_owner, org, w) = fixture(&app).await;
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    sqlx::query(
        "UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github' WHERE id=$1",
    )
    .bind(w.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token("reader-personal-token")
        .unwrap();
    sqlx::query("INSERT INTO user_scm_connections(id,user_id,provider,provider_user_id,provider_username,access_token_enc,scopes) VALUES($1,$2,'github','reader','reader',$3,ARRAY['repo'])")
        .bind(Uuid::new_v4()).bind(reader.id).bind(encrypted).execute(&app.db).await.unwrap();
    Mock::given(method("GET")).and(path("/repos/acme/api/pulls/7"))
        .and(header("authorization","Bearer reader-personal-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":7,"title":"Reader PR","state":"open","head":{"ref":"feature"},"base":{"ref":"main"}})))
        .expect(1).mount(&github).await;
    let (status, body) = get(&app, &reader.jwt, &format!("{}?selector=7", route(w.id))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["pullRequest"]["mergeable"], Value::Null);
    github.verify().await;
    app.cleanup().await;
}

pub(crate) async fn mint_vm(app: &TestApp, org: Uuid, workspace: Uuid, owner: Uuid) -> String {
    let host = Uuid::new_v4();
    let vm = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat) VALUES($1,$2,'us-central1','ready',now())")
        .bind(host).bind(org).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at) VALUES($1,$2,$3,$4,1,'running',now()+interval '1 hour')")
        .bind(org).bind(workspace).bind(host).bind(vm.to_string()).execute(&app.db).await.unwrap();
    app.state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org, workspace, owner, vm, 1, 3600)
        .unwrap()
}

pub(crate) const CHECK_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(crate) const NEXT_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

pub(crate) async fn checks_binding(
    app: &TestApp,
    github: &MockServer,
    owner: &TestUser,
    org: &TestOrg,
    w: &TestWorkspace,
) {
    bind_github(app, owner, org, w).await;
    sqlx::query("UPDATE github_app_installation_bindings SET permissions=$1 WHERE org_id=$2")
        .bind(json!({"pull_requests":"read","contents":"read","checks":"read","statuses":"read"}))
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    mount_checks_token(github).await;
}
/// The installation token the checks binding mints, e.g. again after a reset.
pub(crate) async fn mount_checks_token(github: &MockServer) {
    Mock::given(method("POST")).and(path("/app/installations/555/access_tokens"))
        .and(body_json(json!({"repository_ids":[1002],"permissions":{"pull_requests":"read","contents":"read","checks":"read","statuses":"read"}})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"checks-only-token","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
        .mount(github).await;
}
pub(crate) async fn check_head(github: &MockServer, sha: &str) {
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .and(header("authorization", "Bearer checks-only-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"number":42,"title":"Checked PR","state":"open","mergeable":false,"base":{"ref":"main","sha":NEXT_SHA},"head":{"sha":sha}})),
        )
        .mount(github)
        .await;
}
async fn check_rows(github: &MockServer, runs: Value, statuses: Value) {
    revision_rows(github, CHECK_SHA, runs, statuses).await;
}
/// The base branch's classic protection and rulesets; tests override the
/// low-priority default (nothing required) with their own requirements.
pub(crate) async fn branch_requirements(
    github: &MockServer,
    branch: &str,
    protection: Value,
    rules: Value,
    priority: u8,
) {
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/api/branches/{branch}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"name":branch,"protection":protection})),
        )
        .with_priority(priority)
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/api/rules/branches/{branch}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(rules))
        .with_priority(priority)
        .mount(github)
        .await;
}
pub(crate) async fn revision_rows(
    github: &MockServer,
    sha: &str,
    mut runs: Value,
    statuses: Value,
) {
    branch_requirements(github, "main", json!({"enabled":false}), json!([]), 10).await;
    let mut suites = std::collections::BTreeMap::new();
    for run in runs.as_array_mut().unwrap() {
        if run["check_suite"]["id"].is_null() {
            run["check_suite"] = json!({"id":9});
        }
        let id = run["check_suite"]["id"].as_u64().unwrap();
        suites.insert(id, json!({"id":id,"head_sha":sha,"status":"completed","conclusion":"success","app":{"id":id,"name":format!("App {id}")}}));
    }
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/api/commits/{sha}/check-suites")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"check_suites":suites.into_values().collect::<Vec<_>>()})),
        )
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/api/commits/{sha}/check-runs")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"check_runs":runs})))
        .mount(github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/api/commits/{sha}/statuses")))
        .respond_with(ResponseTemplate::new(200).set_body_json(statuses))
        .mount(github)
        .await;
}

#[tokio::test]
async fn checks_include_current_runs_statuses_and_redacted_diagnostics_for_user_and_vm() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token(
            &json!({"BUILD_SECRET":"configured-build-secret","PASS":"pass","READY":"ready"})
                .to_string(),
        )
        .unwrap();
    sqlx::query("INSERT INTO workspace_configs(org_id,workspace_id,secrets_enc,secret_keys) VALUES($1,$2,$3,ARRAY['BUILD_SECRET']) ON CONFLICT(workspace_id) DO UPDATE SET secrets_enc=EXCLUDED.secrets_enc")
        .bind(org.id).bind(w.id).bind(encrypted).execute(&app.db).await.unwrap();
    check_head(&github, CHECK_SHA).await;
    check_rows(&github,json!([{"id":7,"name":"Cloud Build","head_sha":CHECK_SHA,"status":"completed","conclusion":"failure","details_url":"https://console.cloud.google.com/build/7","output":{"summary":"compile failed configured-build-secret checks-only-token","text":"error[E0308]","annotations_count":1}}]),json!([
        {"id":12,"context":"review","state":"success","description":"latest pass ready"},
        {"id":11,"context":"REVIEW","state":"failure","description":"old"}
    ])).await;
    Mock::given(method("GET")).and(path("/repos/acme/api/check-runs/7/annotations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"path":"src/main.rs","start_line":12,"end_line":12,"annotation_level":"failure","message":"type mismatch"}]))).expect(2).mount(&github).await;
    let vm = mint_vm(&app, org.id, w.id, owner.id).await;
    for (token, endpoint) in [
        (&owner.jwt, route(w.id)),
        (
            &vm,
            format!("/api/internal/workspaces/{}/pull-requests/info", w.id),
        ),
    ] {
        let (status, body) = get(
            &app,
            token,
            &format!("{endpoint}?checks=true&selector=42&details=true&head={CHECK_SHA}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let repo = &body["data"]["repositories"][0];
        assert_eq!(repo["availability"], "ready", "{body}");
        assert_eq!(repo["headRefOid"], CHECK_SHA);
        let checks = repo["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0]["bucket"], "fail");
        assert_eq!(checks[0]["details"]["annotations"][0]["startLine"], 12);
        assert_eq!(checks[1]["bucket"], "pass");
        assert_eq!(checks[1]["description"], "latest [redacted] [redacted]");
        assert!(!body.to_string().contains("configured-build-secret"));
        assert!(!body.to_string().contains("checks-only-token"));
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_pull_requests WHERE org_id=$1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 0, "check reads cannot create PRs");
    assert!(github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method.as_str() == "GET"
            || r.url.path() == "/app/installations/555/access_tokens"));
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn check_reads_never_report_old_head_success_or_follow_redirects() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    check_head(&github, CHECK_SHA).await;
    let (_, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&selector=42&head={NEXT_SHA}", route(w.id)),
    )
    .await;
    assert_eq!(
        body["data"]["repositories"][0]["availability"], "pending",
        "{body}"
    );
    assert!(github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| !r.url.path().contains("/commits/")));
    github.reset().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET")).and(path("/repos/acme/api/pulls/42"))
        .respond_with(move |_: &wiremock::Request| ResponseTemplate::new(200).set_body_json(json!({"number":42,"state":"open","mergeable":false,"base":{"ref":"main","sha":NEXT_SHA},"head":{"sha":if calls.fetch_add(1,Ordering::SeqCst)==0 {CHECK_SHA} else {NEXT_SHA}}})))
        .mount(&github).await;
    check_rows(&github,json!([{"id":1,"name":"tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"success"}]),json!([])).await;
    let (_, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&selector=42", route(w.id)),
    )
    .await;
    let repo = &body["data"]["repositories"][0];
    assert_eq!(repo["availability"], "pending", "{body}");
    assert_eq!(repo["headRefOid"], NEXT_SHA);
    assert_eq!(repo["checks"], json!([]));
    // A cross-origin redirect is not followed: the other origin sees nothing.
    let other = MockServer::start().await;
    github.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/leak-token", other.uri())),
        )
        .mount(&github)
        .await;
    let (_, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&selector=42", route(w.id)),
    )
    .await;
    assert_eq!(
        body["data"]["repositories"][0]["availability"],
        "unavailable"
    );
    assert_eq!(github.received_requests().await.unwrap().len(), 1);
    assert!(other.received_requests().await.unwrap().is_empty());
    // A same-origin redirect (a moved repository) is followed, and what it
    // returns is still validated: here it is not a pull request.
    github.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/moved", github.uri())),
        )
        .mount(&github)
        .await;
    let (_, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&selector=42", route(w.id)),
    )
    .await;
    assert_eq!(
        body["data"]["repositories"][0]["availability"],
        "unavailable"
    );
    assert_eq!(github.received_requests().await.unwrap().len(), 2);
    app.cleanup().await;
}

#[tokio::test]
async fn checks_enforce_repository_permissions_and_current_workspace_authority() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    bind_github(&app, &owner, &org, &w).await;
    let outsider = TestUser::create(&app).await;
    let url = format!("{}?checks=true&selector=42", route(w.id));
    let (status, _) = get(&app, &outsider.jwt, &url).await;
    assert!(matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    let (status, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["repositories"][0]["availability"],
        "unavailable"
    );
    assert!(body.to_string().contains("Checks and Commit statuses"));
    for options in [
        "checks=true&selector=42&repo=other/private",
        "checks=true&head=short",
        "checks=true&comments=true",
        "checks=true&reviews=true",
        "checks=true&selector=https://evil.test/acme/api/pull/42",
    ] {
        let (status, body) = get(&app, &owner.jwt, &format!("{}?{options}", route(w.id))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    let vm = mint_vm(&app, org.id, w.id, owner.id).await;
    sqlx::query("UPDATE workspace_vms SET epoch=epoch+1 WHERE workspace_id=$1")
        .bind(w.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, _) = get(
        &app,
        &vm,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/info?checks=true&selector=42",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(github.received_requests().await.unwrap().is_empty());
    app.cleanup().await;
}

#[tokio::test]
async fn checks_bound_enumeration_and_diagnostics_without_claiming_empty_success() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    check_head(&github, CHECK_SHA).await;
    check_rows(&github, json!([]), json!([])).await;
    let url = format!("{}?checks=true&selector=42&details=true", route(w.id));
    let (_, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(body["data"]["repositories"][0]["checks"], json!([]));
    github.reset().await;
    check_head(&github, CHECK_SHA).await;
    check_rows(&github,json!([{"id":7,"name":"tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"failure","output":{"summary":"x".repeat(40_000),"annotations_count":101}}]),json!([])).await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/check-runs/7/annotations"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<https://evil.test>; rel=\"next\"")
                .set_body_json(json!([])),
        )
        .expect(1)
        .mount(&github)
        .await;
    let (_, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(
        body["data"]["repositories"][0]["checks"][0]["details"]["truncated"], true,
        "{body}"
    );
    // `reset` discards expectations unverified; check the single annotation read.
    github.verify().await;
    github.reset().await;
    check_head(&github, CHECK_SHA).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/acme/api/commits/{CHECK_SHA}/check-suites"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<https://evil.test>; rel=\"next\"")
                .set_body_json(json!({"check_suites":[]})),
        )
        .expect(10)
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/acme/api/commits/{CHECK_SHA}/statuses"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&github)
        .await;
    let (_, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(
        body["data"]["repositories"][0]["availability"], "unavailable",
        "{body}"
    );
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn stored_pr_checks_default_to_outgoing_and_preserve_all_repository_outcomes() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    check_head(&github, CHECK_SHA).await;
    check_rows(&github,json!([{"id":1,"name":"tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"success"}]),json!([])).await;
    let target = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE repos SET git_url='https://github.com/acme/other.git',provider='github' WHERE id=$1").bind(other.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT $1,$2,$3,'other',role FROM workspace_repos WHERE workspace_id=$4 AND repo_id=$3")
        .bind(org.id).bind(w.id).bind(other.repo_id).bind(other.id).execute(&app.db).await.unwrap();
    let pr = Uuid::new_v4();
    let preflight = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,"decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"checks-fixture","repoResults":[],"message":"ready"});
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,title,local_merge_status,last_preflight_fingerprint,preflight) VALUES($1,$2,$3,$4,'Outgoing PR','ready','checks-fixture',$5)")
        .bind(pr).bind(org.id).bind(w.id).bind(target.id).bind(preflight).execute(&app.db).await.unwrap();
    for (repo, name) in [(w.repo_id, "acme/api"), (other.repo_id, "acme/other")] {
        sqlx::query("INSERT INTO workspace_pull_request_provider_repos(id,org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_status) VALUES($1,$2,$3,$4,'github',$5,'feature','main',42,'open')")
            .bind(Uuid::new_v4()).bind(org.id).bind(pr).bind(repo).bind(name).execute(&app.db).await.unwrap();
    }
    for selector in [String::new(), format!("&selector={pr}")] {
        let (status, body) = get(
            &app,
            &owner.jwt,
            &format!("{}?checks=true{selector}", route(w.id)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let repos = body["data"]["repositories"].as_array().unwrap();
        assert_eq!(repos.len(), 2, "{body}");
        assert!(repos
            .iter()
            .any(|r| r["repository"] == "acme/api" && r["checks"][0]["bucket"] == "pass"));
        assert!(repos
            .iter()
            .any(|r| r["repository"] == "acme/other" && r["availability"] == "unavailable"));
    }
    let (status, _) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&head={CHECK_SHA}", route(w.id)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&repo=acme/api&head={CHECK_SHA}", route(w.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["repositories"].as_array().unwrap().len(), 1);
    let outsider = TestWorkspace::create(&app, &org).await;
    let (status, _) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&selector={pr}", route(outsider.id)),
    )
    .await;
    // A G5 ID reads any PR the reader can read, from any workspace
    // (`g5.git.pr-reads`); invisibility is covered by
    // `api_pull_request_listing::pr_reads_follow_workspace_read_access_from_any_workspace`.
    assert_eq!(
        status,
        StatusCode::OK,
        "a readable PR is read from any workspace"
    );
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_repo_full_name='acme/other' WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(pr).bind(w.repo_id).execute(&app.db).await.unwrap();
    let (status, body) = get(&app, &owner.jwt, &format!("{}?checks=true", route(w.id))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("identity changed"));
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_repo_full_name='acme/api' WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(pr).bind(w.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO github_app_installation_binding_repositories(org_id,binding_id,github_repo_id,full_name) SELECT org_id,id,1003,'acme/other' FROM github_app_installation_bindings WHERE org_id=$1")
        .bind(org.id).execute(&app.db).await.unwrap();
    github.reset().await;
    Mock::given(method("POST")).and(path("/app/installations/555/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"checks-only-token","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()}))).mount(&github).await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .expect(1)
        .mount(&github)
        .await;
    let (_, body) = get(&app, &owner.jwt, &format!("{}?checks=true", route(w.id))).await;
    let repos = body["data"]["repositories"].as_array().unwrap();
    assert_eq!(repos.len(), 2);
    assert!(
        repos.iter().all(|r| r["availability"] == "unavailable"),
        "{body}"
    );
    assert!(repos.iter().any(|r| r["rateLimited"] == true), "{body}");
    assert!(
        repos
            .iter()
            .any(|r| r["message"].as_str().unwrap_or("").contains("Read stopped")),
        "{body}"
    );
    github.verify().await;
    app.cleanup().await;
}

/// `g5.git.pr-no-github`, `g5.git.pr-repository`: in a PR that expects
/// GitHub, a member not on GitHub is reported beside GitHub's as its local
/// repository PR, with no external checks, instead of being left out.
#[tokio::test]
async fn checks_report_local_repository_pull_requests_beside_githubs() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    check_head(&github, CHECK_SHA).await;
    check_rows(&github,json!([{"id":1,"name":"tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"success"}]),json!([])).await;
    let target = TestWorkspace::create(&app, &org).await;
    // A second member that is not on GitHub.
    let docs = TestWorkspace::create(&app, &org).await;
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT $1,$2,$3,'docs',role FROM workspace_repos WHERE workspace_id=$4 AND repo_id=$3")
        .bind(org.id).bind(w.id).bind(docs.repo_id).bind(docs.id).execute(&app.db).await.unwrap();
    let docs_name: String = sqlx::query_scalar("SELECT name FROM repos WHERE id=$1")
        .bind(docs.repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let heads = |workspace: Uuid, sha: char| {
        json!([
            {"repoId":w.repo_id,"workspaceId":workspace,"branchName":"feature","headCommit":sha.to_string().repeat(40)},
            {"repoId":docs.repo_id,"workspaceId":workspace,"branchName":"feature","headCommit":sha.to_string().repeat(40)},
        ])
    };
    let pr = Uuid::new_v4();
    let preflight = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,"decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"mixed","repoResults":[],"message":"ready"});
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,title,local_merge_status,last_preflight_fingerprint,preflight,source_heads,target_heads) VALUES($1,$2,$3,$4,'Mixed PR','ready','mixed',$5,$6,$7)")
        .bind(pr).bind(org.id).bind(w.id).bind(target.id).bind(preflight).bind(heads(w.id, 'a')).bind(heads(target.id, 'b'))
        .execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_pull_request_provider_repos(id,org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_status) VALUES($1,$2,$3,$4,'github','acme/api','feature','main',42,'open')")
        .bind(Uuid::new_v4()).bind(org.id).bind(pr).bind(w.repo_id).execute(&app.db).await.unwrap();
    let number: i64 = sqlx::query_scalar("SELECT number FROM workspace_pull_requests WHERE id=$1")
        .bind(pr)
        .fetch_one(&app.db)
        .await
        .unwrap();

    let (status, body) = get(&app, &owner.jwt, &format!("{}?checks=true", route(w.id))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let repos = body["data"]["repositories"].as_array().unwrap();
    assert_eq!(repos.len(), 2, "{body}");
    assert_eq!(repos[0]["repository"], "acme/api");
    assert_eq!(repos[0]["checks"][0]["bucket"], "pass");
    assert_eq!(
        repos[1],
        json!({"repository":docs_name,"tracker":"g5","number":null,"g5Number":format!("PR{number:06x}"),"availability":"ready",
               "noExternalChecks":true,"message":"No external checks apply: this repository is not on GitHub.","checks":[]})
    );
    // The merge policy reads the local member as passing.
    assert_eq!(
        g5_api::services::workspace_pull_request_provider_lifecycle::external_checks_verdict(
            &body["data"]
        ),
        g5_api::services::workspace_pull_request_provider_lifecycle::ExternalChecksVerdict::Passing
    );
    // The G5 PR names both repository pull requests.
    let (status, body) = get(&app, &owner.jwt, &format!("{}?selector={pr}", route(w.id))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let bound = &body["data"]["pullRequest"]["repositoryPullRequests"];
    assert_eq!(bound.as_array().unwrap().len(), 2, "{body}");
    assert_eq!(bound[0]["adapter"], "github");
    assert_eq!(bound[0]["number"], 42);
    // The GitHub adapter's record travels with its repository pull request.
    assert_eq!(bound[0]["headRefName"], "feature");
    assert_eq!(bound[0]["github"]["providerRepoFullName"], "acme/api");
    assert_eq!(bound[1]["headRefName"], "feature");
    assert_eq!(bound[1].get("github"), None);
    assert_eq!(bound[1]["adapter"], "g5");
    // Named by its G5 PR, never by a number (g5.git.pr-local-number).
    assert_eq!(bound[1]["number"], Value::Null);
    assert_eq!(
        body["data"]["pullRequest"]["number"],
        format!("PR{number:06x}")
    );
    assert_eq!(bound[1]["repository"], json!(docs_name));
    assert_eq!(bound[1]["state"], "OPEN");
    // Selecting the local member through the G5 PR's name answers without
    // GitHub.
    let before = github.received_requests().await.unwrap().len();
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!(
            "{}?checks=true&selector=PR{number:06x}&repo={docs_name}",
            route(w.id)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let repositories = body["data"]["repositories"].as_array().unwrap();
    assert_eq!(repositories.len(), 1, "{body}");
    assert_eq!(repositories[0]["tracker"], "g5");
    assert_eq!(github.received_requests().await.unwrap().len(), before);
    // A selected GitHub repository still takes --head alone.
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?checks=true&repo=acme/api&head={CHECK_SHA}", route(w.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["repositories"].as_array().unwrap().len(), 1);
    assert!(github.received_requests().await.unwrap().len() > before);
    app.cleanup().await;
}

#[tokio::test]
async fn checks_revalidate_same_head_reruns_and_preserve_distinct_apps() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    let url = format!("{}?checks=true&selector=42", route(w.id));
    for mode in ["queued", "transition", "completed"] {
        if mode != "queued" {
            github.reset().await;
        }
        check_head(&github, CHECK_SHA).await;
        check_rows(&github,json!([
            {"id":1,"name":"tests","head_sha":CHECK_SHA,"check_suite":{"id":9},"status":"completed","conclusion":"success"},
            {"id":2,"name":"tests","head_sha":CHECK_SHA,"check_suite":{"id":10},"status":"completed","conclusion":"failure"}
        ]),json!([])).await;
        if mode != "completed" {
            let calls = Arc::new(AtomicUsize::new(0));
            Mock::given(method("GET"))
                .and(path(format!(
                    "/repos/acme/api/commits/{CHECK_SHA}/check-suites"
                )))
                .respond_with(move |_: &wiremock::Request| {
                    let state = if mode == "queued" || calls.fetch_add(1, Ordering::SeqCst) > 0 {
                        "queued"
                    } else {
                        "completed"
                    };
                    ResponseTemplate::new(200).set_body_json(json!({"check_suites":[
                        {"id":9,"head_sha":CHECK_SHA,"status":state,"app":{"name":"App 9"}},
                        {"id":10,"head_sha":CHECK_SHA,"status":"completed","app":{"name":"App 10"}}
                    ]}))
                })
                .with_priority(1)
                .mount(&github)
                .await;
        }
        let (_, body) = get(&app, &owner.jwt, &url).await;
        let repo = &body["data"]["repositories"][0];
        match mode {
            "queued" => {
                let rows = repo["checks"].as_array().unwrap();
                assert!(rows.iter().all(|r| r["kind"] != "check_suite"), "{body}");
                assert!(rows
                    .iter()
                    .any(|r| r["kind"] == "check_run" && r["id"] == 1 && r["bucket"] == "pending"));
            }
            "transition" => {
                // The suite was requeued mid-read: the read is retried and
                // reports the requeued run as pending, never the old success.
                assert_eq!(repo["availability"], "ready", "{body}");
                assert!(
                    repo["checks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["id"] == 1 && r["bucket"] == "pending"),
                    "{body}"
                );
            }
            _ => {
                assert_eq!(repo["checks"].as_array().unwrap().len(), 2, "{body}");
                assert_eq!(repo["checks"][0]["bucket"], "pass");
                assert_eq!(repo["checks"][1]["bucket"], "fail");
            }
        }
    }
    app.cleanup().await;
}

/// g5.git.check-observation: a read still finding a required check pending
/// reports that one observation; only a settled result is observed twice. Each
/// repository reports its PR's GitHub lifecycle (issue #1016).
#[tokio::test]
async fn pending_check_reads_observe_once_and_settled_reads_confirm() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    let url = format!("{}?checks=true&selector=42", route(w.id));
    let reads = |suffix: &'static str| {
        let github = &github;
        async move {
            github
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.url.path().ends_with(suffix))
                .count()
        }
    };
    for (status, conclusion, expected) in [
        ("in_progress", Value::Null, 1),
        ("completed", json!("success"), 2),
    ] {
        // The first read mints the installation token; later ones reuse it.
        if expected > 1 {
            github.reset().await;
        }
        check_head(&github, CHECK_SHA).await;
        branch_requirements(
            &github,
            "main",
            json!({"enabled":true,"required_status_checks":{"enforcement_level":"everyone","contexts":["tests"],"checks":[{"context":"tests","app_id":null}]}}),
            json!([]),
            1,
        )
        .await;
        check_rows(
            &github,
            json!([{"id":1,"name":"tests","head_sha":CHECK_SHA,"status":status,"conclusion":conclusion}]),
            json!([]),
        )
        .await;
        let (_, body) = get(&app, &owner.jwt, &url).await;
        let repo = &body["data"]["repositories"][0];
        assert_eq!(repo["availability"], "ready", "{body}");
        assert_eq!(repo["prState"], "open", "{body}");
        assert_eq!(
            repo["checks"][0]["bucket"],
            if expected == 1 { "pending" } else { "pass" }
        );
        for suffix in ["/pulls/42", "/check-suites", "/check-runs", "/statuses"] {
            assert_eq!(reads(suffix).await, expected, "{status} {suffix}");
        }
    }
    github.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":42,"state":"closed","merged":true,"mergeable":null,"base":{"ref":"main","sha":NEXT_SHA},"head":{"sha":CHECK_SHA}})))
        .mount(&github)
        .await;
    check_rows(&github, json!([{"id":1,"name":"tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"success"}]), json!([])).await;
    let (_, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(
        body["data"]["repositories"][0]["prState"], "merged",
        "{body}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn required_checks_come_from_the_base_branch_and_are_expected_until_reported() {
    use g5_api_core::services::git::workspace_pull_request_provider_lifecycle::{
        external_checks_verdict, ExternalChecksVerdict,
    };
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    let url = format!("{}?checks=true&selector=42", route(w.id));
    let run = |id: u64, name: &str, app: u64, status: &str, conclusion: Value| json!({"id":id,"name":name,"head_sha":CHECK_SHA,"check_suite":{"id":app},"app":{"id":app},"status":status,"conclusion":conclusion});
    let read = |runs: Value, statuses: Value| {
        let (github, app, owner, url) = (&github, &app, &owner, &url);
        async move {
            check_head(github, CHECK_SHA).await;
            check_rows(github, runs, statuses).await;
            // A silent App's suite: queued forever with no runs.
            Mock::given(method("GET"))
                .and(path(format!("/repos/acme/api/commits/{CHECK_SHA}/check-suites")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"check_suites":[
                    {"id":9,"head_sha":CHECK_SHA,"status":"completed","app":{"id":9}},
                    {"id":77,"head_sha":CHECK_SHA,"status":"in_progress","app":{"id":77}},
                    {"id":99,"head_sha":CHECK_SHA,"status":"completed","app":{"id":99}},
                    {"id":20,"head_sha":CHECK_SHA,"status":"queued","latest_check_runs_count":0,"app":{"id":20,"name":"Silent App"}}
                ]})))
                .with_priority(1)
                .mount(github)
                .await;
            let (status, body) = get(app, &owner.jwt, url).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            body["data"].clone()
        }
    };
    let required = |data: &Value| -> Vec<(String, String, bool)> {
        data["repositories"][0]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["name"].as_str().unwrap().to_owned(),
                    format!(
                        "{}/{}",
                        r["kind"].as_str().unwrap(),
                        r["bucket"].as_str().unwrap()
                    ),
                    r["required"] == true,
                )
            })
            .collect()
    };
    let row = |name: &str, kind: &str, required: bool| (name.to_owned(), kind.to_owned(), required);

    // Classic protection binds Cloud Build to App 77; a ruleset requires "lint"
    // from any source. Other rule types and duplicates are ignored.
    branch_requirements(
        &github,
        "main",
        json!({"enabled":true,"required_status_checks":{"enforcement_level":"non_admins","contexts":["Cloud Build"],"checks":[{"context":"Cloud Build","app_id":77}]}}),
        json!([
            {"type":"pull_request","parameters":{"required_approving_review_count":1}},
            {"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true,"required_status_checks":[{"context":"lint"},{"context":"Cloud Build","integration_id":77}]}}
        ]),
        1,
    )
    .await;
    let data = read(
        json!([run(1, "tests", 9, "completed", json!("success"))]),
        json!([]),
    )
    .await;
    let repo = &data["repositories"][0];
    assert_eq!(repo["availability"], "ready", "{data}");
    assert_eq!(repo["baseRefName"], "main");
    assert_eq!(
        repo["requiredChecks"],
        json!([{"context":"Cloud Build","appId":77},{"context":"lint","appId":null}])
    );
    // The silent App's suite leaves no row; the required checks wait by name.
    assert_eq!(
        required(&data),
        vec![
            row("Cloud Build", "expected/pending", true),
            row("lint", "expected/pending", true),
            row("tests", "check_run/pass", false),
        ],
        "{data}"
    );
    assert_eq!(
        repo["checks"][0]["description"],
        "Required by main; not reported on this revision yet."
    );
    assert_eq!(
        external_checks_verdict(&data),
        ExternalChecksVerdict::Pending
    );

    // The requirement set is cached: these reads never ask for it again. The
    // wrong App's "Cloud Build" does not satisfy the App-bound requirement; a
    // status context matches by name regardless of case.
    github.reset().await;
    let data = read(
        json!([
            run(2, "Cloud Build", 99, "completed", json!("success")),
            run(3, "Cloud Build", 77, "in_progress", Value::Null)
        ]),
        json!([{"id":5,"context":"LINT","state":"success"}]),
    )
    .await;
    assert_eq!(
        required(&data),
        vec![
            row("Cloud Build", "check_run/pass", false),
            row("Cloud Build", "check_run/pending", true),
            row("LINT", "status/pass", true),
        ],
        "{data}"
    );
    assert_eq!(
        external_checks_verdict(&data),
        ExternalChecksVerdict::Pending
    );

    // Required checks passed: a not-required check still running is advisory.
    github.reset().await;
    let data = read(
        json!([
            run(3, "Cloud Build", 77, "completed", json!("success")),
            run(4, "slow review", 9, "in_progress", Value::Null)
        ]),
        json!([{"id":5,"context":"lint","state":"success"}]),
    )
    .await;
    assert_eq!(
        external_checks_verdict(&data),
        ExternalChecksVerdict::Passing,
        "{data}"
    );

    // A not-required failure still blocks.
    github.reset().await;
    let data = read(
        json!([
            run(3, "Cloud Build", 77, "completed", json!("success")),
            run(4, "flaky", 9, "completed", json!("failure"))
        ]),
        json!([{"id":5,"context":"lint","state":"success"}]),
    )
    .await;
    assert_eq!(
        external_checks_verdict(&data),
        ExternalChecksVerdict::Failing,
        "{data}"
    );
    assert!(github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| !r.url.path().contains("/branches/")));
    app.cleanup().await;
}

/// GitHub judges required checks on the test-merge commit once it has any check
/// or status, and on the head otherwise; an unreadable requirement source makes
/// the repository unavailable rather than "nothing required".
#[tokio::test]
async fn required_checks_are_judged_on_githubs_evaluated_revision_and_unreadable_requirements_are_unavailable(
) {
    use g5_api_core::services::git::workspace_pull_request_provider_lifecycle::{
        external_checks_verdict, ExternalChecksVerdict,
    };
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    let url = format!("{}?checks=true&selector=42", route(w.id));
    let run = |id: u64, name: &str, sha: &str| json!({"id":id,"name":name,"head_sha":sha,"status":"completed","conclusion":"success"});
    // (case, base branch, test-merge runs)
    for (case, branch, merge_runs) in [
        ("merge missing", "main", json!([run(2, "lint", NEXT_SHA)])),
        ("merge met", "main", json!([run(2, "tests", NEXT_SHA)])),
        ("head", "main", json!([])),
        ("unreadable", "release%2F1", json!([])),
    ] {
        if case != "merge missing" {
            github.reset().await;
        }
        let base = branch.replace("%2F", "/");
        Mock::given(method("GET")).and(path("/repos/acme/api/pulls/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":42,"state":"open","head":{"sha":CHECK_SHA},"base":{"ref":base,"sha":NEXT_SHA},"mergeable":true,"merge_commit_sha":NEXT_SHA})))
            .mount(&github).await;
        check_rows(&github, json!([run(1, "tests", CHECK_SHA)]), json!([])).await;
        revision_rows(&github, NEXT_SHA, merge_runs, json!([])).await;
        // Unenforced classic protection requires nothing. After the first read the
        // set for main is cached, so later cases see it without these mocks.
        branch_requirements(
            &github,
            branch,
            json!({"enabled":true,"required_status_checks":{"enforcement_level":"off","contexts":["ignored"],"checks":[{"context":"ignored","app_id":null}]}}),
            json!([{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"tests"}]}}]),
            if case == "merge missing" || case == "unreadable" { 2 } else { 20 },
        )
        .await;
        if case == "unreadable" {
            // The base branch name travels as one encoded path segment.
            Mock::given(method("GET"))
                .and(path(format!("/repos/acme/api/rules/branches/{branch}")))
                .respond_with(
                    ResponseTemplate::new(403)
                        .set_body_json(json!({"message":"Resource not accessible by integration"})),
                )
                .with_priority(1)
                .mount(&github)
                .await;
        }
        let (status, body) = get(&app, &owner.jwt, &url).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let repo = &body["data"]["repositories"][0];
        let rows = repo["checks"].as_array().unwrap();
        let required = |revision: &str| {
            rows.iter()
                .filter(|r| r["required"] == true && r["revision"] == revision)
                .map(|r| r["kind"].as_str().unwrap())
                .collect::<Vec<_>>()
        };
        let verdict = external_checks_verdict(&body["data"]);
        match case {
            "merge missing" => {
                assert_eq!(repo["availability"], "ready", "{body}");
                // The head's passing "tests" does not count once the test merge
                // has checks: "tests" is expected there.
                assert_eq!(required("head"), Vec::<&str>::new(), "{body}");
                assert_eq!(required("test_merge"), vec!["expected"], "{body}");
                let expected = rows.iter().find(|r| r["kind"] == "expected").unwrap();
                assert_eq!(expected["commitOid"], NEXT_SHA);
                assert!(rows.iter().all(|r| r["name"] != "ignored"));
                assert_eq!(verdict, ExternalChecksVerdict::Pending);
            }
            "merge met" => {
                assert_eq!(required("head"), Vec::<&str>::new(), "{body}");
                assert_eq!(required("test_merge"), vec!["check_run"], "{body}");
                assert_eq!(verdict, ExternalChecksVerdict::Passing);
            }
            "head" => {
                assert_eq!(required("head"), vec!["check_run"], "{body}");
                assert_eq!(verdict, ExternalChecksVerdict::Passing);
            }
            _ => {
                assert_eq!(repo["availability"], "unavailable", "{body}");
                assert_eq!(repo["checks"], json!([]));
                assert!(github
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .any(|r| r.url.path() == "/repos/acme/api/rules/branches/release%2F1"));
                assert_eq!(verdict, ExternalChecksVerdict::Pending);
            }
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn checks_include_test_merge_failures_and_wait_for_unknown_or_changed_revisions() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    // What GitHub reports on each PR read: a base that advances once or keeps
    // advancing, a test merge recomputed once, mergeability still computing,
    // and a head that moves. A read that could settle is confirmed: only a
    // moved head ends it at once, and the rest are re-read, up to three times.
    // Mergeability still computing is pending anyway, so it is reported from
    // one read (g5.git.check-observation).
    for mode in [
        "stable",
        "unknown",
        "base_once",
        "merge_once",
        "base_moving",
        "head",
    ] {
        if mode != "stable" {
            github.reset().await;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let pulls = calls.clone();
        Mock::given(method("GET")).and(path("/repos/acme/api/pulls/42"))
            .respond_with(move |_: &wiremock::Request| {
                let call = pulls.fetch_add(1, Ordering::SeqCst);
                let later = call > 0;
                let base = match mode {
                    "base_once" if later => CHECK_SHA.to_owned(),
                    "base_moving" => format!("{:040x}", call + 1),
                    _ => NEXT_SHA.to_owned(),
                };
                let merge = if mode == "merge_once" && later { "c".repeat(40) } else { NEXT_SHA.to_owned() };
                let head = if mode == "head" && later { NEXT_SHA } else { CHECK_SHA };
                ResponseTemplate::new(200).set_body_json(json!({"number":42,"state":"open","head":{"sha":head},"base":{"ref":"main","sha":base},"mergeable":if mode=="unknown" {Value::Null} else {json!(true)},"merge_commit_sha":merge}))
            }).mount(&github).await;
        check_rows(&github,json!([{"id":1,"name":"head tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"success"}]),json!([])).await;
        revision_rows(&github,NEXT_SHA,json!([{"id":2,"name":"merge tests","head_sha":NEXT_SHA,"status":"completed","conclusion":"failure"}]),json!([])).await;
        revision_rows(&github,&"c".repeat(40),json!([{"id":3,"name":"merge tests","head_sha":"c".repeat(40),"status":"completed","conclusion":"failure"}]),json!([])).await;
        let (_, body) = get(
            &app,
            &owner.jwt,
            &format!("{}?checks=true&selector=42", route(w.id)),
        )
        .await;
        let repo = &body["data"]["repositories"][0];
        let reads = calls.load(Ordering::SeqCst);
        match mode {
            "stable" | "base_once" | "merge_once" => {
                assert_eq!(repo["availability"], "ready", "{mode}: {body}");
                assert_eq!(repo["checks"][1]["bucket"], "fail");
                assert_eq!(repo["checks"][1]["revision"], "test_merge");
                // One read, or one re-read after GitHub settled.
                assert_eq!(reads, if mode == "stable" { 2 } else { 4 }, "{mode}");
            }
            "unknown" => {
                assert_eq!(repo["availability"], "pending", "{body}");
                assert!(
                    repo["message"]
                        .as_str()
                        .unwrap()
                        .contains("computing the current test-merge"),
                    "{body}"
                );
                assert_eq!(repo["checks"][0]["name"], "head tests", "{body}");
                assert_eq!(reads, 1, "a pending read is not repeated");
            }
            "base_moving" => {
                assert_eq!(repo["availability"], "pending", "{body}");
                assert!(
                    repo["message"]
                        .as_str()
                        .unwrap()
                        .contains("base branch kept advancing"),
                    "{body}"
                );
                assert_eq!(repo["checks"], json!([]));
                assert_eq!(reads, 6);
            }
            _ => {
                assert_eq!(repo["availability"], "pending", "{body}");
                assert!(
                    repo["message"]
                        .as_str()
                        .unwrap()
                        .contains("head, state or base branch changed"),
                    "{body}"
                );
                assert_eq!(repo["headRefOid"], NEXT_SHA);
                assert_eq!(repo["checks"], json!([]));
                assert_eq!(reads, 2, "a moved head is not re-read");
            }
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn annotation_failures_preserve_diagnostics_and_stop_provider_requests() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    let secret = "SCOPED_CONFIG_SECRET_abcdefghijklmnopqrstuvwxyz";
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token(&json!({"BUILD_SECRET":secret}).to_string())
        .unwrap();
    sqlx::query("INSERT INTO workspace_configs(org_id,workspace_id,secrets_enc,secret_keys) VALUES($1,$2,$3,ARRAY['BUILD_SECRET']) ON CONFLICT(workspace_id) DO UPDATE SET secrets_enc=EXCLUDED.secrets_enc")
        .bind(org.id).bind(w.id).bind(encrypted).execute(&app.db).await.unwrap();
    for code in [503, 429, 403] {
        if code != 503 {
            github.reset().await;
        }
        check_head(&github, CHECK_SHA).await;
        check_rows(&github,json!([{"id":7,"name":"tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"failure","output":{"summary":"compiler E0308 checks-only-token","text":"expected bool","annotations_count":1}}]),json!([])).await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/check-runs/7/annotations"))
            .respond_with(
                ResponseTemplate::new(code)
                    .insert_header("Retry-After", "60")
                    .insert_header("X-RateLimit-Remaining", "0")
                    .insert_header("X-RateLimit-Reset", "9999999999")
                    .insert_header(
                        "X-GitHub-Request-Id",
                        format!("{} {secret}", "x".repeat(1000)),
                    )
                    .set_body_json(json!({"message":format!("{} {secret}", "x".repeat(1000))})),
            )
            .expect(1)
            .mount(&github)
            .await;
        let (_, body) = get(
            &app,
            &owner.jwt,
            &format!("{}?checks=true&selector=42&details=true", route(w.id)),
        )
        .await;
        assert!(!body.to_string().contains("SCOPED_CONFIG_SECRET"), "{body}");
        assert!(body.to_string().contains("retry-after 60"), "{body}");
        let repo = &body["data"]["repositories"][0];
        assert_eq!(repo["availability"], "unavailable", "{body}");
        assert_eq!(repo["rateLimited"], code != 503);
        let details = &repo["checks"][0]["details"];
        assert_eq!(details["summary"], "compiler E0308 [redacted]");
        assert_eq!(details["text"], "expected bool");
        assert_eq!(details["availability"], "partial");
        assert_eq!(details["annotationAvailability"], "unavailable");
        let requests = github.received_requests().await.unwrap();
        assert!(
            requests
                .last()
                .unwrap()
                .url
                .path()
                .ends_with("/annotations"),
            "no request after annotation failure"
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.url.path().ends_with("/pulls/42"))
                .count(),
            1
        );
        github.verify().await;
    }
    app.cleanup().await;
}

#[tokio::test]
async fn active_suites_preserve_current_failures_and_details_for_users_and_vm_agents() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    checks_binding(&app, &github, &owner, &org, &w).await;
    check_head(&github, CHECK_SHA).await;
    check_rows(&github,json!([
        {"id":1,"name":"failed job","head_sha":CHECK_SHA,"status":"completed","conclusion":"failure","output":{"summary":"compiler E0308","annotations_count":0}},
        {"id":2,"name":"running job","head_sha":CHECK_SHA,"status":"in_progress"}
    ]),json!([])).await;
    Mock::given(method("GET")).and(path(format!("/repos/acme/api/commits/{CHECK_SHA}/check-suites")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"check_suites":[{"id":9,"head_sha":CHECK_SHA,"status":"in_progress","app":{"name":"Tests"}}]})))
        .with_priority(1).mount(&github).await;
    let vm = mint_vm(&app, org.id, w.id, owner.id).await;
    for (token, endpoint) in [
        (&owner.jwt, route(w.id)),
        (
            &vm,
            format!("/api/internal/workspaces/{}/pull-requests/info", w.id),
        ),
    ] {
        let (_, body) = get(
            &app,
            token,
            &format!("{endpoint}?checks=true&selector=42&details=true"),
        )
        .await;
        let rows = body["data"]["repositories"][0]["checks"]
            .as_array()
            .unwrap();
        // The active suite is not a row of its own; its running job is.
        assert_eq!(rows.len(), 2, "{body}");
        let failed = rows
            .iter()
            .find(|r| r["kind"] == "check_run" && r["id"] == 1)
            .unwrap();
        assert_eq!(failed["bucket"], "fail");
        assert_eq!(failed["details"]["summary"], "compiler E0308");
        assert_eq!(rows.iter().filter(|r| r["bucket"] == "pending").count(), 1);
    }
    app.cleanup().await;
}

#[tokio::test]
async fn configured_instance_reads_use_attached_identity_and_reject_foreign_origin() {
    for with_app in [true, false] {
        let github = MockServer::start().await;
        let mut config = test_config();
        let mut integration = helpers::github_app::github_app_config(&github);
        integration.web_base_url = "https://enterprise.test/git".into();
        if with_app {
            config.github_app = Some(integration);
        } else {
            config.auth.scm_github_api_base_url = github.uri();
            config.auth.scm_github_web_base_url = "https://enterprise.test/git".into();
        }
        // The App's API and browser bases compose repository operations. The login
        // and default public SCM API are deliberately different from this instance.
        let app = TestApp::spawn_with_config(config).await;
        let (owner, org, w) = fixture(&app).await;
        if with_app {
            checks_binding(&app, &github, &owner, &org, &w).await;
        } else {
            let encrypted = app
                .state
                .scm_token_manager
                .as_ref()
                .unwrap()
                .encrypt_token("checks-only-token")
                .unwrap();
            sqlx::query("INSERT INTO user_scm_connections(id,user_id,provider,provider_user_id,provider_username,access_token_enc,scopes) VALUES($1,$2,'github','reader','reader',$3,ARRAY['repo'])")
            .bind(Uuid::new_v4()).bind(owner.id).bind(encrypted).execute(&app.db).await.unwrap();
        }
        sqlx::query("UPDATE repos SET git_url='https://enterprise.test/git/acme/api.git',provider='github' WHERE id=$1")
        .bind(w.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
        if with_app {
            mount_token(&github).await;
        }
        Mock::given(method("GET")).and(path("/repos/acme/api/pulls/42"))
        .and(header("authorization","Bearer installation-read-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":42,"state":"open","title":"Instance PR","head":{"sha":CHECK_SHA},"base":{"sha":NEXT_SHA}})))
        .expect(if with_app { 1 } else { 0 }).mount(&github).await;
        check_head(&github, CHECK_SHA).await;
        check_rows(&github, json!([]), json!([])).await;
        let selector = "https://enterprise.test/git/acme/api/pull/42";
        let (status, body) = get(
            &app,
            &owner.jwt,
            &format!("{}?selector={selector}", route(w.id)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["pullRequest"]["url"], selector);
        assert!(!body.to_string().contains("checks-only-token"));
        let (status, body) = get(
            &app,
            &owner.jwt,
            &format!("{}?checks=true&selector={selector}", route(w.id)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["data"]["repositories"][0]["availability"], "ready",
            "{body}"
        );
        let before = github.received_requests().await.unwrap().len();
        sqlx::query("UPDATE repos SET git_url='https://github.com/acme/api.git' WHERE id=$1")
            .bind(w.repo_id)
            .execute(&app.db)
            .await
            .unwrap();
        for extra in ["", "&checks=true"] {
            let (status, body) = get(
                &app,
                &owner.jwt,
                &format!("{}?selector={selector}{extra}", route(w.id)),
            )
            .await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        }
        assert_eq!(github.received_requests().await.unwrap().len(), before);
        github.verify().await;
        app.cleanup().await;
    }
}

/// Every provider request was a read: GETs, Markdown rendering and token
/// minting only.
async fn assert_provider_reads_only(github: &MockServer) {
    assert!(github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method == wiremock::http::Method::GET
            || (r.method == wiremock::http::Method::POST
                && (r.url.path() == "/markdown" || r.url.path().ends_with("/access_tokens")))));
}

/// `GET …/pull-requests/:id/description` (g5.git.pr-description): the saved
/// description against each provider body as the publication serializer
/// would write it, GitHub's lifecycle and rendering, and no provider call or
/// write for a PR without publications.
#[tokio::test]
async fn pr_description_compares_provider_bodies_with_the_serializer_without_writing() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    bind_github(&app, &owner, &org, &w).await;
    Mock::given(method("POST"))
        .and(path("/app/installations/555/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"installation-read-only","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
        .mount(&github)
        .await;
    let target = TestWorkspace::create(&app, &org).await;
    let saved = "Saved **text** for #12";
    let preflight = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,"decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"description-fixture","repoResults":[],"message":"ready"});
    let pr = Uuid::new_v4();
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,title,description_override,local_merge_status,last_preflight_fingerprint,preflight) VALUES($1,$2,$3,$4,'Outgoing PR',$5,'ready','description-fixture',$6)")
        .bind(pr).bind(org.id).bind(w.id).bind(target.id).bind(saved).bind(&preflight).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_pull_request_provider_repos(id,org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_pr_url,provider_status,source_head_sha,target_head_sha) VALUES($1,$2,$3,$4,'github','acme/api','feature','main',42,'https://github.com/acme/api/pull/42','open','1111111111111111111111111111111111111111','2222222222222222222222222222222222222222')")
        .bind(Uuid::new_v4()).bind(org.id).bind(pr).bind(w.repo_id).execute(&app.db).await.unwrap();
    // The serializer appends the provenance line to the saved description.
    let published = format!(
        "{saved}\nCreated by G5 workspace {}/{}/{}",
        app.base_url, org.slug, w.name
    );
    let pull = |body: &str| {
        json!({"number":42,"state":"open","draft":true,"merged":false,"merged_at":null,
               "title":"Outgoing PR","body":body,"body_html":"<p>GitHub rendering</p>"})
    };
    let mount_pull = |body: String| {
        let github = &github;
        async move {
            Mock::given(method("GET"))
                .and(path("/repos/acme/api/pulls/42"))
                .and(header("accept", "application/vnd.github.full+json"))
                .and(header("authorization", "Bearer installation-read-only"))
                .respond_with(ResponseTemplate::new(200).set_body_json(pull(&body)))
                .mount(github)
                .await;
        }
    };
    mount_pull(published.replace('\n', "\r\n")).await;
    Mock::given(method("POST"))
        .and(path("/markdown"))
        .and(body_json(
            json!({"text":saved,"mode":"gfm","context":"acme/api"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<p>Saved <strong>text</strong> for <a>#12</a></p>",
            "text/html",
        ))
        .mount(&github)
        .await;
    let before: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('pr',(SELECT to_jsonb(p) FROM workspace_pull_requests p WHERE id=$1),'repos',(SELECT jsonb_agg(to_jsonb(r)) FROM workspace_pull_request_provider_repos r WHERE pull_request_id=$1))",
    )
    .bind(pr)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let url = format!("/api/v1/workspaces/{}/pull-requests/{pr}/description", w.id);
    let (status, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["description"], saved);
    assert_eq!(data["descriptionSource"], "saved");
    assert_eq!(
        data["descriptionHtml"],
        "<p>Saved <strong>text</strong> for <a>#12</a></p>"
    );
    let repo = &data["repositories"][0];
    assert_eq!(repo["repository"], "acme/api");
    assert_eq!(repo["number"], 42);
    assert_eq!(repo["lifecycle"], "draft");
    assert_eq!(repo["comparison"], "matches", "{body}");
    assert_eq!(repo["bodyHtml"], "<p>GitHub rendering</p>");

    // An edit on GitHub is a difference; G5's description stays available.
    assert_provider_reads_only(&github).await;
    github.reset().await;
    Mock::given(method("POST"))
        .and(path("/app/installations/555/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"installation-read-only","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
        .mount(&github)
        .await;
    mount_pull(format!("{published}\n\nAdditional note on GitHub")).await;
    let (status, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["description"], saved);
    assert_eq!(body["data"]["repositories"][0]["comparison"], "differs");
    assert!(body["data"]["repositories"][0]["body"]
        .as_str()
        .unwrap()
        .ends_with("Additional note on GitHub"));
    // GitHub could not render: the description remains, without HTML.
    assert_eq!(body["data"]["descriptionHtml"], Value::Null);

    // A provider read failure is unavailable, never a match.
    assert_provider_reads_only(&github).await;
    github.reset().await;
    let (status, body) = get(&app, &owner.jwt, &url).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let repo = &body["data"]["repositories"][0];
    assert_eq!(repo["comparison"], "unavailable", "{body}");
    assert_eq!(repo["lifecycle"], Value::Null);
    assert_eq!(body["data"]["description"], saved);

    // Reading never writes either side.
    let after: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('pr',(SELECT to_jsonb(p) FROM workspace_pull_requests p WHERE id=$1),'repos',(SELECT jsonb_agg(to_jsonb(r)) FROM workspace_pull_request_provider_repos r WHERE pull_request_id=$1))",
    )
    .bind(pr)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(before, after);
    assert_provider_reads_only(&github).await;

    // Without publications the description is read without any provider call.
    github.reset().await;
    let native = Uuid::new_v4();
    let other_target = TestWorkspace::create(&app, &org).await;
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,title,description_override,local_merge_status,last_preflight_fingerprint,preflight) VALUES($1,$2,$3,$4,'Native PR','Native description','ready','description-fixture',$5)")
        .bind(native).bind(org.id).bind(w.id).bind(other_target.id).bind(&preflight).execute(&app.db).await.unwrap();
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/{native}/description",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["description"], "Native description");
    assert_eq!(body["data"]["repositories"], json!([]));
    assert_eq!(body["data"]["descriptionHtml"], Value::Null);
    assert!(github.received_requests().await.unwrap().is_empty());

    // `g5d pr view` reads the same description through the VM's route.
    let vm = mint_vm(&app, org.id, w.id, owner.id).await;
    let (status, internal) = get(
        &app,
        &vm,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/{native}/description",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{internal}");
    assert_eq!(internal["data"], body["data"]);
    let (status, _) = get(
        &app,
        &vm,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/{}/description",
            w.id,
            Uuid::new_v4()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Any workspace reads a PR its reader can read (`g5.git.pr-reads`).
    let outsider = TestWorkspace::create(&app, &org).await;
    let (status, _) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/{pr}/description",
            outsider.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    app.cleanup().await;
}

/// `GET …/pull-requests/descriptions?ids=…` (g5.git.pr-description): a
/// listing's `body` resolves each PR exactly as the single read does (saved,
/// else the PR-body Project Doc, else none) in one request, with no provider
/// call even for a published PR, for the user and the VM alike.
#[tokio::test]
async fn pr_description_batch_resolves_like_the_single_read_without_provider_calls() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    bind_github(&app, &owner, &org, &w).await;
    let target = TestWorkspace::create(&app, &org).await;

    // The workspace's own repository serves as its docs repository, with the
    // PR-body Project Doc committed on the workspace branch.
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
        .bind(w.id)
        .bind(w.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let doc_path = g5_contracts::g5_paths::workspace_pr_body_doc_path(&w.id.to_string(), &w.name);
    let mirror = app.repo_path(w.repo_id).await;
    let repo = g5_scm::GitRepo::open(&mirror).unwrap();
    repo.checkout_branch_force(&w.name).unwrap();
    std::fs::create_dir_all(mirror.join(&doc_path).parent().unwrap()).unwrap();
    std::fs::write(mirror.join(&doc_path), "Body from the Project Doc\n").unwrap();
    repo.add_all().unwrap();
    let docs_head = repo
        .commit(
            "PR body",
            &g5_scm::GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
        )
        .unwrap();
    let with_docs = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,
        "decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"batch","message":"ready",
        "repoResults":[{"repoId":w.repo_id,"sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,
            "sourceBranchName":w.name,"targetBranchName":target.name,"sourceHead":docs_head,
            "targetHead":"b".repeat(40),"outcome":"fastForward","conflictPaths":[],"message":null}]});
    let without_docs = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,
        "decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"batch","message":"ready","repoResults":[]});
    // One open PR per source and target: each PR gets its own target.
    let insert = |saved: Option<&'static str>, preflight: Value| {
        let (app, org_ref) = (&app, &org);
        let (org, w) = (org.id, w.id);
        async move {
            let target = TestWorkspace::create(app, org_ref).await.id;
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,title,description_override,local_merge_status,last_preflight_fingerprint,preflight) VALUES($1,$2,$3,$4,'PR',$5,'ready','batch',$6)")
                .bind(id).bind(org).bind(w).bind(target).bind(saved).bind(preflight).execute(&app.db).await.unwrap();
            id
        }
    };
    // A saved description wins over the Project Doc, and a blank one does not.
    let saved = insert(Some(" Saved text \n"), with_docs.clone()).await;
    let blank = insert(Some("   "), with_docs.clone()).await;
    let doc = insert(None, with_docs.clone()).await;
    let none = insert(None, without_docs).await;
    // A published PR: the single read would compare it with GitHub.
    sqlx::query("INSERT INTO workspace_pull_request_provider_repos(id,org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_pr_url,provider_status,source_head_sha,target_head_sha) VALUES($1,$2,$3,$4,'github','acme/api','feature','main',42,'https://github.com/acme/api/pull/42','open','1111111111111111111111111111111111111111','2222222222222222222222222222222222222222')")
        .bind(Uuid::new_v4()).bind(org.id).bind(saved).bind(w.repo_id).execute(&app.db).await.unwrap();

    let ids = [saved, blank, doc, none, saved];
    let query = ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let (status, batch) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/descriptions?ids={query}",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch}");
    assert_eq!(
        batch["data"],
        json!([
            {"pullRequestId":saved,"description":"Saved text","descriptionSource":"saved"},
            {"pullRequestId":blank,"description":"Body from the Project Doc","descriptionSource":"projectDoc"},
            {"pullRequestId":doc,"description":"Body from the Project Doc","descriptionSource":"projectDoc"},
            {"pullRequestId":none,"description":null,"descriptionSource":null},
        ])
    );
    assert!(github.received_requests().await.unwrap().is_empty());
    // The same text as the single read the landing uses.
    for (index, id) in [blank, doc, none].into_iter().enumerate() {
        let (status, single) = get(
            &app,
            &owner.jwt,
            &format!("/api/v1/workspaces/{}/pull-requests/{id}/description", w.id),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{single}");
        assert_eq!(
            single["data"]["description"],
            batch["data"][index + 1]["description"]
        );
        assert_eq!(
            single["data"]["descriptionSource"],
            batch["data"][index + 1]["descriptionSource"]
        );
    }
    // `g5d pr list --json body` in the VM gets the same answer.
    let vm = mint_vm(&app, org.id, w.id, owner.id).await;
    let (status, internal) = get(
        &app,
        &vm,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/descriptions?ids={query}",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{internal}");
    assert_eq!(internal["data"], batch["data"]);

    // A readable PR is read from any workspace (`g5.git.pr-reads`); an unknown
    // id, an invalid id and an oversized batch are refused.
    let outsider = TestWorkspace::create(&app, &org).await;
    let (status, _) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/descriptions?ids={saved}",
            outsider.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/descriptions?ids={saved},{}",
            outsider.id,
            Uuid::new_v4()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/descriptions?ids={saved},nope",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let many = (0..101)
        .map(|_| Uuid::new_v4().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let (status, _) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/descriptions?ids={many}",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    app.cleanup().await;
}

/// `g5.git.pr-no-github`: checks for a PR that expects no GitHub succeed with
/// nothing to observe, for the user and the VM, with or without `head`; once
/// a member would publish to GitHub, a missing publication is reported again.
#[tokio::test]
async fn checks_for_a_pr_without_github_report_that_no_external_checks_apply() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    let target = TestWorkspace::create(&app, &org).await;
    let pr = Uuid::new_v4();
    let preflight = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,"decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"local","repoResults":[],"message":"ready"});
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,title,local_merge_status,last_preflight_fingerprint,preflight) VALUES($1,$2,$3,$4,'Local PR','ready','local',$5)")
        .bind(pr).bind(org.id).bind(w.id).bind(target.id).bind(preflight).execute(&app.db).await.unwrap();
    let vm = mint_vm(&app, org.id, w.id, owner.id).await;
    let head = "a".repeat(40);
    for (token, prefix) in [(&owner.jwt, "v1"), (&vm, "internal")] {
        for query in ["checks=true".to_owned(), format!("checks=true&head={head}")] {
            let (status, body) = get(
                &app,
                token,
                &format!(
                    "/api/{prefix}/workspaces/{}/pull-requests/info?{query}",
                    w.id
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{prefix} {query}: {body}");
            assert_eq!(body["data"]["noExternalChecks"], true, "{body}");
            assert_eq!(body["data"]["repositories"], json!([]), "{body}");
            assert!(
                body["data"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("No external checks apply"),
                "{body}"
            );
        }
    }
    assert!(github.received_requests().await.unwrap().is_empty());

    // A GitHub repository tracking a remote branch on both sides would be
    // published, so its missing publication is reported, not hidden.
    sqlx::query(
        "UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github' WHERE id=$1",
    )
    .bind(w.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT org_id,$1,repo_id,'shared',role FROM workspace_repos WHERE workspace_id=$2 AND repo_id=$3")
        .bind(target.id).bind(w.id).bind(w.repo_id).execute(&app.db).await.unwrap();
    let with_repo = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,
        "decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"local","message":"ready",
        "repoResults":[{"repoId":w.repo_id,"sourceWorkspaceId":w.id,"targetWorkspaceId":target.id,
            "sourceBranchName":w.name,"targetBranchName":target.name,"sourceHead":"a".repeat(40),
            "targetHead":"b".repeat(40),"outcome":"fastForward","conflictPaths":[],"message":null}]});
    sqlx::query("UPDATE workspace_pull_requests SET preflight=$2 WHERE id=$1")
        .bind(pr)
        .bind(with_repo)
        .execute(&app.db)
        .await
        .unwrap();
    for workspace in [w.id, target.id] {
        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,'main')")
            .bind(org.id).bind(workspace).bind(w.repo_id).execute(&app.db).await.unwrap();
    }
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests/info?checks=true", w.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["noExternalChecks"], Value::Null, "{body}");
    assert_eq!(
        body["data"]["repositories"][0]["message"],
        "Provider publication is incomplete. Run g5d pr update and retry.",
        "{body}"
    );
    app.cleanup().await;
}

/// `g5d pr attachments` (`g5.attachments`, `g5.git.pr-adapter`): the owner's
/// agent lists a GitHub PR's attachments from its description and, when
/// asked, its reviews and review comments, under the same selection and
/// read grant as `g5d pr view -c`. A stale handle and a G5 PR's name are
/// refused. Opening content without a credential is the shared mechanism's
/// (`g5-scm` `pull_request_attachments_come_from_every_text_in_github_s_rendering`).
#[tokio::test]
async fn the_workspace_agent_lists_a_github_pull_request_s_attachments_through_g5() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    bind_github(&app, &owner, &org, &w).await;
    Mock::given(method("POST")).and(path("/app/installations/555/access_tokens"))
        .and(body_json(json!({"repository_ids":[1002],"permissions":{"pull_requests":"read","contents":"read"}})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"installation-read-only","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
        .mount(&github).await;
    let full = "application/vnd.github.full+json";
    let uri = github.uri();
    let pull_api = format!("{uri}/repos/acme/api/pulls/42");
    let img = |name: &str| {
        format!(
            r#"<p><img src="https://private-user-images.githubusercontent.com/{name}.png?jwt=t" alt="{name}"></p>"#
        )
    };
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .and(header("accept", full))
        .and(header("authorization", "Bearer installation-read-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "Fix popup", "html_url": "https://github.com/acme/api/pull/42",
            "user": {"login": "author"}, "body": "before", "body_html": img("before"),
            "created_at": "2026-11-01T00:00:00Z"
        })))
        .mount(&github)
        .await;
    let line = json!({"id": 701, "user": {"login": "reviewer"}, "body": "x", "body_html": img("after"),
        "path": "src/ui.rs", "created_at": "2026-11-01T02:00:00Z", "pull_request_url": pull_api});
    for (at, body) in [
        ("/repos/acme/api/issues/42/comments", json!([])),
        ("/repos/acme/api/pulls/42/reviews", json!([])),
        ("/repos/acme/api/pulls/42/comments", json!([line.clone()])),
        ("/repos/acme/api/pulls/comments/701", line),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .and(header("accept", full))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&github)
            .await;
    }

    let host = Uuid::new_v4();
    let vm = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat) VALUES($1,$2,'us-central1','ready',now())")
        .bind(host).bind(org.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at) VALUES($1,$2,$3,$4,1,'running',now()+interval '1 hour')")
        .bind(org.id).bind(w.id).bind(host).bind(vm.to_string()).execute(&app.db).await.unwrap();
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org.id, w.id, owner.id, vm, 1, 3600)
        .unwrap();
    let listing = format!(
        "/api/internal/workspaces/{}/pull-requests/github/attachments",
        w.id
    );

    let (status, body) = get(&app, &token, &format!("{listing}?selector=42")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["pullRequest"]["displayKey"], "acme/api#42");
    let found = body["data"]["attachments"].as_array().unwrap();
    assert_eq!(found.len(), 1, "{body}");
    assert_eq!(found[0]["place"], "description");
    assert_eq!(found[0]["retrievable"], true);
    assert!(!body.to_string().contains("jwt"), "{body}");

    let (status, body) = get(
        &app,
        &token,
        &format!("{listing}?selector=42&comments=true"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let found = body["data"]["attachments"].as_array().unwrap();
    assert_eq!(found.len(), 2, "{body}");
    assert_eq!(found[1]["place"], "review comment");
    assert_eq!(found[1]["path"], "src/ui.rs");
    assert_eq!(found[1]["author"]["login"], "reviewer");
    let handle = found[1]["handle"].as_str().unwrap().to_string();

    // A handle whose text has changed since the listing is refused before
    // anything is fetched.
    let stale = format!("{}0000000000000000", &handle[..handle.len() - 16]);
    let (status, body) = get(
        &app,
        &token,
        &format!("{listing}/content?selector=42&handle={stale}"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("pull_request_text_changed"),
        "{body}"
    );

    // A G5 PR has no attachments of its own.
    let (status, body) = get(&app, &token, &format!("{listing}?selector=PR3fa9c1")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("no PR attachments"), "{body}");
}
