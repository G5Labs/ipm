//! Make-Remote **Adopt-Remote** git-URL re-point integration tests
//! (`workspace_service` D46 item 1; `models/repo` D15).
//!
//! These tests drive the REAL promote-remote-tracking route
//! (`POST /workspaces/:id/branches/:branchId/repos/:repoId/remote_tracking`)
//! against a real Postgres tenant DB, real repo storage, and real local bare
//! git remotes — no mocks/stubs of in-scope services (Tier 1 discipline). They
//! model `coordinated_repo_sync.rs` / `remote_repo_tracking_integration.rs`.
//!
//! Scope: ONLY the Adopt-Remote RE-POINT half of D46 (item 1). The absent-ref
//! create path (item 2) and divergent-lineage merge routing (item 3) are owned
//! by a later lane and are not exercised here.
//!
//! Coverage:
//!   * non-admin actor → 403, no mutation of the shared `repos` row;
//!   * org-admin re-point → `repos.id` stable, `git_url`/provider re-pointed,
//!     a sibling workspace's child row byte-identical (siblings untouched);
//!   * already-remote row with a DIFFERENT supplied url → `MAKE_REMOTE_URL_CONFLICT`;
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
    /// removed on `Drop` so tests do not leak `temp_dir()/g5-make-remote-*` dirs.
    root: PathBuf,
    path: PathBuf,
    tip: String,
}

impl Bare {
    fn url(&self) -> String {
        self.path.display().to_string()
    }
}

impl Drop for Bare {
    fn drop(&mut self) {
        // Best-effort cleanup of the temp root (bare + seed). Ignore errors:
        // a failed removal must never mask a test assertion failure.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Create a real bare git repo with one seed commit on `main`.
fn make_bare(name: &str) -> Bare {
    let root = std::env::temp_dir().join(format!("g5-make-remote-{}", Uuid::new_v4().simple()));
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

    let tip = git(&bare, &["rev-parse", "refs/heads/main"]);
    Bare {
        root,
        path: bare,
        tip,
    }
}

/// Insert a `repos` row with an explicit `git_url` / provider metadata.
async fn insert_repo(
    app: &TestApp,
    org_id: Uuid,
    name: &str,
    git_url: &str,
    provider: Option<&str>,
    provider_full_name: Option<&str>,
) -> Uuid {
    let repo_id = Uuid::new_v4();
    let slug = format!("adopt-{name}-{}", repo_id.simple());
    sqlx::query(
        r#"
        INSERT INTO repos
            (id, org_id, name, slug, git_url, default_branch, provider, provider_repo_id, provider_full_name)
        VALUES ($1, $2, $3, $4, $5, 'main', $6, $7, $7)
        "#,
    )
    .bind(repo_id)
    .bind(org_id)
    .bind(name)
    .bind(&slug)
    .bind(git_url)
    .bind(provider)
    .bind(provider_full_name)
    .execute(&app.db)
    .await
    .expect("insert repo row");
    repo_id
}

/// Clone the bare into the shared mirror at `repo_storage.local_path(repo_id)`
/// so create-time materialization and the promote classification can read it.
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

async fn fork_workspace(
    app: &TestApp,
    user: &TestUser,
    source_workspace_id: Uuid,
    name: &str,
) -> Uuid {
    let resp = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/workspaces/{source_workspace_id}/clone"),
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
    fork_workspace(app, user, source.id, name).await
}

async fn set_org_role(app: &TestApp, org_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("UPDATE org_memberships SET role = $3 WHERE org_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .execute(&app.db)
        .await
        .expect("update org role");
}

async fn repo_git_url(app: &TestApp, repo_id: Uuid) -> String {
    sqlx::query_scalar("SELECT git_url FROM repos WHERE id = $1")
        .bind(repo_id)
        .fetch_one(&app.db)
        .await
        .expect("repo git_url")
}

/// The mutable columns of a `workspace_branch_repos` child, used to prove a
/// sibling row is byte-identical across an Adopt-Remote re-point.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Non-admin (org `member`) with `workspaces:write` is refused the Adopt-Remote
/// re-point with 403 and no mutation of the shared identity.
#[tokio::test]
async fn adopt_remote_requires_org_admin_no_mutation() {
    let config = test_config();
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    // `create_with_scopes` inserts an org membership with role = 'member'.
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url(), None, None).await;
    ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "adopt-authz").await;

    // Flip the shared repo to a `g5-local://` identity so a differing gitUrl is
    // an Adopt-Remote attempt.
    let local_url = format!("g5-local://workspaces/{ws}/repos/code.git");
    sqlx::query("UPDATE repos SET git_url = $1, provider = NULL WHERE id = $2")
        .bind(&local_url)
        .bind(repo_id)
        .execute(&app.db)
        .await
        .expect("flip repo to g5-local identity");

    let (status, body) = promote(
        &app,
        &user,
        ws,
        repo_id,
        json!({ "remoteBranch": "main", "gitUrl": bare.url() }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "non-admin Adopt-Remote must be forbidden: {body}"
    );
    assert_eq!(
        repo_git_url(&app, repo_id).await,
        local_url,
        "a forbidden Adopt-Remote must not mutate repos.git_url"
    );
    assert_eq!(
        child_snapshot(&app, ws, repo_id).await.tracking_mode,
        "pinned",
        "a forbidden Adopt-Remote must leave the child pinned"
    );

    app.cleanup().await;
}

/// Org-admin re-point: `repos.id` stays stable, `git_url`/provider are
/// re-pointed to the adopted remote, and a sibling workspace's child row is
/// byte-identical (siblings inherit implicitly and are never rebound).
#[tokio::test]
async fn adopt_remote_admin_repoints_repo_id_stable_and_sibling_untouched() {
    let config = test_config();
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    set_org_role(&app, org.id, user.id, "admin").await;

    // Insert pointing at the real bare so create-time materialization succeeds,
    // then simulate a project born local by re-pointing to a g5-local identity.
    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url(), None, None).await;
    ensure_mirror(&app, repo_id, &bare.path).await;

    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "adopt-primary").await;
    let sibling = fork_workspace(&app, &user, ws, "adopt-sibling").await;

