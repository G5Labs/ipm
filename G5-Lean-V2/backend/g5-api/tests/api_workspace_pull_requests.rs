//! Local Ship to Base must publish Git refs, the merge run, and PR completion
//! together. These tests use real repositories and the production HTTP routes.

use crate::helpers;

use g5_api::services::coordinated_repo_publication::MergeWorkspacePullRequestOutcome;
use g5_scm::{GitAuthor, GitRepo};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

#[path = "provider_merge_targets.rs"]
mod provider_merge_targets;

struct Fixture {
    owner: TestUser,
    org: TestOrg,
    source: TestWorkspace,
    base_id: Uuid,
    other_repo_id: Uuid,
    initial_head: String,
    source_head: String,
    base_head: String,
    pull_request_id: Uuid,
}

fn vm_pr_token(
    app: &TestApp,
    fixture: &Fixture,
    peer: &helpers::TestWorkspaceRuntimePeer,
) -> String {
    app.state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(
            fixture.org.id,
            fixture.source.id,
            fixture.owner.id,
            peer.vm_id(),
            1,
            3600,
        )
        .unwrap()
}

async fn post_pr(app: &TestApp, token: &str, route: &str, body: &Value) -> reqwest::Response {
    app.client
        .post(format!("{}{route}", app.base_url))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn read_pr_summary(app: &TestApp, f: &Fixture) -> Value {
    let response = app
        .authenticated_client(&f.owner)
        .get(&format!(
            "/api/v1/workspaces/{}/pull-requests?role=source",
            f.source.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pr| pr["id"] == f.pull_request_id.to_string())
        .unwrap()
        .clone()
}

async fn remove_fixture_pr(app: &TestApp, fixture: &Fixture) {
    sqlx::query("DELETE FROM workspace_pull_requests WHERE id=$1")
        .bind(fixture.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
}

async fn wait_for_operation(peer: &helpers::TestWorkspaceRuntimePeer, operation: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if peer
                .received_rpc_methods()
                .await
                .iter()
                .any(|method| method == &format!("modules.invoke:workspace.changes/{operation}"))
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Create PR reached the workspace save operation");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_pr_saves_live_changes_before_evidence_and_retries_existing_pr() {
    assert_pr_saves_and_reuses(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vm_create_pr_saves_live_changes_and_ui_reuses_the_same_record() {
    assert_pr_saves_and_reuses(true).await;
}

async fn assert_pr_saves_and_reuses(vm: bool) {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &fixture.source,
        fixture.owner.id,
        "modules.invoke",
    )
    .await;
    let token = if vm {
        vm_pr_token(&app, &fixture, &peer)
    } else {
        fixture.owner.jwt.clone()
    };
    let route = format!(
        "/api/{}/workspaces/{}/pull-requests",
        if vm { "internal" } else { "v1" },
        fixture.source.id
    );
    if vm {
        // The VM path exercises Base fallback; the user path retains its parent.
        sqlx::query("UPDATE workspaces SET cloned_from_id=NULL WHERE id=$1")
            .bind(fixture.source.id)
            .execute(&app.db)
            .await
            .unwrap();
    }
    let request = json!({"title":"Initial explicit title"});
    let create = post_pr(&app, &token, &route, &request);
    let save = async {
        wait_for_operation(&peer, "commit").await;
        // Stand in for the guest commit/receive-pack. The API must capture
        // this new durable head, never the one from before the save.
        let saved = commit(
            &app,
            fixture.source.repo_id,
            &fixture.source.name,
            "draft.txt",
        )
        .await;
        peer.release_one_rpc();
        wait_for_operation(&peer, "push").await;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
        )
        .bind(fixture.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(count, 0, "PR must wait for the internal push");
        peer.release_one_rpc();
        saved
    };
    let (response, saved) = tokio::join!(create, save);
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let pr = &body["data"]["pullRequest"];
    assert!(
        pr["sourceHeads"]
            .as_array()
            .unwrap()
            .iter()
            .any(|repo| repo["repoId"] == fixture.source.repo_id.to_string()
                && repo["headCommit"] == saved),
        "{body}"
    );
    assert_eq!(pr["targetWorkspaceId"], fixture.base_id.to_string());
    assert_eq!(pr["title"], "Initial explicit title");
    let id = pr["id"].clone();
    peer.release_rpc_barrier();
    let response = post_pr(
        &app,
        &fixture.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", fixture.source.id),
        &json!({"description":"Updated through UI", "title":"Updated explicit title"}),
    )
    .await;
    let status = response.status();
    let retried: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{retried}");
    assert_eq!(retried["data"]["pullRequest"]["id"], id);
    assert_eq!(retried["data"]["alreadyExisted"], true);
    assert_eq!(
        retried["data"]["pullRequest"]["descriptionOverride"],
        "Updated through UI"
    );
    assert_eq!(
        retried["data"]["pullRequest"]["title"],
        "Updated explicit title"
    );
    peer.release_rpc_barrier();
    let response = post_pr(
        &app,
        &fixture.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", fixture.source.id),
        &json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let omitted: Value = response.json().await.unwrap();
    assert_eq!(
        omitted["data"]["pullRequest"]["title"],
        "Updated explicit title"
    );
    let stored: String =
        sqlx::query_scalar("SELECT title FROM workspace_pull_requests WHERE id=$1")
            .bind(id.as_str().unwrap().parse::<Uuid>().unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored, "Updated explicit title");
    let driven = peer.received_rpc_methods().await;
    assert_eq!(
        driven
            .iter()
            .filter(|m| *m == "modules.invoke:workspace.changes/push")
            .count(),
        3,
        "a retry must push even when the commit was already saved"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn create_pr_refuses_failed_save_without_creating_local_pr() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &fixture.source,
        fixture.owner.id,
        json!([{"repo_id":fixture.source.repo_id,"committed":false,"errors":["UNMERGED_PATHS"]}]),
    )
    .await;
    for (source, target) in [
        (fixture.base_id, fixture.source.id),
        (fixture.source.id, fixture.source.id),
    ] {
        let response = app
            .authenticated_client(&fixture.owner)
            .post_json(
                &format!("/api/v1/workspaces/{source}/pull-requests"),
                &json!({"targetWorkspaceId":target}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert!(
        peer.received_rpc_methods().await.is_empty(),
        "invalid PRs cannot commit files"
    );
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/pull-requests", fixture.source.id),
            &json!({"targetWorkspaceId":fixture.base_id}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("UNMERGED_PATHS"), "{body}");
    assert!(!peer
        .received_rpc_methods()
        .await
        .contains(&"modules.invoke:workspace.changes/push".into()));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_pr_refuses_a_replaced_incarnation_after_commit() {
    assert_pr_rejects_replacement(false, "commit").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vm_create_pr_refuses_a_replaced_incarnation_after_commit() {
    assert_pr_rejects_replacement(true, "commit").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vm_create_pr_rechecks_incarnation_after_push_before_recording_pr() {
    assert_pr_rejects_replacement(true, "push").await;
}

async fn assert_pr_rejects_replacement(vm: bool, phase: &str) {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &fixture.source,
        fixture.owner.id,
        "modules.invoke",
    )
    .await;
    let token = if vm {
        vm_pr_token(&app, &fixture, &peer)
    } else {
        fixture.owner.jwt.clone()
    };
    let route = format!(
        "/api/{}/workspaces/{}/pull-requests",
        if vm { "internal" } else { "v1" },
        fixture.source.id
    );
    let request = json!({"targetWorkspaceId":fixture.base_id});
    let replace = async {
        wait_for_operation(&peer, "commit").await;
        if phase == "push" {
            peer.release_one_rpc();
            wait_for_operation(&peer, "push").await;
        }
        // The admitted call holds no database lock
        // (`g5.runtime.guest-call-admission`), so the replacement lands while
        // the commit is still held; once the commit finishes, the next RPC
        // must reject the old incarnation rather than act on it.
        sqlx::query("/* pr-incarnation-race */ UPDATE workspace_vms SET epoch=epoch+1 WHERE workspace_id=$1")
            .bind(fixture.source.id)
            .execute(&app.db)
            .await
            .unwrap();
        peer.release_one_rpc();
    };
    let (response, ()) = tokio::join!(post_pr(&app, &token, &route, &request), replace);
    assert!(!response.status().is_success());
    let body: Value = response.json().await.unwrap();
    // The API maps the runtime's stale-incarnation refusal to this stable
    // public code; a transport timeout would be SERVICE_UNAVAILABLE instead.
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING", "{body}");
    assert_eq!(
        body["error"]["details"]["kind"], "pull_request_source_save_failed",
        "{body}"
    );
    assert_eq!(
        peer.received_rpc_methods()
            .await
            .contains(&"modules.invoke:workspace.changes/push".into()),
        phase == "push"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn create_pr_push_failure_retains_commits_and_can_retry() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_changes_failure(
        &app,
        &fixture.source,
        fixture.owner.id,
        "push",
    )
    .await;
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/pull-requests", fixture.source.id);
    let request = json!({"targetWorkspaceId":fixture.base_id});
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string()
            .contains("Could not push workspace changes"),
        "{body}"
    );
    let driven = peer.received_rpc_methods().await;
    assert!(driven.contains(&"modules.invoke:workspace.changes/commit".into()));
    assert!(driven.contains(&"modules.invoke:workspace.changes/push".into()));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        head(&app, fixture.source.repo_id, &fixture.source.name).await,
        fixture.source_head
    );
    // Replace the scripted peer with a working incarnation: the clean commit
    // still reaches push, so a previous partial save can be published.
    drop(peer);
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1")
        .bind(fixture.source.id)
        .execute(&app.db)
        .await
        .unwrap();
    let retry =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(retry
        .received_rpc_methods()
        .await
        .contains(&"modules.invoke:workspace.changes/push".into()));
    app.cleanup().await;
}

/// Commit `contents` at `file` (which may be nested) on `branch` in the mirror.
async fn commit_path(
    app: &TestApp,
    repo_id: Uuid,
    branch: &str,
    file: &str,
    contents: &str,
) -> String {
    let path = app.repo_path(repo_id).await;
    let repo = GitRepo::open(&path).unwrap();
    repo.checkout_branch_force(branch).unwrap();
    let target = path.join(file);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, contents).unwrap();
    repo.add_all().unwrap();
    repo.commit(
        file,
        &GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
    )
    .unwrap()
}

/// `g5.git.workflow-files` (#612): Create PR publishes a workflow-file change
/// through the organization's GitHub App only after the installation grants
/// Workflows write and G5 records it. Before that, every attempt is a
/// sanitized, actionable refusal that saves no PR and publishes nothing.
/// The remote models GitHub's rule from the minted token's permissions.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_pr_publishes_workflow_changes_only_under_a_recorded_app_grant() {
    let github = helpers::github_app::MockedGithubApp::start().await;
    let mut config = helpers::test_config();
    github.configure(&mut config);
    let app = TestApp::spawn_with_config(config).await;
    let f = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &f).await;
    let mirror = app.repo_path(f.source.repo_id).await;
    let remote = tempfile::tempdir().unwrap();
    git(
        remote.path(),
        &["clone", "--bare", mirror.to_str().unwrap(), "remote.git"],
    );
    let bare = remote.path().join("remote.git");
    github
        .bind(&app.db, f.org.id, f.owner.id, f.source.repo_id, &bare)
        .await;
    github.enforce_workflow_permission(&bare);
    git2::Repository::open(&mirror)
        .unwrap()
        .remote_set_url("origin", &github.git_url)
        .or_else(|_| {
            git2::Repository::open(&mirror)
                .unwrap()
                .remote("origin", &github.git_url)
                .map(|_| ())
        })
        .unwrap();
    // Publication captures the coordinated repository set, which names a docs repo.
    sqlx::query("UPDATE workspaces SET docs_repo_id=$3 WHERE id IN ($1,$2)")
        .bind(f.source.id)
        .bind(f.base_id)
        .bind(f.other_repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    // The target tracks the provider's main; the source is local-only, so
    // Create PR publishes it with a create-only push to `g5/feature`.
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,'main')")
        .bind(f.org.id).bind(f.base_id).bind(f.source.repo_id).execute(&app.db).await.unwrap();
    let workflow_head = commit_path(
        &app,
        f.source.repo_id,
        &f.source.name,
        ".github/workflows/ci.yml",
        "on: push\n",
    )
    .await;
    let published_ref = format!("refs/heads/g5/{}", f.source.name);
    let remote_ref = |reference: &str| {
        git2::Repository::open_bare(&bare)
            .unwrap()
            .refname_to_id(reference)
            .ok()
            .map(|oid| oid.to_string())
    };
    let pr_count = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
        )
        .bind(f.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap()
    };
    let contents_only = json!({"contents":"write"});
    let with_workflows = json!({"contents":"write","workflows":"write"});
    let client = app.authenticated_client(&f.owner);
    let route = format!("/api/v1/workspaces/{}/pull-requests", f.source.id);
    let request = json!({"targetWorkspaceId":f.base_id,"title":"Change CI"});

    // Without the grant: refused before any PR is saved, on every retry,
    // reusing the one cached contents-only token.
    for _attempt in 0..2 {
        let response = client.post_json(&route, &request).await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        let error = &body["error"];
        assert_eq!(
            error["details"]["code"], "PROVIDER_PERMISSION_REQUIRED",
            "{body}"
        );
        assert_eq!(error["details"]["permission"], "workflows");
        assert_eq!(error["details"]["credential"], "github_app");
        assert_eq!(error["details"]["path"], ".github/workflows/ci.yml");
        assert_eq!(error["details"]["kind"], "pull_request_source_save_failed");
        let message = error["message"].as_str().unwrap();
        for expected in [
            ".github/workflows/ci.yml",
            "Workflows",
            "Nothing was published",
            "Refresh from GitHub",
        ] {
            assert!(message.contains(expected), "{message}");
        }
        assert!(!message.contains("ghs_"), "{message}");
        assert_eq!(pr_count().await, 0, "a refused publication saves no PR");
        assert_eq!(remote_ref(&published_ref), None);
    }
    let writes = |minted: Vec<Value>| -> Vec<Value> {
        minted
            .into_iter()
            .filter(|permissions| permissions["contents"] == "write")
            .collect()
    };
    let minted = writes(github.minted_permissions().await);
    assert_eq!(minted, vec![contents_only.clone()], "{minted:?}");

    // The installation owner approves Workflows; G5 does not use it until
    // the binding records it.
    github.grant_on_github(json!({
        "metadata":"read","contents":"write","pull_requests":"write","workflows":"write"
    }));
    let response = client.post_json(&route, &request).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(pr_count().await, 0);
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{}/github-app/binding/refresh", f.org.id),
            &json!({"bindingVersion":1}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["binding"]["version"], 2, "{body}");
    assert_eq!(body["data"]["binding"]["permissions"]["workflows"], "write");

    // A competing remote branch is never overwritten, even with the grant.
    git(&bare, &["update-ref", &published_ref, &f.initial_head]);
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string().contains("PUBLISH_TARGET_REF_EXISTS"),
        "{body}"
    );
    assert_eq!(
        remote_ref(&published_ref).as_deref(),
        Some(f.initial_head.as_str())
    );
    assert_eq!(pr_count().await, 0);
    git(&bare, &["update-ref", "-d", &published_ref]);

    // The source moves before the successful retry: exactly the new head is
    // published, under a newly minted token that carries the grant.
    let next_head = commit_path(
        &app,
        f.source.repo_id,
        &f.source.name,
        "notes.txt",
        "next\n",
    )
    .await;
    assert_ne!(next_head, workflow_head);
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["alreadyExisted"], false, "{body}");
    assert_eq!(
        remote_ref(&published_ref).as_deref(),
        Some(next_head.as_str())
    );
    assert_eq!(pr_count().await, 1);
    let minted = writes(github.minted_permissions().await);
    assert_eq!(
        minted,
        vec![contents_only.clone(), with_workflows.clone()],
        "the recorded grant mints one new token and never reuses the contents-only one"
    );

    // Retrying reuses the saved PR and the published branch.
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["alreadyExisted"], true, "{body}");
    assert_eq!(pr_count().await, 1);
    assert_eq!(
        remote_ref(&published_ref).as_deref(),
        Some(next_head.as_str())
    );
    app.cleanup().await;
}

#[tokio::test]
async fn create_pr_does_not_omit_dirty_stopped_flush_snapshots() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    let flush_id = Uuid::new_v4();
    let receipts = json!([{
        "repo_id":fixture.source.repo_id,"branch_oid":fixture.source_head,
        "local_branch_oid":fixture.source_head,"local_head_oid":fixture.source_head,
        "head_ref":format!("refs/heads/{}",fixture.source.name),"sequencer":"none",
        "snapshot_oid":fixture.source_head,"dirty":true,"dirty_paths":["draft.txt"]
    }]);
    sqlx::query("INSERT INTO workspace_flushes(flush_id,org_id,workspace_id,origin,vm_id,epoch,reason,required_repo_ids,repos,complete_at,request_key) VALUES($1,$2,$3,'flush',$4,1,'shutdown',$5,$6,now(),'pr-test')")
        .bind(flush_id).bind(fixture.org.id).bind(fixture.source.id).bind(Uuid::new_v4().to_string())
        .bind(vec![fixture.source.repo_id]).bind(receipts).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,status,last_flush_id) VALUES($1,$2,'stopped',$3)")
        .bind(fixture.org.id).bind(fixture.source.id).bind(flush_id).execute(&app.db).await.unwrap();
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/pull-requests", fixture.source.id),
            &json!({"targetWorkspaceId":fixture.base_id}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("Start the workspace"), "{body}");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

async fn commit(app: &TestApp, repo_id: Uuid, branch: &str, file: &str) -> String {
    let path = app.repo_path(repo_id).await;
    let repo = GitRepo::open(&path).unwrap();
    repo.checkout_branch_force(branch).unwrap();
    std::fs::write(path.join(file), format!("{file}\n")).unwrap();
    repo.add_all().unwrap();
    repo.commit(
        file,
        &GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
    )
    .unwrap()
}

/// Merges a new commit on `base` into `branch` and returns the merge commit.
async fn merge_side_commit(app: &TestApp, repo_id: Uuid, branch: &str, base: &str) -> String {
    let path = app.repo_path(repo_id).await;
    let repo = git2::Repository::open(&path).unwrap();
    let signature = git2::Signature::now("Test", "test@g5.test").unwrap();
    let base = repo.find_commit(base.parse().unwrap()).unwrap();
    let mut tree = repo.treebuilder(Some(&base.tree().unwrap())).unwrap();
    tree.insert("side.txt", repo.blob(b"side\n").unwrap(), 0o100644)
        .unwrap();
    let side_tree = repo.find_tree(tree.write().unwrap()).unwrap();
    let side = repo
        .commit(None, &signature, &signature, "side", &side_tree, &[&base])
        .unwrap();
    let side = repo.find_commit(side).unwrap();
    let reference = format!("refs/heads/{branch}");
    let tip = repo
        .find_commit(repo.refname_to_id(&reference).unwrap())
        .unwrap();
    let merged = repo
        .merge_commits(&tip, &side, None)
        .unwrap()
        .write_tree_to(&repo)
        .unwrap();
    let merge = repo
        .commit(
            Some(&reference),
            &signature,
            &signature,
            "Merge side into feature",
            &repo.find_tree(merged).unwrap(),
            &[&tip, &side],
        )
        .unwrap()
        .to_string();
    GitRepo::open(&path)
        .unwrap()
        .checkout_branch_force(branch)
        .unwrap();
    merge
}

async fn head(app: &TestApp, repo_id: Uuid, branch: &str) -> String {
    let path = app.repo_path(repo_id).await;
    GitRepo::open(&path)
        .unwrap()
        .resolve_branch_head(branch)
        .unwrap()
}

async fn conflicting_fixture(app: &TestApp) -> Fixture {
    let mut fixture = Fixture::create(app, true).await;
    let path = app.repo_path(fixture.source.repo_id).await;
    let repo = GitRepo::open(&path).unwrap();
    for (branch, content) in [(&fixture.source.name[..], "child\n"), ("main", "base\n")] {
        repo.checkout_branch_force(branch).unwrap();
        std::fs::write(path.join("conflict.txt"), content).unwrap();
        repo.add_all().unwrap();
        repo.commit(
            "conflicting change",
            &GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
        )
        .unwrap();
    }
    fixture.source_head = head(app, fixture.source.repo_id, &fixture.source.name).await;
    fixture.base_head = head(app, fixture.source.repo_id, "main").await;
    fixture
}

/// g5.governance.pr.read.freshness: stored merge evidence names its conflicting
/// paths and is reported stale once the target moves, without recomputing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stored_pr_reads_report_conflicting_paths_and_stale_evidence() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let read = || async {
        let response = app
            .authenticated_client(&f.owner)
            .get(&format!(
                "/api/v1/workspaces/{}/pull-requests/info",
                f.source.id
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["source"], "g5");
        body["data"]["pullRequests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pr| pr["id"] == f.pull_request_id.to_string())
            .unwrap()["mergeEvidence"]
            .clone()
    };
    let repo = |evidence: &Value, id: Uuid| {
        evidence["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["repoId"] == id.to_string())
            .unwrap()
            .clone()
    };
    let fresh = read().await;
    assert_eq!(fresh["stale"], false, "{fresh}");
    assert_eq!(fresh["localMergeStatus"], "ready");
    assert_eq!(fresh["repos"].as_array().unwrap().len(), 2, "{fresh}");

    // Conflicting changes on both sides; Refresh recomputes the evidence.
    let path = app.repo_path(f.source.repo_id).await;
    let git_repo = GitRepo::open(&path).unwrap();
    for (branch, content) in [(&f.source.name[..], "child\n"), ("main", "base\n")] {
        git_repo.checkout_branch_force(branch).unwrap();
        std::fs::write(path.join("conflict.txt"), content).unwrap();
        git_repo.add_all().unwrap();
        git_repo
            .commit(
                "conflicting change",
                &GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
            )
            .unwrap();
    }
    let response = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/pull-requests/{}/refresh",
                f.base_id, f.pull_request_id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let conflicted = read().await;
    assert_eq!(conflicted["localMergeStatus"], "conflict", "{conflicted}");
    assert_eq!(conflicted["stale"], false, "{conflicted}");
    let member = repo(&conflicted, f.source.repo_id);
    assert_eq!(member["outcome"], "gitConflicts", "{member}");
    assert!(
        member["conflictPaths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().unwrap().ends_with("conflict.txt")),
        "{member}"
    );

    // The target moves; the read reports it without recomputing anything.
    let preflight_before: Value =
        sqlx::query_scalar("SELECT preflight FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    commit(&app, f.source.repo_id, "main", "later.txt").await;
    let stale = read().await;
    assert_eq!(stale["stale"], true, "{stale}");
    assert_eq!(repo(&stale, f.source.repo_id)["targetMoved"], true);
    assert_eq!(repo(&stale, f.other_repo_id)["targetMoved"], false);
    assert_eq!(stale["localMergeStatus"], "conflict");
    let preflight_after: Value =
        sqlx::query_scalar("SELECT preflight FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(preflight_before, preflight_after);
    app.cleanup().await;
}

fn git(path: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn get_latest_from_base_conflict_starts_an_agent_in_the_child() {
    let app = TestApp::spawn().await;
    let fixture = conflicting_fixture(&app).await;
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    let request = json!({"targetWorkspaceId": fixture.base_id});
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "stopped: {body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING", "{body}");
    let runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    // Confirm explicit settings through the same preview API as the modal.
    let preferences = json!({"schemaVersion":1,
        "provider":{"source":"explicit","value":"claude_code"},
        "permissionMode":{"source":"explicit","value":"bypassPermissions"}});
    let resolved = client
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/config/agent-launch/resolve",
                fixture.source.id
            ),
            &json!({"preferences":preferences}),
        )
        .await;
    let status = resolved.status();
    let resolved: Value = resolved.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "launch preview: {resolved}");
    let request = json!({"targetWorkspaceId":fixture.base_id, "agentLaunch":{
        "preferences":preferences, "confirmedResolutionId":resolved["data"]["resolved"]["resolutionId"],
    }});
    let (first, second) = tokio::join!(
        client.post_json(&route, &request),
        client.post_json(&route, &request)
    );
    let first_status = first.status();
    let second_status = second.status();
    let first: Value = first.json().await.unwrap();
    let second: Value = second.json().await.unwrap();
    assert_eq!(
        first_status,
        StatusCode::OK,
        "Get latest from Base: {first}"
    );
    assert_eq!(second_status, StatusCode::OK, "concurrent start: {second}");
    assert_eq!(first["data"]["resolution"], second["data"]["resolution"]);
    let resolution = &first["data"]["resolution"];
    assert!(resolution["sessionId"].is_string(), "{first}");
    let task_id: Uuid = resolution["taskId"].as_str().unwrap().parse().unwrap();
    let task_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(fixture.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(task_count, 1);
    let spec: Value = sqlx::query_scalar("SELECT spec FROM tasks WHERE id=$1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        spec["agent"]["agentLaunch"]["permissionMode"],
        preferences["permissionMode"]
    );
    fixture.assert_unpublished(&app).await;

    let mut db = app.scoped_db_for_org(fixture.org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dispatch.task_id, task_id);
    assert_eq!(dispatch.vm_id, runtime.vm_id());
    db.commit().await.unwrap();
    let disposition = app
        .state
        .task_service
        .invoke_agent_dispatch(&dispatch)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(fixture.org.id).await;
    app.state
        .task_service
        .apply(&mut db, dispatch.start_disposition(disposition))
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert!(runtime
        .received_rpc_methods()
        .await
        .contains(&"modules.invoke".into()));

    let prompt: String = sqlx::query_scalar("SELECT content FROM session_inputs WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let inputs: Value = serde_json::from_str(prompt.lines().nth(1).unwrap()).unwrap();
    assert!(prompt.contains("G5 has already fetched and delivered"));
    assert!(prompt.contains("do not fetch, refresh, create, or update the input refs"));
    let repositories = inputs["repositories"].as_array().unwrap();
    assert_eq!(
        repositories.len(),
        2,
        "the full repository vector must reach the agent"
    );
    // Exercise publication through the same HTTP receiver and incarnation
    // identity as the running child, including non-fast-forward receive policy.
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(
            fixture.org.id,
            fixture.source.id,
            fixture.owner.id,
            runtime.vm_id(),
            1,
            3600,
        )
        .unwrap();
    let auth_header = format!("http.extraHeader=Authorization: Bearer {token}");
    let tmp = tempfile::tempdir().unwrap();
    for input in repositories {
        let repo_id: Uuid = input["repoId"].as_str().unwrap().parse().unwrap();
        let origin = format!("{}/git/{repo_id}", app.base_url);
        let checkout = tmp.path().join(input["rootPath"].as_str().unwrap());
        git(
            tmp.path(),
            &[
                "-c",
                &auth_header,
                "clone",
                "--branch",
                &fixture.source.name,
                &origin,
                checkout.to_str().unwrap(),
            ],
        );
        git(
            &checkout,
            &[
                "config",
                "http.extraHeader",
                &format!("Authorization: Bearer {token}"),
            ],
        );
        git(&checkout, &["config", "user.email", "fixture@g5.test"]);
        git(&checkout, &["config", "user.name", "Fixture Agent"]);
        let specs = g5_contracts::workspace_git::provider_fetch_refspecs(fixture.source.id);
        let mut args = vec!["fetch", "origin"];
        args.extend(specs.iter().map(String::as_str));
        git(&checkout, &args);
        let remote_ref = input["remoteInputRef"].as_str().unwrap();
        assert_eq!(
            git(&checkout, &["rev-parse", remote_ref]),
            input["remoteTip"].as_str().unwrap()
        );
        assert_eq!(
            git(&checkout, &["rev-parse", "HEAD"]),
            input["localTip"].as_str().unwrap()
        );
        let rebase_args: Vec<&str> = input["rebaseArgs"]
            .as_array()
            .expect("the agent must receive an explicit rebase command")
            .iter()
            .map(|arg| arg.as_str().unwrap())
            .collect();
        assert_eq!(rebase_args[0], "rebase");
        assert!(rebase_args.contains(&"--no-rebase-merges"));
        let rebase = std::process::Command::new("git")
            .current_dir(&checkout)
            .args(&rebase_args)
            .output()
            .unwrap();
        if repo_id == fixture.source.repo_id {
            assert_eq!(
                rebase.status.code(),
                Some(1),
                "must exercise the content conflict"
            );
            assert_eq!(
                git(&checkout, &["diff", "--name-only", "--diff-filter=U"]),
                "conflict.txt"
            );
            std::fs::write(checkout.join("conflict.txt"), "child\nbase\n").unwrap();
            git(&checkout, &["add", "conflict.txt"]);
            git(
                &checkout,
                &["-c", "core.editor=true", "rebase", "--continue"],
            );
        } else {
            assert!(rebase.status.success());
        }
        let remote_tip = input["remoteTip"].as_str().unwrap();
        git(
            &checkout,
            &["merge-base", "--is-ancestor", remote_tip, "HEAD"],
        );
        assert!(git(
            &checkout,
            &["rev-list", "--merges", &format!("{remote_tip}..HEAD")]
        )
        .is_empty());
        if repo_id == fixture.source.repo_id {
            assert_eq!(
                git(
                    &checkout,
                    &["log", "--format=%s", &format!("{remote_tip}..HEAD")]
                ),
                "conflicting change\nfeature.txt"
            );
            assert_eq!(
                std::fs::read_to_string(checkout.join("feature.txt")).unwrap(),
                "feature.txt\n"
            );
            assert_eq!(
                std::fs::read_to_string(checkout.join("base.txt")).unwrap(),
                "base.txt\n"
            );
            assert_eq!(
                std::fs::read_to_string(checkout.join("conflict.txt")).unwrap(),
                "child\nbase\n"
            );
            assert!(
                !std::process::Command::new("git")
                    .current_dir(&checkout)
                    .args([
                        "merge-base",
                        "--is-ancestor",
                        input["localTip"].as_str().unwrap(),
                        "HEAD"
                    ])
                    .status()
                    .unwrap()
                    .success(),
                "rebase must replace the old child commits"
            );
        }
        let lease = format!(
            "--force-with-lease=refs/heads/{}:{}",
            fixture.source.name,
            input["localTip"].as_str().unwrap()
        );
        git(
            &checkout,
            &[
                "push",
                &lease,
                "origin",
                &format!("HEAD:refs/heads/{}", fixture.source.name),
            ],
        );
    }
    assert_eq!(
        head(&app, fixture.source.repo_id, "main").await,
        fixture.base_head
    );
    let preflight = client
        .post_json(
            &format!("{route}/preflight"),
            &json!({"targetWorkspaceId":fixture.base_id}),
        )
        .await;
    let status = preflight.status();
    let preflight: Value = preflight.json().await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "preflight after resolution: {preflight}"
    );
    assert_eq!(
        preflight["data"]["preflight"]["decision"],
        "fastForwardOrNoOp"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn get_latest_from_base_clean_update_rebases_child_commits() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, true).await;
    // Access authorization locks its subjects briefly. Start the concurrent
    // update after that capture: during final application the target is only
    // an optimistic proof input, with no target SHARE lock. NO KEY UPDATE
    // still permits the merge history's foreign-key check of its target ID.
    let barrier =
        g5_api::services::workspace_service::install_prepared_landing_barrier(fixture.source.id);
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    let request = json!({"targetWorkspaceId":fixture.base_id});
    let rebase = client.post_json(&route, &request);
    tokio::pin!(rebase);
    tokio::select! {
        response = &mut rebase => panic!("rebase finished before preparation: {}", response.status()),
        result = tokio::time::timeout(std::time::Duration::from_secs(10), barrier.wait_until_prepared()) => {
            result.expect("rebase must reach the prepared application boundary");
        }
    }
    let mut target_update = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR NO KEY UPDATE")
        .bind(fixture.base_id)
        .fetch_one(&mut *target_update)
        .await
        .unwrap();
    barrier.release();
    let response = tokio::time::timeout(std::time::Duration::from_secs(10), &mut rebase)
        .await
        .expect("source-only rebase must not wait for a target content update");
    target_update.rollback().await.unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["data"]["run"].is_object(),
        "clean replay needs no agent: {body}"
    );
    let path = app.repo_path(fixture.source.repo_id).await;
    let rebased = head(&app, fixture.source.repo_id, &fixture.source.name).await;
    assert_ne!(rebased, fixture.source_head);
    assert_eq!(
        git(&path, &["rev-parse", &format!("{rebased}^")]),
        fixture.base_head
    );
    assert_eq!(
        git(&path, &["show", &format!("{rebased}:feature.txt")]),
        "feature.txt"
    );
    assert_eq!(
        git(&path, &["show", &format!("{rebased}:base.txt")]),
        "base.txt"
    );
    assert!(git(
        &path,
        &[
            "rev-list",
            "--merges",
            &format!("{}..{rebased}", fixture.base_head)
        ]
    )
    .is_empty());
    assert_eq!(
        head(&app, fixture.source.repo_id, "main").await,
        fixture.base_head
    );
    app.cleanup().await;
}

// `g5.architecture.workspace_rebase`: a running VM executes even a clean
// replay and a no-op; G5 accepts the VM result and the target never moves.
#[tokio::test]
async fn running_workspace_rebases_in_its_vm_even_when_central_git_could() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, true).await;
    let runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    let request = json!({"targetWorkspaceId":fixture.base_id});
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["run"].is_null(), "no central run: {body}");
    let members = body["data"]["rebase"]["members"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(members.len(), 2, "{body}");
    let rebased = head(&app, fixture.source.repo_id, &fixture.source.name).await;
    assert_ne!(rebased, fixture.source_head);
    let path = app.repo_path(fixture.source.repo_id).await;
    assert_eq!(
        git(&path, &["rev-parse", &format!("{rebased}^")]),
        fixture.base_head
    );
    assert_eq!(
        head(&app, fixture.source.repo_id, "main").await,
        fixture.base_head
    );
    let source_member = members
        .iter()
        .find(|member| member["repoId"] == fixture.source.repo_id.to_string())
        .unwrap();
    assert_eq!(source_member["head"], rebased);
    assert_eq!(source_member["previousHead"], fixture.source_head);
    assert_eq!(source_member["published"], true);
    let other_member = members
        .iter()
        .find(|member| member["repoId"] == fixture.other_repo_id.to_string())
        .unwrap();
    assert_eq!(other_member["published"], false, "validated no-op: {body}");
    let invoked = |methods: &[String]| {
        methods
            .iter()
            .filter(|method| *method == "modules.invoke:workspace.changes/rebase")
            .count()
    };
    assert_eq!(invoked(&runtime.received_rpc_methods().await), 2);
    let runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_merge_runs WHERE source_workspace_id=$1 AND phase='source_rebased'",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        runs, 0,
        "the VM result is the evidence, not a central landing"
    );

    // Already rebased: the VM validates a no-op and nothing moves.
    let again = client.post_json(&route, &request).await;
    let status = again.status();
    let body: Value = again.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["rebase"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .all(|member| member["published"] == false));
    assert_eq!(invoked(&runtime.received_rpc_methods().await), 4);
    assert_eq!(
        head(&app, fixture.source.repo_id, &fixture.source.name).await,
        rebased
    );
    app.cleanup().await;
}

