//! Live integration tests for the **branch integration worker** — the
//! Phase 2.B remote-repo-tracking sync pipeline (G5 hardening, chat C22).
//!
//! Unlike the inline `classify_and_act` unit tests (which exercise the pure
//! decision function in isolation), this module drives the REAL worker entry
//! point — [`run_branch_integration_worker`] — end to end against:
//!
//!   * a real Postgres tenant database (isolated per test via `TestApp`),
//!   * a real local git **bare remote** standing in for the customer's
//!     upstream, and
//!   * a real on-disk **mirror** clone at `repo_storage.local_path(repo_id)`.
//!
//! Each test seeds a canonical member and `remote_integrations` policy + a
//! `remote_integration_queue` entry, then spawns the worker loop and polls
//! until the queue row is drained (deterministic synchronization — no fixed
//! sleeps). It then asserts the durable side effects of one classify →
//! fetch/ls-remote → execute cycle:
//!
//!   * **Noop** (`Equal`)            — tips equal → no mutation.            (D10)
//!   * **Imported** (`MirrorAhead`)  — remote moved, clean → head advances. (D13)
//!   * **Pushed** (`WorkspaceAhead`) — local ahead → remote ref advances.   (D11/D12)
//!   * **DirtySkipped** (`MirrorAhead` + dirty) — import deferred.          (D15)
//!   * **Diverged** (`Diverged`)     — both moved → pause + open merge run. (D17)
//!
//! No mocks or stubs of in-scope services (Tier 1 discipline): the git
//! transport, the worker, the DB, and the merge-handoff write are all real.
//!
//! SOT:
//! - [`remote_repo_tracking.arch_view.md`](../../views/arch_views/remote_repo_tracking.arch_view.md) D10–D18
//! - [`remote_repo_tracking.test_view.md`](../../views/test_views/remote_repo_tracking.test_view.md) §3 worker-pipeline scenarios
//! - [`remote_tracking_gap_closure.plan.md`](../../aux_files/future_implementation/remote_tracking_gap_closure.plan.md) Phase G5
//! - Chat: `remote-tracking-gap-closure-plan-2026-05-28` (C22)

use crate::helpers;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use g5_api::services::remote_integrations;
use g5_auth::Scope;
use g5_contracts::types::WorkspaceRepoKey;
use g5_scm::GitRepo;
use helpers::{seed_workspace_authority_head, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// git helpers — drive a real local bare remote + mirror clone
// ---------------------------------------------------------------------------

/// Runs `git <args>` in `dir`, asserting success, and returns trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?} in {dir:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} in {dir:?} failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write_file(dir: &Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents)
        .unwrap_or_else(|e| panic!("write {name} in {dir:?}: {e}"));
}

/// Fixture topology for one remote-tracking branch under test.
#[allow(dead_code)] // `workspace_id` / `repo_id` document the fixture identity.
struct TrackingFixture {
    org_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    workspace_branch: String,
    /// Bare repo standing in for the customer's upstream (`repos.git_url`).
    bare: PathBuf,
    /// Working clone used to advance the upstream ("remote moved").
    seed: PathBuf,
    /// The mirror the worker fetches into / pushes from.
    mirror: PathBuf,
    /// Additional mirrors attached by scenario-specific coordinated repo setup.
    extra_mirrors: Vec<PathBuf>,
    /// Initial commit on `main` (workspace tip + baseline at setup time).
    c1: String,
    /// Root temp dir (bare + seed); cleaned up best-effort by the test.
    tmp_root: PathBuf,
}

impl TrackingFixture {
    /// Builds the bare remote (main @ C1), a working seed clone, and the
    /// mirror clone at `repo_storage.local_path(repo_id)`, then inserts the
    /// canonical membership and its remote integration policy.
    async fn setup(
        app: &TestApp,
        org: &TestOrg,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
    ) -> Self {
        // The worker stamps `remote_sync_runs.acting_user_id` from the current
        // workspace owner and resolves the SCM token as that user.
        sqlx::query("UPDATE workspaces SET owner_user_id = $2, push_mode = 'manual' WHERE id = $1")
            .bind(workspace.id)
            .bind(owner_user_id)
            .execute(&app.db)
            .await
            .expect("set current workspace owner");

        let tmp_root = std::env::temp_dir().join(format!("g5-rt-it-{}", Uuid::new_v4().simple()));
        let bare = tmp_root.join("remote.git");
        let seed = tmp_root.join("seed");
        std::fs::create_dir_all(&bare).expect("create bare dir");
        std::fs::create_dir_all(&seed).expect("create seed dir");

        // Bare remote.
        git(&bare, &["init", "--bare", "--initial-branch=main"]);

        // Seed working clone → commit C1 on main → push to the bare remote.
        git(&seed, &["init", "--initial-branch=main"]);
        git(&seed, &["config", "user.email", "g5@test.local"]);
        git(&seed, &["config", "user.name", "G5 Test"]);
        write_file(&seed, "README.md", "remote tracking fixture\n");
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-m", "C1 seed"]);
        let c1 = git(&seed, &["rev-parse", "HEAD"]);
        git(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&seed, &["push", "origin", "main"]);
        // Pin the bare HEAD so a plain clone lands on `main`.
        git(&bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);

        // Mirror = clone of the bare into the deterministic storage path.
        let repo_id = Uuid::new_v4();
        let mirror = app.repo_storage.test_path(org.id, repo_id);
        if let Some(parent) = mirror.parent() {
            std::fs::create_dir_all(parent).expect("create mirror parent dir");
        }
        git(
            &tmp_root,
            &["clone", bare.to_str().unwrap(), mirror.to_str().unwrap()],
        );
        git(&mirror, &["config", "user.email", "g5@test.local"]);
        git(&mirror, &["config", "user.name", "G5 Test"]);
        git(
            &mirror,
            &["update-ref", &format!("refs/heads/{}", workspace.name), &c1],
        );

        // repos row — git_url points at the bare remote; provider stays NULL
        // so the worker's auth path resolves GitAuth::None (no token needed
        // for the local transport) instead of misclassifying auth_required.
        let slug = format!("rt-repo-{}", repo_id.simple());
        sqlx::query(
            r#"
            INSERT INTO repos (id, org_id, name, slug, git_url, default_branch)
            VALUES ($1, $2, $3, $4, $5, 'main')
            "#,
        )
        .bind(repo_id)
        .bind(org.id)
        .bind(&slug)
        .bind(&slug)
        .bind(bare.to_str().unwrap())
        .execute(&app.db)
        .await
        .expect("insert tracked repo");

        sqlx::query(
            r#"
            INSERT INTO workspace_repos (
                org_id, workspace_id, repo_id, root_path, role
            )
            VALUES ($1, $2, $3, $4, 'code')
            "#,
        )
        .bind(org.id)
        .bind(workspace.id)
        .bind(repo_id)
        .bind(&slug)
        .execute(&app.db)
        .await
        .expect("insert authoritative tracked repository mapping");

        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled)
                     VALUES ($1,$2,$3,'main',true)")
            .bind(org.id).bind(workspace.id).bind(repo_id)
            .execute(&app.db).await.expect("insert remote integration");
        GitRepo::open(&mirror)
            .unwrap()
            .write_baseline_ref(&workspace.name, &c1)
            .unwrap();

        Self {
            org_id: org.id,
            workspace_id: workspace.id,
            repo_id,
            workspace_branch: workspace.name.clone(),
            bare,
            seed,
            mirror,
            extra_mirrors: Vec::new(),
            c1,
            tmp_root,
        }
    }

    /// Adds a real remote-tracked docs repo to the workspace so the
    /// coordinated PushToRemote route sees the required docs+code repo set.
    async fn attach_remote_tracked_docs_repo(
        &mut self,
        app: &TestApp,
        org: &TestOrg,
        workspace: &TestWorkspace,
    ) {
        let docs_repo_id = Uuid::new_v4();
        let docs_bare = self.tmp_root.join("docs-remote.git");
        let docs_seed = self.tmp_root.join("docs-seed");
        std::fs::create_dir_all(&docs_bare).expect("create docs bare dir");
        std::fs::create_dir_all(&docs_seed).expect("create docs seed dir");

        git(&docs_bare, &["init", "--bare", "--initial-branch=main"]);
        git(&docs_seed, &["init", "--initial-branch=main"]);
        git(&docs_seed, &["config", "user.email", "g5@test.local"]);
        git(&docs_seed, &["config", "user.name", "G5 Test"]);
        write_file(&docs_seed, "README.md", "remote tracking docs fixture\n");
        git(&docs_seed, &["add", "."]);
        git(&docs_seed, &["commit", "-m", "docs C1 seed"]);
        let docs_c1 = git(&docs_seed, &["rev-parse", "HEAD"]);
        git(
            &docs_seed,
            &["remote", "add", "origin", docs_bare.to_str().unwrap()],
        );
        git(&docs_seed, &["push", "origin", "main"]);
        git(&docs_bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);

        let docs_mirror = app.repo_storage.test_path(org.id, docs_repo_id);
        if let Some(parent) = docs_mirror.parent() {
            std::fs::create_dir_all(parent).expect("create docs mirror parent dir");
        }
        git(
            &self.tmp_root,
            &[
                "clone",
                docs_bare.to_str().unwrap(),
                docs_mirror.to_str().unwrap(),
            ],
        );
        git(&docs_mirror, &["config", "user.email", "g5@test.local"]);
        git(&docs_mirror, &["config", "user.name", "G5 Test"]);
        git(
            &docs_mirror,
            &[
                "update-ref",
                &format!("refs/heads/{}", workspace.name),
                &docs_c1,
            ],
        );

        let slug = format!("rt-docs-{}", docs_repo_id.simple());
        sqlx::query(
            r#"
            INSERT INTO repos (id, org_id, name, slug, git_url, default_branch)
            VALUES ($1, $2, 'docs', $3, $4, 'main')
            "#,
        )
        .bind(docs_repo_id)
        .bind(org.id)
        .bind(&slug)
        .bind(docs_bare.to_str().unwrap())
        .execute(&app.db)
        .await
        .expect("insert tracked docs repo");

        sqlx::query(
            r#"
            UPDATE workspaces
               SET docs_repo_id = $2
             WHERE id = $1
            "#,
        )
        .bind(workspace.id)
        .bind(docs_repo_id)
        .execute(&app.db)
        .await
        .expect("attach docs repo to workspace");

        sqlx::query(
            r#"
            INSERT INTO workspace_repos (
                org_id, workspace_id, repo_id, root_path, role
            )
            VALUES ($1, $2, $3, 'docs', 'docs')
            "#,
        )
        .bind(org.id)
        .bind(workspace.id)
        .bind(docs_repo_id)
        .execute(&app.db)
        .await
        .expect("insert authoritative docs repository mapping");

        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled)
                     VALUES ($1,$2,$3,'main',true)")
            .bind(org.id).bind(workspace.id).bind(docs_repo_id)
            .execute(&app.db).await.expect("insert docs remote integration");
        GitRepo::open(&docs_mirror)
            .unwrap()
            .write_baseline_ref(&workspace.name, &docs_c1)
            .unwrap();

        self.extra_mirrors.push(docs_mirror);
    }

    /// Advances the upstream: commit a new tip on `main` in the seed clone and
    /// push it to the bare remote. Returns the new remote tip SHA.
    fn advance_remote(&self, file: &str, contents: &str) -> String {
        git(&self.seed, &["checkout", "main"]);
        write_file(&self.seed, file, contents);
        git(&self.seed, &["add", "."]);
        git(&self.seed, &["commit", "-m", "remote moved"]);
        let tip = git(&self.seed, &["rev-parse", "HEAD"]);
        git(&self.seed, &["push", "origin", "main"]);
        tip
    }

    /// Advances the local mirror's exact workspace branch.
    /// Returns the new local tip SHA. Used to simulate the workspace being
    /// ahead of / diverged from the upstream.
    fn advance_mirror(&self, file: &str, contents: &str) -> String {
        git(&self.mirror, &["checkout", &self.workspace_branch]);
        write_file(&self.mirror, file, contents);
        git(&self.mirror, &["add", "."]);
        git(&self.mirror, &["commit", "-m", "local work"]);
        git(&self.mirror, &["rev-parse", "HEAD"])
    }

    /// Reads the bare remote's current `main` tip.
    fn remote_tip(&self) -> String {
        git(&self.bare, &["rev-parse", "refs/heads/main"])
    }

    fn mirror_origin_url(&self) -> String {
        git(&self.mirror, &["config", "--get", "remote.origin.url"])
    }

    /// Seeds one exact committed workspace cut for worker classification.
    async fn set_workspace_tip(&self, app: &TestApp, sha: &str) {
        seed_workspace_authority_head(app, self.workspace_id, self.repo_id, sha).await;
    }

    fn key(&self) -> WorkspaceRepoKey {
        WorkspaceRepoKey {
            org_id: self.org_id,
            workspace_id: self.workspace_id,
            repo_id: self.repo_id,
        }
    }

    async fn seed_gitlab_connection_for_owner(&self, app: &TestApp, user_id: Uuid) {
        // Used only after a test binds this fixture to a configured GitLab origin.
        let encrypted = app
            .state
            .scm_token_manager
            .as_ref()
            .expect("test app should have scm token manager")
            .encrypt_token("ghp_remote_tracking_test_token")
            .expect("encrypt test scm token");

        sqlx::query(
            r#"
            INSERT INTO user_scm_connections (
                id, user_id, provider, provider_user_id, provider_username,
                access_token_enc, scopes
            )
            VALUES ($1, $2, 'gitlab', $3, $4, $5, ARRAY['read_repository']::text[])
            ON CONFLICT (user_id, provider)
            DO UPDATE SET
                provider_user_id = EXCLUDED.provider_user_id,
                provider_username = EXCLUDED.provider_username,
                access_token_enc = EXCLUDED.access_token_enc,
                scopes = EXCLUDED.scopes,
                updated_at = now()
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind("remote-tracking-owner")
        .bind("remote-tracking-owner")
        .bind(encrypted)
        .execute(&app.db)
        .await
        .expect("seed owner scm connection");
    }

    /// Inserts the pending integration-queue row that the worker drains.
    async fn enqueue(&self, app: &TestApp, trigger_kind: &str) {
        let mut db = app.scoped_db_for_org(self.org_id).await;
        enqueue_test_request(db.conn(), self.key(), trigger_kind).await;
        db.commit().await.unwrap();
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.tmp_root);
        let _ = std::fs::remove_dir_all(&self.mirror);
        for mirror in &self.extra_mirrors {
            let _ = std::fs::remove_dir_all(mirror);
        }
    }
}

// ---------------------------------------------------------------------------
// Worker driver
// ---------------------------------------------------------------------------

/// Spawns a `TestApp` whose runtime pools are sized for the worker's sweep
/// (which holds the global advisory-lock connection for the duration of an
/// org drain while `resolve_auth` acquires a second system-pool connection).
async fn spawn_worker_app() -> TestApp {
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    TestApp::spawn_with_config(config).await
}

/// Drives the REAL worker loop until it drains the given queue row, then
/// shuts it down. `tokio::time::interval`'s first tick fires immediately, so
/// the first sweep runs without waiting out `SWEEP_INTERVAL`. We poll the
/// queue table (not a fixed sleep) so the test synchronizes on the cycle
/// actually committing.
async fn drain_one_cycle(app: &TestApp, key: WorkspaceRepoKey) {
    use g5_api::maintenance::branch_integration_worker::{
        run_branch_integration_worker, BranchIntegrationWorkerDeps,
    };
    use tokio_util::sync::CancellationToken;

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

    let cancel = CancellationToken::new();
    let handle = tokio::spawn(run_branch_integration_worker(deps, cancel.clone()));

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
        )
        .bind(key.workspace_id)
    .bind(key.repo_id)
        .fetch_one(&app.db)
        .await
        .expect("count queue rows");

        if remaining == 0 {
            break;
        }
        if Instant::now() > deadline {
            // Surface any audit row the worker may have written so a hang is
            // diagnosable (e.g. a classify/fetch error that left the row
            // locked rather than drained).
            let diag: Vec<(String, Option<String>)> = sqlx::query_as(
                "SELECT status, error_detail FROM remote_sync_runs WHERE workspace_id = $1 AND repo_id = $2 ORDER BY started_at DESC",
            )
            .bind(key.workspace_id)
    .bind(key.repo_id)
            .fetch_all(&app.db)
            .await
            .unwrap_or_default();
            cancel.cancel();
            let _ = handle.await;
            panic!("worker did not drain queue row within 45s; audit rows = {diag:?}");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    cancel.cancel();
    let _ = handle.await;
}