    let local_url = format!("g5-local://workspaces/{ws}/repos/code.git");
    sqlx::query(
        "UPDATE repos SET git_url = $1, provider = NULL, provider_repo_id = NULL, provider_full_name = NULL WHERE id = $2",
    )
    .bind(&local_url)
    .bind(repo_id)
    .execute(&app.db)
    .await
    .expect("flip repo to g5-local identity");

    let sibling_before = child_snapshot(&app, sibling, repo_id).await;

    let (status, body) = promote(
        &app,
        &user,
        ws,
        repo_id,
        json!({ "remoteBranch": "main", "gitUrl": bare.url() }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "admin Adopt-Remote should succeed: {body}"
    );

    // repos.id stable + git_url/provider re-pointed. Assert EXACT values: a bare
    // filesystem-path remote yields `effective_provider(None, barepath) == None`
    // (no host to infer a provider from), while `provider_full_name` is the
    // parsed `owner/repo` tail of the path (leading '/' and trailing '.git'
    // stripped) — mirroring `repo_full_name_from_git_url`.
    let (still_id, new_url, provider, provider_full): (
        Uuid,
        String,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as("SELECT id, git_url, provider, provider_full_name FROM repos WHERE id = $1")
        .bind(repo_id)
        .fetch_one(&app.db)
        .await
        .expect("repo row after adopt");
    assert_eq!(
        still_id, repo_id,
        "repos.id must stay stable across Adopt-Remote"
    );
    assert_eq!(
        new_url,
        bare.url(),
        "git_url must be re-pointed to the adopted remote url"
    );
    assert_eq!(
        provider, None,
        "a filesystem-path remote infers no provider, so provider must stay NULL"
    );
    let expected_full = {
        let u = bare.url();
        let trimmed = u.trim_matches('/');
        trimmed
            .strip_suffix(".git")
            .unwrap_or(trimmed)
            .trim_matches('/')
            .to_string()
    };
    assert_eq!(
        provider_full.as_deref(),
        Some(expected_full.as_str()),
        "provider_full_name must be the parsed owner/repo tail of the adopted path"
    );

    // Primary child is now remote-tracked at the adopted tip.
    let primary = child_snapshot(&app, ws, repo_id).await;
    assert_eq!(primary.tracking_mode, "remote_tracking");
    assert_eq!(primary.remote_branch.as_deref(), Some("main"));
    assert_eq!(primary.baseline_commit.as_deref(), Some(bare.tip.as_str()));

    // Sibling child row is byte-identical (siblings untouched — they inherit the
    // new git_url implicitly via the shared repos row).
    let sibling_after = child_snapshot(&app, sibling, repo_id).await;
    assert_eq!(
        sibling_before, sibling_after,
        "sibling child row must be byte-identical after Adopt-Remote"
    );

    app.cleanup().await;
}

/// A repo already bound to a DIFFERENT remote url cannot be re-pointed:
/// `MAKE_REMOTE_URL_CONFLICT`, no mutation.
#[tokio::test]
async fn adopt_remote_conflicts_when_already_remote_different_url() {
    let config = test_config();
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    set_org_role(&app, org.id, user.id, "admin").await;

    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url(), None, None).await;
    ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "adopt-conflict").await;