async fn stop_with_receipt(app: &TestApp, fixture: &Fixture, dirty: bool) {
    let flush_id = Uuid::new_v4();
    let receipt = |repo_id: Uuid, oid: &str, dirty: bool| {
        json!({
            "repo_id":repo_id,"branch_oid":oid,"local_branch_oid":oid,"local_head_oid":oid,
            "head_ref":format!("refs/heads/{}",fixture.source.name),"sequencer":"none",
            "snapshot_oid":oid,"dirty":dirty,
            "dirty_paths": if dirty { json!(["draft.txt"]) } else { json!([]) }
        })
    };
    let other_head = head(app, fixture.other_repo_id, &fixture.source.name).await;
    let receipts = json!([
        receipt(fixture.source.repo_id, &fixture.source_head, false),
        receipt(fixture.other_repo_id, &other_head, dirty),
    ]);
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1")
        .bind(fixture.source.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace_flushes(flush_id,org_id,workspace_id,origin,vm_id,epoch,reason,required_repo_ids,repos,complete_at,request_key) VALUES($1,$2,$3,'flush',$4,1,'shutdown',$5,$6,now(),$7)")
        .bind(flush_id).bind(fixture.org.id).bind(fixture.source.id).bind(Uuid::new_v4().to_string())
        .bind(vec![fixture.source.repo_id, fixture.other_repo_id]).bind(receipts).bind(format!("rebase-test-{flush_id}"))
        .execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,status,last_flush_id) VALUES($1,$2,'stopped',$3)")
        .bind(fixture.org.id).bind(fixture.source.id).bind(flush_id).execute(&app.db).await.unwrap();
}

// A stopped workspace rebases centrally only when restore would adopt the
// new heads; otherwise the owner starts it and the rebase runs there.
#[tokio::test]
async fn stopped_workspace_rebases_centrally_only_with_a_clean_published_receipt() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, true).await;
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    let request = json!({"targetWorkspaceId":fixture.base_id});
    stop_with_receipt(&app, &fixture, true).await;
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("start the workspace"), "{body}");
    assert_eq!(
        head(&app, fixture.source.repo_id, &fixture.source.name).await,
        fixture.source_head
    );

    stop_with_receipt(&app, &fixture, false).await;
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["run"].is_object(), "{body}");
    assert_ne!(
        head(&app, fixture.source.repo_id, &fixture.source.name).await,
        fixture.source_head
    );
    app.cleanup().await;
}

#[tokio::test]
async fn starting_workspace_refuses_rebase_without_central_fallback() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, true).await;
    stop_with_receipt(&app, &fixture, false).await;
    // A queued start captures boot heads under the same fence a central
    // landing takes; while it is pending the rebase waits for the VM.
    sqlx::query(
        "UPDATE workspace_vms SET start_intent = jsonb_build_object(
            'id', gen_random_uuid(), 'kind', 'resume', 'owner_user_id', $2::uuid,
            'lifecycle_generation', 0, 'status', 'pending', 'created_at', now(),
            'expires_at', now() + interval '10 minutes', 'attempts', 0)
         WHERE workspace_id=$1",
    )
    .bind(fixture.source.id)
    .bind(fixture.owner.id)
    .execute(&app.db)
    .await
    .unwrap();
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id),
            &json!({"targetWorkspaceId":fixture.base_id}),
        )
        .await;
    let code = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("starting or stopping"), "{body}");
    assert_eq!(
        head(&app, fixture.source.repo_id, &fixture.source.name).await,
        fixture.source_head
    );
    app.cleanup().await;
}

/// The brief a session queued: its input id, delivery and bound Start.
async fn brief_state(app: &TestApp, session: Uuid) -> (Uuid, String, Option<Uuid>) {
    sqlx::query_as::<_, (Uuid, String, Option<Uuid>)>(
        "SELECT id, delivery, start_consent_id FROM session_inputs WHERE session_id = $1",
    )
    .bind(session)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn start_intent(app: &TestApp, workspace_id: Uuid) -> Option<(Uuid, String, String)> {
    sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT (start_intent ->> 'id')::uuid, start_intent ->> 'kind', start_intent ->> 'status'
         FROM workspace_vms WHERE workspace_id = $1 AND start_intent IS NOT NULL",
    )
    .bind(workspace_id)
    .fetch_optional(&app.db)
    .await
    .unwrap()
}

async fn stop_via_route(app: &TestApp, fixture: &Fixture) {
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", fixture.source.id),
            &json!({ "operationId": Uuid::new_v4() }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Start and resolve with agent (`g5.git.stopped-rebase.resolve-with-agent`,
/// `g5.chat.start-and-send`): on a stopped child, one request records the
/// owner's compound Start and queues the resolution brief bound to it; a
/// second click while that Start is under way returns the same task and records
/// no second Start; a Stop that cancels the Start withdraws the brief; and a fresh
/// click afterwards admits a new resolution under a new Start, never the stale task.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_and_resolve_with_agent_binds_the_brief_to_the_owners_start() {
    let app = TestApp::spawn().await;
    let fixture = conflicting_fixture(&app).await;
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    let _runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    stop_via_route(&app, &fixture).await;

    // Without the compound flag a stopped child still refuses, as before.
    let plain = client
        .post_json(&route, &json!({"targetWorkspaceId": fixture.base_id}))
        .await;
    let status = plain.status();
    let body: Value = plain.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING", "{body}");
    assert!(start_intent(&app, fixture.source.id).await.is_none());

    let request = json!({"targetWorkspaceId": fixture.base_id, "startCompute": true});
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let first: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "start and resolve: {first}");
    let resolution = &first["data"]["resolution"];
    assert_eq!(
        first["data"]["startCompute"]["status"],
        json!("starting"),
        "{first}"
    );
    let intent_id: Uuid = first["data"]["startCompute"]["intentId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (recorded, kind, _) = start_intent(&app, fixture.source.id).await.unwrap();
    assert_eq!(recorded, intent_id);
    assert_eq!(kind, "compound_start", "a clearly labelled compound Start");
    let task_id: Uuid = resolution["taskId"].as_str().unwrap().parse().unwrap();
    let session_id: Uuid = resolution["sessionId"].as_str().unwrap().parse().unwrap();
    let (input_id, delivery, bound) = brief_state(&app, session_id).await;
    assert_eq!(delivery, "queued");
    assert_eq!(bound, Some(intent_id), "the brief is bound to its Start");
    assert!(
        resolution.get("inputId").is_none(),
        "the input id is not part of the wire form: {first}"
    );
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id=$1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(task_status, "queued");
    fixture.assert_unpublished(&app).await;

    // A second click while the Start is under way returns the active task
    // and records no second Start: the first click's consent stands.
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let again: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["resolution"]["taskId"], resolution["taskId"]);
    assert!(again["data"].get("startCompute").is_none(), "{again}");
    assert_eq!(
        start_intent(&app, fixture.source.id).await.unwrap().0,
        intent_id
    );
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(fixture.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 1);

    // Stop while starting cancels the consent: the brief is withdrawn.
    stop_via_route(&app, &fixture).await;
    let (same_input, delivery, bound) = brief_state(&app, session_id).await;
    assert_eq!(same_input, input_id);
    assert_eq!(delivery, "withdrawn");
    assert_eq!(bound, None);
    assert_eq!(
        start_intent(&app, fixture.source.id).await.unwrap().2,
        "cancelled"
    );

    // A fresh click admits a new resolution bound to a new Start; the task
    // left without its brief no longer counts as the active resolution.
    let response = client.post_json(&route, &request).await;
    let status = response.status();
    let third: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{third}");
    let fresh = &third["data"]["resolution"];
    assert_ne!(fresh["taskId"], resolution["taskId"], "{third}");
    let fresh_intent: Uuid = third["data"]["startCompute"]["intentId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(fresh_intent, intent_id);
    let fresh_session: Uuid = fresh["sessionId"].as_str().unwrap().parse().unwrap();
    let (_, delivery, bound) = brief_state(&app, fresh_session).await;
    assert_eq!(delivery, "queued");
    assert_eq!(bound, Some(fresh_intent));
    fixture.assert_unpublished(&app).await;
    app.cleanup().await;
}

/// A stopped child that kept unpublished work cannot resolve centrally: the
/// request is refused before any Start is recorded, with the same reason a
/// plain rebase gives (`g5.git.dirty-rebase`), and a wrong-owner request
/// records nothing either.
#[tokio::test]
async fn start_and_resolve_with_agent_refuses_before_starting_when_it_could_not_run() {
    let app = TestApp::spawn().await;
    let fixture = conflicting_fixture(&app).await;
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    let request = json!({"targetWorkspaceId": fixture.base_id, "startCompute": true});
    stop_with_receipt(&app, &fixture, true).await;
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(&route, &request)
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("start the workspace"), "{body}");
    assert!(start_intent(&app, fixture.source.id).await.is_none());

    stop_with_receipt(&app, &fixture, false).await;
    let other = TestUser::create_with_scopes(
        &app,
        fixture.org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    let response = app
        .authenticated_client(&other)
        .post_json(&route, &request)
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(start_intent(&app, fixture.source.id).await.is_none());
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(fixture.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 0);
    fixture.assert_unpublished(&app).await;
    app.cleanup().await;
}

#[tokio::test]
async fn get_latest_from_base_intermediate_replay_conflict_starts_an_agent() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, true).await;
    let path = app.repo_path(fixture.source.repo_id).await;
    let repo = GitRepo::open(&path).unwrap();
    repo.checkout_branch_force(&fixture.source.name).unwrap();
    // The final child tree merges cleanly, but replaying its transient edit
    // conflicts with Base before the following commit removes that edit.
    let author = GitAuthor::from_profile(Some("Test"), Some("test@g5.test"));
    std::fs::write(path.join("base.txt"), "transient child edit\n").unwrap();
    repo.add_all().unwrap();
    repo.commit("transient edit", &author).unwrap();
    std::fs::remove_file(path.join("base.txt")).unwrap();
    repo.add_all().unwrap();
    let captured_child = repo.commit("remove transient edit", &author).unwrap();
    let _runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    let preflight: Value = client
        .post_json(
            &format!("{route}/preflight"),
            &json!({"targetWorkspaceId":fixture.base_id}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        preflight["data"]["preflight"]["decision"], "cleanMergeNoGitConflicts",
        "{preflight}"
    );
    let response = client
        .post_json(&route, &json!({"targetWorkspaceId":fixture.base_id}))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "replay conflicts must offer agent resolution: {body}"
    );
    assert!(body["data"]["resolution"]["taskId"].is_string(), "{body}");
    assert_eq!(
        head(&app, fixture.source.repo_id, &fixture.source.name).await,
        captured_child
    );
    assert_eq!(
        head(&app, fixture.source.repo_id, "main").await,
        fixture.base_head
    );
    app.cleanup().await;
}

#[tokio::test]
async fn base_conflict_enqueue_failure_rolls_back_and_retry_uses_fresh_inputs() {
    let app = TestApp::spawn().await;
    let fixture = conflicting_fixture(&app).await;
    let _runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    let client = app.authenticated_client(&fixture.owner);
    let route = format!("/api/v1/workspaces/{}/rebase-merge", fixture.source.id);
    // Defer the failure until COMMIT: even a fully prepared session/task must
    // roll back, leaving the next click able to enqueue against fresh heads.
    sqlx::raw_sql("CREATE FUNCTION fail_resolution_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected resolution commit failure'; END $$; CREATE CONSTRAINT TRIGGER fail_resolution_commit AFTER INSERT ON session_inputs DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fail_resolution_commit();")
        .execute(&app.db).await.unwrap();
    let response = client
        .post_json(&route, &json!({"targetWorkspaceId":fixture.base_id}))
        .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id=$1")
        .bind(fixture.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    fixture.assert_unpublished(&app).await;
    sqlx::raw_sql("DROP TRIGGER fail_resolution_commit ON session_inputs; DROP FUNCTION fail_resolution_commit();").execute(&app.db).await.unwrap();
    let new_base = commit(&app, fixture.source.repo_id, "main", "new-base.txt").await;
    let response = client
        .post_json(&route, &json!({"targetWorkspaceId":fixture.base_id}))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "retry: {body}");
    let task_id: Uuid = body["data"]["resolution"]["taskId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let prompt: String = sqlx::query_scalar("SELECT content FROM session_inputs WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(prompt.contains(&new_base));
    let newer_base = commit(&app, fixture.source.repo_id, "main", "even-newer-base.txt").await;
    let retry = client
        .post_json(&route, &json!({"targetWorkspaceId":fixture.base_id}))
        .await;
    let retry: Value = retry.json().await.unwrap();
    assert_eq!(retry["data"]["resolution"]["taskId"], task_id.to_string());
    let same_prompt: String =
        sqlx::query_scalar("SELECT content FROM session_inputs WHERE task_id=$1")
            .bind(task_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        same_prompt, prompt,
        "retry must not replace an admitted task's captured inputs"
    );
    sqlx::query("UPDATE tasks SET status='failed' WHERE id=$1")
        .bind(task_id)
        .execute(&app.db)
        .await
        .unwrap();
    let retry = client
        .post_json(&route, &json!({"targetWorkspaceId":fixture.base_id}))
        .await;
    let status = retry.status();
    let retry: Value = retry.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "retry after failure: {retry}");
    let new_task: Uuid = retry["data"]["resolution"]["taskId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(new_task, task_id);
    let new_prompt: String =
        sqlx::query_scalar("SELECT content FROM session_inputs WHERE task_id=$1")
            .bind(new_task)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(new_prompt.contains(&newer_base));
    app.cleanup().await;
}

#[tokio::test]
async fn base_conflict_revalidates_captured_heads_before_starting_an_agent() {
    use g5_api::services::workspace_merge_service::{
        PreparedSourceWorkspaceMerge, StartWorkspaceMergeRequest,
    };
    let app = TestApp::spawn().await;
    let fixture = conflicting_fixture(&app).await;
    let _runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    let db = app.scoped_db_for_org(fixture.org.id).await;
    let prepared = app
        .state
        .workspace_merge_service
        .prepare_source_workspace_merge(
            db,
            fixture.source.id,
            fixture.owner.id,
            &StartWorkspaceMergeRequest {
                target_workspace_id: fixture.base_id,
            },
        )
        .await
        .unwrap();
    let PreparedSourceWorkspaceMerge::WorkflowRequired(prepared) = prepared else {
        panic!("expected conflict");
    };
    commit(&app, fixture.source.repo_id, "main", "base-moved.txt").await;
    let mut db = app.scoped_db_for_org(fixture.org.id).await;
    let result = app
        .state
        .workspace_merge_service
        .enqueue_source_resolution(&app.state, &mut db, *prepared, fixture.owner.id, None)
        .await;
    assert!(result.is_err(), "changed heads must fail admission");
    db.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(fixture.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

impl Fixture {
    async fn create(app: &TestApp, diverged: bool) -> Self {
        Self::create_with_kind(app, diverged, true).await
    }

    async fn create_with_kind(app: &TestApp, diverged: bool, is_base: bool) -> Self {
        Self::create_with_history(app, diverged, is_base, false).await
    }

    /// `source_merge` merges a side commit into the source after its own
    /// commit, as when an agent merges the base into its branch (#582).
    async fn create_with_history(
        app: &TestApp,
        diverged: bool,
        is_base: bool,
        source_merge: bool,
    ) -> Self {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        let source = TestWorkspace::create_named(app, &org, "feature").await;
        let other = TestWorkspace::create(app, &org).await;
        helpers::seed_branch_ref(app, other.repo_id, &source.name, &other.head_commit).await;
        sqlx::query("UPDATE workspace_repos SET root_path='code' WHERE workspace_id=$1")
            .bind(source.id)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,'other','code')")
            .bind(org.id).bind(source.id).bind(other.repo_id).execute(&app.db).await.unwrap();
        let base_id = Uuid::new_v4();
        sqlx::query("INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) SELECT $2,org_id,'main',$3,owner_user_id,created_by FROM workspaces WHERE id=$1")
            .bind(source.id).bind(base_id).bind(is_base).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO workspace_settings(org_id,workspace_id) VALUES($1,$2)")
            .bind(org.id)
            .bind(base_id)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT org_id,$2,repo_id,root_path,role FROM workspace_repos WHERE workspace_id=$1")
            .bind(source.id).bind(base_id).execute(&app.db).await.unwrap();
        sqlx::query("UPDATE workspaces SET cloned_from_id=$2 WHERE id=$1")
            .bind(source.id)
            .bind(base_id)
            .execute(&app.db)
            .await
            .unwrap();
        let initial_head = head(app, source.repo_id, "main").await;
        let mut source_head = commit(app, source.repo_id, &source.name, "feature.txt").await;
        if source_merge {
            source_head = merge_side_commit(app, source.repo_id, &source.name, &initial_head).await;
        }
        let base_head = if diverged {
            commit(app, source.repo_id, "main", "base.txt").await
        } else {
            initial_head.clone()
        };
        let response = app
            .authenticated_client(&owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/pull-requests", source.id),
                &json!({"targetWorkspaceId": base_id}),
            )
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "create PR: {body}");
        assert_eq!(
            body["data"]["pullRequest"]["localMergeStatus"], "ready",
            "{body}"
        );
        let pull_request_id = body["data"]["pullRequest"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        Self {
            owner,
            org,
            source,
            base_id,
            other_repo_id: other.repo_id,
            initial_head,
            source_head,
            base_head,
            pull_request_id,
        }
    }

    async fn ship(&self, app: &TestApp) -> (StatusCode, Value) {
        let response = app
            .authenticated_client(&self.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{}/merge",
                    self.base_id, self.pull_request_id
                ),
                &json!({}),
            )
            .await;
        (response.status(), response.json().await.unwrap())
    }

    async fn assert_unpublished(&self, app: &TestApp) {
        assert_eq!(head(app, self.source.repo_id, "main").await, self.base_head);
        assert_eq!(
            head(app, self.source.repo_id, &self.source.name).await,
            self.source_head
        );
        let status: String =
            sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id=$1")
                .bind(self.pull_request_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(status, "open");
        let runs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM workspace_merge_runs WHERE source_workspace_id=$1",
        )
        .bind(self.source.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(
            runs, 0,
            "failed landing must not leave a completed or blocking merge run"
        );
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_pull_request_events WHERE pull_request_id=$1 AND event_type='merged'")
            .bind(self.pull_request_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(events, 0);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM workspace_advance_consumer_receipts WHERE org_id=$1",
        )
        .bind(self.org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(receipts, 0, "failed landing must roll back its receipts");
    }

    async fn assert_shipped(&self, app: &TestApp, response: &Value) {
        let landed_head = head(app, self.source.repo_id, "main").await;
        let path = app.repo_path(self.source.repo_id).await;
        let repo = git2::Repository::open(path).unwrap();
        for ancestor in [&self.source_head, &self.base_head] {
            assert!(
                landed_head == *ancestor
                    || repo
                        .graph_descendant_of(
                            landed_head.parse().unwrap(),
                            ancestor.parse().unwrap()
                        )
                        .unwrap()
            );
        }
        assert_eq!(
            head(app, self.source.repo_id, &self.source.name).await,
            self.source_head
        );
        assert_eq!(
            head(app, self.other_repo_id, "main").await,
            head(app, self.other_repo_id, &self.source.name).await
        );
        let (status, target_heads): (String, Value) =
            sqlx::query_as("SELECT status,target_heads FROM workspace_pull_requests WHERE id=$1")
                .bind(self.pull_request_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(status, "merged", "{response}");
        assert_eq!(target_heads.as_array().unwrap().len(), 2);
        assert!(target_heads
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["repoId"] == self.source.repo_id.to_string()
                && member["headCommit"] == landed_head));
        let (status, advance_id): (String, Uuid) = sqlx::query_as("SELECT status,completion_advance_id FROM workspace_merge_runs WHERE source_workspace_id=$1 AND target_workspace_id=$2 AND advances_target")
            .bind(self.source.id).bind(self.base_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(status, "completed");
        let receipt: Value = sqlx::query_scalar("SELECT result FROM workspace_advance_consumer_receipts WHERE operation_id=$1 AND consumer_kind='workspace_pull_request_merge'")
            .bind(advance_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(receipt, json!(true));
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_pull_request_events WHERE pull_request_id=$1 AND event_type='merged'")
            .bind(self.pull_request_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(events, 1);
    }
}

#[tokio::test]
async fn local_ship_to_base_fast_forwards_and_retry_cannot_duplicate_completion() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(status, StatusCode::OK, "ship: {body}");
    fixture.assert_shipped(&app, &body).await;
    assert_eq!(
        head(&app, fixture.source.repo_id, "main").await,
        fixture.source_head
    );
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(status, StatusCode::CONFLICT, "duplicate ship: {body}");
    fixture.assert_shipped(&app, &body).await;
    app.cleanup().await;
}

#[tokio::test]
async fn local_ship_to_base_clean_merge_preserves_both_histories() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, true).await;
    assert_ne!(fixture.base_head, fixture.initial_head);
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(status, StatusCode::OK, "ship: {body}");
    fixture.assert_shipped(&app, &body).await;
    app.cleanup().await;
}

#[tokio::test]
async fn local_ship_to_base_settlement_failure_rolls_back_and_can_retry() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    // Fail after merge validation and the PR status update, at the actual
    // event insertion boundary. The app owns a disposable test database.
    sqlx::query("ALTER TABLE workspace_pull_request_events ADD CONSTRAINT test_refuse_merge_event CHECK(event_type <> 'merged') NOT VALID")
        .execute(&app.db).await.unwrap();
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "injected failure: {body}"
    );
    fixture.assert_unpublished(&app).await;
    sqlx::query(
        "ALTER TABLE workspace_pull_request_events DROP CONSTRAINT test_refuse_merge_event",
    )
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(status, StatusCode::OK, "retry: {body}");
    fixture.assert_shipped(&app, &body).await;
    app.cleanup().await;
}

#[tokio::test]
async fn local_ship_to_base_refuses_missing_or_mismatched_merge_completion() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    let service = &app.state.coordinated_repo_publication_service;
    let merges = &app.state.workspace_merge_service;
    for mutation in [
        "DELETE FROM workspace_merge_runs WHERE id=$1",
        "UPDATE workspace_merge_runs SET status='cancelled' WHERE id=$1",
        "UPDATE workspace_merge_runs SET advances_target=false WHERE id=$1",
        "UPDATE workspace_merge_runs SET completion_advance_id=NULL WHERE id=$1",
        "UPDATE workspace_merge_runs SET source_workspace_id=target_workspace_id WHERE id=$1",
        "UPDATE workspace_merge_runs SET target_workspace_id=source_workspace_id WHERE id=$1",
    ] {
        let db = app
            .scoped_db_for_user_org(fixture.owner.id, fixture.org.id)
            .await;
        let outcome = service
            .merge_workspace_pull_request(
                db,
                merges,
                fixture.base_id,
                fixture.pull_request_id,
                fixture.owner.id,
                None,
            )
            .await
            .unwrap();
        let MergeWorkspacePullRequestOutcome::PreparedLocal(prepared) = outcome else {
            panic!("expected local merge")
        };
        let mut db = app
            .scoped_db_for_user_org(fixture.owner.id, fixture.org.id)
            .await;
        let result = service
            .apply_prepared_workspace_pull_request_merge(&mut db, merges, *prepared)
            .await
            .unwrap();
        sqlx::query(mutation)
            .bind(result.run.id)
            .execute(db.conn())
            .await
            .unwrap();
        let result = service
            .land_workspace_pull_request_merge(
                merges,
                result.authority_advance.unwrap(),
                db,
                &app.state.event_bus,
                &app.repo_storage,
            )
            .await;
        assert!(result.is_err(), "must refuse {mutation}");
        fixture.assert_unpublished(&app).await;
    }
    app.cleanup().await;
}

