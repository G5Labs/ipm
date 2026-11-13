//! Base re-verification runs (g5.reverification) through the real routes: a run
//! starts in one step with its fork of Base, at most one is in progress per
//! organization, its state is read from the fork, the workspace and its pull
//! requests into Base, and Home's status counts what Base gained since the
//! last completed run forked.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn switch_on(app: &TestApp, org_id: Uuid) {
    sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json) \
         VALUES ($1, '{\"verification_action_enabled\": true}') \
         ON CONFLICT (org_id) DO UPDATE SET settings_json = \
             org_configs.settings_json || '{\"verification_action_enabled\": true}'",
    )
    .bind(org_id)
    .execute(&app.db)
    .await
    .expect("turn the verification switch on");
}

async fn status(client: &helpers::AuthenticatedClient, org_id: Uuid) -> Value {
    let response = client
        .get(&format!("/api/v1/orgs/{org_id}/home/base-reverification"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    body["data"].clone()
}

async fn start(
    client: &helpers::AuthenticatedClient,
    source_id: Uuid,
    kind: &str,
    key: &str,
) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{source_id}/clone"),
            &json!({
                "name": "base-reverification",
                "idempotencyKey": key,
                "uniquifyName": true,
                "reverification": {"kind": kind},
            }),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn run_count(app: &TestApp, org_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM base_reverification_runs WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A merged pull request into Base, as the merge's receipt reconciliation
/// records one.
async fn merged_pull_request(
    app: &TestApp,
    org_id: Uuid,
    source: Uuid,
    base: Uuid,
    title: &str,
    by: Uuid,
) {
    sqlx::query(
        "INSERT INTO workspace_pull_requests (id, org_id, source_workspace_id, \
             target_workspace_id, status, local_merge_status, provider_status, title, \
             last_preflight_fingerprint, created_by, merged_by, merged_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'merged', 'merged', 'local_only', $4, \
             'reverification-test', $5, $5, now())",
    )
    .bind(org_id)
    .bind(source)
    .bind(base)
    .bind(title)
    .bind(by)
    .execute(&app.db)
    .await
    .expect("record a merged pull request");
}

/// Commit `files` on Base's `main` branch, moving Base's head.
fn commit_on_base(path: &std::path::Path, files: &[(&str, &str)], message: &str) -> String {
    for (file, contents) in files {
        let target = path.join(file);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(path)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.test"])
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", message]);
    git(&["rev-parse", "HEAD"])
}

fn base_head(path: &std::path::Path) -> String {
    g5_scm::GitRepo::open(path)
        .unwrap()
        .read_branch_oid("main")
        .unwrap()
        .expect("Base main branch")
}

#[tokio::test]
async fn the_status_and_starts_wait_for_the_switch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let client = app.authenticated_client(&owner);

    // Off: Home learns only that, and a start is refused before any fork.
    assert_eq!(status(&client, org.id).await, json!({"enabled": false}));
    let (code, body) = start(&client, base.id, "full", "off-1").await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("turned off"),
        "the refusal says why: {body}"
    );
    assert_eq!(run_count(&app, org.id).await, 0);
    let forks: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_forks WHERE org_id = $1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(forks, 0, "a refused start forks nothing");

    // On: never re-verified, nothing in progress, and the owner may start.
    switch_on(&app, org.id).await;
    let on = status(&client, org.id).await;
    assert_eq!(on["enabled"], true, "{on}");
    assert_eq!(on["state"], "never");
    assert_eq!(on["canStart"], true);
    assert_eq!(on["baseWorkspaceId"], base.id.to_string());
    assert_eq!(on["pullRequestCount"], 0);
    assert_eq!(on["lines"], Value::Null);
    assert_eq!(on["runs"], json!([]));

    app.cleanup().await;
}

#[tokio::test]
async fn the_first_run_is_full_and_only_one_runs_at_a_time() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);
    let base_path = app.repo_path(base.repo_id).await;
    let frozen = base_head(&base_path);

    // Incremental needs a completed run first; the fork is rolled back with it.
    let (code, body) = start(&client, base.id, "incremental", "inc-1").await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(run_count(&app, org.id).await, 0);
    let live: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE org_id = $1 AND NOT is_base")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(live, 0, "a refused run leaves no workspace");

    let (code, body) = start(&client, base.id, "full", "full-1").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let run = body["data"]["reverificationRun"].clone();
    let workspace = body["data"]["workspace"].clone();
    assert_eq!(run["kind"], "full", "{run}");
    assert_eq!(run["state"], "in_progress");
    assert_eq!(run["id"], workspace["id"], "a run is its fork's workspace");
    assert_eq!(run["workspaceName"], workspace["name"]);
    assert_eq!(run["ownerUserId"], owner.id.to_string());
    assert_eq!(run["startedBy"], owner.id.to_string());
    assert_eq!(run["sinceHeads"], Value::Null);
    assert_eq!(run["workspaceDeleted"], false);
    assert_eq!(
        run["baseHeads"],
        json!([{"repoId": base.repo_id, "rootPath": "", "commit": frozen}]),
        "the run ends at the Base commits its fork froze"
    );
    let label = run["label"].as_str().unwrap();
    assert!(
        label.starts_with("Base re-verification ") && label.ends_with(" (full)"),
        "{label}"
    );
    assert!(workspace["name"]
        .as_str()
        .unwrap()
        .starts_with("base-reverification"));

    // A replay of the same request returns the same run.
    let (code, replay) = start(&client, base.id, "full", "full-1").await;
    assert_eq!(code, StatusCode::CREATED, "{replay}");
    assert_eq!(replay["data"]["reverificationRun"]["id"], run["id"]);
    assert_eq!(run_count(&app, org.id).await, 1);

    // Another start is refused while this one is in progress, naming it.
    let (code, body) = start(&client, base.id, "full", "full-2").await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string()
            .contains(workspace["name"].as_str().unwrap()),
        "{body}"
    );
    assert_eq!(run_count(&app, org.id).await, 1);

    let seen = status(&client, org.id).await;
    assert_eq!(seen["state"], "in_progress", "{seen}");
    assert_eq!(seen["inProgress"]["id"], run["id"]);
    assert_eq!(seen["lastCompleted"], Value::Null);
    assert_eq!(seen["runs"].as_array().unwrap().len(), 1);

    app.cleanup().await;
}