/// Fetches the single audit row the worker wrote this cycle.
async fn latest_run(
    app: &TestApp,
    key: WorkspaceRepoKey,
) -> (String, Option<Uuid>, Option<String>) {
    sqlx::query_as(
        r#"
        SELECT status, merge_run_id, new_mirror_tip
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
    .expect("audit row exists")
}

/// Fetches the latest audit row for a specific outcome across explicit cycles.
async fn latest_run_with_status(
    app: &TestApp,
    key: WorkspaceRepoKey,
    status: &str,
) -> (String, Option<Uuid>, Option<String>) {
    sqlx::query_as(
        r#"
        SELECT status, merge_run_id, new_mirror_tip
        FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2
          AND status = $3
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(status)
    .fetch_one(&app.db)
    .await
    .expect("matching audit row exists")
}

async fn latest_run_status_and_trigger(app: &TestApp, key: WorkspaceRepoKey) -> (String, String) {
    sqlx::query_as(
        r#"
        SELECT status, trigger_kind
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
    .expect("audit row exists")
}

async fn integration_run_outcomes(app: &TestApp, key: WorkspaceRepoKey) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT status, trigger_kind FROM remote_sync_runs
         WHERE workspace_id=$1 AND repo_id=$2 ORDER BY started_at, id",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

async fn policy(app: &TestApp, key: WorkspaceRepoKey) -> remote_integrations::RemoteIntegration {
    let mut db = app.scoped_db_for_org(key.org_id).await;
    let policy = remote_integrations::load(db.conn(), key, false)
        .await
        .unwrap()
        .unwrap();
    db.rollback().await.unwrap();
    policy
}

async fn workspace_head(app: &TestApp, key: WorkspaceRepoKey) -> String {
    let policy = policy(app, key).await;
    let path = app.repo_path(key.repo_id).await;
    git(
        &path,
        &["rev-parse", &format!("refs/heads/{}", policy.local_branch)],
    )
}

async fn sync_watermark(app: &TestApp, key: WorkspaceRepoKey) -> String {
    let policy = policy(app, key).await;
    let path = app.repo_path(key.repo_id).await;
    GitRepo::open(&path)
        .unwrap()
        .read_sync_watermark(&policy.local_branch)
        .unwrap()
        .unwrap()
}

async fn paused_reason(app: &TestApp, key: WorkspaceRepoKey) -> Option<String> {
    sqlx::query_scalar(
        "SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// Disposable observations are UI hints; exact attempt inputs live in the audit.
async fn observed_tip(app: &TestApp, key: WorkspaceRepoKey) -> Option<String> {
    let policy = policy(app, key).await;
    let mut db = app.scoped_db_for_org(key.org_id).await;
    let observation = remote_integrations::observed_tip(db.conn(), &policy)
        .await
        .unwrap()
        .map(|observation| observation.oid);
    db.rollback().await.unwrap();
    observation
}

/// Records a hint the way the pinger or worker would, in its own transaction.
async fn observe_tip(app: &TestApp, key: WorkspaceRepoKey, oid: &str) -> bool {
    let policy = policy(app, key).await;
    let mut db = app.scoped_db_for_org(key.org_id).await;
    let changed = remote_integrations::observe_tip(db.conn(), &policy, oid)
        .await
        .unwrap();
    db.commit().await.unwrap();
    changed
}

async fn workspace_push_mode(app: &TestApp, workspace_id: Uuid) -> String {
    sqlx::query_scalar("SELECT push_mode FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .expect("workspace push_mode")
}

async fn audit_acting_users(app: &TestApp, key: WorkspaceRepoKey) -> Vec<Uuid> {
    sqlx::query_scalar(
        r#"
        SELECT acting_user_id
        FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2
        ORDER BY started_at ASC, id ASC
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_all(&app.db)
    .await
    .expect("acting_user_id audit rows")
}

// ---------------------------------------------------------------------------
// D10 — Equal tips → Noop
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_noop_when_tips_equal() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Workspace tip == baseline == remote tip (all C1).
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, merge_run_id, _advisory) = latest_run(&app, fx.key()).await;
    assert_eq!(status, "noop", "equal tips must classify as noop");
    assert!(merge_run_id.is_none(), "noop must not open a merge run");
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        fx.c1,
        "noop must not move the workspace head"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Provider ahead: deliver observations without advancing the workspace
// ---------------------------------------------------------------------------

/// A plain org member (not the workspace owner) with a token for `org`.
async fn create_member(app: &TestApp, org: &TestOrg) -> TestUser {
    let mut member = TestUser::create(app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .expect("insert member membership");
    member.jwt = helpers::test_jwt_minter()
        .mint(
            member.id,
            org.id,
            vec![
                Scope::WorkspacesRead,
                Scope::WorkspacesWrite,
                Scope::OrgRead,
            ],
            3600,
        )
        .expect("mint member token");
    member
}

/// The "has a new commit" notices in `workspace_id`'s inbox as `client`.
async fn new_commit_notices(
    app: &TestApp,
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
) -> Vec<Value> {
    let _ = app;
    let response = client
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_imports_when_remote_moved_clean() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // A reviewer holds a live review copy of the workspace: the import
    // below moves the branch their rounds are measured against, so they
    // are told once, in their copy's inbox.
    let reviewer = create_member(&app, &org).await;
    let reviewer_client = app.authenticated_client(&reviewer);
    let started = reviewer_client
        .post_json(
            &format!("/api/v1/workspaces/{}/review/start", workspace.id),
            &json!({}),
        )
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
        new_commit_notices(&app, &reviewer_client, review_copy)
            .await
            .is_empty(),
        "nothing before the branch moves"
    );

    // Remote advances to C2; workspace stays clean at C1.
    let c2 = fx.advance_remote("remote_only.txt", "remote work\n");
    assert_ne!(c2, fx.c1);

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        new_commit_notices(&app, &reviewer_client, review_copy)
            .await
            .len(),
        1,
        "fast-forwarding the reviewed branch notifies its reviewer once"
    );
    let (status, merge_run_id, _advisory) =
        latest_run_with_status(&app, fx.key(), "imported").await;
    assert_eq!(
        status, "imported",
        "remote refresh must preserve workspace files"
    );
    assert!(merge_run_id.is_none(), "import must not open a merge run");
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        c2,
        "fast-forward publishes the provider tip as the G5 branch head"
    );
    assert_eq!(
        sync_watermark(&app, fx.key()).await,
        c2,
        "fast-forward establishes convergence"
    );
    assert!(
        paused_reason(&app, fx.key()).await.is_none(),
        "a healthy import must leave sync un-paused"
    );
    // Provider status advances while the workspace remains at its own tip.
    assert_eq!(
        observed_tip(&app, fx.key()).await.as_deref(),
        Some(c2.as_str()),
        "provider observation must expose incoming changes"
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_fetch_fast_forwards_g5_preserves_vm_and_snapshot_then_native_pull_works() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id<>$2")
        .bind(workspace.id)
        .bind(fx.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE remote_integrations SET auto_pull_enabled=false WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .unwrap();
    let auth_header = format!("http.extraHeader=Authorization: Bearer {token}");
    let origin = format!("{}/git/{}", app.base_url, fx.repo_id);
    let clean = fx.tmp_root.join("clean-vm");
    let dirty = fx.tmp_root.join("dirty-vm");
    for checkout in [&clean, &dirty] {
        git(
            &fx.tmp_root,
            &[
                "-c",
                &auth_header,
                "clone",
                "--branch",
                &workspace.name,
                &origin,
                checkout.to_str().unwrap(),
            ],
        );
        git(
            checkout,
            &[
                "config",
                "http.extraHeader",
                &format!("Authorization: Bearer {token}"),
            ],
        );
        git(checkout, &["config", "user.name", "Fixture"]);
        git(checkout, &["config", "user.email", "fixture@test.local"]);
    }
    write_file(&dirty, "dirty.txt", "preserve unfinished edits\n");
    git(&dirty, &["add", "dirty.txt"]);
    git(&dirty, &["commit", "-m", "snapshot tree"]);
    let snapshot = git(&dirty, &["rev-parse", "HEAD"]);
    git(&dirty, &["reset", "--mixed", &fx.c1]);
    let flush_url = format!(
        "{}/api/internal/workspace-incarnations/{}/flushes",
        app.base_url, workspace.id
    );
    let started = app
        .client
        .post(format!("{flush_url}/start"))
        .bearer_auth(&token)
        .json(&json!({"reason":"shutdown", "idempotency_key":"before-provider-fast-forward"}))
        .send()
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::OK);
    let started: Value = started.json().await.unwrap();
    let flush_id = started["flush_id"].as_str().unwrap();
    let prefix = format!("refs/g5/flush/{}/{flush_id}", workspace.id);
    git(
        &dirty,
        &[
            "push",
            "origin",
            &format!("{snapshot}:{prefix}/snapshot"),
            &format!("{}:{prefix}/branch", fx.c1),
        ],
    );
    let receipt = json!({"repo_id":fx.repo_id, "branch_oid":fx.c1, "local_branch_oid":fx.c1,
        "local_head_oid":fx.c1, "head_ref":format!("refs/heads/{}",workspace.name),
        "sequencer":"none", "snapshot_oid":snapshot, "dirty":true, "dirty_paths":["dirty.txt"]});
    let complete = || {
        app.client
            .post(format!("{flush_url}/{flush_id}/complete"))
            .bearer_auth(&token)
            .json(&json!({"repos":[receipt]}))
    };
    let first = complete().send().await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first: Value = first.json().await.unwrap();
    let dirty_status = git(&dirty, &["status", "--porcelain"]);
    let remote = fx.advance_remote("remote.txt", "incoming commit\n");
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/sync", workspace.id),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert!(status.is_success(), "{body}");
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(workspace_head(&app, fx.key()).await, remote);
    assert_eq!(sync_watermark(&app, fx.key()).await, remote);
    assert_eq!(
        integration_run_outcomes(&app, fx.key()).await,
        vec![("imported".into(), "manual".into())],
        "one fetch must not schedule another cycle for the commits it just imported"
    );
    for checkout in [&clean, &dirty] {
        assert_eq!(git(checkout, &["rev-parse", "HEAD"]), fx.c1);
        assert!(!checkout.join("remote.txt").exists());
    }
    assert_eq!(git(&dirty, &["status", "--porcelain"]), dirty_status);
    assert_eq!(
        std::fs::read_to_string(dirty.join("dirty.txt")).unwrap(),
        "preserve unfinished edits\n"
    );
    let replay: Value = complete().send().await.unwrap().json().await.unwrap();
    assert_eq!(
        replay, first,
        "fast-forward preserves the exact recovery receipt"
    );
    assert_eq!(
        git(&fx.mirror, &["rev-parse", &format!("{prefix}/snapshot")]),
        snapshot
    );
    let selected: Uuid =
        sqlx::query_scalar("SELECT last_flush_id FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(selected.to_string(), flush_id);
    git(&clean, &["pull", "--ff-only"]);
    assert_eq!(git(&clean, &["rev-parse", "HEAD"]), remote);
    assert_eq!(
        std::fs::read_to_string(clean.join("remote.txt")).unwrap(),
        "incoming commit\n"
    );
    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fast_forward_revalidates_concurrent_g5_commit_and_recovers_after_failed_landing() {
    for scenario in ["head_race", "failed_commit"] {
        let app = spawn_worker_app().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
        let remote = fx.advance_remote("remote.txt", "remote work\n");
        fx.enqueue(&app, "manual").await;
        if scenario == "head_race" {
            let mut gate = app.db.begin().await.unwrap();
            g5_api::services::branch_commit::acquire_workspace_commit_fence(
                &mut gate,
                workspace.id,
            )
            .await
            .unwrap();
            let deps =
                g5_api::maintenance::branch_integration_worker::BranchIntegrationWorkerDeps {
                    landed_commit_notifier: std::sync::Arc::new(
                        g5_api::services::review_source_notifications::ReviewSourceNotifier::from_state(
                            &app.state,
                        ),
                    ),
                    system_pool: app.system_pool.clone(),
                    tenant_pool: app.state.tenant_pool.clone(),
                    authz: app.state.authz.clone(),
                    repo_storage: app.repo_storage.clone(),
                    repository_credentials: app.state.repository_credentials.clone(),
                    event_bus: app.state.event_bus.clone(),
                };
            let cancel = tokio_util::sync::CancellationToken::new();
            let worker = tokio::spawn(
                g5_api::maintenance::branch_integration_worker::run_branch_integration_worker(
                    deps,
                    cancel.clone(),
                ),
            );
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let prepared: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_advance_consumer_inputs WHERE consumer_kind='remote_tracking_convergence')")
                        .fetch_one(&app.db).await.unwrap();
                    if prepared { break; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("worker prepares fast-forward before acquiring publication fence");
            let concurrent = fx.advance_mirror("concurrent.txt", "new G5 commit\n");
            git(
                &fx.mirror,
                &[
                    "update-ref",
                    &format!("refs/heads/{}", workspace.name),
                    &concurrent,
                ],
            );
            gate.commit().await.unwrap();
            // Stop after the in-flight stale cycle releases its lease, before retry.
            cancel.cancel();
            worker.await.unwrap();
            assert_eq!(workspace_head(&app, fx.key()).await, concurrent);
            assert_eq!(sync_watermark(&app, fx.key()).await, fx.c1);
            tokio::time::sleep(Duration::from_secs(2)).await;
            drain_one_cycle(&app, fx.key()).await;
            assert_eq!(latest_run(&app, fx.key()).await.0, "diverged");
            assert_eq!(workspace_head(&app, fx.key()).await, concurrent);
        } else {
            sqlx::raw_sql("CREATE FUNCTION reject_import_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.status='imported' THEN RAISE EXCEPTION 'injected import commit failure'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER reject_import_commit AFTER INSERT ON remote_sync_runs DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_import_commit();")
                .execute(&app.db).await.unwrap();
            // Stop the worker after its first failed landing; the queue remains recoverable.
            let deps =
                g5_api::maintenance::branch_integration_worker::BranchIntegrationWorkerDeps {
                    landed_commit_notifier: std::sync::Arc::new(
                        g5_api::services::review_source_notifications::ReviewSourceNotifier::from_state(
                            &app.state,
                        ),
                    ),
                    system_pool: app.system_pool.clone(),
                    tenant_pool: app.state.tenant_pool.clone(),
                    authz: app.state.authz.clone(),
                    repo_storage: app.repo_storage.clone(),
                    repository_credentials: app.state.repository_credentials.clone(),
                    event_bus: app.state.event_bus.clone(),
                };
            let cancel = tokio_util::sync::CancellationToken::new();
            let worker = tokio::spawn(
                g5_api::maintenance::branch_integration_worker::run_branch_integration_worker(
                    deps,
                    cancel.clone(),
                ),
            );
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    if worker.is_finished() { break; }
                    // Consumer registration survives, but the failed transaction
                    // must leave neither the import receipt nor the Git cut.
                    let prepared: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_advance_consumer_inputs WHERE consumer_kind='remote_tracking_convergence')")
                        .fetch_one(&app.db).await.unwrap();
                    if prepared { break; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.unwrap();
            cancel.cancel();
            worker.await.unwrap();
            assert_eq!(workspace_head(&app, fx.key()).await, fx.c1);
            assert_eq!(sync_watermark(&app, fx.key()).await, fx.c1);
            sqlx::raw_sql("DROP TRIGGER reject_import_commit ON remote_sync_runs; DROP FUNCTION reject_import_commit(); UPDATE remote_ref_leases SET lease_expires_at=now()-interval '1 second'; UPDATE remote_integration_queue SET locked_until=now()-interval '1 second';")
                .execute(&app.db).await.unwrap();
            drain_one_cycle(&app, fx.key()).await;
            assert_eq!(workspace_head(&app, fx.key()).await, remote);
            assert_eq!(sync_watermark(&app, fx.key()).await, remote);
        }
        let imports: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_sync_runs WHERE workspace_id=$1 AND status='imported'",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(imports, if scenario == "head_race" { 0 } else { 1 });
        fx.cleanup();
        app.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// D11/D12 — Workspace ahead, commit trigger → Pushed (remote ref advances)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_pushes_when_workspace_ahead() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Local mirror advances to C2 (a descendant of C1); the upstream stays at
    // C1. Anchor the durable workspace tip at the local commit.
    let c2 = fx.advance_mirror("local_only.txt", "local work\n");
    assert_ne!(c2, fx.c1);
    fx.set_workspace_tip(&app, &c2).await;
    assert_eq!(fx.remote_tip(), fx.c1, "precondition: upstream still at C1");

    // An explicit user request permits publication to this local test remote.
    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("set auto push_mode");

    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, merge_run_id, _advisory) = latest_run(&app, fx.key()).await;
    assert_eq!(status, "pushed", "workspace-ahead on commit must push");
    assert!(merge_run_id.is_none(), "push must not open a merge run");
    assert_eq!(
        fx.remote_tip(),
        c2,
        "push must fast-forward the customer's remote ref to the local tip"
    );
    // C30: after the push the remote == our tip, so the synchronization
    // watermark AND the UI advisory must both advance to the pushed tip
    // (design §6.2 "on a push, advance baseline"; §6.1 watermark invariant
    // "workspace tip == mirror tip ⇒ baseline == workspace tip"). Before the
    // fix, baseline stayed at C1 and the advisory was refreshed to the
    // *pre-push* mirror tip (C1) — leaving advisory < head, which kept the
    // client's `isBehind` (advisory != head) lit as a phantom "behind".
    assert_eq!(
        sync_watermark(&app, fx.key()).await,
        c2,
        "push must advance the baseline watermark to the pushed tip"
    );
    assert_eq!(
        observed_tip(&app, fx.key()).await.as_deref(),
        Some(c2.as_str()),
        "push must advance the advisory to the pushed tip (== head) so the UI clears 'behind'"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// A physical ref that is ahead of the exact DB-recorded cut was never landed
// through the workspace commit path. Remote integration must ignore it rather
// than publish it as a committed workspace change.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_reads_current_git_head_without_a_database_head_update() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let c2 = fx.advance_mirror("local_only.txt", "local work\n");
    assert_eq!(workspace_head(&app, fx.key()).await, c2);
    assert_eq!(fx.remote_tip(), fx.c1);
    sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    let (status, merge_run_id, _) = latest_run(&app, fx.key()).await;
    assert_eq!(status, "pushed");
    assert!(merge_run_id.is_none());
    assert_eq!(fx.remote_tip(), c2);
    assert_eq!(workspace_head(&app, fx.key()).await, c2);
    assert_eq!(sync_watermark(&app, fx.key()).await, c2);
    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_converges_watermark_when_git_heads_already_match() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let c2 = fx.advance_mirror("already_pushed.txt", "already pushed local work\n");
    git(
        &fx.mirror,
        &["push", "origin", &format!("{}:main", fx.workspace_branch)],
    );
    assert_eq!(workspace_head(&app, fx.key()).await, c2);
    assert_eq!(sync_watermark(&app, fx.key()).await, fx.c1);
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    let (status, merge_run_id, _) = latest_run(&app, fx.key()).await;
    assert_eq!(status, "noop");
    assert!(merge_run_id.is_none());
    assert_eq!(workspace_head(&app, fx.key()).await, c2);
    assert_eq!(sync_watermark(&app, fx.key()).await, c2);
    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_fetches_from_git_url_when_mirror_origin_drifted() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    let authoritative_bare = fx.tmp_root.join("authoritative.git");
    let authoritative_seed = fx.tmp_root.join("authoritative-seed");
    std::fs::create_dir_all(&authoritative_bare).expect("create authoritative bare dir");
    git(
        &fx.tmp_root,
        &[
            "clone",
            fx.bare.to_str().unwrap(),
            authoritative_seed.to_str().unwrap(),
        ],
    );
    git(
        &authoritative_seed,
        &["config", "user.email", "g5@test.local"],
    );
    git(&authoritative_seed, &["config", "user.name", "G5 Test"]);
    write_file(
        &authoritative_seed,
        "authoritative.txt",
        "remote from authoritative URL\n",
    );
    git(&authoritative_seed, &["add", "."]);
    git(
        &authoritative_seed,
        &["commit", "-m", "authoritative remote moved"],
    );
    let c2 = git(&authoritative_seed, &["rev-parse", "HEAD"]);
    git(
        &authoritative_bare,
        &["init", "--bare", "--initial-branch=main"],
    );
    git(
        &authoritative_seed,
        &[
            "remote",
            "set-url",
            "origin",
            authoritative_bare.to_str().unwrap(),
        ],
    );
    git(&authoritative_seed, &["push", "origin", "main"]);
    git(
        &authoritative_bare,
        &["symbolic-ref", "HEAD", "refs/heads/main"],
    );

    assert_eq!(
        fx.mirror_origin_url(),
        fx.bare.to_str().unwrap(),
        "precondition: local mirror origin still points to the stale remote"
    );
    sqlx::query("UPDATE repos SET git_url = $2 WHERE id = $1")
        .bind(fx.repo_id)
        .bind(authoritative_bare.to_str().unwrap())
        .execute(&app.db)
        .await
        .expect("point repo metadata at authoritative remote");

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, merge_run_id, _) = latest_run_with_status(&app, fx.key(), "imported").await;
    assert_eq!(status, "imported", "worker must fetch from repos.git_url");
    assert!(
        merge_run_id.is_none(),
        "healthy import must not open a merge run"
    );
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        c2,
        "fast-forward advances the G5 branch"
    );
    assert_eq!(
        fx.mirror_origin_url(),
        fx.bare.to_str().unwrap(),
        "anonymous fetch must leave the configured mirror origin unchanged"
    );
    assert_eq!(
        git(
            &fx.mirror,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::provider_ref(workspace.id, "main")
            ]
        ),
        c2
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// C30 — After a push, a subsequent idle cycle must read the just-pushed tip
// back as Equal → noop, NOT a phantom divergence ("seeing its own commit as
// divergence"). Regression for the stale-advisory/un-advanced-baseline bug.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_push_then_idle_cycle_is_noop_not_phantom_divergence() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Local commit C2 on top of the upstream C1; publish it via auto push.
    let c2 = fx.advance_mirror("local_only.txt", "local work\n");
    assert_ne!(c2, fx.c1);
    fx.set_workspace_tip(&app, &c2).await;
    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("set auto push_mode");

    // Cycle 1: workspace-ahead → push; the customer remote fast-forwards to C2.
    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    let (status1, _, _) = latest_run(&app, fx.key()).await;
    assert_eq!(status1, "pushed", "precondition: cycle 1 pushes");
    assert_eq!(fx.remote_tip(), c2, "precondition: remote is now at C2");

    // Cycle 2: the user touched NOTHING (remote unchanged, no new local
    // commit). The just-pushed tip must classify Equal → noop — never a
    // phantom diverged/watermark_violation. Before C30 the un-advanced
    // baseline + stale advisory surfaced exactly this false divergence.
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status2, merge_run_id2, _) = latest_run(&app, fx.key()).await;
    assert_eq!(
        status2, "noop",
        "a second cycle after a push (remote unchanged) must be noop, not a phantom divergence"
    );
    assert!(
        merge_run_id2.is_none(),
        "no merge run may open after a clean push"
    );
    assert!(
        paused_reason(&app, fx.key()).await.is_none(),
        "sync must stay un-paused after push + idle cycle"
    );
    // Advisory equals the successfully published head.
    assert_eq!(workspace_head(&app, fx.key()).await, c2);
    assert_eq!(
        observed_tip(&app, fx.key()).await.as_deref(),
        Some(workspace_head(&app, fx.key()).await.as_str()),
        "advisory must equal head after a push so the UI shows healthy (not behind)"
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inbound_import_preserves_a_new_request_queued_during_landing() {
    for trigger in ["manual", "push"] {
        let app = spawn_worker_app().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
        let remote = fx.advance_remote("remote.txt", "incoming commit\n");
        fx.enqueue(&app, "manual").await;

        // Deterministically inject a newer request after admission, while the
        // landing holds the queue lock, before the worker's cleanup. A manual
        // request differs only in requested_at; push also changes the intent.
        sqlx::raw_sql(&format!(
            "CREATE FUNCTION request_during_import() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN
                 IF NEW.status = 'imported' THEN
                     UPDATE remote_integration_queue
                     SET requested_at = clock_timestamp(), trigger_kind = '{trigger}',
                         requested_by_user_id = CASE WHEN '{trigger}' = 'push' THEN (SELECT owner_user_id FROM workspaces WHERE id = NEW.workspace_id) END,
                         lifecycle_generation = CASE WHEN '{trigger}' = 'push' THEN (SELECT lifecycle_generation FROM workspaces WHERE id = NEW.workspace_id) END
                     WHERE workspace_id = NEW.workspace_id AND repo_id = NEW.repo_id;
                 END IF;
                 RETURN NEW;
             END $$;
             CREATE TRIGGER request_during_import AFTER INSERT ON remote_sync_runs
             FOR EACH ROW EXECUTE FUNCTION request_during_import();"
        ))
        .execute(&app.db)
        .await
        .unwrap();

        drain_one_cycle(&app, fx.key()).await;
        assert_eq!(workspace_head(&app, fx.key()).await, remote);
        assert_eq!(sync_watermark(&app, fx.key()).await, remote);
        assert_eq!(
            integration_run_outcomes(&app, fx.key()).await,
            vec![
                ("imported".into(), "manual".into()),
                ("noop".into(), trigger.into()),
            ],
            "cleanup must preserve the newer {trigger} request"
        );

        fx.cleanup();
        app.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// C31 — After an inbound import, the advisory must converge to the imported
// tip and a subsequent idle cycle must stay noop with advisory == head —
// NOT a phantom "remote moved — your branch is behind" after importing that
// very commit. Import-path analogue of the C30 push regression above.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_import_then_retry_converges_without_duplicate_import() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Simulate the user's post-push converged state: the advisory is pinned at
    // the current tip C1 (== head), as `record_push_converged` or a prior
    // advisory observation would have left it.
    observe_tip(&app, fx.key(), &fx.c1).await;

    // Remote advances to C2 (the "subsequent commit on the remote side");
    // workspace stays clean.
    let c2 = fx.advance_remote("remote_only.txt", "remote work\n");
    assert_ne!(c2, fx.c1);

    // Cycle 1 accepts the provider fast-forward.
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        integration_run_outcomes(&app, fx.key()).await,
        vec![("imported".into(), "background_inbound".into())],
        "the imported provider commit must not create its own follow-up cycle"
    );
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        c2,
        "fast-forward advances the G5 branch"
    );
    assert_eq!(
        observed_tip(&app, fx.key()).await.as_deref(),
        Some(c2.as_str()),
        "provider observation must advance from C1"
    );

    // Cycle 2 observes equality and does not duplicate the import.
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    let (status2, merge_run_id2, _) = latest_run(&app, fx.key()).await;
    assert_eq!(
        status2, "noop",
        "accepted provider tip makes the next cycle a no-op"
    );
    assert!(
        merge_run_id2.is_none(),
        "refresh alone opens no resolution task"
    );
    assert!(
        paused_reason(&app, fx.key()).await.is_none(),
        "refreshing incoming changes does not pause synchronization"
    );
    assert_eq!(workspace_head(&app, fx.key()).await, c2);
    assert_eq!(
        observed_tip(&app, fx.key()).await.as_deref(),
        Some(c2.as_str()),
        "advisory stays at the accepted provider tip"
    );
    assert_eq!(
        integration_run_outcomes(&app, fx.key()).await,
        vec![
            ("imported".into(), "background_inbound".into()),
            ("noop".into(), "background_inbound".into()),
        ],
        "a separately requested cycle still runs"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// C31 (fix B) — On a converged clean branch the steady-state noop cycle
// subsumes the advisory under head (design §6.4 / arch D3 case (c)). A stale
// advisory left over from a prior observation MUST be reconciled to the equal
// tip, so the client `isBehind` (advisory != head) cannot stay falsely lit.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_noop_reconciles_stale_advisory_to_head() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Tips already equal at C1 (workspace == mirror == baseline). Freeze the
    // advisory at a synthetic stale SHA != head to simulate a value left over
    // from before head caught up. The classifier never reads the advisory
    // (arch D10), so this only exercises the worker's reconcile path.
    let stale = "b".repeat(40);
    observe_tip(&app, fx.key(), &stale).await;

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, merge_run_id, _) = latest_run(&app, fx.key()).await;
    assert_eq!(status, "noop", "equal tips must classify as noop");
    assert!(merge_run_id.is_none(), "noop must not open a merge run");
    let head = workspace_head(&app, fx.key()).await;
    assert_eq!(head, fx.c1, "noop must not move head");
    assert_eq!(
        observed_tip(&app, fx.key()).await.as_deref(),
        Some(workspace_head(&app, fx.key()).await.as_str()),
        "a clean-branch noop must reconcile a stale advisory to head (no durable phantom 'behind')"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D86 — Default manual mode: commit-coupled workspace-ahead does NOT publish
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_commit_in_explicit_manual_mode_does_not_publish() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    assert_eq!(
        workspace_push_mode(&app, workspace.id).await,
        "auto",
        "new workspace bindings default to automatic publication"
    );

    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    let c2 = fx.advance_mirror("manual_commit.txt", "manual mode local work\n");
    assert_ne!(c2, fx.c1);
    fx.set_workspace_tip(&app, &c2).await;
    assert_eq!(fx.remote_tip(), fx.c1, "precondition: upstream still at C1");

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, trigger_kind) = latest_run_status_and_trigger(&app, fx.key()).await;
    assert_eq!(trigger_kind, "commit");
    assert_eq!(
        status, "noop",
        "commit-coupled workspace-ahead must not publish in default manual mode"
    );
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "manual mode must leave the customer remote unchanged until PushToRemote"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D85 — PushToRemote route: explicit 'push' publishes in manual mode
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn push_to_remote_route_publishes_manual_workspace_ahead_branch() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let mut fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    fx.attach_remote_tracked_docs_repo(&app, &org, &workspace)
        .await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2")
        .bind(workspace.id)
        .bind(workspace.repo_id)
        .execute(&app.db)
        .await
        .expect("remove synthetic default workspace mapping");

    sqlx::query("UPDATE workspaces SET push_mode='manual' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        workspace_push_mode(&app, workspace.id).await,
        "manual",
        "precondition: PushToRemote is the manual-mode publication path"
    );

    let c2 = fx.advance_mirror("manual_push.txt", "explicit push work\n");
    fx.set_workspace_tip(&app, &c2).await;
    assert_eq!(fx.remote_tip(), fx.c1, "precondition: upstream still at C1");

    let response = app
        .authenticated_client(&user)
        .post_json::<Value>(
            &format!("/api/v1/workspaces/{}/push_to_remote", workspace.id),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("push response JSON");
    assert_eq!(status, StatusCode::OK, "push response body: {body}");
    assert_eq!(
        body.pointer("/data/enqueued").and_then(Value::as_i64),
        Some(2),
        "PushToRemote must enqueue the coordinated docs+code remote-tracked set"
    );

    let queued_trigger: String = sqlx::query_scalar(
        "SELECT trigger_kind FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("queued push row");
    assert_eq!(queued_trigger, "push");

    drain_one_cycle(&app, fx.key()).await;

    let (status, trigger_kind) = latest_run_status_and_trigger(&app, fx.key()).await;
    assert_eq!(trigger_kind, "push");
    assert_eq!(status, "pushed", "explicit push trigger must publish");
    assert_eq!(
        fx.remote_tip(),
        c2,
        "PushToRemote must fast-forward the customer remote ref"
    );
    assert_eq!(
        sync_watermark(&app, fx.key()).await,
        c2,
        "PushToRemote success must advance the baseline"
    );
    assert_eq!(
        observed_tip(&app, fx.key()).await.as_deref(),
        Some(c2.as_str()),
        "PushToRemote success must clear the advisory behind-state"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D84 — Automatic background publication requires a GitHub App
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auto_mode_background_inbound_requires_github_app() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("set auto push_mode");

    let c2 = fx.advance_mirror("auto_background.txt", "auto background work\n");
    fx.set_workspace_tip(&app, &c2).await;
    assert_eq!(fx.remote_tip(), fx.c1, "precondition: upstream still at C1");

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, trigger_kind) = latest_run_status_and_trigger(&app, fx.key()).await;
    assert_eq!(trigger_kind, "background_inbound");
    assert_eq!(
        status, "push_failed",
        "auto mode cannot authorize a non-App push"
    );
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "automatic publication must leave the remote unchanged"
    );
    assert_eq!(
        sync_watermark(&app, fx.key()).await,
        fx.c1,
        "a refused automatic push cannot advance the baseline"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D46 — acting_user_id follows the current workspace owner per cycle
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_records_acting_user_id_and_preserves_prior_owner_audit_rows() {
    let app = spawn_worker_app().await;
    let mut owner_one = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner_one).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner_one.id).await;

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;
    let first_owner_rows = audit_acting_users(&app, fx.key()).await;
    assert_eq!(
        first_owner_rows,
        vec![owner_one.id],
        "cycle N must stamp the workspace owner at pickup time"
    );

    let owner_two = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(org.id)
        .bind(owner_two.id)
        .execute(&app.db)
        .await
        .expect("add second owner membership");
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(owner_two.id)
        .execute(&app.db)
        .await
        .expect("transfer workspace owner");

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let acting_users = audit_acting_users(&app, fx.key()).await;
    assert_eq!(
        acting_users,
        vec![owner_one.id, owner_two.id],
        "cycle N+1 must stamp the new owner while preserving cycle N's forensic row"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D47 — historical creator identity cannot supply background credentials
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_ignores_stale_creator_and_uses_current_owner_identity() {
    let app = spawn_worker_app().await;
    let mut original_owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut original_owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, original_owner.id).await;

    let c2 = fx.advance_mirror("new_owner.txt", "publish as the current owner\n");
    fx.set_workspace_tip(&app, &c2).await;
    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("set auto push_mode");

    let new_owner = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(org.id)
        .bind(new_owner.id)
        .execute(&app.db)
        .await
        .expect("add new owner membership");
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(new_owner.id)
        .execute(&app.db)
        .await
        .expect("transfer workspace owner");

    sqlx::query("DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(original_owner.id)
        .execute(&app.db)
        .await
        .expect("remove historical creator membership");

    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, trigger_kind) = latest_run_status_and_trigger(&app, fx.key()).await;
    assert_eq!(trigger_kind, "push");
    assert_eq!(
        status, "pushed",
        "publication must act as the current owner"
    );
    assert_eq!(
        fx.remote_tip(),
        c2,
        "publication must use current-owner authority despite stale creator audit"
    );
    assert_eq!(
        audit_acting_users(&app, fx.key()).await,
        vec![new_owner.id],
        "the historical creator must never be stamped as the acting owner"
    );

    fx.cleanup();
    app.cleanup().await;
}

/// A real worker cycle must resolve the current owner's stored credential and
/// stop after that connection is revoked, before attempting transport.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_resolves_current_owner_and_rechecks_connection_before_transport() {
    for disconnect_current_owner in [false, true] {
        let mut config = helpers::test_config();
        config.database.max_connections = 8;
        config.tenant_database.max_connections = 8;
        config.authz_database.max_connections = 8;
        config.auth.scm_gitlab = None;
        config.auth.scm_gitlab_userinfo_url = Some("https://127.0.0.1:1/api/v4/user".into());
        let app = TestApp::spawn_with_config(config).await;
        let mut creator = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut creator).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let fx = TrackingFixture::setup(&app, &org, &workspace, creator.id).await;
        let owner = TestUser::create(&app).await;
        sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'admin')")
            .bind(org.id)
            .bind(owner.id)
            .execute(&app.db)
            .await
            .unwrap();
        fx.seed_gitlab_connection_for_owner(&app, owner.id).await;
        sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
            .bind(workspace.id)
            .bind(owner.id)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("UPDATE repos SET provider = 'gitlab', git_url = 'https://127.0.0.1:1/o/r.git' WHERE id = $1")
            .bind(fx.repo_id).execute(&app.db).await.unwrap();
        sqlx::query("DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2")
            .bind(org.id)
            .bind(creator.id)
            .execute(&app.db)
            .await
            .unwrap();
        if disconnect_current_owner {
            sqlx::query(
                "DELETE FROM user_scm_connections WHERE user_id = $1 AND provider = 'gitlab'",
            )
            .bind(owner.id)
            .execute(&app.db)
            .await
            .unwrap();
        }
        fx.enqueue(&app, "commit").await;
        drain_one_cycle(&app, fx.key()).await;
        let (status, detail): (String, Option<String>) = sqlx::query_as(
            "SELECT status, error_detail FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 ORDER BY started_at DESC LIMIT 1"
        ).bind(fx.workspace_id).bind(fx.repo_id).fetch_one(&app.db).await.unwrap();
        if disconnect_current_owner {
            assert_eq!(status, "auth_required");
            assert!(
                detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("no_scm_connection")),
                "{detail:?}"
            );
        } else {
            assert_eq!(
                status, "fetch_failed",
                "current owner's credential must reach transport: {detail:?}"
            );
        }
        assert_eq!(audit_acting_users(&app, fx.key()).await, vec![owner.id]);
        fx.cleanup();
        app.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// D52 — cross-table org binding mismatch is rejected before worker transport
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn database_rejects_repo_workspace_org_binding_mismatch_before_worker_transport() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;

    let c2 = fx.advance_mirror("org_mismatch.txt", "must not publish across org binding\n");
    fx.set_workspace_tip(&app, &c2).await;

    let mut other_owner = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut other_owner).await;
    // P4.1's workspace_repos projection makes a repo/workspace org mismatch
    // structurally impossible. Reject the corrupting write at the database
    // boundary, before a queue row or Git transport can exist. The retained
    // worker check remains defense-in-depth for databases corrupted outside
    // normal constraints or during a mixed-version recovery.
    let mismatch = sqlx::query("UPDATE repos SET org_id = $2 WHERE id = $1")
        .bind(fx.repo_id)
        .bind(other_org.id)
        .execute(&app.db)
        .await;
    let error = mismatch.expect_err("cross-org repo rebinding must be rejected");
    assert_eq!(
        error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::constraint),
        Some("workspace_repos_repo_fk"),
        "the same-org workspace mapping must own the rejection"
    );
    let queued_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_integration_queue \
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count queued work after rejected mismatch");
    assert_eq!(
        queued_count, 0,
        "a rejected org mismatch must not enqueue worker transport"
    );
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "a rejected org mismatch must not publish to the remote"
    );
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        c2,
        "a rejected org mismatch must not rewrite workspace head"
    );

    fx.cleanup();
    app.cleanup().await;
}

/// The P4.1 FK owns normal rejection, but the worker check remains a
/// defense-in-depth boundary for externally corrupted or mixed-version data.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_fails_closed_on_corrupted_repo_workspace_org_binding() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;

    let c2 = fx.advance_mirror("org_mismatch.txt", "must not publish across org binding\n");
    fx.set_workspace_tip(&app, &c2).await;

    let mut other_owner = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut other_owner).await;
    sqlx::query("ALTER TABLE workspace_repos DROP CONSTRAINT workspace_repos_repo_fk")
        .execute(&app.db)
        .await
        .expect("simulate an externally corrupted database without the P4.1 guard");
    sqlx::query("UPDATE repos SET org_id = $2 WHERE id = $1")
        .bind(fx.repo_id)
        .bind(other_org.id)
        .execute(&app.db)
        .await
        .expect("corrupt repo org binding after bypassing the structural guard");

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "an invalid tenant membership cannot create an attempt"
    );
    let pair_lease_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_ref_leases WHERE repo_id=$1 AND remote_branch='main'",
    )
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(pair_lease_count, 0);
    assert_eq!(fx.remote_tip(), fx.c1);
    assert_eq!(
        git(
            &fx.mirror,
            &["rev-parse", &format!("refs/heads/{}", fx.workspace_branch)]
        ),
        c2
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D54 — worker re-derives org from workspace branch, not queue metadata
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queue_rejects_cross_org_rebinding_before_worker_transport() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;

    let mut other_owner = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut other_owner).await;

    fx.enqueue(&app, "background_inbound").await;
    sqlx::query(
        "UPDATE remote_integration_queue SET org_id = $3 WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .bind(other_org.id)
    .execute(&app.db)
    .await
    .expect_err("queue membership must reject cross-org rebinding");

    drain_one_cycle(&app, fx.key()).await;

    let (audit_org_id, status, trigger_kind): (Uuid, String, String) = sqlx::query_as(
        r#"
        SELECT org_id, status, trigger_kind
        FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("audit row exists");
    assert_eq!(
        audit_org_id, org.id,
        "worker audit rows must use workspace-derived org_id, not stale queue org_id"
    );
    assert_eq!(trigger_kind, "background_inbound");
    assert_eq!(
        status, "noop",
        "stale queue org metadata must not prevent same-org branch processing"
    );
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        fx.c1,
        "noop cycle must leave the workspace head unchanged"
    );
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "noop cycle must leave the remote unchanged"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D15 — Mirror ahead but workspace dirty → DirtySkipped (no head move)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_pauses_and_opens_merge_run_when_diverged() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Upstream advances to C2 (child of C1); the local mirror advances to a
    // SIBLING C2' (also a child of C1). Anchor the workspace tip at C2'.
    let remote_tip = fx.advance_remote("remote_only.txt", "remote work\n");
    let local_tip = fx.advance_mirror("local_only.txt", "local work\n");
    assert_ne!(remote_tip, local_tip, "tips must diverge");
    fx.set_workspace_tip(&app, &local_tip).await;

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    let (status, merge_run_id, _advisory) = latest_run(&app, fx.key()).await;
    assert_eq!(status, "diverged", "sibling tips must classify as diverged");
    let merge_run_id = merge_run_id.expect("diverged must open a merge run and stamp its id");

    assert_eq!(
        paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged"),
        "divergence must pause the branch with reason 'diverged'"
    );

    // The handoff must materialize a remote_tracking merge run for the operator
    // to resolve via the existing workspace-merge workflow (arch D55/D60).
    let (run_kind, run_wbr): (String, Uuid) =
        sqlx::query_as("SELECT source_kind, repo_id FROM workspace_merge_runs WHERE id = $1")
            .bind(merge_run_id)
            .fetch_one(&app.db)
            .await
            .expect("merge run row");
    assert_eq!(
        run_kind, "remote_tracking",
        "merge run must be remote_tracking sourced"
    );
    assert_eq!(
        run_wbr, fx.repo_id,
        "merge run must target the diverged workspace repository"
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conflict_resolution_requires_running_workspace_and_dispatches_one_agent_on_retry() {
    assert_conflict_resolution_flow(false, false).await;
}

// g5.workspace.remote.divergence-auto-complete through the production Git
// receiver: the agent's leased rebase push completes the run with no click.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agent_resolution_pushed_through_git_completes_automatically() {
    assert_conflict_resolution_flow(false, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_only_divergence_can_start_one_resolution_agent_without_an_existing_run() {
    assert_conflict_resolution_flow(true, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_only_resolution_remains_manually_completable_after_agent_start_fails() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    fx.advance_mirror("local.txt", "local work\n");
    let remote = fx.advance_remote("remote.txt", "remote work\n");
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;
    assert!(latest_run(&app, fx.key()).await.1.is_none());
    let client = app.authenticated_client(&owner);
    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/merge_runs",
        workspace.id, fx.repo_id
    );
    let stopped = client
        .post_json(&format!("{route}/start"), &json!({}))
        .await;
    assert_eq!(stopped.status(), StatusCode::CONFLICT);
    let run_id: Uuid = sqlx::query_scalar("SELECT id FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2 AND status='waiting_for_human'")
        .bind(workspace.id).bind(fx.repo_id).fetch_one(&app.db).await.unwrap();

    // The operator can still rebase externally and mark resolved after an
    // unsuccessful agent start; opening the record must preserve its inputs.
    git(&fx.mirror, &["rebase", "--no-rebase-merges", &remote]);
    let resolved = workspace_head(&app, fx.key()).await;
    let completed = client
        .post_json(&format!("{route}/{run_id}/complete"), &json!({}))
        .await;
    let status = completed.status();
    let body: Value = completed.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["completed"], true);
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), resolved);
    assert!(paused_reason(&app, fx.key()).await.is_none());
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 0);

    fx.cleanup();
    app.cleanup().await;
}