// Provider transport is redirected to local repositories through a
// process-global, test-helpers-only map. Run each provider scenario in its own
// test subprocess so redirects never leak between tests. Only transport is
// redirected; production URL identity and credential selection are unchanged.
fn isolated_provider_test(name: &str) -> bool {
    if std::env::var("G5_PROVIDER_TEST_CASE").as_deref() == Ok(name) {
        let path = std::env::var("G5_PROVIDER_TEST_CONFIG").unwrap();
        // SAFETY: this subprocess runs exactly one test. No Git operations or
        // service tasks exist yet; the search path never changes afterward.
        unsafe {
            git2::opts::set_search_path(git2::ConfigLevel::Global, path).unwrap();
        }
        return false;
    }
    let config = tempfile::tempdir().unwrap();
    std::fs::write(config.path().join(".gitconfig"), "").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(helpers::exact_test_name(module_path!(), name))
        .args(["--nocapture", "--test-threads=1"])
        .env("G5_PROVIDER_TEST_CASE", name)
        .env("G5_PROVIDER_TEST_CONFIG", config.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A filter that matches nothing also exits 0: require the one test to
    // have run, or the scenario silently never executes.
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{}\n{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

#[tokio::test]
async fn github_app_pr_creation_uses_the_actor_and_reuses_existing_prs_without_a_grant() {
    assert_github_pr_actor_and_recovery(false).await;
}

#[tokio::test]
async fn github_app_pr_reports_provider_commit_failure_and_retry_recovers_existing_remote() {
    assert_github_pr_actor_and_recovery(true).await;
}

async fn assert_github_pr_actor_and_recovery(fail_provider_commit: bool) {
    let name = if fail_provider_commit {
        "github_app_pr_reports_provider_commit_failure_and_retry_recovers_existing_remote"
    } else {
        "github_app_pr_creation_uses_the_actor_and_reuses_existing_prs_without_a_grant"
    };
    if isolated_provider_test(name) {
        return;
    }
    use wiremock::matchers::{body_json, body_partial_json, header, method, path};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create_with_app(false, false, true).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    let identity = Uuid::new_v4();
    let binding = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO github_app_identities(id,org_id,kind) VALUES($1,$2,'platform_managed')",
    )
    .bind(identity)
    .bind(f.org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO github_app_installation_bindings(id,org_id,app_identity_id,version,disposition,installation_id,account_id,account_login,permissions,provenance,created_by) VALUES($1,$2,$3,1,'active',555,44,'acme',$4,'coverage_change',$5)")
        .bind(binding).bind(f.org.id).bind(identity)
        .bind(json!({"metadata":"read","contents":"write","pull_requests":"write"}))
        .bind(f.owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO github_app_installation_binding_repositories(org_id,binding_id,github_repo_id,full_name) VALUES($1,$2,1002,$3)")
        .bind(f.org.id).bind(binding).bind("acme/api").execute(&app.db).await.unwrap();
    for (permissions, token) in [
        (
            json!({"pull_requests":"write","contents":"read"}),
            "ghs_pr_lookup",
        ),
        (json!({"contents":"write"}), "ghs_git_push"),
    ] {
        Mock::given(method("POST")).and(path("/app/installations/555/access_tokens"))
            .and(body_json(json!({"repository_ids":[1002],"permissions":permissions})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":token,"expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
            .expect(1).mount(&provider.github).await;
    }
    Mock::given(method("GET"))
        .and(path("/repos/acme/api"))
        .and(header("authorization", "Bearer ghu_pr_author"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":1002})))
        .expect(1)
        .mount(&provider.github)
        .await;
    let lookup = Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls"))
        .and(header("authorization", "Bearer ghs_pr_lookup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(2)
        .mount_as_scoped(&provider.github)
        .await;
    // The fixture's recorded #21 is not this branch's open PR, but GitHub still
    // reports it open, so it was not merged or closed outside G5 (#387).
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/21"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":21,
            "html_url":"https://github.com/acme/api/pull/21","state":"open","merged":false,
            "head":{"sha":f.base_head},"base":{"sha":f.base_head},"user":{"id":1,"login":"other"}})))
        .mount(&provider.github)
        .await;
    let pull = json!({"number":22,"html_url":"https://github.com/acme/api/pull/22",
        "state":"open","merged":false,"head":{"sha":f.source_head},"base":{"sha":f.base_head},
        "user":{"id":123,"login":"author"}});
    Mock::given(method("POST"))
        .and(path("/repos/acme/api/pulls"))
        .and(header("authorization", "Bearer ghu_pr_author"))
        .and(body_partial_json(json!({"title":"Publish feature"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(pull.clone()))
        .expect(1)
        .mount(&provider.github)
        .await;
    // Once recorded, #22 stays open on GitHub.
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/22"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull.clone()))
        .mount(&provider.github)
        .await;
    let mut db = app.scoped_db_for_org(f.org.id).await;
    app.state
        .workspace_pull_request_provider_lifecycle_service
        .create_or_update_provider_pull_requests(&mut db, f.pull_request_id, f.owner.id, None)
        .await
        .unwrap();
    db.commit().await.unwrap();
    let (status, error): (String, Option<String>) = sqlx::query_as(
        "SELECT provider_status,last_error FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(f.pull_request_id).bind(f.source.repo_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(status, "action_required", "{error:?}");
    assert!(error.unwrap().contains("github_user_connection_required"));
    assert!(!provider
        .github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|request| request.method == "POST" && request.url.path().ends_with("/pulls")));
    let encrypted = app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token("ghu_pr_author")
        .unwrap();
    sqlx::query("INSERT INTO github_app_user_grants(user_id,github_user_id,github_login,access_token_enc) VALUES($1,123,'author',$2)")
        .bind(f.owner.id).bind(encrypted).execute(&app.db).await.unwrap();
    // A separately supplied/stale display name must not redirect the user's
    // broad credential to a repository outside the checked Git URL/binding.
    sqlx::query("UPDATE repos SET provider_full_name='unselected/public' WHERE id=$1")
        .bind(f.source.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    // The full publication flow uses the mirror's configured origin. The
    // original provider-only fixture did not need a named remote.
    let mirror = app.repo_path(f.source.repo_id).await;
    git2::Repository::open(&mirror)
        .unwrap()
        .remote("origin", "https://github.com/acme/api.git")
        .unwrap();
    let peer = helpers::TestWorkspaceRuntimePeer::attach(app, &f.source, f.owner.id).await;
    let token = vm_pr_token(app, f, &peer);
    if fail_provider_commit {
        // Fail only the provider transaction COMMIT, after GitHub accepted the
        // new PR; the earlier G5 record/publication must remain recoverable.
        sqlx::raw_sql("CREATE FUNCTION fail_provider_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected secret persistence failure'; END $$; CREATE CONSTRAINT TRIGGER fail_provider_commit AFTER INSERT OR UPDATE ON workspace_pull_request_provider_repos DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fail_provider_commit();")
            .execute(&app.db).await.unwrap();
    }
    let response = post_pr(
        app,
        &token,
        &format!("/api/internal/workspaces/{}/pull-requests", f.source.id),
        &json!({"title":"Publish feature"}),
    )
    .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    if fail_provider_commit {
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["details"]["providerSyncPhase"], "commit");
        assert_eq!(body["error"]["details"]["providerSyncOutcome"], "unknown");
        assert_eq!(
            body["error"]["details"]["pullRequestId"],
            f.pull_request_id.to_string()
        );
        assert!(
            !body.to_string().contains("injected secret"),
            "internal failure details must stay on the server"
        );
        let stored_status: String = sqlx::query_scalar("SELECT provider_status FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND repo_id=$2")
            .bind(f.pull_request_id).bind(f.source.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(
            stored_status, "action_required",
            "failed commit keeps the earlier record, not a false open result"
        );
        sqlx::query("DROP TRIGGER fail_provider_commit ON workspace_pull_request_provider_repos")
            .execute(&app.db)
            .await
            .unwrap();
    } else {
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["data"]["pullRequest"]["id"],
            f.pull_request_id.to_string()
        );
        assert_eq!(body["data"]["alreadyExisted"], true);
        let (status, number, error): (String, Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT provider_status,provider_pr_number,last_error FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(f.pull_request_id).bind(f.source.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(status, "open", "{error:?}");
        assert_eq!(number, Some(22));
    }
    drop(lookup);
    sqlx::query("DELETE FROM github_app_user_grants WHERE user_id=$1")
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls"))
        .and(header("authorization", "Bearer ghs_pr_lookup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([pull.clone()])))
        .expect(3)
        .mount(&provider.github)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/repos/acme/api/pulls/22"))
        .and(header("authorization", "Bearer ghs_pr_lookup"))
        .and(body_json(json!({"title":"Renamed feature"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull.clone()))
        .expect(1)
        .mount(&provider.github)
        .await;
    let response = post_pr(
        app,
        &f.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
        &json!({"title":"Renamed feature"}),
    )
    .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["id"],
        f.pull_request_id.to_string()
    );
    assert_eq!(body["data"]["alreadyExisted"], true);
    let (status, number, error): (String, Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT provider_status,provider_pr_number,last_error FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(f.pull_request_id).bind(f.source.repo_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(
        (status.as_str(), number),
        ("open", Some(22)),
        "retry adopts the existing remote PR: {error:?}"
    );
    assert_eq!(body["data"]["pullRequest"]["title"], "Renamed feature");
    // Omitting title must not PATCH or overwrite the local title.
    let route = format!("/api/v1/workspaces/{}/pull-requests", f.source.id);
    let response = post_pr(app, &f.owner.jwt, &route, &json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let omitted: Value = response.json().await.unwrap();
    assert_eq!(omitted["data"]["pullRequest"]["title"], "Renamed feature");
    Mock::given(method("PATCH"))
        .and(path("/repos/acme/api/pulls/22"))
        .and(body_json(json!({"title":"Provider refuses"})))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({"message":"Rejected title"})))
        .expect(1)
        .mount(&provider.github)
        .await;
    let response = post_pr(
        app,
        &f.owner.jwt,
        &route,
        &json!({"title":"Provider refuses"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let failed: Value = response.json().await.unwrap();
    assert_eq!(failed["data"]["pullRequest"]["title"], "Provider refuses");
    assert_eq!(
        failed["data"]["pullRequest"]["providerRepos"][0]["providerStatus"], "failed",
        "{failed}"
    );
    let stored: String =
        sqlx::query_scalar("SELECT title FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored, "Provider refuses");
    provider.github.verify().await;
    provider.app.cleanup().await;
}

struct ProviderFixture {
    app: TestApp,
    fixture: Fixture,
    github: wiremock::MockServer,
    remote: tempfile::TempDir,
    accepted_head: String,
}

impl ProviderFixture {
    async fn create(is_base: bool, fail_first_import: bool) -> Self {
        Self::create_with_app(is_base, fail_first_import, false).await
    }

    async fn create_with_app(is_base: bool, fail_first_import: bool, github_app: bool) -> Self {
        Self::create_with_history(is_base, fail_first_import, github_app, false).await
    }

    async fn create_with_history(
        is_base: bool,
        fail_first_import: bool,
        github_app: bool,
        source_merge: bool,
    ) -> Self {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        use wiremock::matchers::{body_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let github = MockServer::start().await;
        let mut config = helpers::test_config();
        config.auth.scm_github_api_base_url = github.uri();
        if github_app {
            let mut app_config = helpers::github_app::github_app_config(&github);
            app_config.web_base_url = "https://github.com".into();
            config.github_app = Some(app_config);
        }
        let app = TestApp::spawn_with_config(config).await;
        let fixture = Fixture::create_with_history(&app, false, is_base, source_merge).await;
        let mirror = app.repo_path(fixture.source.repo_id).await;
        let remote = tempfile::tempdir().unwrap();
        git(
            remote.path(),
            &["clone", "--bare", mirror.to_str().unwrap(), "remote.git"],
        );
        let remote_path = remote.path().join("remote.git");
        let repo = git2::Repository::open_bare(&remote_path).unwrap();
        let source = repo
            .find_commit(fixture.source_head.parse().unwrap())
            .unwrap();
        let parent = repo
            .find_commit(fixture.base_head.parse().unwrap())
            .unwrap();
        let committer = git2::Signature::now("Provider", "provider@example.test").unwrap();
        let accepted_head = repo
            .commit(
                None,
                &source.author(),
                &committer,
                "provider rebase",
                &source.tree().unwrap(),
                &[&parent],
            )
            .unwrap()
            .to_string();
        assert_ne!(accepted_head, fixture.source_head);
        repo.reference("refs/heads/upstream", parent.id(), true, "fixture")
            .unwrap();
        drop(parent);
        drop(source);
        drop(repo);
        let git_url = if github_app {
            "https://github.com/acme/api.git".to_string()
        } else {
            format!("https://github.com/g5-test-{}/api.git", fixture.org.id)
        };
        g5_scm::native::redirect_transport_for_tests(&git_url, &remote_path);
        sqlx::query("UPDATE repos SET git_url=$2,provider='github',provider_full_name='acme/api' WHERE id=$1")
            .bind(fixture.source.repo_id).bind(&git_url).execute(&app.db).await.unwrap();
        for (workspace, branch) in [
            (fixture.source.id, "feature"),
            (fixture.base_id, "upstream"),
        ] {
            sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,$4)")
                .bind(fixture.org.id).bind(workspace).bind(fixture.source.repo_id).bind(branch).execute(&app.db).await.unwrap();
        }
        GitRepo::open(&mirror)
            .unwrap()
            .write_baseline_ref("main", &fixture.base_head)
            .unwrap();
        let encrypted = app
            .state
            .scm_token_manager
            .as_ref()
            .unwrap()
            .encrypt_token("provider-test-token")
            .unwrap();
        sqlx::query("INSERT INTO user_scm_connections(id,user_id,provider,provider_user_id,provider_username,access_token_enc,scopes) VALUES($1,$2,'github','test','test',$3,ARRAY['repo'])")
            .bind(Uuid::new_v4()).bind(fixture.owner.id).bind(encrypted).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO workspace_pull_request_provider_repos(id,org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_pr_url,provider_status,source_head_sha,target_head_sha) VALUES($1,$2,$3,$4,'github','acme/api','feature','upstream',21,'https://github.com/acme/api/pull/21','open',$5,$6)")
            .bind(Uuid::new_v4()).bind(fixture.org.id).bind(fixture.pull_request_id).bind(fixture.source.repo_id)
            .bind(&fixture.source_head).bind(&fixture.base_head).execute(&app.db).await.unwrap();
        sqlx::query("UPDATE workspace_pull_requests SET provider_status='open' WHERE id=$1")
            .bind(fixture.pull_request_id)
            .execute(&app.db)
            .await
            .unwrap();
        let accepted = Arc::new(AtomicBool::new(false));
        let state = accepted.clone();
        let source_head = fixture.source_head.clone();
        let initial = fixture.base_head.clone();
        let result_head = accepted_head.clone();
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/pulls/21"))
            .respond_with(move |_: &wiremock::Request| {
                let merged = state.load(Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(json!({
                    "number":21,"html_url":"https://github.com/acme/api/pull/21",
                    "state":if merged {"closed"} else {"open"},"merged":merged,
                    "mergeable":if merged {Value::Null} else {json!(false)},
                    "head":{"sha":source_head},"base":{"ref":"upstream","sha":initial},
                    "merge_commit_sha":if merged {Some(&result_head)} else {None}
                }))
            })
            .mount(&github)
            .await;
        let result_head = accepted_head.clone();
        let initial = fixture.base_head.clone();
        Mock::given(method("PUT"))
            .and(path("/repos/acme/api/pulls/21/merge"))
            .and(body_json(
                json!({"sha":fixture.source_head,"merge_method":"rebase"}),
            ))
            .respond_with(move |_: &wiremock::Request| {
                assert_eq!(
                    GitRepo::open(&mirror)
                        .unwrap()
                        .resolve_branch_head("main")
                        .unwrap(),
                    initial,
                    "provider must accept before G5 moves its target"
                );
                let repo = git2::Repository::open_bare(&remote_path).unwrap();
                if fail_first_import {
                    repo.find_reference("refs/heads/upstream")
                        .unwrap()
                        .delete()
                        .unwrap();
                } else {
                    repo.reference(
                        "refs/heads/upstream",
                        result_head.parse().unwrap(),
                        true,
                        "provider merge",
                    )
                    .unwrap();
                }
                accepted.store(true, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(json!({"merged":true,"sha":result_head}))
            })
            .expect(1)
            .mount(&github)
            .await;
        Self {
            app,
            fixture,
            github,
            remote,
            accepted_head,
        }
    }

    async fn assert_imported(&self, body: &Value) {
        assert_eq!(
            head(&self.app, self.fixture.source.repo_id, "main").await,
            self.accepted_head,
            "{body}"
        );
        assert_eq!(
            head(&self.app, self.fixture.source.repo_id, "feature").await,
            self.fixture.source_head
        );
        let repo = GitRepo::open(&self.app.repo_path(self.fixture.source.repo_id).await).unwrap();
        assert_eq!(
            repo.read_sync_watermark("main").unwrap().as_deref(),
            Some(self.accepted_head.as_str())
        );
        assert_eq!(body["data"]["pullRequest"]["status"], "merged", "{body}");
        assert_eq!(
            body["data"]["pullRequest"]["providerStatus"], "merged",
            "{body}"
        );
        let response = self
            .app
            .authenticated_client(&self.fixture.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/clone", self.fixture.base_id),
                &json!({"name":"next-child","idempotencyKey":"after-provider-merge"}),
            )
            .await;
        let status = response.status();
        let fork: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "{fork}");
        assert_eq!(
            head(&self.app, self.fixture.source.repo_id, "next-child").await,
            self.accepted_head,
            "the next fork must inherit GitHub's actual merge commit"
        );
        self.github.verify().await;
    }
}

#[tokio::test]
async fn provider_first_merge_imports_rebased_history_for_base_and_non_base_targets() {
    if isolated_provider_test(
        "provider_first_merge_imports_rebased_history_for_base_and_non_base_targets",
    ) {
        return;
    }
    for is_base in [false, true] {
        let provider = ProviderFixture::create(is_base, false).await;
        let direct = provider
            .app
            .authenticated_client(&provider.fixture.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/merge", provider.fixture.source.id),
                &json!({}),
            )
            .await;
        assert_eq!(
            direct.status(),
            StatusCode::CONFLICT,
            "direct local merges must not bypass provider acceptance for either target kind"
        );
        provider.fixture.assert_unpublished(&provider.app).await;
        let (status, body) = provider.fixture.ship(&provider.app).await;
        assert_eq!(status, StatusCode::OK, "is_base={is_base}: {body}");
        provider.assert_imported(&body).await;
        provider.app.cleanup().await;
    }
}

/// `g5.governance.remote.workspace-rewrite`: Create/Update PR classifies a
/// tracked source against its synchronization watermark like forwarding does.
/// Provider commits the source lacks, or a provider rewrite, are never
/// replaced; an accepted workspace rewrite over the unchanged provider is.
#[tokio::test]
async fn update_pr_publishes_tracked_sources_only_over_unchanged_provider_history() {
    const NAME: &str = "update_pr_publishes_tracked_sources_only_over_unchanged_provider_history";
    if isolated_provider_test(NAME) {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let bare = provider.remote.path().join("remote.git");
    let mirror = app.repo_path(f.source.repo_id).await;
    git2::Repository::open(&mirror)
        .unwrap()
        .remote(
            "origin",
            &format!("https://github.com/g5-test-{}/api.git", f.org.id),
        )
        .unwrap();
    // The provider source branch was last synchronized at the source head.
    git(&bare, &["update-ref", "refs/heads/feature", &f.source_head]);
    GitRepo::open(&mirror)
        .unwrap()
        .write_baseline_ref("feature", &f.source_head)
        .unwrap();
    // Replace the provider source branch from another client.
    let provider_commit = |base: &str, file: &str| {
        let work = tempfile::tempdir().unwrap();
        git(work.path(), &["init", "-q"]);
        git(work.path(), &["fetch", "-q", bare.to_str().unwrap(), base]);
        git(work.path(), &["checkout", "-q", "FETCH_HEAD"]);
        std::fs::write(work.path().join(file), "provider\n").unwrap();
        git(work.path(), &["add", file]);
        git(
            work.path(),
            &[
                "-c",
                "user.name=Other",
                "-c",
                "user.email=o@example.test",
                "commit",
                "-qm",
                file,
            ],
        );
        git(
            work.path(),
            &[
                "push",
                "-q",
                "-f",
                bare.to_str().unwrap(),
                "HEAD:refs/heads/feature",
            ],
        );
        git(&bare, &["rev-parse", "refs/heads/feature"])
    };
    let update_pr = || async {
        let response = app
            .authenticated_client(&f.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
                &json!({"targetWorkspaceId":f.base_id}),
            )
            .await;
        (response.status(), response.json::<Value>().await.unwrap())
    };
    let local = commit(app, f.source.repo_id, "feature", "local-after-sync.txt").await;

    // The provider advanced normally with a commit G5 lacks.
    let advanced = provider_commit(&f.source_head, "provider-advance.txt");
    let (status, body) = update_pr().await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string().contains("PR_SOURCE_PROVIDER_ADVANCED"),
        "{body}"
    );
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/feature"]), advanced);

    // Without a watermark nothing proves a rewrite: only a fast-forward
    // publishes, so the provider commit stays.
    git(
        &mirror,
        &["update-ref", "-d", "refs/g5-tracking/feature/baseline"],
    );
    let (status, body) = update_pr().await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string().contains("PR_SOURCE_PROVIDER_ADVANCED"),
        "{body}"
    );
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/feature"]), advanced);
    GitRepo::open(&mirror)
        .unwrap()
        .write_baseline_ref("feature", &f.source_head)
        .unwrap();

    // The provider rewrote the branch below the watermark.
    let rewritten = provider_commit(&f.base_head, "provider-rewrite.txt");
    let (status, body) = update_pr().await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string().contains("PR_SOURCE_PROVIDER_REWRITTEN"),
        "{body}"
    );
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/feature"]), rewritten);

    // The provider is back at the watermark and G5 holds a workspace rewrite:
    // the leased push replaces it.
    git(&bare, &["update-ref", "refs/heads/feature", &f.source_head]);
    let repo = GitRepo::open(&mirror).unwrap();
    repo.checkout_branch_force("feature").unwrap();
    git(&mirror, &["reset", "-q", "--hard", &f.base_head]);
    let workspace_rewrite = commit(app, f.source.repo_id, "feature", "rewritten-source.txt").await;
    assert_ne!(workspace_rewrite, local);
    {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};
        let pulls = format!("/repos/g5-test-{}/api/pulls", f.org.id);
        let pull = json!({"number":42,"node_id":"PR_rewrite42",
            "html_url":format!("https://github.com/g5-test-{}/api/pull/42", f.org.id),
            "state":"open","merged":false,
            "head":{"sha":workspace_rewrite},"base":{"sha":f.base_head}});
        Mock::given(method("GET"))
            .and(path(&pulls))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&provider.github)
            .await;
        Mock::given(method("POST"))
            .and(path(&pulls))
            .respond_with(ResponseTemplate::new(201).set_body_json(pull))
            .mount(&provider.github)
            .await;
        // The fixture's recorded #21 is still open on GitHub, so it was not
        // merged or closed outside G5 (#387).
        Mock::given(method("GET"))
            .and(path(format!("{pulls}/21")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":21,
                "html_url":format!("https://github.com/g5-test-{}/api/pull/21", f.org.id),
                "state":"open","merged":false,
                "head":{"sha":f.source_head},"base":{"sha":f.base_head}})))
            .mount(&provider.github)
            .await;
    }
    let (status, body) = update_pr().await;
    // The publication completes end to end, not just the source push.
    assert_eq!(status, StatusCode::OK, "{body}");
    let pull_request = &body["data"]["pullRequest"];
    assert_eq!(pull_request["providerStatus"], "open", "{body}");
    assert!(pull_request["blockingMessage"].is_null(), "{body}");
    assert_eq!(
        git(&bare, &["rev-parse", "refs/heads/feature"]),
        workspace_rewrite
    );
    provider.app.cleanup().await;
}

/// Base's own auto-pull can land the accepted provider commit while Ship to
/// Base is importing it: after the import captured Base, before it prepared
/// the landing. Ship recaptures once and completes against the converged Base.
#[tokio::test]
async fn provider_first_merge_completes_when_target_auto_pull_lands_it_first() {
    if isolated_provider_test("provider_first_merge_completes_when_target_auto_pull_lands_it_first")
    {
        return;
    }
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(true, false).await;
    let mirror = provider
        .app
        .repo_path(provider.fixture.source.repo_id)
        .await;
    let remote = provider.remote.path().join("remote.git");
    let landed = Arc::new(AtomicBool::new(false));
    let (accepted, source_head, base_head) = (
        provider.accepted_head.clone(),
        provider.fixture.source_head.clone(),
        provider.fixture.base_head.clone(),
    );
    let hook = landed.clone();
    // The import reads the merged PR after capturing Base; that read lands the
    // accepted commit on Base, the way the background auto-pull cycle does.
    let merged_reads = std::sync::atomic::AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/21"))
        .respond_with(move |_: &wiremock::Request| {
            let merged = git(&remote, &["rev-parse", "refs/heads/upstream"]) == accepted;
            // The merge step verifies the merged PR first; the import's read,
            // after it captured Base, is the second.
            if merged && merged_reads.fetch_add(1, Ordering::SeqCst) == 1 {
                hook.store(true, Ordering::SeqCst);
                git(
                    &mirror,
                    &[
                        "fetch",
                        "-q",
                        remote.to_str().unwrap(),
                        "refs/heads/upstream",
                    ],
                );
                git(
                    &mirror,
                    &["update-ref", "refs/heads/main", &accepted, &base_head],
                );
                git(
                    &mirror,
                    &["update-ref", "refs/g5-tracking/main/baseline", &accepted],
                );
            }
            ResponseTemplate::new(200).set_body_json(json!({
                "number":21,"html_url":"https://github.com/acme/api/pull/21",
                "state":if merged {"closed"} else {"open"},"merged":merged,
                "head":{"sha":source_head},"base":{"sha":base_head},
                "merge_commit_sha":if merged {Some(&accepted)} else {None}
            }))
        })
        .with_priority(1)
        .mount(&provider.github)
        .await;
    let (status, body) = provider.fixture.ship(&provider.app).await;
    assert!(landed.load(Ordering::SeqCst), "the race was not exercised");
    assert_eq!(status, StatusCode::OK, "{body}");
    provider.assert_imported(&body).await;
    provider.app.cleanup().await;
}

#[tokio::test]
async fn provider_first_import_failure_keeps_target_unchanged_and_retries_accepted_merge() {
    if isolated_provider_test(
        "provider_first_import_failure_keeps_target_unchanged_and_retries_accepted_merge",
    ) {
        return;
    }
    let provider = ProviderFixture::create(false, true).await;
    let (status, body) = provider.fixture.ship(&provider.app).await;
    assert_ne!(status, StatusCode::OK, "{body}");
    provider.fixture.assert_unpublished(&provider.app).await;
    let status: String =
        sqlx::query_scalar("SELECT provider_status FROM workspace_pull_requests WHERE id=$1")
            .bind(provider.fixture.pull_request_id)
            .fetch_one(&provider.app.db)
            .await
            .unwrap();
    assert_eq!(
        status, "merged",
        "provider acceptance survives a failed fetch"
    );
    let repo = git2::Repository::open_bare(provider.remote.path().join("remote.git")).unwrap();
    repo.reference(
        "refs/heads/upstream",
        provider.accepted_head.parse().unwrap(),
        true,
        "recover fetch",
    )
    .unwrap();
    let (status, body) = provider.fixture.ship(&provider.app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    provider.assert_imported(&body).await;
    provider.app.cleanup().await;
}

#[tokio::test]
async fn provider_import_rechecks_source_target_and_binding_before_landing() {
    if isolated_provider_test("provider_import_rechecks_source_target_and_binding_before_landing") {
        return;
    }
    for change in ["source", "target", "binding"] {
        let provider = ProviderFixture::create(false, true).await;
        let app = &provider.app;
        let f = &provider.fixture;
        let (status, _) = f.ship(app).await;
        assert_ne!(status, StatusCode::OK);
        let repo = git2::Repository::open_bare(provider.remote.path().join("remote.git")).unwrap();
        repo.reference(
            "refs/heads/upstream",
            provider.accepted_head.parse().unwrap(),
            true,
            "recover fetch",
        )
        .unwrap();
        let db = app.scoped_db_for_org(f.org.id).await;
        let imports = app
            .state
            .workspace_pull_request_provider_lifecycle_service
            .prepare_merged_target_import(db, f.pull_request_id, f.base_id, f.owner.id)
            .await
            .unwrap();
        let db = app.scoped_db_for_org(f.org.id).await;
        let prepared = app
            .state
            .coordinated_repo_publication_service
            .prepare_provider_workspace_pull_request_merge(
                db,
                &app.state.workspace_merge_service,
                g5_api::services::coordinated_repo_publication::ProviderFirstMergeRequest {
                    pull_request_id: f.pull_request_id,
                    org_id: f.org.id,
                    source_workspace_id: f.source.id,
                    target_workspace_id: f.base_id,
                    actor_id: f.owner.id,
                    target_heads: Vec::new(),
                },
                imports,
            )
            .await
            .unwrap();
        let expected_target = match change {
            "source" => {
                commit(app, f.source.repo_id, "feature", "new-source.txt").await;
                f.base_head.clone()
            }
            "target" => commit(app, f.source.repo_id, "main", "new-target.txt").await,
            "binding" => {
                sqlx::query("UPDATE remote_integrations SET remote_branch='another-target' WHERE workspace_id=$1 AND repo_id=$2")
                    .bind(f.base_id).bind(f.source.repo_id).execute(&app.db).await.unwrap();
                f.base_head.clone()
            }
            _ => unreachable!(),
        };
        let mut db = app.scoped_db_for_org(f.org.id).await;
        let result = app
            .state
            .coordinated_repo_publication_service
            .apply_prepared_workspace_pull_request_merge(
                &mut db,
                &app.state.workspace_merge_service,
                prepared,
            )
            .await;
        assert!(
            result.is_err(),
            "{change} must invalidate the prepared import"
        );
        db.rollback().await.unwrap();
        assert_eq!(head(app, f.source.repo_id, "main").await, expected_target);
        let status: String =
            sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id=$1")
                .bind(f.pull_request_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(status, "open");
        provider.github.verify().await;
        provider.app.cleanup().await;
    }
}

async fn bind_other_member(app: &TestApp, f: &Fixture) {
    let git_url = format!("https://github.com/g5-test-{}/unchanged.git", f.org.id);
    if std::env::var("G5_PROVIDER_TEST_CASE").is_ok() {
        let mirror = app.repo_path(f.other_repo_id).await;
        g5_scm::native::redirect_transport_for_tests(&git_url, &mirror);
    }
    sqlx::query("UPDATE repos SET git_url=$2,provider='github',provider_full_name='acme/unchanged' WHERE id=$1")
        .bind(f.other_repo_id).bind(git_url).execute(&app.db).await.unwrap();
    for (workspace_id, branch) in [(f.base_id, "main"), (f.source.id, "feature")] {
        let mut db = app.scoped_db_for_org(f.org.id).await;
        g5_api::services::remote_integrations::bind_published(
            db.conn(),
            g5_contracts::types::WorkspaceRepoKey {
                org_id: f.org.id,
                workspace_id,
                repo_id: f.other_repo_id,
            },
            branch,
        )
        .await
        .unwrap();
        db.commit().await.unwrap();
    }
}

#[tokio::test]
async fn local_changes_merge_with_identical_or_included_remote_members() {
    for (is_base, target_ahead) in [(false, false), (true, true)] {
        let app = TestApp::spawn().await;
        let f = Fixture::create_with_kind(&app, false, is_base).await;
        let unchanged = head(&app, f.other_repo_id, "feature").await;
        assert_eq!(unchanged, head(&app, f.other_repo_id, "main").await);
        bind_other_member(&app, &f).await;
        let expected_target = if target_ahead {
            commit(&app, f.other_repo_id, "main", "already-included.txt").await
        } else {
            unchanged.clone()
        };
        let repo = GitRepo::open(&app.repo_path(f.other_repo_id).await).unwrap();
        repo.write_baseline_ref("main", &unchanged).unwrap();
        sqlx::query("UPDATE remote_integrations SET sync_paused_reason='diverged' WHERE workspace_id=$1 AND repo_id=$2")
            .bind(f.base_id).bind(f.other_repo_id).execute(&app.db).await.unwrap();
        let mut db = app.scoped_db_for_org(f.org.id).await;
        app.state
            .workspace_pull_request_provider_lifecycle_service
            .create_or_update_provider_pull_requests(&mut db, f.pull_request_id, f.owner.id, None)
            .await
            .unwrap();
        db.commit().await.unwrap();
        let provider_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
        )
        .bind(f.pull_request_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(provider_rows, 0, "an unchanged member has no provider PR");
        let (status, body) = f.ship(&app).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(head(&app, f.source.repo_id, "main").await, f.source_head);
        assert_eq!(head(&app, f.other_repo_id, "main").await, expected_target);
        assert_eq!(body["data"]["pullRequest"]["status"], "merged");
        assert_eq!(body["data"]["pullRequest"]["providerStatus"], "local_only");
        assert_eq!(repo.read_sync_watermark("main").unwrap(), Some(unchanged));
        let pause: Option<String> = sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2")
            .bind(f.base_id).bind(f.other_repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(
            pause.as_deref(),
            Some("diverged"),
            "a local no-op does not claim provider convergence"
        );
        let sync_runs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2",
        )
        .bind(f.base_id)
        .bind(f.other_repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(sync_runs, 0);
        let (status, _) = f.ship(&app).await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "completion remains idempotent"
        );
        app.cleanup().await;
    }
}

#[tokio::test]
async fn provider_merge_supports_unchanged_remote_and_changed_local_siblings() {
    if isolated_provider_test("provider_merge_supports_unchanged_remote_and_changed_local_siblings")
    {
        return;
    }
    for remote_sibling in [false, true] {
        let provider = ProviderFixture::create(false, false).await;
        let app = &provider.app;
        let f = &provider.fixture;
        let sibling_head = if remote_sibling {
            bind_other_member(app, f).await;
            head(app, f.other_repo_id, "main").await
        } else {
            commit(app, f.other_repo_id, "feature", "local-change.txt").await
        };
        let (status, body) = f.ship(app).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "remote_sibling={remote_sibling}: {body}"
        );
        provider.assert_imported(&body).await;
        assert_eq!(head(app, f.other_repo_id, "main").await, sibling_head);
        let sync_runs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2",
        )
        .bind(f.base_id)
        .bind(f.other_repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(
            sync_runs, 0,
            "only accepted provider history has convergence effects"
        );
        provider.github.verify().await;
        provider.app.cleanup().await;
    }
}

#[tokio::test]
async fn changed_remote_member_without_provider_pr_blocks_before_any_provider_mutation() {
    if isolated_provider_test(
        "changed_remote_member_without_provider_pr_blocks_before_any_provider_mutation",
    ) {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    bind_other_member(app, f).await;
    commit(app, f.other_repo_id, "feature", "unpublished-source.txt").await;
    provider.github.reset().await;
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("source changes without a matching provider PR"));
    // The merge route reconciles provider acceptance with a read before
    // admitting; no provider merge may follow.
    assert!(
        !provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method == "PUT" && request.url.path().ends_with("/merge")),
        "missing provider coverage must block before the first provider merge"
    );
    f.assert_unpublished(app).await;
    provider.app.cleanup().await;
}

#[tokio::test]
async fn unchanged_remote_import_rechecks_heads_and_policy_before_landing() {
    for change in ["before_import", "source", "target", "binding"] {
        let app = TestApp::spawn().await;
        let f = Fixture::create_with_kind(&app, false, false).await;
        bind_other_member(&app, &f).await;
        let service = &app.state.workspace_pull_request_provider_lifecycle_service;
        let db = app.scoped_db_for_org(f.org.id).await;
        // No provider PR: only omitted members, so no target is classified.
        let db = service
            .merge_provider_pull_requests(db, f.pull_request_id, &[], f.owner.id, || async {
                Ok(app.scoped_db_for_org(f.org.id).await)
            })
            .await
            .unwrap();
        db.commit().await.unwrap();
        let old_target = head(&app, f.other_repo_id, "main").await;
        if change == "before_import" {
            commit(&app, f.other_repo_id, "feature", "new-source.txt").await;
        }
        let db = app.scoped_db_for_org(f.org.id).await;
        let imports = service
            .prepare_merged_target_import(db, f.pull_request_id, f.base_id, f.owner.id)
            .await;
        if change == "before_import" {
            assert!(
                imports.is_err(),
                "new source changes invalidate the no-op before import preparation"
            );
            f.assert_unpublished(&app).await;
            app.cleanup().await;
            continue;
        }
        let db = app.scoped_db_for_org(f.org.id).await;
        let prepared = app
            .state
            .coordinated_repo_publication_service
            .prepare_provider_workspace_pull_request_merge(
                db,
                &app.state.workspace_merge_service,
                g5_api::services::coordinated_repo_publication::ProviderFirstMergeRequest {
                    pull_request_id: f.pull_request_id,
                    org_id: f.org.id,
                    source_workspace_id: f.source.id,
                    target_workspace_id: f.base_id,
                    actor_id: f.owner.id,
                    target_heads: Vec::new(),
                },
                imports.unwrap(),
            )
            .await
            .unwrap();
        let expected_target = match change {
            "source" => {
                commit(&app, f.other_repo_id, "feature", "new-source.txt").await;
                old_target
            }
            "target" => commit(&app, f.other_repo_id, "main", "new-target.txt").await,
            "binding" => {
                sqlx::query("UPDATE remote_integrations SET remote_branch='moved' WHERE workspace_id=$1 AND repo_id=$2")
                    .bind(f.base_id).bind(f.other_repo_id).execute(&app.db).await.unwrap();
                old_target
            }
            _ => unreachable!(),
        };
        let mut db = app.scoped_db_for_org(f.org.id).await;
        let result = app
            .state
            .coordinated_repo_publication_service
            .apply_prepared_workspace_pull_request_merge(
                &mut db,
                &app.state.workspace_merge_service,
                prepared,
            )
            .await;
        assert!(
            result.is_err(),
            "{change} must invalidate the prepared unchanged member"
        );
        db.rollback().await.unwrap();
        assert_eq!(head(&app, f.other_repo_id, "main").await, expected_target);
        f.assert_unpublished(&app).await;
        app.cleanup().await;
    }
}

#[tokio::test]
async fn vm_create_pr_refuses_wrong_authority_before_saving_or_publishing() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &f).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach(&app, &f.source, f.owner.id).await;
    let token = vm_pr_token(&app, &f, &peer);
    let route = format!("/api/internal/workspaces/{}/pull-requests", f.source.id);
    let request = json!({"targetWorkspaceId": f.base_id});
    let host = helpers::test_jwt_minter()
        .mint_host_jwt(
            peer.host_id(),
            f.org.id,
            std::time::Duration::from_secs(3600),
        )
        .unwrap();
    for credential in [&f.owner.jwt, &host] {
        assert_eq!(
            post_pr(&app, credential, &route, &request).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        post_pr(
            &app,
            &token,
            &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
            &request
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_pr(
            &app,
            &token,
            &format!("/api/internal/workspaces/{}/pull-requests", f.base_id),
            &request
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    for status in ["starting", "stopping"] {
        sqlx::query("UPDATE workspace_vms SET status=$2 WHERE workspace_id=$1")
            .bind(f.source.id)
            .bind(status)
            .execute(&app.db)
            .await
            .unwrap();
        assert!(
            !post_pr(&app, &token, &route, &request)
                .await
                .status()
                .is_success(),
            "{status}"
        );
    }
    sqlx::query(
        "UPDATE workspace_vms SET status='stopped',host_id=NULL,vm_id=NULL WHERE workspace_id=$1",
    )
    .bind(f.source.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert!(!post_pr(&app, &token, &route, &request)
        .await
        .status()
        .is_success());
    sqlx::query(
        "UPDATE workspace_vms SET status='running',host_id=$2,vm_id=$3 WHERE workspace_id=$1",
    )
    .bind(f.source.id)
    .bind(peer.host_id())
    .bind(peer.vm_id().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("UPDATE workspace_vms SET status='running',epoch=2 WHERE workspace_id=$1")
        .bind(f.source.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(!post_pr(&app, &token, &route, &request)
        .await
        .status()
        .is_success());
    sqlx::query("UPDATE workspace_vms SET epoch=1,lease_expires_at=now()-interval '1 minute' WHERE workspace_id=$1").bind(f.source.id).execute(&app.db).await.unwrap();
    assert!(!post_pr(&app, &token, &route, &request)
        .await
        .status()
        .is_success());
    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at=now()+interval '1 hour' WHERE workspace_id=$1",
    )
    .bind(f.source.id)
    .execute(&app.db)
    .await
    .unwrap();
    let other = TestUser::create_with_scopes(
        &app,
        f.org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    sqlx::query("INSERT INTO workspace_ownership_transfers(id,org_id,workspace_id,from_user_id,to_user_id,requested_by,idempotency_key,request_digest,status) VALUES($1,$2,$3,$4,$5,$4,'pr-transfer',repeat('a',64),'pending')")
        .bind(Uuid::new_v4()).bind(f.org.id).bind(f.source.id).bind(f.owner.id).bind(other.id).execute(&app.db).await.unwrap();
    assert!(!post_pr(&app, &token, &route, &request)
        .await
        .status()
        .is_success());
    sqlx::query("DELETE FROM workspace_ownership_transfers WHERE workspace_id=$1")
        .bind(f.source.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.source.id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        post_pr(&app, &token, &route, &request).await.status(),
        StatusCode::FORBIDDEN
    );
    assert!(
        peer.received_rpc_methods().await.is_empty(),
        "refused callers must not save source work"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn vm_and_ui_pr_adapters_share_target_access_and_source_restrictions() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &f).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach(&app, &f.source, f.owner.id).await;
    let token = vm_pr_token(&app, &f, &peer);
    let other = TestUser::create_with_scopes(
        &app,
        f.org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    TestUser::create_admin(&app, f.org.id).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.base_id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    for (prefix, credential) in [("v1", &f.owner.jwt), ("internal", &token)] {
        let route = format!("/api/{prefix}/workspaces/{}/pull-requests", f.source.id);
        assert_eq!(
            post_pr(
                &app,
                credential,
                &route,
                &json!({"targetWorkspaceId":f.base_id,"save":false})
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert_eq!(
            post_pr(
                &app,
                credential,
                &route,
                &json!({"targetWorkspaceId":f.source.id})
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.base_id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET mutation_policy='read_only' WHERE id=$1")
        .bind(f.source.id)
        .execute(&app.db)
        .await
        .unwrap();
    for (prefix, credential) in [("v1", &f.owner.jwt), ("internal", &token)] {
        let response = post_pr(
            &app,
            credential,
            &format!("/api/{prefix}/workspaces/{}/pull-requests", f.source.id),
            &json!({"targetWorkspaceId":f.base_id,"save":false}),
        )
        .await;
        assert!(!response.status().is_success());
        let body: Value = response.json().await.unwrap();
        assert!(body.to_string().contains("READ_ONLY"), "{body}");
    }
    assert!(peer.received_rpc_methods().await.is_empty());
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vm_create_pr_rechecks_target_access_after_save() {
    assert_target_access_rechecked_after_save(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ui_create_pr_rechecks_target_access_after_save() {
    assert_target_access_rechecked_after_save(false).await;
}

async fn assert_target_access_rechecked_after_save(vm: bool) {
    let app = TestApp::spawn().await;
    let f = Fixture::create_with_kind(&app, false, false).await;
    remove_fixture_pr(&app, &f).await;
    let target_owner = TestUser::create_with_scopes(
        &app,
        f.org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    TestUser::create_admin(&app, f.org.id).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2,mutation_policy='read_only' WHERE id=$1")
        .bind(f.base_id)
        .bind(target_owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &f.source,
        f.owner.id,
        "modules.invoke",
    )
    .await;
    let token = if vm {
        vm_pr_token(&app, &f, &peer)
    } else {
        f.owner.jwt.clone()
    };
    let route = format!(
        "/api/{}/workspaces/{}/pull-requests",
        if vm { "internal" } else { "v1" },
        f.source.id
    );
    let request = if vm {
        json!({})
    } else {
        json!({"targetWorkspaceId":f.base_id})
    };
    insert_pr_base(&app, &f.org, "Fallback").await;
    let revoke = async {
        wait_for_operation(&peer, "commit").await;
        let saved = commit(
            &app,
            f.source.repo_id,
            &f.source.name,
            "before-target-revocation.txt",
        )
        .await;
        peer.release_one_rpc();
        wait_for_operation(&peer, "push").await;
        // Deletion wins during the save gap, before the next admission phase.
        sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
            .bind(f.base_id)
            .execute(&app.db)
            .await
            .unwrap();
        peer.release_one_rpc();
        saved
    };
    let (response, saved) = tokio::join!(post_pr(&app, &token, &route, &request), revoke);
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(head(&app, f.source.repo_id, &f.source.name).await, saved);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "revocation must stop before recording target evidence"
    );
    // Restoring the target's liveness restores its ordinary audience.
    sqlx::query("UPDATE workspaces SET deleted_at=NULL WHERE id=$1")
        .bind(f.base_id)
        .execute(&app.db)
        .await
        .unwrap();
    peer.release_one_rpc();
    peer.release_one_rpc();
    let response = post_pr(&app, &token, &route, &request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vm_create_pr_reports_admission_failure_after_local_commit_and_retry_reuses_pr() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &f).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach(&app, &f.source, f.owner.id).await;
    let token = vm_pr_token(&app, &f, &peer);
    sqlx::raw_sql("CREATE FUNCTION pause_pr_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(8675309); RETURN NEW; END $$; CREATE TRIGGER pause_pr_insert BEFORE INSERT ON workspace_pull_requests FOR EACH ROW EXECUTE FUNCTION pause_pr_insert();")
        .execute(&app.db).await.unwrap();
    let mut barrier = app.db.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(8675309)")
        .execute(&mut *barrier)
        .await
        .unwrap();
    let route = format!("/api/internal/workspaces/{}/pull-requests", f.source.id);
    let request = json!({"targetWorkspaceId":f.base_id});
    let replacement = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND locktype='advisory' AND objid=8675309 AND NOT granted)")
                    .fetch_one(&mut *barrier).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        let pool = app.db.clone();
        let workspace = f.source.id;
        let replace = tokio::spawn(async move {
            sqlx::query("/* pr-provider-admission-race */ UPDATE workspace_vms SET epoch=epoch+1 WHERE workspace_id=$1")
                .bind(workspace).execute(&pool).await.unwrap();
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND query LIKE '/* pr-provider-admission-race */%' AND wait_event_type='Lock')")
                    .fetch_one(&mut *barrier).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        // The queued authority change acquires the source fence as soon as the
        // local PR commits, before provider admission can take it again.
        sqlx::query("SELECT pg_advisory_unlock(8675309)")
            .execute(&mut *barrier)
            .await
            .unwrap();
        replace.await.unwrap();
    };
    let (response, ()) = tokio::join!(post_pr(&app, &token, &route, &request), replacement);
    drop(barrier);
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["error"]["details"]["kind"], "pull_request_provider_sync_incomplete",
        "{body}"
    );
    assert_eq!(body["error"]["details"]["providerSyncPhase"], "admission");
    assert_eq!(
        body["error"]["details"]["providerSyncOutcome"],
        "not_attempted"
    );
    let id: Uuid =
        sqlx::query_scalar("SELECT id FROM workspace_pull_requests WHERE source_workspace_id=$1")
            .bind(f.source.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(body["error"]["details"]["pullRequestId"], id.to_string());
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains(&id.to_string()));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    let current_token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, f.source.id, f.owner.id, peer.vm_id(), 2, 3600)
        .unwrap();
    let response = post_pr(&app, &current_token, &route, &request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let persisted: Uuid =
        sqlx::query_scalar("SELECT id FROM workspace_pull_requests WHERE source_workspace_id=$1")
            .bind(f.source.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(persisted, id);
    app.cleanup().await;
}

async fn insert_pr_base(app: &TestApp, org: &TestOrg, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) SELECT $1,$2,$3,true,user_id,user_id FROM org_memberships WHERE org_id=$2 ORDER BY user_id LIMIT 1")
        .bind(id).bind(org.id).bind(name).execute(&app.db).await.unwrap();
    id
}

#[tokio::test]
async fn pr_target_choices_are_shared_ordered_and_audience_scoped() {
    let app = TestApp::spawn().await;
    let f = Fixture::create_with_kind(&app, false, false).await;
    // Exercise ordinary member access rather than an administrator bypass.
    TestUser::create_admin(&app, f.org.id).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let other =
        TestUser::create_with_scopes(&app, f.org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let alpha = insert_pr_base(&app, &f.org, "Alpha").await;
    let beta = insert_pr_base(&app, &f.org, "Beta").await;
    let private = TestWorkspace::create_personal(&app, &f.org, other.id)
        .await
        .id;
    let deleted = insert_pr_base(&app, &f.org, "A deleted").await;
    let sibling = TestWorkspace::create_named(&app, &f.org, "a-sibling").await;
    let mut outsider = TestUser::create(&app).await;
    let foreign_org = TestOrg::create_with_admin(&app, &mut outsider).await;
    insert_pr_base(&app, &foreign_org, "A foreign").await;
    sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
        .bind(deleted)
        .execute(&app.db)
        .await
        .unwrap();
    let route = format!("/api/v1/workspaces/{}/pull-requests", f.source.id);
    let ids = |body: &Value| {
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().parse::<Uuid>().unwrap())
            .collect::<Vec<_>>()
    };
    let response = app
        .authenticated_client(&f.owner)
        .get(&format!("{route}/targets"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let choices: Value = response.json().await.unwrap();
    assert_eq!(ids(&choices), vec![f.base_id, alpha, beta]);
    assert!(!ids(&choices).contains(&sibling.id));
    // Inaccessible or deleted parents cannot become the implicit target.
    for parent in [private, deleted] {
        sqlx::query("UPDATE workspaces SET cloned_from_id=$2 WHERE id=$1")
            .bind(f.source.id)
            .bind(parent)
            .execute(&app.db)
            .await
            .unwrap();
        let response = app
            .authenticated_client(&f.owner)
            .get(&format!("{route}/targets"))
            .await;
        let choices: Value = response.json().await.unwrap();
        assert_eq!(ids(&choices), vec![alpha, beta]);
    }
    // A missing parent falls back to the ordered Base list. Live names in an
    // organization are unique ignoring case and Base keeps its name
    // (g5.workspace.identity), so names never tie.
    sqlx::query("UPDATE workspaces SET cloned_from_id=NULL WHERE id=$1")
        .bind(f.source.id)
        .execute(&app.db)
        .await
        .unwrap();
    let response = app
        .authenticated_client(&f.owner)
        .get(&format!("{route}/targets"))
        .await;
    let choices: Value = response.json().await.unwrap();
    assert_eq!(ids(&choices), vec![alpha, beta]);
    // No default and blank titles must fail before contacting the live workspace.
    sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE org_id=$1 AND is_base")
        .bind(f.org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let peer = helpers::TestWorkspaceRuntimePeer::attach(&app, &f.source, f.owner.id).await;
    let vm = vm_pr_token(&app, &f, &peer);
    for (token, path) in [
        (&f.owner.jwt, route.clone()),
        (
            &vm,
            format!("/api/internal/workspaces/{}/pull-requests", f.source.id),
        ),
    ] {
        for body in [
            json!({}),
            json!({"targetWorkspaceId": f.base_id, "title":"   "}),
        ] {
            let response = post_pr(&app, token, &path, &body).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    assert!(peer.received_rpc_methods().await.is_empty());
    // Explicit readable targets are still valid even when absent from the defaults.
    let response = post_pr(
        &app,
        &f.owner.jwt,
        &route,
        &json!({"targetWorkspaceId":f.base_id}),
    )
    .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["targetWorkspaceId"],
        f.base_id.to_string()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn explicit_title_retries_preserve_identity_and_update_included_members() {
    const NAME: &str = "explicit_title_retries_preserve_identity_and_update_included_members";
    if isolated_provider_test(NAME) {
        return;
    }
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    sqlx::query("DELETE FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    let git_url = format!("https://github.com/g5-test-{}/api.git", f.org.id);
    let mirror = app.repo_path(f.source.repo_id).await;
    git2::Repository::open(&mirror)
        .unwrap()
        .remote("origin", &git_url)
        .unwrap();
    let pulls_path = format!("/repos/g5-test-{}/api/pulls", f.org.id);
    let pull_path = format!("{pulls_path}/42");
    let remote: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
    let fail_create = Arc::new(AtomicBool::new(true));
    let fail_patch = Arc::new(AtomicBool::new(false));
    let creates = Arc::new(AtomicUsize::new(0));
    let patches = Arc::new(AtomicUsize::new(0));
    let delayed_patch_started = Arc::new(AtomicBool::new(false));
    let state = remote.clone();
    Mock::given(method("GET"))
        .and(path(&pulls_path))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200)
                .set_body_json(state.lock().unwrap().iter().cloned().collect::<Vec<_>>())
        })
        .mount(&provider.github)
        .await;
    let state = remote.clone();
    let fail = fail_create.clone();
    let count = creates.clone();
    let pull = json!({"number":42,"node_id":"PR_saved42","html_url":format!("https://github.com/g5-test-{}/api/pull/42", f.org.id), "state":"open","merged":false,"head":{"sha":f.source_head},"base":{"sha":f.base_head}});
    Mock::given(method("POST"))
        .and(path(&pulls_path))
        .respond_with(move |request: &wiremock::Request| {
            count.fetch_add(1, Ordering::SeqCst);
            if fail.swap(false, Ordering::SeqCst) {
                return ResponseTemplate::new(503)
                    .insert_header("x-github-request-id", "long-create-request")
                    .set_body_json(json!({"message":"x".repeat(900)}));
            }
            let mut value = pull.clone();
            value["title"] = request.body_json::<Value>().unwrap()["title"].clone();
            *state.lock().unwrap() = Some(value.clone());
            ResponseTemplate::new(201).set_body_json(value)
        })
        .mount(&provider.github)
        .await;
    let state = remote.clone();
    Mock::given(method("GET"))
        .and(path(&pull_path))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(state.lock().unwrap().clone().unwrap())
        })
        .mount(&provider.github)
        .await;
    let state = remote.clone();
    let fail = fail_patch.clone();
    let count = patches.clone();
    let delayed = delayed_patch_started.clone();
    Mock::given(method("PATCH"))
        .and(path(&pull_path))
        .respond_with(move |request: &wiremock::Request| {
            count.fetch_add(1, Ordering::SeqCst);
            if fail.swap(false, Ordering::SeqCst) {
                return ResponseTemplate::new(422).set_body_json(json!({"message":"Retry title"}));
            }
            let mut guard = state.lock().unwrap();
            let value = guard.as_mut().unwrap();
            value["title"] = request.body_json::<Value>().unwrap()["title"].clone();
            let response = ResponseTemplate::new(200).set_body_json(value.clone());
            if value["title"] == "Concurrent A" {
                delayed.store(true, Ordering::SeqCst);
                response.set_delay(std::time::Duration::from_secs(5))
            } else {
                response
            }
        })
        .mount(&provider.github)
        .await;
    let route = format!("/api/v1/workspaces/{}/pull-requests", f.source.id);
    let route = route.as_str();
    let post = |body: Value| async move {
        let response = post_pr(app, &f.owner.jwt, route, &body).await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    };
    let first = post(json!({"title":"First title"})).await;
    assert_eq!(
        first["data"]["pullRequest"]["providerStatus"], "failed",
        "{first}"
    );
    assert!(
        first["data"]["pullRequest"]["providerRepos"][0]["lastError"]
            .as_str()
            .unwrap()
            .contains("--title")
    );
    let persisted: String = sqlx::query_scalar(
        "SELECT last_error FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let event: String = sqlx::query_scalar("SELECT message FROM workspace_pull_request_events WHERE pull_request_id=$1 AND event_type='provider_failed' ORDER BY created_at DESC LIMIT 1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    for message in [
        &persisted,
        &event,
        first["data"]["pullRequest"]["providerRepos"][0]["lastError"]
            .as_str()
            .unwrap(),
    ] {
        assert!(message.contains("result is unknown"), "{message}");
        assert!(
            message.contains("inspect current state before retrying"),
            "{message}"
        );
        assert!(message.contains("long-create-request"), "{message}");
    }
    assert_eq!(
        creates.load(Ordering::SeqCst),
        1,
        "no automatic create replay"
    );
    post(json!({"title":"First title"})).await;
    assert_eq!(
        remote.lock().unwrap().as_ref().unwrap()["title"],
        "First title"
    );
    fail_patch.store(true, Ordering::SeqCst);
    let failed = post(json!({"title":"Second title"})).await;
    let row = &failed["data"]["pullRequest"]["providerRepos"][0];
    assert_eq!(row["providerStatus"], "failed");
    assert_eq!(row["providerPrNumber"], 42);
    assert_eq!(
        row["providerPrUrl"],
        format!("https://github.com/g5-test-{}/api/pull/42", f.org.id)
    );
    let stored: (Option<i64>, Option<String>, Option<String>) = sqlx::query_as("SELECT provider_pr_number,provider_pr_node_id,provider_pr_url FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(
        stored,
        (
            Some(42),
            Some("PR_saved42".into()),
            Some(format!(
                "https://github.com/g5-test-{}/api/pull/42",
                f.org.id
            ))
        )
    );
    assert!(row["lastError"].as_str().unwrap().contains("--title"));
    // Omission retries publication, but cannot silently replay failed title intent.
    post(json!({})).await;
    assert_eq!(
        remote.lock().unwrap().as_ref().unwrap()["title"],
        "First title"
    );
    post(json!({"title":"Second title"})).await;
    assert_eq!(
        remote.lock().unwrap().as_ref().unwrap()["title"],
        "Second title"
    );
    // A newer accepted title supersedes an older request across the local/provider
    // transaction boundary. Omitted updates do not reassert either title.
    sqlx::query("DELETE FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    fail_patch.store(true, Ordering::SeqCst);
    let discovered = post(json!({"title":"Superseded title"})).await;
    assert_eq!(
        discovered["data"]["pullRequest"]["providerRepos"][0]["providerPrNumber"], 42,
        "failed title change preserves identity even on first remote discovery"
    );
    post(json!({"title":"Latest title"})).await;
    let before = patches.load(Ordering::SeqCst);
    let mut stale = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    let rejected = app
        .state
        .workspace_pull_request_provider_lifecycle_service
        .create_or_update_provider_pull_requests(
            &mut stale,
            f.pull_request_id,
            f.owner.id,
            Some("Superseded title"),
        )
        .await
        .unwrap_err();
    assert!(rejected.to_string().contains("newer PR title"));
    stale.rollback().await.unwrap();
    assert_eq!(patches.load(Ordering::SeqCst), before);
    let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    app.state
        .workspace_pull_request_provider_lifecycle_service
        .create_or_update_provider_pull_requests(&mut db, f.pull_request_id, f.owner.id, None)
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert_eq!(patches.load(Ordering::SeqCst), before);
    assert_eq!(
        remote.lock().unwrap().as_ref().unwrap()["title"],
        "Latest title"
    );
    // Completed metadata does not force later external edits back on omission.
    remote.lock().unwrap().as_mut().unwrap()["title"] = json!("External title");
    post(json!({})).await;
    assert_eq!(patches.load(Ordering::SeqCst), before);
    assert_eq!(
        remote.lock().unwrap().as_ref().unwrap()["title"],
        "External title"
    );
    // A's PATCH stays in flight while B reaches its actual local title UPDATE.
    // Without the PR-row fence B completes early; the shared workspace read lock
    // alone cannot serialize these public-user requests.
    let first = post(json!({"title":"Concurrent A"}));
    let second = async {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !delayed_patch_started.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("first provider title PATCH reached its delay");
        let request = post(json!({"title":"Concurrent B"}));
        tokio::pin!(request);
        let wait_for_lock = async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND query LIKE 'UPDATE workspace_pull_requests SET title=%' AND wait_event_type='Lock')")
                    .fetch_one(&app.db).await.unwrap();
                if waiting {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        };
        tokio::select! {
            _ = &mut request => panic!("new title acceptance bypassed the in-flight provider title fence"),
            result = tokio::time::timeout(std::time::Duration::from_secs(3), wait_for_lock) => result.expect("second request waits for the PR row"),
        }
        request.await
    };
    let (_, concurrent) = tokio::join!(first, second);
    assert_eq!(concurrent["data"]["pullRequest"]["title"], "Concurrent B");
    assert_eq!(
        remote.lock().unwrap().as_ref().unwrap()["title"],
        "Concurrent B"
    );
    let before = patches.load(Ordering::SeqCst);
    // Actual member histories now match at a new commit while another member
    // still differs. A title-less Update PR must repair the existing provider
    // pins even though creating a new empty provider PR would be invalid.
    let included_head = commit(app, f.source.repo_id, "feature", "included-new-commit.txt").await;
    helpers::seed_branch_ref(app, f.source.repo_id, "main", &included_head).await;
    remote.lock().unwrap().as_mut().unwrap()["head"]["sha"] = json!(included_head);
    commit(app, f.other_repo_id, "feature", "still-outgoing.txt").await;
    let retried = post(json!({})).await;
    let pr = &retried["data"]["pullRequest"];
    assert_eq!(pr["id"], f.pull_request_id.to_string());
    assert_eq!(pr["providerRepos"][0]["sourceHeadSha"], included_head);
    assert_eq!(pr["providerRepos"][0]["providerStatus"], "open");
    assert_eq!(patches.load(Ordering::SeqCst), before);
    let included = post(json!({"title":"Included member title"})).await;
    let preflight = &included["data"]["pullRequest"]["preflight"]["repoResults"];
    let member = preflight
        .as_array()
        .unwrap()
        .iter()
        .find(|repo| repo["repoId"] == f.source.repo_id.to_string())
        .unwrap();
    assert_eq!(member["sourceHead"], member["targetHead"], "{included}");
    assert_eq!(
        remote.lock().unwrap().as_ref().unwrap()["title"],
        "Included member title"
    );
    assert_eq!(
        creates.load(Ordering::SeqCst),
        2,
        "no empty provider PR is created"
    );
    assert_eq!(patches.load(Ordering::SeqCst), before + 1);
    provider.github.verify().await;
    provider.app.cleanup().await;
}

// Most provider fixtures use an intentionally stale display identity to test
// publication repair. Scenarios merging the canonical clone repository must
// repair both the repository metadata and its provider PR record.
async fn use_fixture_clone_identity(app: &TestApp, f: &Fixture) {
    let name = format!("g5-test-{}/api", f.org.id);
    sqlx::query("UPDATE repos SET provider_full_name=$2 WHERE id=$1")
        .bind(f.source.repo_id)
        .bind(&name)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_repo_full_name=$2 WHERE pull_request_id=$1")
        .bind(f.pull_request_id).bind(&name).execute(&app.db).await.unwrap();
}

async fn mount_published_refs(provider: &ProviderFixture) {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let remote = provider.remote.path().join("remote.git");
    Mock::given(method("GET"))
        .and(path_regex("/repos/.*/api/git/ref/heads/.*"))
        .respond_with(move |request: &wiremock::Request| {
            let branch = request.url.path().rsplit('/').next().unwrap();
            let repo = git2::Repository::open_bare(&remote).unwrap();
            match repo.refname_to_id(&format!("refs/heads/{branch}")) {
                Ok(sha) => ResponseTemplate::new(200)
                    .set_body_json(json!({"object":{"sha":sha.to_string()}})),
                Err(_) => ResponseTemplate::new(404),
            }
        })
        .mount(&provider.github)
        .await;
}

/// `provider_failed` history of a PR, oldest first.
async fn refresh_failures(app: &TestApp, pull_request_id: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT message FROM workspace_pull_request_events
         WHERE pull_request_id=$1 AND event_type='provider_failed' ORDER BY created_at, id",
    )
    .bind(pull_request_id)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

/// The source workspace's Activity entries for publication failures.
async fn activity_failures(app: &TestApp, f: &Fixture) -> Vec<Value> {
    let response = app
        .authenticated_client(&f.owner)
        .get(&format!("/api/v1/workspaces/{}/activity", f.source.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    body["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["kind"] == "pull_request" && entry["eventType"] == "provider_failed")
        .cloned()
        .collect()
}

/// Issue #519: a pushed commit that has not reached GitHub yet is a settling
/// state. The retrying scan keeps it out of the PR's history until it outlasts
/// its budget, then reports it once with its reason; the next completed
/// publication resolves it in Activity. A single attempt reports it directly.
#[tokio::test]
async fn automatic_pr_refresh_reports_a_settling_push_only_after_its_budget() {
    const NAME: &str = "automatic_pr_refresh_reports_a_settling_push_only_after_its_budget";
    if isolated_provider_test(NAME) {
        return;
    }
    use g5_api::maintenance::workspace_pull_requests::run_once_tracked;
    use g5_api::services::workspace_pull_request_create::{
        SettlingRefreshes, PUSH_REFRESH_TIMEOUT,
    };
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    provider.github.reset().await;
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let remote_path = provider.remote.path().join("remote.git");
    let path = remote_path.clone();
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls(/21)?$"))
        .respond_with(move |request: &wiremock::Request| {
            let repo = git2::Repository::open_bare(&path).unwrap();
            let source = repo.refname_to_id("refs/heads/feature").unwrap().to_string();
            let target = repo.refname_to_id("refs/heads/upstream").unwrap().to_string();
            let pull = json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),"state":"open","merged":false,"head":{"sha":source},"base":{"sha":target}});
            ResponseTemplate::new(200).set_body_json(if request.url.path().ends_with("/21") {pull} else {json!([pull])})
        }).mount(&provider.github).await;
    let mirror = app.repo_path(f.source.repo_id).await;
    let push = || {
        git(
            &mirror,
            &["push", remote_path.to_str().unwrap(), "feature:feature"],
        )
    };
    let budget = std::time::Duration::from_secs(2);
    let mut settling = SettlingRefreshes::new(budget);

    let new_head = commit(app, f.source.repo_id, "feature", "settling.txt").await;
    // Within the budget: nothing reaches history, the PR or Activity.
    assert_eq!(
        run_once_tracked(&app.state, None, PUSH_REFRESH_TIMEOUT, &mut settling)
            .await
            .unwrap(),
        0
    );
    assert!(refresh_failures(app, f.pull_request_id).await.is_empty());
    assert!(read_pr_summary(app, f).await["blockingMessage"].is_null());
    assert!(activity_failures(app, f).await.is_empty());

    // Outlasting it: reported once, with where it stopped and why.
    tokio::time::sleep(budget).await;
    for _ in 0..2 {
        run_once_tracked(&app.state, None, PUSH_REFRESH_TIMEOUT, &mut settling)
            .await
            .unwrap();
    }
    let reported = refresh_failures(app, f.pull_request_id).await;
    assert_eq!(
        reported.len(),
        1,
        "a persisting state is reported once: {reported:?}"
    );
    assert!(
        reported[0].contains("has not completed for over 1 min while synchronizing with GitHub: Waiting for the pushed source commit to reach the remote branch"),
        "{reported:?}"
    );
    assert!(reported[0].contains("g5d pr update"), "{reported:?}");
    let shown = activity_failures(app, f).await;
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0]["message"], json!(reported[0]));
    assert_eq!(shown[0]["failureCount"], json!(1));

    // The push reaches GitHub: the refresh completes and resolves the failure.
    push();
    assert_eq!(
        run_once_tracked(&app.state, None, PUSH_REFRESH_TIMEOUT, &mut settling)
            .await
            .unwrap(),
        1
    );
    let recovered = read_pr_summary(app, f).await;
    assert_eq!(recovered["providerRepos"][0]["sourceHeadSha"], new_head);
    assert!(recovered["blockingMessage"].is_null(), "{recovered}");
    assert!(activity_failures(app, f).await.is_empty());

    // The completion restarted the budget: the next push settles silently again.
    commit(app, f.source.repo_id, "feature", "settling-again.txt").await;
    run_once_tracked(&app.state, None, PUSH_REFRESH_TIMEOUT, &mut settling)
        .await
        .unwrap();
    assert_eq!(refresh_failures(app, f.pull_request_id).await.len(), 1);

    // One attempt has no retry left: it reports the state with its reason.
    let error = g5_api::services::workspace_pull_request_create::refresh_after_push(
        &app.state,
        f.org.id,
        f.pull_request_id,
    )
    .await
    .unwrap_err();
    assert!(g5_api::services::workspace_pull_request_create::is_settling(&error.inner));
    let reported = refresh_failures(app, f.pull_request_id).await;
    assert_eq!(reported.len(), 2, "{reported:?}");
    assert!(
        reported[1].contains("failed while synchronizing with GitHub (CONFLICT): Waiting for the pushed source commit to reach the remote branch"),
        "{reported:?}"
    );
    let shown = activity_failures(app, f).await;
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(
        shown[0]["failureCount"],
        json!(1),
        "the completion resolved the earlier one"
    );
    provider.app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_waits_for_remote_push_recovers_and_keeps_merge_checks() {
    const NAME: &str = "automatic_pr_refresh_waits_for_remote_push_recovers_and_keeps_merge_checks";
    if isolated_provider_test(NAME) {
        return;
    }
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    provider.github.reset().await;
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let remote_path = provider.remote.path().join("remote.git");
    let fail_read = Arc::new(AtomicBool::new(true));
    let fail = fail_read.clone();
    let delay = Arc::new(AtomicBool::new(false));
    let delayed = Arc::new(AtomicBool::new(false));
    let delay_read = delay.clone();
    let read_started = delayed.clone();
    let path = remote_path.clone();
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls(/21)?$"))
        .respond_with(move |request: &wiremock::Request| {
            if fail.swap(false, Ordering::SeqCst) {
                return ResponseTemplate::new(503).set_body_json(json!({"message":"temporary failure"}));
            }
            let repo = git2::Repository::open_bare(&path).unwrap();
            let source = repo.refname_to_id("refs/heads/feature").unwrap().to_string();
            let target = repo.refname_to_id("refs/heads/upstream").unwrap().to_string();
            let pull = json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),"state":if source==target {"closed"} else {"open"},"merged":source==target,"merge_commit_sha":if source==target {Some(source.clone())} else {None},"head":{"sha":source},"base":{"sha":target}});
            let response=ResponseTemplate::new(200).set_body_json(if request.url.path().ends_with("/21") {pull} else {json!([pull])});
            if delay_read.swap(false,Ordering::SeqCst) {
                read_started.store(true,Ordering::SeqCst);
                response.set_delay(std::time::Duration::from_secs(3))
            } else { response }
        }).mount(&provider.github).await;
    let path = remote_path.clone();
    Mock::given(method("PUT"))
        .and(path_regex("/repos/.*/api/pulls/21/merge$"))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = request.body_json().unwrap();
            let repo = git2::Repository::open_bare(&path).unwrap();
            let source = repo.refname_to_id("refs/heads/feature").unwrap();
            assert_eq!(
                body["sha"],
                source.to_string(),
                "merge must carry the refreshed exact SHA"
            );
            repo.reference("refs/heads/upstream", source, true, "provider accepted")
                .unwrap();
            ResponseTemplate::new(200)
                .set_body_json(json!({"merged":true,"sha":source.to_string()}))
        })
        .expect(1)
        .mount(&provider.github)
        .await;
    let new_head = commit(app, f.source.repo_id, "feature", "after-pr.txt").await;
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .is_err()
    );
    let failed = read_pr_summary(app, f).await;
    assert!(failed["blockingMessage"]
        .as_str()
        .unwrap()
        .contains("g5d pr update"));
    assert!(failed["sourceHeads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == f.source_head));
    // Refresh only local evidence through the target's real Refresh action. It
    // must not claim that provider publication recovered or erase the retry hint.
    let response = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/pull-requests/{}/refresh",
                f.base_id, f.pull_request_id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let locally_refreshed = read_pr_summary(app, f).await;
    assert_eq!(
        locally_refreshed["blockingMessage"],
        failed["blockingMessage"]
    );
    assert_eq!(
        locally_refreshed["providerRepos"][0]["sourceHeadSha"],
        f.source_head
    );
    // A successful internal push does not override remote manual/auto policy.
    g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
        .await
        .unwrap();
    let remote = git2::Repository::open_bare(&remote_path).unwrap();
    assert_eq!(
        remote
            .refname_to_id("refs/heads/feature")
            .unwrap()
            .to_string(),
        f.source_head
    );
    drop(remote);
    let row: (Option<i64>, String) = sqlx::query_as("SELECT provider_pr_number,provider_status FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(
        row,
        (Some(21), "open".into()),
        "unconfirmed publication preserves the last provider version"
    );
    let mirror = app.repo_path(f.source.repo_id).await;
    git(
        &mirror,
        &["push", remote_path.to_str().unwrap(), "feature:feature"],
    );
    // No event delivery: periodic/startup discovery must finish persisted work.
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        1
    );
    let recovered = read_pr_summary(app, f).await;
    assert_eq!(recovered["providerRepos"][0]["sourceHeadSha"], new_head);
    assert!(recovered["blockingMessage"].is_null(), "{recovered}");
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        0,
        "no-op and old hints do not repeat provider work"
    );
    let latest = commit(app, f.source.repo_id, "feature", "raced-before-merge.txt").await;
    let (status, _) = f.ship(app).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "merge still refuses a changed source"
    );
    git(
        &mirror,
        &["push", remote_path.to_str().unwrap(), "feature:feature"],
    );
    let refresh = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/pull-requests/{}/refresh",
                f.base_id, f.pull_request_id
            ),
            &json!({}),
        )
        .await;
    let status = refresh.status();
    let body: Value = refresh.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "local preflight: {body}");
    let local = &body["data"]["pullRequest"];
    assert_ne!(local["providerRepos"][0]["sourceHeadSha"], latest);
    // Recorded provider pins independently keep discovery alive after a local refresh.
    delay.store(true, Ordering::SeqCst);
    let state = app.state.clone();
    let org = f.org.id;
    let id = f.pull_request_id;
    let auto = tokio::spawn(async move {
        g5_api::services::workspace_pull_request_create::refresh_after_push(&state, org, id).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !delayed.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // The real target Refresh holds its owner SHARE lock while waiting for
    // automatic refresh's PR lock. Neither may upgrade a workspace underneath it.
    let local_refresh = async {
        let response = app
            .authenticated_client(&f.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{}/refresh",
                    f.base_id, id
                ),
                &json!({}),
            )
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "overlapping local refresh: {body}");
    };
    let observe_wait = async {
        let began = std::time::Instant::now();
        loop {
            if began.elapsed() > std::time::Duration::from_secs(7) {
                let activity:Vec<(String,Option<String>)>=sqlx::query_as("SELECT query,wait_event_type FROM pg_stat_activity WHERE datname=current_database() AND state='active'").fetch_all(&app.db).await.unwrap();
                panic!("Expected blocked public Refresh query: {activity:?}");
            }
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%FOR UPDATE OF pr%')")
                .fetch_one(&app.db).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(local_refresh, observe_wait);
        assert!(auto.await.unwrap().unwrap());
    })
    .await
    .expect("automatic and manual refresh must not deadlock");
    let (status, response) = f.ship(app).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "merge after push without Update PR: {response}"
    );
    assert_eq!(head(app, f.source.repo_id, "main").await, latest);
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        0,
        "merged PR stays terminal"
    );
    provider.app.cleanup().await;
}

/// Issue #551: GitHub fixes `base.sha` when a PR is opened and never refreshes
/// it on head pushes. Mounts a provider whose PR keeps that creation-time base
/// while its head follows the remote source branch; the merge records a real
/// merge commit onto the remote target. `base_ref` is the PR's base branch.
/// Returns the accepted merge commit.
async fn mount_creation_time_base_pull(
    provider: &ProviderFixture,
    base_ref: &'static str,
    rebaseable: Value,
) -> std::sync::Arc<std::sync::Mutex<Option<String>>> {
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let org_id = provider.fixture.org.id;
    let creation_base = provider.fixture.base_head.clone();
    let accepted = Arc::new(Mutex::new(None::<String>));
    let remote_path = provider.remote.path().join("remote.git");
    let (path, merged) = (remote_path.clone(), accepted.clone());
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls(/21)?$"))
        .respond_with(move |request: &wiremock::Request| {
            let repo = git2::Repository::open_bare(&path).unwrap();
            let source = repo.refname_to_id("refs/heads/feature").unwrap().to_string();
            let merge = merged.lock().unwrap().clone();
            let pull = json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),"state":if merge.is_some() {"closed"} else {"open"},"merged":merge.is_some(),"merge_commit_sha":merge,"head":{"sha":source},"base":{"ref":base_ref,"sha":creation_base},"rebaseable":rebaseable.clone()});
            ResponseTemplate::new(200).set_body_json(if request.url.path().ends_with("/21") {pull} else {json!([pull])})
        }).mount(&provider.github).await;
    let (path, merged) = (remote_path, accepted.clone());
    Mock::given(method("PUT"))
        .and(path_regex("/repos/.*/api/pulls/21/merge$"))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = request.body_json().unwrap();
            let repo = git2::Repository::open_bare(&path).unwrap();
            let head = repo
                .find_commit(body["sha"].as_str().unwrap().parse().unwrap())
                .unwrap();
            let base = repo
                .find_commit(repo.refname_to_id("refs/heads/upstream").unwrap())
                .unwrap();
            let tree = repo
                .merge_commits(&base, &head, None)
                .unwrap()
                .write_tree_to(&repo)
                .unwrap();
            let signature = git2::Signature::now("Provider", "provider@example.test").unwrap();
            let merge = repo
                .commit(
                    Some("refs/heads/upstream"),
                    &signature,
                    &signature,
                    "Merge pull request #21",
                    &repo.find_tree(tree).unwrap(),
                    &[&base, &head],
                )
                .unwrap()
                .to_string();
            *merged.lock().unwrap() = Some(merge.clone());
            ResponseTemplate::new(200).set_body_json(json!({"merged":true,"sha":merge}))
        })
        .mount(&provider.github)
        .await;
    accepted
}

/// Issue #551: the target moved, the source was pushed and the automatic
/// refresh repinned the target; GitHub still reports the PR's creation-time
/// base. Merge to Base must accept on the live target tip, not refuse on the
/// stale `base.sha`.
#[tokio::test]
async fn provider_merge_accepts_a_pr_republished_after_the_target_moved() {
    const NAME: &str = "provider_merge_accepts_a_pr_republished_after_the_target_moved";
    if isolated_provider_test(NAME) {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let accepted = mount_creation_time_base_pull(&provider, "upstream", Value::Null).await;
    let remote_path = provider.remote.path().join("remote.git");
    let mirror = app.repo_path(f.source.repo_id).await;

    // The target advances on GitHub and G5's Base imports it.
    let moved_target = commit(app, f.source.repo_id, "main", "target-moved.txt").await;
    GitRepo::open(&mirror)
        .unwrap()
        .write_baseline_ref("main", &moved_target)
        .unwrap();
    git(
        &mirror,
        &["push", remote_path.to_str().unwrap(), "main:upstream"],
    );
    // A source push reaches GitHub; the automatic refresh republishes the PR.
    let new_source = commit(app, f.source.repo_id, "feature", "after-target-moved.txt").await;
    git(
        &mirror,
        &["push", remote_path.to_str().unwrap(), "feature:feature"],
    );
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        1
    );
    let republished = read_pr_summary(app, f).await;
    let repo = &republished["providerRepos"][0];
    assert_eq!(repo["sourceHeadSha"], new_source, "{republished}");
    assert_eq!(repo["targetHeadSha"], moved_target, "{republished}");
    assert_eq!(
        repo["providerBaseSha"], f.base_head,
        "GitHub's creation-time base stays observable as divergence: {republished}"
    );

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let merge = accepted.lock().unwrap().clone().expect("provider merged");
    assert_eq!(head(app, f.source.repo_id, "main").await, merge);
    assert_eq!(body["data"]["pullRequest"]["status"], "merged", "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["providerStatus"], "merged",
        "{body}"
    );
    provider.app.cleanup().await;
}

/// Issue #582: the target recorded at publication goes stale whenever Base
/// moves. Merge to Base verifies GitHub against the Base head its own fresh
/// preflight classified, so it merges without republication and records the
/// target it merged onto.
#[tokio::test]
async fn provider_merge_accepts_a_target_that_moved_after_the_last_publication() {
    const NAME: &str = "provider_merge_accepts_a_target_that_moved_after_the_last_publication";
    if isolated_provider_test(NAME) {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let accepted = mount_creation_time_base_pull(&provider, "upstream", Value::Null).await;
    let remote_path = provider.remote.path().join("remote.git");
    let mirror = app.repo_path(f.source.repo_id).await;

    // The target advances on GitHub and Base imports it; nothing republishes.
    let moved_target = commit(app, f.source.repo_id, "main", "target-moved.txt").await;
    GitRepo::open(&mirror)
        .unwrap()
        .write_baseline_ref("main", &moved_target)
        .unwrap();
    git(
        &mirror,
        &["push", remote_path.to_str().unwrap(), "main:upstream"],
    );
    let stale = read_pr_summary(app, f).await;
    assert_eq!(
        stale["providerRepos"][0]["targetHeadSha"], f.base_head,
        "{stale}"
    );

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let merge = accepted.lock().unwrap().clone().expect("provider merged");
    assert_eq!(head(app, f.source.repo_id, "main").await, merge);
    assert_eq!(body["data"]["pullRequest"]["status"], "merged", "{body}");
    let recorded: String = sqlx::query_scalar(
        "SELECT target_head_sha FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        recorded, moved_target,
        "records the target the provider merged onto"
    );
    provider.app.cleanup().await;
}

/// Issue #551 and #582: GitHub's target moved but Base has not imported it
/// yet. Merging would let GitHub rebase onto history G5 never checked, so the
/// merge refuses for retry before any provider merge.
#[tokio::test]
async fn provider_merge_waits_for_the_target_to_import_a_provider_target_move() {
    const NAME: &str = "provider_merge_waits_for_the_target_to_import_a_provider_target_move";
    if isolated_provider_test(NAME) {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    let full_name = format!("g5-test-{}/api", f.org.id);
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let accepted = mount_creation_time_base_pull(&provider, "upstream", Value::Null).await;
    let remote = git2::Repository::open_bare(provider.remote.path().join("remote.git")).unwrap();
    let parent = remote
        .find_commit(remote.refname_to_id("refs/heads/upstream").unwrap())
        .unwrap();
    let signature = git2::Signature::now("Outside", "outside@example.test").unwrap();
    let moved = remote
        .commit(
            Some("refs/heads/upstream"),
            &signature,
            &signature,
            "merged outside G5",
            &parent.tree().unwrap(),
            &[&parent],
        )
        .unwrap()
        .to_string();
    drop(parent);
    drop(remote);

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let message = body.to_string();
    assert!(
        message.contains(&format!(
            "{full_name}: Conflict: GitHub upstream is at {moved}, but main is at {}: main hasn't pulled the latest upstream yet. It pulls automatically, usually within a minute; merge again after that, or fetch on main.",
            f.base_head
        )),
        "{body}"
    );
    assert!(message.contains("The target was not advanced"), "{body}");
    assert!(accepted.lock().unwrap().is_none(), "no provider merge");
    assert!(!provider
        .github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|request| request.method == "PUT" && request.url.path().ends_with("/merge")));
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    let summary = read_pr_summary(app, f).await;
    assert_eq!(summary["status"], "open", "{summary}");
    assert_eq!(summary["providerRepos"][0]["providerStatus"], "failed");
    provider.app.cleanup().await;
}

/// Dropping the `base.sha` comparison must not let a PR retargeted on GitHub
/// merge into another branch while the pinned target's tip still matches.
#[tokio::test]
async fn provider_merge_refuses_a_pr_retargeted_on_github() {
    const NAME: &str = "provider_merge_refuses_a_pr_retargeted_on_github";
    if isolated_provider_test(NAME) {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    let full_name = format!("g5-test-{}/api", f.org.id);
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let accepted = mount_creation_time_base_pull(&provider, "release", Value::Null).await;

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains(&format!(
            "{full_name}: Conflict: GitHub PR #21 targets release, expected upstream."
        )),
        "{body}"
    );
    assert!(accepted.lock().unwrap().is_none(), "no provider merge");
    assert!(!provider
        .github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|request| request.method == "PUT" && request.url.path().ends_with("/merge")));
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    let summary = read_pr_summary(app, f).await;
    assert_eq!(summary["status"], "open", "{summary}");
    assert_eq!(summary["providerRepos"][0]["providerStatus"], "failed");
    provider.app.cleanup().await;
}

/// Issue #582: GitHub can't rebase a branch carrying a merge commit, and Merge
/// to Base always rebases. With `rebaseable: false` the preflight refuses
/// before the merge endpoint and names the merge commit and the fix.
#[tokio::test]
async fn provider_merge_explains_an_unrebaseable_branch_before_merging() {
    const NAME: &str = "provider_merge_explains_an_unrebaseable_branch_before_merging";
    if isolated_provider_test(NAME) {
        return;
    }
    let provider = ProviderFixture::create_with_history(false, false, false, true).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    let full_name = format!("g5-test-{}/api", f.org.id);
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let accepted = mount_creation_time_base_pull(&provider, "upstream", json!(false)).await;

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let message = body.to_string();
    assert!(
        message.contains(&format!(
            "{full_name}: Conflict: GitHub can't rebase PR #21 onto upstream: the branch contains merge commit {} (\\\"Merge side into feature\\\"), which GitHub can't rebase. \\\"Get the latest changes from main\\\" can't remove it because this workspace already has them; ask the workspace's agent to rebase its branch onto main without merge commits, then merge again once the PR shows the new commits (with Auto-push off, publish them first with Update PR or g5d pr update).",
            &f.source_head[..7]
        )),
        "{body}"
    );
    assert!(!message.contains("Provider merge"), "{body}");
    assert!(accepted.lock().unwrap().is_none(), "no provider merge");
    assert!(!provider
        .github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|request| request.method == "PUT" && request.url.path().ends_with("/merge")));
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    let summary = read_pr_summary(app, f).await;
    assert_eq!(summary["status"], "open", "{summary}");
    assert_eq!(summary["providerRepos"][0]["providerStatus"], "failed");
    provider.app.cleanup().await;
}

/// Issue #582: `rebaseable` is null while GitHub computes it. The merge
/// endpoint's own 405 refusal gets a plain explanation instead of the raw
/// provider diagnostics.
#[tokio::test]
async fn provider_merge_explains_github_refusing_to_rebase() {
    const NAME: &str = "provider_merge_explains_github_refusing_to_rebase";
    if isolated_provider_test(NAME) {
        return;
    }
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create_with_history(false, false, false, true).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    let full_name = format!("g5-test-{}/api", f.org.id);
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let accepted = mount_creation_time_base_pull(&provider, "upstream", Value::Null).await;
    Mock::given(method("PUT"))
        .and(path_regex("/repos/.*/api/pulls/21/merge$"))
        .respond_with(
            ResponseTemplate::new(405)
                .insert_header("x-github-request-id", "refused-rebase")
                .set_body_json(json!({"message":"This branch can't be rebased"})),
        )
        .with_priority(1)
        .mount(&provider.github)
        .await;

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let message = body.to_string();
    assert!(
        message.contains(&format!(
            "{full_name}: Conflict: GitHub refused to merge PR #21 into upstream (GitHub: This branch can't be rebased): the branch contains merge commit {} (\\\"Merge side into feature\\\"), which GitHub can't rebase.",
            &f.source_head[..7]
        )),
        "{body}"
    );
    assert!(!message.contains("HTTP 405"), "{body}");
    assert!(!message.contains("refused-rebase"), "{body}");
    assert!(accepted.lock().unwrap().is_none(), "no provider merge");
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    let summary = read_pr_summary(app, f).await;
    assert_eq!(summary["status"], "open", "{summary}");
    assert_eq!(summary["providerRepos"][0]["providerStatus"], "failed");
    provider.app.cleanup().await;
}

/// Issue #582: a branch proven linear on the pinned target makes a
/// `rebaseable: false` stale; GitHub's merge endpoint decides.
#[tokio::test]
async fn provider_merge_ignores_a_stale_unrebaseable_flag_on_a_linear_branch() {
    const NAME: &str = "provider_merge_ignores_a_stale_unrebaseable_flag_on_a_linear_branch";
    if isolated_provider_test(NAME) {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let accepted = mount_creation_time_base_pull(&provider, "upstream", json!(false)).await;

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let merge = accepted.lock().unwrap().clone().expect("provider merged");
    assert_eq!(head(app, f.source.repo_id, "main").await, merge);
    assert_eq!(body["data"]["pullRequest"]["status"], "merged", "{body}");
    provider.app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_skips_deleted_targets_and_closed_or_absent_prs() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    // Ordinary members must retain read access to the selected target.
    TestUser::create_admin(&app, f.org.id).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let other = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
        .bind(f.org.id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2,deleted_at=now() WHERE id=$1")
        .bind(f.base_id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    let latest = commit(&app, f.source.repo_id, "feature", "deleted-target.txt").await;
    assert!(
        !g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .unwrap()
    );
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(heads
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == f.source_head));
    sqlx::query("UPDATE workspaces SET deleted_at=NULL WHERE id=$1")
        .bind(f.base_id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .unwrap()
    );
    let response = app
        .authenticated_client(&f.owner)
        .get(&format!(
            "/api/v1/workspaces/{}/pull-requests?role=source",
            f.source.id
        ))
        .await;
    let body: Value = response.json().await.unwrap();
    assert!(body["data"][0]["sourceHeads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == latest));
    sqlx::query("UPDATE workspace_pull_requests SET status='closed' WHERE id=$1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    commit(&app, f.source.repo_id, "feature", "after-close.txt").await;
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        0
    );
    remove_fixture_pr(&app, &f).await;
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        0
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0, "push cannot implicitly create a G5 PR");
    app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_uses_published_members_and_leaves_approval_version_unchanged() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let client = app.authenticated_client(&f.owner);
    let (definition_id, reviewer) =
        attach_phase_requirement(&app, &f.owner, f.org.id, f.base_id, "Sign off", &["ie"]).await;
    let approved = decide_outcome(
        &app,
        &f.owner,
        &reviewer,
        f.source.id,
        &definition_id,
        "ie",
        "approved",
    )
    .await;
    assert_eq!(
        approved["data"]["outgoing"]["requirements"][0]["state"],
        "approved"
    );
    let reviewed: Value = sqlx::query_scalar(
        "SELECT reviewed_git_heads FROM approval_statuses WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let pushed = commit(&app, f.source.repo_id, "feature", "pushed.txt").await;
    let sibling = commit(&app, f.other_repo_id, "feature", "sibling-pushed.txt").await;
    helpers::seed_branch_ref(&app, f.source.repo_id, "dirty-snapshot", &pushed).await;
    let dirty = commit(&app, f.source.repo_id, "dirty-snapshot", "unsaved.txt").await;
    let flush_id = Uuid::new_v4();
    let receipts = json!([
        {"repo_id":f.source.repo_id,"branch_oid":pushed,"local_branch_oid":pushed,"local_head_oid":pushed,"head_ref":"refs/heads/feature","sequencer":"none","snapshot_oid":dirty,"dirty":true,"dirty_paths":["unsaved.txt"]},
        {"repo_id":f.other_repo_id,"branch_oid":sibling,"local_branch_oid":sibling,"local_head_oid":sibling,"head_ref":"refs/heads/feature","sequencer":"none","snapshot_oid":sibling,"dirty":false,"dirty_paths":[]}
    ]);
    sqlx::query("INSERT INTO workspace_flushes(flush_id,org_id,workspace_id,origin,vm_id,epoch,reason,required_repo_ids,repos,complete_at,request_key) VALUES($1,$2,$3,'flush',$4,1,'shutdown',$5,$6,now(),'auto-pr-test')")
        .bind(flush_id).bind(f.org.id).bind(f.source.id).bind(Uuid::new_v4().to_string())
        .bind(vec![f.source.repo_id, f.other_repo_id]).bind(receipts).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,status,last_flush_id) VALUES($1,$2,'stopped',$3)")
        .bind(f.org.id).bind(f.source.id).bind(flush_id).execute(&app.db).await.unwrap();
    g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
        .await
        .unwrap();
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let members = heads.as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert!(members
        .iter()
        .any(|m| m["repoId"] == f.source.repo_id.to_string() && m["headCommit"] == pushed));
    assert!(members
        .iter()
        .any(|m| m["repoId"] == f.other_repo_id.to_string() && m["headCommit"] == sibling));
    assert!(
        !members.iter().any(|m| m["headCommit"] == dirty),
        "unpublished dirty snapshot is excluded"
    );
    let after: Value = sqlx::query_scalar(
        "SELECT reviewed_git_heads FROM approval_statuses WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        after, reviewed,
        "PR refresh never renews the human approval"
    );
    let response = client
        .get(&format!("/api/v1/workspaces/{}/approvals", f.source.id))
        .await;
    let state: Value = response.json().await.unwrap();
    // Under IGNORE the unrenewed approval keeps counting; the moved members
    // show only as changed currentness (g5.governance.approval.source-commit-policy).
    let requirement = &state["data"]["outgoing"]["requirements"][0];
    assert_eq!(requirement["state"], "approved", "{state}");
    assert_eq!(requirement["currentness"]["state"], "changed", "{state}");
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_pr_refresh_holds_membership_while_waiting_for_pr_lock() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let other = TestUser::create(&app).await;
    TestUser::create_admin(&app, f.org.id).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
        .bind(f.org.id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.base_id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    // Unchanged inaccessible rows should never acquire a fabricated obligation.
    assert!(
        !g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .unwrap()
    );
    let latest = commit(&app, f.source.repo_id, "feature", "membership-race.txt").await;
    let mut blocker = app.db.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
        .bind(f.pull_request_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let state = app.state.clone();
    let org = f.org.id;
    let id = f.pull_request_id;
    let refresh = tokio::spawn(async move {
        g5_api::services::workspace_pull_request_create::refresh_after_push(&state, org, id).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10),async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) AND query LIKE 'SELECT status FROM workspace_pull_requests%')")
                .bind(pid).fetch_one(&app.db).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.expect("automatic owner is waiting on the actual PR lock");
    assert_membership_revocation_waits(&app, f.org.id, f.owner.id).await;
    blocker.commit().await.unwrap();
    assert!(refresh.await.unwrap().unwrap());
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(heads
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == latest));
    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(org)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    commit(&app, f.source.repo_id, "feature", "after-revocation.txt").await;
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(&app.state, org, id)
            .await
            .is_err()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_checks_identity_included_members_and_terminal_pins() {
    const NAME: &str = "automatic_pr_refresh_checks_identity_included_members_and_terminal_pins";
    if isolated_provider_test(NAME) {
        return;
    }
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    provider.github.reset().await;
    let latest = commit(app, f.source.repo_id, "feature", "included.txt").await;
    // The fixture has a stale recorded full name. Refuse it before any HTTP.
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .is_err()
    );
    assert!(provider
        .github
        .received_requests()
        .await
        .unwrap()
        .is_empty());
    use_fixture_clone_identity(app, f).await;
    let mirror = app.repo_path(f.source.repo_id).await;
    git2::Repository::open(&mirror)
        .unwrap()
        .remote(
            "origin",
            &format!("https://github.com/g5-test-{}/api.git", f.org.id),
        )
        .unwrap();
    let remote = provider.remote.path().join("remote.git");
    git(
        &mirror,
        &[
            "push",
            remote.to_str().unwrap(),
            "feature:feature",
            "feature:upstream",
        ],
    );
    let repo = git2::Repository::open(&mirror).unwrap();
    repo.reference(
        "refs/heads/main",
        latest.parse().unwrap(),
        true,
        "already included",
    )
    .unwrap();
    drop(repo);
    commit(app, f.other_repo_id, "feature", "still-needs-merge.txt").await;
    mount_published_refs(&provider).await;
    let merged = Arc::new(AtomicBool::new(false));
    let state = merged.clone();
    let sha = latest.clone();
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls/21$"))
        .respond_with(move |_: &wiremock::Request| {
            let done=state.load(Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),
                "state":if done {"closed"} else {"open"},"merged":done,"head":{"sha":sha},"base":{"sha":sha},
                "merge_commit_sha":if done {Some(&sha)} else {None}}))
        }).mount(&provider.github).await;
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .unwrap()
    );
    let pins:String=sqlx::query_scalar("SELECT p.source_head_sha FROM workspace_pull_requests pr JOIN workspace_pull_request_provider_repos p ON p.pull_request_id=pr.id WHERE pr.id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(pins, latest.clone());
    assert!(
        provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method.as_str() == "GET"),
        "no new empty PR"
    );
    // Simulate external completion before G5 imports it. A later push must not
    // replace the provider's accepted source version or the stored G5 evidence.
    merged.store(true, Ordering::SeqCst);
    let next = commit(app, f.source.repo_id, "feature", "after-accepted.txt").await;
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .is_err()
    );
    let pins:(String,Value)=sqlx::query_as("SELECT p.source_head_sha,pr.source_heads FROM workspace_pull_requests pr JOIN workspace_pull_request_provider_repos p ON p.pull_request_id=pr.id WHERE pr.id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(pins.0, latest);
    assert!(!pins
        .1
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == next));
    // The member is now included at a newer commit while its provider PR was
    // accepted at the previous version. Both UI and VM Update must retain that
    // accepted provider proof, including after status is already recorded merged.
    helpers::seed_branch_ref(app, f.source.repo_id, "main", &next).await;
    let original: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT source_head_sha,target_head_sha,provider_head_sha,provider_base_sha
         FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let response = post_pr(
        app,
        &f.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
        &json!({}),
    )
    .await;
    let status = response.status();
    let response_body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{response_body}");
    let current: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT source_head_sha,target_head_sha,provider_head_sha,provider_base_sha
         FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        current, original,
        "discovered completion preserves accepted proof"
    );
    let error = read_pr_summary(app, f).await;
    assert!(error["providerRepos"][0]["lastError"]
        .as_str()
        .unwrap()
        .contains("merge recovery"));
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_status='merged' WHERE pull_request_id=$1")
        .bind(f.pull_request_id).execute(&app.db).await.unwrap();
    let peer = helpers::TestWorkspaceRuntimePeer::attach(app, &f.source, f.owner.id).await;
    let token = vm_pr_token(app, f, &peer);
    let response = post_pr(
        app,
        &token,
        &format!("/api/internal/workspaces/{}/pull-requests", f.source.id),
        &json!({}),
    )
    .await;
    let status = response.status();
    let response_body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{response_body}");
    let current: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT source_head_sha,target_head_sha,provider_head_sha,provider_base_sha
         FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        current, original,
        "known completion preserves accepted proof"
    );
    assert_eq!(
        read_pr_summary(app, f).await["providerRepos"][0]["providerStatus"],
        "merged"
    );
    assert!(
        provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method.as_str() == "GET"),
        "retries cannot create empty PRs or modify the completed PR"
    );
    provider.app.cleanup().await;
}

/// Issue #1016: a provider PR found closed on GitHub pauses automatic refresh.
/// Repeated scans report it once and read GitHub once; a GitHub reopen alone is
/// not seen, and an Update that observes the PR open resumes refresh.
#[tokio::test]
async fn automatic_pr_refresh_stops_reading_a_completed_provider_pr() {
    const NAME: &str = "automatic_pr_refresh_stops_reading_a_completed_provider_pr";
    if isolated_provider_test(NAME) {
        return;
    }
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    provider.github.reset().await;
    use_fixture_clone_identity(app, f).await;
    mount_published_refs(&provider).await;
    let remote_path = provider.remote.path().join("remote.git");
    let closed = Arc::new(AtomicBool::new(true));
    let state = closed.clone();
    let path = remote_path.clone();
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls(/21)?$"))
        .respond_with(move |request: &wiremock::Request| {
            let repo = git2::Repository::open_bare(&path).unwrap();
            let source = repo.refname_to_id("refs/heads/feature").unwrap().to_string();
            let target = repo.refname_to_id("refs/heads/upstream").unwrap().to_string();
            let open = !state.load(Ordering::SeqCst);
            let pull = json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),
                "state":if open {"open"} else {"closed"},"merged":false,"head":{"sha":source},"base":{"sha":target}});
            ResponseTemplate::new(200).set_body_json(if request.url.path().ends_with("/21") {pull} else if open {json!([pull])} else {json!([])})
        }).mount(&provider.github).await;
    let pull_reads = || async {
        provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method.as_str() == "GET" && r.url.path().ends_with("/pulls/21"))
            .count()
    };
    let marker = || async {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT provider_completed_pr_number FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
        )
        .bind(f.pull_request_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
    };
    // A source push the provider PR no longer accepts keeps refresh needed.
    commit(app, f.source.repo_id, "feature", "after-close.txt").await;
    for _ in 0..3 {
        assert_eq!(
            g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
                .await
                .unwrap(),
            0
        );
    }
    assert_eq!(
        pull_reads().await,
        1,
        "one GitHub read across repeated scans"
    );
    assert_eq!(marker().await, Some(21));
    let failures = refresh_failures(app, f.pull_request_id).await;
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].contains("Provider PR has completed"),
        "{failures:?}"
    );
    let row: String = sqlx::query_scalar(
        "SELECT provider_status FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        row, "open",
        "merge recovery's status is not decided by refresh"
    );

    // Reopened on GitHub: automatic refresh still makes no read of its own.
    closed.store(false, Ordering::SeqCst);
    g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
        .await
        .unwrap();
    assert_eq!(pull_reads().await, 1);

    // Update observes it open, records it and resumes automatic refresh.
    let mirror = app.repo_path(f.source.repo_id).await;
    git2::Repository::open(&mirror)
        .unwrap()
        .remote(
            "origin",
            &format!("https://github.com/g5-test-{}/api.git", f.org.id),
        )
        .unwrap();
    git(
        &mirror,
        &["push", remote_path.to_str().unwrap(), "feature:feature"],
    );
    let response = post_pr(
        app,
        &f.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
        &json!({}),
    )
    .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(pull_reads().await > 1);
    assert_eq!(marker().await, None, "{body}");
    commit(app, f.source.repo_id, "feature", "after-reopen.txt").await;
    let reads = pull_reads().await;
    g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
        .await
        .unwrap();
    assert!(
        pull_reads().await > reads,
        "refresh reads the open PR again"
    );
    provider.app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_discovers_missing_provider_after_local_refresh() {
    const NAME: &str = "automatic_pr_refresh_discovers_missing_provider_after_local_refresh";
    if isolated_provider_test(NAME) {
        return;
    }
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    provider.github.reset().await;
    sqlx::query("DELETE FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    helpers::seed_branch_ref(app, f.source.repo_id, "main", &f.source_head).await;
    let refresh_route = format!(
        "/api/v1/workspaces/{}/pull-requests/{}/refresh",
        f.base_id, f.pull_request_id
    );
    let response = app
        .authenticated_client(&f.owner)
        .post_json(&refresh_route, &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .unwrap(),
        "included member without a provider PR remains a no-op"
    );
    let latest = commit(app, f.source.repo_id, "feature", "new-provider-member.txt").await;
    let mirror = app.repo_path(f.source.repo_id).await;
    git(
        &mirror,
        &[
            "push",
            provider.remote.path().join("remote.git").to_str().unwrap(),
            "feature:feature",
        ],
    );
    mount_published_refs(&provider).await;
    let pull = json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),"state":"open","merged":false,"head":{"sha":latest},"base":{"sha":f.base_head}});
    Mock::given(method("GET"))
        .and(path_regex("/repos/.*/api/pulls$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([pull])))
        .expect(1)
        .mount(&provider.github)
        .await;
    let response = app
        .authenticated_client(&f.owner)
        .post_json(&refresh_route, &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let before = read_pr_summary(app, f).await;
    assert!(before["sourceHeads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["headCommit"] == latest));
    assert!(before["providerRepos"].as_array().unwrap().is_empty());
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .unwrap(),
        "local evidence does not hide a required missing provider outcome"
    );
    let after = read_pr_summary(app, f).await;
    assert_eq!(after["providerRepos"][0]["sourceHeadSha"], latest);
    assert_eq!(after["providerRepos"][0]["providerPrNumber"], 21);
    assert!(after["blockingMessage"].is_null(), "{after}");
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_bounds_remote_reads_and_visits_later_orgs() {
    const NAME: &str = "automatic_pr_refresh_bounds_remote_reads_and_visits_later_orgs";
    if isolated_provider_test(NAME) {
        return;
    }
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    // Org discovery is by creation time, so the unresponsive provider goes first.
    let later = Fixture::create(app, false).await;
    use_fixture_clone_identity(app, f).await;
    provider.github.reset().await;
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls/21$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),"state":"open","merged":false,"head":{"sha":f.source_head},"base":{"sha":f.base_head}})))
        .mount(&provider.github).await;
    Mock::given(method("GET"))
        .and(path_regex("/repos/.*/api/git/ref/heads/.*"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(90))
                .set_body_json(json!({"object":{"sha":f.source_head}})),
        )
        .mount(&provider.github)
        .await;
    commit(app, f.source.repo_id, "feature", "timeout.txt").await;
    let latest = commit(app, later.source.repo_id, "feature", "later-org.txt").await;
    let count = tokio::time::timeout(
        std::time::Duration::from_secs(40),
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None),
    )
    .await
    .expect("bounded provider wait")
    .unwrap();
    assert_eq!(count, 1, "first remains pending, later completes");
    let failed = read_pr_summary(app, f).await;
    assert!(failed["blockingMessage"]
        .as_str()
        .unwrap()
        .contains("g5d pr update"));
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(later.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(heads
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == latest));
    let cancel = tokio_util::sync::CancellationToken::new();
    let worker =
        g5_api::maintenance::workspace_pull_requests::spawn(app.state.clone(), cancel.clone());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), worker)
        .await
        .expect("cancellable provider I/O")
        .unwrap();
    provider.app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_never_repins_an_unconfirmed_remote_cut() {
    const NAME: &str = "automatic_pr_refresh_never_repins_an_unconfirmed_remote_cut";
    if isolated_provider_test(NAME) {
        return;
    }
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    provider.github.reset().await;
    use_fixture_clone_identity(app, f).await;
    let merged = Arc::new(AtomicBool::new(false));
    let done = merged.clone();
    let source = f.source_head.clone();
    let target = f.base_head.clone();
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls/21$"))
        .respond_with(move |_:&wiremock::Request| {
            let merged=done.load(Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),
                "state":if merged {"closed"} else {"open"},"merged":merged,"head":{"sha":source},"base":{"sha":target}}))
        }).mount(&provider.github).await;
    let done = merged.clone();
    let sha = f.source_head.clone();
    Mock::given(method("GET"))
        .and(path_regex("/repos/.*/api/git/ref/heads/feature$"))
        .respond_with(move |_: &wiremock::Request| {
            // Initial PR observation saw open A. An external actor accepts A
            // while our attempted source B has not reached the provider branch.
            done.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({"object":{"sha":sha}}))
        })
        .mount(&provider.github)
        .await;
    let next = commit(
        app,
        f.source.repo_id,
        "feature",
        "not-remotely-published.txt",
    )
    .await;
    for _ in 0..2 {
        assert!(
            g5_api::services::workspace_pull_request_create::refresh_after_push(
                &app.state,
                f.org.id,
                f.pull_request_id
            )
            .await
            .is_err()
        );
        let stored:(String,Value)=sqlx::query_as("SELECT p.source_head_sha,pr.source_heads FROM workspace_pull_requests pr JOIN workspace_pull_request_provider_repos p ON p.pull_request_id=pr.id WHERE pr.id=$1")
            .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(stored.0, f.source_head);
        assert!(!stored
            .1
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["headCommit"] == next));
        assert!(read_pr_summary(app, f).await["blockingMessage"]
            .as_str()
            .unwrap()
            .contains("g5d pr update"));
    }
    assert!(merged.load(Ordering::SeqCst));
    provider.app.cleanup().await;
}

