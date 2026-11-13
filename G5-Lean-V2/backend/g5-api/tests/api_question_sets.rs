//! Referenced question sets over the real API, Postgres, Git storage and the
//! scripted workspace runtime (`g5.documents.question.*`,
//! `g5.architecture.question_owner`).
//!
//! The owner's answer is an explicit Save of the one set file: the live file
//! is written under its hash and VM, then the selected-file checkpoint runs.
//! Agents propose through the incarnation routes and their writes stay live.
//! Readers and stopped workspaces read the committed cut and cannot write.

use crate::helpers;

use g5_contracts::types::question_set::question_set_path;
use helpers::question_sets::{data, json_of, plan_set, Fixture};
use helpers::{TestApp, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn answer_body(path: &str, revision: u64, key: &str, option: &str) -> Value {
    json!({
        "path": path,
        "questionId": "pick-0123abcd",
        "expectedRevision": revision,
        "idempotencyKey": key,
        "selectedOptionIds": [option],
        "text": "",
        "rationale": "",
    })
}

fn invokes(methods: &[String], operation: &str) -> usize {
    methods
        .iter()
        .filter(|method| *method == &format!("modules.invoke:workspace.changes/{operation}"))
        .count()
}

#[tokio::test]
async fn owner_answer_checkpoints_the_set_file_and_replays_only_the_same_request() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    let set = plan_set("plan");
    fx.commit_docs(&path, &set).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&fx.app, &fx.workspace, fx.owner.id).await;
    runtime.seed_file(fx.docs_repo_id, &path, &set).await;
    let client = fx.app.authenticated_client(&fx.owner);

    let (status, body) = json_of(
        client
            .get(&format!("{}?path={path}", fx.questions_url("/set")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["source"], "live");
    assert_eq!(data(&body)["answerable"], true);
    assert_eq!(data(&body)["questions"][0]["decision"], false);

    let before = runtime.received_rpc_methods().await;
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "tab-a", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let outcome = data(&body);
    assert_eq!(outcome["publication"]["status"], "published");
    assert_eq!(outcome["replayed"], false);
    assert_eq!(outcome["entry"]["status"], "answered");
    assert_eq!(outcome["entry"]["decision"], true);
    assert_eq!(outcome["entry"]["revision"], 2);
    assert_eq!(outcome["entry"]["answer"]["answeredBy"]["kind"], "user");
    assert_eq!(
        outcome["entry"]["answer"]["answeredBy"]["id"],
        fx.owner.id.to_string()
    );
    // The whole file was written live and then committed and pushed.
    let live = runtime.file_content(fx.docs_repo_id, &path).await.unwrap();
    assert!(live.contains("\"status\": \"answered\""), "{live}");
    let after = runtime.received_rpc_methods().await;
    assert_eq!(invokes(&after, "commit") - invokes(&before, "commit"), 1);
    assert_eq!(invokes(&after, "push") - invokes(&before, "push"), 1);

    // The same request replays without a second revision or write.
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "tab-a", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["replayed"], true);
    assert_eq!(data(&body)["entry"]["revision"], 2);
    assert_eq!(
        runtime.file_content(fx.docs_repo_id, &path).await.unwrap(),
        live
    );

    // The same key with another payload conflicts.
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "tab-a", "database"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "QUESTION_CONFLICT");
    assert_eq!(body["error"]["details"]["code"], "idempotency-key-reused");

    // A second, stale tab is refused with the current entry.
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "tab-b", "database"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["details"]["code"], "revision-moved");
    assert_eq!(body["error"]["details"]["currentRevision"], 2);
    assert_eq!(
        body["error"]["details"]["current"]["answer"]["selectedOptionIds"],
        json!(["set-file"])
    );

    // A body that names its own provenance is refused.
    let mut forged = answer_body(&path, 2, "tab-c", "database");
    forged["answeredBy"] = json!({"kind": "user", "id": Uuid::new_v4()});
    let (status, body) = json_of(
        client
            .post_json(&fx.questions_url("/answer"), &forged)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["error"], "provenance-not-accepted");

    // The owner clears the answer: another explicit, published Save.
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/clear"),
                &json!({"path": path, "questionId": "pick-0123abcd", "expectedRevision": 2, "idempotencyKey": "tab-d"}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["entry"]["status"], "open");
    assert_eq!(data(&body)["publication"]["status"], "published");

    fx.app.cleanup().await;
}