#[tokio::test]
async fn deleting_the_run_workspace_abandons_it_and_frees_the_next_start() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);

    let (code, body) = start(&client, base.id, "full", "del-1").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let first = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let deleted = client
        .post_json(&format!("/api/v1/workspaces/{first}/delete"), &json!({}))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let seen = status(&client, org.id).await;
    assert_eq!(seen["state"], "never", "{seen}");
    assert_eq!(seen["inProgress"], Value::Null);
    assert_eq!(seen["runs"][0]["state"], "abandoned");
    assert_eq!(seen["runs"][0]["workspaceDeleted"], true);
    assert_eq!(
        seen["runs"][0]["id"], first,
        "the record stays after deletion"
    );

    let (code, body) = start(&client, base.id, "full", "del-2").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_ne!(body["data"]["workspace"]["id"], first);

    app.cleanup().await;
}

#[tokio::test]
async fn a_merged_run_completes_and_the_status_counts_what_base_gained_since() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);
    let base_path = app.repo_path(base.repo_id).await;

    let (code, body) = start(&client, base.id, "full", "merge-1").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let run = body["data"]["reverificationRun"].clone();
    let run_workspace: Uuid = run["workspaceId"].as_str().unwrap().parse().unwrap();
    let fork_commit = run["baseHeads"][0]["commit"].clone();

    // While Base gains a pull request, the run is still in progress; that
    // merge happened after the fork and stays for the next run.
    let other = TestWorkspace::create(&app, &org).await;
    commit_on_base(
        &base_path,
        &[
            (
                "src/feature.rs",
                "fn one() {}\nfn two() {}\nfn three() {}\n",
            ),
            (".g5/ontology/feature.g5.md", "a\nb\nc\nd\ne\n"),
            (
                ".g5/TLA_verification/models/m/M.tla",
                "---- MODULE M ----\n====\n",
            ),
        ],
        "Feature merged during the run",
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    merged_pull_request(
        &app,
        org.id,
        other.id,
        base.id,
        "Feature during the run",
        owner.id,
    )
    .await;

    // The run's own pull request merges into Base: the run completes, and its
    // own pull request is never counted as unverified work.
    merged_pull_request(
        &app,
        org.id,
        run_workspace,
        base.id,
        "Re-verify Base",
        owner.id,
    )
    .await;

    let seen = status(&client, org.id).await;
    assert_eq!(seen["state"], "stale", "{seen}");
    assert_eq!(seen["inProgress"], Value::Null);
    assert_eq!(seen["lastCompleted"]["id"], run["id"]);
    assert_eq!(seen["lastCompleted"]["state"], "completed");
    assert!(seen["lastCompleted"]["completedAt"].is_string());
    assert_eq!(seen["pullRequestCount"], 1, "{seen}");
    assert_eq!(seen["pullRequests"][0]["title"], "Feature during the run");
    assert!(seen["pullRequests"][0]["number"].as_i64().unwrap() > 0);
    // Lines outside `.g5/` only: the three source lines.
    assert_eq!(seen["lines"], json!({"added": 3, "removed": 0}), "{seen}");
    assert_eq!(seen["linesUnavailable"], false);

    // The next incremental run continues from the completed run's commits.
    let (code, body) = start(&client, base.id, "incremental", "merge-2").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let next = body["data"]["reverificationRun"].clone();
    assert_eq!(next["kind"], "incremental");
    assert_eq!(next["sinceHeads"][0]["commit"], fork_commit);
    assert_eq!(next["baseHeads"][0]["commit"], base_head(&base_path));
    assert!(next["label"].as_str().unwrap().ends_with(" (incremental)"));

    app.cleanup().await;
}