#[tokio::test]
async fn automatic_pr_refresh_reuses_the_app_credential_scope() {
    const NAME: &str = "automatic_pr_refresh_reuses_the_app_credential_scope";
    if isolated_provider_test(NAME) {
        return;
    }
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create_with_app(false, false, true).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    let identity = Uuid::new_v4();
    let binding = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO github_app_identities(id,org_id,kind) VALUES($1,$2,'platform_managed')",
    )
    .bind(identity)
    .bind(f.org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO github_app_installation_bindings(id,org_id,app_identity_id,version,disposition,installation_id,account_id,account_login,permissions,provenance,created_by) VALUES($1,$2,$3,1,'active',555,44,'acme',$4,'coverage_change',$5)")
        .bind(binding).bind(f.org.id).bind(identity).bind(json!({"contents":"write","pull_requests":"write"})).bind(f.owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO github_app_installation_binding_repositories(org_id,binding_id,github_repo_id,full_name) VALUES($1,$2,1002,'acme/api')")
        .bind(f.org.id).bind(binding).execute(&app.db).await.unwrap();
    for permissions in [
        json!({"pull_requests":"read","contents":"read"}),
        json!({"pull_requests":"write","contents":"read"}),
        json!({"contents":"read"}),
    ] {
        Mock::given(method("POST")).and(path("/app/installations/555/access_tokens"))
            .and(body_json(json!({"repository_ids":[1002],"permissions":permissions})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token":"ghs_auto","expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()})))
            .expect(1).mount(&provider.github).await;
    }
    let latest = commit(app, f.source.repo_id, "feature", "app-push.txt").await;
    let mirror = app.repo_path(f.source.repo_id).await;
    git(
        &mirror,
        &[
            "push",
            provider.remote.path().join("remote.git").to_str().unwrap(),
            "feature:feature",
        ],
    );
    mount_published_refs(&provider).await;
    Mock::given(method("GET")).and(path("/repos/acme/api/pulls/21"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":21,"html_url":"https://github.com/acme/api/pull/21","state":"open","merged":false,"head":{"sha":latest},"base":{"sha":f.base_head}})))
        .mount(&provider.github).await;
    assert!(
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id
        )
        .await
        .unwrap()
    );
    let row:String=sqlx::query_scalar("SELECT p.source_head_sha FROM workspace_pull_requests pr JOIN workspace_pull_request_provider_repos p ON p.pull_request_id=pr.id WHERE pr.id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(row, latest);
    for request in provider
        .github
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "GET")
    {
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            "Bearer ghs_auto"
        );
    }
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vm_pr_update_rechecks_target_access_after_pr_wait() {
    assert_target_revocation_while_waiting_for_pr_lock(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ui_pr_update_rechecks_target_access_after_pr_wait() {
    assert_target_revocation_while_waiting_for_pr_lock(false).await;
}

async fn assert_target_revocation_while_waiting_for_pr_lock(vm: bool) {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach(&app, &f.source, f.owner.id).await;
    let token = if vm {
        vm_pr_token(&app, &f, &peer)
    } else {
        f.owner.jwt.clone()
    };
    let other = TestUser::create(&app).await;
    TestUser::create_admin(&app, f.org.id).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
        .bind(f.org.id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.base_id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut blocker = app.db.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
        .bind(f.pull_request_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let route = format!(
        "/api/{}/workspaces/{}/pull-requests",
        if vm { "internal" } else { "v1" },
        f.source.id
    );
    let request = json!({"targetWorkspaceId": f.base_id, "title": "Accepted title", "description": "Accepted description"});
    let execute = post_pr(&app, &token, &route, &request);
    let revoke = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))")
                    .bind(pid).fetch_one(&app.db).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("VM create reached the actual PR row lock");
        assert_membership_revocation_waits(&app, f.org.id, f.owner.id).await;
        blocker.commit().await.unwrap();
    };
    let (response, _) = tokio::join!(execute, revoke);
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    let persisted: Value =
        sqlx::query_scalar("SELECT to_jsonb(pr) FROM workspace_pull_requests pr WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(persisted["title"], "Accepted title");
    // Once the admitted write finishes, revocation prevents the next request.
    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(!post_pr(&app, &token, &route, &request)
        .await
        .status()
        .is_success());
    let providers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(providers, 0);
    // Restore active membership: an ordinary retry must still succeed.
    sqlx::query("UPDATE org_memberships SET deactivated_at=NULL WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let response = post_pr(&app, &token, &route, &request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pr_title_update_refuses_a_concurrent_close() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let mut closing = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    g5_api::services::workspace_mutation_policy::ensure_workspace_owner(&mut closing, f.source.id)
        .await
        .unwrap();
    let closed = app
        .state
        .coordinated_repo_publication_service
        .close_workspace_pull_request(&mut closing, f.source.id, f.pull_request_id, f.owner.id)
        .await
        .unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(closing.conn())
        .await
        .unwrap();
    let expected: Value =
        sqlx::query_scalar("SELECT to_jsonb(pr) FROM workspace_pull_requests pr WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(closing.conn())
            .await
            .unwrap();
    let route = format!("/api/v1/workspaces/{}/pull-requests", f.source.id);
    let body =
        json!({"targetWorkspaceId": f.base_id, "title":"Too late", "description":"Too late"});
    let update = post_pr(&app, &f.owner.jwt, &route, &body);
    let commit_close = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) AND query LIKE 'UPDATE workspace_pull_requests SET title=%')")
                    .bind(pid).fetch_one(&app.db).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("title acceptance waits on the actual close operation");
        closing.commit().await.unwrap();
    };
    let (response, ()) = tokio::join!(update, commit_close);
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(closed.status, "closed");
    let persisted: Value =
        sqlx::query_scalar("SELECT to_jsonb(pr) FROM workspace_pull_requests pr WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        persisted, expected,
        "closed metadata and pending title remain untouched"
    );
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_pr_refresh_skips_contended_pr_and_recovers_after_unlock() {
    let app = TestApp::spawn().await;
    let first = Fixture::create(&app, false).await;
    let later = Fixture::create(&app, false).await;
    let first_head = commit(&app, first.source.repo_id, "feature", "first-waiting.txt").await;
    let later_head = commit(&app, later.source.repo_id, "feature", "later-ready.txt").await;
    let mut blocker = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
        .bind(first.pull_request_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let count = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None),
    )
    .await
    .expect("a contended PR cannot starve a later organization")
    .unwrap();
    assert_eq!(count, 1);
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(later.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(heads
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == later_head));
    let still_old: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(first.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!still_old
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == first_head));
    // Issue #519: the expired lock wait is a settling state for the retrying
    // scan, not an INTERNAL_ERROR. (Recording needs the same PR lock, so
    // nothing can be recorded while it is held either way.)
    let error = g5_api::services::workspace_pull_request_create::refresh_after_push(
        &app.state,
        first.org.id,
        first.pull_request_id,
    )
    .await
    .unwrap_err();
    assert!(
        g5_api::services::workspace_pull_request_create::is_settling(&error.inner),
        "{error:?}"
    );
    assert_eq!(error.inner.error_code(), "CONFLICT", "{error:?}");
    blocker.commit().await.unwrap();
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        1
    );
    let recovered = read_pr_summary(&app, &first).await;
    assert!(recovered["sourceHeads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["headCommit"] == first_head));
    assert!(recovered["blockingMessage"].is_null(), "{recovered}");
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pr_update_rechecks_membership_between_local_and_provider_phases() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let other = TestUser::create(&app).await;
    TestUser::create_admin(&app, f.org.id).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
        .bind(f.org.id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.base_id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION pause_pr_title() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(8675310); RETURN NEW; END $$; CREATE TRIGGER pause_pr_title BEFORE UPDATE OF title ON workspace_pull_requests FOR EACH ROW EXECUTE FUNCTION pause_pr_title();")
        .execute(&app.db).await.unwrap();
    let mut barrier = app.db.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(8675310)")
        .execute(&mut *barrier)
        .await
        .unwrap();
    let route = format!("/api/v1/workspaces/{}/pull-requests", f.source.id);
    let request = json!({"targetWorkspaceId": f.base_id,"title":"Accepted before revocation"});
    let execute = post_pr(&app, &f.owner.jwt, &route, &request);
    let revoke_after_local_commit = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND locktype='advisory' AND objid=8675310 AND NOT granted)")
                    .fetch_one(&mut *barrier).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("local title is accepted under its PR lock");
        // Queue revocation behind the local phase's membership SHARE lock.
        // It must win before the next phase can acquire fresh membership.
        let mut revoker = app.db.begin().await.unwrap();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *revoker)
            .await
            .unwrap();
        let org = f.org.id;
        let actor = f.owner.id;
        let revoke = tokio::spawn(async move {
            sqlx::query(
                "UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2",
            )
            .bind(org)
            .bind(actor)
            .execute(&mut *revoker)
            .await
            .unwrap();
            revoker.commit().await.unwrap();
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event_type='Lock')")
                    .bind(pid).fetch_one(&mut *barrier).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }).await.expect("membership revocation waits for the local commit");
        sqlx::query("SELECT pg_advisory_unlock(8675310)")
            .execute(&mut *barrier)
            .await
            .unwrap();
        revoke.await.unwrap();
        let before: Value =
            sqlx::query_scalar("SELECT to_jsonb(pr) FROM workspace_pull_requests pr WHERE id=$1")
                .bind(f.pull_request_id)
                .fetch_one(&mut *barrier)
                .await
                .unwrap();
        before
    };
    let (response, before) = tokio::join!(execute, revoke_after_local_commit);
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["error"]["details"]["providerSyncPhase"], "admission",
        "{body}"
    );
    assert_eq!(
        body["error"]["details"]["providerSyncOutcome"], "not_attempted",
        "{body}"
    );
    // Revocation can commit before the non-locking capability check, or
    // while provider admission waits for its membership lock. Both guards
    // refuse the phase; scheduling determines which refusal is reported.
    assert!(
        matches!(
            body["error"]["details"]["causeCode"].as_str(),
            Some("UNAUTHORIZED" | "FORBIDDEN")
        ),
        "{body}"
    );
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(pr) FROM workspace_pull_requests pr WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        after, before,
        "provider completion cannot run after admission is lost"
    );
    assert_eq!(
        after["title"], "Accepted before revocation",
        "earlier authorized phase remains durable"
    );
    drop(barrier);
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_pr_refresh_deadline_is_visible_and_recovers() {
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let head = commit(&app, f.source.repo_id, "feature", "deadline.txt").await;
    let mut events = app
        .ws_connect(&f.owner, &format!("workspace:{}", f.source.id))
        .await;
    events.expect_subscribed().await;
    let mut blocker = app.db.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
        .bind(f.pull_request_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let attempt = g5_api::services::workspace_pull_request_create::refresh_after_push_with_timeout(
        &app.state,
        f.org.id,
        f.pull_request_id,
        Duration::from_secs(1),
    );
    let unblock = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))")
                    .bind(pid).fetch_one(&app.db).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("refresh reaches the real PR lock");
        tokio::time::sleep(Duration::from_millis(1100)).await;
        blocker.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(attempt, unblock);
    assert!(
        result.is_err(),
        "deadline must cancel publication, not merely log it"
    );
    events
        .expect_event("workspace.pull_request_updated", Duration::from_secs(3))
        .await;
    let failed = read_pr_summary(&app, &f).await;
    assert!(failed["blockingMessage"]
        .as_str()
        .unwrap()
        .contains("deadline elapsed"));
    assert!(failed["blockingMessage"]
        .as_str()
        .unwrap()
        .contains("g5d pr update"));
    assert!(!failed["sourceHeads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["headCommit"] == head));
    // The UI's actual Update PR operation repairs this failure without pending columns.
    let response = post_pr(
        &app,
        &f.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
        &json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let recovered = read_pr_summary(&app, &f).await;
    assert!(recovered["blockingMessage"].is_null(), "{recovered}");
    assert!(recovered["sourceHeads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["headCommit"] == head));
    drop(events);
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_pr_refresh_scan_ownership_survives_ticks_and_releases_on_cancel() {
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let head = commit(&app, f.source.repo_id, "feature", "leader.txt").await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let leader =
        g5_api::maintenance::workspace_pull_requests::spawn(app.state.clone(), cancel.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let heads: Value =
                sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
                    .bind(f.pull_request_id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            if heads
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["headCommit"] == head)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("leader completes its startup scan");
    let next = commit(&app, f.source.repo_id, "feature", "takeover.txt").await;
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        0,
        "another replica must skip discovery while the leader waits between ticks"
    );
    cancel.cancel();
    leader.await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
                .await
                .unwrap()
                == 1
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancelled leader releases its session lock for takeover");
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(heads
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["headCommit"] == next));
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_pr_refresh_blocked_authority_does_not_starve_later_prs() {
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let blocked = Fixture::create(&app, false).await;
    let later = Fixture::create(&app, false).await;
    let blocked_head = commit(&app, blocked.source.repo_id, "feature", "blocked.txt").await;
    let later_head = commit(&app, later.source.repo_id, "feature", "later.txt").await;
    let mut blocker = app.db.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT user_id FROM org_memberships WHERE org_id=$1 AND user_id=$2 FOR UPDATE")
        .bind(blocked.org.id)
        .bind(blocked.owner.id)
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    let scan = g5_api::maintenance::workspace_pull_requests::run_once_with_timeout(
        &app.state,
        None,
        Duration::from_secs(1),
    );
    let observe_blocked_admission = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))")
                    .bind(pid).fetch_one(&app.db).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("worker reaches the locked authority row");
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(scan, observe_blocked_admission)
    })
    .await
    .expect("failure admission is bounded while the authority lock remains held");
    assert_eq!(result.unwrap(), 1, "the later organization must progress");
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(later.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(heads
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["headCommit"] == later_head));
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(blocked.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!heads
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["headCommit"] == blocked_head));
    blocker.commit().await.unwrap();
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        1
    );
    let heads: Value =
        sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
            .bind(blocked.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(heads
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["headCommit"] == blocked_head));
    app.cleanup().await;
}

