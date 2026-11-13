//! Regression guard from the qa e2e failures after the workspace VM MVP (#117):
//! the user who forks a workspace and owns its running VM must not be refused
//! on `GET /workspaces/{id}/changes` or `POST /workspaces/{id}/docs`, whether
//! they are the org owner or a plain member.
//!
//! Cause: the owner Changes query drained the execution journal
//! (`events.replay`/`events.ack`) with the owner's actor context, and the
//! dispatcher keeps those methods private to the lifecycle authority. The test
//! peer applies the dispatcher's authority rule and journals one terminal
//! event per Changes execution, so both the replay and the ack are exercised:
//! the live reads below fail with the e2e symptom ("Insufficient permissions")
//! if either is sent under the owner's actor.
//!
//! Mirrors the browser e2e sequence: fork the base workspace, stand in a
//! running VM owned by the forking user, read the live Changes report, then
//! create a project doc (which writes through the same association guard).

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::process::Command;
use uuid::Uuid;

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

fn run_git(repo_path: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed in {repo_path:?}");
}

fn git_stdout(repo_path: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {repo_path:?}"
    );
    String::from_utf8(output.stdout)
        .expect("git output is utf-8")
        .trim()
        .to_string()
}

/// A seeded repository the API can serve as a workspace member.
async fn create_repo(app: &TestApp, org: &TestOrg, label: &str) -> (Uuid, String) {
    let repo_id = Uuid::new_v4();
    let name = format!("{label}-{repo_id}");
    let repo_path = app.init_repo(org.id, repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-B", "main"]);
    std::fs::write(repo_path.join("README.md"), format!("{label} repo\n")).expect("seed README");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "seed repo"]);
    let head = git_stdout(&repo_path, &["rev-parse", "HEAD"]);

    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) \
         VALUES ($1, $2, $3, $3, $4, 'main', NULL)",
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(&name)
    .bind(repo_path.display().to_string())
    .execute(&app.db)
    .await
    .expect("insert repository");

    (repo_id, head)
}

/// Attach `repo_id` to the workspace as an exact, materialized member branch.
async fn attach_repo(
    app: &TestApp,
    workspace: &TestWorkspace,
    repo_id: Uuid,
    head: &str,
    root_path: &str,
    role: &str,
) {
    sqlx::query("INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role) VALUES ($1,$2,$3,$4,$5)")
        .bind(workspace.org_id).bind(workspace.id).bind(repo_id).bind(root_path).bind(role)
        .execute(&app.db).await.unwrap();
    helpers::seed_workspace_authority_head(app, workspace.id, repo_id, head).await;
}

/// Fork `base` exactly like frontend/e2e/helpers.ts cloneWorkspaceById does.
async fn fork(client: &helpers::AuthenticatedClient, base_id: Uuid, name: &str) -> Uuid {
    let cloned = client
        .post_json(
            &format!("/api/v1/workspaces/{base_id}/clone"),
            &json!({
                "name": name,
                "idempotencyKey": Uuid::new_v4(),
                "description": "Isolated browser E2E clone (repro)",
            }),
        )
        .await;
    let status = cloned.status();
    let cloned: Value = cloned.json().await.expect("parse clone response");
    assert_eq!(status, StatusCode::CREATED, "clone failed: {cloned}");
    let child_id = data(&cloned)["workspace"]["id"]
        .as_str()
        .or_else(|| data(&cloned)["id"].as_str())
        .expect("clone response carries the child id")
        .to_string();
    Uuid::parse_str(&child_id).expect("child id is a uuid")
}

/// Replace the fork's published (stopped) incarnation with a running test
/// peer owned by `owner_id`.
async fn attach_running_peer(
    app: &TestApp,
    base: &TestWorkspace,
    child_id: Uuid,
    name: &str,
    owner_id: Uuid,
) -> TestWorkspaceRuntimePeer {
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
        .bind(child_id)
        .execute(&app.db)
        .await
        .expect("clear the fork's published incarnation");
    let child = TestWorkspace {
        id: child_id,
        org_id: base.org_id,
        repo_id: base.repo_id,
        head_commit: base.head_commit.clone(),
        name: name.to_string(),
    };
    TestWorkspaceRuntimePeer::attach(app, &child, owner_id).await
}

