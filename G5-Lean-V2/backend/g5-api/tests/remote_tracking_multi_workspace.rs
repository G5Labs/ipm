//! Multi-workspace remote-tracking integration tests.
//!
//! The tests use the real exact-name fork plus remote-promotion routes and a
//! real local bare git remote. That keeps `ls-remote`, branch bootstrap and
//! co-tracking durable-lease behavior on the same paths production uses.

use crate::helpers;

use g5_api::services::remote_integrations;
use g5_auth::Scope;
use g5_contracts::types::WorkspaceRepoKey;
use g5_scm::GitRepo;
use helpers::{seed_workspace_authority_head, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|err| panic!("failed to spawn git {args:?} in {dir:?}: {err}"));
    assert!(
        output.status.success(),
        "git {args:?} in {dir:?} failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

struct RemoteRepoFixture {
    repo_id: Uuid,
    bare: PathBuf,
    _tmp_root: PathBuf,
}

async fn create_local_remote_repo(app: &TestApp, org: &TestOrg) -> RemoteRepoFixture {
    let tmp_root = std::env::temp_dir().join(format!("g5-rt-mw-{}", Uuid::new_v4().simple()));
    let bare = tmp_root.join("remote.git");
    let seed = tmp_root.join("seed");
    std::fs::create_dir_all(&bare).expect("create bare repo dir");
    std::fs::create_dir_all(&seed).expect("create seed repo dir");

    git(&bare, &["init", "--bare", "--initial-branch=main"]);
    git(&seed, &["init", "--initial-branch=main"]);
    git(&seed, &["config", "user.email", "g5@test.local"]);
    git(&seed, &["config", "user.name", "G5 Test"]);
    std::fs::write(seed.join("README.md"), "remote tracking multi-workspace\n")
        .expect("seed README");
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "seed"]);
    git(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&seed, &["push", "origin", "main"]);
    git(&bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let repo_id = Uuid::new_v4();
    let slug = format!("rt-multi-{}", repo_id.simple());
    sqlx::query(
        r#"
        INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider)
        VALUES ($1, $2, $3, $4, $5, 'main', NULL)
        "#,
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(&slug)
    .bind(&slug)
    .bind(bare.display().to_string())
    .execute(&app.db)
    .await
    .expect("insert local bare remote repo");

    let mirror = app.repo_path(repo_id).await;
    if let Some(parent) = mirror.parent() {
        std::fs::create_dir_all(parent).expect("create mirror parent dir");
    }
    git(
        mirror.parent().expect("mirror parent directory"),
        &[
            "clone",
            bare.to_str().expect("bare path"),
            mirror.to_str().expect("mirror path"),
        ],
    );
    git(&mirror, &["config", "user.email", "g5@test.local"]);
    git(&mirror, &["config", "user.name", "G5 Test"]);

    RemoteRepoFixture {
        repo_id,
        bare,
        _tmp_root: tmp_root,
    }
}

impl RemoteRepoFixture {
    fn remote_tip(&self) -> String {
        git(&self.bare, &["rev-parse", "refs/heads/main"])
    }
}

struct FixtureResponse {
    status: StatusCode,
    body: Value,
}

impl FixtureResponse {
    fn status(&self) -> StatusCode {
        self.status
    }

    async fn json(self) -> Result<Value, serde_json::Error> {
        Ok(self.body)
    }
}

/// Create a stopped workspace through the sole exact-name fork path, then use
/// the retained public promotion route to bind that pinned member to `main`.
async fn create_remote_tracking_workspace(
    app: &TestApp,
    user: &TestUser,
    org: &TestOrg,
    repo_id: Uuid,
    name: &str,
    exact_branch_name: Option<&str>,
) -> FixtureResponse {
    let client = app.authenticated_client(user);
    // Make Remote is an organization-administrator action (#1096); this fixture
    // establishes remote tracking through the promote endpoint, so its owner
    // must be an organization administrator.
    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .expect("elevate fixture owner to org admin for Make Remote");
    let exact_name = exact_branch_name.unwrap_or(name);
    let source = TestWorkspace::create(app, org).await;
    // The retained Make Remote preflight validates the complete docs/code set.
    sqlx::query("UPDATE workspace_repos SET role='docs',root_path='docs' WHERE workspace_id=$1 AND repo_id=$2")
        .bind(source.id).bind(source.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
        .bind(source.id)
        .bind(source.repo_id)
        .execute(&app.db)
        .await
        .unwrap();

    let mirror = app.repo_path(repo_id).await;
    let head = git(&mirror, &["rev-parse", "refs/heads/main"]);
    let source_branch = source.name.clone();
    git(
        &mirror,
        &["update-ref", &format!("refs/heads/{source_branch}"), &head],
    );
    let root_path: String = sqlx::query_scalar("SELECT slug FROM repos WHERE id = $1")
        .bind(repo_id)
        .fetch_one(&app.db)
        .await
        .expect("repository slug");
    sqlx::query(
        "INSERT INTO workspace_repos \
             (org_id, workspace_id, repo_id, root_path, role) \
         VALUES ($1, $2, $3, $4, 'code')",
    )
    .bind(org.id)
    .bind(source.id)
    .bind(repo_id)
    .bind(&root_path)
    .execute(&app.db)
    .await
    .expect("insert source repository mapping");
    let fork = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", source.id),
            &json!({
                "name": exact_name,
                "idempotencyKey": format!("remote-tracking-{exact_name}-{}", Uuid::new_v4())
            }),
        )
        .await;
    let fork_status = fork.status();
    let fork_text = fork.text().await.expect("fork response body");
    assert_eq!(fork_status, StatusCode::CREATED, "{fork_text}");
    let fork_body: Value = serde_json::from_str(&fork_text).expect("fork response json");
    let workspace_id = Uuid::parse_str(
        fork_body["data"]["workspace"]["id"]
            .as_str()
            .expect("fork workspace id"),
    )
    .expect("fork workspace uuid");
    let promoted = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/repos/{repo_id}/remote_tracking"),
            &json!({ "remoteBranch": "main" }),
        )
        .await;
    let promoted_status = promoted.status();
    let promoted_text = promoted.text().await.expect("promotion response body");
    assert_eq!(promoted_status, StatusCode::OK, "{promoted_text}");

    // These fixtures exercise passive background inbound cycles. Promotion is
    // deliberately opt-in, so make that test intent explicit.
    sqlx::query(
        r#"
        UPDATE remote_integrations
           SET auto_pull_enabled = true
         WHERE workspace_id = $1 AND repo_id = $2
        "#,
    )
    .bind(workspace_id)
    .bind(repo_id)
    .execute(&app.db)
    .await
    .expect("enable passive auto-pull for background fixture");

    let detail = client
        .get(&format!("/api/v1/workspaces/{workspace_id}"))
        .await;
    let detail_status = detail.status();
    let detail_text = detail.text().await.expect("workspace detail body");
    assert_eq!(detail_status, StatusCode::OK, "{detail_text}");
    let detail_body: Value = serde_json::from_str(&detail_text).expect("workspace detail json");
    FixtureResponse {
        status: StatusCode::CREATED,
        body: detail_body,
    }
}

