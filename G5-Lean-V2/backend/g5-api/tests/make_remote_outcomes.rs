//! Make-Remote **absent-ref create** and **divergent-lineage merge routing**
//! integration tests (`workspace_service` D47 items 2 & 3).
//!
//! These drive the REAL promote-remote-tracking route
//! (`POST /workspaces/:id/branches/:branchId/repos/:repoId/remote_tracking`)
//! against a real Postgres tenant DB, real repo storage, and real local bare
//! git remotes — no mocks/stubs of in-scope services (Tier 1 discipline). They
//! model `make_remote_adopt.rs` / `coordinated_repo_sync.rs` /
//! `remote_repo_tracking_integration.rs`.
//!
//! Scope: the absent-ref create-and-bind path (D46 item 2) and the
//! divergent/local-ahead merge routing (D46 item 3)
//! that both keep the byte-for-byte [D28] behavior (absent → resolve error,
//! divergent → INCOMPATIBLE_REMOTE_LINEAGE).
//!
//! Status: written; compiles under `cargo check -p g5-api --tests`. Execution
//! is DEFERRED — the sandbox has no Postgres, so these are marked
//! `implemented_unverified`. They are NOT claimed to pass.

use crate::helpers;

use g5_auth::Scope;
use g5_contracts::types::WorkspaceRepoKey;
use helpers::{test_config, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// git + fixture helpers (real local bare remote, real mirror)
// ---------------------------------------------------------------------------

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

fn rw_user_scopes() -> [Scope; 2] {
    [Scope::WorkspacesRead, Scope::WorkspacesWrite]
}

/// A real local bare repo standing in for a customer upstream, plus its `main`
/// tip SHA.
struct Bare {
    /// The temp-dir root holding both the bare repo and its seed checkout;
    /// removed on `Drop` so tests do not leak `temp_dir()/g5-make-remote-out-*`
    /// dirs.
    root: PathBuf,
    path: PathBuf,
}

impl Drop for Bare {
    fn drop(&mut self) {
        // Best-effort cleanup of the temp root (bare + seed). Ignore errors:
        // a failed removal must never mask a test assertion failure.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Bare {
    fn url(&self) -> String {
        self.path.display().to_string()
    }

    /// The SHA on `refs/heads/<branch>` in the bare, or `None` if absent.
    fn ref_sha(&self, branch: &str) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
            .current_dir(&self.path)
            .output()
            .expect("spawn git rev-parse");
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }
}

/// Create a real bare git repo with one seed commit on `main`.
fn make_bare(name: &str) -> Bare {
    let root = std::env::temp_dir().join(format!("g5-make-remote-out-{}", Uuid::new_v4().simple()));
    let bare = root.join(format!("{name}.git"));
    let seed = root.join(format!("{name}-seed"));
    std::fs::create_dir_all(&bare).expect("create bare dir");
    std::fs::create_dir_all(&seed).expect("create seed dir");

    git(&bare, &["init", "--bare", "--initial-branch=main"]);
    git(&seed, &["init", "--initial-branch=main"]);
    git(&seed, &["config", "user.email", "g5@test.local"]);
    git(&seed, &["config", "user.name", "G5 Test"]);
    std::fs::write(seed.join("README.md"), format!("{name} seed\n")).expect("seed README");
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "seed"]);
    git(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&seed, &["push", "origin", "main"]);
    git(&bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    Bare { root, path: bare }
}

/// Insert a `repos` row with an explicit `git_url` (manually-added: provider
/// NULL), pointing at a real bare so create-time materialization succeeds.
async fn insert_repo(app: &TestApp, org_id: Uuid, name: &str, git_url: &str) -> Uuid {
    let repo_id = Uuid::new_v4();
    let slug = format!("outcomes-{name}-{}", repo_id.simple());
    sqlx::query(
        r#"
        INSERT INTO repos
            (id, org_id, name, slug, git_url, default_branch, provider, provider_repo_id, provider_full_name)
        VALUES ($1, $2, $3, $4, $5, 'main', NULL, NULL, NULL)
        "#,
    )
    .bind(repo_id)
    .bind(org_id)
    .bind(name)
    .bind(&slug)
    .bind(git_url)
    .execute(&app.db)
    .await
    .expect("insert repo row");
    repo_id
}

/// Clone the bare into the shared mirror at `repo_storage.local_path(repo_id)`.
async fn ensure_mirror(app: &TestApp, repo_id: Uuid, bare: &Path) -> PathBuf {
    let mirror = app.repo_path(repo_id).await;
    if let Some(parent) = mirror.parent() {
        std::fs::create_dir_all(parent).expect("create mirror parent dir");
    }
    if !mirror.exists() {
        git(
            mirror.parent().expect("mirror parent"),
            &[
                "clone",
                bare.to_str().expect("bare path"),
                mirror.to_str().expect("mirror path"),
            ],
        );
    }
    git(&mirror, &["config", "user.email", "g5@test.local"]);
    git(&mirror, &["config", "user.name", "G5 Test"]);
    mirror
}

/// Add one commit on top of `main` in the mirror WITHOUT pushing it, so the
/// workspace's local head becomes a strict descendant of the remote tip
/// (LocalAhead). Returns the new local head SHA.
fn commit_local_ahead(mirror: &Path) -> String {
    std::fs::write(mirror.join("local_only.txt"), "local ahead\n").expect("write local file");
    git(mirror, &["add", "."]);
    git(
        mirror,
        &["commit", "-m", "local-only commit ahead of remote"],
    );
    git(mirror, &["rev-parse", "HEAD"])
}

/// Land one commit on the bare's `main` from a throwaway clone, so the remote
/// is strictly ahead of every workspace still at the seed tip; returns its SHA.
fn commit_remote_ahead(bare: &Bare) -> String {
    let clone = bare.root.join("ahead-clone");
    git(
        &bare.root,
        &["clone", bare.path.to_str().unwrap(), "ahead-clone"],
    );
    git(&clone, &["config", "user.email", "g5@test.local"]);
    git(&clone, &["config", "user.name", "G5 Test"]);
    std::fs::write(clone.join("remote_only.txt"), "remote ahead\n").expect("write remote file");
    git(&clone, &["add", "."]);
    git(
        &clone,
        &["commit", "-m", "remote-only commit ahead of the workspace"],
    );
    git(&clone, &["push", "origin", "main"]);
    git(&clone, &["rev-parse", "HEAD"])
}