#[tokio::test]
async fn a_completed_run_with_nothing_since_reads_current() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);

    let (code, body) = start(&client, base.id, "full", "current-1").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let run_workspace: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    // A pull request into another parent completes nothing.
    let other = TestWorkspace::create(&app, &org).await;
    merged_pull_request(
        &app,
        org.id,
        run_workspace,
        other.id,
        "Not into Base",
        owner.id,
    )
    .await;
    assert_eq!(status(&client, org.id).await["state"], "in_progress");

    merged_pull_request(
        &app,
        org.id,
        run_workspace,
        base.id,
        "Re-verify Base",
        owner.id,
    )
    .await;
    let seen = status(&client, org.id).await;
    assert_eq!(seen["state"], "current", "{seen}");
    assert_eq!(seen["pullRequestCount"], 0);
    assert_eq!(seen["lines"], json!({"added": 0, "removed": 0}));

    app.cleanup().await;
}

#[tokio::test]
async fn only_forks_of_base_by_eligible_members_start_runs() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let ordinary = TestWorkspace::create(&app, &org).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);

    let (code, body) = start(&client, ordinary.id, "full", "not-base").await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(run_count(&app, org.id).await, 0);

    // A viewer reads the status but cannot fork Base, so cannot start.
    let viewer = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role = 'viewer' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(viewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let viewer_client = app.authenticated_client(&viewer);
    let seen = status(&viewer_client, org.id).await;
    assert_eq!(seen["enabled"], true, "{seen}");
    assert_eq!(seen["canStart"], false);
    let (code, body) = start(&viewer_client, base.id, "full", "viewer").await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(run_count(&app, org.id).await, 0);

    // An ordinary fork of Base is not a run.
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", base.id),
            &json!({"name": "plain-fork", "idempotencyKey": "plain"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    assert!(body["data"].get("reverificationRun").is_none(), "{body}");
    assert_eq!(run_count(&app, org.id).await, 0);

    app.cleanup().await;
}

#[tokio::test]
async fn a_run_reaches_base_only_through_a_pull_request() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    switch_on(&app, org.id).await;
    // The organization owner owns Base, so a direct merge would be admitted.
    let client = app.authenticated_client(&owner);

    let (code, body) = start(&client, base.id, "full", "direct-1").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let run_workspace = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let merged = client
        .post_json(
            &format!("/api/v1/workspaces/{run_workspace}/merge"),
            &json!({}),
        )
        .await;
    assert_eq!(merged.status(), StatusCode::CONFLICT);
    let body: Value = merged.json().await.unwrap();
    assert!(body.to_string().contains("pull request"), "{body}");
    assert_eq!(status(&client, org.id).await["state"], "in_progress");

    // An ordinary fork still merges directly as before.
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", base.id),
            &json!({"name": "plain-direct", "idempotencyKey": "plain-direct"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let plain: Value = response.json().await.unwrap();
    let plain_id = plain["data"]["workspace"]["id"].as_str().unwrap();
    let merged = client
        .post_json(&format!("/api/v1/workspaces/{plain_id}/merge"), &json!({}))
        .await;
    assert_eq!(
        merged.status(),
        StatusCode::OK,
        "{}",
        merged.text().await.unwrap()
    );

    app.cleanup().await;
}

/// Add a code repository to Base whose `main` holds `files`.
async fn add_code_repo(
    app: &TestApp,
    base: &TestWorkspace,
    root: &str,
    files: &[(&str, &str)],
) -> Uuid {
    let repo_id = Uuid::new_v4();
    let path = app
        .repo_storage
        .init_repo(
            &g5_api_core::services::git::repo_store::TenantRepoStorage::test_key(
                base.org_id,
                repo_id,
            ),
        )
        .await
        .expect("init code repository");
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(&path)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.test"])
            .args(args)
            .output()
            .expect("run git");
        assert!(output.status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["checkout", "-q", "-B", "main"]);
    commit_on_base(&path, files, "seed");
    let slug = format!("web-{}", &repo_id.to_string()[..8]);
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) \
         VALUES ($1, $2, $3, $3, $4, 'main')",
    )
    .bind(repo_id)
    .bind(base.org_id)
    .bind(&slug)
    .bind(path.display().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role) \
         VALUES ($1, $2, $3, $4, 'code')",
    )
    .bind(base.org_id)
    .bind(base.id)
    .bind(repo_id)
    .bind(root)
    .execute(&app.db)
    .await
    .unwrap();
    repo_id
}