/// A divergence run opened by an owner whose agent could not start, so the
/// owner resolves outside G5. Returns the provider tip the run captured.
async fn open_externally_resolvable_divergence(
    app: &TestApp,
    owner: &TestUser,
    workspace: &TestWorkspace,
    fx: &TrackingFixture,
    push_mode: &str,
) -> (Uuid, String) {
    sqlx::query("UPDATE workspaces SET push_mode=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(push_mode)
        .execute(&app.db)
        .await
        .unwrap();
    fx.advance_mirror("local.txt", "local work\n");
    let remote = fx.advance_remote("remote.txt", "remote work\n");
    fx.enqueue(app, "commit").await;
    drain_one_cycle(app, fx.key()).await;
    assert_eq!(
        paused_reason(app, fx.key()).await.as_deref(),
        Some("diverged")
    );
    let stopped = app
        .authenticated_client(owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/merge_runs/start",
                workspace.id, fx.repo_id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(stopped.status(), StatusCode::CONFLICT);
    let run_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2 AND status='waiting_for_human'",
    )
    .bind(workspace.id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    (run_id, remote)
}

async fn merge_run_completion(app: &TestApp, run_id: Uuid) -> (String, Option<Uuid>) {
    sqlx::query_as("SELECT status, confirmed_by FROM workspace_merge_runs WHERE id=$1")
        .bind(run_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

// g5.workspace.remote.divergence-auto-complete / -auto-publish
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn landed_resolution_completes_divergence_automatically_and_follows_auto_push_policy() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let (run_id, remote) =
        open_externally_resolvable_divergence(&app, &owner, &workspace, &fx, "auto").await;

    git(&fx.mirror, &["rebase", "--no-rebase-merges", &remote]);
    let resolved = workspace_head(&app, fx.key()).await;
    // The accepted update's landing wake; nobody clicks Mark resolved.
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        merge_run_completion(&app, run_id).await,
        ("completed".to_string(), None),
        "automatic completion names no confirming human"
    );
    assert!(paused_reason(&app, fx.key()).await.is_none());
    let resolved_audit: (String, Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT trigger_kind, merge_run_id, new_workspace_tip FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 AND status='resolved'",
    )
    .bind(workspace.id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("automatic completion records a resolved audit row");
    assert_eq!(
        resolved_audit,
        ("commit".to_string(), Some(run_id), Some(resolved.clone()))
    );
    // The same wake then runs an ordinary cycle under the member's auto-push
    // policy. Automatic completion requested no publication of its own, so the
    // implicit push needs the GitHub App (absent in this fixture) and never
    // falls back to the owner's personal connection.
    let outcomes: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT status, error_detail, trigger_kind FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 ORDER BY started_at",
    )
    .bind(workspace.id)
    .bind(fx.repo_id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    let statuses: Vec<(&str, &str)> = outcomes
        .iter()
        .map(|(status, _, trigger)| (status.as_str(), trigger.as_str()))
        .collect();
    assert_eq!(
        statuses,
        [
            ("diverged", "commit"),
            ("resolved", "commit"),
            ("push_failed", "commit")
        ],
        "{outcomes:?}"
    );
    assert!(outcomes[2]
        .1
        .as_deref()
        .unwrap_or_default()
        .contains("Automatic push requires a GitHub App"));
    assert_eq!(fx.remote_tip(), remote);

    // The owner's explicit push publishes the resolved tip as a fast-forward.
    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), resolved);

    // A later owner confirmation is an idempotent no-op, not an error.
    let late = app
        .authenticated_client(&owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/merge_runs/{run_id}/complete",
                workspace.id, fx.repo_id
            ),
            &json!({}),
        )
        .await;
    let status = late.status();
    let body: Value = late.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["completed"], false);

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn landed_resolution_in_manual_push_mode_completes_without_publishing() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let (run_id, remote) =
        open_externally_resolvable_divergence(&app, &owner, &workspace, &fx, "manual").await;

    git(&fx.mirror, &["rebase", "--no-rebase-merges", &remote]);
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        merge_run_completion(&app, run_id).await,
        ("completed".to_string(), None)
    );
    assert!(paused_reason(&app, fx.key()).await.is_none());
    assert_eq!(
        fx.remote_tip(),
        remote,
        "manual push mode keeps the resolved tip in G5 until the owner pushes"
    );
    // The workspace already contains the provider tip, so the watermark catches
    // up to it and the client reads "ahead", not "behind" (advisory != baseline).
    assert_eq!(sync_watermark(&app, fx.key()).await, remote);

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unproven_update_leaves_the_divergence_run_open() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let (run_id, remote) =
        open_externally_resolvable_divergence(&app, &owner, &workspace, &fx, "auto").await;

    // The head advances but still does not descend from the captured provider tip.
    fx.advance_mirror("more_local.txt", "more local work\n");
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        merge_run_completion(&app, run_id).await.0,
        "waiting_for_human"
    );
    assert_eq!(
        paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged")
    );
    assert_eq!(fx.remote_tip(), remote);

    fx.cleanup();
    app.cleanup().await;
}