#[tokio::test]
async fn provider_merge_refusal_keeps_selected_token_out_of_public_error() {
    const NAME: &str = "provider_merge_refusal_keeps_selected_token_out_of_public_error";
    if isolated_provider_test(NAME) {
        return;
    }
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    provider.github.reset().await;
    let f = &provider.fixture;
    Mock::given(method("GET")).and(path("/repos/acme/api/pulls/21"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":21,"html_url":"https://github.com/acme/api/pull/21","state":"open","merged":false,"head":{"sha":f.source_head},"base":{"sha":f.base_head}})))
        .mount(&provider.github).await;
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/21/merge"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-github-request-id", "refused-merge-request")
                .set_body_json(
                    json!({"merged":false,"message":"Cannot merge provider-test-token"}),
                ),
        )
        .expect(1)
        .mount(&provider.github)
        .await;
    let (status, body) = f.ship(&provider.app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let text = body.to_string();
    assert!(!text.contains("provider-test-token"), "{body}");
    let persisted: String = sqlx::query_scalar(
        "SELECT last_error FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&provider.app.db)
    .await
    .unwrap();
    let response = provider
        .app
        .authenticated_client(&f.owner)
        .get(&format!(
            "/api/v1/workspaces/{}/pull-requests?role=source",
            f.source.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let public: Value = response.json().await.unwrap();
    for value in [&persisted, &public.to_string()] {
        assert!(!value.contains("provider-test-token"), "{value}");
        assert!(value.contains("made no provider changes"), "{value}");
        assert!(value.contains("refused-merge-request"), "{value}");
    }
    f.assert_unpublished(&provider.app).await;
    provider.github.verify().await;
    provider.app.cleanup().await;
}

/// PR evidence is persisted and provider PRs are created at its heads, so the
/// captured vectors are checked against one committed cut of each workspace
/// (`g5.fork.source.cut`). While another holder keeps the source's commit
/// fence past the bound, create refuses retryably and records no PR.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_pr_checks_its_evidence_against_one_committed_cut() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &fixture.source,
        fixture.owner.id,
        "modules.invoke",
    )
    .await;
    let route = format!("/api/v1/workspaces/{}/pull-requests", fixture.source.id);
    let holder = helpers::hold_commit_fence(&app, fixture.source.id).await;
    let request = json!({"title": "Cut check"});
    let create = post_pr(&app, &fixture.owner.jwt, &route, &request);
    let save = async {
        wait_for_operation(&peer, "commit").await;
        peer.release_one_rpc();
        wait_for_operation(&peer, "push").await;
        peer.release_one_rpc();
    };
    let (response, ()) = tokio::join!(create, save);
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("WORKSPACE_COMMIT_FENCE_BUSY"), "{body}");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0, "no PR evidence is recorded");
    holder.rollback().await.unwrap();
    app.cleanup().await;
}