/// A run whose fork is still pending and created no workspace, made from a
/// real run's fork record, `minutes_old` minutes ago, under the key `name`.
/// With `same_request`, it carries the model start's request, so the same
/// request finds it, as a retry of a real start would.
async fn pending_run(
    app: &TestApp,
    model_run: &str,
    name: &str,
    minutes_old: i32,
    same_request: bool,
) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_forks (id, org_id, source_workspace_id, requested_by, \
             owner_user_id, idempotency_key, request_digest, proposed_name, status, \
             repo_manifest, created_at) \
         SELECT gen_random_uuid(), org_id, source_workspace_id, requested_by, owner_user_id, \
             $2, CASE WHEN $4 THEN request_digest ELSE repeat('a', 64) END, $2, 'pending', \
             repo_manifest, now() - make_interval(mins => $3) \
         FROM workspace_forks WHERE id = $1::text::uuid RETURNING id",
    )
    .bind(model_run)
    .bind(name)
    .bind(minutes_old)
    .bind(same_request)
    .fetch_one(&app.db)
    .await
    .expect("record a pending fork");
    sqlx::query(
        "INSERT INTO base_reverification_runs (id, org_id, kind, base_workspace_id, \
             workspace_id, started_by, base_heads, created_at) \
         SELECT $1, org_id, 'full', base_workspace_id, $1, started_by, base_heads, \
             now() - make_interval(mins => $3) \
         FROM base_reverification_runs WHERE id = $2::text::uuid",
    )
    .bind(id)
    .bind(model_run)
    .bind(minutes_old)
    .execute(&app.db)
    .await
    .expect("record its run");
    id
}

#[tokio::test]
async fn concurrent_starts_admit_exactly_one_run() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);
    let member_client = app.authenticated_client(&member);

    // Two people start at once: one run, the other refused.
    let (first, second) = tokio::join!(
        start(&client, base.id, "full", "race-a"),
        start(&member_client, base.id, "full", "race-b"),
    );
    let mut codes = vec![first.0, second.0];
    codes.sort();
    assert_eq!(
        codes,
        vec![StatusCode::CREATED, StatusCode::CONFLICT],
        "{} / {}",
        first.1,
        second.1
    );
    assert_eq!(run_count(&app, org.id).await, 1);

    // The admitted fork has finished, so an identical start under a new key
    // is another start: refused while that run is in progress, whoever asks,
    // naming its workspace.
    let admitted = if first.0 == StatusCode::CREATED {
        &first.1
    } else {
        &second.1
    };
    let name = admitted["data"]["workspace"]["name"].as_str().unwrap();
    for (who, key) in [(&client, "race-c"), (&member_client, "race-d")] {
        let (code, again) = start(who, base.id, "full", key).await;
        assert_eq!(code, StatusCode::CONFLICT, "{again}");
        assert!(again.to_string().contains(name), "{again}");
    }
    assert_eq!(run_count(&app, org.id).await, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn a_fork_that_never_finished_stops_blocking_and_the_next_start_aborts_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);

    let (code, body) = start(&client, base.id, "full", "model").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let model = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let deleted = client
        .post_json(&format!("/api/v1/workspaces/{model}/delete"), &json!({}))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    // A fork pending for minutes is still being created: it blocks starts,
    // and Home names its starter.
    let fresh = pending_run(&app, &model, "fresh-run", 5, false).await;
    let seen = status(&client, org.id).await;
    assert_eq!(seen["state"], "in_progress", "{seen}");
    assert_eq!(seen["inProgress"]["id"], fresh.to_string());
    assert_eq!(seen["inProgress"]["workspaceName"], Value::Null);
    assert_eq!(seen["inProgress"]["ownerUserId"], owner.id.to_string());
    let (code, body) = start(&client, base.id, "full", "blocked").await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("fresh-run"), "{body}");

    // Past the bound it has failed: the run reads as abandoned, and the next
    // start aborts the fork so it can never finish afterwards.
    sqlx::query(
        "UPDATE workspace_forks SET created_at = now() - interval '31 minutes' WHERE id = $1",
    )
    .bind(fresh)
    .execute(&app.db)
    .await
    .unwrap();
    let seen = status(&client, org.id).await;
    assert_eq!(seen["state"], "never", "{seen}");
    assert_eq!(seen["inProgress"], Value::Null);
    let stale = seen["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["id"] == fresh.to_string())
        .unwrap()
        .clone();
    assert_eq!(stale["state"], "abandoned");

    // Even a retry of a failed fork, which carries its key and request,
    // aborts it rather than finishing it, with the other failed forks.
    let retried = pending_run(&app, &model, "retried-run", 31, true).await;
    let (code, body) = start(&client, base.id, "full", "retried-run").await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("did not finish"), "{body}");
    let fork_status = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT status FROM workspace_forks WHERE id = $1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(fork_status(retried).await, "aborted");
    assert_eq!(fork_status(fresh).await, "aborted");
    let seen = status(&client, org.id).await;
    assert_eq!(seen["state"], "never", "{seen}");

    // A new key with the same request starts a new run rather than finishing
    // a failed fork it would otherwise find by that request.
    let other = pending_run(&app, &model, "other-run", 31, true).await;
    let (code, body) = start(&client, base.id, "full", "after-stale").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let started = body["data"]["workspace"]["id"].as_str().unwrap();
    assert_ne!(started, other.to_string());
    assert_eq!(body["data"]["reverificationRun"]["id"], started);
    assert_eq!(fork_status(other).await, "aborted");

    app.cleanup().await;
}