// An owner confirmation prepared before the worker completes the run, and
// applied after it, replays quietly and still queues the owner's publication.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_confirmation_racing_automatic_completion_replays_and_requests_publication() {
    use g5_api::services::remote_tracking_merge::{
        apply_external_remote_tracking_merge_completion,
        prepare_external_remote_tracking_merge_completion, CompletionRequester,
        PreparedExternalRemoteTrackingCompletion,
    };
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let (run_id, remote) =
        open_externally_resolvable_divergence(&app, &owner, &workspace, &fx, "manual").await;
    git(&fx.mirror, &["rebase", "--no-rebase-merges", &remote]);
    let resolved = workspace_head(&app, fx.key()).await;

    // The owner's click prepares against the open run...
    let prepared = {
        let mut db = app.scoped_db_for_org(org.id).await;
        let repos = app.state.repo_storage.scoped(&db);
        let prepared = prepare_external_remote_tracking_merge_completion(
            db.conn(),
            run_id,
            &repos,
            CompletionRequester::Owner(owner.id),
        )
        .await
        .unwrap();
        db.rollback().await.unwrap();
        prepared
    };
    assert!(matches!(
        prepared,
        PreparedExternalRemoteTrackingCompletion::Ready { .. }
    ));
    // ...the worker completes it first...
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        merge_run_completion(&app, run_id).await,
        ("completed".to_string(), None)
    );
    assert_eq!(fx.remote_tip(), remote);

    // ...and the owner's apply replays without error and keeps its meaning as
    // an explicit publication request.
    let outcome = apply_external_remote_tracking_merge_completion(
        app.scoped_db_for_org(org.id).await,
        prepared,
        &app.state.event_bus,
        &app.state.repo_storage,
    )
    .await
    .expect("a raced owner confirmation is not an error");
    assert!(!outcome.completed);
    let (trigger, requested_by): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT trigger_kind, requested_by_user_id FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(workspace.id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!((trigger.as_str(), requested_by), ("push", Some(owner.id)));
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), resolved);
    // The completion keeps the automatic attribution.
    assert_eq!(merge_run_completion(&app, run_id).await.1, None);

    fx.cleanup();
    app.cleanup().await;
}

// A retryable automatic-completion failure backs off on its own counter
// (15 s doubling, independent of queue pickups) and, after the last attempt,
// is surfaced on the run instead of holding the wake.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retryable_automatic_completion_failure_backs_off_then_surfaces_on_the_run() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let (run_id, remote) =
        open_externally_resolvable_divergence(&app, &owner, &workspace, &fx, "manual").await;
    git(&fx.mirror, &["rebase", "--no-rebase-merges", &remote]);

    // Fault: every `resolved` audit insert fails as a serialization victim.
    for statement in [
        "CREATE FUNCTION fail_resolved_audit() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN RAISE EXCEPTION 'injected' USING ERRCODE = '40001'; END $$",
        "CREATE TRIGGER fail_resolved_audit BEFORE INSERT ON remote_sync_runs
         FOR EACH ROW WHEN (NEW.status = 'resolved') EXECUTE FUNCTION fail_resolved_audit()",
    ] {
        sqlx::query(statement).execute(&app.db).await.unwrap();
    }

    // Record each retry's attempt count and backoff, then expire the backoff
    // so the drain does not wait it out.
    let observed = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(i32, f64)>::new()));
    let stop = tokio_util::sync::CancellationToken::new();
    let watcher = {
        let (db, observed, stop) = (app.db.clone(), observed.clone(), stop.clone());
        let key = fx.key();
        tokio::spawn(async move {
            while !stop.is_cancelled() {
                let row: Option<(i32, f64)> = sqlx::query_as(
                    "SELECT auto_completion_attempts,
                            EXTRACT(EPOCH FROM locked_until - now())::double precision
                     FROM remote_integration_queue
                     WHERE workspace_id=$1 AND repo_id=$2 AND locked_by IS NULL
                       AND auto_completion_attempts > 0 AND locked_until > now() + interval '5 seconds'",
                )
                .bind(key.workspace_id)
                .bind(key.repo_id)
                .fetch_optional(&db)
                .await
                .unwrap();
                if let Some(row) = row {
                    observed.lock().unwrap().push(row);
                    sqlx::query("UPDATE remote_integration_queue SET locked_until = now() WHERE workspace_id=$1 AND repo_id=$2")
                        .bind(key.workspace_id)
                        .bind(key.repo_id)
                        .execute(&db)
                        .await
                        .unwrap();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;
    stop.cancel();
    watcher.await.unwrap();

    let observed = observed.lock().unwrap().clone();
    let attempts: Vec<i32> = observed.iter().map(|(attempts, _)| *attempts).collect();
    assert_eq!(attempts, [1, 2, 3, 4], "{observed:?}");
    for ((attempt, remaining), expected) in observed.iter().zip([15.0, 30.0, 60.0, 120.0]) {
        assert!(
            *remaining <= expected && *remaining > expected - 5.0,
            "attempt {attempt}: backoff {remaining}s, expected about {expected}s"
        );
    }
    let blocking: Option<String> =
        sqlx::query_scalar("SELECT blocking_reason FROM workspace_merge_runs WHERE id=$1")
            .bind(run_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        blocking
            .as_deref()
            .is_some_and(|reason| reason.starts_with("Automatic completion failed:")),
        "{blocking:?}"
    );
    assert_eq!(
        merge_run_completion(&app, run_id).await.0,
        "waiting_for_human"
    );
    assert_eq!(
        paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged")
    );

    fx.cleanup();
    app.cleanup().await;
}

// A terminal automatic-completion failure must not hold the wake: it is
// surfaced on the run, the member stays paused, and "Mark resolved" recovers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminal_automatic_completion_failure_surfaces_on_the_run_without_holding_the_wake() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let (run_id, remote) =
        open_externally_resolvable_divergence(&app, &owner, &workspace, &fx, "manual").await;
    git(&fx.mirror, &["rebase", "--no-rebase-merges", &remote]);

    // Fault: the `resolved` audit row is refused, a non-retryable database error.
    let restrict = |statuses: &str| {
        format!(
            "ALTER TABLE remote_sync_runs DROP CONSTRAINT remote_sync_runs_status_check, \
             ADD CONSTRAINT remote_sync_runs_status_check CHECK (status IN ({statuses})) NOT VALID"
        )
    };
    let base_statuses = "'noop','fetched','imported','pushed','diverged','dirty_skipped',\
                         'advisory_updated','auth_required','push_failed','fetch_failed'";
    sqlx::query(&restrict(base_statuses))
        .execute(&app.db)
        .await
        .unwrap();

    fx.enqueue(&app, "commit").await;
    // Drains without waiting out a retry window.
    drain_one_cycle(&app, fx.key()).await;

    let (status, blocking): (String, Option<String>) =
        sqlx::query_as("SELECT status, blocking_reason FROM workspace_merge_runs WHERE id=$1")
            .bind(run_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "waiting_for_human");
    let blocking = blocking.expect("the failure is surfaced on the run");
    assert!(
        blocking.starts_with("Automatic completion failed:"),
        "{blocking}"
    );
    assert!(blocking.ends_with("Mark resolved to retry."), "{blocking}");
    assert_eq!(
        paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged")
    );
    assert_eq!(fx.remote_tip(), remote);

    // Clear the fault; the owner's confirmation completes and clears the reason.
    sqlx::query(&restrict(&format!("{base_statuses},'resolved'")))
        .execute(&app.db)
        .await
        .unwrap();
    let completed = app
        .authenticated_client(&owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/merge_runs/{run_id}/complete",
                workspace.id, fx.repo_id
            ),
            &json!({}),
        )
        .await;
    let code = completed.status();
    let body: Value = completed.json().await.unwrap();
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["completed"], true);
    let (status, blocking): (String, Option<String>) =
        sqlx::query_as("SELECT status, blocking_reason FROM workspace_merge_runs WHERE id=$1")
            .bind(run_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!((status.as_str(), blocking), ("completed", None));
    assert!(paused_reason(&app, fx.key()).await.is_none());

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_rewrite_checkpoint_never_completes_automatically() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let (run_id, remote) =
        open_externally_resolvable_divergence(&app, &owner, &workspace, &fx, "auto").await;
    // Same proven head, but the open run is an approval checkpoint.
    sqlx::query("UPDATE workspace_merge_runs SET phase='reset_below_baseline' WHERE id=$1")
        .bind(run_id)
        .execute(&app.db)
        .await
        .unwrap();

    git(&fx.mirror, &["rebase", "--no-rebase-merges", &remote]);
    // The completion service itself refuses an automatic requester, not just
    // the worker's run selection.
    {
        use g5_api::services::remote_tracking_merge::{
            prepare_external_remote_tracking_merge_completion, CompletionRequester,
        };
        let mut db = app.scoped_db_for_org(org.id).await;
        let repos = app.state.repo_storage.scoped(&db);
        let refused = prepare_external_remote_tracking_merge_completion(
            db.conn(),
            run_id,
            &repos,
            CompletionRequester::Automatic {
                trigger_kind: "commit",
            },
        )
        .await;
        assert!(
            matches!(
                refused.as_ref().map_err(|error| &error.inner),
                Err(g5_contracts::error::G5Error::Conflict { .. })
            ),
            "{refused:?}"
        );
    }
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        merge_run_completion(&app, run_id).await.0,
        "waiting_for_human"
    );
    assert_eq!(
        paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged")
    );
    assert_eq!(fx.remote_tip(), remote);

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_only_resolution_refuses_unpaused_state_and_surfaces_provider_rewrite_approval() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/merge_runs/start",
        workspace.id, fx.repo_id
    );
    let rejected = client.post_json(&route, &json!({})).await;
    assert_eq!(rejected.status(), StatusCode::CONFLICT);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_merge_runs WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        count, 0,
        "stale UI intent cannot invent a divergence workflow"
    );

    sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let local = fx.advance_mirror("local.txt", "local work\n");
    fx.advance_remote("remote.txt", "remote work\n");
    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;
    assert!(latest_run(&app, fx.key()).await.1.is_none());

    // The provider rewrites after passive detection. Resolve must refresh the
    // input and return approval, never launch an agent or publish the rewrite.
    let rewritten = git(
        &fx.seed,
        &[
            "commit-tree",
            "HEAD^{tree}",
            "-m",
            "rewritten provider root",
        ],
    );
    git(
        &fx.seed,
        &[
            "push",
            "--force",
            "origin",
            &format!("{rewritten}:refs/heads/main"),
        ],
    );
    let first = client.post_json(&route, &json!({})).await;
    let status = first.status();
    let first: Value = first.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["data"]["run"]["phase"], "reset_below_baseline");
    assert!(first["data"]["run"]["resolutionTaskId"].is_null());
    let second = client.post_json(&route, &json!({})).await;
    let status = second.status();
    let second: Value = second.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(first["data"]["run"]["id"], second["data"]["run"]["id"]);
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 0);
    assert_eq!(workspace_head(&app, fx.key()).await, local);
    assert_eq!(fx.remote_tip(), rewritten);
    assert_eq!(sync_watermark(&app, fx.key()).await, fx.c1);
    assert_eq!(
        paused_reason(&app, fx.key()).await.as_deref(),
        Some("watermark_violation")
    );

    fx.cleanup();
    app.cleanup().await;
}