async fn read_live_changes(client: &helpers::AuthenticatedClient, child_id: Uuid, who: &str) {
    let live = client
        .get(&format!("/api/v1/workspaces/{child_id}/changes"))
        .await;
    let live_status = live.status();
    let live_body: Value = live.json().await.unwrap_or(Value::Null);
    assert_eq!(
        live_status,
        StatusCode::OK,
        "{who} live changes read refused: {live_body}"
    );
    assert_eq!(data(&live_body)["source"], "live");
}

/// The Changes drain must both replay and acknowledge the execution journal;
/// the peer refuses either under the owner's actor.
async fn assert_journal_drained(runtime: &TestWorkspaceRuntimePeer) {
    let methods = runtime.received_rpc_methods().await;
    for expected in ["modules.invoke", "events.replay", "events.ack"] {
        assert!(
            methods.iter().any(|method| method == expected),
            "Changes query must send {expected}; saw {methods:?}"
        );
    }
}

#[tokio::test]
async fn forking_owner_can_read_changes_and_create_docs_on_the_fork() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let (docs_repo_id, docs_head) = create_repo(&app, &org, "docs").await;
    attach_repo(&app, &base, docs_repo_id, &docs_head, ".g5/docs", "docs").await;
    sqlx::query(
        "UPDATE workspaces \
         SET docs_repo_id = $2 \
         WHERE id = $1",
    )
    .bind(base.id)
    .bind(docs_repo_id)
    .execute(&app.db)
    .await
    .expect("mark the base workspace's docs repository");
    let client = app.authenticated_client(&owner);

    let child_id = fork(&client, base.id, "e2e-clone-repro").await;

    let owner_row: (Option<Uuid>,) =
        sqlx::query_as("SELECT owner_user_id FROM workspaces WHERE id = $1")
            .bind(child_id)
            .fetch_one(&app.db)
            .await
            .expect("load fork");
    assert_eq!(
        owner_row.0,
        Some(owner.id),
        "fork must be owned by the forking user"
    );

    // Durable path: no VM attached yet.
    let durable = client
        .get(&format!("/api/v1/workspaces/{child_id}/changes"))
        .await;
    let durable_status = durable.status();
    let durable_body: Value = durable.json().await.unwrap_or(Value::Null);
    assert_eq!(
        durable_status,
        StatusCode::OK,
        "durable changes read refused: {durable_body}"
    );

    // Live path: the fork's VM runs, owned by the same user.
    let runtime = attach_running_peer(&app, &base, child_id, "e2e-clone-repro", owner.id).await;
    read_live_changes(&client, child_id, "owner").await;
    assert_journal_drained(&runtime).await;

    // Doc create writes the new file through write_workspace_file_for_actor,
    // whose association guard is the same Changes query. It must succeed end
    // to end on the fork's docs branch and read back.
    let doc = client
        .post_json(
            &format!("/api/v1/workspaces/{child_id}/docs"),
            &json!({
                "name": "repro-doc",
                "docType": "note",
                "filePath": format!("docs/{child_id}/repro-doc.md"),
                "content": "# repro\n",
            }),
        )
        .await;
    let doc_status = doc.status();
    let doc_body: Value = doc.json().await.unwrap_or(Value::Null);
    assert_eq!(
        doc_status,
        StatusCode::CREATED,
        "owner doc create failed: {doc_body}"
    );
    let doc_id = data(&doc_body)["id"]
        .as_str()
        .expect("created doc carries an id")
        .to_string();
    let readback = client
        .get(&format!("/api/v1/workspaces/{child_id}/docs/{doc_id}"))
        .await;
    let readback_status = readback.status();
    let readback_body: Value = readback.json().await.unwrap_or(Value::Null);
    assert_eq!(
        readback_status,
        StatusCode::OK,
        "created doc readback failed: {readback_body}"
    );
    assert_eq!(data(&readback_body)["id"], doc_id.as_str());
    assert!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "fs.write"),
        "doc create must write the file into the running incarnation"
    );

    // Design v9 §9.1: the attached owner's list reads the live docs surface,
    // so the doc just written (uncommitted, unflushed) lists at once as a
    // draft; a reader's list of the same workspace stays committed-only.
    let listed = client
        .get(&format!(
            "/api/v1/workspaces/{child_id}/docs?surface=workspace"
        ))
        .await;
    let listed_status = listed.status();
    let listed_body: Value = listed.json().await.unwrap_or(Value::Null);
    assert_eq!(listed_status, StatusCode::OK, "docs list: {listed_body}");
    let items = data(&listed_body)
        .as_array()
        .cloned()
        .or_else(|| data(&listed_body)["docs"].as_array().cloned())
        .unwrap_or_default();
    let live_item = items.iter().find(|item| {
        item["filePath"]
            .as_str()
            .is_some_and(|path| path.ends_with("repro-doc.md"))
    });
    let live_item = live_item.unwrap_or_else(|| {
        panic!("the live-created doc must list for the attached owner; got {listed_body}")
    });
    assert_eq!(
        live_item["draftDirty"], true,
        "an unflushed doc lists as a draft"
    );
    let handle = live_item["id"]
        .as_str()
        .expect("listed doc carries its repository handle")
        .to_string();
    let live_path = live_item["filePath"]
        .as_str()
        .expect("listed doc carries its path")
        .to_string();

    // The same live read backs the by-path wrapper, so the unflushed doc
    // opens with its live content.
    let by_path = client
        .get(&format!(
            "/api/v1/workspaces/{child_id}/docs/by-path?path={live_path}"
        ))
        .await;
    let by_path_status = by_path.status();
    let by_path_body: Value = by_path.json().await.unwrap_or(Value::Null);
    assert_eq!(
        by_path_status,
        StatusCode::OK,
        "by-path read of the live doc: {by_path_body}"
    );
    assert_eq!(data(&by_path_body)["content"], "# repro\n");
    assert_eq!(data(&by_path_body)["draftDirty"], true);
    let live_hash = data(&by_path_body)["contentHash"]
        .as_str()
        .expect("the owner's live read carries the file hash")
        .to_string();

    // A rewrite through the Project Docs handle resolves the same live doc
    // and writes back into the incarnation; the attached owner's open
    // question count then reads the live file, uncommitted and unflushed.
    let writes_before = runtime
        .received_rpc_methods()
        .await
        .iter()
        .filter(|method| *method == "fs.write")
        .count();
    let rewritten = client
        .put_json(
            &format!("/api/v1/workspaces/{child_id}/docs/project-docs/{handle}"),
            &json!({
                "content": doc_with_one_open_question(),
                "expectedHash": live_hash,
            }),
        )
        .await;
    let rewritten_status = rewritten.status();
    let rewritten_body: Value = rewritten.json().await.unwrap_or(Value::Null);
    assert_eq!(
        rewritten_status,
        StatusCode::OK,
        "rewrite of the live doc: {rewritten_body}"
    );
    let writes_after = runtime
        .received_rpc_methods()
        .await
        .iter()
        .filter(|method| *method == "fs.write")
        .count();
    assert_eq!(
        writes_after,
        writes_before + 1,
        "the rewrite must write the file into the running incarnation"
    );
    assert_eq!(
        owner_document_open_questions(&client, org.id, child_id).await,
        1,
        "the attached owner counts the question in the live, unflushed doc"
    );

    // Deleting through the handle removes the live file; the doc is gone for
    // the owner's reads and counts even though nothing was flushed.
    let deleted = client
        .delete(&format!(
            "/api/v1/workspaces/{child_id}/docs/project-docs/{handle}"
        ))
        .await;
    assert_eq!(
        deleted.status(),
        StatusCode::NO_CONTENT,
        "delete of the live doc"
    );
    let gone = client
        .get(&format!(
            "/api/v1/workspaces/{child_id}/docs/by-path?path={live_path}"
        ))
        .await;
    assert_eq!(
        gone.status(),
        StatusCode::NOT_FOUND,
        "a live-deleted doc no longer opens by path"
    );
    assert_eq!(
        owner_document_open_questions(&client, org.id, child_id).await,
        0,
        "the count follows the live delete"
    );

    app.cleanup().await;
}