    // Existing identities stay bound even when the stricter authentication
    // policy rejects their transport or stored provider metadata.
    for (existing, provider) in [
        ("https://github.com/g5test/orig.git", Some("github")),
        ("http://github.com/g5test/orig.git", None),
        ("git@github.com:g5test/orig.git", None),
        ("https://gitlab.com/g5test/orig.git", Some("github")),
    ] {
        sqlx::query(
        "UPDATE repos SET git_url = $1, provider = $3, provider_repo_id = 'g5test/orig', provider_full_name = 'g5test/orig' WHERE id = $2",
    )
    .bind(existing)
    .bind(repo_id)
    .bind(provider)
    .execute(&app.db)
    .await
    .expect("set repo already-remote");

        let (status, body) = promote(
            &app,
            &user,
            ws,
            repo_id,
            json!({ "remoteBranch": "main", "gitUrl": "https://github.com/g5test/other.git" }),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "re-pointing an already-remote repo to a different url must conflict: {body}"
        );
        assert_eq!(
            body["error"]["details"]["code"].as_str(),
            Some("MAKE_REMOTE_URL_CONFLICT"),
        );
        assert_eq!(
            repo_git_url(&app, repo_id).await,
            existing,
            "a conflicting Adopt-Remote must not mutate repos.git_url"
        );
    }

    app.cleanup().await;
}

/// Invalid network origins must fail before contacting the remote or mutating
/// shared identity, even when local fixture remotes are explicitly enabled.
#[tokio::test]
async fn adopt_remote_rejects_untrusted_scm_origins_without_mutation() {
    let mut config = test_config();
    config.allow_local_git_remotes = true;
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &rw_user_scopes()).await;
    set_org_role(&app, org.id, user.id, "admin").await;
    let bare = make_bare("code");
    let repo_id = insert_repo(&app, org.id, "code", &bare.url(), None, None).await;
    ensure_mirror(&app, repo_id, &bare.path).await;
    let ws = create_pinned_workspace(&app, &user, &org, &[repo_id], "adopt-origin").await;
    let before = child_snapshot(&app, ws, repo_id).await;
    for url in [
        "https://github.com.attacker.example/o/r.git",
        "https://attacker.example/github.com/r.git",
        "http://github.com/o/r.git",
        "https://user@github.com/o/r.git",
        "https://github.com:8443/o/r.git",
        "ssh://git@github.com/o/r.git",
    ] {
        let (status, body) = promote(
            &app,
            &user,
            ws,
            repo_id,
            json!({"remoteBranch": "main", "gitUrl": url}),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{url}: {body}");
        assert_eq!(body["error"]["details"]["code"], "MAKE_REMOTE_URL_INVALID");
        assert_eq!(repo_git_url(&app, repo_id).await, bare.url());
        assert_eq!(child_snapshot(&app, ws, repo_id).await, before);
    }
    app.cleanup().await;
}

#[tokio::test]
async fn adopt_remote_uses_configured_gitlab_origin_without_token_manager() {
    let mut config = test_config();
    config.auth.scm_encryption_key = None;
    config.auth.scm_gitlab = None;
    config.auth.scm_gitlab_userinfo_url = Some("https://127.0.0.1:1/api/v4/user".into());
    let app = TestApp::spawn_with_config(config).await;
    assert!(app.state.scm_token_manager.is_none());
    // The normal org factory seeds an encrypted LLM key. This route must also
    // enforce the origin policy when no encryption manager exists at all.
    let org = TestOrg {
        id: Uuid::new_v4(),
        name: "Configured origin".into(),
        slug: format!("configured-origin-{}", Uuid::new_v4()),
    };
    let mut user = TestUser::create(&app).await;
    sqlx::query(
        "WITH org AS (INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $3) RETURNING id)
        INSERT INTO org_memberships (org_id,user_id,role) SELECT id,$4,'admin' FROM org",
    )
    .bind(org.id)
    .bind(&org.name)
    .bind(&org.slug)
    .bind(user.id)
    .execute(&app.db)
    .await
    .unwrap();
    user.jwt = crate::helpers::test_jwt_minter()
        .mint(user.id, org.id, rw_user_scopes().to_vec(), 3600)
        .unwrap();
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_id = workspace.repo_id;
    let workspace_id =
        create_pinned_workspace(&app, &user, &org, &[repo_id], "configured-origin").await;
    let original_url = repo_git_url(&app, repo_id).await;
    for (url, allowed_origin) in [
        ("https://127.0.0.1:1/o/r.git", true),
        ("https://gitlab.com/o/r.git", false),
    ] {
        let (status, body) = promote(
            &app,
            &user,
            workspace_id,
            repo_id,
            json!({"remoteBranch": "main", "gitUrl": url}),
        )
        .await;
        assert!(
            status.is_client_error() || status.is_server_error(),
            "closed transport must not publish: {body}"
        );
        let code = body["error"]["details"]["code"].as_str();
        assert_eq!(
            code == Some("MAKE_REMOTE_URL_INVALID"),
            !allowed_origin,
            "{url}: {body}"
        );
        if allowed_origin {
            assert!(
                body["error"]["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("no_scm_token_manager"),
                "{body}"
            );
        }
        assert_eq!(repo_git_url(&app, repo_id).await, original_url);
    }
    app.cleanup().await;
}