async fn assert_conflict_resolution_flow(pause_only: bool, automatic: bool) {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    // Fresh organizations have no permission mode. The launch UI must supply
    // its confirmed selection instead of relying on fixture-only defaults.
    sqlx::query("UPDATE org_configs SET settings_json=settings_json - 'claude_permission_mode' - 'codex_permission_mode' WHERE org_id=$1")
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let local = fx.advance_mirror("README.md", "local conflicting content\n");
    let opening_remote = fx.advance_remote("README.md", "remote conflicting content\n");
    if pause_only {
        sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
            .bind(workspace.id)
            .execute(&app.db)
            .await
            .unwrap();
    }
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    let (_, existing_run_id, _) = latest_run(&app, fx.key()).await;
    let route = if pause_only {
        assert!(
            existing_run_id.is_none(),
            "automatic divergence does not create a workflow"
        );
        format!(
            "/api/v1/workspaces/{}/repos/{}/merge_runs/start",
            workspace.id, fx.repo_id
        )
    } else {
        format!(
            "/api/v1/workspaces/{}/repos/{}/merge_runs/{}/start",
            workspace.id,
            fx.repo_id,
            existing_run_id.unwrap()
        )
    };
    let client = app.authenticated_client(&owner);
    if pause_only {
        let member = create_member(&app, &org).await;
        let rejected = app
            .authenticated_client(&member)
            .post_json(&route, &json!({}))
            .await;
        assert_eq!(
            rejected.status(),
            StatusCode::FORBIDDEN,
            "only the owner may create a resolution"
        );
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM workspace_merge_runs WHERE workspace_id=$1")
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
    let empty_request = json!({});
    let (stopped, repeated) = tokio::join!(
        client.post_json(&route, &empty_request),
        client.post_json(&route, &empty_request)
    );
    assert_eq!(repeated.status(), StatusCode::CONFLICT);
    let status = stopped.status();
    let body: Value = stopped.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING");
    let run_id: Uuid = sqlx::query_scalar("SELECT id FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2 AND status='waiting_for_human'")
        .bind(workspace.id).bind(fx.repo_id).fetch_one(&app.db).await.unwrap();
    let run_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(workspace.id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        run_count, 1,
        "concurrent starts create one recoverable resolution record"
    );
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 0);

    let remote = fx.advance_remote(
        "later.txt",
        "provider advanced while resolution was waiting\n",
    );
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let missing = client.post_json(&route, &json!({})).await;
    let status = missing.status();
    let body: Value = missing.json().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string().contains("missing_agent_permission_mode"),
        "{body}"
    );

    let preferences = json!({"schemaVersion":1,
        "provider":{"source":"explicit","value":"claude_code"},
        "permissionMode":{"source":"explicit","value":"bypassPermissions"}});
    let preview = client
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/config/agent-launch/resolve",
                workspace.id
            ),
            &json!({"preferences":preferences}),
        )
        .await;
    let status = preview.status();
    let preview: Value = preview.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{preview}");
    let request = json!({"agentLaunch": {
        "preferences":preferences,
        "confirmedResolutionId":preview["data"]["resolved"]["resolutionId"],
    }});
    let mut stale = request.clone();
    stale["agentLaunch"]["confirmedResolutionId"] = json!("alr_stale");
    let rejected = client.post_json(&route, &stale).await;
    let status = rejected.status();
    let rejected: Value = rejected.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    assert!(
        rejected
            .to_string()
            .contains("stale_agent_launch_resolution"),
        "{rejected}"
    );
    let task_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(task_count, 0, "invalid launches must not admit work");
    let (first, second) = tokio::join!(
        client.post_json(&route, &request),
        client.post_json(&route, &request)
    );
    let first_status = first.status();
    let second_status = second.status();
    let first: Value = first.json().await.unwrap();
    let second: Value = second.json().await.unwrap();
    assert_eq!(first_status, StatusCode::OK, "{first}");
    assert_eq!(second_status, StatusCode::OK, "{second}");
    let task_id =
        Uuid::parse_str(first["data"]["run"]["resolutionTaskId"].as_str().unwrap()).unwrap();
    assert_eq!(
        first["data"]["run"]["resolutionTaskId"],
        second["data"]["run"]["resolutionTaskId"]
    );
    assert_eq!(
        first["data"]["run"]["resolutionSessionId"],
        second["data"]["run"]["resolutionSessionId"]
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let (spec, name): (Value, String) = sqlx::query_as("SELECT spec, name FROM tasks WHERE id=$1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        spec["agent"]["agentLaunch"]["permissionMode"],
        preferences["permissionMode"]
    );
    assert_eq!(
        name, "Resolve Git conflicts",
        "TaskService names the task it creates"
    );
    assert_eq!(
        spec["agent"]["agentLaunch"]["provider"],
        preferences["provider"]
    );
    let prompt: String = sqlx::query_scalar("SELECT content FROM session_inputs WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(prompt.contains("G5 has already fetched and delivered"));
    assert!(prompt.contains("do not fetch, refresh, create, or update the input refs"));
    assert!(
        prompt.contains(&local)
            && prompt.contains(&remote)
            && prompt.contains(&fx.repo_id.to_string())
    );
    assert_eq!(
        git(
            &fx.mirror,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::resolution_ref(workspace.id, run_id, "remote")
            ]
        ),
        remote
    );
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        local,
        "launch does not publish a candidate"
    );
    let mut db = app.scoped_db_for_org(org.id).await;
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
    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .apply(&mut db, dispatch.start_disposition(disposition))
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert!(runtime
        .received_rpc_methods()
        .await
        .contains(&"modules.invoke".to_string()));
    let (opening, inputs): (Value, Value) = sqlx::query_as(
        "SELECT pre_start_source_heads, source_heads FROM workspace_merge_runs WHERE id=$1",
    )
    .bind(run_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(opening["mirror_tip"], opening_remote);
    assert_eq!(inputs["mirror_tip"], remote);
    assert_eq!(inputs["workspace_tip"], local);

    let complete_route = format!(
        "/api/v1/workspaces/{}/repos/{}/merge_runs/{run_id}/complete",
        workspace.id, fx.repo_id
    );
    let local_tree = format!("{local}^{{tree}}");
    // A result rebased onto the old waiting observation must not complete the
    // new agent attempt, even though it satisfies the original opening cut.
    let stale = git(
        &fx.mirror,
        &[
            "commit-tree",
            &local_tree,
            "-p",
            &opening_remote,
            "-m",
            "stale resolution",
        ],
    );
    fx.set_workspace_tip(&app, &stale).await;
    let rejected = client.post_json(&complete_route, &json!({})).await;
    assert_eq!(rejected.status(), StatusCode::CONFLICT);

    // A merge retains both captured inputs but violates the rebase contract.
    // Also reject a single-parent tip hiding a merge earlier in the result.
    let merged = git(
        &fx.mirror,
        &[
            "commit-tree",
            &local_tree,
            "-p",
            &local,
            "-p",
            &remote,
            "-m",
            "merged inputs",
        ],
    );
    let after_merge = git(
        &fx.mirror,
        &[
            "commit-tree",
            &local_tree,
            "-p",
            &merged,
            "-m",
            "after merge",
        ],
    );
    for candidate in [&merged, &after_merge] {
        fx.set_workspace_tip(&app, candidate).await;
        let rejected = client.post_json(&complete_route, &json!({})).await;
        assert_eq!(
            rejected.status(),
            StatusCode::CONFLICT,
            "agent resolutions must rebase"
        );
        if automatic {
            // The worker applies the same refusal to a landed merge result.
            fx.enqueue(&app, "commit").await;
            drain_one_cycle(&app, fx.key()).await;
            assert_eq!(
                merge_run_completion(&app, run_id).await.0,
                "waiting_for_human"
            );
            assert_eq!(
                paused_reason(&app, fx.key()).await.as_deref(),
                Some("diverged")
            );
        }
    }

    // Execute a deterministic agent fixture through the production Git HTTP
    // receiver, with the same incarnation identity as an attached workspace.
    // A local-path origin would bypass receive-pack policy and hide rejection
    // of the non-fast-forward publication required by this rebase.
    fx.set_workspace_tip(&app, &local).await;
    git(&fx.mirror, &["checkout", "--detach"]);
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .unwrap();
    let auth_header = format!("http.extraHeader=Authorization: Bearer {token}");
    let origin = format!("{}/git/{}", app.base_url, fx.repo_id);
    let checkout = fx.tmp_root.join("resolution-workspace");
    git(
        &fx.tmp_root,
        &[
            "-c",
            &auth_header,
            "clone",
            "--branch",
            &workspace.name,
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
    git(
        &checkout,
        &["config", "user.email", "fixture-agent@test.local"],
    );
    git(&checkout, &["config", "user.name", "Fixture Agent"]);
    let operation: Value = serde_json::from_str(prompt.lines().nth(1).unwrap()).unwrap();
    let remote_input = operation["remoteInputRef"].as_str().unwrap();
    assert_eq!(
        git(&checkout, &["rev-parse", "HEAD"]),
        operation["localTip"].as_str().unwrap()
    );
    assert!(git(&checkout, &["status", "--porcelain"]).is_empty());
    let specs = g5_contracts::workspace_git::provider_fetch_refspecs(workspace.id);
    let mut args = vec!["fetch", "origin"];
    args.extend(specs.iter().map(String::as_str));
    git(&checkout, &args);
    assert_eq!(
        git(&checkout, &["rev-parse", remote_input]),
        operation["remoteTip"].as_str().unwrap()
    );
    let rebase_args: Vec<&str> = operation["rebaseArgs"]
        .as_array()
        .expect("the agent must receive an explicit rebase command")
        .iter()
        .map(|arg| arg.as_str().unwrap())
        .collect();
    assert_eq!(rebase_args[0], "rebase");
    assert!(rebase_args.contains(&"--no-rebase-merges"));
    let rebase = Command::new("git")
        .args(&rebase_args)
        .current_dir(&checkout)
        .output()
        .unwrap();
    assert_eq!(
        rebase.status.code(),
        Some(1),
        "fixture must encounter the actual content conflict"
    );
    assert_eq!(
        git(&checkout, &["diff", "--name-only", "--diff-filter=U"]),
        "README.md"
    );
    write_file(
        &checkout,
        "README.md",
        "local conflicting content\nremote conflicting content\n",
    );
    git(&checkout, &["add", "README.md"]);
    git(
        &checkout,
        &["-c", "core.editor=true", "rebase", "--continue"],
    );
    let resolved = git(&checkout, &["rev-parse", "HEAD"]);
    assert_eq!(git(&checkout, &["rev-parse", "HEAD^"]), remote);
    assert!(git(
        &checkout,
        &["rev-list", "--merges", &format!("{remote}..HEAD")]
    )
    .is_empty());
    assert!(!Command::new("git")
        .current_dir(&checkout)
        .args(["merge-base", "--is-ancestor", &local, "HEAD"])
        .status()
        .unwrap()
        .success());
    assert_eq!(
        std::fs::read_to_string(checkout.join("README.md")).unwrap(),
        "local conflicting content\nremote conflicting content\n"
    );
    let lease = format!("--force-with-lease=refs/heads/{}:{local}", workspace.name);
    git(
        &checkout,
        &[
            "push",
            &lease,
            "origin",
            &format!("HEAD:refs/heads/{}", workspace.name),
        ],
    );
    assert_eq!(workspace_head(&app, fx.key()).await, resolved);
    assert_eq!(
        fx.remote_tip(),
        remote,
        "resolution must not push to the external provider"
    );
    if automatic {
        // The production receive path queued the landing's wake; nobody clicks.
        let queued: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2",
        )
        .bind(workspace.id)
        .bind(fx.repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(
            queued, 1,
            "the accepted push must queue an integration wake"
        );
        drain_one_cycle(&app, fx.key()).await;
        assert_eq!(
            merge_run_completion(&app, run_id).await,
            ("completed".to_string(), None)
        );
        assert!(paused_reason(&app, fx.key()).await.is_none());
        let resolved_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 AND status='resolved' AND merge_run_id=$3 AND new_workspace_tip=$4",
        )
        .bind(workspace.id)
        .bind(fx.repo_id)
        .bind(run_id)
        .bind(&resolved)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(resolved_rows, 1);
        // No implicit publication without the GitHub App; the owner pushes.
        assert_eq!(fx.remote_tip(), remote);
        fx.enqueue(&app, "push").await;
    } else {
        let completed = client.post_json(&complete_route, &json!({})).await;
        let status = completed.status();
        let body: Value = completed.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["completed"], true);
    }

    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(latest_run(&app, fx.key()).await.0, "pushed");
    assert_eq!(
        fx.remote_tip(),
        resolved,
        "completion forwards the rebased commit to the existing provider branch"
    );
    git(
        &fx.bare,
        &["merge-base", "--is-ancestor", &remote, &resolved],
    );

    drop(runtime);
    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resolution_can_complete_at_the_remote_tip_when_local_intent_is_already_present() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    fx.advance_mirror("README.md", "local intent\n");
    fx.advance_remote("README.md", "remote intent\n");
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    let (_, run_id, _) = latest_run(&app, fx.key()).await;
    let run_id = run_id.unwrap();
    // The provider independently incorporates all child intent before the
    // resolution starts. Native rebase legitimately drops the redundant patch.
    let remote = fx.advance_remote("README.md", "local intent\n");
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/merge_runs/{run_id}",
        workspace.id, fx.repo_id
    );
    let start = client
        .post_json(&format!("{route}/start"), &json!({}))
        .await;
    let status = start.status();
    let body: Value = start.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    git(&fx.mirror, &["checkout", &workspace.name]);
    git(
        &fx.mirror,
        &["rebase", "--no-rebase-merges", "--no-fork-point", &remote],
    );
    assert_eq!(workspace_head(&app, fx.key()).await, remote);
    let completed = client
        .post_json(&format!("{route}/complete"), &json!({}))
        .await;
    let status = completed.status();
    let body: Value = completed.json().await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "empty replay range is valid: {body}"
    );
    assert_eq!(body["data"]["completed"], true);
    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resolution_retry_recovers_after_enqueue_or_commit_rollback_and_remote_advance() {
    for fail_commit in [false, true] {
        let app = spawn_worker_app().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
        fx.advance_mirror("README.md", "local conflicting content\n");
        let opening_remote = fx.advance_remote("README.md", "remote conflicting content\n");
        fx.enqueue(&app, "background_inbound").await;
        drain_one_cycle(&app, fx.key()).await;
        let (_, run_id, _) = latest_run(&app, fx.key()).await;
        let run_id = run_id.unwrap();
        let route = format!(
            "/api/v1/workspaces/{}/repos/{}/merge_runs/{run_id}/start",
            workspace.id, fx.repo_id
        );
        let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
        let client = app.authenticated_client(&owner);
        let trigger = if fail_commit {
            "CREATE CONSTRAINT TRIGGER review_fail_enqueue AFTER UPDATE ON workspace_merge_runs DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION review_fail_enqueue()"
        } else {
            "CREATE TRIGGER review_fail_enqueue BEFORE INSERT ON session_inputs FOR EACH ROW EXECUTE FUNCTION review_fail_enqueue()"
        };
        sqlx::raw_sql(&format!("CREATE FUNCTION review_fail_enqueue() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'review injected enqueue failure'; END $$; {trigger};"))
        .execute(&app.db).await.unwrap();
        let first = client.post_json(&route, &json!({})).await;
        let first_status = first.status();
        let first_body: Value = first.json().await.unwrap();
        assert_eq!(
            first_status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{first_body}"
        );
        let table = if fail_commit {
            "workspace_merge_runs"
        } else {
            "session_inputs"
        };
        sqlx::raw_sql(&format!(
            "DROP TRIGGER review_fail_enqueue ON {table}; DROP FUNCTION review_fail_enqueue();"
        ))
        .execute(&app.db)
        .await
        .unwrap();
        let task_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        let persisted_task: Option<Uuid> =
            sqlx::query_scalar("SELECT resolution_task_id FROM workspace_merge_runs WHERE id=$1")
                .bind(run_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        let retained_pin = git(
            &fx.mirror,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::resolution_ref(workspace.id, run_id, "remote"),
            ],
        );
        assert_eq!(task_count, 0);
        assert_eq!(persisted_task, None);
        assert_eq!(retained_pin, opening_remote);
        if !fail_commit {
            // Also recover an interrupted preparation that wrote only one pin.
            git(
                &fx.mirror,
                &[
                    "update-ref",
                    "-d",
                    &g5_contracts::workspace_git::resolution_ref(workspace.id, run_id, "local"),
                ],
            );
        }
        let next_remote = fx.advance_remote("later.txt", "provider advanced after failed launch\n");
        let next_local =
            fx.advance_mirror("local-later.txt", "local advanced after failed launch\n");
        let retry = client.post_json(&route, &json!({})).await;
        let retry_status = retry.status();
        let retry_body: Value = retry.json().await.unwrap();
        eprintln!("RECOVERY: failed_start={first_status}; committed_tasks={task_count}; resolution_task_id={persisted_task:?}; retained_pin={retained_pin}; new_remote={next_remote}; retry={retry_status}; body={retry_body}");
        assert_eq!(retry_status, StatusCode::OK, "{retry_body}");
        fx.advance_remote(
            "after-launch.txt",
            "provider moved after the committed launch\n",
        );
        let repeated = client.post_json(&route, &json!({})).await;
        assert_eq!(repeated.status(), StatusCode::OK);
        let repeated: Value = repeated.json().await.unwrap();
        assert_eq!(
            repeated["data"]["run"]["resolutionTaskId"],
            retry_body["data"]["run"]["resolutionTaskId"]
        );
        let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(tasks, 1);
        assert_eq!(
            git(
                &fx.mirror,
                &[
                    "rev-parse",
                    &g5_contracts::workspace_git::resolution_ref(workspace.id, run_id, "local")
                ]
            ),
            next_local
        );
        assert_eq!(
            git(
                &fx.mirror,
                &[
                    "rev-parse",
                    &g5_contracts::workspace_git::resolution_ref(workspace.id, run_id, "remote")
                ]
            ),
            next_remote
        );
        drop(runtime);
        fx.cleanup();
        app.cleanup().await;
        assert_eq!(
            retry_status,
            StatusCode::OK,
            "retry after rolled-back enqueue must recover: {retry_body}"
        );
    }
}

