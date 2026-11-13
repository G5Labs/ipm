//! Cross-workspace PR reads and repository listing (G5-Lean#830):
//! `g5.git.pr-read-repositories`, `g5.personal.pr-reads`, `g5.git.pr-list`,
//! `g5.git.pr-list.search`, `g5.git.pr-list.local` and `g5.git.pr-g5-identity`.
use crate::helpers;
use helpers::{test_config, TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn get(app: &TestApp, token: &str, path: &str) -> (StatusCode, Value) {
    let response = app
        .client
        .get(format!("{}{path}", app.base_url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn github_app(github: &MockServer) -> TestApp {
    let mut config = test_config();
    let mut app_config = helpers::github_app::github_app_config(github);
    app_config.web_base_url = "https://github.com".into();
    config.github_app = Some(app_config);
    config.auth.scm_github_api_base_url = github.uri();
    TestApp::spawn_with_config(config).await
}

/// Put `repo_id` on GitHub as acme/api, covered by an active App binding.
async fn bind_github(app: &TestApp, owner: Uuid, org: Uuid, repo_id: Uuid) {
    sqlx::query(
        "UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github' WHERE id=$1",
    )
    .bind(repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let identity = Uuid::new_v4();
    let binding = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO github_app_identities(id,org_id,kind) VALUES($1,$2,'platform_managed')",
    )
    .bind(identity)
    .bind(org)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO github_app_installation_bindings(id,org_id,app_identity_id,version,disposition,installation_id,account_id,account_login,permissions,provenance,created_by) VALUES($1,$2,$3,1,'active',555,44,'acme',$4,'coverage_change',$5)")
        .bind(binding).bind(org).bind(identity).bind(json!({"pull_requests":"read","contents":"read"})).bind(owner).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO github_app_installation_binding_repositories(org_id,binding_id,github_repo_id,full_name) VALUES($1,$2,1002,'acme/api')")
        .bind(org).bind(binding).execute(&app.db).await.unwrap();
}

async fn mount_token(github: &MockServer) {
    Mock::given(method("POST")).and(path("/app/installations/555/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"installation-read-only","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
        .mount(github).await;
}

async fn mount_pull(github: &MockServer, number: i64) {
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/api/pulls/{number}")))
        .and(header("authorization", "Bearer installation-read-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "node_id":format!("PR_{number}"),"number":number,"title":"Landing","state":"closed","merged":true,
            "user":{"login":"octo"},"head":{"ref":"issue-816-landing","sha":"abc"},"base":{"ref":"main","sha":"def"}})))
        .mount(github)
        .await;
}

fn node(number: i64) -> Value {
    json!({"id":format!("PR_{number}"),"number":number,"title":format!("PR {number}"),"url":"https://github.com/acme/api/pull/x",
        "state":"MERGED","isDraft":false,"headRefName":"issue-816-landing","headRefOid":"a".repeat(40),"baseRefName":"main",
        "createdAt":"2026-10-20T00:00:00Z","updatedAt":"2026-10-21T00:00:00Z","mergedAt":"2026-10-21T00:00:00Z",
        "closedAt":"2026-10-21T00:00:00Z","author":{"login":"octo"},"labels":{"nodes":[]},
        "repository":{"nameWithOwner":"acme/api"}})
}

/// The owner's personal workspace beside a Base whose code member sits at `code`.
async fn personal_with_base(
    app: &TestApp,
    owner: &TestUser,
    org: &TestOrg,
) -> (TestWorkspace, TestWorkspace) {
    let base = TestWorkspace::create_base(app, org).await;
    sqlx::query("UPDATE workspace_repos SET root_path = 'code' WHERE workspace_id = $1")
        .bind(base.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = get(app, &owner.jwt, &format!("/api/v1/orgs/{}/me", org.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = Uuid::parse_str(body["data"]["workspaceId"].as_str().unwrap()).unwrap();
    let repo_id = Uuid::parse_str(body["data"]["repoId"].as_str().unwrap()).unwrap();
    let name = body["data"]["name"].as_str().unwrap().to_string();
    let path = app.repo_path(repo_id).await;
    let head_commit = g5_scm::GitRepo::open(&path)
        .unwrap()
        .read_branch_oid(&name)
        .unwrap()
        .unwrap();
    (
        base,
        TestWorkspace {
            id,
            org_id: org.id,
            repo_id,
            head_commit,
            name,
        },
    )
}

async fn vm_token(app: &TestApp, org: Uuid, workspace: &TestWorkspace, owner: Uuid) -> String {
    let runtime = TestWorkspaceRuntimePeer::attach(app, workspace, owner).await;
    app.state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org, workspace.id, owner, runtime.vm_id(), 1, 3600)
        .unwrap()
}

/// A stored G5 PR from `source` into `target` covering `repo` on `branch`,
/// optionally linked to GitHub PR `number`.
#[allow(clippy::too_many_arguments)]
async fn stored_pr(
    app: &TestApp,
    org: Uuid,
    source: Uuid,
    target: Uuid,
    repo: Uuid,
    branch: &str,
    title: &str,
    status: &str,
    created_by: Uuid,
    number: Option<i64>,
) -> Uuid {
    let id = Uuid::now_v7();
    // The PR changes `repo`: its source and target versions differ.
    let head = |workspace: Uuid, branch: &str, sha: char| json!([{"repoId":repo,"workspaceId":workspace,"branchName":branch,"headCommit":sha.to_string().repeat(40)}]);
    sqlx::query(
        "INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,status,local_merge_status,title,source_heads,target_heads,last_preflight_fingerprint,created_by,merged_at,description_override) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'f',$10,CASE WHEN $5='merged' THEN now() END,$11)",
    )
    .bind(id).bind(org).bind(source).bind(target).bind(status)
    .bind(match status { "open" => "ready", other => other })
    .bind(title).bind(head(source, branch, 'a')).bind(head(target, "main", 'b')).bind(created_by)
    .bind(format!("Description of {title}"))
    .execute(&app.db).await.unwrap();
    if let Some(number) = number {
        sqlx::query(
            "INSERT INTO workspace_pull_request_provider_repos(org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_pr_url,provider_status) \
             VALUES($1,$2,$3,'github','acme/api',$4,'main',$5,$6,'merged')",
        )
        .bind(org).bind(id).bind(repo).bind(branch).bind(number)
        .bind(format!("https://github.com/acme/api/pull/{number}"))
        .execute(&app.db).await.unwrap();
    }
    // Distinct creation times keep the newest-first order deterministic.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    id
}

#[tokio::test]
async fn explicit_pr_targets_cannot_join_personal_and_ordinary_workspaces() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (_, personal) = personal_with_base(&app, &owner, &org).await;
    let ordinary = TestWorkspace::create(&app, &org).await;

    // Use the real personal creator: its private member is distinct from
    // ordinary members, even when both workspaces belong to the same owner.
    // An explicit target bypasses the target picker, but never this boundary.
    for (source, target) in [(&personal, &ordinary), (&ordinary, &personal)] {
        let vm = vm_token(&app, org.id, source, owner.id).await;
        for (prefix, token) in [("/api/v1", &owner.jwt), ("/api/internal", &vm)] {
            let response = app
                .client
                .post(format!(
                    "{}{prefix}/workspaces/{}/pull-requests",
                    app.base_url, source.id
                ))
                .bearer_auth(token)
                .json(&json!({"targetWorkspaceId": target.id, "save": false}))
                .send()
                .await
                .unwrap();
            let status = response.status();
            let body: Value = response.json().await.unwrap();
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{prefix}: {body}");
            assert!(
                body.to_string().contains("same complete repository vector"),
                "{prefix}: {body}"
            );
        }
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_pull_requests WHERE org_id=$1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 0, "refused creation must not persist a PR");
    app.cleanup().await;
}

#[tokio::test]
async fn personal_workspaces_read_base_prs_but_never_write_through_them() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (base, personal) = personal_with_base(&app, &owner, &org).await;
    bind_github(&app, owner.id, org.id, base.repo_id).await;
    mount_token(&github).await;
    mount_pull(&github, 42).await;
    let info = |w: Uuid| format!("/api/v1/workspaces/{w}/pull-requests/info");

    // The owner reads a Base PR from the personal workspace, by number and URL.
    for query in [
        "selector=42",
        "selector=42&repo=acme/api",
        "selector=https://github.com/acme/api/pull/42",
    ] {
        let (status, body) = get(&app, &owner.jwt, &format!("{}?{query}", info(personal.id))).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(body["data"]["pullRequest"]["number"], 42);
        assert_eq!(body["data"]["pullRequest"]["g5PullRequests"], json!([]));
    }
    // So does the personal VM.
    let vm = vm_token(&app, org.id, &personal, owner.id).await;
    let (status, body) = get(
        &app,
        &vm,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/info?selector=42",
            personal.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // A repository outside Base stays refused.
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?selector=42&repo=other/repo", info(personal.id)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // Writes stay with members: the agent cannot comment through a reference repository.
    let response = app
        .client
        .post(format!(
            "{}/api/internal/workspaces/{}/pull-requests/github/comments",
            app.base_url, personal.id
        ))
        .bearer_auth(&vm)
        .json(&json!({"selector":"42","repo":"acme/api","body":"hello","idempotencyKey":"k1"}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string().contains("No matching GitHub repository"),
        "{body}"
    );
    assert!(github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|request| request.method.as_str() == "GET"
            || request.url.path().ends_with("/access_tokens")));
    // Base membership is read per request: once Base drops the repository,
    // the personal read is refused without a restart.
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id)
        .bind(base.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    // Its only member left is not on GitHub, so a bare number, always
    // GitHub's, names nothing there; G5 PRs are named `PR######`
    // (`g5.git.pr-local-number`).
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("{}?selector=42", info(personal.id)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("PR and six hex digits"), "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn repository_listing_follows_github_and_names_only_readable_g5_prs() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let w = TestWorkspace::create(&app, &org).await;
    let hidden = TestWorkspace::create(&app, &org).await;
    let hidden_target = TestWorkspace::create(&app, &org).await;
    bind_github(&app, owner.id, org.id, w.repo_id).await;
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let visible = stored_pr(
        &app,
        org.id,
        w.id,
        base.id,
        w.repo_id,
        "issue-816-landing",
        "Landing",
        "merged",
        owner.id,
        Some(822),
    )
    .await;
    // An ordinary PR outside Base remains readable by all eligible members.
    let formerly_hidden = stored_pr(
        &app,
        org.id,
        hidden.id,
        hidden_target.id,
        w.repo_id,
        "issue-816-landing",
        "Hidden",
        "closed",
        owner.id,
        Some(822),
    )
    .await;
    mount_token(&github).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer installation-read-only"))
        .and(body_string_contains("pullRequests(first:$first"))
        .and(body_partial_json(
            json!({"variables":{"owner":"acme","name":"api","first":30,
            "states":null,"head":"issue-816-landing","base":null}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{
            "pullRequests":{"pageInfo":{"hasNextPage":false},"nodes":[node(822), node(900)]}}}})),
        )
        .mount(&github)
        .await;
    let list =
        |w: Uuid, query: &str| format!("/api/v1/workspaces/{w}/pull-requests/github?{query}");

    let (status, body) = get(
        &app,
        &reader.jwt,
        &list(w.id, "head=issue-816-landing&state=all"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["source"], "github");
    assert_eq!(data["repository"], "acme/api");
    assert_eq!(data["hasMore"], false);
    let pulls = data["pullRequests"].as_array().unwrap();
    assert_eq!(
        pulls.len(),
        2,
        "GitHub decides membership, including PRs opened outside G5"
    );
    assert_eq!(pulls[0]["number"], 822);
    assert_eq!(pulls[0]["url"], "https://github.com/acme/api/pull/822");
    // Both ordinary workspaces are readable by the member, newest PR first.
    assert_eq!(pulls[0]["g5PullRequests"].as_array().unwrap().len(), 2);
    assert_eq!(pulls[0]["g5PullRequests"][0]["id"], json!(formerly_hidden));
    assert_eq!(pulls[0]["g5PullRequests"][1]["id"], json!(visible));
    assert!(pulls.iter().all(|pull| pull["tracker"] == "github"));
    assert_eq!(pulls[1]["g5PullRequests"], json!([]));
    // An organization owner reads every workspace, newest G5 PR first.
    let (status, body) = get(
        &app,
        &owner.jwt,
        &list(w.id, "head=issue-816-landing&state=all"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let links = body["data"]["pullRequests"][0]["g5PullRequests"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(links.len(), 2, "{links:?}");
    assert_eq!(links[0]["sourceWorkspaceId"], json!(hidden.id));
    assert_eq!(links[1]["id"], json!(visible));
    // A live view names the same G5 PRs.
    mount_pull(&github, 822).await;
    let (status, body) = get(
        &app,
        &reader.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/info?selector=822",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["g5PullRequests"][0]["id"],
        json!(formerly_hidden)
    );
    assert_eq!(
        body["data"]["pullRequest"]["g5PullRequests"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Refusals happen before any GitHub request.
    let before = github.received_requests().await.unwrap().len();
    for query in [
        "limit=101",
        "search=repo:other/repo",
        "state=draft",
        "head=two%20words",
        "repo=other/repo",
    ] {
        let (status, body) = get(&app, &owner.jwt, &list(w.id, query)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
    }
    assert_eq!(github.received_requests().await.unwrap().len(), before);
    app.cleanup().await;
}

#[tokio::test]
async fn agent_pr_searches_spend_the_shared_search_budget_and_people_do_not() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let w = TestWorkspace::create(&app, &org).await;
    bind_github(&app, owner.id, org.id, w.repo_id).await;
    mount_token(&github).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("search(query:$q,type:ISSUE"))
        .and(body_partial_json(json!({"variables":{
            "q":"repo:acme/api is:pr is:merged author:octo label:\"bug\" label:\"ui\" landing sort:created-desc","first":5}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"search":{
            "issueCount":7,"nodes":[node(822)]}}})))
        .mount(&github)
        .await;
    let query = "state=merged&author=octo&label=bug,ui&search=landing&limit=5";
    let vm = {
        let host = Uuid::new_v4();
        let vm = Uuid::new_v4();
        sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat) VALUES($1,$2,'us-central1','ready',now())")
            .bind(host).bind(org.id).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at) VALUES($1,$2,$3,$4,1,'running',now()+interval '1 hour')")
            .bind(org.id).bind(w.id).bind(host).bind(vm.to_string()).execute(&app.db).await.unwrap();
        app.state
            .jwt_minter
            .as_ref()
            .unwrap()
            .mint_workspace_incarnation(org.id, w.id, owner.id, vm, 1, 3600)
            .unwrap()
    };
    let searches = || async {
        github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/graphql")
            .count()
    };
    let internal = format!(
        "/api/internal/workspaces/{}/pull-requests/github?{query}",
        w.id
    );
    for _ in 0..g5_api::services::external_issues::AGENT_SEARCHES_PER_WORKSPACE {
        let (status, body) = get(&app, &vm, &internal).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["hasMore"], true);
        assert_eq!(body["data"]["pullRequests"][0]["number"], 822);
    }
    let spent = searches().await;
    let (status, body) = get(&app, &vm, &internal).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(
        searches().await,
        spent,
        "a refused search sends nothing to GitHub"
    );
    // A plain listing is not a search and is not limited.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("pullRequests(first:$first"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{
            "pullRequests":{"pageInfo":{"hasNextPage":false},"nodes":[]}}}})),
        )
        .mount(&github)
        .await;
    let (status, body) = get(
        &app,
        &vm,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/github?head=x",
            w.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // People's searches are not limited by G5.
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests/github?{query}", w.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn listing_without_github_integration_answers_from_stored_prs() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let w = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let older = stored_pr(
        &app,
        org.id,
        other.id,
        base.id,
        w.repo_id,
        "issue-1-docs",
        "Fix docs typo",
        "merged",
        owner.id,
        None,
    )
    .await;
    let newer = stored_pr(
        &app,
        org.id,
        w.id,
        base.id,
        w.repo_id,
        "issue-830-list",
        "List 50% faster",
        "open",
        owner.id,
        Some(7),
    )
    .await;
    // A PR that does not cover this repository is never listed.
    stored_pr(
        &app,
        org.id,
        other.id,
        w.id,
        base.repo_id,
        "issue-830-list",
        "Elsewhere",
        "open",
        owner.id,
        None,
    )
    .await;
    // A PR that records this repository but leaves it unchanged: identical
    // versions, or a preflight that finds it identical or already merged. GitHub
    // would have no PR here, so neither does the stored listing.
    for (target_sha, outcome) in [
        ('a', "fastForward"),
        ('b', "identical"),
        ('b', "sourceAlreadyMerged"),
    ] {
        let unchanged = stored_pr(
            &app,
            org.id,
            other.id,
            base.id,
            w.repo_id,
            "issue-830-list",
            "Unchanged here",
            "open",
            owner.id,
            None,
        )
        .await;
        sqlx::query("UPDATE workspace_pull_requests SET status='closed', target_heads=jsonb_set(target_heads,'{0,headCommit}',to_jsonb(repeat($2,40))), preflight=jsonb_build_object('repoResults',jsonb_build_array(jsonb_build_object('repoId',$3::text,'outcome',$4::text))) WHERE id=$1")
            .bind(unchanged).bind(target_sha.to_string()).bind(w.repo_id.to_string()).bind(outcome)
            .execute(&app.db).await.unwrap();
    }
    let list = |query: &str| format!("/api/v1/workspaces/{}/pull-requests/github?{query}", w.id);
    let ids = |body: &Value| {
        body["data"]["pullRequests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].clone())
            .collect::<Vec<_>>()
    };

    // A repository not on GitHub: G5's stored PRs, newest first, and why.
    let (status, body) = get(&app, &owner.jwt, &list("state=all")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["source"], "g5");
    assert_eq!(body["data"]["fallback"], "This repository is not on GitHub");
    // Not expected on GitHub, so `g5d` shows no notice (g5.git.pr-no-github).
    assert_eq!(body["data"]["githubExpected"], false);
    assert_eq!(ids(&body), vec![json!(newer), json!(older)]);
    let row = &body["data"]["pullRequests"][0];
    assert_eq!(row["source"], "g5");
    assert_eq!(row["state"], "OPEN");
    assert_eq!(row["number"], 7);
    assert_eq!(row["headRefName"], "issue-830-list");
    assert_eq!(row["headRefOid"], "a".repeat(40));
    assert_eq!(row["baseRefName"], "main");
    assert_eq!(row["g5PullRequests"][0]["sourceWorkspaceId"], json!(w.id));
    let local_part = owner.email.split('@').next().unwrap().to_owned();
    assert_eq!(row["author"]["login"], json!(local_part));
    // Filters: state (closed includes merged), branches, author and every search word.
    for (query, expected) in [
        ("state=open".to_owned(), vec![json!(newer)]),
        ("state=closed".to_owned(), vec![json!(older)]),
        ("state=all&head=issue-1-docs".to_owned(), vec![json!(older)]),
        (
            "state=all&base=main".to_owned(),
            vec![json!(newer), json!(older)],
        ),
        ("state=all&base=other".to_owned(), vec![]),
        (
            format!("state=all&author={local_part}"),
            vec![json!(newer), json!(older)],
        ),
        (
            "state=all&author=@me".to_owned(),
            vec![json!(newer), json!(older)],
        ),
        ("state=all&author=nobody".to_owned(), vec![]),
        (
            "state=all&search=docs%20TYPO".to_owned(),
            vec![json!(older)],
        ),
        ("state=all&search=50%25".to_owned(), vec![json!(newer)]),
        (
            "state=all&search=description%20faster".to_owned(),
            vec![json!(newer)],
        ),
        ("state=all&limit=1".to_owned(), vec![json!(newer)]),
    ] {
        let (status, body) = get(&app, &owner.jwt, &list(&query)).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(ids(&body), expected, "{query}");
        if query.ends_with("limit=1") {
            assert_eq!(body["data"]["hasMore"], true);
        }
    }
    // Labels are GitHub's.
    let (status, body) = get(&app, &owner.jwt, &list("label=bug")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // Refused without blaming a GitHub this repository never had.
    assert!(!body.to_string().contains("GitHub"), "{body}");
    // The same rule for a GitHub repository whose organization has no App
    // binding and whose owner has no GitHub connection.
    sqlx::query(
        "UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github' WHERE id=$1",
    )
    .bind(w.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = get(&app, &owner.jwt, &list("state=all&repo=acme/api")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["fallback"],
        "GitHub is not connected for this repository"
    );
    assert_eq!(body["data"]["githubExpected"], true);
    assert_eq!(body["data"]["repository"], "acme/api");
    assert_eq!(ids(&body), vec![json!(newer), json!(older)]);
    // An integration that exists but fails is an error, never a fallback.
    bind_github(&app, owner.id, org.id, w.repo_id).await;
    sqlx::query(
        "UPDATE github_app_installation_bindings SET disposition='disconnected' WHERE org_id=$1",
    )
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = get(&app, &owner.jwt, &list("state=all")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(github.received_requests().await.unwrap().is_empty());
    app.cleanup().await;
}

/// `g5.git.pr-reads`: whoever can open a workspace reads every PR into or out
/// of it, from any workspace, through every read: view by G5 ID, the
/// `--all`/`--workspace` listings, descriptions and checks. A workspace the
/// reader cannot open stays invisible, and writes keep their owners.
#[tokio::test]
async fn pr_reads_follow_workspace_read_access_from_any_workspace() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let shared = TestWorkspace::create(&app, &org).await;
    let hidden = TestWorkspace::create(&app, &org).await;
    let hidden_target = TestWorkspace::create(&app, &org).await;
    let mine = TestWorkspace::create(&app, &org).await;
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    // The reader's own workspace, whose agent reads as the reader: a plain
    // member, not an organization administrator.
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(mine.id)
        .bind(reader.id)
        .execute(&app.db)
        .await
        .unwrap();
    let visible = stored_pr(
        &app,
        org.id,
        shared.id,
        base.id,
        shared.repo_id,
        "feature",
        "Visible",
        "open",
        owner.id,
        None,
    )
    .await;
    let other_pr = stored_pr(
        &app,
        org.id,
        hidden.id,
        hidden_target.id,
        hidden.repo_id,
        "secret",
        "Other workspace",
        "open",
        owner.id,
        None,
    )
    .await;
    let vm = crate::api_pull_request_reads::mint_vm(&app, org.id, mine.id, reader.id).await;
    let route = |leaf: &str| format!("/api/internal/workspaces/{}/pull-requests/{leaf}", mine.id);

    // View by G5 ID, though `mine` is neither side of the PR.
    let (status, body) = get(&app, &vm, &route(&format!("info?selector={visible}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["source"], "g5");
    assert_eq!(body["data"]["pullRequest"]["id"], json!(visible));
    assert_eq!(
        body["data"]["pullRequest"]["sourceWorkspaceId"],
        json!(shared.id)
    );
    let (status, _) = get(&app, &vm, &route(&format!("info?selector={other_pr}"))).await;
    assert_eq!(status, StatusCode::OK);

    // `--all` and `--workspace` listings.
    let ids = |body: &Value| {
        body["data"]["pullRequests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].clone())
            .collect::<Vec<_>>()
    };
    let (status, body) = get(&app, &vm, &route("info?all=true&state=all")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), vec![json!(other_pr), json!(visible)], "{body}");
    assert_eq!(body["data"]["hasMore"], false);
    let (status, body) = get(
        &app,
        &vm,
        &route(&format!(
            "info?workspace={}&state=open&search=visi",
            shared.name.to_uppercase()
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), vec![json!(visible)]);
    let (status, body) = get(&app, &vm, &route("info?all=true&state=merged")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), Vec::<Value>::new());
    let (status, _) = get(&app, &vm, &route(&format!("info?workspace={}", hidden.id))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(&app, &vm, &route("info?all=true&limit=101")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Descriptions, single and batch.
    let (status, body) = get(&app, &vm, &route(&format!("{visible}/description"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["description"], "Description of Visible");
    let (status, _) = get(&app, &vm, &route(&format!("{other_pr}/description"))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get(&app, &vm, &route(&format!("descriptions?ids={visible}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"][0]["description"], "Description of Visible");
    let (status, _) = get(
        &app,
        &vm,
        &route(&format!("descriptions?ids={visible},{other_pr}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Checks by G5 ID: this local PR has none to observe. Checks read the
    // PR's preflight, which every real PR has.
    sqlx::query("UPDATE workspace_pull_requests SET preflight=jsonb_build_object('sourceKind','workspace','sourceWorkspaceId',source_workspace_id,'targetWorkspaceId',target_workspace_id,'decision','fastForwardOrNoOp','canDirectMerge',true,'preflightFingerprint','f','repoResults','[]'::jsonb,'message','ready') WHERE id = ANY($1)")
        .bind(vec![visible, other_pr])
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = get(
        &app,
        &vm,
        &route(&format!("info?checks=true&selector={visible}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["noExternalChecks"], true, "{body}");
    let (status, _) = get(
        &app,
        &vm,
        &route(&format!("info?checks=true&selector={other_pr}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Writes keep their owners: `mine` cannot close a PR it is not a side of.
    let response = app
        .client
        .post(format!(
            "{}/api/internal/workspace-agent/{}/pull-requests/{visible}/close",
            app.base_url, mine.id
        ))
        .bearer_auth(&vm)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success(), "{}", response.status());
    let status: String =
        sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id=$1")
            .bind(visible)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "open");

    // A GitHub PR in a repository of another readable workspace, named by
    // URL or --repo. A bare number still means this workspace's repository.
    bind_github(&app, owner.id, org.id, shared.repo_id).await;
    mount_token(&github).await;
    mount_pull(&github, 42).await;
    for query in [
        "selector=https://github.com/acme/api/pull/42",
        "selector=42&repo=acme/api",
    ] {
        let (status, body) = get(&app, &vm, &route(&format!("info?{query}"))).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(body["data"]["pullRequest"]["number"], 42);
    }
    // This workspace's own repository is not on GitHub: a bare number names
    // no PR there (G5 PRs are `PR######`), never another workspace's GitHub PR.
    let (status, body) = get(&app, &vm, &route("info?selector=42")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("PR and six hex digits"), "{body}");
    app.cleanup().await;
}

async fn pr_number(app: &TestApp, id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT number FROM workspace_pull_requests WHERE id=$1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// `g5.git.pr-repository`, `g5.git.pr-adapter`, `g5.git.pr-cli.selectors`,
/// `g5.git.pr-local-number`, `g5.git.pr-local-conversation`: a repository not
/// on GitHub has local repository pull requests, read, checked and listed by
/// their G5 PR's name `PR######` (never a bare number), with no conversation.
#[tokio::test]
async fn local_repository_pull_requests_are_read_checked_and_listed_by_g5_number() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let w = TestWorkspace::create(&app, &org).await;
    let repo: String = sqlx::query_scalar("SELECT name FROM repos WHERE id=$1")
        .bind(w.repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let first = stored_pr(
        &app,
        org.id,
        w.id,
        base.id,
        w.repo_id,
        "issue-1-docs",
        "First",
        "merged",
        owner.id,
        None,
    )
    .await;
    let pr = stored_pr(
        &app,
        org.id,
        w.id,
        base.id,
        w.repo_id,
        "issue-9-docs",
        "Docs work",
        "open",
        owner.id,
        None,
    )
    .await;
    // Real PRs carry preflight evidence; checks read it.
    let preflight = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":base.id,
        "decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"f","repoResults":[],"message":"ready"});
    sqlx::query("UPDATE workspace_pull_requests SET preflight=$2 WHERE id=$1")
        .bind(pr)
        .bind(preflight)
        .execute(&app.db)
        .await
        .unwrap();
    // Numbers are random, from 1 to 0xffffff and unique in the organization;
    // the name pads them to six lowercase hex digits.
    let number = pr_number(&app, pr).await;
    let first_number = pr_number(&app, first).await;
    assert!((1..=0xff_ffff).contains(&number) && number != first_number);
    let name = format!("PR{number:06x}");
    assert_eq!(name.len(), 8);
    let info = |query: &str| format!("/api/v1/workspaces/{}/pull-requests/info?{query}", w.id);

    // The G5 PR names its local repository pull request.
    let (status, body) = get(&app, &owner.jwt, &info(&format!("selector={pr}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pull = &body["data"]["pullRequest"];
    assert_eq!(pull["number"], name);
    assert_eq!(
        pull["repositoryPullRequests"],
        json!([{"repoId":w.repo_id,"repository":repo,"adapter":"g5","number":null,"url":null,
                "state":"OPEN","headRefName":"issue-9-docs","baseRefName":"main",
                "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40)}])
    );
    assert_eq!(pull["sourceWorkspaceDeleted"], false);
    // By its name, in any case: the G5 PR.
    for query in [
        format!("selector={name}"),
        format!("selector=pr{number:06X}"),
    ] {
        let (status, body) = get(&app, &owner.jwt, &info(&query)).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(body["data"]["source"], "g5", "{body}");
        assert_eq!(body["data"]["pullRequest"]["id"], json!(pr));
    }
    // A name no readable G5 PR has is not found.
    let (status, _) = get(
        &app,
        &owner.jwt,
        &info(&format!(
            "selector=PR{:06x}",
            (1..).find(|n| ![number, first_number].contains(n)).unwrap()
        )),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // A bare number is GitHub's, so in a repository not on GitHub it is
    // refused, pointing to the `PR` form.
    for query in [
        format!("selector={number}&repo={repo}"),
        format!("selector=%23{number}"),
    ] {
        let (status, body) = get(&app, &owner.jwt, &info(&query)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
        assert!(body.to_string().contains("PR and six hex digits"), "{body}");
    }
    // No conversation: refused before any effect.
    let (status, body) = get(
        &app,
        &owner.jwt,
        &info(&format!("selector={name}&comments=true")),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let response = app
        .client
        .post(format!(
            "{}/api/v1/workspaces/{}/pull-requests/github/comments",
            app.base_url, w.id
        ))
        .bearer_auth(&owner.jwt)
        .json(&json!({"selector":name,"body":"hello","idempotencyKey":"local-1"}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("no PR comments"), "{body}");
    // Checks: none apply, by name and by ID.
    for query in [
        format!("checks=true&selector={name}"),
        format!("checks=true&selector={pr}"),
    ] {
        let (status, body) = get(&app, &owner.jwt, &info(&query)).await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(body["data"]["noExternalChecks"], true, "{body}");
        let repository = &body["data"]["repositories"][0];
        assert_eq!(repository["tracker"], "g5", "{body}");
        assert_eq!(repository["number"], Value::Null, "{body}");
        assert_eq!(repository["g5Number"], name, "{body}");
    }
    // Listing the repository: its local repository pull requests by name.
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests/github?state=all", w.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body["data"]["pullRequests"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{body}");
    assert_eq!(rows[0]["tracker"], "g5");
    assert_eq!(rows[0]["number"], Value::Null);
    assert_eq!(rows[0]["g5Number"], name);
    assert_eq!(rows[0]["g5PullRequests"][0]["number"], name);
    assert!(github.received_requests().await.unwrap().is_empty());
    app.cleanup().await;
}

/// `g5.git.pr-list`: a GitHub repository's listing shows its local
/// repository pull requests beside GitHub's, newest first, under one cap.
#[tokio::test]
async fn github_listing_shows_the_repositorys_local_pull_requests_beside_githubs() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let w = TestWorkspace::create(&app, &org).await;
    bind_github(&app, owner.id, org.id, w.repo_id).await;
    // Neither workspace tracks the repository remotely, so this PR's member
    // is not published: its repository pull request is local.
    let local = stored_pr(
        &app,
        org.id,
        w.id,
        base.id,
        w.repo_id,
        "issue-816-landing",
        "Local landing",
        "open",
        owner.id,
        None,
    )
    .await;
    let number = pr_number(&app, local).await;
    mount_token(&github).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("pullRequests(first:$first"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{
            "pullRequests":{"pageInfo":{"hasNextPage":false},"nodes":[node(822)]}}}})),
        )
        .mount(&github)
        .await;
    let list = |query: &str| format!("/api/v1/workspaces/{}/pull-requests/github?{query}", w.id);
    let (status, body) = get(&app, &owner.jwt, &list("state=all")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["source"], "github");
    let rows = body["data"]["pullRequests"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{body}");
    // The local one was created after GitHub's. It is named by its G5 PR,
    // never by a number beside GitHub's (g5.git.pr-local-number).
    assert_eq!(rows[0]["tracker"], "g5");
    assert_eq!(rows[0]["number"], Value::Null);
    assert_eq!(rows[0]["g5Number"], format!("PR{number:06x}"));
    assert_eq!(rows[0]["id"], json!(local));
    assert_eq!(rows[1]["tracker"], "github");
    assert_eq!(rows[1]["number"], 822);
    assert_eq!(body["data"]["hasMore"], false);
    let (status, body) = get(&app, &owner.jwt, &list("state=all&limit=1")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["pullRequests"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["hasMore"], true);
    // Labels are GitHub's: local rows never match them.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("search(query:$q"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"search":{"issueCount":1,"nodes":[node(822)]}}})),
        )
        .mount(&github)
        .await;
    let (status, body) = get(&app, &owner.jwt, &list("state=all&label=bug")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["pullRequests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["tracker"] == "github"));
    // Its name selects it in a GitHub repository too, and refuses a
    // conversation before anything reaches GitHub (g5.git.pr-local-number).
    let info = |query: &str| format!("/api/v1/workspaces/{}/pull-requests/info?{query}", w.id);
    let (status, body) = get(&app, &owner.jwt, &info(&format!("selector=PR{number:06x}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["pullRequest"]["id"], json!(local));
    let response = app
        .client
        .post(format!(
            "{}/api/v1/workspaces/{}/pull-requests/github/comments",
            app.base_url, w.id
        ))
        .bearer_auth(&owner.jwt)
        .json(
            &json!({"selector":format!("PR{number:06x}"),"body":"hello","idempotencyKey":"named-1"}),
        )
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("no PR comments"), "{body}");
    // Once both workspaces track the repository remotely, publication would
    // publish the member to GitHub, so it is no longer a local row.
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT org_id,$1,repo_id,'shared',role FROM workspace_repos WHERE workspace_id=$2 AND repo_id=$3 ON CONFLICT DO NOTHING")
        .bind(base.id).bind(w.id).bind(w.repo_id).execute(&app.db).await.unwrap();
    for workspace in [w.id, base.id] {
        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,'main')")
            .bind(org.id).bind(workspace).bind(w.repo_id).execute(&app.db).await.unwrap();
    }
    let (status, body) = get(&app, &owner.jwt, &list("state=all")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["data"]["pullRequests"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["tracker"] == "github"),
        "{body}"
    );
    app.cleanup().await;
}

/// `g5.workspace.delete.pull-requests`, `g5.git.pr-reads`: deletion closes a
/// workspace's open PRs with an event that names it, and the PRs stay
/// readable whether the source, the target or both are deleted, to whoever
/// could read the deleted workspace.
#[tokio::test]
async fn deletion_closes_open_prs_and_keeps_them_readable_from_either_side() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let parent = TestWorkspace::create(&app, &org).await;
    let child = TestWorkspace::create(&app, &org).await;
    let other_source = TestWorkspace::create(&app, &org).await;
    let other_target = TestWorkspace::create(&app, &org).await;
    let secret = TestWorkspace::create(&app, &org).await;
    let secret_target = TestWorkspace::create(&app, &org).await;
    let pr = |source: Uuid, target: Uuid, title: &'static str, status: &'static str| {
        let app = &app;
        let owner = owner.id;
        let org = org.id;
        let repo = child.repo_id;
        async move {
            stored_pr(
                app, org, source, target, repo, "b", title, status, owner, None,
            )
            .await
        }
    };
    let from_child = pr(child.id, parent.id, "From the child", "open").await;
    let merged = pr(child.id, other_target.id, "Already merged", "merged").await;
    let into_target = pr(other_source.id, other_target.id, "Into a target", "open").await;
    let hidden = pr(secret.id, secret_target.id, "Hidden", "open").await;
    let delete = |workspace: Uuid| {
        let client = app.authenticated_client(&owner);
        async move {
            let response = client
                .post_json(
                    &format!("/api/v1/workspaces/{workspace}/delete"),
                    &json!({}),
                )
                .await;
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
        }
    };
    let state = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_as::<_, (String, String, Option<String>, Option<Uuid>)>(
                "SELECT pr.status, pr.local_merge_status, e.message, e.actor_id \
                 FROM workspace_pull_requests pr \
                 LEFT JOIN workspace_pull_request_events e \
                   ON e.pull_request_id = pr.id AND e.event_type = 'closed' \
                 WHERE pr.id = $1",
            )
            .bind(id)
            .fetch_one(&db)
            .await
            .unwrap()
        }
    };

    // The source is deleted: its open PR closes, naming the deletion; the
    // merged one is untouched.
    delete(child.id).await;
    assert_eq!(
        state(from_child).await,
        (
            "closed".into(),
            "closed".into(),
            Some("Closed because its source workspace was deleted".into()),
            Some(owner.id)
        )
    );
    assert_eq!(state(merged).await.0, "merged");
    assert_eq!(state(merged).await.2, None);
    // The target is deleted.
    delete(other_target.id).await;
    assert_eq!(
        state(into_target).await.2,
        Some("Closed because its target workspace was deleted".into())
    );
    // Both are deleted: still readable.
    delete(other_source.id).await;
    let info = |query: String| {
        format!(
            "/api/v1/workspaces/{}/pull-requests/info?{query}",
            parent.id
        )
    };
    for (id, source_deleted, target_deleted) in [
        (from_child, true, false),
        (merged, true, true),
        (into_target, true, true),
    ] {
        let (status, body) = get(&app, &reader.jwt, &info(format!("selector={id}"))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let pull = &body["data"]["pullRequest"];
        assert_eq!(pull["sourceWorkspaceDeleted"], source_deleted, "{body}");
        assert_eq!(pull["targetWorkspaceDeleted"], target_deleted, "{body}");
    }
    // Listings include them: every readable PR, and a deleted workspace's by ID.
    let ids = |body: &Value| {
        body["data"]["pullRequests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].clone())
            .collect::<Vec<_>>()
    };
    let (status, body) = get(&app, &reader.jwt, &info("all=true&state=all".into())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let all = ids(&body);
    for id in [from_child, merged, into_target] {
        assert!(all.contains(&json!(id)), "{id} in {body}");
    }
    assert!(all.contains(&json!(hidden)), "{body}");
    let (status, body) = get(
        &app,
        &reader.jwt,
        &info(format!("workspace={}&state=all", child.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), vec![json!(merged), json!(from_child)]);
    // The parent's own listing shows the closed PR from its deleted child,
    // and no surface that acts on open PRs does.
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests?role=target&status=all",
            parent.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"][0]["id"], json!(from_child), "{body}");
    assert_eq!(body["data"][0]["sourceWorkspaceDeleted"], true);
    let (status, body) = get(
        &app,
        &owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests?role=target&status=open",
            parent.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"], json!([]), "{body}");

    // Every ordinary workspace's PRs remain organization-readable after deletion.
    delete(secret.id).await;
    let (status, _) = get(&app, &reader.jwt, &info(format!("selector={hidden}"))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get(&app, &owner.jwt, &info(format!("selector={hidden}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["pullRequest"]["status"], "closed");
    app.cleanup().await;
}

/// `g5.git.pr-repository`, `g5.git.pr-adapter`: a changed member publication
/// would publish to GitHub is GitHub's whatever the PR's status: pending while
/// the PR is open, never published once it ended. Reads, checks and the
/// listing all take that from the adapter binding.
#[tokio::test]
async fn an_unpublished_github_member_binds_github_whether_open_or_ended() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let w = TestWorkspace::create(&app, &org).await;
    // On GitHub, but without a usable connection: the listing answers from
    // stored records, and nothing reaches GitHub.
    sqlx::query(
        "UPDATE repos SET git_url='https://github.com/acme/api.git',provider='github' WHERE id=$1",
    )
    .bind(w.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT org_id,$1,repo_id,'shared',role FROM workspace_repos WHERE workspace_id=$2 AND repo_id=$3 ON CONFLICT DO NOTHING")
        .bind(base.id).bind(w.id).bind(w.repo_id).execute(&app.db).await.unwrap();
    for workspace in [w.id, base.id] {
        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,'main')")
            .bind(org.id).bind(workspace).bind(w.repo_id).execute(&app.db).await.unwrap();
    }
    let pr = stored_pr(
        &app,
        org.id,
        w.id,
        base.id,
        w.repo_id,
        "issue-5-api",
        "Unpublished",
        "open",
        owner.id,
        None,
    )
    .await;
    let preflight = json!({"sourceKind":"workspace","sourceWorkspaceId":w.id,"targetWorkspaceId":base.id,
        "decision":"fastForwardOrNoOp","canDirectMerge":true,"preflightFingerprint":"f","repoResults":[],"message":"ready"});
    sqlx::query("UPDATE workspace_pull_requests SET preflight=$2 WHERE id=$1")
        .bind(pr)
        .bind(preflight)
        .execute(&app.db)
        .await
        .unwrap();
    let info = |query: &str| format!("/api/v1/workspaces/{}/pull-requests/info?{query}", w.id);
    let bound = |body: &Value| body["data"]["pullRequest"]["repositoryPullRequests"][0].clone();

    // Open: GitHub's, pending publication.
    let (status, body) = get(&app, &owner.jwt, &info(&format!("selector={pr}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let open = bound(&body);
    assert_eq!(open["adapter"], "github", "{body}");
    assert_eq!(open["number"], Value::Null);
    assert_eq!(open["state"], Value::Null);
    assert_eq!(open["providerStatus"], "pending_create");
    assert_eq!(open.get("github"), None);
    let (status, body) = get(
        &app,
        &owner.jwt,
        &info(&format!("checks=true&selector={pr}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["repositories"][0]["message"],
        "Provider publication is incomplete. Run g5d pr update and retry.",
        "{body}"
    );
    let list = |query: &str| format!("/api/v1/workspaces/{}/pull-requests/github?{query}", w.id);
    let (status, body) = get(&app, &owner.jwt, &list("state=all")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequests"][0]["tracker"], "github",
        "{body}"
    );
    assert_eq!(body["data"]["pullRequests"][0]["number"], Value::Null);

    // Ended without publication: still GitHub's, sharing the G5 PR's end, and
    // no external checks are expected of it.
    sqlx::query("UPDATE workspace_pull_requests SET status='closed', local_merge_status='closed' WHERE id=$1")
        .bind(pr)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = get(&app, &owner.jwt, &info(&format!("selector={pr}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ended = bound(&body);
    assert_eq!(ended["adapter"], "github", "{body}");
    assert_eq!(ended["state"], "CLOSED");
    assert_eq!(ended.get("providerStatus"), None);
    let (status, body) = get(
        &app,
        &owner.jwt,
        &info(&format!("checks=true&selector={pr}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["noExternalChecks"], true, "{body}");
    let (status, body) = get(&app, &owner.jwt, &list("state=all")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequests"][0]["tracker"], "github",
        "{body}"
    );
    assert!(github.received_requests().await.unwrap().is_empty());
    app.cleanup().await;
}
