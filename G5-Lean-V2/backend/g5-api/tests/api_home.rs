//! Real request-to-database proof for the organization Home attention feed.
//! Scenario ownership: test_contract/organization_home D5-D6/D10.

use crate::helpers;

use g5_auth::Scope;
use g5_contracts::types::question_set::{
    question_set_path, render_file, Question, QuestionOption, QuestionSetFile, SetMeta,
};
use g5_scm::{GitAuthor, GitRepo};
use helpers::{seed_workspace_authority_head, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn actionable_at(item: &Value) -> chrono::DateTime<chrono::FixedOffset> {
    chrono::DateTime::parse_from_rfc3339(
        item["actionableAt"]
            .as_str()
            .expect("attention item actionableAt"),
    )
    .expect("valid RFC 3339 actionableAt")
}

fn attention_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/home/attention")
}

fn open_question_metrics_path(org_id: Uuid, workspace_ids: &[Uuid]) -> String {
    let workspace_ids = workspace_ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("/api/v1/orgs/{org_id}/workspaces/open-question-metrics?workspaceIds={workspace_ids}")
}

fn workspace_approvals_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals")
}

fn approval_decisions_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/decisions")
}

async fn open_review_pr(
    app: &TestApp,
    source: &TestWorkspace,
    target: &TestWorkspace,
    owner: &TestUser,
) {
    sqlx::query("INSERT INTO workspace_pull_requests (id,org_id,source_workspace_id,target_workspace_id,status,local_merge_status,provider_status,title,last_preflight_fingerprint,created_by) VALUES ($1,$2,$3,$4,'open','ready','local_only','Home review','home-test',$5)")
        .bind(Uuid::new_v4()).bind(source.org_id).bind(source.id).bind(target.id).bind(owner.id).execute(&app.db).await.unwrap();
}