/// Base is remote-tracked like any other workspace
/// (`g5.workspace.remote.base-parity`): the ordinary worker imports its
/// provider fast-forwards with their watermark, converges a head that already
/// equals the provider tip, and leaves Base alone when auto-pull is off.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn base_tracks_its_provider_through_the_ordinary_worker() {
    for scenario in ["import", "equal_head_lagging_watermark", "auto_pull_off"] {
        let app = spawn_worker_app().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create_base(&app, &org).await;
        sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
            .bind(workspace.id)
            .bind(workspace.repo_id)
            .execute(&app.db)
            .await
            .unwrap();
        let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
        assert_eq!(
            fx.workspace_branch, "main",
            "Base tracks its default branch"
        );
        if scenario == "auto_pull_off" {
            let mut db = app.scoped_db_for_org(org.id).await;
            remote_integrations::set_auto_pull(db.conn(), fx.key(), false, owner.id)
                .await
                .expect("Base auto-pull uses the ordinary per-repository control");
            db.commit().await.unwrap();
        }
        let c2 = fx.advance_remote("remote_only.txt", "remote work\n");
        if scenario == "equal_head_lagging_watermark" {
            // A former out-of-band writer moved Base to the provider tip
            // without advancing the synchronization watermark.
            git(&fx.mirror, &["fetch", "origin", "main"]);
            git(&fx.mirror, &["update-ref", "refs/heads/main", &c2]);
        }

        fx.enqueue(&app, "background_inbound").await;
        drain_one_cycle(&app, fx.key()).await;

        let (expected_head, expected_watermark) = match scenario {
            "auto_pull_off" => (&fx.c1, &fx.c1),
            _ => (&c2, &c2),
        };
        assert_eq!(
            &workspace_head(&app, fx.key()).await,
            expected_head,
            "{scenario}"
        );
        assert_eq!(
            &sync_watermark(&app, fx.key()).await,
            expected_watermark,
            "{scenario}: the watermark moves with the accepted provider tip"
        );
        let outcomes = integration_run_outcomes(&app, fx.key()).await;
        match scenario {
            "import" => {
                assert_eq!(
                    outcomes,
                    vec![("imported".into(), "background_inbound".into())]
                );
                let provider_ref = g5_contracts::workspace_git::provider_ref(workspace.id, "main");
                assert_eq!(
                    git(&fx.mirror, &["rev-parse", &provider_ref]),
                    c2,
                    "Base records the provider observation like any workspace"
                );
            }
            "equal_head_lagging_watermark" => {
                assert_eq!(outcomes.len(), 1, "{outcomes:?}");
                assert_ne!(outcomes[0].0, "imported", "the head was already current");
            }
            _ => assert!(
                outcomes.is_empty(),
                "a disabled Base auto-pull runs no background cycle: {outcomes:?}"
            ),
        }
        assert!(paused_reason(&app, fx.key()).await.is_none(), "{scenario}");
        fx.cleanup();
        app.cleanup().await;
    }
}

/// Base never publishes to its provider (`g5.workspace.remote.base-parity`).
/// A lagging watermark and a provider reset to an older commit leave Base
/// ahead of its provider. Even in auto push mode a background cycle pauses
/// Base instead of pushing, while an ordinary workspace in the same state
/// still publishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn base_ahead_of_a_reset_provider_pauses_instead_of_pushing() {
    for is_base in [true, false] {
        let app = spawn_worker_app().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = if is_base {
            let base = TestWorkspace::create_base(&app, &org).await;
            sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
                .bind(base.id)
                .bind(base.repo_id)
                .execute(&app.db)
                .await
                .unwrap();
            base
        } else {
            TestWorkspace::create(&app, &org).await
        };
        let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;

        // The provider moves C1 -> C2 -> C3 and the workspace reaches C3 out
        // of band, leaving the watermark at C1.
        let c2 = fx.advance_remote("second.txt", "second\n");
        let c3 = fx.advance_remote("third.txt", "third\n");
        git(&fx.mirror, &["fetch", "origin", "main"]);
        fx.set_workspace_tip(&app, &c3).await;
        assert_eq!(sync_watermark(&app, fx.key()).await, fx.c1);
        // The provider resets to C2, an ancestor of the workspace head.
        git(&fx.seed, &["reset", "--hard", &c2]);
        git(&fx.seed, &["push", "--force", "origin", "main"]);
        assert_eq!(fx.remote_tip(), c2);

        // Auto push mode written directly, as a Base created before the
        // push-mode invariant would carry it.
        sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
            .bind(workspace.id)
            .execute(&app.db)
            .await
            .unwrap();
        fx.enqueue(&app, "background_inbound").await;
        drain_one_cycle(&app, fx.key()).await;

        let (status, trigger) = latest_run_status_and_trigger(&app, fx.key()).await;
        assert_eq!(trigger, "background_inbound");
        if is_base {
            assert_eq!(status, "diverged", "Base pauses instead of pushing");
            assert_eq!(
                paused_reason(&app, fx.key()).await.as_deref(),
                Some("diverged")
            );
            let runs: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2",
            )
            .bind(workspace.id)
            .bind(fx.repo_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(runs, 0, "Base opens no VM resolution run");
            assert_eq!(fx.remote_tip(), c2, "nothing is pushed over the reset");
            assert_eq!(workspace_head(&app, fx.key()).await, c3);

            // An explicit owner request still publishes nothing.
            fx.enqueue(&app, "manual").await;
            drain_one_cycle(&app, fx.key()).await;
            assert_eq!(fx.remote_tip(), c2, "a manual pull never pushes Base");
            assert_eq!(
                paused_reason(&app, fx.key()).await.as_deref(),
                Some("diverged")
            );
            // An explicit publication request for Base queues nothing.
            fx.enqueue(&app, "push").await;
            let queued: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1",
            )
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(queued, 0, "Base push requests are never queued");
        } else {
            // Auto mode attempts the implicit publication; this local remote
            // has no GitHub App, so the attempt is refused rather than paused.
            assert_eq!(status, "push_failed", "a working workspace still publishes");
            assert!(paused_reason(&app, fx.key()).await.is_none());
            fx.enqueue(&app, "push").await;
            drain_one_cycle(&app, fx.key()).await;
            assert_eq!(
                fx.remote_tip(),
                c3,
                "an explicit push publishes the ahead head"
            );
        }
        fx.cleanup();
        app.cleanup().await;
    }
}

/// A provider force-push pauses Base for rewrite review; the owner accepting
/// the provider history resets Base to the rewritten tip with its watermark
/// and pushes nothing (`g5.workspace.remote.base-provider-rewrite`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn base_accepts_a_provider_rewrite_without_a_vm() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    // Base keeps its fixture repository as the docs member, as every Base has.
    sqlx::query("UPDATE workspace_repos SET role='docs',root_path='docs' WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id).bind(base.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
        .bind(base.id)
        .bind(base.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let fx = TrackingFixture::setup(&app, &org, &base, owner.id).await;

    // The provider rewrites `main`: C1 is replaced by an unrelated sibling.
    git(&fx.seed, &["checkout", "main"]);
    git(&fx.seed, &["commit", "--amend", "-m", "rewritten C1"]);
    let rewritten = git(&fx.seed, &["rev-parse", "HEAD"]);
    git(&fx.seed, &["push", "--force", "origin", "main"]);
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        paused_reason(&app, fx.key()).await.as_deref(),
        Some("watermark_violation"),
        "a provider rewrite is never force-imported"
    );
    assert_eq!(workspace_head(&app, fx.key()).await, fx.c1);

    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/accept_provider_history",
        base.id, fx.repo_id
    );
    let preview_route = format!(
        "/api/v1/workspaces/{}/repos/{}/provider_history_preview",
        base.id, fx.repo_id
    );
    let preview = app
        .authenticated_client(&owner)
        .post_json(&preview_route, &json!({}))
        .await;
    let preview_status = preview.status();
    let preview: Value = preview.json().await.unwrap();
    assert_eq!(preview_status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["providerTip"], rewritten, "{preview}");
    assert_eq!(preview["data"]["discardedCommits"], 1, "{preview}");

    let accepted = app
        .authenticated_client(&owner)
        .post_json(
            &route,
            &json!({ "expectedProviderTip": rewritten, "expectedHead": fx.c1 }),
        )
        .await;
    let accepted_status = accepted.status();
    let accepted: Value = accepted.json().await.unwrap();
    assert_eq!(accepted_status, StatusCode::OK, "{accepted}");
    assert_eq!(workspace_head(&app, fx.key()).await, rewritten);
    assert_eq!(sync_watermark(&app, fx.key()).await, rewritten);
    assert!(paused_reason(&app, fx.key()).await.is_none());
    let open_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_merge_runs WHERE workspace_id=$1 AND repo_id=$2
           AND status IN ('pending','running','waiting_for_human')",
    )
    .bind(base.id)
    .bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(open_runs, 0, "the rewrite review closes with acceptance");
    assert_eq!(
        git(&fx.bare, &["rev-parse", "refs/heads/main"]),
        rewritten,
        "nothing is pushed back over the rewrite"
    );

    // The next ordinary cycle finds Base in sync with the provider.
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    assert!(paused_reason(&app, fx.key()).await.is_none());
    assert_eq!(workspace_head(&app, fx.key()).await, rewritten);
    fx.cleanup();
    app.cleanup().await;
}