fn doc_with_one_open_question() -> String {
    [
        "# repro",
        "",
        "[Q1. Fixture question]",
        "",
        "```g5-question",
        "schema: 1",
        "id: fork-owner-live-1",
        "status: open",
        "prompt: Keep the current behaviour?",
        "severity: medium",
        "multi_select: false",
        "allow_free_text: true",
        "options:",
        "  - label: Keep",
        "    description: Keep the current behaviour.",
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

/// The caller's `documentOpenQuestions` for one workspace from the org
/// metrics route.
async fn owner_document_open_questions(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    workspace_id: Uuid,
) -> i64 {
    let response = client
        .get(&format!(
            "/api/v1/orgs/{org_id}/workspaces/open-question-metrics?workspaceIds={workspace_id}"
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "open-question metrics: {body}");
    let metric = data(&body)
        .as_array()
        .and_then(|metrics| {
            metrics
                .iter()
                .find(|metric| metric["workspaceId"].as_str() == Some(&workspace_id.to_string()))
        })
        .unwrap_or_else(|| panic!("metrics carry the workspace: {body}"))
        .clone();
    metric["documentOpenQuestions"]
        .as_i64()
        .unwrap_or_else(|| panic!("documentOpenQuestions is a number: {metric}"))
}

/// Design v9 §5.7 / §9.2: on an attached VM every UI mutation checks the
/// live association first; a foreign checkout refuses Save and doc create
/// with WORKSPACE_BRANCH_MISMATCH carrying the report, and no file write or
/// lifecycle command reaches the VM. The report itself stays readable so the
/// A live Changes read that gives up on its VM execution cancels it, so an
/// abandoned run cannot keep holding one of `workspace.changes`' few slots.
#[tokio::test]
async fn an_abandoned_live_changes_read_cancels_its_vm_execution() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime =
        TestWorkspaceRuntimePeer::attach_with_unresponsive_changes(&app, &workspace, owner.id)
            .await;

    let report = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/workspaces/{}/changes", workspace.id))
        .await;
    assert_eq!(report.status(), StatusCode::SERVICE_UNAVAILABLE);
    let methods = runtime.received_rpc_methods().await;
    let invoke = methods
        .iter()
        .position(|method| method == "modules.invoke:workspace.changes/get")
        .expect("the read started a Changes execution");
    assert!(
        methods[invoke..]
            .iter()
            .any(|method| method == "modules.cancel"),
        "the abandoned execution must be cancelled: {methods:?}"
    );
}

/// A live Changes read whose VM execution is still running at the caller's
/// deadline fails at that deadline and then cancels the execution. (Slow: the
/// budget is the production 30 s.)
#[tokio::test]
async fn a_live_changes_read_cancels_its_vm_execution_at_the_deadline() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime =
        TestWorkspaceRuntimePeer::attach_with_endless_changes(&app, &workspace, owner.id).await;

    let started = std::time::Instant::now();
    let report = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/workspaces/{}/changes", workspace.id))
        .await;
    let elapsed = started.elapsed();
    let status = report.status();
    let body: Value = report.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.to_string().contains("exceeded its deadline"),
        "the read fails at its deadline: {body}"
    );
    assert!(
        elapsed >= std::time::Duration::from_secs(29),
        "the read waited out its budget: {elapsed:?}"
    );
    let methods = runtime.received_rpc_methods().await;
    let last_status = methods
        .iter()
        .rposition(|method| method == "modules.status")
        .expect("the read polled the running execution");
    let cancel = methods
        .iter()
        .position(|method| method == "modules.cancel")
        .expect("the execution must be cancelled at the deadline");
    assert!(
        cancel > last_status,
        "cancel follows the final poll: {methods:?}"
    );
}