#[tokio::test]
async fn a_failed_push_is_applied_not_published_and_retries_only_that_version() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    let set = plan_set("plan");
    fx.commit_docs(&path, &set).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_changes_failure(
        &fx.app,
        &fx.workspace,
        fx.owner.id,
        "push",
    )
    .await;
    runtime.seed_file(fx.docs_repo_id, &path, &set).await;
    let client = fx.app.authenticated_client(&fx.owner);

    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "k1", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let outcome = data(&body).clone();
    assert_eq!(
        outcome["publication"]["status"], "notPublished",
        "{outcome}"
    );
    assert_eq!(outcome["entry"]["status"], "answered");
    let applied_hash = outcome["hash"].as_str().unwrap().to_string();
    let applied = runtime.file_content(fx.docs_repo_id, &path).await.unwrap();

    // A lost response (no applied hash) reports the applied version again.
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "k1", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["replayed"], true);
    assert_eq!(data(&body)["publication"]["status"], "notPublished");
    assert_eq!(data(&body)["hash"], applied_hash.as_str());

    // A retry naming another version is refused.
    let mut wrong = answer_body(&path, 1, "k1", "set-file");
    wrong["appliedHash"] = json!("0000000000000000000000000000000000000000");
    let (status, body) =
        json_of(client.post_json(&fx.questions_url("/answer"), &wrong).await).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["details"]["code"], "reload-required");

    // Once the push works, the retry publishes exactly the applied bytes
    // without a second transition.
    runtime.allow_changes().await;
    let mut retry = answer_body(&path, 1, "k1", "set-file");
    retry["appliedHash"] = json!(applied_hash);
    let (status, body) =
        json_of(client.post_json(&fx.questions_url("/answer"), &retry).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["publication"]["status"], "published");
    assert_eq!(data(&body)["replayed"], true);
    assert_eq!(data(&body)["entry"]["revision"], 2);
    assert_eq!(
        runtime.file_content(fx.docs_repo_id, &path).await.unwrap(),
        applied
    );

    // A set that changed before the retry needs a reload.
    let reworded = applied.replace("Which storage backend?", "Which backend?");
    runtime.seed_file(fx.docs_repo_id, &path, &reworded).await;
    let (status, body) =
        json_of(client.post_json(&fx.questions_url("/answer"), &retry).await).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["details"]["code"], "reload-required");

    fx.app.cleanup().await;
}