/// A new binding starts with auto-pull on for Base and off elsewhere
/// (`g5.workspace.remote.base-auto-pull-default`).
#[tokio::test]
async fn new_bindings_default_auto_pull_on_only_for_base() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id)
        .bind(base.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let working = TestWorkspace::create(&app, &org).await;
    let mut fixtures = Vec::new();
    for workspace in [&base, &working] {
        let fx = TrackingFixture::setup(&app, &org, workspace, owner.id).await;
        sqlx::query("DELETE FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2")
            .bind(fx.workspace_id)
            .bind(fx.repo_id)
            .execute(&app.db)
            .await
            .unwrap();
        let mut db = app.scoped_db_for_org(org.id).await;
        remote_integrations::bind_published(db.conn(), fx.key(), "main")
            .await
            .expect("bind published member");
        db.commit().await.unwrap();
        fixtures.push(fx);
    }
    let mut defaults = Vec::new();
    for fx in &fixtures {
        let enabled: bool = sqlx::query_scalar(
            "SELECT auto_pull_enabled FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2",
        )
        .bind(fx.workspace_id)
        .bind(fx.repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        defaults.push(enabled);
    }
    assert_eq!(
        defaults,
        vec![true, false],
        "Base on, working workspace off"
    );
    for fx in &fixtures {
        fx.cleanup();
    }
    app.cleanup().await;
}

#[tokio::test]
async fn integration_queue_sql_preserves_precedence_and_approval_handoffs() {
    use g5_api::models::branch_integration_queue::{
        enqueue_branch_integration_for_advance_with_trigger, PushAuthority,
    };
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let kinds = [
        "approved_rebase_publish",
        "push",
        "commit",
        "manual",
        "background_inbound",
        "workspace_open",
    ];
    // Rows are existing intent, columns incoming intent. A table of outcomes
    // exercises the production SQL; no second implementation of its CASE exists.
    let expected = [
        [0, 0, 0, 0, 0, 0],
        [0, 1, 1, 1, 1, 1],
        [0, 1, 2, 3, 2, 2],
        [0, 1, 3, 3, 3, 3],
        [0, 1, 2, 3, 4, 5],
        [0, 1, 2, 3, 4, 5],
    ];
    for (row, existing) in kinds.iter().enumerate() {
        for (col, incoming) in kinds.iter().enumerate() {
            let mut db = app.scoped_db_for_org(org.id).await;
            enqueue_test_request(db.conn(), fx.key(), existing).await;
            enqueue_test_request(db.conn(), fx.key(), incoming).await;
            let actual: String = sqlx::query_scalar("SELECT trigger_kind FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2")
                .bind(workspace.id).bind(fx.repo_id).fetch_one(db.conn()).await.unwrap();
            assert_eq!(
                actual, kinds[expected[row][col]],
                "{existing} <- {incoming}"
            );
            db.rollback().await.unwrap();
        }
    }
    for active_checkpoint in [false, true] {
        for has_advance in [false, true] {
            let mut db = app.scoped_db_for_org(org.id).await;
            if active_checkpoint {
                g5_api::services::remote_tracking_merge::open_remote_tracking_merge_run(
                    db.conn(),
                    workspace.id,
                    fx.repo_id,
                    "main",
                    g5_api::services::branch_integration_service::DivergedPhase::ResetBelowBaseline,
                    &fx.c1,
                    Some(&fx.c1),
                    &fx.c1,
                )
                .await
                .unwrap();
            }
            enqueue_test_request(db.conn(), fx.key(), kinds[0]).await;
            if has_advance {
                let authority = PushAuthority::capture(db.conn(), workspace.id, owner.id)
                    .await
                    .unwrap();
                enqueue_branch_integration_for_advance_with_trigger(
                    db.conn(),
                    fx.key(),
                    Uuid::new_v4(),
                    "push",
                    Some(authority),
                )
                .await
                .unwrap();
            } else {
                enqueue_test_request(db.conn(), fx.key(), "push").await;
            }
            let actual: String = sqlx::query_scalar("SELECT trigger_kind FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2")
                .bind(workspace.id).bind(fx.repo_id).fetch_one(db.conn()).await.unwrap();
            assert_eq!(
                actual,
                if has_advance && !active_checkpoint {
                    "push"
                } else {
                    kinds[0]
                }
            );
            db.rollback().await.unwrap();
        }
    }
    fx.cleanup();
    app.cleanup().await;
}

async fn enqueue_test_request(
    conn: &mut sqlx::PgConnection,
    key: g5_contracts::types::WorkspaceRepoKey,
    trigger: &str,
) {
    use g5_api::models::branch_integration_queue::{
        enqueue_authorized_branch_integration, enqueue_branch_integration, PushAuthority,
    };
    if matches!(trigger, "push" | "approved_rebase_publish") {
        let owner: Uuid = sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
            .bind(key.workspace_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        let authority = PushAuthority::capture(conn, key.workspace_id, owner)
            .await
            .unwrap();
        enqueue_authorized_branch_integration(conn, key, trigger, chrono::Utc::now(), authority)
            .await
            .unwrap();
    } else {
        enqueue_branch_integration(conn, key, trigger)
            .await
            .unwrap();
    }
}

/// All implicit triggers reach the production credential boundary, even when
/// auto mode is enabled. An explicit route request still publishes normally.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_publication_requires_app_but_explicit_push_remains_available() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let mut fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    fx.attach_remote_tracked_docs_repo(&app, &org, &workspace)
        .await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
        .bind(workspace.id)
        .bind(workspace.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let tip = fx.advance_mirror("automatic.txt", "saved to G5, not implicitly published\n");
    sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    for trigger in ["commit", "manual", "workspace_open", "background_inbound"] {
        fx.enqueue(&app, trigger).await;
        drain_one_cycle(&app, fx.key()).await;
        let (status, detail): (String, Option<String>) = sqlx::query_as(
            "SELECT status,error_detail FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 ORDER BY started_at DESC LIMIT 1"
        ).bind(workspace.id).bind(fx.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(status, "push_failed", "{trigger}: {detail:?}");
        assert!(detail
            .unwrap()
            .contains("Automatic push requires a GitHub App"));
        assert_eq!(fx.remote_tip(), fx.c1);
        assert_eq!(workspace_head(&app, fx.key()).await, tip);
    }
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/push_to_remote", workspace.id),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    // An automatic wake coalesces with, but cannot replace, explicit consent.
    let mut db = app.scoped_db_for_org(org.id).await;
    enqueue_test_request(db.conn(), fx.key(), "commit").await;
    db.commit().await.unwrap();
    let actor: Uuid = sqlx::query_scalar("SELECT requested_by_user_id FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2")
        .bind(workspace.id).bind(fx.repo_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(actor, owner.id);
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        latest_run_status_and_trigger(&app, fx.key()).await,
        ("pushed".into(), "push".into())
    );
    assert_eq!(fx.remote_tip(), tip);
    fx.cleanup();
    app.cleanup().await;
}

/// Real transfer routes revoke queued consent, including transfer away and back.
/// A new owner's explicit request replaces stale approval and can publish.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_push_cannot_acquire_a_transferred_owners_authority() {
    for scenario in [
        "transfer",
        "transfer_back",
        "lifecycle",
        "legacy",
        "new_request",
    ] {
        let app = spawn_worker_app().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let mut fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
        fx.attach_remote_tracked_docs_repo(&app, &org, &workspace)
            .await;
        sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
            .bind(workspace.id)
            .bind(workspace.repo_id)
            .execute(&app.db)
            .await
            .unwrap();
        let recipient = TestUser::create_with_scopes(
            &app,
            org.id,
            &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
        )
        .await;
        let tip = fx.advance_mirror(
            "queued.txt",
            "authorizing owner must survive until execution\n",
        );
        fx.enqueue(&app, "push").await;
        if matches!(scenario, "transfer" | "transfer_back" | "new_request") {
            let response = app
                .authenticated_client(&owner)
                .post_json(
                    &format!("/api/v1/workspaces/{}/transfer", workspace.id),
                    &json!({"toUserId":recipient.id,"idempotencyKey":Uuid::new_v4().to_string()}),
                )
                .await;
            let status = response.status();
            let body: Value = response.json().await.unwrap();
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["data"]["status"], "completed", "{body}");
            if scenario == "transfer_back" {
                let response = app
                    .authenticated_client(&recipient)
                    .post_json(
                        &format!("/api/v1/workspaces/{}/transfer", workspace.id),
                        &json!({"toUserId":owner.id,"idempotencyKey":Uuid::new_v4().to_string()}),
                    )
                    .await;
                assert_eq!(response.status(), StatusCode::OK);
            }
        } else if scenario == "lifecycle" {
            sqlx::query(
                "UPDATE workspaces SET lifecycle_generation=lifecycle_generation+1 WHERE id=$1",
            )
            .bind(workspace.id)
            .execute(&app.db)
            .await
            .unwrap();
        } else {
            sqlx::query("UPDATE remote_integration_queue SET requested_by_user_id=NULL,lifecycle_generation=NULL WHERE workspace_id=$1")
                .bind(workspace.id).execute(&app.db).await.unwrap();
        }
        let mut db = app.scoped_db_for_org(org.id).await;
        // A delayed commit callback must not revive a former owner's request.
        enqueue_test_request(db.conn(), fx.key(), "commit").await;
        db.commit().await.unwrap();
        if scenario == "new_request" {
            let response = app
                .authenticated_client(&recipient)
                .post_json(
                    &format!("/api/v1/workspaces/{}/push_to_remote", workspace.id),
                    &json!({}),
                )
                .await;
            let status = response.status();
            let body: Value = response.json().await.unwrap();
            assert_eq!(status, StatusCode::OK, "{body}");
        }
        drain_one_cycle(&app, fx.key()).await;
        let (status, _) = latest_run_status_and_trigger(&app, fx.key()).await;
        assert_eq!(
            status,
            if scenario == "new_request" {
                "pushed"
            } else {
                "push_failed"
            },
            "{scenario}"
        );
        assert_eq!(
            fx.remote_tip(),
            if scenario == "new_request" {
                tip
            } else {
                fx.c1.clone()
            },
            "{scenario}"
        );
        fx.cleanup();
        app.cleanup().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_push_rechecks_authority_after_worker_admission() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, owner.id).await;
    let tip = fx.advance_mirror("racing.txt", "do not publish expired consent\n");
    fx.enqueue(&app, "push").await;

    // Expire consent after the worker captures its owner/generation snapshot,
    // at lease admission. Transport must recheck, not trust the picked row.
    sqlx::raw_sql(
        "CREATE FUNCTION expire_push_during_admission() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             UPDATE workspaces SET lifecycle_generation=lifecycle_generation+1
             WHERE id IN (SELECT workspace_id FROM remote_integrations WHERE repo_id=NEW.repo_id);
             RETURN NEW;
         END $$;
         CREATE TRIGGER expire_push_during_admission AFTER INSERT ON remote_ref_leases
         FOR EACH ROW EXECUTE FUNCTION expire_push_during_admission();",
    )
    .execute(&app.db)
    .await
    .unwrap();
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "expired consent must not reach transport"
    );
    assert_eq!(workspace_head(&app, fx.key()).await, tip);
    assert_eq!(
        latest_run_status_and_trigger(&app, fx.key()).await,
        ("push_failed".into(), "push".into())
    );
    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Co-landed imports (`g5.governance.remote.pull`)
// ---------------------------------------------------------------------------

/// A workspace with a remote-tracked code repository (the fixture's) and a
/// remote-tracked docs repository, both with auto-pull on.
async fn two_member_fixture(
    app: &TestApp,
) -> (TrackingFixture, WorkspaceRepoKey, TestWorkspace, TestOrg) {
    let (fx, docs, workspace, org, _owner) = two_member_fixture_with_owner(app).await;
    (fx, docs, workspace, org)
}

async fn two_member_fixture_with_owner(
    app: &TestApp,
) -> (
    TrackingFixture,
    WorkspaceRepoKey,
    TestWorkspace,
    TestOrg,
    TestUser,
) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let workspace = TestWorkspace::create(app, &org).await;
    let mut fx = TrackingFixture::setup(app, &org, &workspace, owner.id).await;
    fx.attach_remote_tracked_docs_repo(app, &org, &workspace)
        .await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2")
        .bind(workspace.id)
        .bind(workspace.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let docs_repo_id: Uuid = sqlx::query_scalar(
        "SELECT repo_id FROM workspace_repos WHERE workspace_id = $1 AND role = 'docs'",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let docs = WorkspaceRepoKey {
        org_id: fx.org_id,
        workspace_id: fx.workspace_id,
        repo_id: docs_repo_id,
    };
    (fx, docs, workspace, org, owner)
}

/// Advances the docs provider attached by `attach_remote_tracked_docs_repo`.
fn advance_docs_remote(fx: &TrackingFixture, file: &str) -> String {
    let seed = fx.tmp_root.join("docs-seed");
    git(&seed, &["checkout", "main"]);
    write_file(&seed, file, "docs change\n");
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "docs remote moved"]);
    git(&seed, &["push", "origin", "main"]);
    git(&seed, &["rev-parse", "HEAD"])
}

async fn enqueue_key(app: &TestApp, key: WorkspaceRepoKey, trigger: &str) {
    let mut db = app.scoped_db_for_org(key.org_id).await;
    enqueue_test_request(db.conn(), key, trigger).await;
    db.commit().await.unwrap();
}

/// Landings whose remote-tracking convergence covered every given repository.
async fn landings_covering(app: &TestApp, repos: &[Uuid]) -> i64 {
    let patterns: Vec<String> = repos.iter().map(|repo| format!("%{repo}%")).collect();
    sqlx::query_scalar(
        "SELECT count(*) FROM workspace_advance_consumer_inputs i
         JOIN workspace_advance_consumer_receipts r USING (operation_id, consumer_kind)
         WHERE i.payload::text LIKE ALL($1)",
    )
    .bind(&patterns)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// Runs `workers` concurrent worker loops until the workspace queue drains.
async fn drain_workspace(app: &TestApp, workspace_id: Uuid, workers: usize) {
    use g5_api::maintenance::branch_integration_worker::{
        run_branch_integration_worker, BranchIntegrationWorkerDeps,
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handles: Vec<_> = (0..workers)
        .map(|_| {
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
            tokio::spawn(run_branch_integration_worker(deps, cancel.clone()))
        })
        .collect();
    // Far below the 600s cycle lease: a cycle that waited for lease expiry
    // could not drain in time.
    let deadline = Instant::now() + Duration::from_secs(45);
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
        if Instant::now() > deadline {
            cancel.cancel();
            panic!("workers did not drain the workspace queue without waiting for lease expiry");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    cancel.cancel();
    for handle in handles {
        let _ = handle.await;
    }
}

/// When one member imports, every other member whose import is due lands in
/// the same cut, even without its own queued request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn due_imports_land_together_in_one_cut() {
    let app = spawn_worker_app().await;
    let (fx, docs, _workspace, _org) = two_member_fixture(&app).await;
    let code_tip = fx.advance_remote("code.txt", "code change\n");
    let docs_tip = advance_docs_remote(&fx, "docs.txt");

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(workspace_head(&app, fx.key()).await, code_tip);
    assert_eq!(workspace_head(&app, docs).await, docs_tip);
    assert_eq!(sync_watermark(&app, docs).await, docs_tip);
    assert_eq!(
        integration_run_outcomes(&app, docs).await,
        vec![("imported".to_string(), "background_inbound".to_string())],
        "the docs member imported without a queued request of its own"
    );
    assert_eq!(
        landings_covering(&app, &[fx.repo_id, docs.repo_id]).await,
        1,
        "both members landed in a single cut"
    );
    let leases: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_ref_leases WHERE repo_id = ANY($1)")
            .bind(vec![fx.repo_id, docs.repo_id])
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(leases, 0, "every co-member lease is released");
    fx.cleanup();
    app.cleanup().await;
}

/// A member that is paused, failing authentication or busy is left out of the
/// cut and never blocks the importing member.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn co_landing_leaves_out_paused_failing_or_busy_members() {
    for scenario in ["paused", "auth_failing", "busy"] {
        let app = spawn_worker_app().await;
        let (fx, docs, workspace, _org) = two_member_fixture(&app).await;
        let code_tip = fx.advance_remote("code.txt", "code change\n");
        let docs_c1 = workspace_head(&app, docs).await;
        advance_docs_remote(&fx, "docs.txt");
        match scenario {
            "paused" => {
                sqlx::query("UPDATE remote_integrations SET sync_paused_reason = 'fetch_failed' WHERE workspace_id = $1 AND repo_id = $2")
                    .bind(workspace.id).bind(docs.repo_id).execute(&app.db).await.unwrap();
            }
            "auth_failing" => {
                // A GitHub origin with no owner connection or App installation.
                sqlx::query("UPDATE repos SET provider = 'github', git_url = 'https://github.com/example/private-docs.git' WHERE id = $1")
                    .bind(docs.repo_id).execute(&app.db).await.unwrap();
            }
            _ => {
                sqlx::query(
                    "INSERT INTO remote_ref_leases (repo_id, remote_branch, org_id, owner_token, lease_generation, lease_expires_at)
                     VALUES ($1, 'main', $2, $3, 1, now() + interval '10 minutes')",
                )
                .bind(docs.repo_id)
                .bind(fx.org_id)
                .bind(Uuid::new_v4())
                .execute(&app.db)
                .await
                .unwrap();
            }
        }

        fx.enqueue(&app, "background_inbound").await;
        drain_one_cycle(&app, fx.key()).await;

        assert_eq!(workspace_head(&app, fx.key()).await, code_tip, "{scenario}");
        assert_eq!(
            integration_run_outcomes(&app, fx.key()).await,
            vec![("imported".to_string(), "background_inbound".to_string())],
            "{scenario}"
        );
        assert_eq!(
            workspace_head(&app, docs).await,
            docs_c1,
            "{scenario}: left out"
        );
        assert!(
            integration_run_outcomes(&app, docs).await.is_empty(),
            "{scenario}: the left-out member's own cycle records its outcome"
        );
        fx.cleanup();
        app.cleanup().await;
    }
}

/// A landing refused because another writer moved the workspace releases its
/// leases and retries on the next sweep instead of after lease expiry.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refused_landing_retries_promptly() {
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, _org) = two_member_fixture(&app).await;
    let code_tip = fx.advance_remote("code.txt", "code change\n");

    let barrier =
        g5_api::services::workspace_service::install_prepared_landing_barrier(workspace.id);
    fx.enqueue(&app, "background_inbound").await;
    let intervene = async {
        barrier.wait_until_prepared().await;
        // Another writer moves the docs member after the code cycle captured
        // the workspace's Git vector.
        let docs_mirror = app.repo_path(docs.repo_id).await;
        git(&docs_mirror, &["checkout", &workspace.name]);
        write_file(&docs_mirror, "concurrent.txt", "concurrent landing\n");
        git(&docs_mirror, &["add", "."]);
        git(&docs_mirror, &["commit", "-m", "concurrent landing"]);
        drop(barrier);
    };
    tokio::join!(drain_one_cycle(&app, fx.key()), intervene);

    assert_eq!(
        workspace_head(&app, fx.key()).await,
        code_tip,
        "the refused import retried with the fresh vector"
    );
    fx.cleanup();
    app.cleanup().await;
}

/// Two workers cycling different members of one workspace at once converge
/// without waiting for a lease to expire.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_member_cycles_converge_without_lease_expiry() {
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, _org) = two_member_fixture(&app).await;
    let code_tip = fx.advance_remote("code.txt", "code change\n");
    let docs_tip = advance_docs_remote(&fx, "docs.txt");
    fx.enqueue(&app, "background_inbound").await;
    enqueue_key(&app, docs, "background_inbound").await;

    drain_workspace(&app, workspace.id, 2).await;

    assert_eq!(workspace_head(&app, fx.key()).await, code_tip);
    assert_eq!(workspace_head(&app, docs).await, docs_tip);
    for key in [fx.key(), docs] {
        let imported = integration_run_outcomes(&app, key)
            .await
            .into_iter()
            .filter(|(status, _)| status == "imported")
            .count();
        assert_eq!(imported, 1, "each member imports exactly once");
    }
    fx.cleanup();
    app.cleanup().await;
}

/// Background auto-pull does not half-import a multi-repository workspace PR:
/// a member whose provider PR is merged, or being merged, while a sibling's
/// is not waits for the merged-PR import or an explicit pull.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auto_pull_holds_a_partly_merged_multi_repository_pull_request() {
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, org) = two_member_fixture(&app).await;
    let source = TestWorkspace::create(&app, &org).await;
    let pr: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_pull_requests (org_id, source_workspace_id, target_workspace_id,
             local_merge_status, provider_status, title, last_preflight_fingerprint)
         VALUES ($1, $2, $3, 'ready', 'partial', 'two repos', 'fp') RETURNING id",
    )
    .bind(fx.org_id)
    .bind(source.id)
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    for (repo, number, status) in [(fx.repo_id, 1_i64, "open"), (docs.repo_id, 2, "open")] {
        sqlx::query(
            "INSERT INTO workspace_pull_request_provider_repos (org_id, pull_request_id, repo_id, provider,
                 provider_repo_full_name, source_remote_branch, target_remote_branch, provider_pr_number, provider_status)
             VALUES ($1, $2, $3, 'github', 'example/repo', 'feature', 'main', $4, $5)",
        )
        .bind(fx.org_id)
        .bind(pr)
        .bind(repo)
        .bind(number)
        .bind(status)
        .execute(&app.db)
        .await
        .unwrap();
    }
    let code_c1 = fx.c1.clone();
    let code_tip = fx.advance_remote("code.txt", "merged code PR\n");

    // An in-flight provider merge holds the PR row: the code import waits.
    let mut merging = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspace_pull_requests WHERE id = $1 FOR UPDATE")
        .bind(pr)
        .execute(&mut *merging)
        .await
        .unwrap();
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    merging.rollback().await.unwrap();
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        code_c1,
        "held while merging"
    );
    assert_eq!(
        latest_run_status_and_trigger(&app, fx.key()).await.0,
        "fetched",
        "a held member records the observation only"
    );

    // A lost provider transaction no longer holds the PR lock. Its durable
    // unresolved attempt still protects the multi-repository observation.
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET merge_attempt_id=gen_random_uuid(),merge_execution_id=gen_random_uuid(),merge_target_head_sha=$3,merge_pending=true,merge_uncertain=true WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(pr).bind(fx.repo_id).bind(&code_c1).execute(&app.db).await.unwrap();
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        code_c1,
        "held after the provider transaction is lost"
    );

    // The code PR merged but the docs PR did not: still held, and the docs
    // member's own import does not co-land it.
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_status = 'merged',merge_pending=false,merge_accepted=true WHERE pull_request_id = $1 AND repo_id = $2")
        .bind(pr).bind(fx.repo_id).execute(&app.db).await.unwrap();
    let docs_tip = advance_docs_remote(&fx, "docs.txt");
    enqueue_key(&app, docs, "background_inbound").await;
    drain_one_cycle(&app, docs).await;
    assert_eq!(workspace_head(&app, docs).await, docs_tip);
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        code_c1,
        "not half-imported"
    );

    // The last unconfirmed member remains held even after all its siblings
    // report acceptance: its delayed provider effect is still part of the PR.
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET merge_pending=true,merge_accepted=false WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(pr).bind(fx.repo_id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspace_pull_request_provider_repos SET provider_status='merged' WHERE pull_request_id=$1 AND repo_id=$2")
        .bind(pr).bind(docs.repo_id).execute(&app.db).await.unwrap();
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        code_c1,
        "the last unresolved member stays held"
    );

    // An explicit pull is the owner's choice and imports.
    fx.enqueue(&app, "manual").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(workspace_head(&app, fx.key()).await, code_tip);
    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Fork capture of a multi-repository cut (`g5.fork.source.cut`)
// ---------------------------------------------------------------------------