/// The "has a new commit" notices in `workspace_id`'s inbox as `user`.
async fn new_commit_notices(app: &TestApp, user: &TestUser, workspace_id: Uuid) -> Vec<Value> {
    let response = app
        .authenticated_client(user)
        .get(&format!("/api/v1/workspaces/{workspace_id}/notifications"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap_or(Value::Null);
    let data = &body["data"];
    data.as_array()
        .cloned()
        .or_else(|| data["notifications"].as_array().cloned())
        .or_else(|| data["items"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|n| {
            n["title"]
                .as_str()
                .unwrap_or_default()
                .contains("has a new commit")
        })
        .collect()
}

/// Seed a source workspace with the given pinned repositories, then create the
/// tested working workspace through the sole public fork path.
async fn create_pinned_workspace(
    app: &TestApp,
    user: &TestUser,
    org: &TestOrg,
    repo_ids: &[Uuid],
    name: &str,
) -> Uuid {
    let source = TestWorkspace::create(app, org).await;
    sqlx::query("UPDATE workspace_repos SET role='docs',root_path='docs' WHERE workspace_id=$1 AND repo_id=$2")
        .bind(source.id).bind(source.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
        .bind(source.id)
        .bind(source.repo_id)
        .execute(&app.db)
        .await
        .unwrap();

    for repo_id in repo_ids {
        let mirror = app.repo_path(*repo_id).await;
        let head = git(&mirror, &["rev-parse", "refs/heads/main"]);
        let root_path: String = sqlx::query_scalar("SELECT name FROM repos WHERE id = $1")
            .bind(repo_id)
            .fetch_one(&app.db)
            .await
            .expect("repository name");
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
        .expect("insert source workspace repository mapping");
        git(
            &mirror,
            &["update-ref", &format!("refs/heads/{}", source.name), &head],
        );
    }
    let resp = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", source.id),
            &json!({
                "name": name,
                "idempotencyKey": format!("make-remote-{name}-{}", Uuid::new_v4())
            }),
        )
        .await;
    let status = resp.status();
    let text = resp.text().await.expect("fork workspace body");
    assert_eq!(
        status,
        StatusCode::CREATED,
        "workspace fork should succeed: {text}"
    );
    let body: Value = serde_json::from_str(&text).expect("fork workspace json");
    Uuid::parse_str(
        body["data"]["workspace"]["id"]
            .as_str()
            .expect("workspace id"),
    )
    .expect("workspace uuid")
}

/// The mutable columns of a `workspace_branch_repos` child row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChildSnapshot {
    key: WorkspaceRepoKey,
    tracking_mode: String,
    remote_branch: Option<String>,
    head_commit: String,
    baseline_commit: Option<String>,
    remote_tip_advisory_sha: Option<String>,
    sync_paused_reason: Option<String>,
}

#[allow(clippy::type_complexity)] // Positional sqlx row tuple mirrors ChildSnapshot's columns in query order.
async fn child_snapshot(app: &TestApp, workspace_id: Uuid, repo_id: Uuid) -> ChildSnapshot {
    let (org_id, local_branch, remote_branch, sync_paused_reason): (Uuid, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT w.org_id,w.name,i.remote_branch,i.sync_paused_reason FROM workspace_repos m JOIN workspaces w ON w.id=m.workspace_id AND w.org_id=m.org_id LEFT JOIN remote_integrations i ON (i.workspace_id,i.repo_id)=(m.workspace_id,m.repo_id) WHERE m.workspace_id=$1 AND m.repo_id=$2")
        .bind(workspace_id).bind(repo_id).fetch_one(&app.db).await.unwrap();
    let key = WorkspaceRepoKey {
        org_id,
        workspace_id,
        repo_id,
    };
    let path = app.repo_path(repo_id).await;
    let head_commit = git(&path, &["rev-parse", &format!("refs/heads/{local_branch}")]);
    let baseline_commit = g5_scm::GitRepo::open(&path)
        .unwrap()
        .read_sync_watermark(&local_branch)
        .unwrap();
    let mut db = app.scoped_db_for_org(org_id).await;
    let policy = g5_api_core::services::remote_integrations::load(db.conn(), key, false)
        .await
        .unwrap();
    let remote_tip_advisory_sha = match &policy {
        Some(policy) => g5_api_core::services::remote_integrations::observed_tip(db.conn(), policy)
            .await
            .unwrap()
            .map(|o| o.oid),
        None => None,
    };
    db.rollback().await.unwrap();
    ChildSnapshot {
        key,
        tracking_mode: if remote_branch.is_some() {
            "remote_tracking"
        } else {
            "pinned"
        }
        .into(),
        remote_branch,
        head_commit,
        baseline_commit,
        remote_tip_advisory_sha,
        sync_paused_reason,
    }
}

/// Force the workspace's local child head to `sha` (used to synthesize a
/// LocalAhead workspace whose head strictly descends from the remote tip).
/// The branch ref is the head authority (P1.1): the workspace branch ref moves
/// with its projection, as a landed local commit leaves them.
async fn set_child_head(app: &TestApp, mirror: &Path, key: WorkspaceRepoKey, sha: &str) {
    let branch_name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(key.workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    git(
        mirror,
        &["update-ref", &format!("refs/heads/{branch_name}"), sha],
    );
}

/// Count active remote-tracking merge runs opened for a child row.
async fn remote_tracking_merge_run_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM workspace_merge_runs
        WHERE workspace_id = $1 AND repo_id = $2
          AND source_kind = 'remote_tracking'
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("merge run count")
}

async fn promote(
    app: &TestApp,
    user: &TestUser,
    workspace_id: Uuid,
    repo_id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/repos/{repo_id}/remote_tracking"),
            &body,
        )
        .await;
    let status = resp.status();
    let json: Value = resp.json().await.expect("promote response json");
    (status, json)
}

/// POST the separate Publish operation (`publish_branch_repo`), which adds a new
/// `g5/<branch>` ref to the repository's existing binding and stays the owner's.
async fn publish(
    app: &TestApp,
    user: &TestUser,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> (StatusCode, Value) {
    let resp = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/repos/{repo_id}/publish"),
            &json!({}),
        )
        .await;
    let status = resp.status();
    let json: Value = resp.json().await.expect("publish response json");
    (status, json)
}

// ---------------------------------------------------------------------------
// Make Remote is an organization-administrator action (#1096)
// ---------------------------------------------------------------------------

/// A non-administrator owner is refused Make Remote (the existing-URL bind), and
/// the provider is never touched: the absent branch it would have created stays
/// absent and the child stays pinned (`g5.git.make-remote`).
#[tokio::test]
async fn make_remote_refuses_non_admin_owner_without_touching_provider() {
    let app = TestApp::spawn_with_config(test_config()).await;
    let org = TestOrg::create(&app).await;
    // A plain member who owns the workspace: not an organization administrator.
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "non-admin-owner").await;

    // After setup, re-point the repository at an UNREACHABLE remote (a bare
    // path that does not exist). The admin refusal must precede every provider
    // read or write (`g5.git.remote-binding`), so the request still returns a
    // clean typed Forbidden rather than a REMOTE_BIND_REF_LOOKUP_FAILED — any
    // `ls-remote` or create-push against this URL would fail instead. This
    // detects provider access that the absent-ref check alone would miss (an
    // `ls-remote` would leave the ref absent too).
    let unreachable = std::env::temp_dir()
        .join(format!("g5-unreachable-{}.git", Uuid::new_v4().simple()))
        .display()
        .to_string();
    assert!(
        !Path::new(&unreachable).exists(),
        "precondition: the re-pointed remote does not exist"
    );
    sqlx::query("UPDATE repos SET git_url = $1 WHERE id = $2")
        .bind(&unreachable)
        .bind(repo_id)
        .execute(&app.db)
        .await
        .expect("re-point repo at an unreachable remote");

    let (status, body) = promote(
        &app,
        &user,
        ws,
        repo_id,
        json!({ "remoteBranch": "feature-x" }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a non-admin owner must be refused Make Remote with a clean Forbidden, \
         before any provider I/O against the unreachable remote: {body}"
    );
    assert_eq!(
        child_snapshot(&app, ws, repo_id).await.tracking_mode,
        "pinned",
        "a refused Make Remote leaves the child pinned"
    );
    let git_url: String = sqlx::query_scalar("SELECT git_url FROM repos WHERE id = $1")
        .bind(repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        git_url, unreachable,
        "a refused Make Remote must not mutate the repository binding"
    );

    app.cleanup().await;
}

/// An administrator who does not own the workspace is refused Make Remote on an
/// ordinary workspace ("block admin-but-not-owner"), before any provider I/O.
#[tokio::test]
async fn make_remote_refuses_non_owner_admin_on_ordinary_workspace() {
    let app = TestApp::spawn_with_config(test_config()).await;
    let org = TestOrg::create(&app).await;
    // The owner is a plain member; a separate organization administrator does
    // not own the workspace.
    let owner = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    let admin = org_user(&app, org.id, "admin").await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &owner, &org, &[repo_id], "non-owner-admin").await;

    assert!(
        bare.ref_sha("feature-x").is_none(),
        "precondition: absent ref"
    );

    let (status, body) = promote(
        &app,
        &admin,
        ws,
        repo_id,
        json!({ "remoteBranch": "feature-x" }),
    )
    .await;

    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "a non-owner administrator must be refused Make Remote on an ordinary workspace: {status} {body}"
    );
    assert!(
        bare.ref_sha("feature-x").is_none(),
        "a refused Make Remote must not create the provider branch"
    );
    assert_eq!(
        child_snapshot(&app, ws, repo_id).await.tracking_mode,
        "pinned"
    );

    app.cleanup().await;
}