#[tokio::test]
async fn stopped_workspaces_and_readers_see_committed_read_only_sets() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    fx.commit_docs(&path, &plan_set("plan")).await;
    let client = fx.app.authenticated_client(&fx.owner);

    // No attachment: the committed cut, not answerable, and no compute.
    let (status, body) = json_of(
        client
            .get(&format!("{}?path={path}", fx.questions_url("/set")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["source"], "committed");
    assert_eq!(data(&body)["answerable"], false);
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "k1", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING");
    let vms: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_vms WHERE workspace_id = $1")
        .bind(fx.workspace.id)
        .fetch_one(&fx.app.db)
        .await
        .unwrap();
    assert_eq!(vms, 0, "answering must never start compute");

    // A reader of the running workspace reads the committed set and cannot
    // write, and none of the reader's requests reach the owner's VM.
    let runtime = TestWorkspaceRuntimePeer::attach(&fx.app, &fx.workspace, fx.owner.id).await;
    runtime
        .seed_file(fx.docs_repo_id, &path, &plan_set("plan"))
        .await;
    let reader = TestUser::create_with_scopes(
        &fx.app,
        fx.org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    sqlx::query(
        "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member') \
         ON CONFLICT (org_id, user_id) DO UPDATE SET role = 'member'",
    )
    .bind(fx.org.id)
    .bind(reader.id)
    .execute(&fx.app.db)
    .await
    .unwrap();
    let calls = runtime.received_rpc_methods().await.len();
    let reader_client = fx.app.authenticated_client(&reader);
    let (status, body) = json_of(
        reader_client
            .get(&format!("{}?path={path}", fx.questions_url("/set")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["source"], "committed");
    assert_eq!(data(&body)["answerable"], false);
    let (status, body) = json_of(
        reader_client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "reader", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(runtime.received_rpc_methods().await.len(), calls);

    fx.app.cleanup().await;
}

async fn agent_request(
    app: &TestApp,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let mut request = reqwest::Client::new()
        .request(method, format!("{}{path}", app.base_url))
        .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(body);
    }
    json_of(request.send().await.unwrap()).await
}

#[tokio::test]
async fn agent_writes_are_live_proposals_the_owner_confirms() {
    let fx = Fixture::new().await;
    let runtime = TestWorkspaceRuntimePeer::attach(&fx.app, &fx.workspace, fx.owner.id).await;
    let token = fx
        .app
        .state
        .jwt_minter
        .as_ref()
        .expect("test jwt minter")
        .mint_workspace_incarnation(
            fx.org.id,
            fx.workspace.id,
            fx.owner.id,
            runtime.vm_id(),
            1,
            3600,
        )
        .unwrap();
    let base = format!("/api/internal/workspaces/{}/questions", fx.workspace.id);

    // The agent authors a set: a live write, no checkpoint.
    let before = runtime.received_rpc_methods().await;
    let (status, body) = agent_request(
        &fx.app,
        &token,
        reqwest::Method::POST,
        &base,
        Some(&json!({
            "setId": "agent-plan",
            "title": "Agent plan",
            "questions": [{
                "prompt": "Ship the cache?",
                "options": [{"label": "Yes", "recommended": true}, {"label": "No"}],
            }],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["created"], true);
    assert_eq!(data(&body)["publication"]["status"], "live");
    let path = data(&body)["set"]["path"].as_str().unwrap().to_string();
    assert_eq!(
        path,
        question_set_path(&fx.workspace.id.to_string(), "agent-plan")
    );
    let question_id = data(&body)["questionIds"][0].as_str().unwrap().to_string();
    let after = runtime.received_rpc_methods().await;
    assert_eq!(invokes(&after, "commit"), invokes(&before, "commit"));
    assert_eq!(invokes(&after, "push"), invokes(&before, "push"));
    let created = runtime.file_content(fx.docs_repo_id, &path).await.unwrap();
    assert!(created.contains("\"kind\": \"agent\""), "{created}");

    // The list shows decision flags.
    let (status, body) = agent_request(&fx.app, &token, reqwest::Method::GET, &base, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["sets"][0]["questions"][0]["decision"], false);

    // A body naming a person is refused; the route records the agent.
    let answer = |key: &str, revision: u64| {
        json!({
            "path": path,
            "questionId": question_id,
            "expectedRevision": revision,
            "idempotencyKey": key,
            "selectedOptionIds": ["yes"],
        })
    };
    let mut forged = answer("agent:agent-plan:q:1", 1);
    forged["answeredBy"] = json!({"kind": "user", "id": fx.owner.id});
    let (status, body) = agent_request(
        &fx.app,
        &token,
        reqwest::Method::POST,
        &format!("{base}/answer"),
        Some(&forged),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = agent_request(
        &fx.app,
        &token,
        reqwest::Method::POST,
        &format!("{base}/answer"),
        Some(&answer("agent:agent-plan:q:1", 1)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["entry"]["status"], "proposed");
    assert_eq!(data(&body)["entry"]["decision"], false);
    assert_eq!(
        data(&body)["entry"]["answer"]["answeredBy"]["kind"],
        "agent"
    );
    assert_eq!(data(&body)["publication"]["status"], "live");
    // A retried command replays.
    let (status, body) = agent_request(
        &fx.app,
        &token,
        reqwest::Method::POST,
        &format!("{base}/answer"),
        Some(&answer("agent:agent-plan:q:1", 1)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["replayed"], true);
    let after_agent = runtime.received_rpc_methods().await;
    assert_eq!(invokes(&after_agent, "commit"), invokes(&before, "commit"));

    // The owner confirms the proposal: a decision, checkpointed.
    let client = fx.app.authenticated_client(&fx.owner);
    let (status, body) = json_of(
        client
            .post_json(&fx.questions_url("/answer"), &answer("owner-confirm", 2))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["entry"]["decision"], true);
    assert_eq!(data(&body)["publication"]["status"], "published");

    // The agent can neither replace nor clear the person's decision.
    let (status, body) = agent_request(
        &fx.app,
        &token,
        reqwest::Method::POST,
        &format!("{base}/answer"),
        Some(&answer("agent:agent-plan:q:3", 3)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["error"],
        "foreign-answer-immutable"
    );

    // Another workspace's token cannot reach this workspace's sets.
    let other = TestWorkspace::create(&fx.app, &fx.org).await;
    let (status, _) = agent_request(
        &fx.app,
        &token,
        reqwest::Method::GET,
        &format!("/api/internal/workspaces/{}/questions", other.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    fx.app.cleanup().await;
}

#[tokio::test]
async fn unparseable_sets_are_refused_by_the_file_routes_and_shown_as_notices() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    let set = plan_set("plan");
    fx.commit_docs(&path, &set).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&fx.app, &fx.workspace, fx.owner.id).await;
    let hash = runtime.seed_file(fx.docs_repo_id, &path, &set).await;
    let client = fx.app.authenticated_client(&fx.owner);

    // The owner's file route refuses a broken set and a renamed one.
    let (status, body) = json_of(
        client
            .put_json(
                &format!(
                    "/api/v1/workspaces/{}/files/{path}?repoId={}",
                    fx.workspace.id, fx.docs_repo_id
                ),
                &json!({"content": "{ broken", "encoding": "utf-8", "expectedHash": hash}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["error"],
        "question-set-unparseable"
    );
    assert_eq!(
        runtime.file_content(fx.docs_repo_id, &path).await.unwrap(),
        set
    );
    let other_set = plan_set("other");
    let (status, body) = json_of(
        client
            .put_json(
                &format!(
                    "/api/v1/workspaces/{}/files/{path}?repoId={}",
                    fx.workspace.id, fx.docs_repo_id
                ),
                &json!({"content": other_set, "encoding": "utf-8", "expectedHash": hash}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Renaming a broken file onto a set path is refused the same way.
    let stray = format!("docs/{}/stray.json", fx.workspace.id);
    let stray_hash = runtime.seed_file(fx.docs_repo_id, &stray, "{ broken").await;
    let (status, body) = json_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/files/rename", fx.workspace.id),
                &json!({
                    "repoId": fx.docs_repo_id,
                    "from": stray,
                    "to": question_set_path(&fx.workspace.id.to_string(), "renamed"),
                    "expectedHash": stray_hash,
                }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["error"],
        "question-set-unparseable"
    );

    // A valid hand edit is simply the new state.
    let reworded = set.replace("Which storage backend?", "Which store?");
    let (status, body) = json_of(
        client
            .put_json(
                &format!(
                    "/api/v1/workspaces/{}/files/{path}?repoId={}",
                    fx.workspace.id, fx.docs_repo_id
                ),
                &json!({"content": reworded, "encoding": "utf-8", "expectedHash": hash}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A native edit nothing gates: readers see a notice, answers refuse.
    runtime
        .seed_file(fx.docs_repo_id, &path, "{ not a set")
        .await;
    let (status, body) = json_of(
        client
            .get(&format!("{}?path={path}", fx.questions_url("/set")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["invalid"]["code"], "invalid-json");
    assert_eq!(data(&body)["answerable"], false);
    let (status, body) = json_of(client.get(&fx.questions_url("")).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["sets"][0]["invalid"]["code"], "invalid-json");
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "k1", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["error"],
        "question-set-unparseable"
    );

    // Repaired as ordinary source, it is answerable again.
    runtime.seed_file(fx.docs_repo_id, &path, &set).await;
    let (status, body) = json_of(
        client
            .post_json(
                &fx.questions_url("/answer"),
                &answer_body(&path, 1, "k2", "set-file"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Paths outside the question set rule are refused.
    for bad in [
        "docs/../questions/plan.g5q.json",
        "docs/ws/questions/sub/plan.g5q.json",
        "README.md",
    ] {
        let (status, body) = json_of(
            client
                .get(&format!("{}?path={bad}", fx.questions_url("/set")))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }

    fx.app.cleanup().await;
}

#[tokio::test]
async fn a_fork_resolves_and_answers_an_inherited_set_independently() {
    let fx = Fixture::new().await;
    // The parent authored the set under its own id.
    let path = fx.set_path();
    let set = plan_set("plan");
    fx.commit_docs(&path, &set).await;
    let client = fx.app.authenticated_client(&fx.owner);
    let (status, cloned) = json_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/clone", fx.workspace.id),
                &json!({"name": "question-fork", "idempotencyKey": Uuid::new_v4()}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{cloned}");
    let child_id: Uuid = data(&cloned)["workspace"]["id"]
        .as_str()
        .or_else(|| data(&cloned)["id"].as_str())
        .unwrap()
        .parse()
        .unwrap();
    let docs_repo: Option<Uuid> =
        sqlx::query_scalar("SELECT docs_repo_id FROM workspaces WHERE id = $1")
            .bind(child_id)
            .fetch_one(&fx.app.db)
            .await
            .unwrap();
    assert_eq!(
        docs_repo,
        Some(fx.docs_repo_id),
        "a fork keeps the docs repository"
    );

    // The child reads the inherited path as given (its own id is different).
    let (status, body) = json_of(
        client
            .get(&format!(
                "/api/v1/workspaces/{child_id}/questions/set?path={path}"
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["questions"][0]["id"], "pick-0123abcd");
    // The child's own questions directory does not include the parent's set.
    let (status, body) = json_of(
        client
            .get(&format!("/api/v1/workspaces/{child_id}/questions"))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["sets"], json!([]));

    // The child's owner answers in the child; the parent is unchanged.
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
        .bind(child_id)
        .execute(&fx.app.db)
        .await
        .unwrap();
    let child = TestWorkspace {
        id: child_id,
        org_id: fx.org.id,
        repo_id: fx.workspace.repo_id,
        head_commit: fx.workspace.head_commit.clone(),
        name: "question-fork".to_string(),
    };
    let runtime = TestWorkspaceRuntimePeer::attach(&fx.app, &child, fx.owner.id).await;
    runtime.seed_file(fx.docs_repo_id, &path, &set).await;
    let (status, body) = json_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{child_id}/questions/answer"),
                &answer_body(&path, 1, "fork", "database"),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["entry"]["decision"], true);
    let (status, body) = json_of(
        client
            .get(&format!("{}?path={path}", fx.questions_url("/set")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["questions"][0]["status"], "open");

    fx.app.cleanup().await;
}