// ===========================================================================
// Merge admission (g5.governance.approval.requirements, g5.workbench.gates)
// ===========================================================================

/// Attach a definition bound to `phases` as a requirement of `target`.
async fn attach_phase_requirement(
    app: &TestApp,
    owner: &TestUser,
    org_id: Uuid,
    target: Uuid,
    name: &str,
    phases: &[&str],
) -> (String, TestUser) {
    let reviewer =
        TestUser::create_with_scopes(app, org_id, &[g5_auth::Scope::WorkspacesRead]).await;
    let client = app.authenticated_client(owner);
    let response = client
        .post_json(
            &format!("/api/v1/orgs/{org_id}/approval-definitions"),
            &json!({ "name": name, "humanUserIds": [reviewer.id], "phases": phases }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let definition_id = body["data"]["id"].as_str().unwrap().to_string();
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{target}/approvals/requirements"),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    (definition_id, reviewer)
}

async fn decide(
    app: &TestApp,
    owner: &TestUser,
    reviewer: &TestUser,
    source: Uuid,
    definition_id: &str,
    phase: &str,
) {
    decide_outcome(
        app,
        owner,
        reviewer,
        source,
        definition_id,
        phase,
        "approved",
    )
    .await;
}

async fn decide_outcome(
    app: &TestApp,
    owner: &TestUser,
    reviewer: &TestUser,
    source: Uuid,
    definition_id: &str,
    phase: &str,
    outcome: &str,
) -> Value {
    let request = app.authenticated_client(owner).post_json(
        &format!("/api/v1/workspaces/{source}/approvals/requests"),
        &json!({"definitionId": definition_id, "definitionVersion": 1, "phase": phase, "actorUserIds": [reviewer.id]}),
    ).await;
    let status = request.status();
    let request: Value = request.json().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{request}");
    let heads = helpers::review_flow::heads(app, reviewer, source).await;
    let response = app
        .authenticated_client(reviewer)
        .post_json(
            &format!("/api/v1/workspaces/{source}/approvals/decisions"),
            &json!({ "definitionId": definition_id, "requestId": request["data"]["requests"][0]["requestId"], "outcome": outcome, "comment": "Requested type reviewed", "phase": phase, "reviewedGitHeads": heads }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Every attached target requirement gates merge independently of its phase
/// recommendation. The admitted ship records the same pinned evidence on
/// the pull request and merge run.
#[tokio::test]
async fn ship_is_admitted_by_target_requirements_independent_of_phase() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    let (be, business_reviewer) = attach_phase_requirement(
        &app,
        &fixture.owner,
        fixture.org.id,
        fixture.base_id,
        "Business sign-off",
        &["be"],
    )
    .await;
    let (ie, reviewer) = attach_phase_requirement(
        &app,
        &fixture.owner,
        fixture.org.id,
        fixture.base_id,
        "Merge sign-off",
        &["ie"],
    )
    .await;

    // The listed pull request carries the same refusal every surface shows.
    let listed: Value = app
        .authenticated_client(&fixture.owner)
        .get(&format!(
            "/api/v1/workspaces/{}/pull-requests?role=target",
            fixture.base_id
        ))
        .await
        .json()
        .await
        .unwrap();
    let admission = &listed["data"][0]["mergeAdmission"];
    assert_eq!(admission["admitted"], false, "{listed}");
    assert_eq!(admission["requirements"].as_array().unwrap().len(), 2);
    let required = admission["requirements"].as_array().unwrap();
    assert!(required.iter().any(|r| r["definitionId"] == be));
    assert!(required.iter().any(|r| r["definitionId"] == ie));

    let (status, body) = fixture.ship(&app).await;
    assert_eq!(status, StatusCode::CONFLICT, "ship refused: {body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Approval(s) required before merging"),
        "{body}"
    );
    fixture.assert_unpublished(&app).await;

    // A BE recommendation is still a target requirement after the IE vote.
    decide(
        &app,
        &fixture.owner,
        &reviewer,
        fixture.source.id,
        &ie,
        "ie",
    )
    .await;
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "BE requirement still pending: {body}"
    );
    fixture.assert_unpublished(&app).await;
    decide(
        &app,
        &fixture.owner,
        &business_reviewer,
        fixture.source.id,
        &be,
        "se",
    )
    .await;
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(status, StatusCode::OK, "ship admitted: {body}");
    fixture.assert_shipped(&app, &body).await;
    assert_eq!(body["data"]["run"]["approvalAdmission"]["admitted"], true);
    let required = body["data"]["run"]["approvalAdmission"]["requirements"]
        .as_array()
        .unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.iter().any(|r| r["definitionId"] == be));
    assert!(required.iter().any(|r| r["definitionId"] == ie));
    let recorded: Value =
        sqlx::query_scalar("SELECT merge_admission FROM workspace_pull_requests WHERE id = $1")
            .bind(fixture.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(recorded["admitted"], true);
    assert_eq!(recorded["targetWorkspaceId"], fixture.base_id.to_string());
    let run_admission: Value = sqlx::query_scalar(
        "SELECT approval_admission FROM workspace_merge_runs WHERE source_workspace_id = $1 AND advances_target",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(run_admission, recorded);
    app.cleanup().await;
}

/// Admission evaluates the workspace's configured merge target only: a
/// pull request toward another destination is refused rather than
/// evaluated against requirements the approvals page cannot show.
#[tokio::test]
async fn ship_toward_a_non_configured_target_is_refused() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    let elsewhere = TestWorkspace::create(&app, &fixture.org).await;
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $2 WHERE id = $1")
        .bind(fixture.source.id)
        .bind(elsewhere.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = fixture.ship(&app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("configured merge target"),
        "{body}"
    );
    fixture.assert_unpublished(&app).await;
    app.cleanup().await;
}

/// The provider-first path admits before any provider mutation: with an IE
/// requirement pending the route refuses without calling GitHub; once
/// approved, the merge proceeds and lands under the recorded admission.
#[tokio::test]
async fn provider_first_ship_refuses_before_any_provider_call_while_be_review_pending() {
    if isolated_provider_test(
        "provider_first_ship_refuses_before_any_provider_call_while_be_review_pending",
    ) {
        return;
    }
    let provider = ProviderFixture::create(true, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let (ie, reviewer) = attach_phase_requirement(
        app,
        &f.owner,
        f.org.id,
        f.base_id,
        "Merge sign-off",
        &["be"],
    )
    .await;
    let before = provider.github.received_requests().await.unwrap().len();
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        !provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .skip(before)
            .any(|request| request.method == "PUT" && request.url.path().ends_with("/merge")),
        "no provider merge may be attempted before admission"
    );
    f.assert_unpublished(app).await;

    decide(app, &f.owner, &reviewer, f.source.id, &ie, "se").await;
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    provider.assert_imported(&body).await;
    assert_eq!(body["data"]["run"]["approvalAdmission"]["admitted"], true);
    provider.app.cleanup().await;
}

/// With the organization's external-check policy on, the merge route re-reads
/// checks for the PR: a check the base branch requires that has not reported
/// refuses the merge before any provider call, even with every other check
/// still running; once it passes, the merge proceeds
/// (g5.governance.merge.external-policy, g5.governance.pr.checks.required).
#[tokio::test]
async fn provider_ship_waits_for_required_external_checks_under_org_policy() {
    if isolated_provider_test("provider_ship_waits_for_required_external_checks_under_org_policy") {
        return;
    }
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    sqlx::query("INSERT INTO org_configs(org_id,settings_json) VALUES($1,$2) ON CONFLICT(org_id) DO UPDATE SET settings_json=org_configs.settings_json||EXCLUDED.settings_json")
        .bind(f.org.id)
        .bind(json!({"external_checks_block_merge":true}))
        .execute(&app.db)
        .await
        .unwrap();
    // Check reads require the published PR's repository to be the attached one.
    g5_scm::native::redirect_transport_for_tests(
        "https://github.com/acme/api.git",
        &provider.remote.path().join("remote.git"),
    );
    sqlx::query("UPDATE repos SET git_url='https://github.com/acme/api.git' WHERE id=$1")
        .bind(f.source.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let repo = "^/repos/acme/api";
    let head = &f.source_head;
    Mock::given(method("GET"))
        .and(path_regex(format!("{repo}/branches/upstream$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name":"upstream","protection":{"enabled":true,
            "required_status_checks":{"enforcement_level":"everyone","contexts":["ci"],"checks":[{"context":"ci","app_id":null}]}}})))
        .mount(&provider.github)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(format!("{repo}/rules/branches/upstream$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&provider.github)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(format!("{repo}/commits/{head}/check-suites$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"check_suites":[
            {"id":9,"head_sha":head,"status":"in_progress","app":{"id":9}},
            {"id":20,"head_sha":head,"status":"queued","latest_check_runs_count":0,"app":{"id":20}}
        ]})))
        .mount(&provider.github)
        .await;
    let passed = Arc::new(AtomicBool::new(false));
    let state = passed.clone();
    let run_head = head.clone();
    Mock::given(method("GET"))
        .and(path_regex(format!("{repo}/commits/{head}/check-runs$")))
        .respond_with(move |_: &wiremock::Request| {
            let mut runs = vec![json!({"id":2,"name":"review","head_sha":run_head,"check_suite":{"id":9},"app":{"id":9},"status":"in_progress"})];
            if state.load(Ordering::SeqCst) {
                runs.push(json!({"id":1,"name":"ci","head_sha":run_head,"check_suite":{"id":9},"app":{"id":9},"status":"completed","conclusion":"success"}));
            }
            ResponseTemplate::new(200).set_body_json(json!({"check_runs":runs}))
        })
        .mount(&provider.github)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(format!("{repo}/commits/{head}/statuses$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&provider.github)
        .await;

    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string()
            .contains("external checks have not finished"),
        "{body}"
    );
    let requests = provider.github.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .any(|r| r.url.path().ends_with("/branches/upstream")),
        "the merge route read the base branch's required checks"
    );
    assert!(
        !requests
            .iter()
            .any(|r| r.method == "PUT" && r.url.path().ends_with("/merge")),
        "no provider merge while a required check is expected"
    );
    f.assert_unpublished(app).await;

    // "ci" passes; the not-required "review" run still in progress and the
    // silent App's empty suite do not hold the merge.
    passed.store(true, Ordering::SeqCst);
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    provider.assert_imported(&body).await;
    // Each merge decision read the base branch's requirements from GitHub
    // rather than the display cache.
    assert_eq!(
        provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/repos/acme/api/branches/upstream")
            .count(),
        2
    );
    provider.app.cleanup().await;
}

/// The provider merge validates the recorded admission's complete identity
/// before any provider mutation: a push and PR update after the admission
/// (a changed source head vector) refuse the provider merge with the
/// admission reason, and GitHub is never asked to merge.
#[tokio::test]
async fn provider_merge_refuses_a_revision_changed_since_admission() {
    if isolated_provider_test("provider_merge_refuses_a_revision_changed_since_admission") {
        return;
    }
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let (ie, reviewer) = attach_phase_requirement(
        app,
        &f.owner,
        f.org.id,
        f.base_id,
        "Merge sign-off",
        &["ie"],
    )
    .await;
    decide(app, &f.owner, &reviewer, f.source.id, &ie, "ie").await;

    // The merge route's first half: admission pinned for the current cut.
    let db = app.scoped_db_for_org(f.org.id).await;
    let outcome = app
        .state
        .coordinated_repo_publication_service
        .merge_workspace_pull_request(
            db,
            &app.state.workspace_merge_service,
            f.base_id,
            f.pull_request_id,
            f.owner.id,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        MergeWorkspacePullRequestOutcome::ProviderFirstMerge(_)
    ));
    let admitted: Value =
        sqlx::query_scalar("SELECT merge_admission FROM workspace_pull_requests WHERE id = $1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(admitted["admitted"], true);

    // A push and PR update land between the admission and the provider call.
    let moved = commit(app, f.source.repo_id, "feature", "after-admission.txt").await;
    sqlx::query(
        "UPDATE workspace_pull_request_provider_repos SET source_head_sha = $2 WHERE pull_request_id = $1",
    )
    .bind(f.pull_request_id)
    .bind(&moved)
    .execute(&app.db)
    .await
    .unwrap();

    let db = app.scoped_db_for_org(f.org.id).await;
    let refused = app
        .state
        .workspace_pull_request_provider_lifecycle_service
        .merge_provider_pull_requests(
            db,
            f.pull_request_id,
            &[
                g5_api::services::coordinated_repo_publication::ProviderMergeTargetHead {
                    repo_id: f.source.repo_id,
                    head_commit: f.base_head.clone(),
                },
            ],
            f.owner.id,
            || async { Ok(app.scoped_db_for_org(f.org.id).await) },
        )
        .await
        .err()
        .unwrap();
    assert!(
        refused
            .to_string()
            .contains("changed since the merge admission"),
        "{refused}"
    );
    assert!(
        !provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method == "PUT" && request.url.path().ends_with("/merge")),
        "no provider merge may follow a changed revision"
    );
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    let (status, provider_status): (String, String) =
        sqlx::query_as("SELECT status, provider_status FROM workspace_pull_requests WHERE id = $1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), provider_status.as_str()),
        ("open", "open")
    );
    // No merge was ever expected here: drop the fixture's merge expectation.
    provider.github.reset().await;
    provider.app.cleanup().await;
}