struct TrackedWorkspaceBranch {
    workspace_id: Uuid,
    repo_id: Uuid,
    org_id: Uuid,
    head_commit: String,
    local_branch_name: String,
}
impl TrackedWorkspaceBranch {
    fn key(&self) -> WorkspaceRepoKey {
        WorkspaceRepoKey {
            org_id: self.org_id,
            workspace_id: self.workspace_id,
            repo_id: self.repo_id,
        }
    }
}
async fn tracked_workspace_branch(app: &TestApp, workspace_id: Uuid) -> TrackedWorkspaceBranch {
    let (org_id, repo_id, local_branch_name): (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT w.org_id, i.repo_id, w.name FROM workspaces w JOIN remote_integrations i ON i.workspace_id=w.id WHERE w.id=$1")
        .bind(workspace_id).fetch_one(&app.db).await.unwrap();
    let path = app.repo_path(repo_id).await;
    let head_commit = git(
        &path,
        &["rev-parse", &format!("refs/heads/{local_branch_name}")],
    );
    TrackedWorkspaceBranch {
        workspace_id,
        repo_id,
        org_id,
        local_branch_name,
        head_commit,
    }
}
async fn member_state(app: &TestApp, key: WorkspaceRepoKey) -> (String, String, Option<String>) {
    let mut db = app.scoped_db_for_org(key.org_id).await;
    let policy = remote_integrations::load(db.conn(), key, false)
        .await
        .unwrap()
        .unwrap();
    let observed = remote_integrations::observed_tip(db.conn(), &policy)
        .await
        .unwrap()
        .map(|o| o.oid);
    db.rollback().await.unwrap();
    let path = app.repo_path(key.repo_id).await;
    (
        git(
            &path,
            &["rev-parse", &format!("refs/heads/{}", policy.local_branch)],
        ),
        GitRepo::open(&path)
            .unwrap()
            .read_sync_watermark(&policy.local_branch)
            .unwrap()
            .unwrap(),
        observed,
    )
}

async fn enqueue_integration(
    app: &TestApp,
    key: WorkspaceRepoKey,
    org_id: Uuid,
    trigger_kind: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO remote_integration_queue (workspace_id, repo_id, org_id, trigger_kind, requested_by_user_id, lifecycle_generation)
        SELECT $1, $2, $3, $4,
               CASE WHEN $4 IN ('push','approved_rebase_publish') THEN owner_user_id END,
               CASE WHEN $4 IN ('push','approved_rebase_publish') THEN lifecycle_generation END
        FROM workspaces WHERE id=$1
        ON CONFLICT (workspace_id, repo_id) DO UPDATE
            SET trigger_kind = EXCLUDED.trigger_kind,
                requested_by_user_id = EXCLUDED.requested_by_user_id,
                lifecycle_generation = EXCLUDED.lifecycle_generation,
                requested_at = now(),
                locked_by = NULL,
                locked_until = NULL
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(org_id)
    .bind(trigger_kind)
    .execute(&app.db)
    .await
    .expect("enqueue integration row");
}

fn worker_deps(
    app: &TestApp,
) -> g5_api::maintenance::branch_integration_worker::BranchIntegrationWorkerDeps {
    g5_api::maintenance::branch_integration_worker::BranchIntegrationWorkerDeps {
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
        repository_credentials: app.state.repository_credentials.clone(),
        event_bus: app.state.event_bus.clone(),
        landed_commit_notifier: std::sync::Arc::new(
            g5_api::services::review_source_notifications::ReviewSourceNotifier::from_state(
                &app.state,
            ),
        ),
    }
}

async fn queue_row_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(key.workspace_id).bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count queue rows")
}

async fn pushed_run_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_sync_runs \
         WHERE workspace_id = $1 AND repo_id = $2 AND status = 'pushed'",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count pushed runs")
}

/// Claims the same durable generation-fenced remote-pair lease used by the
/// worker. The row is coordination only and contains no authority SHA.
async fn harness_hold_remote_pair_lease(
    app: &TestApp,
    repo_id: Uuid,
    org_id: Uuid,
    remote_branch: &str,
) -> Uuid {
    let owner_token = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO remote_ref_leases (
            repo_id, remote_branch, org_id, owner_token,
            lease_generation, lease_expires_at
        ) VALUES ($1, $2, $3, $4, 1, now() + INTERVAL '5 minutes')
        "#,
    )
    .bind(repo_id)
    .bind(remote_branch)
    .bind(org_id)
    .bind(owner_token)
    .execute(&app.db)
    .await
    .expect("harness claims durable remote-pair lease");
    owner_token
}

async fn harness_release_remote_pair_lease(
    app: &TestApp,
    repo_id: Uuid,
    remote_branch: &str,
    owner_token: Uuid,
) {
    let released = sqlx::query(
        r#"
        DELETE FROM remote_ref_leases
        WHERE repo_id = $1
          AND remote_branch = $2
          AND owner_token = $3
          AND lease_generation = 1
        "#,
    )
    .bind(repo_id)
    .bind(remote_branch)
    .bind(owner_token)
    .execute(&app.db)
    .await
    .expect("release durable remote-pair lease");
    assert_eq!(
        released.rows_affected(),
        1,
        "harness must own the lease it releases"
    );
}