/// owner can switch back.
#[tokio::test]
async fn ui_mutations_refuse_a_branch_mismatch_before_any_write() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let (docs_repo_id, docs_head) = create_repo(&app, &org, "docs").await;
    attach_repo(&app, &base, docs_repo_id, &docs_head, ".g5/docs", "docs").await;
    sqlx::query(
        "UPDATE workspaces \
         SET docs_repo_id = $2 \
         WHERE id = $1",
    )
    .bind(base.id)
    .bind(docs_repo_id)
    .execute(&app.db)
    .await
    .expect("mark the base workspace's docs repository");
    let client = app.authenticated_client(&owner);
    let child_id = fork(&client, base.id, "e2e-mismatch-fork").await;
    let branch: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
        .bind(child_id)
        .fetch_one(&app.db)
        .await
        .expect("fork branch name");

    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
        .bind(child_id)
        .execute(&app.db)
        .await
        .expect("clear the fork's published incarnation");
    let child = TestWorkspace {
        id: child_id,
        org_id: org.id,
        repo_id: base.repo_id,
        head_commit: base.head_commit.clone(),
        name: "e2e-mismatch-fork".to_string(),
    };
    let foreign = json!([{
        "repo_id": docs_repo_id,
        "root_path": ".g5/docs",
        "shared_branch": branch,
        "upstream_branch": "main",
        "head": "feature/elsewhere",
        "head_oid": docs_head,
        "on_shared_branch": false,
        "detached_head": false,
        "foreign_head": true,
        "internal_ref_present": true,
        "internal_ref": docs_head,
        "against_internal": {"ahead": 0, "behind": 0},
        "against_base": {"ahead": 0, "behind": 0},
        "dirty": [],
        "errors": ["BRANCH_ASSOCIATION_MISMATCH"],
        "sequencer": [],
        "conflicts": [],
    }]);
    let runtime =
        TestWorkspaceRuntimePeer::attach_with_changes_report(&app, &child, owner.id, foreign).await;

    // The report stays readable and names the mismatch.
    let report = client
        .get(&format!("/api/v1/workspaces/{child_id}/changes"))
        .await;
    let report_status = report.status();
    let report_body: Value = report.json().await.unwrap_or(Value::Null);
    assert_eq!(report_status, StatusCode::OK, "report: {report_body}");
    assert_eq!(data(&report_body)["source"], "live");
    assert_eq!(
        data(&report_body)["members"][0]["errors"][0],
        "BRANCH_ASSOCIATION_MISMATCH"
    );
    let calls_before = runtime.received_rpc_methods().await.len();

    let save = client
        .put_json(
            &format!("/api/v1/workspaces/{child_id}/files/blocked.txt?repoId={docs_repo_id}"),
            &json!({"content": "blocked\n", "encoding": "utf-8", "expectedHash": null}),
        )
        .await;
    let save_status = save.status();
    let save_body: Value = save.json().await.unwrap_or(Value::Null);
    assert_eq!(save_status, StatusCode::CONFLICT, "save: {save_body}");
    assert_eq!(save_body["error"]["code"], "WORKSPACE_BRANCH_MISMATCH");
    assert_eq!(
        save_body["error"]["details"]["members"][0]["errors"][0], "BRANCH_ASSOCIATION_MISMATCH",
        "the refusal carries the association report"
    );

    let doc = client
        .post_json(
            &format!("/api/v1/workspaces/{child_id}/docs"),
            &json!({
                "name": "blocked-doc",
                "docType": "note",
                "filePath": "blocked-doc.md",
                "content": "# blocked\n",
            }),
        )
        .await;
    let doc_status = doc.status();
    let doc_body: Value = doc.json().await.unwrap_or(Value::Null);
    assert_eq!(doc_status, StatusCode::CONFLICT, "doc create: {doc_body}");
    assert_eq!(doc_body["error"]["code"], "WORKSPACE_BRANCH_MISMATCH");

    let methods = runtime.received_rpc_methods().await;
    assert!(
        !methods[calls_before..]
            .iter()
            .any(|method| method == "fs.write"),
        "a mismatched association must refuse before any file write; saw {methods:?}"
    );
    assert!(
        runtime.received_host_command_methods().await.is_empty(),
        "a refused Save must send no lifecycle command"
    );

    app.cleanup().await;
}