/// A provider acceptance whose acknowledgement never reached the database
/// is reconciled before the merge is admitted again: the admission recorded
/// for that cut stands through a withdrawal, and the retry lands the merge
/// without a second provider call.
#[tokio::test]
async fn provider_lost_acknowledgement_keeps_the_recorded_admission_through_a_withdrawal() {
    if isolated_provider_test(
        "provider_lost_acknowledgement_keeps_the_recorded_admission_through_a_withdrawal",
    ) {
        return;
    }
    let provider = ProviderFixture::create(false, true).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let (ie, reviewer) = attach_phase_requirement(
        app,
        &f.owner,
        f.org.id,
        f.base_id,
        "Merge sign-off",
        &["ie"],
    )
    .await;
    decide(app, &f.owner, &reviewer, f.source.id, &ie, "ie").await;
    let (status, body) = f.ship(app).await;
    assert_ne!(status, StatusCode::OK, "{body}");
    f.assert_unpublished(app).await;
    // The acknowledgement is lost: GitHub merged, the database still says open.
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_status = 'open' WHERE pull_request_id = $1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspace_pull_requests SET provider_status = 'open' WHERE id = $1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    // A newly requested decision asks for changes after the provider merge.
    // Recovery must retain the admission already recorded before that merge.
    let withdrawn =
        decide_outcome(app, &f.owner, &reviewer, f.source.id, &ie, "ie", "rejected").await;
    assert_eq!(
        withdrawn["data"]["outgoing"]["mergeAdmission"]["admitted"], false,
        "a fresh evaluation would refuse"
    );
    let repo = git2::Repository::open_bare(provider.remote.path().join("remote.git")).unwrap();
    repo.reference(
        "refs/heads/upstream",
        provider.accepted_head.parse().unwrap(),
        true,
        "recover fetch",
    )
    .unwrap();
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    provider.assert_imported(&body).await;
    assert_eq!(body["data"]["run"]["approvalAdmission"]["admitted"], true);
    provider.app.cleanup().await;
}

/// A partial provider acceptance (one repository merged, the aggregate a
/// failed sibling left behind) keeps the admission recorded for that cut
/// through a withdrawal: the retry lands on the recorded admission instead
/// of re-evaluating after an irreversible provider merge.
#[tokio::test]
async fn provider_partial_acceptance_keeps_the_recorded_admission_through_a_withdrawal() {
    if isolated_provider_test(
        "provider_partial_acceptance_keeps_the_recorded_admission_through_a_withdrawal",
    ) {
        return;
    }
    let provider = ProviderFixture::create(false, true).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let (ie, reviewer) = attach_phase_requirement(
        app,
        &f.owner,
        f.org.id,
        f.base_id,
        "Merge sign-off",
        &["ie"],
    )
    .await;
    decide(app, &f.owner, &reviewer, f.source.id, &ie, "ie").await;
    let (status, body) = f.ship(app).await;
    assert_ne!(status, StatusCode::OK, "{body}");
    f.assert_unpublished(app).await;
    // The aggregate a failed sibling repository leaves: this repository's
    // provider PR is merged, the pull request as a whole is not.
    sqlx::query("UPDATE workspace_pull_requests SET provider_status = 'partial' WHERE id = $1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    decide_outcome(app, &f.owner, &reviewer, f.source.id, &ie, "ie", "rejected").await;
    let repo = git2::Repository::open_bare(provider.remote.path().join("remote.git")).unwrap();
    repo.reference(
        "refs/heads/upstream",
        provider.accepted_head.parse().unwrap(),
        true,
        "recover fetch",
    )
    .unwrap();
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    provider.assert_imported(&body).await;
    provider.app.cleanup().await;
}

/// A direct (non-PR) merge is admitted against the current source cut: an
/// Integration Engineering approval recorded for an earlier revision is
/// stale and refuses the merge.
#[tokio::test]
async fn direct_merge_refuses_a_stale_integration_engineering_approval() {
    // The selected IGNORE policy keeps a moved approval counting (covered by
    // the approvals suites); MAKE_PENDING remains supported and must refuse
    // it on the direct merge path too (g5.governance.approval.source-commit-policy).
    let mut config = helpers::test_config();
    config.approval_policy.on_source_commit_vector_change =
        g5_contracts::types::SourceCommitVectorPolicy::MakePending;
    let app = TestApp::spawn_with_config(config).await;
    let fixture = Fixture::create(&app, false).await;
    let (ie, reviewer) = attach_phase_requirement(
        &app,
        &fixture.owner,
        fixture.org.id,
        fixture.base_id,
        "Merge sign-off",
        &["ie"],
    )
    .await;
    decide(
        &app,
        &fixture.owner,
        &reviewer,
        fixture.source.id,
        &ie,
        "ie",
    )
    .await;
    commit(
        &app,
        fixture.source.repo_id,
        "feature",
        "after-approval.txt",
    )
    .await;
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/merge", fixture.source.id),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Approval(s) required before merging"),
        "{body}"
    );
    assert_eq!(
        head(&app, fixture.source.repo_id, "main").await,
        fixture.base_head
    );
    // A fresh approval of the current revision admits the direct merge.
    decide(
        &app,
        &fixture.owner,
        &reviewer,
        fixture.source.id,
        &ie,
        "ie",
    )
    .await;
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/merge", fixture.source.id),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_ne!(
        head(&app, fixture.source.repo_id, "main").await,
        fixture.base_head
    );
    app.cleanup().await;
}

/// A member of the fixture's organization holding the workspace scopes and an
/// `org:admin` token scope, promoted to `role` (g5.governance.pr.base-admin).
async fn org_user_with_role(app: &TestApp, fixture: &Fixture, role: &str) -> TestUser {
    let user = TestUser::create_with_scopes(
        app,
        fixture.org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
            g5_auth::Scope::OrgAdmin,
        ],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role=$3 WHERE org_id=$1 AND user_id=$2")
        .bind(fixture.org.id)
        .bind(user.id)
        .bind(role)
        .execute(&app.db)
        .await
        .unwrap();
    user
}

