//! Integration coverage for the final workspace materialization manifest.
//!
//! Public workspaces are created only by fork. Every repository in a workspace
//! uses the workspace's exact branch name, while `workspace_repos` owns the
//! repository inventory and root paths consumed by the manifest.

use crate::helpers;
use g5_auth::Scope;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::process::Command;
use uuid::Uuid;

fn run_git(repo_path: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(repo_path: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");
    assert!(output.status.success(), "git {:?} failed", args);
    String::from_utf8(output.stdout)
        .expect("git stdout should be UTF-8")
        .trim()
        .to_string()
}

async fn create_repo(app: &TestApp, org: &TestOrg, label: &str) -> (Uuid, String, String) {
    let repo_id = Uuid::new_v4();
    let name = format!("{label}-{repo_id}");
    let repo_path = app.init_repo(org.id, repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-B", "main"]);
    std::fs::write(
        repo_path.join("README.md"),
        format!("{label} manifest repo\n"),
    )
    .expect("seed README");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-m", "seed manifest repo"]);
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

    (repo_id, name, head)
}

async fn attach_repo(
    app: &TestApp,
    workspace: &TestWorkspace,
    repo_id: Uuid,
    head: &str,
    root_path: &str,
    role: &str,
) {
    sqlx::query(
        "INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role)
                 VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(repo_id)
    .bind(root_path)
    .bind(role)
    .execute(&app.db)
    .await
    .expect("insert canonical repository member");
    helpers::seed_workspace_authority_head(app, workspace.id, repo_id, head).await;
}

async fn manifest(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Value {
    let response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}/manifest"))
        .await;
    let status = response.status();
    let text = response.text().await.expect("manifest body");
    assert_eq!(status, StatusCode::OK, "{text}");
    serde_json::from_str(&text).expect("manifest JSON")
}

#[tokio::test]
async fn fork_manifest_pins_code_and_docs_to_the_exact_workspace_branch() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let source = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let (docs_repo_id, _docs_name, docs_head) = create_repo(&app, &org, "docs").await;
    attach_repo(&app, &source, docs_repo_id, &docs_head, ".g5/docs", "docs").await;
    sqlx::query(
        "UPDATE workspaces \
         SET docs_repo_id = $2 \
         WHERE id = $1",
    )
    .bind(source.id)
    .bind(docs_repo_id)
    .execute(&app.db)
    .await
    .expect("mark workspace docs repository");

    let source_manifest = manifest(&client, source.id).await;
    assert_eq!(
        source_manifest["data"]["repos"].as_array().unwrap().len(),
        1
    );
    assert_eq!(source_manifest["data"]["docs"]["branch"], source.name);
    assert_eq!(source_manifest["data"]["docs"]["commit"], docs_head);
    assert_eq!(
        source_manifest["data"]["docs"]["repo_id"],
        docs_repo_id.to_string()
    );

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", source.id),
            &json!({
                "name": "manifest-clone",
                "idempotencyKey": "manifest-clone-contract"
            }),
        )
        .await;
    let status = response.status();
    let text = response.text().await.expect("fork body");
    assert_eq!(status, StatusCode::CREATED, "{text}");
    let body: Value = serde_json::from_str(&text).expect("fork JSON");
    let cloned = &body["data"]["workspace"];
    let clone_id = Uuid::parse_str(cloned["id"].as_str().expect("clone id")).unwrap();
    assert!(cloned["branches"]
        .as_array()
        .expect("clone branches")
        .iter()
        .all(|branch| branch["branchName"] == "manifest-clone"));

    let clone_manifest = manifest(&client, clone_id).await;
    assert_eq!(clone_manifest["data"]["workspace_id"], clone_id.to_string());
    assert_eq!(clone_manifest["data"]["docs"]["branch"], "manifest-clone");
    assert_eq!(clone_manifest["data"]["docs"]["commit"], docs_head);
    let code = &clone_manifest["data"]["repos"][0];
    assert_eq!(code["branch"], "manifest-clone");
    assert_eq!(code["commit"], source.head_commit);
    assert_eq!(
        code["url"],
        format!("{}/git/{}", app.state.git_external_url, source.repo_id)
    );

    app.cleanup().await;
}

#[tokio::test]
async fn manifest_uses_the_complete_multi_repo_mapping() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let reader = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let (repo_id, repo_name, head) = create_repo(&app, &org, "backend").await;
    attach_repo(&app, &workspace, repo_id, &head, "services/backend", "code").await;

    let body = manifest(&app.authenticated_client(&reader), workspace.id).await;
    let repos = body["data"]["repos"].as_array().expect("manifest repos");
    assert_eq!(repos.len(), 2);
    let added = repos
        .iter()
        .find(|repo| repo["name"] == repo_name)
        .expect("mapped backend repository");
    assert_eq!(added["branch"], workspace.name);
    assert_eq!(added["commit"], head);
    assert_eq!(added["path"], "services/backend");
    assert!(repos.iter().all(|repo| repo["branch"] == workspace.name));

    app.cleanup().await;
}

#[tokio::test]
async fn manifest_fails_closed_when_canonical_membership_is_empty() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let reader = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("remove exact repository child binding");

    let response = app
        .authenticated_client(&reader)
        .get(&format!("/api/v1/workspaces/{}/manifest", workspace.id))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("error JSON");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

    app.cleanup().await;
}

#[tokio::test]
async fn manifest_requires_workspace_read_scope() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[]).await;
    let response = app
        .authenticated_client(&user)
        .get(&format!("/api/v1/workspaces/{}/manifest", workspace.id))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
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

async fn internal_manifest(app: &TestApp, token: &str, workspace: Uuid) -> (StatusCode, Value) {
    let response = app
        .client
        .get(format!(
            "{}/api/internal/workspaces/{workspace}/manifest",
            app.base_url
        ))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}

/// The committed-cut capture releases the admission transaction before
/// fencing, so the incarnation is checked again once the cut is read: a VM
/// replaced while its manifest was captured receives no manifest.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn internal_manifest_refuses_an_incarnation_replaced_during_capture() {
    use g5_api::services::git_test_barrier::GitTestBarrier;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let token = mint_vm(&app, org.id, workspace.id, owner.id).await;

    let (status, body) = internal_manifest(&app, &token, workspace.id).await;
    assert_eq!(status, StatusCode::OK, "live incarnation: {body}");
    assert_eq!(body["data"]["repos"][0]["commit"], workspace.head_commit);

    let barrier = GitTestBarrier::after_member_read(workspace.repo_id, &workspace.name);
    let request = {
        let client = app.client.clone();
        let url = format!(
            "{}/api/internal/workspaces/{}/manifest",
            app.base_url, workspace.id
        );
        let token = token.clone();
        tokio::spawn(async move { client.get(url).bearer_auth(token).send().await.unwrap() })
    };
    barrier.reached().await;
    // Admission already passed; the capture is mid-read when the VM is replaced.
    sqlx::query("UPDATE workspace_vms SET epoch = epoch + 1 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("replace the incarnation");
    barrier.release();
    let response = request.await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
}