/// Design v9 §4.3 invariant 12 / §9.1: a non-owner's repository reads always
/// resolve durable Git and never touch the owner's running VM, and a
/// non-owner's writes and compute controls are refused without any VM call.
#[tokio::test]
async fn reader_routes_resolve_durable_git_and_make_zero_vm_calls() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let (docs_repo_id, docs_head) = create_repo(&app, &org, "docs").await;
    attach_repo(&app, &base, docs_repo_id, &docs_head, ".g5/docs", "docs").await;
    sqlx::query(
        "UPDATE workspaces \
         SET docs_repo_id = $2 \
         WHERE id = $1",
    )
    .bind(base.id)
    .bind(docs_repo_id)
    .execute(&app.db)
    .await
    .expect("mark the base workspace's docs repository");
    let owner_client = app.authenticated_client(&owner);
    let child_id = fork(&owner_client, base.id, "e2e-reader-fork").await;
    let runtime = attach_running_peer(&app, &base, child_id, "e2e-reader-fork", owner.id).await;
    // The owner's own live read proves the VM is reachable for the owner.
    read_live_changes(&owner_client, child_id, "owner").await;
    let owner_calls = runtime.received_rpc_methods().await.len();
    assert!(owner_calls > 0, "owner live read must reach the VM");

    let reader = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
            g5_auth::Scope::TasksRead,
            g5_auth::Scope::TasksExecute,
        ],
    )
    .await;
    sqlx::query(
        "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member') \
         ON CONFLICT (org_id, user_id) DO UPDATE SET role = 'member'",
    )
    .bind(org.id)
    .bind(reader.id)
    .execute(&app.db)
    .await
    .expect("add reader");
    let client = app.authenticated_client(&reader);

    // Durable reads.
    let changes = client
        .get(&format!("/api/v1/workspaces/{child_id}/changes"))
        .await;
    let changes_status = changes.status();
    let changes_body: Value = changes.json().await.unwrap_or(Value::Null);
    assert_eq!(
        changes_status,
        StatusCode::OK,
        "reader changes refused: {changes_body}"
    );
    assert_eq!(data(&changes_body)["source"], "durable");
    assert_eq!(data(&changes_body)["attached"], false);
    let file = client
        .get(&format!(
            "/api/v1/workspaces/{child_id}/files/README.md?repoId={docs_repo_id}"
        ))
        .await;
    let file_status = file.status();
    let file_body: Value = file.json().await.unwrap_or(Value::Null);
    assert_eq!(file_status, StatusCode::OK, "reader file read: {file_body}");
    assert_eq!(data(&file_body)["source"], "committed");
    let docs = client
        .get(&format!(
            "/api/v1/workspaces/{child_id}/docs?surface=workspace"
        ))
        .await;
    assert_eq!(docs.status(), StatusCode::OK);

    // Writes and compute controls are refused.
    let write = client
        .put_json(
            &format!("/api/v1/workspaces/{child_id}/files/reader.txt?repoId={docs_repo_id}"),
            &json!({"content": "no\n", "encoding": "utf-8", "expectedHash": null}),
        )
        .await;
    assert_eq!(write.status(), StatusCode::FORBIDDEN);
    for action in ["stop", "resume"] {
        let refused = client
            .post_json(
                &format!("/api/v1/workspaces/{child_id}/{action}"),
                &json!({}),
            )
            .await;
        assert_eq!(
            refused.status(),
            StatusCode::FORBIDDEN,
            "reader {action} must be forbidden"
        );
    }
    let doc = client
        .post_json(
            &format!("/api/v1/workspaces/{child_id}/docs"),
            &json!({
                "name": "reader-doc",
                "docType": "note",
                "filePath": "reader-doc.md",
                "content": "",
            }),
        )
        .await;
    assert_eq!(doc.status(), StatusCode::FORBIDDEN);

    // None of the reader's requests reached the owner's incarnation, and no
    // lifecycle command was sent on the reader's behalf.
    assert_eq!(
        runtime.received_rpc_methods().await.len(),
        owner_calls,
        "reader routes must make zero VM calls"
    );
    assert!(
        runtime.received_host_command_methods().await.is_empty(),
        "reader routes must send no host lifecycle command"
    );

    app.cleanup().await;
}

