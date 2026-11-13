//! PR conversation writes (`g5d pr comment`, `g5d pr resolve`) through the
//! real public and workspace-incarnation routes, restricted tenant roles and a
//! stand-in GitHub: identity, repository binding, recording and replay.
use crate::helpers;
use helpers::{test_config, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn post(app: &TestApp, token: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .client
        .post(format!("{}{path}", app.base_url))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}

async fn github_app(github: &MockServer) -> TestApp {
    let mut config = test_config();
    let mut app_config = helpers::github_app::github_app_config(github);
    // Browser links are public-GitHub links; the API is the local stand-in.
    app_config.web_base_url = "https://github.com".into();
    config.github_app = Some(app_config);
    config.auth.scm_github_api_base_url = github.uri();
    TestApp::spawn_with_config(config).await
}

/// A workspace whose repository is `acme/api`, covered by an App binding
/// that may write pull requests.
async fn fixture(app: &TestApp) -> (TestUser, TestOrg, TestWorkspace) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let w = TestWorkspace::create(app, &org).await;
    sqlx::query(
        "UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github' WHERE id=$1",
    )
    .bind(w.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
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
        .bind(binding).bind(org.id).bind(identity).bind(json!({"pull_requests":"write","contents":"write"})).bind(owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO github_app_installation_binding_repositories(org_id,binding_id,github_repo_id,full_name) VALUES($1,$2,1002,'acme/api')")
        .bind(org.id).bind(binding).execute(&app.db).await.unwrap();
    (owner, org, w)
}

/// The person's GitHub App user grant, and GitHub confirming its repository access.
async fn user_grant(app: &TestApp, github: &MockServer, user: &TestUser, token: &str) {
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token(token)
        .unwrap();
    sqlx::query("INSERT INTO github_app_user_grants(user_id,github_user_id,github_login,access_token_enc) VALUES($1,123,'owner-login',$2)")
        .bind(user.id).bind(encrypted).execute(&app.db).await.unwrap();
    Mock::given(method("GET"))
        .and(path("/repos/acme/api"))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":1002})))
        .mount(github)
        .await;
}

async fn mint_vm(app: &TestApp, org: Uuid, workspace: Uuid, owner: Uuid) -> String {
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

fn thread(id: &str, repository: &str, resolved: bool, comments: Value) -> Value {
    json!({"data":{"node":{"id":id,"isResolved":resolved,
        "pullRequest":{"number":42,"repository":{"nameWithOwner":repository}},
        "comments":{"nodes":comments}}}})
}

async fn mount_thread(github: &MockServer, id: &str, body: Value, priority: u8) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_string_contains("...on PullRequestReviewThread"))
        .and(body_partial_json(json!({"variables":{"id":id}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .with_priority(priority)
        .mount(github)
        .await;
}

async fn publications(app: &TestApp, key: &str) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM issue_comment_publications p WHERE idempotency_key=$1",
    )
    .bind(key)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn agent_comments_replies_and_resolution_act_as_the_owner_and_replays_never_repost() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    user_grant(&app, &github, &owner, "ghu_owner").await;
    let vm = mint_vm(&app, org.id, w.id, owner.id).await;
    let comments = format!(
        "/api/internal/workspaces/{}/pull-requests/github/comments",
        w.id
    );
    let resolve = format!(
        "/api/internal/workspaces/{}/pull-requests/github/threads/resolve",
        w.id
    );

    // A PR discussion comment: the number is confirmed to be a PR, then the
    // marked payload is posted as the owner.
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .and(header("authorization", "Bearer ghu_owner"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number":42,"node_id":"PR_42","html_url":"https://github.com/acme/api/pull/42",
            "state":"open","head":{"sha":"abc"},"base":{"sha":"def"}})))
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_string_contains("@coderabbitai review"))
        .and(body_string_contains("<!-- g5-comment "))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id":7,"html_url":"https://github.com/acme/api/pull/42#issuecomment-7"})))
        .expect(1)
        .mount(&github)
        .await;
    let request = json!({"selector":"42","body":"@coderabbitai review","idempotencyKey":"k1"});
    let (status, body) = post(&app, &vm, &comments, request.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first = &body["data"];
    assert_eq!(first["state"], "published");
    assert_eq!(first["destination"], "pull_request");
    assert_eq!(first["origin"], "agent");
    assert_eq!(first["githubLogin"], "owner-login");
    assert_eq!(first["displayKey"], "acme/api#42");
    assert_eq!(first["body"], "@coderabbitai review");
    assert_eq!(
        first["externalCommentUrl"],
        "https://github.com/acme/api/pull/42#issuecomment-7"
    );
    // A replay returns the record without posting; another text is refused.
    let (status, body) = post(&app, &vm, &comments, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["id"], first["id"]);
    let (status, body) = post(
        &app,
        &vm,
        &comments,
        json!({"selector":"42","body":"something else","idempotencyKey":"k1"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let rows = publications(&app, "agent:k1").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["author_user_id"], json!(owner.id));
    assert_eq!(rows[0]["domain_id"], Value::Null);
    assert_eq!(rows[0]["external_issue_id"], "42");
    assert_eq!(
        rows[0]["locator"],
        json!({"repository":"acme/api","number":42})
    );

    // A reply whose answer is lost stays unknown; its replay finds the marker
    // in the thread instead of replying again.
    mount_thread(
        &github,
        "PRRT_1",
        thread("PRRT_1", "acme/api", false, json!([])),
        5,
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThreadReply"))
        .and(body_partial_json(json!({"variables":{"id":"PRRT_1"}})))
        .respond_with(ResponseTemplate::new(502))
        .expect(1)
        .mount(&github)
        .await;
    let reply = json!({"replyTo":"PRRT_1","body":"Fixed in abc","idempotencyKey":"k2"});
    let (status, body) = post(&app, &vm, &comments, reply.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["state"], "outcome_unknown");
    assert_eq!(body["data"]["destination"], "review_thread");
    assert_eq!(body["data"]["threadId"], "PRRT_1");
    let id = body["data"]["id"].as_str().unwrap().to_owned();
    mount_thread(
        &github,
        "PRRT_1",
        thread(
            "PRRT_1",
            "acme/api",
            false,
            json!([{"databaseId":12,"body":format!("Fixed in abc\n\n<!-- g5-comment {id} -->"),
                "url":"https://github.com/acme/api/pull/42#discussion_r12"}]),
        ),
        1,
    )
    .await;
    let (status, body) = post(&app, &vm, &comments, reply).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["id"], id);
    assert_eq!(body["data"]["state"], "published");
    assert_eq!(
        body["data"]["externalCommentUrl"],
        "https://github.com/acme/api/pull/42#discussion_r12"
    );
    // A recorded reply's destination and thread are frozen like its payload.
    for change in [
        "target_thread_id='PRRT_2'",
        "destination='pull_request', target_thread_id=NULL",
    ] {
        let error = sqlx::query(&format!(
            "UPDATE issue_comment_publications SET {change} WHERE id=$1::uuid"
        ))
        .bind(&id)
        .execute(&app.db)
        .await
        .expect_err(change);
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("23514"),
            "{change}: {error}"
        );
    }

    // A thread of another repository, or of another selected PR, is refused
    // before anything is recorded or sent.
    mount_thread(
        &github,
        "PRRT_9",
        thread("PRRT_9", "other/repo", false, json!([])),
        1,
    )
    .await;
    let (status, body) = post(
        &app,
        &vm,
        &comments,
        json!({"replyTo":"PRRT_9","body":"x","idempotencyKey":"k3"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = post(
        &app,
        &vm,
        &comments,
        json!({"selector":"7","replyTo":"PRRT_1","body":"x","idempotencyKey":"k4"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(publications(&app, "agent:k3").await.is_empty());
    assert!(publications(&app, "agent:k4").await.is_empty());
    // Unattached repositories and malformed requests never reach GitHub.
    for request in [
        json!({"selector":"https://github.com/other/repo/pull/1","body":"x","idempotencyKey":"k5"}),
        json!({"body":"x","idempotencyKey":"k5"}),
        json!({"replyTo":"not a thread","body":"x","idempotencyKey":"k5"}),
        json!({"selector":"42","body":"ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","idempotencyKey":"k5"}),
    ] {
        let (status, body) = post(&app, &vm, &comments, request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    assert!(publications(&app, "agent:k5").await.is_empty());

    // Resolution writes only when the state differs.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer ghu_owner"))
        .and(body_string_contains("{resolveReviewThread(input"))
        .and(body_partial_json(json!({"variables":{"id":"PRRT_1"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "resolveReviewThread":{"thread":{"id":"PRRT_1","isResolved":true}}}})))
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = post(
        &app,
        &vm,
        &resolve,
        json!({"thread":"PRRT_1","resolved":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        json!({"threadId":"PRRT_1","repository":"acme/api","number":42,"isResolved":true,"changed":true})
    );
    mount_thread(
        &github,
        "PRRT_2",
        thread("PRRT_2", "acme/api", true, json!([])),
        1,
    )
    .await;
    let (status, body) = post(
        &app,
        &vm,
        &resolve,
        json!({"thread":"PRRT_2","resolved":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["changed"], false);
    let (status, body) = post(
        &app,
        &vm,
        &resolve,
        json!({"thread":"PRRT_9","resolved":true}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Every write used the owner's own grant: no installation token was minted.
    let requests = github.received_requests().await.unwrap();
    assert!(!requests
        .iter()
        .any(|r| r.url.path().ends_with("/access_tokens")));
    assert!(requests.iter().all(|r| r
        .headers
        .get("authorization")
        .is_some_and(|v| v == "Bearer ghu_owner")));
    github.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn people_comment_as_themselves_and_need_their_own_github_connection() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let (owner, org, w) = fixture(&app).await;
    let comments = format!("/api/v1/workspaces/{}/pull-requests/github/comments", w.id);
    let request = json!({"selector":"42","body":"Looks good to me","idempotencyKey":"p1"});
    // The owner has not connected GitHub: refused before any GitHub request,
    // never falling back to the installation.
    let (status, body) = post(&app, &owner.jwt, &comments, request.clone()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("github_user_connection_required"),
        "{body}"
    );
    assert!(github.received_requests().await.unwrap().is_empty());
    assert!(publications(&app, "p1").await.is_empty());
    // A member with workspace write access posts under their own grant.
    let member =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesWrite]).await;
    user_grant(&app, &github, &member, "ghu_member").await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number":42,"html_url":"https://github.com/acme/api/pull/42","state":"open",
            "head":{"sha":"abc"},"base":{"sha":"def"}})))
        .mount(&github)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .and(header("authorization", "Bearer ghu_member"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "message":"Resource not accessible by integration"})))
        .expect(1)
        .mount(&github)
        .await;
    let (status, body) = post(&app, &member.jwt, &comments, request).await;
    // GitHub's refusal is recorded as failed, not reported as posted.
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["state"], "failed");
    assert_eq!(body["data"]["origin"], "person");
    assert!(body["data"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not accessible"));
    let rows = publications(&app, "p1").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["author_user_id"], json!(member.id));
    // A reader cannot write.
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let (status, body) = post(
        &app,
        &reader.jwt,
        &comments,
        json!({"selector":"42","body":"x","idempotencyKey":"p2"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!body.to_string().contains("ghu_"));
    github.verify().await;
    app.cleanup().await;
}