async fn request_fork(
    app: &TestApp,
    user: &TestUser,
    source: Uuid,
    name: &str,
    key: &str,
) -> (StatusCode, Value) {
    let response = app
        .authenticated_client(user)
        .post_json(
            &format!("/api/v1/workspaces/{source}/clone"),
            &json!({ "name": name, "idempotencyKey": key }),
        )
        .await;
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

/// The source vector a fork froze at admission, by repository.
async fn fork_source_vector(app: &TestApp, key: &str) -> Option<BTreeMap<Uuid, String>> {
    let manifest: Option<Value> =
        sqlx::query_scalar("SELECT repo_manifest FROM workspace_forks WHERE idempotency_key = $1")
            .bind(key)
            .fetch_optional(&app.db)
            .await
            .unwrap();
    manifest.map(|manifest| {
        manifest
            .as_array()
            .unwrap()
            .iter()
            .map(|member| {
                (
                    member["repo_id"].as_str().unwrap().parse().unwrap(),
                    member["source_oid"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    })
}

fn vector(pairs: &[(Uuid, &str)]) -> BTreeMap<Uuid, String> {
    pairs
        .iter()
        .map(|(repo, head)| (*repo, (*head).to_string()))
        .collect()
}

/// A fork admitted while a two-repository landing is prepared but not yet
/// landing captures the whole untouched vector.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fork_before_a_landing_captures_the_untouched_vector() {
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, _org, owner) = two_member_fixture_with_owner(&app).await;
    let code_c1 = workspace_head(&app, fx.key()).await;
    let docs_c1 = workspace_head(&app, docs).await;
    let code_tip = fx.advance_remote("code.txt", "code change\n");
    let docs_tip = advance_docs_remote(&fx, "docs.txt");

    let barrier =
        g5_api::services::workspace_service::install_prepared_landing_barrier(workspace.id);
    fx.enqueue(&app, "background_inbound").await;
    let fork = async {
        barrier.wait_until_prepared().await;
        let (status, body) =
            request_fork(&app, &owner, workspace.id, "fork-before", "fork-before").await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        drop(barrier);
    };
    tokio::join!(drain_one_cycle(&app, fx.key()), fork);

    assert_eq!(
        fork_source_vector(&app, "fork-before").await.unwrap(),
        vector(&[(fx.repo_id, &code_c1), (docs.repo_id, &docs_c1)]),
        "the fork froze the untouched prior vector"
    );
    assert_eq!(workspace_head(&app, fx.key()).await, code_tip);
    assert_eq!(workspace_head(&app, docs).await, docs_tip);
    fx.cleanup();
    app.cleanup().await;
}

/// A fork requested while a two-repository landing has moved its refs but not
/// committed waits for that landing and freezes the whole new vector. A fork
/// that finds the commit fence busy past its bound refuses with a retryable
/// error and leaves no fork record, and an identical retry then admits.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fork_during_a_landing_waits_or_refuses_retryably() {
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, _org, owner) = two_member_fixture_with_owner(&app).await;
    let code_tip = fx.advance_remote("code.txt", "code change\n");
    let docs_tip = advance_docs_remote(&fx, "docs.txt");
    let new_vector = vector(&[(fx.repo_id, &code_tip), (docs.repo_id, &docs_tip)]);

    let barrier =
        g5_api::services::workspace_service::install_refs_applied_landing_barrier(workspace.id);
    fx.enqueue(&app, "background_inbound").await;
    let fork = async {
        barrier.wait_until_prepared().await;
        // Both refs already carry the new heads, uncommitted.
        assert_eq!(workspace_head(&app, fx.key()).await, code_tip);
        let finish_landing = async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            drop(barrier);
        };
        let ((status, body), ()) = tokio::join!(
            request_fork(&app, &owner, workspace.id, "fork-wait", "fork-wait"),
            finish_landing
        );
        assert_eq!(status, StatusCode::CREATED, "{body}");
    };
    tokio::join!(drain_one_cycle(&app, fx.key()), fork);
    assert_eq!(
        fork_source_vector(&app, "fork-wait").await.unwrap(),
        new_vector,
        "the fork froze the whole committed vector"
    );

    // Another fence holder keeps the fence past the fork's bound.
    let mut holder = app.db.begin().await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('g5:workspace-commit:' || $1::text, 0))",
    )
    .bind(workspace.id)
    .execute(&mut *holder)
    .await
    .unwrap();
    let (status, body) = request_fork(&app, &owner, workspace.id, "fork-busy", "fork-busy").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.to_string().contains("WORKSPACE_COMMIT_FENCE_BUSY"),
        "{body}"
    );
    assert!(
        fork_source_vector(&app, "fork-busy").await.is_none(),
        "a busy refusal leaves no fork record"
    );
    holder.rollback().await.unwrap();
    let (status, body) = request_fork(&app, &owner, workspace.id, "fork-busy", "fork-busy").await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "an identical retry admits: {body}"
    );
    assert_eq!(
        fork_source_vector(&app, "fork-busy").await.unwrap(),
        new_vector
    );
    fx.cleanup();
    app.cleanup().await;
}

/// A landing whose database COMMIT fails after Git accepted its cut never
/// retracts the refs, so no fork can capture heads that later disappear: a
/// fork afterwards freezes exactly the refs, the whole new vector.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refs_are_never_retracted_after_a_failed_commit() {
    use g5_api::maintenance::branch_integration_worker::{
        run_branch_integration_worker, BranchIntegrationWorkerDeps,
    };
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, _org, owner) = two_member_fixture_with_owner(&app).await;
    let code_tip = fx.advance_remote("code.txt", "code change\n");
    let docs_tip = advance_docs_remote(&fx, "docs.txt");

    let barrier =
        g5_api::services::workspace_service::install_refs_applied_landing_barrier(workspace.id);
    fx.enqueue(&app, "background_inbound").await;
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
    let worker = tokio::spawn(run_branch_integration_worker(deps, cancel.clone()));
    barrier.wait_until_prepared().await;

    // Kill the landing's session: its COMMIT can no longer succeed.
    let killed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT pg_terminate_backend(lock.pid) FROM pg_locks lock
             JOIN pg_stat_activity activity ON activity.pid = lock.pid
             WHERE lock.locktype = 'advisory' AND lock.granted
               AND activity.datname = current_database() AND lock.pid <> pg_backend_pid()) killed",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(killed >= 1, "the landing holds the commit fence");
    drop(barrier);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let fenced: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_locks lock JOIN pg_stat_activity activity
                 ON activity.pid = lock.pid
              WHERE lock.locktype = 'advisory' AND activity.datname = current_database())",
        )
        .fetch_one(&app.db)
        .await
        .unwrap();
        if !fenced {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the failed landing released the fence"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    cancel.cancel();
    let _ = worker.await;

    assert!(
        integration_run_outcomes(&app, fx.key()).await.is_empty(),
        "the database side of the landing did not commit"
    );
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        code_tip,
        "never retracted"
    );
    assert_eq!(
        workspace_head(&app, docs).await,
        docs_tip,
        "never retracted"
    );
    let (status, body) = request_fork(&app, &owner, workspace.id, "fork-after", "fork-after").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        fork_source_vector(&app, "fork-after").await.unwrap(),
        vector(&[(fx.repo_id, &code_tip), (docs.repo_id, &docs_tip)])
    );
    fx.cleanup();
    app.cleanup().await;
}

/// Forking warns, without refusing, about remote-tracked source members that
/// are held, paused or behind (`g5.fork.source.advisory`). The tracking summary
/// and an explicit Pull name the half-merged workspace PR holding a member
/// (`g5.governance.remote.pull`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fork_advisories_and_pull_name_held_paused_and_behind_members() {
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, org, owner) = two_member_fixture_with_owner(&app).await;
    let source = TestWorkspace::create(&app, &org).await;
    let pr: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_pull_requests (org_id, source_workspace_id, target_workspace_id,
             local_merge_status, provider_status, title, last_preflight_fingerprint)
         VALUES ($1, $2, $3, 'ready', 'partial', 'Cross-repo change', 'fp') RETURNING id",
    )
    .bind(fx.org_id)
    .bind(source.id)
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    for (repo, number, status) in [(fx.repo_id, 1_i64, "merged"), (docs.repo_id, 2, "open")] {
        sqlx::query(
            "INSERT INTO workspace_pull_request_provider_repos (org_id, pull_request_id, repo_id, provider,
                 provider_repo_full_name, source_remote_branch, target_remote_branch, provider_pr_number, provider_status)
             VALUES ($1, $2, $3, 'github', 'example/repo', 'feature', 'main', $4, $5)",
        )
        .bind(fx.org_id)
        .bind(pr)
        .bind(repo)
        .bind(number)
        .bind(status)
        .execute(&app.db)
        .await
        .unwrap();
    }
    // The code member is held and behind its provider; the docs member is paused.
    let code_tip = fx.advance_remote("code.txt", "merged code PR\n");
    let observed = observe_tip(&app, fx.key(), &code_tip).await;
    sqlx::query("UPDATE remote_integrations SET sync_paused_reason = 'fetch_failed' WHERE workspace_id = $1 AND repo_id = $2")
        .bind(workspace.id)
        .bind(docs.repo_id)
        .execute(&app.db)
        .await
        .unwrap();

    let response = app
        .authenticated_client(&owner)
        .get(&format!(
            "/api/v1/workspaces/{}/fork_advisories",
            workspace.id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let advisories = body["data"].as_array().unwrap();
    let kinds = |repo: Uuid| {
        advisories
            .iter()
            .filter(|advisory| advisory["repoId"] == repo.to_string())
            .map(|advisory| advisory["kind"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert!(observed, "the provider observation is recorded");
    assert_eq!(
        kinds(fx.repo_id),
        vec!["import_held".to_string(), "behind".to_string()],
        "{body}"
    );
    assert_eq!(kinds(docs.repo_id), vec!["paused".to_string()], "{body}");
    let held = advisories
        .iter()
        .find(|advisory| advisory["kind"] == "import_held")
        .unwrap();
    assert_eq!(held["pullRequest"]["title"], "Cross-repo change");
    assert_eq!(held["pullRequest"]["pullRequestId"], pr.to_string());
    let paused = advisories
        .iter()
        .find(|advisory| advisory["kind"] == "paused")
        .unwrap();
    assert_eq!(paused["pauseReason"], "fetch_failed");

    // The tracking summary names the holding PR.
    let response = app
        .authenticated_client(&owner)
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/sync_runs",
            workspace.id, fx.repo_id
        ))
        .await;
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["data"]["branch"]["repos"][0]["importHeldBy"]["title"], "Cross-repo change",
        "{body}"
    );

    // An explicit Pull imports the held member anyway and names the PR.
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/sync", workspace.id),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let details = body["data"]["details"].as_array().unwrap();
    let held = details
        .iter()
        .find(|detail| detail["repoId"] == fx.repo_id.to_string())
        .unwrap();
    assert_eq!(held["heldBy"]["title"], "Cross-repo change", "{body}");
    assert!(details
        .iter()
        .find(|detail| detail["repoId"] == docs.repo_id.to_string())
        .unwrap()
        .get("heldBy")
        .is_none());
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        workspace_head(&app, fx.key()).await,
        code_tip,
        "the explicit Pull imported the held member"
    );

    // Forking is never refused for these, and its response carries them.
    let (status, body) =
        request_fork(&app, &owner, workspace.id, "fork-advised", "fork-advised").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let kinds = body["data"]["sourceAdvisories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|advisory| advisory["kind"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"import_held".to_string()), "{body}");
    assert!(kinds.contains(&"paused".to_string()), "{body}");

    // Closing the workspace PR lifts the hold.
    sqlx::query("UPDATE workspace_pull_requests SET status = 'closed' WHERE id = $1")
        .bind(pr)
        .execute(&app.db)
        .await
        .unwrap();
    let response = app
        .authenticated_client(&owner)
        .get(&format!(
            "/api/v1/workspaces/{}/fork_advisories",
            workspace.id
        ))
        .await;
    let body: Value = response.json().await.unwrap();
    assert!(
        !body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|advisory| advisory["kind"] == "import_held"),
        "{body}"
    );
    fx.cleanup();
    app.cleanup().await;
}

/// An organization administrator who does not own Base may Pull it and turn
/// its auto-pull off and on; the import still acts as Base's owner
/// (`g5.workspace.remote.base-admin-pull`, `g5.workspace.remote.base-parity`).
/// A plain member, an administrator without an `org:admin` token scope and an
/// administrator on a working workspace they do not own stay refused.
#[tokio::test]
async fn org_admin_pulls_base_and_sets_its_auto_pull_as_base_owner() {
    let app = spawn_worker_app().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id)
        .bind(base.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut fx = TrackingFixture::setup(&app, &org, &base, owner.id).await;
    // Coordinated tracking actions need the workspace's docs repository.
    fx.attach_remote_tracked_docs_repo(&app, &org, &base).await;
    let working = TestWorkspace::create(&app, &org).await;

    let org_user = |role: &'static str| {
        let app = &app;
        let org_id = org.id;
        async move {
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
            sqlx::query("UPDATE org_memberships SET role=$3 WHERE org_id=$1 AND user_id=$2")
                .bind(org_id)
                .bind(user.id)
                .bind(role)
                .execute(&app.db)
                .await
                .unwrap();
            user
        }
    };
    let admin = org_user("admin").await;
    let member = org_user("member").await;
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
    let auto_pull_path = format!(
        "/api/v1/workspaces/{}/repos/{}/auto_pull",
        base.id, fx.repo_id
    );
    let sync_path = format!("/api/v1/workspaces/{}/sync", base.id);
    let auto_pull = || async {
        sqlx::query_scalar::<_, bool>(
            "SELECT auto_pull_enabled FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2",
        )
        .bind(fx.workspace_id)
        .bind(fx.repo_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
    };
    let initial = auto_pull().await;

    for (label, user) in [
        ("member", &member),
        (
            "admin without org:admin token scope",
            &admin_without_org_scope,
        ),
    ] {
        let response = app
            .authenticated_client(user)
            .put_json(&auto_pull_path, &json!({ "enabled": !initial }))
            .await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{label} auto-pull"
        );
        let response = app
            .authenticated_client(user)
            .post_json(&sync_path, &json!({}))
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{label} pull");
    }
    assert_eq!(auto_pull().await, initial);
    assert!(integration_run_outcomes(&app, fx.key()).await.is_empty());
    let response = app
        .authenticated_client(&admin)
        .post_json(
            &format!("/api/v1/workspaces/{}/sync", working.id),
            &json!({}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Pull on a working workspace stays its owner's"
    );

    for enabled in [!initial, initial] {
        let response = app
            .authenticated_client(&admin)
            .put_json(&auto_pull_path, &json!({ "enabled": enabled }))
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "admin auto-pull {enabled}: {body}");
        assert_eq!(body["data"]["autoPullEnabled"], enabled, "{body}");
        assert_eq!(auto_pull().await, enabled);
    }
    sqlx::query("DELETE FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2")
        .bind(fx.workspace_id)
        .bind(fx.repo_id)
        .execute(&app.db)
        .await
        .unwrap();

    let remote = fx.advance_remote("admin-pull.txt", "pulled by an administrator\n");
    let response = app
        .authenticated_client(&admin)
        .post_json(&sync_path, &json!({}))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "admin pull: {body}");
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(workspace_head(&app, fx.key()).await, remote);
    assert_eq!(
        integration_run_outcomes(&app, fx.key()).await,
        vec![("imported".into(), "manual".into())]
    );
    let acting: Vec<Uuid> = sqlx::query_scalar(
        "SELECT acting_user_id FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(acting, vec![owner.id], "the import acts as Base's owner");
    fx.cleanup();
    app.cleanup().await;
}

/// The fork's advisory read is only a hint,
/// so a failed statement on the hint table (missing here, as during a rollout
/// that has not yet applied its migration) must not poison the fork's
/// transaction. The fork completes with no advisories instead of failing at
/// commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_advisory_read_never_blocks_the_fork() {
    let app = spawn_worker_app().await;
    let (_fx, _docs, workspace, _org, owner) = two_member_fixture_with_owner(&app).await;
    sqlx::query("ALTER TABLE remote_tip_observations RENAME TO remote_tip_observations_missing")
        .execute(&app.db)
        .await
        .unwrap();

    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({"name": "fork-without-hints", "idempotencyKey": "fork-without-hints-1"}),
        )
        .await;
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    // The DTO omits the list when it is empty.
    assert!(
        body["data"]["sourceAdvisories"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "a failed hint read yields no advisories and never blocks the fork: {body}"
    );
    assert_eq!(
        body["data"]["workspace"]["name"], "fork-without-hints",
        "{body}"
    );
    app.cleanup().await;
}

/// The standalone fork-advisories route treats the remote-tip hint as
/// optional: when the hint cannot be read (table missing, as during a
/// rollout) the held advisory is still returned and no member is reported
/// "behind". Both members read the hint (none is paused), so whichever
/// member is processed first, the next member's policy read follows the
/// failed hint read: without the savepoint that read meets an aborted
/// transaction and the route answers 500 regardless of member order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fork_advisories_degrade_a_failed_hint_read_without_losing_the_rest() {
    let app = spawn_worker_app().await;
    let (fx, docs, workspace, org, owner) = two_member_fixture_with_owner(&app).await;
    // Hold the code member through a merged provider PR (as the sibling test does).
    let source = TestWorkspace::create(&app, &org).await;
    let pr: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_pull_requests (org_id, source_workspace_id, target_workspace_id,
             local_merge_status, provider_status, title, last_preflight_fingerprint)
         VALUES ($1, $2, $3, 'ready', 'partial', 'Cross-repo change', 'fp') RETURNING id",
    )
    .bind(fx.org_id)
    .bind(source.id)
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    for (repo, number, status) in [(fx.repo_id, 1_i64, "merged"), (docs.repo_id, 2, "open")] {
        sqlx::query(
            "INSERT INTO workspace_pull_request_provider_repos (org_id, pull_request_id, repo_id, provider,
                 provider_repo_full_name, source_remote_branch, target_remote_branch, provider_pr_number, provider_status)
             VALUES ($1, $2, $3, 'github', 'example/repo', 'feature', 'main', $4, $5)",
        )
        .bind(fx.org_id)
        .bind(pr)
        .bind(repo)
        .bind(number)
        .bind(status)
        .execute(&app.db)
        .await
        .unwrap();
    }
    sqlx::query("ALTER TABLE remote_tip_observations RENAME TO remote_tip_observations_missing")
        .execute(&app.db)
        .await
        .unwrap();

    let response = app
        .authenticated_client(&owner)
        .get(&format!(
            "/api/v1/workspaces/{}/fork_advisories",
            workspace.id
        ))
        .await;
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    let advisories = body["data"].as_array().unwrap();
    let kinds = |repo: Uuid| {
        advisories
            .iter()
            .filter(|advisory| advisory["repoId"] == repo.to_string())
            .map(|advisory| advisory["kind"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds(fx.repo_id), vec!["import_held".to_string()], "{body}");
    assert!(
        kinds(docs.repo_id).is_empty(),
        "no hint means no 'behind' advisory and nothing else: {body}"
    );
    fx.cleanup();
    app.cleanup().await;
}