/// Same sequence as the browser suite runs it in CI: the acting user is a
/// plain org member (the OAuth identity), not the org owner who provisioned
/// the fixtures. The member forks Base, owns the fork and its running VM, and
/// must be able to read the live Changes report.
#[tokio::test]
async fn forking_member_can_read_changes_on_the_fork() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create(&app, &org).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
            g5_auth::Scope::TasksRead,
        ],
    )
    .await;
    sqlx::query(
        "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member') \
         ON CONFLICT (org_id, user_id) DO UPDATE SET role = 'member'",
    )
    .bind(org.id)
    .bind(member.id)
    .execute(&app.db)
    .await
    .expect("add member");
    let client = app.authenticated_client(&member);

    let child_id = fork(&client, base.id, "e2e-member-clone").await;

    let durable = client
        .get(&format!("/api/v1/workspaces/{child_id}/changes"))
        .await;
    let durable_status = durable.status();
    let durable_body: Value = durable.json().await.unwrap_or(Value::Null);
    assert_eq!(
        durable_status,
        StatusCode::OK,
        "member durable read refused: {durable_body}"
    );

    let runtime = attach_running_peer(&app, &base, child_id, "e2e-member-clone", member.id).await;
    read_live_changes(&client, child_id, "member").await;
    assert_journal_drained(&runtime).await;

    app.cleanup().await;
}