#[tokio::test]
async fn a_run_takes_no_other_fork_options_and_its_key_names_one_kind() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);

    for (key, extra) in [
        ("with-chat", json!({"startChat": {"content": "go"}})),
        ("with-target", json!({"mergeTargetWorkspaceId": other.id})),
    ] {
        let mut request = json!({
            "name": "base-reverification",
            "idempotencyKey": key,
            "uniquifyName": true,
            "reverification": {"kind": "full"},
        });
        request
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let response = client
            .post_json(&format!("/api/v1/workspaces/{}/clone", base.id), &request)
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{key}");
    }
    assert_eq!(run_count(&app, org.id).await, 0);

    let (code, body) = start(&client, base.id, "full", "one-key").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    // The same key for another kind is another request, refused.
    let (code, body) = start(&client, base.id, "incremental", "one-key").await;
    assert!(code.is_client_error(), "{code} {body}");
    assert_eq!(run_count(&app, org.id).await, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn lines_count_code_repositories_only_and_a_new_one_from_empty() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let docs_repo = base.add_docs_repo(&app).await;
    switch_on(&app, org.id).await;
    let client = app.authenticated_client(&owner);
    let base_path = app.repo_path(base.repo_id).await;

    let (code, body) = start(&client, base.id, "full", "lines-1").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let run_workspace: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        body["data"]["reverificationRun"]["baseHeads"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the docs repository is not part of a run's commits"
    );
    merged_pull_request(
        &app,
        org.id,
        run_workspace,
        base.id,
        "Re-verify Base",
        owner.id,
    )
    .await;

    // Two lines of code, ten lines of docs, and a code repository added after
    // the run with four lines across two files.
    commit_on_base(&base_path, &[("src/a.rs", "one\ntwo\n")], "code");
    base.commit_file_in_repo(
        &app,
        docs_repo,
        "notes/n.md",
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n",
    );
    add_code_repo(
        &app,
        &base,
        "web",
        &[("README.md", "web\n"), ("app.ts", "a\nb\nc\n")],
    )
    .await;

    // A later pull request from the completed run's own workspace counts.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    merged_pull_request(&app, org.id, run_workspace, base.id, "Follow-up", owner.id).await;

    let seen = status(&client, org.id).await;
    assert_eq!(seen["lines"], json!({"added": 6, "removed": 0}), "{seen}");
    assert_eq!(seen["linesUnavailable"], false);
    assert_eq!(seen["pullRequestCount"], 1, "{seen}");
    assert_eq!(seen["pullRequests"][0]["title"], "Follow-up");
    assert_eq!(seen["runCount"], 1);

    app.cleanup().await;
}