async fn drain_one_cycle(app: &TestApp, key: WorkspaceRepoKey) {
    use g5_api::maintenance::branch_integration_worker::run_branch_integration_worker;

    let cancel = CancellationToken::new();
    let handle = tokio::spawn(run_branch_integration_worker(
        worker_deps(app),
        cancel.clone(),
    ));
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if queue_row_count(app, key).await == 0 {
            break;
        }
        if Instant::now() > deadline {
            cancel.cancel();
            let _ = handle.await;
            panic!("worker did not drain queue row within 45s");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    cancel.cancel();
    let _ = handle.await;
}

async fn latest_run_status(app: &TestApp, key: WorkspaceRepoKey) -> String {
    sqlx::query_scalar(
        r#"
        SELECT status
        FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("latest sync run status")
}

async fn latest_run_status_for_trigger(
    app: &TestApp,
    key: WorkspaceRepoKey,
    trigger_kind: &str,
) -> String {
    sqlx::query_scalar(
        r#"
        SELECT status
        FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2
          AND trigger_kind = $3
        ORDER BY started_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(trigger_kind)
    .fetch_one(&app.db)
    .await
    .expect("latest trigger-correlated sync run status")
}

/// Ensure the worker's shared local mirror for `repo_id` exists, is a clone of
/// the bare remote, and has a committer identity configured. Returns the mirror
/// path. The worker keys the mirror by `repo_id`, so all co-tracking workspaces
/// share this one checkout (and therefore one `refs/remotes/origin/<branch>`),
/// which is exactly what makes the durable per-`(repo_id, remote_branch)` lease
/// the right serialization primitive.
async fn ensure_shared_mirror(app: &TestApp, repo: &RemoteRepoFixture) -> PathBuf {
    let mirror = app.repo_path(repo.repo_id).await;
    if let Some(parent) = mirror.parent() {
        std::fs::create_dir_all(parent).expect("create mirror parent dir");
    }
    if !mirror.exists() {
        git(
            mirror
                .parent()
                .expect("mirror parent for clone working directory"),
            &[
                "clone",
                repo.bare.to_str().expect("bare path"),
                mirror.to_str().expect("mirror path"),
            ],
        );
    }
    git(&mirror, &["config", "user.email", "g5@test.local"]);
    git(&mirror, &["config", "user.name", "G5 Test"]);
    mirror
}

/// Create a divergent local commit for `wbr_id`: a child of `parent_commit`
/// that touches `file` (sibling commits made from the same parent with distinct
/// file names do NOT share each other in ancestry, so two such tips diverge from
/// each other). Materializes the commit in the shared mirror's object DB so the
/// worker's `graph_ahead_behind` can resolve it, advances the workspace's
/// `head_commit`, and returns the new local tip SHA.
async fn commit_divergent_local_tip(
    app: &TestApp,
    mirror: &Path,
    key: WorkspaceRepoKey,
    parent_commit: &str,
    file: &str,
    contents: &str,
) -> String {
    let work_branch = format!("local-{}", Uuid::new_v4().simple());
    git(mirror, &["checkout", "-B", &work_branch, parent_commit]);
    std::fs::write(mirror.join(file), contents).unwrap_or_else(|err| {
        panic!("write divergent local file {file}: {err}");
    });
    git(mirror, &["add", file]);
    git(
        mirror,
        &["commit", "-m", &format!("local divergent {file}")],
    );
    let tip = git(mirror, &["rev-parse", "HEAD"]);

    seed_workspace_authority_head(app, key.workspace_id, key.repo_id, &tip).await;
    tip
}

/// Push a fresh disjoint commit onto the bare remote's `main`, so the worker's
/// `ls-remote`-first fetch sees it as the authoritative mirror tip. Returns the
/// new remote tip SHA. Used to drive a true three-way divergence (the remote
/// moves to a commit neither workspace has) and to stand in for a peer
/// workspace's merge commit `M_A`.
fn advance_remote_main(mirror: &Path, parent_commit: &str, file: &str, contents: &str) -> String {
    let push_branch = format!("remote-{}", Uuid::new_v4().simple());
    git(mirror, &["checkout", "-B", &push_branch, parent_commit]);
    std::fs::write(mirror.join(file), contents).unwrap_or_else(|err| {
        panic!("write remote file {file}: {err}");
    });
    git(mirror, &["add", file]);
    git(mirror, &["commit", "-m", &format!("remote {file}")]);
    let tip = git(mirror, &["rev-parse", "HEAD"]);
    git(
        mirror,
        &[
            "push",
            "--force",
            "origin",
            &format!("{tip}:refs/heads/main"),
        ],
    );
    tip
}

#[tokio::test]
async fn co_tracking_with_distinct_local_branch_succeeds_and_reports_peer_count() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo = create_local_remote_repo(&app, &org).await;

    let first = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-a",
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let _first_body: Value = first.json().await.expect("first response json");

    let second = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "Remote Tracking B",
        Some("main-b"),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_body: Value = second.json().await.expect("second response json");
    let branch = &second_body["data"]["branches"][0];

    assert_eq!(branch["trackingMode"].as_str(), Some("remote_tracking"));
    assert_eq!(branch["remoteBranch"].as_str(), Some("main"));
    // The exact fork name is distinct from the remote branch name.
    let b_local = branch["branchName"]
        .as_str()
        .expect("canonical local branch name");
    assert!(!b_local.is_empty() && b_local != "main");
    assert_eq!(branch["coTrackingWorkspaceCount"].as_i64(), Some(1));

    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT w.id, w.name, i.remote_branch FROM remote_integrations i JOIN workspaces w ON w.id=i.workspace_id WHERE i.repo_id=$1 ORDER BY w.name")
        .bind(repo.repo_id).fetch_all(&app.db).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].1, "main-b");
    assert_eq!(rows[1].1, "remote-tracking-a");
    for (workspace_id, _, remote_branch) in rows {
        assert_eq!(remote_branch, "main");
        let member = tracked_workspace_branch(&app, workspace_id).await;
        let (head, watermark, _) = member_state(&app, member.key()).await;
        assert_eq!(head, repo.remote_tip());
        assert_eq!(watermark, head);
        let bootstrap_audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 AND trigger_kind='manual' AND status='noop' AND new_baseline=$3")
            .bind(workspace_id).bind(repo.repo_id).bind(&head).fetch_one(&app.db).await.unwrap();
        assert_eq!(bootstrap_audits, 1);
    }
    app.cleanup().await;
}