/// Publish stays the owner's: a non-administrator owner publishes a local-only
/// branch to a new `g5/<branch>` ref on the repository's existing binding, which
/// Make Remote's administrator rule does not gate (`g5.git.remote-binding`).
#[tokio::test]
async fn publish_succeeds_for_non_admin_owner() {
    let app = TestApp::spawn_with_config(test_config()).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "publish-owner").await;

    let before = child_snapshot(&app, ws, repo_id).await;
    let local_head = before.head_commit.clone();
    let branch_name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(ws)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let published_ref = format!("g5/{branch_name}");

    let (status, body) = publish(&app, &user, ws, repo_id).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a non-admin owner must still be able to Publish: {body}"
    );
    assert_eq!(
        bare.ref_sha(&published_ref).as_deref(),
        Some(local_head.as_str()),
        "Publish creates the g5/<branch> ref at the local head"
    );
    let after = child_snapshot(&app, ws, repo_id).await;
    assert_eq!(after.tracking_mode, "remote_tracking");
    assert_eq!(after.remote_branch.as_deref(), Some(published_ref.as_str()));

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D46 item 2 — absent remote ref → create-and-bind
// ---------------------------------------------------------------------------

/// Flag on + a `remote_branch` that does NOT exist on the remote → the service
/// publishes the local head to that branch verbatim, binds `remote_tracking`
/// with `baseline = advisory = local head`, opens NO merge run, and returns
/// `outcome = "published"`.
#[tokio::test]
async fn absent_ref_publishes_and_binds_by_default_without_merge_run() {
    let config = test_config();
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    // Make Remote needs an owner who is an organization administrator.
    set_org_role(&app, org.id, user.id, "admin").await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "absent-create").await;

    // The workspace local head is the seed tip (materialized at the bare main).
    let before = child_snapshot(&app, ws, repo_id).await;
    let local_head = before.head_commit.clone();

    // A branch the bare does not have yet.
    assert!(
        bare.ref_sha("feature-x").is_none(),
        "precondition: absent ref"
    );

    let (status, body) = promote(
        &app,
        &user,
        ws,
        repo_id,
        json!({ "remoteBranch": "feature-x" }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "absent-ref create-and-bind should succeed: {body}"
    );
    assert_eq!(
        body["data"]["outcome"].as_str(),
        Some("published"),
        "absent-ref outcome must be 'published': {body}"
    );
    assert!(
        body["data"]["mergeRunId"].is_null(),
        "published outcome must carry no mergeRunId: {body}"
    );

    // Ref was created on the remote at the local head.
    assert_eq!(
        bare.ref_sha("feature-x").as_deref(),
        Some(local_head.as_str()),
        "the local head must be published to the requested remote branch verbatim"
    );

    // Child bound remote_tracking with baseline = advisory = local head, unpaused.
    let after = child_snapshot(&app, ws, repo_id).await;
    assert_eq!(after.tracking_mode, "remote_tracking");
    assert_eq!(after.remote_branch.as_deref(), Some("feature-x"));
    assert_eq!(after.baseline_commit.as_deref(), Some(local_head.as_str()));
    assert!(
        after.remote_tip_advisory_sha.is_none(),
        "poll hints are collected independently"
    );
    assert_eq!(after.sync_paused_reason, None);

    // No merge run opened.
    assert_eq!(
        remote_tracking_merge_run_count(&app, after.key).await,
        0,
        "absent-ref create must not open a merge run"
    );

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Remote ahead → bind imports the remote tip, and the reviewers are told
// ---------------------------------------------------------------------------

/// An existing remote ref strictly ahead of the local head → the service
/// binds `remote_tracking` and imports the remote tip as the branch head.
/// That moves the branch a review copy is measured against, so the reviewer
/// holding a live copy is told once, in their copy's inbox; the actor is not.
#[tokio::test]
async fn remote_ahead_bind_imports_the_tip_and_tells_reviewers() {
    let config = test_config();
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    // Make Remote needs an owner who is an organization administrator.
    set_org_role(&app, org.id, user.id, "admin").await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    let mirror = ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "remote-ahead").await;
    let before = child_snapshot(&app, ws, repo_id).await;
    assert_eq!(
        before.head_commit,
        git(&bare.path, &["rev-parse", "refs/heads/main"]),
        "precondition: at the seed tip"
    );

    // A reviewer holds a live review copy of the workspace.
    let reviewer = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    let started = app
        .authenticated_client(&reviewer)
        .post_json(&format!("/api/v1/workspaces/{ws}/review/start"), &json!({}))
        .await;
    let status = started.status();
    let body: Value = started.json().await.unwrap_or(Value::Null);
    assert!(
        matches!(status, StatusCode::CREATED | StatusCode::OK),
        "start review: {body}"
    );
    let review_copy: Uuid = body["data"]["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("review workspace id");
    assert!(
        new_commit_notices(&app, &reviewer, review_copy)
            .await
            .is_empty(),
        "nothing before the branch moves"
    );

    // The remote moves on while the workspace stays at the seed tip.
    let remote_tip = commit_remote_ahead(&bare);
    assert_ne!(remote_tip, before.head_commit);
    let stale = make_bare("stale-origin");
    git(&mirror, &["remote", "set-url", "origin", &stale.url()]);
    assert!(
        !g5_scm::GitRepo::open(&mirror)
            .unwrap()
            .has_commit(&remote_tip)
            .unwrap(),
        "exact-tip preparation must download the authoritative remote's missing objects"
    );

    let (status, body) = promote(&app, &user, ws, repo_id, json!({ "remoteBranch": "main" })).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "remote-ahead bind should succeed: {body}"
    );
    assert_eq!(
        body["data"]["outcome"].as_str(),
        Some("bound"),
        "remote-ahead outcome must be 'bound': {body}"
    );
    let after = child_snapshot(&app, ws, repo_id).await;
    assert_eq!(after.tracking_mode, "remote_tracking");
    assert_eq!(
        after.head_commit, remote_tip,
        "the remote tip was imported as the branch head"
    );

    assert_eq!(
        git(&mirror, &["remote", "get-url", "origin"]),
        stale.url(),
        "classification must not rewrite shared remote configuration"
    );
    let notices = new_commit_notices(&app, &reviewer, review_copy).await;
    assert_eq!(notices.len(), 1, "the reviewer is told once: {notices:?}");
    assert!(
        notices[0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains(&remote_tip[..7]),
        "names the imported commit: {notices:?}"
    );
    assert_eq!(
        notices[0]["actions"][0]["target"],
        format!("workspace:{review_copy}?view=review"),
        "{notices:?}"
    );
    assert!(
        new_commit_notices(&app, &user, ws).await.is_empty(),
        "the actor is not notified"
    );

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D46 item 3 — divergent / local-ahead → merge-conflict workflow
// ---------------------------------------------------------------------------

/// Flag on + an existing remote ref whose tip the local head strictly descends
/// from (LocalAhead) → the service binds `remote_tracking` with the sentinel
/// `ZERO_GIT_SHA` baseline, pauses `diverged`, opens exactly one remote-tracking
/// merge run, and returns `outcome = "merge_required"` with the run id.
#[tokio::test]
async fn local_ahead_opens_single_merge_run_by_default() {
    let config = test_config();
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    // Make Remote needs an owner who is an organization administrator.
    set_org_role(&app, org.id, user.id, "admin").await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    let mirror = ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "local-ahead").await;

    // Synthesize a workspace local head that strictly descends from remote main.
    let local_head = commit_local_ahead(&mirror);
    let wbr_id = child_snapshot(&app, ws, repo_id).await.key;
    set_child_head(&app, &mirror, wbr_id, &local_head).await;

    let (status, body) = promote(&app, &user, ws, repo_id, json!({ "remoteBranch": "main" })).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "divergent/local-ahead should route into the merge workflow: {body}"
    );
    assert_eq!(
        body["data"]["outcome"].as_str(),
        Some("merge_required"),
        "local-ahead outcome must be 'merge_required': {body}"
    );
    let merge_run_id = body["data"]["mergeRunId"]
        .as_str()
        .expect("merge_required outcome must carry a mergeRunId");
    assert!(
        Uuid::parse_str(merge_run_id).is_ok(),
        "mergeRunId must be a uuid: {merge_run_id}"
    );

    // Child bound remote_tracking with the sentinel baseline, advisory = remote
    // tip, paused diverged.
    let after = child_snapshot(&app, ws, repo_id).await;
    assert_eq!(after.tracking_mode, "remote_tracking");
    assert_eq!(after.remote_branch.as_deref(), Some("main"));
    assert!(
        after.baseline_commit.is_none(),
        "an unreconciled bind has no synchronization watermark"
    );
    assert!(
        after.remote_tip_advisory_sha.is_none(),
        "poll hints are collected independently"
    );
    assert_eq!(after.sync_paused_reason.as_deref(), Some("diverged"));

    // Exactly one remote-tracking merge run was opened.
    assert_eq!(
        remote_tracking_merge_run_count(&app, after.key).await,
        1,
        "divergent bind must open exactly one remote-tracking merge run"
    );

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Flag OFF — unchanged [D28] behavior
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// g5.workspace.remote.base-parity / base-provider-rewrite — Base Make Remote
// ---------------------------------------------------------------------------

/// Make Remote on a Base that has its own commits binds it paused, as for any
/// workspace. Accepting the provider history then adopts the remote and
/// discards Base's local commit, pushing nothing and keeping it pinned.
#[tokio::test]
async fn base_make_remote_adopts_provider_history_on_acceptance() {
    let app = TestApp::spawn_with_config(test_config()).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    // Base's owner must be an organization administrator to Make Remote.
    set_org_role(&app, org.id, user.id, "admin").await;
    let base = TestWorkspace::create_base(&app, &org).await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    let mirror = ensure_mirror(&app, repo_id, &bare.path).await;
    // Base keeps its fixture repository as the docs member.
    sqlx::query("UPDATE workspace_repos SET role='docs',root_path='docs' WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id).bind(base.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
        .bind(base.id)
        .bind(base.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,'code','code')",
    )
    .bind(org.id)
    .bind(base.id)
    .bind(repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let provider_tip = git(&bare.path, &["rev-parse", "refs/heads/main"]);

    // A pinned Base that received a local merge is ahead of the provider.
    let local_head = commit_local_ahead(&mirror);
    let (status, body) = promote(
        &app,
        &user,
        base.id,
        repo_id,
        json!({ "remoteBranch": "main" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["outcome"].as_str(),
        Some("merge_required"),
        "{body}"
    );
    let bound = child_snapshot(&app, base.id, repo_id).await;
    assert_eq!(bound.sync_paused_reason.as_deref(), Some("diverged"));
    let auto_pull: bool = sqlx::query_scalar(
        "SELECT auto_pull_enabled FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(base.id)
    .bind(repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(auto_pull, "Base binds with auto-pull on");

    let route = format!(
        "/api/v1/workspaces/{}/repos/{repo_id}/accept_provider_history",
        base.id
    );
    let preview_route = format!(
        "/api/v1/workspaces/{}/repos/{repo_id}/provider_history_preview",
        base.id
    );
    let preview = app
        .authenticated_client(&user)
        .post_json(&preview_route, &json!({}))
        .await;
    let preview_status = preview.status();
    let preview: Value = preview.json().await.unwrap();
    assert_eq!(preview_status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["providerTip"], provider_tip, "{preview}");
    assert_eq!(preview["data"]["head"], local_head, "{preview}");
    assert_eq!(preview["data"]["discardedCommits"], 1, "{preview}");

    // A stale review changes nothing.
    let stale = app
        .authenticated_client(&user)
        .post_json(
            &route,
            &json!({ "expectedProviderTip": local_head, "expectedHead": local_head }),
        )
        .await;
    let stale_status = stale.status();
    let stale: Value = stale.json().await.unwrap();
    assert_eq!(stale_status, StatusCode::UNPROCESSABLE_ENTITY, "{stale}");
    assert_eq!(
        stale["error"]["details"]["code"], "PROVIDER_HISTORY_CHANGED",
        "{stale}"
    );
    assert_eq!(git(&mirror, &["rev-parse", "refs/heads/main"]), local_head);

    let accepted = app
        .authenticated_client(&user)
        .post_json(
            &route,
            &json!({ "expectedProviderTip": provider_tip, "expectedHead": local_head }),
        )
        .await;
    let accepted_status = accepted.status();
    let accepted: Value = accepted.json().await.unwrap();
    assert_eq!(accepted_status, StatusCode::OK, "{accepted}");

    let after = child_snapshot(&app, base.id, repo_id).await;
    assert_eq!(
        after.head_commit, provider_tip,
        "Base adopts the provider tip"
    );
    assert_eq!(
        after.baseline_commit.as_deref(),
        Some(provider_tip.as_str())
    );
    assert_eq!(after.sync_paused_reason, None, "the pause clears");
    assert_eq!(
        git(
            &mirror,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::provider_ref(base.id, "main"),
            ]
        ),
        provider_tip,
        "the provider observation moves with the branch"
    );
    let open_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2
           AND status IN ('pending','running','waiting_for_human')",
    )
    .bind(base.id)
    .bind(repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(open_runs, 0, "acceptance closes the open resolution run");
    let pins = git(
        &mirror,
        &[
            "for-each-ref",
            "--format=%(objectname)",
            "refs/g5/base-accepted/",
        ],
    );
    assert_eq!(pins, local_head, "the discarded head stays pinned");
    assert_eq!(
        bare.ref_sha("main").as_deref(),
        Some(provider_tip.as_str()),
        "nothing is pushed to the provider"
    );

    // Base now follows the provider, so there is nothing left to accept.
    let again = app
        .authenticated_client(&user)
        .post_json(&preview_route, &json!({}))
        .await;
    let again_status = again.status();
    let again: Value = again.json().await.unwrap();
    assert_eq!(again_status, StatusCode::UNPROCESSABLE_ENTITY, "{again}");
    assert_eq!(
        again["error"]["details"]["code"],
        "PROVIDER_HISTORY_ALREADY_FOLLOWED"
    );

    app.cleanup().await;
}

/// Drives the real integration worker until the workspace queue drains.
async fn drain_worker(app: &TestApp, workspace_id: Uuid) {
    use g5_api::maintenance::branch_integration_worker::{
        run_branch_integration_worker, BranchIntegrationWorkerDeps,
    };
    let deps = BranchIntegrationWorkerDeps {
        landed_commit_notifier: std::sync::Arc::new(
            g5_api::services::review_source_notifications::ReviewSourceNotifier::from_state(
                &app.state,
            ),
        ),
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
        repository_credentials: app.state.repository_credentials.clone(),
        event_bus: app.state.event_bus.clone(),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(run_branch_integration_worker(deps, cancel.clone()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_integration_queue WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        if remaining == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not drain the queue"
        );
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    cancel.cancel();
    let _ = handle.await;
}

/// Base never publishes (`g5.workspace.remote.base-parity`). Make Remote on a
/// Base that is ahead binds it paused; once its run has ended, a manual Pull
/// still pushes nothing, and Push to Remote and auto-push are refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn base_make_remote_ahead_never_pushes() {
    let mut config = test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    // Base's owner must be an organization administrator to Make Remote.
    set_org_role(&app, org.id, user.id, "admin").await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    let mirror = ensure_mirror(&app, repo_id, &bare.path).await;
    sqlx::query("UPDATE workspace_repos SET role='docs',root_path='docs' WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id).bind(base.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2, owner_user_id=$3 WHERE id=$1")
        .bind(base.id)
        .bind(base.repo_id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,'code','code')",
    )
    .bind(org.id)
    .bind(base.id)
    .bind(repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let provider_tip = git(&bare.path, &["rev-parse", "refs/heads/main"]);
    let local_head = commit_local_ahead(&mirror);
    let (status, body) = promote(
        &app,
        &user,
        base.id,
        repo_id,
        json!({ "remoteBranch": "main" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        child_snapshot(&app, base.id, repo_id)
            .await
            .sync_paused_reason
            .as_deref(),
        Some("diverged")
    );

    // Push to Remote and auto-push are refused on Base.
    let push = app
        .authenticated_client(&user)
        .post_json(
            &format!("/api/v1/workspaces/{}/push_to_remote", base.id),
            &json!({}),
        )
        .await;
    let push_status = push.status();
    let push: Value = push.json().await.unwrap();
    assert_eq!(push_status, StatusCode::UNPROCESSABLE_ENTITY, "{push}");
    assert!(
        push.to_string()
            .contains("COORDINATED_PUSH_REQUIRES_NON_BASE"),
        "{push}"
    );
    let mode = app
        .authenticated_client(&user)
        .put_json(
            &format!("/api/v1/workspaces/{}/push_mode", base.id),
            &json!({ "pushMode": "auto" }),
        )
        .await;
    assert_eq!(mode.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // The Make Remote run ends without acceptance (as a legacy auto-mode
    // Base would carry it), then the owner pulls.
    sqlx::query(
        "UPDATE workspace_merge_runs SET status='cancelled', completed_at=now(), updated_at=now()
          WHERE workspace_id=$1 AND repo_id=$2 AND status IN ('pending','running','waiting_for_human')",
    )
    .bind(base.id)
    .bind(repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
        .bind(base.id)
        .execute(&app.db)
        .await
        .unwrap();
    let pull = app
        .authenticated_client(&user)
        .post_json(&format!("/api/v1/workspaces/{}/sync", base.id), &json!({}))
        .await;
    let pull_status = pull.status();
    let pull: Value = pull.json().await.unwrap();
    assert_eq!(pull_status, StatusCode::OK, "{pull}");
    drain_worker(&app, base.id).await;

    assert_eq!(
        bare.ref_sha("main").as_deref(),
        Some(provider_tip.as_str()),
        "a manual Pull never pushes Base"
    );
    let after = child_snapshot(&app, base.id, repo_id).await;
    assert_eq!(after.head_commit, local_head);
    assert_eq!(after.sync_paused_reason.as_deref(), Some("diverged"));
    app.cleanup().await;
}

/// Add `repo_id` to Base as its code member, keeping Base's fixture
/// repository as the docs member.
async fn attach_base_code_repo(
    app: &TestApp,
    org: &TestOrg,
    base: &TestWorkspace,
    repo_id: Uuid,
    root: &str,
) {
    sqlx::query("UPDATE workspace_repos SET role='docs',root_path='docs' WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id).bind(base.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
        .bind(base.id)
        .bind(base.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,$4,'code')",
    )
    .bind(org.id)
    .bind(base.id)
    .bind(repo_id)
    .bind(root)
    .execute(&app.db)
    .await
    .unwrap();
}

/// A member of `org_id` holding `org:admin` in their token, with `role` at
/// organization level.
async fn org_user(app: &TestApp, org_id: Uuid, role: &str) -> TestUser {
    let user = TestUser::create_with_scopes(
        app,
        org_id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::OrgAdmin,
        ],
    )
    .await;
    set_org_role(app, org_id, user.id, role).await;
    user
}

async fn set_org_role(app: &TestApp, org_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("UPDATE org_memberships SET role=$3 WHERE org_id=$1 AND user_id=$2")
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .execute(&app.db)
        .await
        .unwrap();
}

async fn post(app: &TestApp, user: &TestUser, route: &str, body: Value) -> (StatusCode, Value) {
    let response = app.authenticated_client(user).post_json(route, &body).await;
    let status = response.status();
    let text = response.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

/// An organization administrator who does not own Base makes its
/// repositories remote and adopts the provider history, acting as themselves
/// (`g5.workspace.remote.base-admin-pull`). An absent provider branch is
/// created only once the administrator confirms it
/// (`g5.workspace.remote.base-parity`). Everyone else is refused with nothing
/// changed.
#[tokio::test]
async fn org_admin_makes_base_remote_and_adopts_provider_history() {
    let app = TestApp::spawn_with_config(test_config()).await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let base_owner: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
        .bind(base.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(base_owner, owner.id, "the first member owns Base");
    let admin = org_user(&app, org.id, "admin").await;
    let member = org_user(&app, org.id, "member").await;
    let admin_without_org_scope = TestUser {
        id: admin.id,
        email: admin.email.clone(),
        display_name: admin.display_name.clone(),
        jwt: helpers::test_jwt_minter()
            .mint(
                admin.id,
                org.id,
                vec![Scope::WorkspacesRead, Scope::WorkspacesWrite],
                3600,
            )
            .unwrap(),
    };
    let other_org = TestOrg::create(&app).await;
    let other_admin = org_user(&app, other_org.id, "admin").await;

    // `code` is ahead of its provider; `lib`'s provider has no `main`.
    let code = make_bare("code");
    let code_id = insert_repo(&app, org.id, "code", &code.url()).await;
    let code_mirror = ensure_mirror(&app, code_id, &code.path).await;
    attach_base_code_repo(&app, &org, &base, code_id, "code").await;
    let lib = make_bare("lib");
    let lib_id = insert_repo(&app, org.id, "lib", &lib.url()).await;
    ensure_mirror(&app, lib_id, &lib.path).await;
    sqlx::query(
        "INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,'lib','code')",
    )
    .bind(org.id)
    .bind(base.id)
    .bind(lib_id)
    .execute(&app.db)
    .await
    .unwrap();
    git(&lib.path, &["symbolic-ref", "HEAD", "refs/heads/other"]);
    git(&lib.path, &["update-ref", "-d", "refs/heads/main"]);
    let provider_tip = git(&code.path, &["rev-parse", "refs/heads/main"]);
    let local_head = commit_local_ahead(&code_mirror);
    let lib_head = child_snapshot(&app, base.id, lib_id).await.head_commit;

    let promote_route = |repo: Uuid| {
        format!(
            "/api/v1/workspaces/{}/repos/{repo}/remote_tracking",
            base.id
        )
    };
    let preview_route = format!(
        "/api/v1/workspaces/{}/repos/{code_id}/provider_history_preview",
        base.id
    );
    let accept_route = format!(
        "/api/v1/workspaces/{}/repos/{code_id}/accept_provider_history",
        base.id
    );
    for (label, user) in [
        ("plain member", &member),
        (
            "admin without org:admin token scope",
            &admin_without_org_scope,
        ),
        ("administrator of another organization", &other_admin),
    ] {
        let (status, body) = post(
            &app,
            user,
            &promote_route(code_id),
            json!({ "remoteBranch": "main" }),
        )
        .await;
        assert!(
            matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
            "{label} Make Remote: {status} {body}"
        );
    }
    for repo in [code_id, lib_id] {
        assert_eq!(
            child_snapshot(&app, base.id, repo).await.tracking_mode,
            "pinned",
            "refusals bind nothing"
        );
    }

    // An absent provider branch needs the administrator's confirmation.
    let (status, body) = post(
        &app,
        &admin,
        &promote_route(lib_id),
        json!({ "remoteBranch": "main" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION",
        "{body}"
    );
    assert_eq!(lib.ref_sha("main"), None, "nothing is pushed unconfirmed");
    assert_eq!(
        child_snapshot(&app, base.id, lib_id).await.tracking_mode,
        "pinned"
    );
    let (status, body) = post(
        &app,
        &admin,
        &promote_route(lib_id),
        json!({ "remoteBranch": "main", "createProviderBranch": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "published", "{body}");
    assert_eq!(
        lib.ref_sha("main").as_deref(),
        Some(lib_head.as_str()),
        "the confirmed branch is created from Base's history"
    );
    let lib_bound = child_snapshot(&app, base.id, lib_id).await;
    assert_eq!(lib_bound.tracking_mode, "remote_tracking");
    assert_eq!(lib_bound.sync_paused_reason, None);

    // A Base ahead of its provider binds paused; the administrator reviews
    // and adopts the provider history.
    let (status, body) = post(
        &app,
        &admin,
        &promote_route(code_id),
        json!({ "remoteBranch": "main" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "merge_required", "{body}");
    assert_eq!(
        child_snapshot(&app, base.id, code_id)
            .await
            .sync_paused_reason
            .as_deref(),
        Some("diverged")
    );
    let actors: Vec<Uuid> = sqlx::query_scalar(
        "SELECT acting_user_id FROM remote_sync_runs WHERE workspace_id=$1 ORDER BY started_at, id",
    )
    .bind(base.id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(
        actors,
        vec![admin.id, admin.id],
        "both bindings are the administrator's"
    );

    for (label, user) in [
        ("plain member", &member),
        (
            "admin without org:admin token scope",
            &admin_without_org_scope,
        ),
        ("administrator of another organization", &other_admin),
    ] {
        let (status, body) = post(&app, user, &preview_route, json!({})).await;
        assert!(
            matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
            "{label} preview: {status} {body}"
        );
        let (status, body) = post(
            &app,
            user,
            &accept_route,
            json!({ "expectedProviderTip": provider_tip, "expectedHead": local_head }),
        )
        .await;
        assert!(
            matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
            "{label} accept: {status} {body}"
        );
    }
    let (status, preview) = post(&app, &admin, &preview_route, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["providerTip"], provider_tip, "{preview}");
    assert_eq!(preview["data"]["head"], local_head, "{preview}");
    assert_eq!(preview["data"]["discardedCommits"], 1, "{preview}");
    assert_eq!(preview["data"]["initialOnly"], false, "{preview}");

    // An administrator demoted after reviewing is refused.
    set_org_role(&app, org.id, admin.id, "member").await;
    let (status, body) = post(
        &app,
        &admin,
        &accept_route,
        json!({ "expectedProviderTip": provider_tip, "expectedHead": local_head }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        git(&code_mirror, &["rev-parse", "refs/heads/main"]),
        local_head
    );
    set_org_role(&app, org.id, admin.id, "admin").await;

    let (status, body) = post(
        &app,
        &admin,
        &accept_route,
        json!({ "expectedProviderTip": provider_tip, "expectedHead": local_head }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let after = child_snapshot(&app, base.id, code_id).await;
    assert_eq!(
        after.head_commit, provider_tip,
        "Base adopts the provider tip"
    );
    assert_eq!(after.sync_paused_reason, None, "the pause clears");
    assert_eq!(
        git(
            &code_mirror,
            &[
                "for-each-ref",
                "--format=%(objectname)",
                "refs/g5/base-accepted/"
            ],
        ),
        local_head,
        "the discarded head stays pinned"
    );
    assert_eq!(
        code.ref_sha("main").as_deref(),
        Some(provider_tip.as_str()),
        "nothing is pushed to the provider"
    );
    let owner_actions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM remote_sync_runs WHERE workspace_id=$1 AND acting_user_id=$2",
    )
    .bind(base.id)
    .bind(owner.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(owner_actions, 0, "nothing is attributed to Base's owner");
    app.cleanup().await;
}

/// Make Remote on a workspace whose repository holds only its initial commit
/// binds it paused against the provider's unrelated history. The owner
/// adopts the provider history in the running VM: G5 moves to the reviewed
/// tip under a lease, nothing reaches the provider, and the run completes
/// through the ordinary proof (`g5.workspace.remote.adopt-provider-history`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_adopts_provider_history_over_an_initial_commit_in_the_vm() {
    let mut config = test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    // Make Remote needs an owner who is an organization administrator; `admin`
    // below is a separate non-owner administrator used for the refusal checks.
    set_org_role(&app, org.id, user.id, "admin").await;
    let admin = org_user(&app, org.id, "admin").await;
    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url()).await;
    let mirror = ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "init-only").await;
    let key = child_snapshot(&app, ws, repo_id).await.key;
    let empty_tree = git(&mirror, &["hash-object", "-t", "tree", "/dev/null"]);
    let init = git(
        &mirror,
        &["commit-tree", &empty_tree, "-m", "Initial commit"],
    );
    set_child_head(&app, &mirror, key, &init).await;
    let provider_tip = git(&bare.path, &["rev-parse", "refs/heads/main"]);

    let (status, body) = promote(&app, &user, ws, repo_id, json!({ "remoteBranch": "main" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "merge_required", "{body}");
    assert_eq!(
        child_snapshot(&app, ws, repo_id)
            .await
            .sync_paused_reason
            .as_deref(),
        Some("diverged")
    );
    let run_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2 AND source_kind='remote_tracking'",
    )
    .bind(ws)
    .bind(repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let preview_route = format!("/api/v1/workspaces/{ws}/repos/{repo_id}/provider_history_preview");
    let accept_route = format!("/api/v1/workspaces/{ws}/repos/{repo_id}/accept_provider_history");
    let (status, preview) = post(&app, &user, &preview_route, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["providerTip"], provider_tip, "{preview}");
    assert_eq!(preview["data"]["head"], init, "{preview}");
    assert_eq!(preview["data"]["discardedCommits"], 1, "{preview}");
    assert_eq!(preview["data"]["initialOnly"], true, "{preview}");
    let review = json!({ "expectedProviderTip": provider_tip, "expectedHead": init });

    // Adopting off Base is the owner's alone.
    let (status, body) = post(&app, &admin, &preview_route, json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = post(&app, &admin, &accept_route, review.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // Adopting needs the running owner VM.
    let (status, body) = post(&app, &user, &accept_route, review.clone()).await;
    assert!(!status.is_success(), "a stopped VM adopts nothing: {body}");
    assert_eq!(child_snapshot(&app, ws, repo_id).await.head_commit, init);

    let removed = sqlx::query(
        "DELETE FROM workspace_vms WHERE workspace_id=$1 AND status='stopped' AND host_id IS NULL",
    )
    .bind(ws)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(removed.rows_affected(), 1);
    let workspace_name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(ws)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let running = TestWorkspace {
        id: ws,
        org_id: org.id,
        repo_id,
        name: workspace_name,
        head_commit: init.clone(),
    };
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &running, user.id).await;

    // A stale review changes nothing.
    let (status, body) = post(
        &app,
        &user,
        &accept_route,
        json!({ "expectedProviderTip": init, "expectedHead": init }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "PROVIDER_HISTORY_CHANGED",
        "{body}"
    );
    assert_eq!(child_snapshot(&app, ws, repo_id).await.head_commit, init);

    let (status, body) = post(&app, &user, &accept_route, review.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["head"], provider_tip, "{body}");
    assert!(
        runtime
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "modules.invoke"),
        "the VM ran the adoption"
    );
    assert_eq!(
        child_snapshot(&app, ws, repo_id).await.head_commit,
        provider_tip,
        "G5 holds the provider tip"
    );
    assert_eq!(
        git(
            &mirror,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::resolution_ref(ws, run_id, "local"),
            ]
        ),
        init,
        "the replaced head stays pinned"
    );

    // The guest's push lands through receive-pack, which signals the
    // integration worker (`routes/git.rs`); the scripted guest moved the ref
    // directly, so send that signal here. The worker then completes the run
    // through the ordinary proof.
    let mut db = app.scoped_db_for_org(org.id).await;
    g5_api_core::services::remote_integrations::enqueue_after_push(db.conn(), key)
        .await
        .unwrap();
    db.commit().await.unwrap();
    drain_worker(&app, ws).await;
    let after = child_snapshot(&app, ws, repo_id).await;
    assert_eq!(after.head_commit, provider_tip);
    assert_eq!(after.sync_paused_reason, None, "the pause clears");
    let run_status: String =
        sqlx::query_scalar("SELECT status FROM workspace_merge_runs WHERE id=$1")
            .bind(run_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(run_status, "completed");
    assert_eq!(
        bare.ref_sha("main").as_deref(),
        Some(provider_tip.as_str()),
        "nothing is pushed to the provider"
    );

    // With the divergence settled there is nothing left to adopt.
    let (status, body) = post(&app, &user, &preview_route, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    drop(runtime);
    app.cleanup().await;
}