async fn pull_request_action(
    app: &TestApp,
    user: &TestUser,
    workspace_id: Uuid,
    pull_request_id: Uuid,
    action: &str,
) -> (StatusCode, Value) {
    let response = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/pull-requests/{pull_request_id}/{action}"),
            &json!({}),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn pull_request_event_actors(app: &TestApp, pull_request_id: Uuid) -> Vec<(String, Uuid)> {
    sqlx::query_as(
        "SELECT event_type, actor_id FROM workspace_pull_request_events
          WHERE pull_request_id=$1 AND actor_id IS NOT NULL ORDER BY created_at, id",
    )
    .bind(pull_request_id)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn org_admin_refreshes_and_merges_a_pull_request_into_base_they_do_not_own() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, true).await;
    let admin = org_user_with_role(&app, &fixture, "admin").await;
    let before = pull_request_event_actors(&app, fixture.pull_request_id).await;

    let (status, body) = pull_request_action(
        &app,
        &admin,
        fixture.base_id,
        fixture.pull_request_id,
        "refresh",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin refresh: {body}");
    let (status, body) = pull_request_action(
        &app,
        &admin,
        fixture.base_id,
        fixture.pull_request_id,
        "merge",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin merge: {body}");
    fixture.assert_shipped(&app, &body).await;

    let after = pull_request_event_actors(&app, fixture.pull_request_id).await;
    let new_events = &after[before.len()..];
    assert!(
        new_events
            .iter()
            .any(|(kind, actor)| kind == "merged" && *actor == admin.id),
        "the merge is attributed to the administrator: {new_events:?}"
    );
    assert!(
        new_events.iter().all(|(_, actor)| *actor == admin.id),
        "every new event is the administrator's: {new_events:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn org_admin_closes_a_pull_request_into_base_through_the_target_only() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    let admin = org_user_with_role(&app, &fixture, "admin").await;

    // The source side keeps its owner's authority.
    let (status, body) = pull_request_action(
        &app,
        &admin,
        fixture.source.id,
        fixture.pull_request_id,
        "close",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "close through the source: {body}"
    );
    fixture.assert_unpublished(&app).await;

    let (status, body) = pull_request_action(
        &app,
        &admin,
        fixture.base_id,
        fixture.pull_request_id,
        "close",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin close: {body}");
    assert_eq!(body["data"]["pullRequest"]["status"], "closed", "{body}");
    let closed_by: Uuid = sqlx::query_scalar(
        "SELECT actor_id FROM workspace_pull_request_events WHERE pull_request_id=$1 AND event_type='closed'",
    )
    .bind(fixture.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(closed_by, admin.id);
    app.cleanup().await;
}

#[tokio::test]
async fn pull_requests_into_base_refuse_callers_without_organization_administration() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;

    // A member whose token claims `org:admin` and who holds a workspace-scoped
    // `org:admin` grant on Base is still not an organization administrator.
    let member = org_user_with_role(&app, &fixture, "member").await;
    sqlx::query(
        "INSERT INTO oauth_scopes (scope, description, category) \
         VALUES ('org:admin', 'test', 'test') ON CONFLICT (scope) DO NOTHING",
    )
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_permission_overrides \
         (workspace_id, user_id, scope, granted_by, granted_at) \
         VALUES ($1, $2, 'org:admin', $3, now())",
    )
    .bind(fixture.base_id)
    .bind(member.id)
    .bind(fixture.owner.id)
    .execute(&app.db)
    .await
    .unwrap();

    // An administrator whose token lacks `org:admin` acts with workspace
    // scopes only, which admit the owner alone.
    let admin = org_user_with_role(&app, &fixture, "admin").await;
    let admin_without_org_scope = TestUser {
        id: admin.id,
        email: admin.email.clone(),
        display_name: admin.display_name.clone(),
        jwt: helpers::test_jwt_minter()
            .mint(
                admin.id,
                fixture.org.id,
                vec![
                    g5_auth::Scope::WorkspacesRead,
                    g5_auth::Scope::WorkspacesWrite,
                ],
                3600,
            )
            .unwrap(),
    };

    // An administrator of another organization.
    let mut other_tenant = TestUser::create(&app).await;
    TestOrg::create_with_admin(&app, &mut other_tenant).await;

    for (label, user) in [
        ("member", &member),
        (
            "admin without org:admin token scope",
            &admin_without_org_scope,
        ),
        ("other organization's admin", &other_tenant),
    ] {
        for action in ["refresh", "close", "merge"] {
            let (status, body) =
                pull_request_action(&app, user, fixture.base_id, fixture.pull_request_id, action)
                    .await;
            assert!(
                matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
                "{label} {action}: {status} {body}"
            );
        }
    }
    fixture.assert_unpublished(&app).await;
    assert!(
        pull_request_event_actors(&app, fixture.pull_request_id)
            .await
            .iter()
            .all(|(_, actor)| *actor == fixture.owner.id),
        "refused callers record nothing"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn org_admin_cannot_act_on_pull_requests_into_a_workspace_that_is_not_base() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create_with_kind(&app, false, false).await;
    let admin = org_user_with_role(&app, &fixture, "admin").await;

    for action in ["refresh", "close", "merge"] {
        let (status, body) = pull_request_action(
            &app,
            &admin,
            fixture.base_id,
            fixture.pull_request_id,
            action,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{action}: {body}");
    }
    fixture.assert_unpublished(&app).await;

    // The owner of the target keeps the actions.
    let (status, body) = pull_request_action(
        &app,
        &fixture.owner,
        fixture.base_id,
        fixture.pull_request_id,
        "refresh",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner refresh: {body}");
    app.cleanup().await;
}

#[tokio::test]
async fn update_reports_a_provider_pr_merged_outside_g5_and_only_create_new_replaces_it() {
    const NAME: &str =
        "update_reports_a_provider_pr_merged_outside_g5_and_only_create_new_replaces_it";
    if isolated_provider_test(NAME) {
        return;
    }
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    sqlx::query("DELETE FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    let git_url = format!("https://github.com/g5-test-{}/api.git", f.org.id);
    let mirror = app.repo_path(f.source.repo_id).await;
    git2::Repository::open(&mirror)
        .unwrap()
        .remote("origin", &git_url)
        .unwrap();
    // GitHub's PRs by number, created on POST.
    let pulls_path = format!("/repos/g5-test-{}/api/pulls", f.org.id);
    let pulls: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let state = pulls.clone();
    Mock::given(method("GET"))
        .and(path(&pulls_path))
        .respond_with(move |_: &wiremock::Request| {
            let open: Vec<_> = state
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p["state"] == "open")
                .cloned()
                .collect();
            ResponseTemplate::new(200).set_body_json(open)
        })
        .mount(&provider.github)
        .await;
    let state = pulls.clone();
    let (org, head, base) = (f.org.id, f.source_head.clone(), f.base_head.clone());
    Mock::given(method("POST"))
        .and(path(&pulls_path))
        .respond_with(move |request: &wiremock::Request| {
            let mut pulls = state.lock().unwrap();
            let number = 42 + pulls.len() as i64;
            let body = request.body_json::<Value>().unwrap();
            let pull = json!({"number":number,"node_id":format!("PR_{number}"),
                "html_url":format!("https://github.com/g5-test-{org}/api/pull/{number}"),
                "state":"open","merged":false,"title":body["title"],"body":body["body"],
                "head":{"sha":head},"base":{"sha":base}});
            pulls.push(pull.clone());
            ResponseTemplate::new(201).set_body_json(pull)
        })
        .mount(&provider.github)
        .await;
    let state = pulls.clone();
    let number_of = |request: &wiremock::Request| -> i64 {
        request
            .url
            .path()
            .rsplit('/')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    Mock::given(method("GET"))
        .and(path_regex(format!("^{pulls_path}/[0-9]+$")))
        .respond_with(move |request: &wiremock::Request| {
            let n = number_of(request);
            let pulls = state.lock().unwrap();
            let pull = pulls.iter().find(|p| p["number"] == n).cloned().unwrap();
            ResponseTemplate::new(200).set_body_json(pull)
        })
        .mount(&provider.github)
        .await;
    let state = pulls.clone();
    let patches: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = patches.clone();
    Mock::given(method("PATCH"))
        .and(path_regex(format!("^{pulls_path}/[0-9]+$")))
        .respond_with(move |request: &wiremock::Request| {
            let n = number_of(request);
            let body = request.body_json::<Value>().unwrap();
            seen.lock().unwrap().push(body.clone());
            let mut pulls = state.lock().unwrap();
            let pull = pulls.iter_mut().find(|p| p["number"] == n).unwrap();
            for field in ["title", "body"] {
                if let Some(value) = body.get(field) {
                    pull[field] = value.clone();
                }
            }
            ResponseTemplate::new(200).set_body_json(pull.clone())
        })
        .mount(&provider.github)
        .await;
    let route = format!("/api/v1/workspaces/{}/pull-requests", f.source.id);
    let post = |body: Value| {
        let route = route.clone();
        async move {
            let response = post_pr(app, &f.owner.jwt, &route, &body).await;
            let status = response.status();
            let body: Value = response.json().await.unwrap();
            (status, body)
        }
    };
    let repo = |body: &Value| body["data"]["publication"]["repositories"][0].clone();

    let (status, first) = post(json!({"mode":"create","description":"First body"})).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["data"]["publication"]["complete"], true, "{first}");
    let created = repo(&first);
    assert_eq!(
        (
            created["action"].clone(),
            created["number"].clone(),
            created["head"].clone()
        ),
        (json!("created"), json!(42), json!("published")),
        "{first}"
    );
    assert!(pulls.lock().unwrap()[0]["body"]
        .as_str()
        .unwrap()
        .starts_with("First body"));

    // An explicit description reaches the existing provider PR.
    let (status, second) = post(json!({"mode":"update","description":"Second body"})).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let reused = repo(&second);
    assert_eq!(
        (
            reused["action"].clone(),
            reused["body"].clone(),
            reused["title"].clone()
        ),
        (json!("reused"), json!("applied"), Value::Null),
        "{second}"
    );
    let patch = patches.lock().unwrap().last().cloned().unwrap();
    assert!(patch["body"].as_str().unwrap().starts_with("Second body"));
    assert!(patch.get("title").is_none(), "{patch}");
    // Omission never reasserts the stored description.
    let before = patches.lock().unwrap().len();
    let (status, _) = post(json!({"mode":"update"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patches.lock().unwrap().len(), before);

    // Merged on GitHub outside G5: update reports it and opens nothing.
    {
        let mut pulls = pulls.lock().unwrap();
        pulls[0]["state"] = json!("closed");
        pulls[0]["merged"] = json!(true);
    }
    let (status, merged) = post(json!({"mode":"update"})).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["data"]["publication"]["complete"], false, "{merged}");
    let reported = repo(&merged);
    assert_eq!(reported["action"], "action_required", "{merged}");
    let message = reported["message"].as_str().unwrap();
    assert!(message.contains("#42 was merged outside"), "{message}");
    assert!(message.contains("g5d pr create --new"), "{message}");
    assert_eq!(pulls.lock().unwrap().len(), 1, "no replacement PR");
    // Plain create does not replace it either; --new does.
    let (_, plain) = post(json!({"mode":"create"})).await;
    assert_eq!(repo(&plain)["action"], "action_required", "{plain}");
    assert_eq!(pulls.lock().unwrap().len(), 1);
    let (status, replaced) = post(json!({"mode":"create","replace":true})).await;
    assert_eq!(status, StatusCode::OK, "{replaced}");
    let replacement = repo(&replaced);
    assert_eq!(
        (replacement["action"].clone(), replacement["number"].clone()),
        (json!("replaced"), json!(43)),
        "{replaced}"
    );
    assert_eq!(replaced["data"]["publication"]["complete"], true);
    // --new belongs to create.
    let (status, body) = post(json!({"mode":"update","replace":true})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Update never opens a G5 PR.
    sqlx::query("UPDATE workspace_pull_requests SET status='closed' WHERE id=$1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = post(json!({"mode":"update"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["details"]["kind"], "no_open_pull_request");
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1 AND status='open'",
    )
    .bind(f.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(open, 0);
    provider.app.cleanup().await;
}

#[tokio::test]
async fn explicit_pr_description_over_the_limit_is_refused_before_saving() {
    // The provider body ceiling (60,000) less the provenance reserve (1,024).
    const LIMIT: usize = 58_976;
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    let peer =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &fixture.source, fixture.owner.id).await;
    let route = format!("/api/v1/workspaces/{}/pull-requests", fixture.source.id);
    // Characters, not bytes: "é" is two bytes but one character.
    let over = "é".repeat(LIMIT + 1);
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &route,
            &json!({"targetWorkspaceId":fixture.base_id,"description":over}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string()
            .contains(&format!("{} characters; limit {LIMIT}", LIMIT + 1)),
        "{body}"
    );
    assert!(
        peer.received_rpc_methods().await.is_empty(),
        "a refused description must not save the workspace"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(fixture.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    // Surrounding whitespace is trimmed before counting, so this fits exactly.
    let at_limit = "é".repeat(LIMIT);
    let response = app
        .authenticated_client(&fixture.owner)
        .post_json(
            &route,
            &json!({"targetWorkspaceId":fixture.base_id,"description":format!("\n {at_limit} \n")}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["descriptionOverride"]
            .as_str()
            .map(|text| text.trim().chars().count()),
        Some(LIMIT),
        "{body}"
    );
    app.cleanup().await;
}

/// No merge bypasses GitHub's rules silently (g5.git.provider-rules): a
/// repository GitHub reports blocked refuses the ordinary merge before any
/// provider merge. The person's GitHub override (g5.git.provider-override)
/// then merges it under their own GitHub identity, and the record keeps it.
#[tokio::test]
async fn a_github_blocked_repository_refuses_the_merge_until_a_github_override() {
    if isolated_provider_test(
        "a_github_blocked_repository_refuses_the_merge_until_a_github_override",
    ) {
        return;
    }
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    for refusal_status in [None, Some(401), Some(403)] {
        let provider_denies = refusal_status.is_some();
        let provider = ProviderFixture::create(false, false).await;
        let app = &provider.app;
        let f = &provider.fixture;
        let sibling_target = head(app, f.other_repo_id, "main").await;
        if provider_denies {
            // A refused bypass must stop before an otherwise mergeable sibling.
            let sibling_source =
                commit(app, f.other_repo_id, "feature", "sibling-change.txt").await;
            bind_other_member(app, f).await;
            sqlx::query("INSERT INTO workspace_pull_request_provider_repos(id,org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_pr_url,provider_status,source_head_sha,target_head_sha) VALUES($1,$2,$3,$4,'github','acme/unchanged','feature','main',22,'https://github.com/acme/unchanged/pull/22','open',$5,$6)")
                .bind(Uuid::new_v4()).bind(f.org.id).bind(f.pull_request_id).bind(f.other_repo_id)
                .bind(&sibling_source).bind(&sibling_target).execute(&app.db).await.unwrap();
            Mock::given(method("GET"))
                .and(path("/repos/acme/unchanged/pulls/22"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "number":22,"html_url":"https://github.com/acme/unchanged/pull/22",
                    "state":"open","merged":false,"mergeable":true,"rebaseable":true,"mergeable_state":"clean",
                    "head":{"sha":sibling_source},"base":{"ref":"main","sha":sibling_target}
                }))).mount(&provider.github).await;
            Mock::given(method("PUT"))
                .and(path("/repos/acme/unchanged/pulls/22/merge"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"merged":true,"sha":sibling_source})),
                )
                .expect(0)
                .mount(&provider.github)
                .await;
        }
        let (definition, reviewer) =
            attach_phase_requirement(app, &f.owner, f.org.id, f.base_id, "Sign-off", &["ie"]).await;
        decide(app, &f.owner, &reviewer, f.source.id, &definition, "ie").await;
        sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
            .bind(f.base_id)
            .bind(reviewer.id)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
            .bind(f.org.id)
            .bind(f.owner.id)
            .execute(&app.db)
            .await
            .unwrap();
        // GitHub reports the pull request blocked for every read before the
        // override's provider merge: the refused ship (reconcile, preflight), the
        // preview and the override (reconcile, its shown set, preflight).
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/pulls/21"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "number":21,"html_url":"https://github.com/acme/api/pull/21",
                "state":"open","merged":false,"mergeable":true,"mergeable_state":"blocked",
                "head":{"sha":f.source_head},"base":{"ref":"upstream","sha":f.base_head}
            })))
            .with_priority(1)
            .up_to_n_times(6)
            .mount(&provider.github)
            .await;

        let (status, refused) = f.ship(app).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(
            refused
                .to_string()
                .contains("blocked by its branch protection or rulesets"),
            "{refused}"
        );
        assert!(
            !provider
                .github
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|r| r.method == "PUT"),
            "no provider merge of a blocked repository"
        );
        f.assert_unpublished(app).await;

        let token = app
            .state
            .auth_service
            .issue_user_tokens(f.owner.id, Some(f.org.id), g5_auth::SESSION_CLIENT_BROWSER)
            .await
            .unwrap()
            .jwt;
        let path = format!(
            "{}/api/v1/workspaces/{}/pull-requests/{}/merge-override",
            app.base_url, f.base_id, f.pull_request_id
        );
        let browser = |request: reqwest::RequestBuilder| {
            request
                .bearer_auth(&token)
                .header("Sec-Fetch-Site", "same-origin")
        };
        let shown: Value = browser(app.client.get(&path))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let blocked = &shown["data"]["blockers"]["github"];
        assert_eq!(blocked[0]["repository"], "acme/api", "{shown}");
        assert_eq!(blocked[0]["mergeState"], "blocked");
        assert_eq!(shown["data"]["blockers"]["approvals"], json!([]));
        assert_eq!(shown["data"]["authority"]["github"]["allowed"], true);
        assert_eq!(shown["data"]["authority"]["g5"]["allowed"], false);

        if provider_denies {
            Mock::given(method("PUT"))
                .and(wiremock::matchers::path("/repos/acme/api/pulls/21/merge"))
                .respond_with(
                    ResponseTemplate::new(refusal_status.unwrap()).set_body_json(
                        json!({"message":"Bypass permission denied provider-test-token"}),
                    ),
                )
                .with_priority(1)
                .expect(1)
                .mount(&provider.github)
                .await;
        }
        let response = browser(app.client.post(&path))
        .json(&json!({"reason":"Ruleset misconfigured","confirmed":true,"digest":shown["data"]["digest"]}))
        .send()
        .await
        .unwrap();
        if provider_denies {
            let status = response.status();
            let refused: Value = response.json().await.unwrap();
            assert!(!status.is_success(), "{refused}");
            assert!(
                refused
                    .to_string()
                    .contains("Your GitHub account can't bypass the rules"),
                "{refused}"
            );
            assert!(
                refused.to_string().contains("Bypass permission denied"),
                "{refused}"
            );
            assert!(
                !refused.to_string().contains("provider-test-token"),
                "{refused}"
            );
            f.assert_unpublished(app).await;
            let attempts = provider.github.received_requests().await.unwrap();
            let attempts: Vec<_> = attempts
                .iter()
                .filter(|request| request.method == "PUT")
                .collect();
            assert_eq!(
                attempts.len(),
                1,
                "no sibling merge or alternate-credential retry"
            );
            assert_eq!(attempts[0].url.path(), "/repos/acme/api/pulls/21/merge");
            assert_eq!(head(app, f.other_repo_id, "main").await, sibling_target);
            let reserved: bool = sqlx::query_scalar("SELECT bool_or(merge_pending OR merge_accepted) FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
                .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
            assert!(
                !reserved,
                "refused and undispatched reservations are released"
            );
            assert_eq!(
                attempts[0].headers.get("authorization").unwrap(),
                "Bearer provider-test-token"
            );
            provider.github.reset().await;
            provider.app.cleanup().await;
            continue;
        }
        assert_eq!(response.status(), StatusCode::OK);
        let merged: Value = response.json().await.unwrap();
        provider.assert_imported(&merged).await;
        assert_eq!(
            merged["data"]["pullRequest"]["providerRepos"][0]["mergeTargetHeadSha"], f.base_head,
            "an override uses the same durable target evidence as an ordinary merge"
        );
        let record = &merged["data"]["pullRequest"]["mergeOverride"];
        assert_eq!(
            record["githubRepositories"][0]["repository"], "acme/api",
            "{merged}"
        );
        assert_eq!(record["waivedApprovals"], json!([]));
        // A provider-first override is recorded once, with its admission.
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM workspace_pull_request_events
          WHERE pull_request_id = $1 AND event_type = 'merge_overridden'",
        )
        .bind(f.pull_request_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(events, 1);
        let requests = provider.github.received_requests().await.unwrap();
        let merges: Vec<_> = requests
            .iter()
            .filter(|request| request.method == "PUT")
            .collect();
        assert_eq!(merges.len(), 1);
        assert_eq!(
            merges[0].headers.get("authorization").unwrap(),
            "Bearer provider-test-token"
        );
        provider.app.cleanup().await;
    }
}

/// G5 admission is retained through a provider acceptance and failed import,
/// even when the original administrator subsequently loses admin authority.
#[tokio::test]
async fn a_g5_override_survives_provider_acceptance_for_recovery_only() {
    if isolated_provider_test("a_g5_override_survives_provider_acceptance_for_recovery_only") {
        return;
    }
    let provider = ProviderFixture::create(false, true).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let (_, _reviewer) = attach_phase_requirement(
        app,
        &f.owner,
        f.org.id,
        f.base_id,
        "Merge sign-off",
        &["ie"],
    )
    .await;
    let token = app
        .state
        .auth_service
        .issue_user_tokens(f.owner.id, Some(f.org.id), g5_auth::SESSION_CLIENT_BROWSER)
        .await
        .unwrap()
        .jwt;
    let path = format!(
        "{}/api/v1/workspaces/{}/pull-requests/{}/merge-override",
        app.base_url, f.base_id, f.pull_request_id
    );
    let shown = helpers::review_flow::body(
        app.client
            .get(&path)
            .bearer_auth(&token)
            .header("Sec-Fetch-Site", "same-origin")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(shown["authority"]["g5"]["allowed"], true);
    assert_eq!(shown["blockers"]["approvals"].as_array().unwrap().len(), 1);
    let response = app
        .client
        .post(&path)
        .bearer_auth(&token)
        .header("Sec-Fetch-Site", "same-origin")
        .json(&json!({"reason":"Approver unavailable","confirmed":true,"digest":shown["digest"]}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let failed_import: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND, "{failed_import}");
    f.assert_unpublished(app).await;
    let accepted: bool = sqlx::query_scalar(
        "SELECT merge_accepted FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        accepted,
        "authorized G5 override reached and was accepted by the provider"
    );

    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let repo = git2::Repository::open_bare(provider.remote.path().join("remote.git")).unwrap();
    repo.reference(
        "refs/heads/upstream",
        provider.accepted_head.parse().unwrap(),
        true,
        "recover fetch",
    )
    .unwrap();
    let (status, merged) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    provider.assert_imported(&merged).await;
    assert_eq!(
        merged["data"]["pullRequest"]["mergeOverride"]["waivedApprovals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_pull_request_events WHERE pull_request_id=$1 AND event_type='merge_overridden'")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(events, 1);
    provider.app.cleanup().await;
}

/// Reservation commits before provider dispatch. The reopened transaction
/// cannot use a G5 waiver after the person loses administration, even though
/// ownership continues to authorize ordinary merge. Base's audience is fixed
/// (g5.workspace.audience), so an active member cannot lose it separately.
#[tokio::test]
async fn overrides_recheck_authority_before_provider_dispatch() {
    if isolated_provider_test("overrides_recheck_authority_before_provider_dispatch") {
        return;
    }
    use g5_api::services::git::merge_override::{
        current_merge_override_authority, MergeOverrideIntent,
    };
    for change in ["admin_role", "source_owner", "last_requirement"] {
        let provider = ProviderFixture::create(false, false).await;
        let app = &provider.app;
        let f = &provider.fixture;
        let (definition, reviewer) = attach_phase_requirement(
            app,
            &f.owner,
            f.org.id,
            f.base_id,
            "Merge sign-off",
            &["ie"],
        )
        .await;
        if change != "admin_role" {
            decide(app, &f.owner, &reviewer, f.source.id, &definition, "ie").await;
            sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
                .bind(f.base_id)
                .bind(reviewer.id)
                .execute(&app.db)
                .await
                .unwrap();
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/repos/acme/api/pulls/21"))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                    "number":21,"html_url":"https://github.com/acme/api/pull/21",
                    "state":"open","merged":false,"mergeable":true,"mergeable_state":"blocked",
                    "head":{"sha":f.source_head},"base":{"ref":"upstream","sha":f.base_head}
                })))
                .with_priority(1)
                .mount(&provider.github)
                .await;
        }
        let token = app
            .state
            .auth_service
            .issue_user_tokens(f.owner.id, Some(f.org.id), g5_auth::SESSION_CLIENT_BROWSER)
            .await
            .unwrap()
            .jwt;
        let shown = helpers::review_flow::body(
            app.client
                .get(format!(
                    "{}/api/v1/workspaces/{}/pull-requests/{}/merge-override",
                    app.base_url, f.base_id, f.pull_request_id
                ))
                .bearer_auth(&token)
                .header("Sec-Fetch-Site", "same-origin")
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
        )
        .await;
        let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
        let authority = current_merge_override_authority(&mut db, f.source.id, f.base_id, true)
            .await
            .unwrap();
        let intent = MergeOverrideIntent {
            actor_id: f.owner.id,
            actor_name: None,
            reason: "Approver unavailable".into(),
            self_override: true,
            expected_digest: shown["digest"].as_str().unwrap().into(),
            authority,
            checks: vec![],
            github: serde_json::from_value(shown["blockers"]["github"].clone()).unwrap(),
        };
        let outcome = app
            .state
            .coordinated_repo_publication_service
            .merge_workspace_pull_request(
                db,
                &app.state.workspace_merge_service,
                f.base_id,
                f.pull_request_id,
                f.owner.id,
                Some(&intent),
            )
            .await
            .unwrap();
        let MergeWorkspacePullRequestOutcome::ProviderFirstMerge(request) = outcome else {
            panic!("fixture must prepare a provider merge");
        };
        let db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
        let refused = app
        .state
        .workspace_pull_request_provider_lifecycle_service
        .merge_provider_pull_requests(
            db,
            f.pull_request_id,
            &request.target_heads,
            f.owner.id,
            || async {
                match change {
                    "admin_role" => {
                        sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
                            .bind(f.org.id).bind(f.owner.id).execute(&app.db).await.unwrap();
                    },
                    "source_owner" => {
                        sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
                            .bind(f.source.id).bind(reviewer.id).execute(&app.db).await.unwrap();
                    },
                    "last_requirement" => {
                        helpers::review_flow::body(app.authenticated_client(&f.owner).post_json(
                            &format!("/api/v1/workspaces/{}/approvals/requirements/{definition}/retire", f.base_id),
                            &json!({}),
                        ).await, StatusCode::OK).await;
                    },
                    _ => unreachable!(),
                }
                Ok(app.scoped_db_for_user_org(f.owner.id, f.org.id).await)
            },
        )
        .await
        .err()
        .expect("dispatch refuses revoked authority");
        assert!(
            matches!(refused, g5_contracts::G5Error::ConflictDetails { ref details, .. }
        if details["code"] == "merge_override_not_authorized"),
            "{refused}"
        );
        assert!(!provider
            .github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method == "PUT"));
        f.assert_unpublished(app).await;
        let reservation: (bool, bool) = sqlx::query_as("SELECT merge_pending,merge_accepted FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(
            reservation,
            (false, false),
            "the undispatched reservation is released"
        );
        provider.github.reset().await;
        provider.app.cleanup().await;
    }
}

/// A repository someone already merged on GitHub is recorded as merged
/// outside G5, and the external-checks policy, which evaluates only what G5
/// has yet to merge, no longer holds the pull request
/// (g5.git.provider-merged-outside).
#[tokio::test]
async fn a_repository_merged_on_github_is_recorded_and_outside_the_checks_policy() {
    if isolated_provider_test(
        "a_repository_merged_on_github_is_recorded_and_outside_the_checks_policy",
    ) {
        return;
    }
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    sqlx::query("INSERT INTO org_configs(org_id,settings_json) VALUES($1,$2) ON CONFLICT(org_id) DO UPDATE SET settings_json=org_configs.settings_json||EXCLUDED.settings_json")
        .bind(f.org.id)
        .bind(json!({"external_checks_block_merge":true}))
        .execute(&app.db)
        .await
        .unwrap();
    // Someone merged the pull request on GitHub: the base branch moved.
    git2::Repository::open_bare(provider.remote.path().join("remote.git"))
        .unwrap()
        .reference(
            "refs/heads/upstream",
            provider.accepted_head.parse().unwrap(),
            true,
            "merged on GitHub",
        )
        .unwrap();
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/21"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number":21,"html_url":"https://github.com/acme/api/pull/21",
            "state":"closed","merged":true,"mergeable":null,
            "head":{"sha":f.source_head},"base":{"ref":"upstream","sha":f.base_head},
            "merge_commit_sha":provider.accepted_head
        })))
        .with_priority(1)
        .mount(&provider.github)
        .await;

    let (status, merged) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    // GitHub's merge commit landed; G5 made no provider merge of its own.
    assert_eq!(
        head(app, f.source.repo_id, "main").await,
        provider.accepted_head,
        "{merged}"
    );
    assert_eq!(merged["data"]["pullRequest"]["status"], "merged");
    assert!(
        merged["data"]["pullRequest"]["providerRepos"][0]["mergeTargetHeadSha"].is_null(),
        "an outside merge without a G5 attempt has unknown merge-time target evidence"
    );
    assert_eq!(
        merged["data"]["pullRequest"]["providerRepos"][0]["mergedOutsideG5"], true,
        "{merged}"
    );
    let events: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT message FROM workspace_pull_request_events
          WHERE pull_request_id = $1 AND event_type = 'provider_merged_outside'",
    )
    .bind(f.pull_request_id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(events, vec![Some("acme/api".to_string())]);
    // G5 never asked GitHub to merge it; the fixture's merge mock is unused.
    assert!(!provider
        .github
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.method == "PUT"));
    provider.github.reset().await;
    provider.app.cleanup().await;
}

/// `g5d pr` publishes committed work only (`g5.git.pr-cli.create-save`): with
/// `save: false` the running VM is never asked to commit or push.
#[tokio::test]
async fn g5d_create_publishes_committed_work_without_saving() {
    let app = TestApp::spawn().await;
    let fixture = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &fixture).await;
    // Every VM RPC waits on this barrier, so a Save would never return.
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &fixture.source,
        fixture.owner.id,
        "modules.invoke",
    )
    .await;
    let token = vm_pr_token(&app, &fixture, &peer);
    let route = format!(
        "/api/internal/workspaces/{}/pull-requests",
        fixture.source.id
    );
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        post_pr(
            &app,
            &token,
            &route,
            &json!({"targetWorkspaceId":fixture.base_id,"title":"From the CLI","save":false}),
        ),
    )
    .await
    .expect("no Save, so no VM RPC to wait on");
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let pr = &body["data"]["pullRequest"];
    assert_eq!(pr["title"], "From the CLI");
    assert_eq!(pr["sourceAutoPush"], true, "{body}");
    assert!(pr["sourceHeads"]
        .as_array()
        .unwrap()
        .iter()
        .any(|repo| repo["headCommit"] == fixture.source_head));
    peer.release_rpc_barrier();
    app.cleanup().await;
}

/// `g5d pr edit` (`g5.git.pr-cli.edit`) changes only the open PR's text.
#[tokio::test]
async fn edit_changes_only_the_open_prs_text_without_saving() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &f.source,
        f.owner.id,
        "modules.invoke",
    )
    .await;
    let token = vm_pr_token(&app, &f, &peer);
    let route = format!("/api/internal/workspaces/{}/pull-requests", f.source.id);
    let before = read_pr_summary(&app, &f).await;
    let edit = |body: Value| {
        let (route, token, app) = (route.clone(), token.clone(), &app);
        async move {
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                post_pr(app, &token, &route, &body),
            )
            .await
            .expect("edit never asks the VM to save");
            let status = response.status();
            (status, response.json::<Value>().await.unwrap())
        }
    };
    let (status, body) =
        edit(json!({"mode":"edit","title":"Edited title","description":"  Edited body  "})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["alreadyExisted"], true);
    let pr = &body["data"]["pullRequest"];
    assert_eq!(pr["id"], f.pull_request_id.to_string());
    assert_eq!(pr["title"], "Edited title");
    assert_eq!(pr["descriptionOverride"], "Edited body");
    // Evidence is untouched: edit neither saves nor republishes.
    assert_eq!(pr["sourceHeads"], before["sourceHeads"]);
    // Title alone keeps the description; an empty body clears it.
    let (status, body) = edit(json!({"mode":"edit","title":"Second title"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["descriptionOverride"],
        "Edited body"
    );
    let (status, body) = edit(json!({"mode":"edit","description":""})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["data"]["pullRequest"]["descriptionOverride"].is_null(),
        "{body}"
    );
    assert_eq!(body["data"]["pullRequest"]["title"], "Second title");
    // Nothing to edit, or a replacement, is refused.
    let (status, body) = edit(json!({"mode":"edit"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = edit(json!({"mode":"edit","title":"x","replace":true})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // Edit never opens a G5 PR.
    sqlx::query("UPDATE workspace_pull_requests SET status='closed' WHERE id=$1")
        .bind(f.pull_request_id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = edit(json!({"mode":"edit","title":"Reopen?"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["details"]["kind"], "no_open_pull_request");
    peer.release_rpc_barrier();
    app.cleanup().await;
}

/// With Auto-push off a push stays in G5 (`g5.git.pr-cli.manual-forwarding`):
/// refresh says so once in the PR's history instead of waiting, and the PR
/// follows as soon as someone publishes the branch (the UI's Push).
#[tokio::test]
async fn auto_push_off_records_once_that_a_push_was_not_forwarded() {
    const NAME: &str = "auto_push_off_records_once_that_a_push_was_not_forwarded";
    if isolated_provider_test(NAME) {
        return;
    }
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    let org_id = f.org.id;
    provider.github.reset().await;
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_repo_full_name=$2 WHERE pull_request_id=$1")
        .bind(f.pull_request_id).bind(format!("g5-test-{}/api", f.org.id)).execute(&app.db).await.unwrap();
    mount_published_refs(&provider).await;
    let remote_path = provider.remote.path().join("remote.git");
    let path = remote_path.clone();
    Mock::given(method("GET")).and(path_regex("/repos/.*/api/pulls(/21)?$"))
        .respond_with(move |request: &wiremock::Request| {
            let repo = git2::Repository::open_bare(&path).unwrap();
            let source = repo.refname_to_id("refs/heads/feature").unwrap().to_string();
            let target = repo.refname_to_id("refs/heads/upstream").unwrap().to_string();
            let pull = json!({"number":21,"html_url":format!("https://github.com/g5-test-{}/api/pull/21", org_id),
                "state":"open","merged":false,"head":{"sha":source},"base":{"sha":target}});
            ResponseTemplate::new(200).set_body_json(if request.url.path().ends_with("/21") {pull} else {json!([pull])})
        }).mount(&provider.github).await;
    sqlx::query("UPDATE workspaces SET push_mode='manual' WHERE id=$1")
        .bind(f.source.id)
        .execute(&app.db)
        .await
        .unwrap();
    let new_head = commit(app, f.source.repo_id, "feature", "after-pr.txt").await;
    let refresh = || {
        g5_api::services::workspace_pull_request_create::refresh_after_push(
            &app.state,
            f.org.id,
            f.pull_request_id,
        )
    };
    for _ in 0..2 {
        assert!(!refresh().await.unwrap(), "nothing reached GitHub");
    }
    assert_eq!(
        refresh_failures(app, f.pull_request_id).await,
        vec![g5_api::services::workspace_pull_request_create::NOT_FORWARDED.to_owned()]
    );
    let summary = read_pr_summary(app, f).await;
    assert_eq!(summary["sourceAutoPush"], false);
    assert!(summary["blockingMessage"]
        .as_str()
        .unwrap()
        .contains("Auto-push is off"));
    // Someone publishes the branch, as the UI's Push does: the PR follows.
    let mirror = app.repo_path(f.source.repo_id).await;
    git(
        &mirror,
        &["push", remote_path.to_str().unwrap(), "feature:feature"],
    );
    assert!(refresh().await.unwrap());
    let recovered = read_pr_summary(app, f).await;
    assert_eq!(recovered["providerRepos"][0]["sourceHeadSha"], new_head);
    assert!(recovered["blockingMessage"].is_null(), "{recovered}");
    provider.app.cleanup().await;
}

/// `g5d pr create -B NAME` and `--dry-run` from the VM read the UI's targets
/// and preview, which write nothing.
#[tokio::test]
async fn the_vm_reads_pr_targets_and_previews_without_writing() {
    let app = TestApp::spawn().await;
    let f = Fixture::create(&app, false).await;
    remove_fixture_pr(&app, &f).await;
    let peer = helpers::TestWorkspaceRuntimePeer::attach(&app, &f.source, f.owner.id).await;
    let token = vm_pr_token(&app, &f, &peer);
    let response = app
        .client
        .get(format!(
            "{}/api/internal/workspaces/{}/pull-requests/targets",
            app.base_url, f.source.id
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let targets: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{targets}");
    assert!(targets["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == f.base_id.to_string()));
    let response = post_pr(
        &app,
        &token,
        &format!(
            "/api/internal/workspaces/{}/pull-requests/preview",
            f.source.id
        ),
        &json!({"targetWorkspaceId": f.base_id}),
    )
    .await;
    let status = response.status();
    let preview: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert!(preview["data"]["title"].is_string(), "{preview}");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0, "a preview writes nothing");
    app.cleanup().await;
}

/// `g5d pr edit` applies text to the open GitHub PR and pushes nothing
/// (`g5.git.pr-cli.edit`).
#[tokio::test]
async fn edit_applies_text_to_github_without_pushing_or_opening_prs() {
    const NAME: &str = "edit_applies_text_to_github_without_pushing_or_opening_prs";
    if isolated_provider_test(NAME) {
        return;
    }
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let provider = ProviderFixture::create(false, false).await;
    let app = &provider.app;
    let f = &provider.fixture;
    provider.github.reset().await;
    let full_name = format!("g5-test-{}/api", f.org.id);
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_repo_full_name=$2 WHERE pull_request_id=$1")
        .bind(f.pull_request_id).bind(&full_name).execute(&app.db).await.unwrap();
    let remote = provider.remote.path().join("remote.git");
    let feature = || {
        git2::Repository::open_bare(&remote)
            .unwrap()
            .refname_to_id("refs/heads/feature")
            .unwrap()
            .to_string()
    };
    let before = feature();
    let patches: Arc<Mutex<Vec<Value>>> = Arc::default();
    let seen = patches.clone();
    let (head, base) = (f.source_head.clone(), f.base_head.clone());
    let url = format!("https://github.com/{full_name}/pull/21");
    Mock::given(method("PATCH"))
        .and(path(format!("/repos/{full_name}/pulls/21")))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = request.body_json().unwrap();
            seen.lock().unwrap().push(body.clone());
            ResponseTemplate::new(200).set_body_json(json!({
                "number":21,"node_id":"PR_21","html_url":url,"state":"open","merged":false,
                "title":body["title"],"body":body["body"],
                "head":{"sha":head,"ref":"feature"},"base":{"sha":base,"ref":"upstream"}}))
        })
        .expect(1)
        .mount(&provider.github)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/repos/{full_name}/pulls")))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&provider.github)
        .await;
    // The GitHub PR's live state: an edit never touches a closed PR.
    let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let state = closed.clone();
    let (head, base) = (f.source_head.clone(), f.base_head.clone());
    let url = format!("https://github.com/{full_name}/pull/21");
    Mock::given(method("GET"))
        .and(path(format!("/repos/{full_name}/pulls/21")))
        .respond_with(move |_: &wiremock::Request| {
            let closed = state.load(std::sync::atomic::Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({
                "number":21,"html_url":url,
                "state":if closed {"closed"} else {"open"},"merged":false,
                "head":{"sha":head,"ref":"feature"},"base":{"sha":base,"ref":"upstream"}}))
        })
        .mount(&provider.github)
        .await;
    let response = post_pr(
        app,
        &f.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
        &json!({"mode":"edit","title":"Edited title","description":"Edited body"}),
    )
    .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let repo = &body["data"]["publication"]["repositories"][0];
    assert_eq!(
        (
            repo["number"].clone(),
            repo["title"].clone(),
            repo["body"].clone()
        ),
        (json!(21), json!("applied"), json!("applied")),
        "{body}"
    );
    let patch = patches.lock().unwrap()[0].clone();
    assert_eq!(patch["title"], "Edited title");
    assert!(
        patch["body"].as_str().unwrap().starts_with("Edited body"),
        "{patch}"
    );
    assert_eq!(feature(), before, "edit never pushes the source branch");
    // Closed on GitHub outside G5: nothing is sent and the row is not reopened.
    closed.store(true, std::sync::atomic::Ordering::SeqCst);
    let response = post_pr(
        app,
        &f.owner.jwt,
        &format!("/api/v1/workspaces/{}/pull-requests", f.source.id),
        &json!({"mode":"edit","title":"Too late"}),
    )
    .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let repo = &body["data"]["publication"]["repositories"][0];
    assert_eq!(repo["title"], "not_applied", "{body}");
    assert!(
        repo["message"]
            .as_str()
            .unwrap_or_default()
            .contains("no longer open"),
        "{body}"
    );
    provider.github.verify().await;
    provider.app.cleanup().await;
}

// Assert a competing revocation cannot cross an already admitted reader write.
async fn assert_membership_revocation_waits(app: &TestApp, org: Uuid, actor: Uuid) {
    let mut revoker = app.db.begin().await.unwrap();
    sqlx::query("SET LOCAL lock_timeout='100ms'")
        .execute(&mut *revoker)
        .await
        .unwrap();
    let error = sqlx::query(
        "UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2",
    )
    .bind(org)
    .bind(actor)
    .execute(&mut *revoker)
    .await
    .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("55P03")
    );
    revoker.rollback().await.unwrap();
}