#[tokio::test]
async fn workspace_detail_reports_live_co_tracking_workspace_count() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let repo = create_local_remote_repo(&app, &org).await;

    let first = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-a",
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body: Value = first.json().await.expect("first response json");
    let first_workspace_id = first_body["data"]["id"]
        .as_str()
        .expect("first workspace id")
        .to_string();

    let second = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "Remote Tracking B",
        Some("main-b"),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);

    let third = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "Remote Tracking C",
        Some("main-c"),
    )
    .await;
    assert_eq!(third.status(), StatusCode::CREATED);
    let third_body: Value = third.json().await.expect("third response json");
    let third_workspace_id = third_body["data"]["id"]
        .as_str()
        .expect("third workspace id")
        .to_string();

    let detail = client
        .get(&format!("/api/v1/workspaces/{third_workspace_id}"))
        .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body: Value = detail.json().await.expect("detail response json");
    let branch = &detail_body["data"]["branches"][0];
    assert_eq!(branch["trackingMode"].as_str(), Some("remote_tracking"));
    assert_eq!(branch["coTrackingWorkspaceCount"].as_i64(), Some(2));
    assert!(
        branch.get("coTrackingWorkspaces").is_none(),
        "workspace detail should expose the count, not the create-time peer list"
    );

    let delete_first = client
        .post_json(
            &format!("/api/v1/workspaces/{first_workspace_id}/delete"),
            &json!({}),
        )
        .await;
    assert_eq!(delete_first.status(), StatusCode::NO_CONTENT);

    let detail_after_delete = client
        .get(&format!("/api/v1/workspaces/{third_workspace_id}"))
        .await;
    assert_eq!(detail_after_delete.status(), StatusCode::OK);
    let detail_after_delete_body: Value = detail_after_delete
        .json()
        .await
        .expect("detail response json after delete");
    let branch_after_delete = &detail_after_delete_body["data"]["branches"][0];
    assert_eq!(
        branch_after_delete["coTrackingWorkspaceCount"].as_i64(),
        Some(1)
    );

    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pushed_commit_in_one_workspace_fast_forwards_the_auto_pull_peer() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo = create_local_remote_repo(&app, &org).await;

    let first = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-a",
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body: Value = first.json().await.expect("first response json");
    let workspace_a_id =
        Uuid::parse_str(first_body["data"]["id"].as_str().expect("workspace A id"))
            .expect("workspace A uuid");

    let second = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "Remote Tracking B",
        Some("main-b"),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_body: Value = second.json().await.expect("second response json");
    let workspace_b_id =
        Uuid::parse_str(second_body["data"]["id"].as_str().expect("workspace B id"))
            .expect("workspace B uuid");

    let branch_a = tracked_workspace_branch(&app, workspace_a_id).await;
    let branch_b = tracked_workspace_branch(&app, workspace_b_id).await;
    assert_eq!(
        branch_a.head_commit, branch_b.head_commit,
        "co-tracking workspaces should bootstrap from the same remote tip"
    );

    let mut ws_a = app
        .ws_connect(&user, &format!("workspace:{workspace_a_id}"))
        .await;
    let _ = ws_a.expect_subscribed().await;
    let mut ws_b = app
        .ws_connect(&user, &format!("workspace:{workspace_b_id}"))
        .await;
    let _ = ws_b.expect_subscribed().await;

    let mirror = app.repo_path(repo.repo_id).await;
    if let Some(parent) = mirror.parent() {
        std::fs::create_dir_all(parent).expect("create mirror parent dir");
    }
    if !mirror.exists() {
        git(
            mirror
                .parent()
                .expect("mirror parent for clone working directory"),
            &[
                "clone",
                repo.bare.to_str().expect("bare path"),
                mirror.to_str().expect("mirror path"),
            ],
        );
    }
    git(&mirror, &["config", "user.email", "g5@test.local"]);
    git(&mirror, &["config", "user.name", "G5 Test"]);
    // The worker pushes `push_refspec(<local_branch_name>:<remote_branch>)`, so
    // the commit must land on W_A's canonical LOCAL branch in the shared mirror
    // (G5 assigns it; it is NOT the remote name "main") — exactly what a real
    // draft-commit cycle leaves behind.
    git(
        &mirror,
        &[
            "checkout",
            "-B",
            &branch_a.local_branch_name,
            &branch_a.head_commit,
        ],
    );
    std::fs::write(
        mirror.join("workspace-a.txt"),
        "workspace A published a change\n",
    )
    .expect("write workspace A change");
    git(&mirror, &["add", "."]);
    git(&mirror, &["commit", "-m", "workspace A change"]);
    let workspace_a_tip = git(&mirror, &["rev-parse", "HEAD"]);

    seed_workspace_authority_head(
        &app,
        branch_a.workspace_id,
        branch_a.repo_id,
        &workspace_a_tip,
    )
    .await;

    enqueue_integration(&app, branch_a.key(), org.id, "push").await;
    drain_one_cycle(&app, branch_a.key()).await;
    assert_eq!(
        latest_run_status_for_trigger(&app, branch_a.key(), "push").await,
        "pushed",
        "workspace A commit-coupled cycle must publish to the shared remote"
    );
    assert_eq!(
        repo.remote_tip(),
        workspace_a_tip,
        "customer remote should fast-forward to workspace A's commit"
    );

    enqueue_integration(&app, branch_b.key(), org.id, "background_inbound").await;
    drain_one_cycle(&app, branch_b.key()).await;

    assert_eq!(
        latest_run_status_for_trigger(&app, branch_b.key(), "background_inbound",).await,
        "imported",
        "workspace B auto-pull accepts the provider fast-forward"
    );
    let (b_child_head, b_baseline, _) = member_state(&app, branch_b.key()).await;
    assert_eq!(b_child_head, workspace_a_tip);
    assert_eq!(b_baseline, workspace_a_tip);
    assert_eq!(
        git(
            &mirror,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::provider_ref(workspace_b_id, "main")
            ]
        ),
        workspace_a_tip
    );
    let update = ws_b
        .expect_event("workspace.branch_updated", Duration::from_secs(5))
        .await;
    assert!(update.to_string().contains(&workspace_a_tip));
    let file = app
        .authenticated_client(&user)
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files/workspace-a.txt",
            branch_b.workspace_id, branch_b.repo_id
        ))
        .await;
    assert_eq!(file.status(), StatusCode::OK);
    let body: Value = file.json().await.unwrap();
    assert_eq!(body["data"]["content"], "workspace A published a change\n");
    assert_eq!(body["data"]["repoOid"], workspace_a_tip);

    app.cleanup().await;
}