async fn request_review(
    app: &TestApp,
    owner: &TestUser,
    source: Uuid,
    definition: &str,
    reviewer: Uuid,
    phase: &str,
    version: i32,
) -> String {
    let response = app.authenticated_client(owner).post_json(
        &format!("/api/v1/workspaces/{source}/approvals/requests"),
        &json!({"definitionId": definition, "definitionVersion": version, "actorUserIds": [reviewer], "phase": phase}),
    ).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert!(
        matches!(status, StatusCode::CREATED | StatusCode::OK),
        "{body}"
    );
    assert_eq!(
        status == StatusCode::OK,
        body["data"]["requests"][0]["reused"] == true
    );
    body["data"]["requests"][0]["requestId"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn member_user(app: &TestApp, org_id: Uuid) -> TestUser {
    TestUser::create_with_scopes(
        app,
        org_id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await
}

async fn set_workspace_owner(app: &TestApp, workspace_id: Uuid, owner_id: Uuid) {
    sqlx::query("UPDATE workspaces SET owner_user_id = $1 WHERE id = $2")
        .bind(owner_id)
        .bind(workspace_id)
        .execute(&app.db)
        .await
        .expect("set workspace owner");
}

async fn mark_project_docs_available(app: &TestApp, workspace: &TestWorkspace) {
    sqlx::query("UPDATE workspaces SET docs_repo_id = $1 WHERE id = $2")
        .bind(workspace.repo_id)
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("mark workspace repository as Project Docs");
    sqlx::query(
        "UPDATE workspace_repos SET role = 'docs' \
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace.id)
    .bind(workspace.repo_id)
    .execute(&app.db)
    .await
    .expect("mark authoritative workspace repository mapping as Project Docs");
}

async fn commit_project_doc_fixture(
    app: &TestApp,
    workspace: &TestWorkspace,
    files: &[(String, String)],
) {
    let repo_path = app.init_repo(workspace.org_id, workspace.repo_id).await;
    for (path, content) in files {
        let file = repo_path.join(path);
        std::fs::create_dir_all(file.parent().expect("fixture file has a directory"))
            .expect("create Home Project Doc fixture directory");
        std::fs::write(file, content).expect("write Home Project Doc fixture");
    }
    let git_repo = GitRepo::open(&repo_path).expect("open Home Project Docs Git repository");
    git_repo
        .add_all()
        .expect("stage Home Project Docs fixtures");
    let head_commit = git_repo
        .commit(
            "Add Home Project Docs question fixtures",
            &GitAuthor::from_profile(Some("G5 Home Test"), Some("home-test@g5.local")),
        )
        .expect("commit Home Project Docs fixtures");
    helpers::seed_branch_ref(app, workspace.repo_id, &workspace.name, &head_commit).await;
}

async fn create_definition(
    app: &TestApp,
    owner: &TestUser,
    org_id: Uuid,
    name: &str,
    human_ids: &[Uuid],
) -> String {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/orgs/{org_id}/approval-definitions"),
            &json!({ "name": name, "humanUserIds": human_ids }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse approval definition");
    body["data"]["id"].as_str().expect("definition id").into()
}

async fn create_two_phase_definition(
    app: &TestApp,
    owner: &TestUser,
    org_id: Uuid,
    name: &str,
    human_ids: &[Uuid],
) -> String {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/orgs/{org_id}/approval-definitions"),
            &json!({
                "name": name,
                "humanUserIds": human_ids,
                "phases": ["be", "ie"],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse approval definition");
    body["data"]["id"].as_str().expect("definition id").into()
}

async fn attach_requirement(
    app: &TestApp,
    owner: &TestUser,
    target_workspace_id: Uuid,
    definition_id: &str,
) {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/workspaces/{target_workspace_id}/approvals/requirements"),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn approval_projection_states(
    app: &TestApp,
    user: &TestUser,
    org_id: Uuid,
    source_workspace_id: Uuid,
    definition_id: &str,
) -> (bool, bool) {
    let client = app.authenticated_client(user);
    let rich_response = client
        .get(&workspace_approvals_path(source_workspace_id))
        .await;
    assert_eq!(rich_response.status(), StatusCode::OK);
    let rich_body: Value = rich_response
        .json()
        .await
        .expect("parse rich approval state");
    let rich_can_decide = rich_body["data"]["outgoing"]["requirements"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|requirement| requirement["definition"]["id"] == definition_id)
        .and_then(|requirement| requirement["callerCanDecide"].as_bool())
        .unwrap_or(false);

    let home_response = client.get(&attention_path(org_id)).await;
    assert_eq!(home_response.status(), StatusCode::OK);
    let home_body: Value = home_response
        .json()
        .await
        .expect("parse compact approval state");
    let home_can_decide = home_body["data"]["items"]
        .as_array()
        .expect("Home items")
        .iter()
        .any(|item| {
            item["kind"] == "approval"
                && item["workspaceId"] == source_workspace_id.to_string()
                && item["sourceId"] == definition_id
        });
    (rich_can_decide, home_can_decide)
}

async fn assert_approval_projection_matches(
    app: &TestApp,
    user: &TestUser,
    org_id: Uuid,
    source_workspace_id: Uuid,
    definition_id: &str,
    expected: bool,
) {
    let states =
        approval_projection_states(app, user, org_id, source_workspace_id, definition_id).await;
    assert_eq!(states, (expected, expected));
}

async fn set_merge_target(app: &TestApp, source_workspace_id: Uuid, target_workspace_id: Uuid) {
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $1 WHERE id = $2")
        .bind(target_workspace_id)
        .bind(source_workspace_id)
        .execute(&app.db)
        .await
        .expect("set merge target");
}

async fn create_batched_approval_sources(
    app: &TestApp,
    org: &TestOrg,
    owner_id: Uuid,
    target: &TestWorkspace,
    definition_id: &str,
    reviewer_id: Uuid,
    count: i32,
) -> Vec<Uuid> {
    let prefix = format!("home-batch-{}", Uuid::new_v4());
    let workspace_ids = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH sources AS (
            SELECT gen_random_uuid() AS id, ordinal
            FROM generate_series(1, $5) AS ordinal
        )
        INSERT INTO workspaces (
            id, org_id, name, is_base, owner_user_id,
            created_by, merge_target_workspace_id
        )
        SELECT id, $1, $2 || '-' || ordinal, false, $3, $3, $4
        FROM sources
        RETURNING id
        "#,
    )
    .bind(org.id)
    .bind(prefix)
    .bind(owner_id)
    .bind(target.id)
    .bind(count)
    .fetch_all(&app.db)
    .await
    .expect("create batched Home approval source workspaces");

    sqlx::query(
        "INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role)
                 SELECT $1,workspace_id,$2,'','code' FROM UNNEST($3::uuid[]) AS workspace_id",
    )
    .bind(org.id)
    .bind(target.repo_id)
    .bind(&workspace_ids)
    .execute(&app.db)
    .await
    .expect("create batched canonical members");
    for workspace_id in &workspace_ids {
        seed_workspace_authority_head(app, *workspace_id, target.repo_id, &target.head_commit)
            .await;
    }

    // Batch real persisted PRs and addressed tasks; rule eligibility alone
    // must not populate Home. Request admission is covered by the API journeys.
    sqlx::query("WITH prs AS (INSERT INTO workspace_pull_requests (id,org_id,source_workspace_id,target_workspace_id,status,local_merge_status,provider_status,title,last_preflight_fingerprint,created_by) SELECT gen_random_uuid(),$1,id,$2,'open','ready','local_only','Home batch review','home-batch',$3 FROM UNNEST($4::uuid[]) AS id RETURNING id,source_workspace_id) INSERT INTO approval_requests (id,org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,actor_user_id,requested_by,phase,pull_request_id) SELECT gen_random_uuid(),$1,source_workspace_id,$5::uuid,1,md5(source_workspace_id::text)||md5(id::text),'human',$6,$3,'ie',id FROM prs")
        .bind(org.id).bind(target.id).bind(owner_id).bind(&workspace_ids).bind(definition_id).bind(reviewer_id).execute(&app.db).await.unwrap();

    workspace_ids
}

#[cfg(debug_assertions)]
fn approval_query_ids_for_workspace_count(
    observations: &[g5_api::services::approval_config_service::HomeApprovalQueryObservation],
    visible_workspace_count: usize,
) -> Vec<&'static str> {
    let batch_ids = observations
        .iter()
        .filter(|observation| observation.visible_workspace_count == visible_workspace_count)
        .map(|observation| observation.batch_id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        batch_ids.len(),
        1,
        "expected one observed Home approval batch for {visible_workspace_count} visible workspaces"
    );
    let batch_id = *batch_ids.iter().next().expect("observed batch id");
    observations
        .iter()
        .filter(|observation| observation.batch_id == batch_id)
        .map(|observation| observation.query_id)
        .collect()
}

#[tokio::test]
async fn organization_home_attention_is_scoped_and_empty_by_default() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    let response = client.get(&attention_path(org.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse Home response");
    assert_eq!(body["data"]["items"], serde_json::json!([]));
    assert!(body["data"]["nextCursor"].is_null());
}

#[tokio::test]
async fn organization_home_attention_rejects_invalid_page_bounds() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    for limit in [0, 51] {
        let response = client
            .get(&format!("{}?limit={limit}", attention_path(org.id)))
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    let response = client
        .get(&format!(
            "{}?kind=not_an_attention_kind",
            attention_path(org.id)
        ))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn organization_home_attention_rejects_cross_org_and_malformed_cursor() {
    let app = TestApp::spawn().await;
    let mut first_owner = TestUser::create(&app).await;
    let first_org = TestOrg::create_with_admin(&app, &mut first_owner).await;
    let mut second_owner = TestUser::create(&app).await;
    let second_org = TestOrg::create_with_admin(&app, &mut second_owner).await;

    let response = app
        .authenticated_client(&second_owner)
        .get(&attention_path(first_org.id))
        .await;
    assert_ne!(response.status(), StatusCode::OK);

    let response = app
        .authenticated_client(&second_owner)
        .get(&format!(
            "{}?cursor=definitely-not-a-cursor",
            attention_path(second_org.id)
        ))
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn organization_home_attention_includes_only_visible_caller_decidable_approvals() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, target.id, owner.id).await;
    set_workspace_owner(&app, source.id, owner.id).await;
    set_merge_target(&app, source.id, target.id).await;
    let definition_id =
        create_definition(&app, &owner, org.id, "Member sign-off", &[member.id]).await;
    attach_requirement(&app, &owner, target.id, &definition_id).await;
    open_review_pr(&app, &source, &target, &owner).await;
    request_review(&app, &owner, source.id, &definition_id, member.id, "ie", 1).await;

    let response = app
        .authenticated_client(&member)
        .get(&attention_path(org.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse attention response");
    let items = body["data"]["items"].as_array().expect("attention items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "approval");
    assert_eq!(items[0]["workspaceId"], source.id.to_string());
    // The row carries the owning workspace's owner, not the caller (here the
    // deciding member), so Home can show the owner avatar the Workspaces tile
    // does.
    assert_eq!(items[0]["ownerUserId"], owner.id.to_string());
    assert_eq!(items[0]["displayName"], "Member sign-off");
    assert!(items[0].get("openQuestionCount").is_none());
    let _ = actionable_at(&items[0]);
    assert!(items[0].get("humanInputTarget").is_none());

    let owner_response = app
        .authenticated_client(&owner)
        .get(&attention_path(org.id))
        .await;
    assert_eq!(owner_response.status(), StatusCode::OK);
    let owner_body: Value = owner_response.json().await.expect("parse owner response");
    let owner_items = owner_body["data"]["items"]
        .as_array()
        .expect("owner attention items");
    assert!(
        owner_items.is_empty(),
        "the owner has no personal approval task"
    );

    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(source.id)
        .execute(&app.db)
        .await
        .expect("delete source workspace");
    let deleted_response = app
        .authenticated_client(&member)
        .get(&attention_path(org.id))
        .await;
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_body: Value = deleted_response
        .json()
        .await
        .expect("parse deleted response");
    assert_eq!(deleted_body["data"]["items"], json!([]));
}

#[tokio::test]
async fn organization_home_isolates_clean_unborn_project_docs_and_preserves_relational_attention() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let ready_workspace = TestWorkspace::create(&app, &org).await;
    let unborn_workspace = TestWorkspace::create_base(&app, &org).await;
    set_workspace_owner(&app, ready_workspace.id, owner.id).await;
    set_workspace_owner(&app, unborn_workspace.id, owner.id).await;
    mark_project_docs_available(&app, &ready_workspace).await;
    mark_project_docs_available(&app, &unborn_workspace).await;

    let ready_path = format!("docs/{}/ready-question.md", ready_workspace.id);
    let unborn_path = format!("docs/{}/unborn-question.md", unborn_workspace.id);
    let question = |id: &str, prompt: &str| {
        [
            "# Home attention question",
            "",
            "[Q1. Choose the production option]",
            "",
            "```g5-question",
            "schema: 1",
            id,
            "status: open",
            prompt,
            "options:",
            "  - label: First",
            "    description: Use the first production option",
            "answer:",
            "  selected: []",
            "  text: \"\"",
            "  rationale: \"\"",
            "  answered_by: null",
            "  answered_at: null",
            "```",
            "",
        ]
        .join("\n")
    };
    commit_project_doc_fixture(
        &app,
        &ready_workspace,
        &[(
            ready_path,
            question("id: ready-question", "prompt: Keep the ready source?"),
        )],
    )
    .await;
    commit_project_doc_fixture(
        &app,
        &unborn_workspace,
        &[(
            unborn_path,
            question("id: unborn-question", "prompt: Materialize this source?"),
        )],
    )
    .await;

    let unborn_materialized_head = unborn_workspace.git_head(&app);
    let unborn_repo_path = app.repo_path(unborn_workspace.repo_id).await;
    let populated_repo_path = unborn_repo_path.with_extension("materialized-fixture");
    std::fs::rename(&unborn_repo_path, &populated_repo_path).unwrap();
    let empty = git2::Repository::init(&unborn_repo_path).unwrap();
    empty.set_head("refs/heads/main").unwrap();
    empty
        .config()
        .unwrap()
        .set_str("init.defaultBranch", "different-bootstrap-default")
        .unwrap();
    assert!(empty.references().unwrap().next().is_none());
    drop(empty);

    let client = app.authenticated_client(&owner);
    let response = client.get(&attention_path(org.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse Home attention");
    let items = body["data"]["items"].as_array().expect("Home items");
    assert!(items.iter().any(|item| {
        item["kind"] == "open_question" && item["workspaceId"] == ready_workspace.id.to_string()
    }));
    assert!(!items.iter().any(|item| {
        item["kind"] == "open_question" && item["workspaceId"] == unborn_workspace.id.to_string()
    }));

    let metrics_response = client
        .get(&open_question_metrics_path(
            org.id,
            &[ready_workspace.id, unborn_workspace.id],
        ))
        .await;
    assert_eq!(metrics_response.status(), StatusCode::OK);
    let metrics_body: Value = metrics_response
        .json()
        .await
        .expect("parse open-question metrics");
    let metrics = metrics_body["data"].as_array().expect("workspace metrics");
    let ready_metrics = metrics
        .iter()
        .find(|metric| metric["workspaceId"] == ready_workspace.id.to_string())
        .expect("ready workspace metrics");
    assert_eq!(ready_metrics["documentOpenQuestions"], json!(1));
    let unborn_metrics = metrics
        .iter()
        .find(|metric| metric["workspaceId"] == unborn_workspace.id.to_string())
        .expect("unborn workspace metrics");
    assert_eq!(unborn_metrics["documentOpenQuestions"], json!(0));
    assert_eq!(unborn_metrics["projectDocSources"], json!([]));

    std::fs::remove_dir_all(&unborn_repo_path).unwrap();
    std::fs::rename(&populated_repo_path, &unborn_repo_path).unwrap();
    seed_workspace_authority_head(
        &app,
        unborn_workspace.id,
        unborn_workspace.repo_id,
        &unborn_materialized_head,
    )
    .await;
    let materialized_response = client.get(&attention_path(org.id)).await;
    assert_eq!(materialized_response.status(), StatusCode::OK);
    let materialized_body: Value = materialized_response
        .json()
        .await
        .expect("parse materialized Home attention");
    assert!(materialized_body["data"]["items"]
        .as_array()
        .expect("materialized Home items")
        .iter()
        .any(|item| {
            item["kind"] == "open_question"
                && item["workspaceId"] == unborn_workspace.id.to_string()
        }));

    let materialized_metrics_response = client
        .get(&open_question_metrics_path(org.id, &[unborn_workspace.id]))
        .await;
    assert_eq!(materialized_metrics_response.status(), StatusCode::OK);
    let materialized_metrics_body: Value = materialized_metrics_response
        .json()
        .await
        .expect("parse materialized open-question metrics");
    assert_eq!(
        materialized_metrics_body["data"][0]["documentOpenQuestions"],
        json!(1)
    );
}

#[tokio::test]
async fn organization_home_keeps_invalid_workspace_authority_failures_typed() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, workspace.id, owner.id).await;
    mark_project_docs_available(&app, &workspace).await;
    let client = app.authenticated_client(&owner);

    // The branch ref is the head authority for every read (P1.1); the
    // `materialization_status` and `head_commit` columns are not consulted.
    // The invalid authority state is a workspace member whose branch ref is
    // missing from its mirror.
    helpers::seed_branch_ref(
        &app,
        workspace.repo_id,
        &workspace.name,
        "0000000000000000000000000000000000000000",
    )
    .await;
    let response = client.get(&attention_path(org.id)).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = response.json().await.expect("parse typed authority error");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    let metrics_response = client
        .get(&open_question_metrics_path(org.id, &[workspace.id]))
        .await;
    assert_eq!(metrics_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let metrics_body: Value = metrics_response
        .json()
        .await
        .expect("parse typed metrics authority error");
    assert_eq!(metrics_body["error"]["code"], "VALIDATION_ERROR");
    helpers::seed_branch_ref(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
    )
    .await;
    let restored_response = client.get(&attention_path(org.id)).await;
    assert_eq!(restored_response.status(), StatusCode::OK);

    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1 AND repo_id=$2")
        .bind(workspace.id)
        .bind(workspace.repo_id)
        .execute(&app.db)
        .await
        .expect("remove authority inventory member");
    let incomplete_response = client.get(&attention_path(org.id)).await;
    assert_eq!(
        incomplete_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let incomplete_body: Value = incomplete_response
        .json()
        .await
        .expect("parse incomplete authority error");
    assert_eq!(incomplete_body["error"]["code"], "VALIDATION_ERROR");
    let incomplete_metrics_response = client
        .get(&open_question_metrics_path(org.id, &[workspace.id]))
        .await;
    assert_eq!(
        incomplete_metrics_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn organization_home_batched_approval_matches_rich_eligibility_transitions() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, target.id, owner.id).await;
    set_workspace_owner(&app, source.id, owner.id).await;
    set_merge_target(&app, source.id, target.id).await;
    let definition_id =
        create_definition(&app, &owner, org.id, "Differential sign-off", &[member.id]).await;
    attach_requirement(&app, &owner, target.id, &definition_id).await;
    assert_approval_projection_matches(&app, &member, org.id, source.id, &definition_id, false)
        .await;
    open_review_pr(&app, &source, &target, &owner).await;
    let client = app.authenticated_client(&member);
    for outcome in ["approved", "rejected", "approved"] {
        let request =
            request_review(&app, &owner, source.id, &definition_id, member.id, "ie", 1).await;
        assert_approval_projection_matches(&app, &member, org.id, source.id, &definition_id, true)
            .await;
        let response = client
            .post_json(
                &approval_decisions_path(source.id),
                &json!({ "definitionId": definition_id, "requestId": request, "outcome": outcome, "comment": "Type-specific assessment", "phase": "ie", "reviewedGitHeads": helpers::review_flow::heads(&app, &member, source.id).await }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_approval_projection_matches(&app, &member, org.id, source.id, &definition_id, false)
            .await;
    }

    commit_project_doc_fixture(
        &app,
        &source,
        &[("changed.md".into(), "updated source version".into())],
    )
    .await;
    assert_approval_projection_matches(&app, &member, org.id, source.id, &definition_id, false)
        .await;
    let request = request_review(&app, &owner, source.id, &definition_id, member.id, "ie", 1).await;
    let response = client
        .post_json(
            &approval_decisions_path(source.id),
            &json!({ "definitionId": definition_id, "requestId": request, "outcome": "approved", "phase": "ie", "reviewedGitHeads": helpers::review_flow::heads(&app, &member, source.id).await }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!(
                "/api/v1/orgs/{}/approval-definitions/{definition_id}/versions",
                org.id
            ),
            &json!({
                "name": "Differential sign-off",
                "humanUserIds": [member.id],
                "invalidatesPrevious": true,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_approval_projection_matches(&app, &member, org.id, source.id, &definition_id, false)
        .await;
    request_review(&app, &owner, source.id, &definition_id, member.id, "ie", 2).await;
    assert_approval_projection_matches(&app, &member, org.id, source.id, &definition_id, true)
        .await;

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/approvals/requirements/{definition_id}/retire",
                target.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_approval_projection_matches(&app, &member, org.id, source.id, &definition_id, false)
        .await;
}

#[tokio::test]
async fn organization_home_batched_approval_is_one_task_across_phase_moves() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, target.id, owner.id).await;
    set_workspace_owner(&app, source.id, owner.id).await;
    set_merge_target(&app, source.id, target.id).await;
    let definition_id = create_two_phase_definition(
        &app,
        &owner,
        org.id,
        "Two-phase Home sign-off",
        &[member.id],
    )
    .await;
    attach_requirement(&app, &owner, target.id, &definition_id).await;
    open_review_pr(&app, &source, &target, &owner).await;
    let be_request =
        request_review(&app, &owner, source.id, &definition_id, member.id, "be", 1).await;
    let ie_request =
        request_review(&app, &owner, source.id, &definition_id, member.id, "ie", 1).await;
    let client = app.authenticated_client(&member);
    let home_response = client.get(&attention_path(org.id)).await;
    assert_eq!(home_response.status(), StatusCode::OK);
    let home_body: Value = home_response.json().await.expect("parse Home attention");
    let approval_ids = home_body["data"]["items"]
        .as_array()
        .expect("Home items")
        .iter()
        .filter(|item| {
            item["kind"] == "approval"
                && item["workspaceId"] == source.id.to_string()
                && item["sourceId"] == definition_id
        })
        .map(|item| item["id"].as_str().expect("approval item id"))
        .collect::<Vec<_>>();
    assert_eq!(be_request, ie_request);
    assert_eq!(approval_ids.len(), 1);

    let be_response = client
        .post_json(
            &approval_decisions_path(source.id),
            &json!({
                "definitionId": definition_id,
                "requestId": be_request,
                "reviewedGitHeads": helpers::review_flow::heads(&app, &member, source.id).await,
                "phase": "be",
                "outcome": "approved",
            }),
        )
        .await;
    assert_eq!(be_response.status(), StatusCode::OK);

    let home_response = client.get(&attention_path(org.id)).await;
    assert_eq!(home_response.status(), StatusCode::OK);
    let home_body: Value = home_response.json().await.expect("parse Home attention");
    let remaining_ids = home_body["data"]["items"]
        .as_array()
        .expect("Home items")
        .iter()
        .filter(|item| item["kind"] == "approval" && item["sourceId"] == definition_id)
        .map(|item| item["id"].as_str().expect("approval item id"))
        .collect::<Vec<_>>();
    assert!(remaining_ids.is_empty());

    let ie_response = client
        .post_json(
            &approval_decisions_path(source.id),
            &json!({
                "definitionId": definition_id,
                "requestId": ie_request,
                "reviewedGitHeads": helpers::review_flow::heads(&app, &member, source.id).await,
                "phase": "ie",
                "outcome": "approved",
            }),
        )
        .await;
    assert_eq!(ie_response.status(), StatusCode::OK);

    let home_response = client.get(&attention_path(org.id)).await;
    assert_eq!(home_response.status(), StatusCode::OK);
    let home_body: Value = home_response.json().await.expect("parse Home attention");
    assert!(!home_body["data"]["items"]
        .as_array()
        .expect("Home items")
        .iter()
        .any(|item| item["kind"] == "approval" && item["sourceId"] == definition_id));
}

fn open_question_doc(id: &str, prompt: &str) -> String {
    [
        "# Home attention question",
        "",
        "[Q1. Choose the production option]",
        "",
        "```g5-question",
        "schema: 1",
        id,
        "status: open",
        prompt,
        "options:",
        "  - label: First",
        "    description: Use the first production option",
        "answer:",
        "  selected: []",
        "  text: \"\"",
        "  rationale: \"\"",
        "  answered_by: null",
        "  answered_at: null",
        "```",
        "",
    ]
    .join("\n")
}

/// Walk every Home attention page at the given limit, returning `(id, kind)`
/// in page order.
async fn collect_home_pages(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    limit: usize,
    kind: Option<&str>,
) -> Vec<(String, String)> {
    collect_ordered_home_pages(client, org_id, limit, kind, None).await
}

/// [`collect_home_pages`] naming an `order` (`newest`/`oldest`) on every
/// page, or none.
async fn collect_ordered_home_pages(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    limit: usize,
    kind: Option<&str>,
    order: Option<&str>,
) -> Vec<(String, String)> {
    let kind_query = kind.map_or(String::new(), |kind| format!("&kind={kind}"))
        + &order.map_or(String::new(), |order| format!("&order={order}"));
    let mut cursor: Option<String> = None;
    let mut items_seen = Vec::new();
    loop {
        let path = cursor.as_ref().map_or_else(
            || format!("{}?limit={limit}{kind_query}", attention_path(org_id)),
            |cursor| {
                format!(
                    "{}?limit={limit}{kind_query}&cursor={cursor}",
                    attention_path(org_id)
                )
            },
        );
        let response = client.get(&path).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.expect("parse Home page");
        let items = body["data"]["items"].as_array().expect("page items");
        assert!(!items.is_empty(), "a page with a cursor must not be empty");
        assert!(items.len() <= limit);
        for item in items {
            items_seen.push((
                item["id"].as_str().expect("stable item id").to_string(),
                item["kind"].as_str().expect("attention kind").to_string(),
            ));
        }
        cursor = body["data"]["nextCursor"].as_str().map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    items_seen
}

#[tokio::test]
async fn organization_home_attention_mixed_kind_cursor_has_no_duplicates_or_skips() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let question_workspace = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, target.id, owner.id).await;
    set_workspace_owner(&app, source.id, owner.id).await;
    set_workspace_owner(&app, question_workspace.id, member.id).await;
    mark_project_docs_available(&app, &question_workspace).await;
    set_merge_target(&app, source.id, target.id).await;
    let definition_id =
        create_definition(&app, &owner, org.id, "Mixed cursor sign-off", &[member.id]).await;
    attach_requirement(&app, &owner, target.id, &definition_id).await;
    open_review_pr(&app, &source, &target, &owner).await;
    request_review(&app, &owner, source.id, &definition_id, member.id, "ie", 1).await;
    let prefix = format!("docs/{}/", question_workspace.id);
    commit_project_doc_fixture(
        &app,
        &question_workspace,
        &[
            (
                format!("{prefix}alpha.md"),
                open_question_doc("id: alpha-question", "prompt: Keep alpha?"),
            ),
            (
                format!("{prefix}beta.md"),
                open_question_doc("id: beta-question", "prompt: Keep beta?"),
            ),
        ],
    )
    .await;

    let client = app.authenticated_client(&member);
    let all = collect_home_pages(&client, org.id, 1, None).await;
    assert_eq!(
        all.len(),
        3,
        "one approval plus two Project Doc sources: {all:?}"
    );
    assert_eq!(
        all.iter()
            .map(|(id, _)| id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        all.len(),
        "pages must not repeat an item"
    );
    assert_eq!(all.iter().filter(|(_, kind)| kind == "approval").count(), 1);
    assert_eq!(
        all.iter()
            .filter(|(_, kind)| kind == "open_question")
            .count(),
        2
    );
    let two_per_page = collect_home_pages(&client, org.id, 2, None).await;
    assert_eq!(
        two_per_page, all,
        "page size must not change order or membership"
    );
}

#[tokio::test]
async fn organization_home_attention_filters_before_capture_and_paginates_within_kind() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let question_workspace = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, target.id, owner.id).await;
    set_workspace_owner(&app, source.id, owner.id).await;
    set_workspace_owner(&app, question_workspace.id, owner.id).await;
    mark_project_docs_available(&app, &question_workspace).await;
    set_merge_target(&app, source.id, target.id).await;
    let definition_id =
        create_definition(&app, &owner, org.id, "Filtered sign-off", &[owner.id]).await;
    attach_requirement(&app, &owner, target.id, &definition_id).await;
    let prefix = format!("docs/{}/", question_workspace.id);
    commit_project_doc_fixture(
        &app,
        &question_workspace,
        &[
            (
                format!("{prefix}one.md"),
                open_question_doc("id: one-question", "prompt: Keep one?"),
            ),
            (
                format!("{prefix}two.md"),
                open_question_doc("id: two-question", "prompt: Keep two?"),
            ),
            (
                format!("{prefix}three.md"),
                open_question_doc("id: three-question", "prompt: Keep three?"),
            ),
        ],
    )
    .await;

    #[cfg(debug_assertions)]
    let _ =
        g5_api::services::approval_config_service::take_home_approval_query_observations(org.id);
    #[cfg(debug_assertions)]
    let _ = g5_api::services::home_service::take_home_candidate_observations(org.id);

    let client = app.authenticated_client(&owner);
    let first_response = client
        .get(&format!(
            "{}?limit=2&kind=open_question",
            attention_path(org.id)
        ))
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_body: Value = first_response
        .json()
        .await
        .expect("parse filtered first page");
    let first_items = first_body["data"]["items"]
        .as_array()
        .expect("filtered first items");
    assert_eq!(first_items.len(), 2);
    assert!(first_items
        .iter()
        .all(|item| item["kind"] == "open_question"));
    let cursor = first_body["data"]["nextCursor"]
        .as_str()
        .expect("filtered next cursor")
        .to_string();

    #[cfg(debug_assertions)]
    {
        assert!(
            g5_api::services::approval_config_service::take_home_approval_query_observations(
                org.id
            )
            .is_empty(),
            "open-question filtering must prune approval capture"
        );
        assert!(
            g5_api::services::home_service::take_home_candidate_observations(org.id)
                .iter()
                .any(|observation| {
                    observation.candidate_limit == 3
                        && observation.approval_count == 0
                        && observation.project_doc_count == 3
                        && observation.merged_count == 3
                }),
            "filtered capture must retain only one bounded candidate stream"
        );
    }

    let filtered = collect_home_pages(&client, org.id, 2, Some("open_question")).await;
    assert_eq!(filtered.len(), 3);
    assert!(filtered.iter().all(|(_, kind)| kind == "open_question"));
    assert_eq!(
        filtered
            .iter()
            .map(|(id, _)| id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );

    let mismatched_response = client
        .get(&format!(
            "{}?limit=2&kind=approval&cursor={cursor}",
            attention_path(org.id)
        ))
        .await;
    assert_eq!(
        mismatched_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let retired_kind = client
        .get(&format!(
            "{}?limit=2&kind=human_escalation",
            attention_path(org.id)
        ))
        .await;
    assert_eq!(retired_kind.status(), StatusCode::BAD_REQUEST);
}

fn open_question_set_file(set_id: &str) -> String {
    let mut question = Question::new_open("open-1", "Still open?");
    question.options = vec![QuestionOption {
        id: "keep".to_string(),
        label: "Keep".to_string(),
        description: String::new(),
        tradeoffs: None,
        risks: None,
        recommended: None,
        extra: Default::default(),
    }];
    let mut file = QuestionSetFile::new(SetMeta {
        id: set_id.to_string(),
        title: format!("Set {set_id}"),
        created_at: "2026-10-20T00:00:00Z".to_string(),
        created_by: None,
        extra: Default::default(),
    });
    file.questions = vec![question];
    render_file(&file)
}

fn question_set_reference_doc(paths: &[&str]) -> String {
    let mut doc = "# References\n\n".to_string();
    for (index, path) in paths.iter().enumerate() {
        doc.push_str(&format!(
            "[Q{}. Referenced]\n\n```g5-question-ref\n{{\"path\": \"{path}\", \"id\": \"open-1\"}}\n```\n\n",
            index + 1
        ));
    }
    doc
}

/// One member addressed by two tied approval requests (two sources requested
/// in one statement) who owns two question workspaces: the first has two
/// Project Docs and two question sets that all share its docs time, the
/// second one Project Doc.
async fn ordered_attention_fixture(app: &TestApp) -> (TestOrg, TestUser) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let member = member_user(app, org.id).await;
    let target = TestWorkspace::create(app, &org).await;
    set_workspace_owner(app, target.id, owner.id).await;
    let definition_id =
        create_definition(app, &owner, org.id, "Ordered sign-off", &[member.id]).await;
    attach_requirement(app, &owner, target.id, &definition_id).await;
    create_batched_approval_sources(app, &org, owner.id, &target, &definition_id, member.id, 2)
        .await;

    let tied = TestWorkspace::create(app, &org).await;
    set_workspace_owner(app, tied.id, member.id).await;
    mark_project_docs_available(app, &tied).await;
    let tied_ws = tied.id.to_string();
    let first_set = question_set_path(&tied_ws, "first");
    let second_set = question_set_path(&tied_ws, "second");
    commit_project_doc_fixture(
        app,
        &tied,
        &[
            (
                format!("docs/{tied_ws}/alpha.md"),
                open_question_doc("id: alpha-question", "prompt: Keep alpha?"),
            ),
            (
                format!("docs/{tied_ws}/beta.md"),
                open_question_doc("id: beta-question", "prompt: Keep beta?"),
            ),
            (first_set.clone(), open_question_set_file("first")),
            (second_set.clone(), open_question_set_file("second")),
            (
                format!("docs/{tied_ws}/references.md"),
                question_set_reference_doc(&[&first_set, &second_set]),
            ),
        ],
    )
    .await;

    let single = TestWorkspace::create(app, &org).await;
    set_workspace_owner(app, single.id, member.id).await;
    mark_project_docs_available(app, &single).await;
    commit_project_doc_fixture(
        app,
        &single,
        &[(
            format!("docs/{}/gamma.md", single.id),
            open_question_doc("id: gamma-question", "prompt: Keep gamma?"),
        )],
    )
    .await;
    (org, member)
}

fn reversed<T: Clone>(items: &[T]) -> Vec<T> {
    items.iter().rev().cloned().collect()
}

#[tokio::test]
async fn organization_home_attention_newest_is_oldest_reversed_across_every_page() {
    let app = TestApp::spawn().await;
    let (org, member) = ordered_attention_fixture(&app).await;
    let client = app.authenticated_client(&member);

    let oldest = collect_ordered_home_pages(&client, org.id, 50, None, Some("oldest")).await;
    let ids = oldest.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>();
    assert_eq!(
        oldest.len(),
        7,
        "two approvals, three docs, two sets: {ids:?}"
    );
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        7,
        "no item repeats: {ids:?}"
    );
    assert_eq!(
        ids.iter()
            .filter(|id| id.starts_with("open_question_set:"))
            .count(),
        2,
        "{ids:?}"
    );
    let newest = collect_ordered_home_pages(&client, org.id, 50, None, Some("newest")).await;
    assert_eq!(
        newest,
        reversed(&oldest),
        "Newest is exactly Oldest reversed"
    );
    // A read naming no order keeps the order the API always had.
    assert_eq!(collect_home_pages(&client, org.id, 50, None).await, oldest);

    // Every page continues one order, at any page size, and the order is
    // stable between reads. A successful walk also proves each stream stayed
    // within its `limit + 1` candidate bound (`validate_candidate_bounds`
    // fails the read otherwise).
    for limit in [1, 2, 3] {
        assert_eq!(
            collect_ordered_home_pages(&client, org.id, limit, None, Some("oldest")).await,
            oldest,
            "oldest at limit {limit}"
        );
        assert_eq!(
            collect_ordered_home_pages(&client, org.id, limit, None, Some("newest")).await,
            newest,
            "newest at limit {limit}"
        );
    }

    // Ordered by the time the row shows: never older further down Newest.
    let response = client
        .get(&format!("{}?limit=50&order=newest", attention_path(org.id)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse newest page");
    let times = body["data"]["items"]
        .as_array()
        .expect("newest items")
        .iter()
        .map(actionable_at)
        .collect::<Vec<_>>();
    assert!(
        times.windows(2).all(|pair| pair[0] >= pair[1]),
        "newest first by Attention updated: {times:?}"
    );

    // The kind filter keeps the order: each filtered walk is the full list's
    // subsequence in the same order, and Newest still reverses Oldest.
    for kind in ["approval", "open_question"] {
        let expected_oldest = oldest
            .iter()
            .filter(|(_, item_kind)| item_kind == kind)
            .cloned()
            .collect::<Vec<_>>();
        assert!(!expected_oldest.is_empty());
        for limit in [1, 2] {
            assert_eq!(
                collect_ordered_home_pages(&client, org.id, limit, Some(kind), Some("oldest"))
                    .await,
                expected_oldest,
                "{kind} oldest at limit {limit}"
            );
            assert_eq!(
                collect_ordered_home_pages(&client, org.id, limit, Some(kind), Some("newest"))
                    .await,
                reversed(&expected_oldest),
                "{kind} newest at limit {limit}"
            );
        }
    }

    app.cleanup().await;
}

/// The `nextCursor` of the first page for `query`.
async fn first_page_cursor(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    query: &str,
) -> String {
    let response = client
        .get(&format!("{}?{query}", attention_path(org_id)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse first page");
    body["data"]["nextCursor"]
        .as_str()
        .expect("first page has a next cursor")
        .to_string()
}

async fn page_ids(client: &helpers::AuthenticatedClient, path: &str) -> Vec<String> {
    let response = client.get(path).await;
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    let body: Value = response.json().await.expect("parse page");
    body["data"]["items"]
        .as_array()
        .expect("page items")
        .iter()
        .map(|item| item["id"].as_str().expect("item id").to_string())
        .collect()
}

#[tokio::test]
async fn organization_home_attention_cursor_never_crosses_orders() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    let app = TestApp::spawn().await;
    let (org, member) = ordered_attention_fixture(&app).await;
    let client = app.authenticated_client(&member);
    let path = attention_path(org.id);

    let newest_cursor = first_page_cursor(&client, org.id, "limit=2&order=newest").await;
    let oldest_cursor = first_page_cursor(&client, org.id, "limit=2&order=oldest").await;
    for (cursor, query) in [
        (&newest_cursor, "&order=oldest"),
        // No order means oldest, so a Newest position cannot continue it.
        (&newest_cursor, ""),
        (&oldest_cursor, "&order=newest"),
    ] {
        let response = client
            .get(&format!("{path}?limit=2&cursor={cursor}{query}"))
            .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "cursor from the other order with {query:?}"
        );
    }
    let unknown = client.get(&format!("{path}?limit=2&order=sideways")).await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);

    // A cursor issued before orders existed carries no order and continues
    // oldest first, exactly like the current oldest cursor.
    let mut legacy: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(&oldest_cursor)
            .expect("cursor is URL-safe base64"),
    )
    .expect("cursor is JSON");
    assert_eq!(legacy["order"], "oldest");
    legacy
        .as_object_mut()
        .expect("cursor object")
        .remove("order");
    let legacy_cursor =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&legacy).expect("encode legacy cursor"));
    let continued = page_ids(
        &client,
        &format!("{path}?limit=2&cursor={oldest_cursor}&order=oldest"),
    )
    .await;
    assert!(!continued.is_empty());
    assert_eq!(
        page_ids(&client, &format!("{path}?limit=2&cursor={legacy_cursor}")).await,
        continued
    );

    app.cleanup().await;
}

#[tokio::test]
async fn organization_home_attention_scans_past_closed_project_docs_for_later_open_source() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, workspace.id, owner.id).await;
    mark_project_docs_available(&app, &workspace).await;

    let prefix = format!("docs/{}/", workspace.id);
    let mut files = (0..40)
        .map(|ordinal| {
            (
                format!("{prefix}aa-closed-{ordinal:02}.md"),
                format!("# Closed document {ordinal}\n\nNo open questions.\n"),
            )
        })
        .collect::<Vec<_>>();
    files.push((
        format!("{prefix}zz-open.md"),
        [
            "# Later open document",
            "",
            "[Q1. Later open question]",
            "",
            "```g5-question",
            "schema: 1",
            "id: later-open",
            "status: open",
            "prompt: Which production option should be used?",
            "options:",
            "  - label: First",
            "    description: Use the first production option",
            "answer:",
            "  selected: []",
            "  text: \"\"",
            "  rationale: \"\"",
            "  answered_by: null",
            "  answered_at: null",
            "```",
            "",
        ]
        .join("\n"),
    ));
    commit_project_doc_fixture(&app, &workspace, &files).await;

    #[cfg(debug_assertions)]
    let _ = g5_api::services::home_service::take_home_candidate_observations(org.id);
    let response = app
        .authenticated_client(&owner)
        .get(&format!("{}?limit=1", attention_path(org.id)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse Project Doc Home page");
    let items = body["data"]["items"].as_array().expect("Project Doc items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "open_question");
    assert_eq!(items[0]["workspaceId"], workspace.id.to_string());
    // Open-question rows carry the owning workspace's owner for the avatar.
    assert_eq!(items[0]["ownerUserId"], owner.id.to_string());
    assert_eq!(items[0]["displayName"], "zz-open.md");
    assert_eq!(items[0]["openQuestionCount"], 1);
    #[cfg(debug_assertions)]
    assert!(
        g5_api::services::home_service::take_home_candidate_observations(org.id)
            .iter()
            .any(|observation| {
                observation.candidate_limit == 2
                    && observation.project_doc_count == 1
                    && observation.merged_count == 1
            })
    );
}

#[tokio::test]
async fn organization_home_attention_batches_approval_reads_at_large_visible_workspace_count() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let target = TestWorkspace::create(&app, &org).await;
    set_workspace_owner(&app, target.id, owner.id).await;
    let definition_id =
        create_definition(&app, &owner, org.id, "Large batch sign-off", &[member.id]).await;
    attach_requirement(&app, &owner, target.id, &definition_id).await;

    let mut source_workspace_ids = create_batched_approval_sources(
        &app,
        &org,
        owner.id,
        &target,
        &definition_id,
        member.id,
        1,
    )
    .await;
    #[cfg(debug_assertions)]
    let _ =
        g5_api::services::approval_config_service::take_home_approval_query_observations(org.id);
    let client = app.authenticated_client(&member);
    let small_response = client.get(&attention_path(org.id)).await;
    assert_eq!(small_response.status(), StatusCode::OK);
    #[cfg(debug_assertions)]
    let small_query_ids = approval_query_ids_for_workspace_count(
        &g5_api::services::approval_config_service::take_home_approval_query_observations(org.id),
        2,
    );

    source_workspace_ids.extend(
        create_batched_approval_sources(
            &app,
            &org,
            owner.id,
            &target,
            &definition_id,
            member.id,
            499,
        )
        .await,
    );
    #[cfg(debug_assertions)]
    let _ =
        g5_api::services::approval_config_service::take_home_approval_query_observations(org.id);
    #[cfg(debug_assertions)]
    let _ = g5_api::services::home_service::take_home_candidate_observations(org.id);
    let large_response = client
        .get(&format!("{}?limit=50", attention_path(org.id)))
        .await;
    assert_eq!(large_response.status(), StatusCode::OK);
    #[cfg(debug_assertions)]
    {
        let large_query_ids = approval_query_ids_for_workspace_count(
            &g5_api::services::approval_config_service::take_home_approval_query_observations(
                org.id,
            ),
            501,
        );
        assert_eq!(large_query_ids, small_query_ids);
        assert_eq!(
            large_query_ids,
            vec![
                "caller-membership",
                "source-targets",
                "active-configs",
                "latest-definitions",
                "definition-profiles",
                "caller-decisions",
                "source-heads",
                "invalidation-boundaries",
            ]
        );
        let candidate_observations =
            g5_api::services::home_service::take_home_candidate_observations(org.id);
        assert!(candidate_observations.iter().any(|observation| {
            observation.candidate_limit == 51
                && observation.approval_count == 51
                && observation.project_doc_count == 0
                && observation.merged_count == 51
        }));
    }
    let large_body: Value = large_response.json().await.expect("parse large Home page");
    assert_eq!(
        large_body["data"]["items"]
            .as_array()
            .expect("large Home items")
            .len(),
        50
    );
    assert!(large_body["data"]["nextCursor"].is_string());

    let visible_workspace_ids = source_workspace_ids
        .iter()
        .copied()
        .chain(std::iter::once(target.id))
        .collect::<Vec<_>>();
    let target_workspace_ids = vec![target.id];
    let definition_ids = vec![Uuid::parse_str(&definition_id).expect("definition UUID")];
    let source_target_explain: Value = sqlx::query_scalar(
        r#"
        EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
        SELECT id AS source_workspace_id,
               COALESCE(merge_target_workspace_id, cloned_from_id) AS target_workspace_id
        FROM workspaces
        WHERE org_id = $1
          AND id = ANY($2)
          AND deleted_at IS NULL
          AND COALESCE(merge_target_workspace_id, cloned_from_id) IS NOT NULL
        ORDER BY id
        "#,
    )
    .bind(org.id)
    .bind(&visible_workspace_ids)
    .fetch_one(&app.db)
    .await
    .expect("EXPLAIN Home approval source targets");
    let active_config_explain: Value = sqlx::query_scalar(
        r#"
        EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
        WITH latest AS (
            SELECT DISTINCT ON (target_workspace_id, approval_definition_id)
                   target_workspace_id, approval_definition_id, retired, created_at
            FROM approval_configs
            WHERE org_id = $1
              AND target_workspace_id = ANY($2)
            ORDER BY target_workspace_id, approval_definition_id, seq DESC
        )
        SELECT target_workspace_id, approval_definition_id, created_at AS attached_at
        FROM latest
        WHERE retired = false
        ORDER BY target_workspace_id, approval_definition_id
        "#,
    )
    .bind(org.id)
    .bind(&target_workspace_ids)
    .fetch_one(&app.db)
    .await
    .expect("EXPLAIN Home active approval configs");
    let caller_decision_explain: Value = sqlx::query_scalar(
        r#"
        EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
        SELECT source_workspace_id, approval_definition_id,
               approval_definition_version, outcome, actor_type,
               actor_user_id, reviewed_git_heads, created_at
        FROM approval_statuses
        WHERE org_id = $1
          AND source_workspace_id = ANY($2)
          AND approval_definition_id = ANY($3)
          AND actor_type = 'human'
          AND actor_user_id = $4
        ORDER BY source_workspace_id, approval_definition_id, seq DESC
        "#,
    )
    .bind(org.id)
    .bind(&source_workspace_ids)
    .bind(&definition_ids)
    .bind(member.id)
    .fetch_one(&app.db)
    .await
    .expect("EXPLAIN Home caller approval decisions");
    let explain = json!({
        "sourceTargets": source_target_explain,
        "activeConfigs": active_config_explain,
        "callerDecisions": caller_decision_explain,
    });
    let explain_text = explain.to_string();
    assert!(explain_text.contains("idx_approval_configs_target"));
    assert!(explain_text.contains("idx_approval_statuses_eval"));
    println!(
        "HOME_APPROVAL_EXPLAIN_INDEXES=idx_workspaces_org_live,\
         idx_approval_configs_target,idx_approval_statuses_eval"
    );
}