#[tokio::test]
async fn co_tracking_second_workspace_succeeds_with_distinct_exact_name() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo = create_local_remote_repo(&app, &org).await;

    let first = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-a",
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let duplicate = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-duplicate",
        None,
    )
    .await;
    // The second exact fork name is distinct, so it can co-track the same
    // (repo, remote_branch) without a local branch collision.
    assert_eq!(duplicate.status(), StatusCode::CREATED);

    let row_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM remote_integrations
        WHERE repo_id = $1
          AND remote_branch = 'main'
        "#,
    )
    .bind(repo.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("remote-tracking row count should be queryable");
    assert_eq!(row_count, 2, "both co-tracking workspaces must be created");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D71 — two workspaces diverging simultaneously open two PARALLEL merge runs.
//
// W_A and W_B both track (repo_R, "main"). Each has a local commit (C_A / C_B)
// that the other does not share, and the customer remote moves to a disjoint
// C_R (true three-way divergence past the shared baseline T0). Both
// commit-coupled cycles MUST classify `diverged` independently and each MUST
// open its OWN `workspace_merge_runs` row with a distinct `id` and distinct
// `workspace_branch_repo_id` but the SAME `source_branch_name = "main"`. Both
// rows MUST be active (`waiting_for_human`) concurrently. (test-view D71,
// arch D60.)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_workspaces_diverging_open_two_parallel_merge_runs() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo = create_local_remote_repo(&app, &org).await;

    let first = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-a",
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body: Value = first.json().await.expect("first response json");
    let workspace_a_id =
        Uuid::parse_str(first_body["data"]["id"].as_str().expect("workspace A id"))
            .expect("workspace A uuid");

    let second = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "Remote Tracking B",
        Some("main-b"),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_body: Value = second.json().await.expect("second response json");
    let workspace_b_id =
        Uuid::parse_str(second_body["data"]["id"].as_str().expect("workspace B id"))
            .expect("workspace B uuid");

    let branch_a = tracked_workspace_branch(&app, workspace_a_id).await;
    let branch_b = tracked_workspace_branch(&app, workspace_b_id).await;
    assert_ne!(
        branch_a.key(),
        branch_b.key(),
        "co-tracking workspaces must have distinct workspace_branch_repos rows"
    );
    let baseline = branch_a.head_commit.clone();
    assert_eq!(
        baseline, branch_b.head_commit,
        "both workspaces bootstrap from the same remote tip T0"
    );

    let workspace_ids = [branch_a.workspace_id, branch_b.workspace_id];
    sqlx::query(
        "DELETE FROM remote_integration_queue WHERE workspace_id = ANY($1) AND repo_id = $2",
    )
    .bind(&workspace_ids[..])
    .bind(repo.repo_id)
    .execute(&app.db)
    .await
    .expect("discard bootstrap queue work before explicit divergence cycles");
    let paused_reasons: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id = ANY($1) AND repo_id = $2 ORDER BY workspace_id",
    )
    .bind(&workspace_ids[..]).bind(repo.repo_id)
    .fetch_all(&app.db)
    .await
    .expect("bootstrap pause state should be queryable");
    assert_eq!(
        paused_reasons,
        vec![None, None],
        "both co-tracking members must enter the explicit divergence proof unpaused"
    );

    let mirror = ensure_shared_mirror(&app, &repo).await;

    // Remote moves to a disjoint C_R (a child of T0 that neither workspace has).
    let remote_c_r = advance_remote_main(&mirror, &baseline, "remote_only.txt", "remote work\n");
    assert_ne!(remote_c_r, baseline);

    // W_A and W_B each get a sibling local commit off T0 — disjoint from each
    // other AND from C_R.
    let local_c_a = commit_divergent_local_tip(
        &app,
        &mirror,
        branch_a.key(),
        &baseline,
        "workspace_a_only.txt",
        "workspace A work\n",
    )
    .await;
    let local_c_b = commit_divergent_local_tip(
        &app,
        &mirror,
        branch_b.key(),
        &baseline,
        "workspace_b_only.txt",
        "workspace B work\n",
    )
    .await;
    assert_ne!(local_c_a, local_c_b, "the two local tips must be disjoint");
    assert_ne!(local_c_a, remote_c_r);
    assert_ne!(local_c_b, remote_c_r);

    // Both commit-coupled cycles run independently.
    enqueue_integration(&app, branch_a.key(), org.id, "manual").await;
    drain_one_cycle(&app, branch_a.key()).await;
    assert_eq!(
        latest_run_status_for_trigger(&app, branch_a.key(), "manual").await,
        "diverged",
        "W_A's commit-coupled cycle must classify diverged"
    );

    enqueue_integration(&app, branch_b.key(), org.id, "manual").await;
    drain_one_cycle(&app, branch_b.key()).await;
    let second_run_evidence: (String, Option<String>, String, String, Option<String>) =
        sqlx::query_as(
            r#"
            SELECT status, error_detail, old_workspace_tip, new_workspace_tip, new_mirror_tip
            FROM remote_sync_runs
            WHERE workspace_id = $1 AND repo_id = $2
              AND trigger_kind = 'manual'
            ORDER BY started_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(branch_b.workspace_id)
        .bind(branch_b.repo_id)
        .fetch_one(&app.db)
        .await
        .expect("W_B trigger-correlated audit evidence");
    assert_eq!(
        second_run_evidence.0,
        "diverged",
        "W_B's commit-coupled cycle must classify diverged independently; audit={second_run_evidence:?}"
    );

    // Each workspace opened its OWN active merge run.
    let runs: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        r#"
        SELECT id, workspace_id, source_branch_name, status
        FROM workspace_merge_runs
        WHERE source_kind = 'remote_tracking'
          AND workspace_id = ANY($1) AND repo_id = $2
        ORDER BY workspace_id
        "#,
    )
    .bind(vec![branch_a.workspace_id, branch_b.workspace_id])
    .bind(repo.repo_id)
    .fetch_all(&app.db)
    .await
    .expect("parallel merge runs");

    assert_eq!(
        runs.len(),
        2,
        "exactly two parallel remote_tracking merge runs must exist, got {runs:?}"
    );
    let run_a = runs
        .iter()
        .find(|r| r.1 == branch_a.workspace_id)
        .expect("W_A merge run");
    let run_b = runs
        .iter()
        .find(|r| r.1 == branch_b.workspace_id)
        .expect("W_B merge run");

    assert_ne!(
        run_a.0, run_b.0,
        "the two merge runs must have distinct ids"
    );
    assert_ne!(
        run_a.1, run_b.1,
        "the two merge runs must have distinct workspace_branch_repo_ids"
    );
    assert_eq!(
        run_a.2, "main",
        "W_A's run carries the operator-facing remote branch name"
    );
    assert_eq!(
        run_b.2, "main",
        "W_B's run carries the SAME source_branch_name as W_A's"
    );
    assert_eq!(
        run_a.3, "waiting_for_human",
        "W_A's merge run must be active"
    );
    assert_eq!(
        run_b.3, "waiting_for_human",
        "W_B's merge run must be active concurrently with W_A's"
    );

    // Both branches paused on their own row; the parent name is shared upstream.
    let (paused_a, paused_b): (Option<String>, Option<String>) = (
        sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2")
            .bind(branch_a.workspace_id).bind(branch_a.repo_id)
            .fetch_one(&app.db)
            .await
            .expect("W_A pause reason"),
        sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2")
            .bind(branch_b.workspace_id).bind(branch_b.repo_id)
            .fetch_one(&app.db)
            .await
            .expect("W_B pause reason"),
    );
    assert_eq!(paused_a.as_deref(), Some("diverged"));
    assert_eq!(paused_b.as_deref(), Some("diverged"));

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D72 — first merge resolution invalidates the second; second's resume
// recomputes `source_heads.mirror_tip`.
//
// Given the parallel-merge-runs state (W_A and W_B each with an active
// `workspace_merge_runs` row whose `source_heads.mirror_tip` was the detection-time
// remote tip C_R), when W_A's merge resolves first and pushes merge commit M_A
// to the remote, and the operator later RESUMES W_B's merge run, then W_B's
// `workspace_merge_runs.source_heads.mirror_tip` MUST be recomputed to M_A (not the
// stale C_R). The recompute reads the fresh on-disk `refs/remotes/origin/main`
// mirror ref — the exact path `start_remote_tracking_merge_job` runs FIRST on
// resume (merge leaf D5). The audit trail MUST preserve the original
// `pre_start_source_heads` for forensics; ONLY `source_heads.mirror_tip` moves.
// (test-view D72, arch D60, merge-leaf D5.)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn peer_merge_push_recomputes_second_runs_mirror_tip_on_resume() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo = create_local_remote_repo(&app, &org).await;

    let first = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-a",
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body: Value = first.json().await.expect("first response json");
    let workspace_a_id =
        Uuid::parse_str(first_body["data"]["id"].as_str().expect("workspace A id"))
            .expect("workspace A uuid");

    let second = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "Remote Tracking B",
        Some("main-b"),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_body: Value = second.json().await.expect("second response json");
    let workspace_b_id =
        Uuid::parse_str(second_body["data"]["id"].as_str().expect("workspace B id"))
            .expect("workspace B uuid");

    let branch_a = tracked_workspace_branch(&app, workspace_a_id).await;
    let branch_b = tracked_workspace_branch(&app, workspace_b_id).await;
    let baseline = branch_a.head_commit.clone();

    let mirror = ensure_shared_mirror(&app, &repo).await;

    // Detection-time remote tip C_R (disjoint from both workspaces).
    let remote_c_r = advance_remote_main(&mirror, &baseline, "remote_only.txt", "remote work\n");
    git(&mirror, &["fetch", "origin"]);
    assert_eq!(
        git(&mirror, &["rev-parse", "refs/remotes/origin/main"]),
        remote_c_r,
        "mirror's origin/main must reflect the detection-time tip C_R"
    );

    // Each workspace diverges with its own local commit, then both open merge
    // runs at the SAME detection-time mirror tip C_R.
    let local_c_a = commit_divergent_local_tip(
        &app,
        &mirror,
        branch_a.key(),
        &baseline,
        "workspace_a_only.txt",
        "workspace A work\n",
    )
    .await;
    let local_c_b = commit_divergent_local_tip(
        &app,
        &mirror,
        branch_b.key(),
        &baseline,
        "workspace_b_only.txt",
        "workspace B work\n",
    )
    .await;

    enqueue_integration(&app, branch_a.key(), org.id, "manual").await;
    drain_one_cycle(&app, branch_a.key()).await;
    enqueue_integration(&app, branch_b.key(), org.id, "manual").await;
    drain_one_cycle(&app, branch_b.key()).await;

    let run_b_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT id FROM workspace_merge_runs
        WHERE source_kind = 'remote_tracking' AND workspace_id = $1 AND repo_id = $2
        "#,
    )
    .bind(branch_b.workspace_id)
    .bind(branch_b.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("W_B merge run id");

    // Both runs opened with mirror_tip == C_R, captured in source_heads AND in
    // the forensic pre_start_source_heads.
    let (b_source_mirror, b_prestart_mirror): (Option<String>, Option<String>) = sqlx::query_as(
        r#"
        SELECT source_heads->>'mirror_tip', pre_start_source_heads->>'mirror_tip'
        FROM workspace_merge_runs WHERE id = $1
        "#,
    )
    .bind(run_b_id)
    .fetch_one(&app.db)
    .await
    .expect("W_B open-time heads");
    assert_eq!(
        b_source_mirror.as_deref(),
        Some(remote_c_r.as_str()),
        "W_B's open-time source_heads.mirror_tip is the detection-time C_R"
    );
    assert_eq!(
        b_prestart_mirror.as_deref(),
        Some(remote_c_r.as_str()),
        "W_B's pre_start_source_heads.mirror_tip is the detection-time C_R"
    );

    // W_A resolves first and pushes a merge commit M_A to the remote. M_A
    // descends from both C_R (the remote it reconciled against) and C_A.
    git(&mirror, &["checkout", "-B", "wa-merge", &remote_c_r]);
    git(
        &mirror,
        &["merge", "--no-ff", "-m", "W_A merge resolution", &local_c_a],
    );
    let merge_m_a = git(&mirror, &["rev-parse", "HEAD"]);
    git(
        &mirror,
        &[
            "push",
            "--force",
            "origin",
            &format!("{merge_m_a}:refs/heads/main"),
        ],
    );
    assert_eq!(repo.remote_tip(), merge_m_a, "remote must advance to M_A");
    assert_ne!(merge_m_a, remote_c_r);
    assert_ne!(merge_m_a, local_c_b);

    // The recompute reads the on-disk mirror ref; bring it up to M_A the way the
    // worker would under the per-branch lock (a fresh fetch). This stands in for
    // the follow-up re-fetch the merge leaf D5 references.
    git(&mirror, &["fetch", "origin"]);
    assert_eq!(
        git(&mirror, &["rev-parse", "refs/remotes/origin/main"]),
        merge_m_a,
        "mirror's origin/main must now point at M_A"
    );

    let running_workspace = helpers::TestWorkspace {
        id: workspace_b_id,
        org_id: org.id,
        repo_id: repo.repo_id,
        name: branch_b.local_branch_name.clone(),
        head_commit: local_c_b.clone(),
    };
    // The production fork publishes a stopped incarnation. Replace that
    // placeholder with the running peer used for the resolution-agent test.
    let removed = sqlx::query(
        "DELETE FROM workspace_vms WHERE workspace_id=$1 AND status='stopped' AND host_id IS NULL",
    )
    .bind(workspace_b_id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(removed.rows_affected(), 1);
    let runtime =
        helpers::TestWorkspaceRuntimePeer::attach(&app, &running_workspace, user.id).await;
    // Operator resumes W_B through the production endpoint. The start service
    // captures C_B, closes its DB transaction, reads the fresh mirror M_A, and
    // persists both only after fenced exact-cut revalidation.
    let start = app
        .authenticated_client(&user)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/merge_runs/{}/start",
                workspace_b_id, branch_b.repo_id, run_b_id
            ),
            &json!({}),
        )
        .await;
    let status = start.status();
    let response_text = start.text().await.expect("resume response body");
    assert_eq!(
        status,
        StatusCode::OK,
        "resume should start cleanly: {response_text}"
    );

    // Persisted source_heads moved to M_A; pre_start_source_heads is untouched.
    let (final_source_mirror, final_source_workspace, final_prestart_mirror): (
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        r#"
        SELECT
            source_heads->>'mirror_tip',
            source_heads->>'workspace_tip',
            pre_start_source_heads->>'mirror_tip'
        FROM workspace_merge_runs WHERE id = $1
        "#,
    )
    .bind(run_b_id)
    .fetch_one(&app.db)
    .await
    .expect("W_B heads after recompute");
    assert_eq!(
        final_source_mirror.as_deref(),
        Some(merge_m_a.as_str()),
        "persisted source_heads.mirror_tip MUST be M_A after resume"
    );
    assert_eq!(
        final_source_workspace.as_deref(),
        Some(local_c_b.as_str()),
        "persisted source_heads.workspace_tip stays at C_B"
    );
    assert_eq!(
        final_prestart_mirror.as_deref(),
        Some(remote_c_r.as_str()),
        "pre_start_source_heads.mirror_tip MUST remain the open-time C_R for forensics"
    );

    assert_eq!(
        git(&mirror, &["rev-parse", &branch_b.local_branch_name]),
        local_c_b,
        "even clean reconciliation waits for the workspace agent to publish"
    );
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(workspace_b_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 1);
    drop(runtime);
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D75 — durable per-(repo, remote_branch) lease serializes integration cycles
// across co-tracking workspaces.
//
// Two assertions, both against the real generation-fenced remote-pair lease:
//
//   PART 1 — provable serialization: while the harness owns the durable
//   `(repo_R, "main")` lease, a running worker MUST NOT drain W_B's
//   queue row — its lease claim returns no owner and DEFERS (no audit row,
//   queue row preserved). When the harness releases the lease, the same worker
//   drains the row. This is the "second cycle defers on the remote-pair lease"
//   half of the contract: W_A and W_B share one `(repo_id, remote_branch)` even
//   though they are distinct `workspace_branch_repos` rows (both track remote
//   "main"; each has its own exact local name) — the lease key, keyed on
//   `remote_branch`, is identical for both.
//
//   PART 2 — no-clobber outcome: with the lease free, both cycles run so NEITHER
//   push is lost — W_A fast-forwards the remote T0 -> C_A, then W_B re-reads the
//   now-advanced remote and fast-forwards C_A -> C_B. The remote ends at C_B,
//   both audit rows are `pushed`, and their `started_at` timestamps are
//   strictly ordered (no interleaved/concurrent push to the same ref).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn co_tracking_explicit_pushes_serialize_without_clobber() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo = create_local_remote_repo(&app, &org).await;

    let first = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "remote-tracking-a",
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body: Value = first.json().await.expect("first response json");
    let workspace_a_id =
        Uuid::parse_str(first_body["data"]["id"].as_str().expect("workspace A id"))
            .expect("workspace A uuid");

    let second = create_remote_tracking_workspace(
        &app,
        &user,
        &org,
        repo.repo_id,
        "Remote Tracking B",
        Some("main-b"),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_body: Value = second.json().await.expect("second response json");
    let workspace_b_id =
        Uuid::parse_str(second_body["data"]["id"].as_str().expect("workspace B id"))
            .expect("workspace B uuid");

    // Explicit requests remain valid in auto mode for this local test remote.
    for ws in [workspace_a_id, workspace_b_id] {
        sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
            .bind(ws)
            .execute(&app.db)
            .await
            .expect("set auto push_mode");
    }

    let branch_a = tracked_workspace_branch(&app, workspace_a_id).await;
    let branch_b = tracked_workspace_branch(&app, workspace_b_id).await;
    let baseline = branch_a.head_commit.clone();
    assert_eq!(baseline, branch_b.head_commit, "both bootstrap from T0");

    let mirror = ensure_shared_mirror(&app, &repo).await;

    // C_A descends from T0; C_B descends from C_A (stacked, so each cycle is a
    // clean fast-forward over the remote tip it observes).
    git(&mirror, &["checkout", "-B", "stack", &baseline]);
    std::fs::write(mirror.join("a.txt"), "workspace A change\n").expect("write a.txt");
    git(&mirror, &["add", "a.txt"]);
    git(&mirror, &["commit", "-m", "C_A"]);
    let commit_c_a = git(&mirror, &["rev-parse", "HEAD"]);
    std::fs::write(mirror.join("b.txt"), "workspace B change\n").expect("write b.txt");
    git(&mirror, &["add", "b.txt"]);
    git(&mirror, &["commit", "-m", "C_B"]);
    let commit_c_b = git(&mirror, &["rev-parse", "HEAD"]);
    assert_ne!(commit_c_a, baseline);
    assert_ne!(commit_c_b, commit_c_a);

    // The worker pushes `refs/heads/<local_branch_name>:refs/heads/<remote>`
    // (`push_refspec`), so each workspace's LOCAL branch ref in the shared
    // mirror must point at that workspace's tip — exactly what a real commit
    // cycle leaves behind. Each workspace has an exact local name that differs
    // from the remote name "main"; read it from the DB.
    git(
        &mirror,
        &["branch", "-f", &branch_a.local_branch_name, &commit_c_a],
    );
    git(
        &mirror,
        &["branch", "-f", &branch_b.local_branch_name, &commit_c_b],
    );

    seed_workspace_authority_head(&app, branch_a.workspace_id, branch_a.repo_id, &commit_c_a).await;
    seed_workspace_authority_head(&app, branch_b.workspace_id, branch_b.repo_id, &commit_c_b).await;

    // =====================================================================
    // PART 1 — provable remote-pair lease serialization (the deferral half).
    //
    // Hold the `(repo_R, "main")` durable lease from the harness, enqueue W_B's
    // cycle, and run a worker. W_B and W_A share the SAME `(repo_id,
    // remote_branch)` (both remote "main"; distinct exact local names), so the
    // worker cannot claim that live generation and the cycle DEFERS: W_B's queue
    // row MUST remain undrained and NO `pushed` audit
    // row may appear while the harness holds the lease (arch D57 invariant 1).
    // The worker is cancelled WHILE the lease is still held, so W_B's cycle is
    // never consumed here — both queue rows remain pending for PART 2's
    // no-clobber flow, exactly as a real two-replica race would leave them.
    // =====================================================================
    use g5_api::maintenance::branch_integration_worker::run_branch_integration_worker;

    let lease_owner = harness_hold_remote_pair_lease(&app, repo.repo_id, org.id, "main").await;

    enqueue_integration(&app, branch_b.key(), org.id, "push").await;
    assert_eq!(
        queue_row_count(&app, branch_b.key()).await,
        1,
        "W_B's queue row is enqueued and contended"
    );

    let defer_cancel = CancellationToken::new();
    let defer_worker = tokio::spawn(run_branch_integration_worker(
        worker_deps(&app),
        defer_cancel.clone(),
    ));

    // Observe across two sweep ticks (sweep interval is 5s): the row must stay
    // put and unintegrated the entire time the harness holds the lease.
    let observe_until = Instant::now() + Duration::from_secs(7);
    while Instant::now() < observe_until {
        assert_eq!(
            queue_row_count(&app, branch_b.key()).await,
            1,
            "while the harness holds the per-(repo_id, remote_branch) lease, W_B's cycle MUST \
             defer — its queue row may not be drained (D75 serialization)"
        );
        assert_eq!(
            pushed_run_count(&app, branch_b.key()).await,
            0,
            "W_B MUST NOT push while another cycle holds the remote-pair lease (D75)"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // Cancel the worker BEFORE releasing the lease so W_B's deferred cycle is
    // never drained in PART 1 — it stays pending for the no-clobber flow.
    defer_cancel.cancel();
    let _ = defer_worker.await;
    harness_release_remote_pair_lease(&app, repo.repo_id, "main", lease_owner).await;

    // W_B's row survived the leased window: still queued, still un-pushed.
    assert_eq!(
        queue_row_count(&app, branch_b.key()).await,
        1,
        "W_B's row remains queued after the leased window (it only ever deferred)"
    );
    assert_eq!(
        pushed_run_count(&app, branch_b.key()).await,
        0,
        "W_B never pushed while the remote-pair lease was held"
    );

    // Clear PART 1's W_B queue row so PART 2 starts from a clean slate with a
    // deterministic A-before-B pickup order (`requested_at ASC`). The deferral
    // contract (no audit row, row preserved) is already proven above.
    sqlx::query("DELETE FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2")
        .bind(branch_b.workspace_id)
        .bind(branch_b.repo_id)
        .execute(&app.db)
        .await
        .expect("clear W_B's PART 1 queue row");

    // =====================================================================
    // PART 2 — no-clobber outcome with the lease free.
    //
    // Both queue rows ready at the same moment (A enqueued before B so a single
    // worker's `requested_at ASC` drain processes A first). One worker drains
    // both: W_A (T0 -> C_A), then W_B (re-reads the advanced remote, C_A ->
    // C_B). The durable per-`(repo_id, remote_branch)` lease serializes the two
    // cycles so neither push is lost.
    // =====================================================================
    enqueue_integration(&app, branch_a.key(), org.id, "push").await;
    enqueue_integration(&app, branch_b.key(), org.id, "push").await;
    drain_one_cycle(&app, branch_a.key()).await;
    drain_one_cycle(&app, branch_b.key()).await;

    // Neither push was lost: the remote ends at C_B (W_B's tip stacked on C_A).
    assert_eq!(
        repo.remote_tip(),
        commit_c_b,
        "the shared remote ref must end at C_B with W_A's C_A preserved underneath \
         (no clobber / no lost update)"
    );
    // C_A must be an ancestor of the final remote tip — W_A's push was not lost.
    let merge_base = git(&mirror, &["merge-base", &commit_c_a, &repo.remote_tip()]);
    assert_eq!(
        merge_base, commit_c_a,
        "W_A's commit C_A must be an ancestor of the final remote tip"
    );

    assert_eq!(
        latest_run_status_for_trigger(&app, branch_a.key(), "push").await,
        "pushed",
        "W_A's explicit push must publish"
    );
    assert_eq!(
        latest_run_status_for_trigger(&app, branch_b.key(), "push").await,
        "pushed",
        "W_B's explicit push must publish on top of W_A's"
    );

    // Audit rows record sequential (non-overlapping) execution: W_A's push
    // started before W_B's. We compare the two `pushed` rows' start times.
    let a_started: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        r#"
        SELECT started_at FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2 AND status = 'pushed'
        ORDER BY started_at DESC LIMIT 1
        "#,
    )
    .bind(branch_a.workspace_id)
    .bind(branch_a.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("W_A pushed audit row");
    let b_started: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        r#"
        SELECT started_at FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2 AND status = 'pushed'
        ORDER BY started_at DESC LIMIT 1
        "#,
    )
    .bind(branch_b.workspace_id)
    .bind(branch_b.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("W_B pushed audit row");
    assert!(
        a_started <= b_started,
        "audit rows must record sequential execution: W_A push at {a_started} \
         must not start after W_B push at {b_started}"
    );

    // Both workspaces converge: baseline + advisory advance to the pushed tip
    // (post-push convergence, C30), and neither is paused.
    for (key, expected_tip) in [(branch_a.key(), &commit_c_a), (branch_b.key(), &commit_c_b)] {
        let (_, baseline_commit, _) = member_state(&app, key).await;
        let paused: Option<String> = sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2")
            .bind(key.workspace_id).bind(key.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(
            &baseline_commit, expected_tip,
            "baseline must advance to the pushed tip after a clean FF push"
        );
        assert_eq!(
            paused, None,
            "a clean serialized push must not pause the branch"
        );
    }

    app.cleanup().await;
}

/// Regression (C16 / worker D6): a co-tracking workspace that was AHEAD
/// (`head > baseline`) and whose peer then published the IDENTICAL inherited
/// commit to the shared remote ends with `head == mirror_tip` but `baseline`
/// lagging. The `Equal → Noop` cycle MUST catch the lagging baseline up to the
/// tip (§6.1 invariant `workspace tip == mirror tip ⇒ baseline == tip`), else
/// the read-side `isBehind = advisory != baseline` stays falsely lit and the
/// parent is stuck "behind" forever. (The Noop-path analogue of the push C30 /
/// import C31 convergence.)
#[tokio::test]
async fn noop_advances_lagging_baseline_when_workspace_tip_equals_remote() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let repo = create_local_remote_repo(&app, &org).await;

    let created =
        create_remote_tracking_workspace(&app, &user, &org, repo.repo_id, "cotrack-parent", None)
            .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let body: Value = created.json().await.expect("create json");
    let workspace_id = Uuid::parse_str(body["data"]["id"].as_str().expect("workspace id")).unwrap();
    let tracked = tracked_workspace_branch(&app, workspace_id).await;
    let c1 = repo.remote_tip(); // baseline at create
    assert_eq!(tracked.head_commit, c1);

    let mirror = ensure_shared_mirror(&app, &repo).await;

    // 1. Workspace goes AHEAD: a local commit C2 (child of C1) advances the
    //    child head; baseline stays C1 (a commit does not advance baseline).
    let c2 = commit_divergent_local_tip(
        &app,
        &mirror,
        tracked.key(),
        &c1,
        "feature.txt",
        "parent local work\n",
    )
    .await;
    assert_ne!(c2, c1);

    // 2. A co-tracking PEER publishes the IDENTICAL commit C2 to the shared
    //    remote (the inherited-commit convergence). Now the shared remote tip ==
    //    C2 == the workspace tip, but the parent's baseline is still C1.
    git(
        &mirror,
        &["push", "origin", &format!("{c2}:refs/heads/main")],
    );
    assert_eq!(repo.remote_tip(), c2);

    // Precondition: the stuck state — head == C2, baseline lags at C1.
    let (head_before, baseline_before, _) = member_state(&app, tracked.key()).await;
    assert_eq!(head_before, c2);
    assert_eq!(
        baseline_before, c1,
        "baseline lags the equal tip before the noop convergence"
    );

    // 3. Background-inbound cycle: tips equal (C2 == C2) → Noop, which MUST
    //    advance the lagging baseline to the tip.
    enqueue_integration(&app, tracked.key(), org.id, "background_inbound").await;
    drain_one_cycle(&app, tracked.key()).await;

    assert_eq!(latest_run_status(&app, tracked.key()).await, "noop");
    let (head_after, baseline_after, advisory_after) = member_state(&app, tracked.key()).await;
    assert_eq!(head_after, c2);
    assert_eq!(
        baseline_after, c2,
        "Noop MUST advance the lagging baseline to the equal tip (C16) — \
         else the parent is stuck 'behind'"
    );
    assert_eq!(
        advisory_after.as_deref(),
        Some(c2.as_str()),
        "advisory == tip; with baseline == tip the read-side isBehind clears"
    );

    app.cleanup().await;
}
