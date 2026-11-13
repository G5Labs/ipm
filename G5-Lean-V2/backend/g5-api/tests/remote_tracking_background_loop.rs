//! Live integration tests for the **background inbound sync loop** — the
//! `branch_integration_scheduler` + the worker's `background_inbound` behavior
//! (test_view §3.12 D57–D63, G5 hardening, chat C26).
//!
//! Two real components are driven end to end against a real Postgres tenant DB
//! + a real local bare git remote + an on-disk mirror:
//!
//!   * [`run_branch_integration_scheduler`] — the timer loop that UPSERTs a
//!     `background_inbound` queue row for every UNPAUSED remote-tracking branch
//!     (D57). Per design §6.2 / arch D50 it does NOT classify, fetch, or check
//!     cleanliness — it only enqueues; the worker handles dirtiness at pickup.
//!   * [`run_branch_integration_worker`] — the drain loop, exercised here with
//!     `trigger_kind = 'background_inbound'`:
//!       - **D58** clean + remote-ahead → `imported` (head + baseline advance).
//!       - **D59** clean + workspace-ahead → `noop` (NO push — push requires a
//!         commit-coupled / operator trigger).
//!       - **D60** dirty → `dirty_skipped` (`error_detail='background_dirty'`),
//!         advisory refreshed, head unchanged.
//!       - **D61** dirty → clean transition resumes sync with no explicit signal
//!         (the next background tick imports).
//!       - **D62** a prior `push_failed` does NOT trigger a background push
//!         retry — the tick is `noop`, the remote ref is unchanged.
//!
//! D63 (cadence is deployment-config-driven) is structural — the scheduler/worker
//! loops take their interval from `AppConfig` (no hardcoded cadence); the exact
//! wall-clock tick rate is not unit-assertable, so D63 stays `partial` in §7.
//!
//! No mocks/stubs of in-scope services (Tier 1 discipline): the scheduler, the
//! worker, the git transport, and the DB are all real.
//!
//! SOT:
//! - `designs/REMOTE_REPO_TRACKING.md` §6.2 (background inbound loop — clean-only convergence)
//! - `views/arch_views/remote_repo_tracking.arch_view.md` D50 (scheduler), D53 (commit wins), D59 (background noop)
//! - `views/test_views/remote_repo_tracking.test_view.md` §3.12 D57–D63 (D57 reconciled C26)
//! - `generation/.semlift/specs/g5/g5-api/maintenance/branch_integration_scheduler.semlift.md`
//! - Chat: `remote-tracking-gap-closure-plan-2026-05-28` (C26)

use crate::helpers;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use g5_contracts::types::WorkspaceRepoKey;
use g5_scm::GitRepo;
use helpers::{seed_workspace_authority_head, TestApp, TestOrg, TestUser, TestWorkspace};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// git helpers — drive a real local bare remote + mirror clone
// ---------------------------------------------------------------------------

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

/// One remote-tracking branch (its own repo + bare remote + mirror).
struct TrackingFixture {
    org_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    workspace_branch: String,
    bare: PathBuf,
    seed: PathBuf,
    mirror: PathBuf,
    c1: String,
    tmp_root: PathBuf,
}

impl TrackingFixture {
    async fn setup(
        app: &TestApp,
        org: &TestOrg,
        workspace: &TestWorkspace,
        owner_user_id: Uuid,
    ) -> Self {
        // The worker attributes reconciliation to the current workspace owner.
        sqlx::query("UPDATE workspaces SET owner_user_id = $2, push_mode = 'manual' WHERE id = $1")
            .bind(workspace.id)
            .bind(owner_user_id)
            .execute(&app.db)
            .await
            .expect("set current workspace owner");

        let tmp_root = std::env::temp_dir().join(format!("g5-rt-bgl-{}", Uuid::new_v4().simple()));
        let bare = tmp_root.join("remote.git");
        let seed = tmp_root.join("seed");
        std::fs::create_dir_all(&bare).expect("create bare dir");
        std::fs::create_dir_all(&seed).expect("create seed dir");

        git(&bare, &["init", "--bare", "--initial-branch=main"]);

        git(&seed, &["init", "--initial-branch=main"]);
        git(&seed, &["config", "user.email", "g5@test.local"]);
        git(&seed, &["config", "user.name", "G5 Test"]);
        write_file(&seed, "README.md", "background loop fixture\n");
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-m", "C1 seed"]);
        let c1 = git(&seed, &["rev-parse", "HEAD"]);
        git(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&seed, &["push", "origin", "main"]);
        git(&bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);

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

        let slug = format!("rt-bgl-repo-{}", repo_id.simple());
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

        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled) VALUES ($1,$2,$3,'main',true)")
            .bind(org.id).bind(workspace.id).bind(repo_id).execute(&app.db).await.unwrap();
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
            c1,
            tmp_root,
        }
    }

    fn advance_remote(&self, file: &str, contents: &str) -> String {
        git(&self.seed, &["checkout", "main"]);
        write_file(&self.seed, file, contents);
        git(&self.seed, &["add", "."]);
        git(&self.seed, &["commit", "-m", "remote moved"]);
        let tip = git(&self.seed, &["rev-parse", "HEAD"]);
        git(&self.seed, &["push", "origin", "main"]);
        tip
    }

    fn advance_mirror(&self, file: &str, contents: &str) -> String {
        git(&self.mirror, &["checkout", &self.workspace_branch]);
        write_file(&self.mirror, file, contents);
        git(&self.mirror, &["add", "."]);
        git(&self.mirror, &["commit", "-m", "local work"]);
        git(&self.mirror, &["rev-parse", "HEAD"])
    }

    fn remote_tip(&self) -> String {
        git(&self.bare, &["rev-parse", "refs/heads/main"])
    }

    async fn set_workspace_tip(&self, app: &TestApp, sha: &str) {
        seed_workspace_authority_head(app, self.workspace_id, self.repo_id, sha).await;
    }

    /// Pauses the branch (the scheduler must skip paused branches).
    async fn set_paused(&self, app: &TestApp, reason: &str) {
        sqlx::query("UPDATE remote_integrations SET sync_paused_reason = $3 WHERE workspace_id = $1 AND repo_id = $2")
            .bind(self.workspace_id).bind(self.repo_id)
            .bind(reason)
            .execute(&app.db)
            .await
            .expect("set paused");
    }

    /// Inserts a prior commit-coupled `push_failed` audit row (D62 precedent).
    async fn insert_push_failed(&self, app: &TestApp, acting_user_id: Uuid) {
        sqlx::query(
            r#"
            INSERT INTO remote_sync_runs (
                org_id, workspace_id, repo_id, acting_user_id,
                kind, trigger_kind, status, error_detail,
                started_at, completed_at
            )
            VALUES ($1, $2, $3, $4, 'integration', 'commit', 'push_failed', 'transient network failure', now(), now())
            "#,
        )
        .bind(self.org_id)
        .bind(self.workspace_id).bind(self.repo_id)
        .bind(acting_user_id)
        .execute(&app.db)
        .await
        .expect("insert push_failed precedent");
    }

    async fn enqueue(&self, app: &TestApp, trigger_kind: &str) {
        let mut db = app.scoped_db_for_org(self.org_id).await;
        enqueue_test_request(db.conn(), self.key(), trigger_kind).await;
        db.commit().await.unwrap();
    }

    /// Count of `background_inbound` queue rows for this branch.
    async fn background_queue_count(&self, app: &TestApp) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2 AND trigger_kind = 'background_inbound'",
        )
        .bind(self.workspace_id).bind(self.repo_id)
        .fetch_one(&app.db)
        .await
        .expect("background queue count")
    }

    fn key(&self) -> WorkspaceRepoKey {
        WorkspaceRepoKey {
            org_id: self.org_id,
            workspace_id: self.workspace_id,
            repo_id: self.repo_id,
        }
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.tmp_root);
        let _ = std::fs::remove_dir_all(&self.mirror);
    }
}

// ---------------------------------------------------------------------------
// Drivers
// ---------------------------------------------------------------------------

async fn spawn_worker_app() -> TestApp {
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    TestApp::spawn_with_config(config).await
}

/// Drives the REAL scheduler loop until at least `expect_total` background_inbound
/// queue rows exist across the given branches, then shuts it down.
async fn run_scheduler_until(app: &TestApp, members: &[WorkspaceRepoKey], expect_total: i64) {
    run_scheduler_with_policy(app, members, expect_total, true).await;
}

async fn run_scheduler_with_policy(
    app: &TestApp,
    members: &[WorkspaceRepoKey],
    expect_total: i64,
    passive_pull_enabled: bool,
) {
    use g5_api::maintenance::branch_integration_scheduler::run_branch_integration_scheduler;
    use g5_api::maintenance::TenantMaintenanceDeps;

    let deps = TenantMaintenanceDeps {
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
    };
    let cancel = CancellationToken::new();
    let handle = tokio::spawn(run_branch_integration_scheduler(
        deps,
        Duration::from_millis(200),
        passive_pull_enabled,
        cancel.clone(),
    ));

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM remote_integration_queue WHERE trigger_kind = 'background_inbound' AND (workspace_id, repo_id) IN (SELECT * FROM UNNEST($1::uuid[], $2::uuid[]))",
        )
        .bind(members.iter().map(|m| m.workspace_id).collect::<Vec<_>>()).bind(members.iter().map(|m| m.repo_id).collect::<Vec<_>>())
        .fetch_one(&app.db)
        .await
        .expect("count background rows");
        if n >= expect_total {
            break;
        }
        if Instant::now() > deadline {
            cancel.cancel();
            let _ = handle.await;
            panic!("scheduler enqueued only {n}/{expect_total} background_inbound rows within 45s");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    cancel.cancel();
    let _ = handle.await;
}

/// Drives the REAL worker loop until it drains the given queue row.
async fn drain_one_cycle(app: &TestApp, key: WorkspaceRepoKey) {
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

    let cancel = CancellationToken::new();
    let handle = tokio::spawn(run_branch_integration_worker(deps, cancel.clone()));

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
        )
        .bind(key.workspace_id).bind(key.repo_id)
        .fetch_one(&app.db)
        .await
        .expect("count queue rows");
        if remaining == 0 {
            break;
        }
        if Instant::now() > deadline {
            let diag: Vec<(String, Option<String>)> = sqlx::query_as(
                "SELECT status, error_detail FROM remote_sync_runs WHERE workspace_id = $1 AND repo_id = $2 ORDER BY started_at DESC",
            )
            .bind(key.workspace_id).bind(key.repo_id)
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

#[derive(Debug, sqlx::FromRow)]
struct AuditRow {
    status: String,
    trigger_kind: String,
    error_detail: Option<String>,
}

async fn latest_run_for_trigger(app: &TestApp, key: WorkspaceRepoKey, trigger: &str) -> AuditRow {
    sqlx::query_as::<_, AuditRow>(
        r#"
        SELECT status, trigger_kind, error_detail
        FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2 AND trigger_kind = $3
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(trigger)
    .fetch_one(&app.db)
    .await
    .expect("trigger audit row exists")
}

/// The latest `background_inbound` audit row for a branch.
async fn latest_background_run(app: &TestApp, key: WorkspaceRepoKey) -> AuditRow {
    latest_run_for_trigger(app, key, "background_inbound").await
}

async fn wbr_head(app: &TestApp, key: WorkspaceRepoKey) -> String {
    let branch: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(key.workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let path = app.repo_path(key.repo_id).await;
    git(&path, &["rev-parse", &format!("refs/heads/{branch}")])
}

async fn wbr_baseline(app: &TestApp, key: WorkspaceRepoKey) -> String {
    let branch: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(key.workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let path = app.repo_path(key.repo_id).await;
    GitRepo::open(&path)
        .unwrap()
        .read_sync_watermark(&branch)
        .unwrap()
        .unwrap()
}

// ---------------------------------------------------------------------------
// D57 — scheduler enqueues every UNPAUSED branch (clean + dirty), skips paused
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scheduler_enqueues_every_unpaused_branch_and_skips_paused() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // Three unpaused plus one paused remote-tracking branch (each its own
    // repository). N is representative of D57's N=5.
    let clean_a = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let clean_b = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let third = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let paused = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    paused.set_paused(&app, "auth_required").await;

    // Three unpaused branches must each get exactly one
    // background_inbound row; the paused branch gets none.
    let unpaused = [clean_a.key(), clean_b.key(), third.key()];
    run_scheduler_until(&app, &unpaused, 3).await;

    assert_eq!(
        clean_a.background_queue_count(&app).await,
        1,
        "clean branch A enqueued"
    );
    assert_eq!(
        clean_b.background_queue_count(&app).await,
        1,
        "clean branch B enqueued"
    );
    assert_eq!(
        third.background_queue_count(&app).await,
        1,
        "third unpaused branch enqueued"
    );
    assert_eq!(
        paused.background_queue_count(&app).await,
        0,
        "the PAUSED branch must NOT be enqueued"
    );

    clean_a.cleanup();
    clean_b.cleanup();
    third.cleanup();
    paused.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scheduler_enqueues_base_like_any_workspace() {
    // g5.workspace.remote.base-parity: Base rows enter the same per-repository
    // background queue as every other remote-tracked workspace.
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let base_workspace = TestWorkspace::create_base(&app, &org).await;
    let non_base_workspace = TestWorkspace::create(&app, &org).await;

    let base = TrackingFixture::setup(&app, &org, &base_workspace, user.id).await;
    let non_base = TrackingFixture::setup(&app, &org, &non_base_workspace, user.id).await;

    run_scheduler_until(&app, &[non_base.key(), base.key()], 2).await;

    assert_eq!(
        non_base.background_queue_count(&app).await,
        1,
        "non-Base remote-tracking branch keeps background enqueue behavior"
    );
    assert_eq!(
        base.background_queue_count(&app).await,
        1,
        "Base remote-tracking branch enters the ordinary per-repository worker queue"
    );

    base.cleanup();
    non_base.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D58 — background tick on clean branch with remote ahead → provider observation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_tick_fast_forwards_g5_when_remote_ahead_clean() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    let c2 = fx.advance_remote("remote_only.txt", "remote work\n");
    assert_ne!(c2, fx.c1);

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let run = latest_background_run(&app, fx.key()).await;
    assert_eq!(
        run.status, "imported",
        "remote-ahead auto-pull must fast-forward G5"
    );
    assert_eq!(run.trigger_kind, "background_inbound");
    assert_eq!(
        wbr_head(&app, fx.key()).await,
        c2,
        "auto-pull advances the published G5 branch"
    );
    assert_eq!(
        wbr_baseline(&app, fx.key()).await,
        c2,
        "auto-pull advances the convergence watermark"
    );

    assert_eq!(
        git(
            &fx.mirror,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::provider_ref(fx.workspace_id, "main")
            ]
        ),
        c2
    );
    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D59 — background tick on clean branch with workspace ahead → noop (no push)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_tick_noops_when_workspace_ahead_no_push() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Workspace is one commit ahead of the (still-C1) upstream.
    let c2 = fx.advance_mirror("local_only.txt", "local work\n");
    fx.set_workspace_tip(&app, &c2).await;
    assert_eq!(fx.remote_tip(), fx.c1, "precondition: upstream at C1");

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    // The background loop never pushes — push is reserved for commit-coupled /
    // operator triggers. Workspace-ahead is treated as noop.
    let run = latest_background_run(&app, fx.key()).await;
    assert_eq!(
        run.status, "noop",
        "background workspace-ahead must be noop, not pushed"
    );
    assert_ne!(run.status, "pushed");
    assert_ne!(run.status, "push_failed");
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "the background loop must NOT push — the customer remote ref stays at C1"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D60 — background tick on dirty branch → dirty_skipped + advisory refresh
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_loop_never_retries_failed_push() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // Workspace ahead of the upstream (the state left by a prior failed push),
    // plus a prior commit-coupled push_failed audit row.
    let c2 = fx.advance_mirror("local_only.txt", "local work\n");
    fx.set_workspace_tip(&app, &c2).await;
    fx.insert_push_failed(&app, user.id).await;
    assert_eq!(fx.remote_tip(), fx.c1, "precondition: upstream still at C1");

    // A background tick MUST NOT retry the push — it classifies workspace-ahead
    // as noop and leaves the remote untouched.
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let run = latest_background_run(&app, fx.key()).await;
    assert_eq!(
        run.status, "noop",
        "the background loop does not retry a failed push"
    );
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "no background push retry — the remote ref is unchanged"
    );

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// #1253 regression — the background scheduler must NOT block on (or deadlock
// with) a `workspace_branch_repos` row a concurrent commit holds FOR UPDATE.
// ---------------------------------------------------------------------------
//
// Root cause (GitHub issue #1253): the commit path locks the wbr row FOR UPDATE
// while applying a draft commit (`workspace_ops::advance_workspace_branch_*`,
// e.g. `SELECT ... FROM workspace_branch_repos ... FOR UPDATE`) and then UPSERTs
// its commit-coupled `branch_integration_queue` row. The pre-fix scheduler ran
// an unordered bulk `INSERT ... SELECT ... ON CONFLICT DO NOTHING`; the INSERT's
// foreign-key key-share on that same wbr row waited on the commit's FOR UPDATE,
// while the commit's own queue UPSERT waited on the scheduler's uncommitted
// queue-row insert — a lock inversion PostgreSQL aborts with SQLSTATE 40P01
// (observed 15x on dev in July 2026). The fix makes the scheduler's candidate
// SELECT take `FOR KEY SHARE OF wbr SKIP LOCKED`, so a wbr row a commit is
// holding is skipped this sweep (retried on a later tick) and the sweep never
// blocks on — nor deadlocks with — a user commit.
//
// Deterministic discriminator: X (commit-locked) and a free sibling Y live in
// the SAME org, so one per-org `INSERT ... SELECT` covers both.
//   * Pre-fix: the statement blocks on X's FK lock, so the whole per-org sweep
//     stalls and NEITHER X nor Y is enqueued while X is held → the poll for Y
//     times out (RED).
//   * Fixed: X is skipped, Y is enqueued → the poll succeeds (GREEN); X gets no
//     background_inbound row; and the commit's own `commit` UPSERT on X then
//     commits with no 40P01.
//
// SOT:
// - `branch_integration_scheduler.semlift.md` (nonblocking scheduler try/skip)
// - `branch_integration_queue.semlift.md` (single-row precedence UPSERT)
// - GitHub issue #1253
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scheduler_skips_commit_locked_branch_without_deadlock() {
    use g5_api::models::branch_integration_queue::{enqueue_branch_integration, trigger_kind};

    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // X = the branch a concurrent commit is applying (wbr row held FOR UPDATE);
    // Y = an unrelated sibling remote-tracking branch in the SAME org/workspace
    // (its own repo → UNIQUE(workspace_id, repo_id)).
    let locked = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let sibling = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // --- Step 1: a concurrent commit holds wbr[X] FOR UPDATE, exactly as
    // `workspace_ops::advance_workspace_branch_*` does while applying the commit.
    // Hold it on a dedicated transaction for the whole observation window.
    let mut commit_tx = app.db.begin().await.expect("begin commit txn");
    let held: Uuid =
        sqlx::query_scalar("SELECT repo_id FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2 FOR UPDATE")
            .bind(locked.workspace_id).bind(locked.repo_id)
            .fetch_one(&mut *commit_tx)
            .await
            .expect("commit holds wbr[X] FOR UPDATE");
    assert_eq!(held, locked.repo_id);

    // --- Step 2: drive the REAL scheduler while X is held. The fixed scheduler
    // skips X (FOR KEY SHARE ... SKIP LOCKED) and still enqueues the free
    // sibling Y within the window. The pre-fix scheduler blocks the per-org
    // INSERT...SELECT on X's FK lock, so Y is never enqueued and this call
    // panics at its 45s deadline — the regression proof.
    run_scheduler_until(&app, &[sibling.key()], 1).await;

    assert_eq!(
        sibling.background_queue_count(&app).await,
        1,
        "the sibling (unlocked) branch must be enqueued — the scheduler sweep completed \
         without blocking on the commit-locked branch (#1253 nonblocking try/skip)"
    );
    assert_eq!(
        locked.background_queue_count(&app).await,
        0,
        "the commit-locked branch must be SKIPPED this sweep (FOR KEY SHARE ... SKIP LOCKED), \
         not block the sweep — it is retried on a later tick"
    );

    // --- Step 3: the commit finishes its own commit-coupled enqueue on X and
    // commits. This is the UPSERT PostgreSQL aborted with 40P01 in the deadlock;
    // it must now succeed against the concurrently-running scheduler.
    enqueue_branch_integration(&mut commit_tx, locked.key(), trigger_kind::COMMIT)
        .await
        .expect("commit-coupled enqueue must not deadlock (40P01) with the scheduler");
    commit_tx.commit().await.expect("commit txn");

    // --- Step 4: the commit trigger won X's row; X carries exactly one 'commit'
    // row and was never demoted to background_inbound.
    let x_rows: Vec<String> = sqlx::query_scalar(
        "SELECT trigger_kind FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(locked.workspace_id).bind(locked.repo_id)
    .fetch_all(&app.db)
    .await
    .expect("read X queue rows");
    assert_eq!(
        x_rows,
        vec![trigger_kind::COMMIT.to_string()],
        "the commit-locked branch ends with exactly one 'commit' queue row"
    );

    locked.cleanup();
    sibling.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Issues #1798/#2054 adjacent guard — narrowly subsumed from Ben PR #2137.
// Automatic auto-push divergence pauses without opening reconciliation, while
// explicit intent must be able to resume a pause-only row into exactly one
// existing remote-tracking merge workflow.
// ---------------------------------------------------------------------------

/// Create two distinct children of the C1 baseline: the workspace tip C2a and
/// the customer remote tip C2b. The worker must classify this as `Diverged`,
/// not `WatermarkViolation`.
async fn seed_rebase_divergence(app: &TestApp, fx: &TrackingFixture) -> (String, String) {
    let c2a = fx.advance_mirror("local_rebased.txt", "rebased local work\n");
    fx.set_workspace_tip(app, &c2a).await;
    let c2b = fx.advance_remote("remote_only.txt", "remote work\n");
    assert_ne!(c2a, c2b, "precondition: divergent tips are distinct");
    assert_ne!(c2a, fx.c1);
    assert_ne!(c2b, fx.c1);
    (c2a, c2b)
}

async fn sync_paused_reason(app: &TestApp, key: WorkspaceRepoKey) -> Option<String> {
    sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2")
        .bind(key.workspace_id).bind(key.repo_id)
        .fetch_one(&app.db)
        .await
        .expect("read sync_paused_reason")
}

async fn remote_tracking_merge_run_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM workspace_merge_runs \
         WHERE workspace_id = $1 AND repo_id = $2 AND source_kind = 'remote_tracking'",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("read remote-tracking merge-run count")
}

async fn merge_active_audit_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_sync_runs \
         WHERE workspace_id = $1 AND repo_id = $2 AND error_detail = 'merge_active'",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("read merge-active audit count")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_tick_diverged_auto_push_pauses_without_opening_merge_run() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("enable auto-push");
    let (workspace_tip, remote_tip) = seed_rebase_divergence(&app, &fx).await;

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    let run = latest_background_run(&app, fx.key()).await;
    assert_eq!(run.status, "diverged");
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        Some("diverged".to_string())
    );
    assert_eq!(
        remote_tracking_merge_run_count(&app, fx.key()).await,
        0,
        "automatic auto-push divergence must pause without opening reconciliation"
    );
    assert_eq!(
        wbr_head(&app, fx.key()).await,
        workspace_tip,
        "pause-only preserves the workspace head"
    );
    assert_eq!(
        fx.remote_tip(),
        remote_tip,
        "pause-only preserves the customer remote head"
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_tick_diverged_manual_mode_opens_merge_run() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let (_workspace_tip, _remote_tip) = seed_rebase_divergence(&app, &fx).await;

    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        Some("diverged".to_string())
    );
    assert_eq!(
        remote_tracking_merge_run_count(&app, fx.key()).await,
        1,
        "manual-mode divergence retains the operator reconciliation workflow"
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_push_resumes_pause_only_divergence_with_one_merge_run() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("enable auto-push");
    let (_workspace_tip, _remote_tip) = seed_rebase_divergence(&app, &fx).await;
    fx.set_paused(&app, "diverged").await;
    assert_eq!(
        remote_tracking_merge_run_count(&app, fx.key()).await,
        0,
        "precondition: pause-only state has no active reconciliation run"
    );

    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        remote_tracking_merge_run_count(&app, fx.key()).await,
        1,
        "explicit push must resume a pause-only divergence into exactly one reconciliation run"
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_push_resumes_pause_only_into_noop_convergence() {
    assert_explicit_trigger_resumes_pause_only_into_noop_convergence("push").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_manual_resumes_pause_only_into_noop_convergence() {
    assert_explicit_trigger_resumes_pause_only_into_noop_convergence("manual").await;
}

async fn assert_explicit_trigger_resumes_pause_only_into_noop_convergence(trigger_kind: &str) {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let (workspace_tip, _remote_tip) = seed_rebase_divergence(&app, &fx).await;
    fx.set_paused(&app, "diverged").await;
    let workspace_refspec = format!("{workspace_tip}:refs/heads/main");
    git(
        &fx.mirror,
        &["push", "--force", "origin", &workspace_refspec],
    );

    fx.enqueue(&app, trigger_kind).await;
    drain_one_cycle(&app, fx.key()).await;

    let run = latest_run_for_trigger(&app, fx.key(), trigger_kind).await;
    assert_eq!(run.status, "noop");
    assert_eq!(run.trigger_kind, trigger_kind);
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        None,
        "current-tip noop convergence clears only the stale divergence pause"
    );
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 0);
    assert_eq!(wbr_head(&app, fx.key()).await, workspace_tip);
    assert_eq!(fx.remote_tip(), workspace_tip);

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_push_resumes_pause_only_into_pushed_convergence() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let (workspace_tip, _remote_tip) = seed_rebase_divergence(&app, &fx).await;
    fx.set_paused(&app, "diverged").await;
    git(&fx.bare, &["update-ref", "refs/heads/main", &fx.c1]);

    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;

    let run = latest_run_for_trigger(&app, fx.key(), "push").await;
    assert_eq!(run.status, "pushed");
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        None,
        "successful push convergence clears only the stale divergence pause"
    );
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 0);
    assert_eq!(wbr_head(&app, fx.key()).await, workspace_tip);
    assert_eq!(fx.remote_tip(), workspace_tip);

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_push_failed_fetch_keeps_divergence_pause() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("enable auto-push");
    let (workspace_tip, remote_tip) = seed_rebase_divergence(&app, &fx).await;
    fx.set_paused(&app, "diverged").await;
    // Error messages include this path. Authentication-looking path text must
    // not turn a missing repository into an authentication failure.
    let unavailable_remote = fx.tmp_root.join("unavailable-401-403-authentication.git");
    sqlx::query("UPDATE repos SET git_url = $2 WHERE id = $1")
        .bind(fx.repo_id)
        .bind(unavailable_remote.to_str().expect("UTF-8 fixture path"))
        .execute(&app.db)
        .await
        .expect("point tracked repo at an unavailable remote");

    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;

    let run = latest_run_for_trigger(&app, fx.key(), "push").await;
    assert_eq!(run.status, "fetch_failed");
    assert!(run.error_detail.is_some(), "failed fetch records its cause");
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        Some("diverged".to_string()),
        "failed convergence must retain the divergence pause"
    );
    assert_eq!(
        remote_tracking_merge_run_count(&app, fx.key()).await,
        0,
        "a failed safe-convergence push must not open reconciliation"
    );
    assert_eq!(
        wbr_head(&app, fx.key()).await,
        workspace_tip,
        "failed convergence preserves the workspace head"
    );
    assert_eq!(
        fx.remote_tip(),
        remote_tip,
        "the unavailable remote preserves its captured head"
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_manual_workspace_ahead_keeps_divergence_pause() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    let (workspace_tip, _remote_tip) = seed_rebase_divergence(&app, &fx).await;
    fx.set_paused(&app, "diverged").await;
    git(&fx.bare, &["update-ref", "refs/heads/main", &fx.c1]);

    fx.enqueue(&app, "manual").await;
    drain_one_cycle(&app, fx.key()).await;

    let run = latest_run_for_trigger(&app, fx.key(), "manual").await;
    assert_eq!(run.status, "noop");
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        Some("diverged".to_string()),
        "a non-converged manual noop must not clear the divergence pause"
    );
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 0);
    assert_eq!(wbr_head(&app, fx.key()).await, workspace_tip);
    assert_eq!(fx.remote_tip(), fx.c1);

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_repeat_on_pause_only_divergence_does_not_write_merge_active_audit() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    sqlx::query("UPDATE workspaces SET push_mode = 'auto' WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("enable auto-push");
    seed_rebase_divergence(&app, &fx).await;
    fx.enqueue(&app, "background_inbound").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        Some("diverged".to_string())
    );

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    assert_eq!(
        merge_active_audit_count(&app, fx.key()).await,
        0,
        "pause-only repeats must not claim a merge run is active"
    );
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 0);
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        Some("diverged".to_string())
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_push_fast_forward_clears_pause_only_divergence() {
    assert_explicit_trigger_fast_forward_clears_pause_only_divergence("push").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_manual_fast_forward_clears_pause_only_divergence() {
    assert_explicit_trigger_fast_forward_clears_pause_only_divergence("manual").await;
}

async fn assert_explicit_trigger_fast_forward_clears_pause_only_divergence(trigger_kind: &str) {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    let (_workspace_tip, remote_tip) = seed_rebase_divergence(&app, &fx).await;
    fx.set_paused(&app, "diverged").await;
    fx.set_workspace_tip(&app, &fx.c1).await;
    git(&fx.mirror, &["checkout", &fx.workspace_branch]);
    git(&fx.mirror, &["reset", "--hard", &fx.c1]);

    fx.enqueue(&app, trigger_kind).await;
    drain_one_cycle(&app, fx.key()).await;

    let imports: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_sync_runs WHERE workspace_id=$1 AND repo_id=$2 AND trigger_kind=$3 AND status='imported' AND old_workspace_tip=$4 AND new_workspace_tip=$5 AND new_mirror_tip=$5")
        .bind(fx.workspace_id).bind(fx.repo_id).bind(trigger_kind).bind(&fx.c1).bind(&remote_tip)
        .fetch_one(&app.db).await.unwrap();
    assert_eq!(
        imports, 1,
        "the fast-forward is recorded with the exact old and new heads"
    );
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await,
        None,
        "safe convergence clears the pause-only divergence"
    );
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 0);
    assert_eq!(wbr_head(&app, fx.key()).await, remote_tip);
    assert_eq!(fx.remote_tip(), remote_tip);

    fx.cleanup();
    app.cleanup().await;
}

/// Production smart HTTP -> implicit queue -> real provider transport. No
/// direct enqueue in the test; auto-pull is disabled throughout.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_push_releases_admission_before_waiting_for_integration_queue() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let checkout = fx.tmp_root.join("agent");
    let header = format!("http.extraHeader=Authorization: Bearer {}", user.jwt);
    let cloned = tokio::process::Command::new("git")
        .args(["-c", &header, "clone", "--branch", &workspace.name])
        .arg(format!("{}/git/{}", app.base_url, fx.repo_id))
        .arg(&checkout)
        .output()
        .await
        .unwrap();
    assert!(
        cloned.status.success(),
        "{}",
        String::from_utf8_lossy(&cloned.stderr)
    );
    git(&checkout, &["config", "user.name", "Agent"]);
    git(&checkout, &["config", "user.email", "agent@test.local"]);
    git(
        &checkout,
        &["commit", "--allow-empty", "-qm", "concurrent push"],
    );
    let head = git(&checkout, &["rev-parse", "HEAD"]);

    // Pause at the worker's finalization boundary: it owns the queue row
    // before loading and locking the current integration/workspace policy.
    fx.enqueue(&app, "background_inbound").await;
    let mut finalization = app.scoped_db_for_org(org.id).await;
    let finalizer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(finalization.conn())
        .await
        .unwrap();
    sqlx::query("SELECT repo_id FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2 FOR UPDATE")
        .bind(workspace.id).bind(fx.repo_id)
        .fetch_one(finalization.conn()).await.unwrap();
    let push = tokio::process::Command::new("git")
        .current_dir(&checkout)
        .args(["-c", &header, "push"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    // Observe the actual queue wait rather than relying on timing. At this
    // point receive-pack has accepted the Git write and is enqueueing it.
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                 WHERE $1 = ANY(pg_blocking_pids(pid))
                   AND query LIKE '%INSERT INTO remote_integration_queue%')",
            )
            .bind(finalizer_pid)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("native push never reached the locked integration queue");
    assert_eq!(
        git(
            &fx.mirror,
            &["rev-parse", &format!("refs/heads/{}", workspace.name)]
        ),
        head
    );

    // This is the production policy lock used by worker finalization. Before
    // the fix it times out: admission holds workspace SHARE, awaiting this
    // transaction's queue row through a separate enqueue transaction.
    sqlx::query("SET LOCAL statement_timeout = '2s'")
        .execute(finalization.conn())
        .await
        .unwrap();
    let policy =
        g5_api::services::remote_integrations::load(finalization.conn(), fx.key(), true).await;
    finalization.rollback().await.unwrap();
    let pushed = tokio::time::timeout(Duration::from_secs(15), push.wait_with_output())
        .await
        .expect("native push remained stuck after releasing the queue")
        .unwrap();
    assert!(
        pushed.status.success(),
        "{}",
        String::from_utf8_lossy(&pushed.stderr)
    );

    // Automatic publication refuses this non-App remote, while an explicit
    // request can publish the accepted Git commit once contention ends.
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), fx.c1);
    assert_eq!(
        latest_run_for_trigger(&app, fx.key(), "commit")
            .await
            .status,
        "push_failed"
    );
    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), head);
    fx.cleanup();
    app.cleanup().await;
    assert!(
        matches!(policy, Ok(Some(_))),
        "worker policy lock was blocked by push admission: {policy:?}"
    );
}

/// An agent checkout of the workspace branch through the production Git
/// receiver, as a running VM would publish it.
fn agent_checkout(
    app: &TestApp,
    fx: &TrackingFixture,
    user: &TestUser,
    name: &str,
) -> (PathBuf, String) {
    let checkout = fx.tmp_root.join(name);
    let header = format!("http.extraHeader=Authorization: Bearer {}", user.jwt);
    git(
        &fx.tmp_root,
        &[
            "-c",
            &header,
            "clone",
            "--branch",
            &fx.workspace_branch,
            &format!("{}/git/{}", app.base_url, fx.repo_id),
            checkout.to_str().unwrap(),
        ],
    );
    git(&checkout, &["config", "user.name", "Agent"]);
    git(&checkout, &["config", "user.email", "agent@test.local"]);
    (checkout, header)
}

/// Publish the checkout's HEAD with an explicit lease on the G5 head the
/// agent rewrote, the way native agent rebases publish.
fn leased_push(checkout: &Path, header: &str, branch: &str, expected: &str) -> String {
    let lease = format!("--force-with-lease=refs/heads/{branch}:{expected}");
    git(
        checkout,
        &[
            "-c",
            header,
            "push",
            &lease,
            "origin",
            &format!("HEAD:refs/heads/{branch}"),
        ],
    );
    git(checkout, &["rev-parse", "HEAD"])
}

fn amend(checkout: &Path, contents: &str) {
    write_file(checkout, "published.txt", contents);
    git(checkout, &["add", "published.txt"]);
    git(checkout, &["commit", "--amend", "--no-edit"]);
}

fn watermark(fx: &TrackingFixture) -> Option<String> {
    GitRepo::open(&fx.mirror)
        .unwrap()
        .read_sync_watermark(&fx.workspace_branch)
        .unwrap()
}

async fn published_fixture(
    app: &TestApp,
    push_mode: &str,
) -> (TestUser, TestWorkspace, TrackingFixture, String) {
    published_fixture_with(app, push_mode, None).await
}

/// [`published_fixture`], optionally behind a mocked organization App so
/// automatic publication runs the production App credential path.
async fn published_fixture_with(
    app: &TestApp,
    push_mode: &str,
    github: Option<&helpers::github_app::MockedGithubApp>,
) -> (TestUser, TestWorkspace, TrackingFixture, String) {
    let mut user = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut user).await;
    let workspace = TestWorkspace::create(app, &org).await;
    let fx = TrackingFixture::setup(app, &org, &workspace, user.id).await;
    if let Some(github) = github {
        github
            .bind(&app.db, org.id, user.id, fx.repo_id, &fx.bare)
            .await;
    }
    let published = fx.advance_mirror("published.txt", "original published commit\n");
    fx.enqueue(app, "push").await;
    drain_one_cycle(app, fx.key()).await;
    assert_eq!(fx.remote_tip(), published);
    assert_eq!(watermark(&fx).as_deref(), Some(published.as_str()));
    sqlx::query("UPDATE workspaces SET push_mode=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(push_mode)
        .execute(&app.db)
        .await
        .unwrap();
    (user, workspace, fx, published)
}

// `g5.governance.remote.workspace-rewrite`: an accepted workspace rewrite
// against the unchanged provider is published automatically, with the
// provider tip as its lease, under the organization App's credentials,
// including a rewind the provider contains.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_workspace_rewrite_auto_publishes_under_the_app_with_the_provider_lease() {
    let github = helpers::github_app::MockedGithubApp::start().await;
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    github.configure(&mut config);
    let app = TestApp::spawn_with_config(config).await;
    let (user, workspace, fx, published) =
        published_fixture_with(&app, "auto", Some(&github)).await;
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let (checkout, header) = agent_checkout(&app, &fx, &user, "rewritten-agent");
    amend(&checkout, "rewritten commit\n");
    let rewritten = leased_push(&checkout, &header, &fx.workspace_branch, &published);
    assert_ne!(rewritten, published);
    assert_eq!(wbr_head(&app, fx.key()).await, rewritten);

    // The native push queues automatic publication of the rewrite, not an
    // approval checkpoint, and the worker publishes it without an explicit push.
    drain_one_cycle(&app, fx.key()).await;
    let run = latest_run_for_trigger(&app, fx.key(), "commit").await;
    assert_eq!(run.status, "pushed", "{:?}", run.error_detail);
    assert_eq!(fx.remote_tip(), rewritten);
    assert_eq!(watermark(&fx).as_deref(), Some(rewritten.as_str()));
    assert_eq!(sync_paused_reason(&app, fx.key()).await, None);
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 0);
    // Only the App path admits automatic publication; its installation token
    // with write access (cached across cycles) carried the push.
    let minted = github.minted_permissions().await;
    assert!(
        minted
            .iter()
            .any(|permissions| permissions["contents"] == "write"),
        "{minted:?}"
    );

    // A rewind that the provider still contains is a rewrite too: the
    // provider tip is not imported back over it.
    git(&checkout, &["reset", "--hard", &fx.c1]);
    let rewound = leased_push(&checkout, &header, &fx.workspace_branch, &rewritten);
    assert_eq!(rewound, fx.c1);
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        latest_run_for_trigger(&app, fx.key(), "commit")
            .await
            .status,
        "pushed"
    );
    assert_eq!(wbr_head(&app, fx.key()).await, fx.c1);
    assert_eq!(fx.remote_tip(), fx.c1);
    assert_eq!(watermark(&fx).as_deref(), Some(fx.c1.as_str()));
    assert_eq!(sync_paused_reason(&app, fx.key()).await, None);
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 0);
    fx.cleanup();
    app.cleanup().await;
}

// `g5.git.workflow-files` (#612): automatic forwarding of a workflow-file
// change under an App installation without Workflows records an actionable
// push failure and leaves the provider unchanged, while the ordinary commit
// before it forwarded with contents write alone. Once an administrator
// refreshes the binding after the grant, the next cycle publishes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_forwarding_publishes_workflow_changes_only_under_a_recorded_grant() {
    let github = helpers::github_app::MockedGithubApp::start().await;
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    github.configure(&mut config);
    let app = TestApp::spawn_with_config(config).await;
    let (user, workspace, fx, published) =
        published_fixture_with(&app, "auto", Some(&github)).await;
    github.enforce_workflow_permission(&fx.bare);
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let (checkout, header) = agent_checkout(&app, &fx, &user, "workflow-agent");
    std::fs::create_dir_all(checkout.join(".github/workflows")).unwrap();
    write_file(&checkout, ".github/workflows/ci.yml", "on: push\n");
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "change CI"]);
    let workflow = leased_push(&checkout, &header, &fx.workspace_branch, &published);
    assert_eq!(wbr_head(&app, fx.key()).await, workflow);

    drain_one_cycle(&app, fx.key()).await;
    let run = latest_run_for_trigger(&app, fx.key(), "commit").await;
    assert_eq!(run.status, "push_failed", "{:?}", run.error_detail);
    let detail = run.error_detail.unwrap_or_default();
    for expected in [
        ".github/workflows/ci.yml",
        "Workflows",
        "Nothing was published",
        "Refresh from GitHub",
    ] {
        assert!(detail.contains(expected), "{detail}");
    }
    assert_eq!(fx.remote_tip(), published);
    assert!(
        github
            .minted_permissions()
            .await
            .iter()
            .all(|permissions| permissions.get("workflows").is_none()),
        "no workflow grant is recorded yet"
    );

    github.grant_on_github(serde_json::json!({
        "metadata":"read","contents":"write","pull_requests":"write","workflows":"write"
    }));
    let response = app
        .authenticated_client(&user)
        .post_json(
            &format!("/api/v1/orgs/{}/github-app/binding/refresh", fx.org_id),
            &serde_json::json!({"bindingVersion":1}),
        )
        .await;
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(body["data"]["binding"]["permissions"]["workflows"], "write");

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;
    let run = latest_run_for_trigger(&app, fx.key(), "commit").await;
    assert_eq!(run.status, "pushed", "{:?}", run.error_detail);
    assert_eq!(fx.remote_tip(), workflow);
    assert_eq!(watermark(&fx).as_deref(), Some(workflow.as_str()));
    assert!(
        github
            .minted_permissions()
            .await
            .contains(&serde_json::json!({"contents":"write","workflows":"write"})),
        "the recorded grant mints a token carrying it"
    );
    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manual_mode_keeps_a_workspace_rewrite_pending_until_explicit_push() {
    let app = spawn_worker_app().await;
    let (user, workspace, fx, published) = published_fixture(&app, "manual").await;
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let (checkout, header) = agent_checkout(&app, &fx, &user, "manual-agent");
    git(&checkout, &["reset", "--hard", &fx.c1]);
    let rewound = leased_push(&checkout, &header, &fx.workspace_branch, &published);

    // Background and commit cycles neither publish, import the old provider
    // tip back, nor move the watermark.
    for trigger in ["commit", "background_inbound", "manual"] {
        fx.enqueue(&app, trigger).await;
        drain_one_cycle(&app, fx.key()).await;
        assert_eq!(fx.remote_tip(), published, "{trigger}");
        assert_eq!(wbr_head(&app, fx.key()).await, rewound, "{trigger}");
        assert_eq!(
            watermark(&fx).as_deref(),
            Some(published.as_str()),
            "{trigger}"
        );
        assert_eq!(sync_paused_reason(&app, fx.key()).await, None, "{trigger}");
    }
    // Tracking status names the pending rewrite until it is published.
    let rewrite_pending = || async {
        let body: serde_json::Value = app
            .authenticated_client(&user)
            .get(&format!(
                "/api/v1/workspaces/{}/repos/{}/sync_runs",
                fx.workspace_id, fx.repo_id
            ))
            .await
            .json()
            .await
            .unwrap();
        body["data"]["branch"]["repos"][0]["rewritePending"].clone()
    };
    assert_eq!(rewrite_pending().await, serde_json::json!(true));
    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), rewound);
    assert_eq!(watermark(&fx).as_deref(), Some(rewound.as_str()));
    assert_eq!(rewrite_pending().await, serde_json::json!(false));
    fx.cleanup();
    app.cleanup().await;
}

// A workspace rewrite never overwrites a provider that advanced normally;
// a provider rewrite keeps its approval checkpoint.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workspace_rewrite_never_overwrites_independent_provider_changes() {
    let app = spawn_worker_app().await;
    let (user, workspace, fx, published) = published_fixture(&app, "auto").await;
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let (checkout, header) = agent_checkout(&app, &fx, &user, "racing-agent");
    git(&fx.seed, &["fetch", "origin"]);
    git(&fx.seed, &["reset", "--hard", "origin/main"]);
    let advanced = fx.advance_remote("provider.txt", "provider advance\n");
    amend(&checkout, "rewritten commit\n");
    let rewritten = leased_push(&checkout, &header, &fx.workspace_branch, &published);

    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), advanced);
    assert_eq!(wbr_head(&app, fx.key()).await, rewritten);
    assert_eq!(
        sync_paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged")
    );

    // Rewrite the provider instead: its approval checkpoint remains.
    let other = &app;
    let (other_user, other_workspace, other_fx, other_published) =
        published_fixture(other, "auto").await;
    let _other_runtime =
        helpers::TestWorkspaceRuntimePeer::attach(other, &other_workspace, other_user.id).await;
    git(&other_fx.seed, &["fetch", "origin"]);
    git(&other_fx.seed, &["reset", "--hard", &other_fx.c1]);
    write_file(&other_fx.seed, "provider.txt", "provider rewrite\n");
    git(&other_fx.seed, &["add", "."]);
    git(&other_fx.seed, &["commit", "-m", "provider rewrite"]);
    git(&other_fx.seed, &["push", "--force", "origin", "main"]);
    let provider_rewrite = other_fx.remote_tip();
    other_fx.enqueue(other, "push").await;
    drain_one_cycle(other, other_fx.key()).await;
    assert_eq!(other_fx.remote_tip(), provider_rewrite);
    assert_eq!(wbr_head(other, other_fx.key()).await, other_published);
    assert_eq!(
        sync_paused_reason(other, other_fx.key()).await.as_deref(),
        Some("watermark_violation")
    );
    // Rechecks by the sweep keep the genuine checkpoint.
    other_fx.enqueue(other, "background_inbound").await;
    drain_one_cycle(other, other_fx.key()).await;
    assert_eq!(
        sync_paused_reason(other, other_fx.key()).await.as_deref(),
        Some("watermark_violation")
    );
    assert_eq!(
        remote_tracking_merge_run_count(other, other_fx.key()).await,
        1
    );
    assert_eq!(other_fx.remote_tip(), provider_rewrite);
    other_fx.cleanup();
    fx.cleanup();
    app.cleanup().await;
}

// A checkpoint opened for a workspace rewrite under the earlier rule is
// rechecked by the maintenance sweep and closed; manual mode still does not
// publish implicitly.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sweep_closes_a_false_provider_rewrite_checkpoint() {
    let app = spawn_worker_app().await;
    let (user, workspace, fx, published) = published_fixture(&app, "manual").await;
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let (checkout, header) = agent_checkout(&app, &fx, &user, "old-checkpoint-agent");
    amend(&checkout, "rewritten commit\n");
    let rewritten = leased_push(&checkout, &header, &fx.workspace_branch, &published);
    sqlx::query("DELETE FROM remote_integration_queue WHERE workspace_id=$1")
        .bind(fx.workspace_id)
        .execute(&app.db)
        .await
        .unwrap();
    // The checkpoint the earlier classification opened.
    fx.set_paused(&app, "watermark_violation").await;
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_merge_runs (org_id, workspace_id, repo_id, source_kind, source_branch_name, status, phase,
             source_heads, target_heads, pre_start_source_heads)
         VALUES ($5, $1, $2, 'remote_tracking', 'main', 'waiting_for_human', 'reset_below_baseline',
             jsonb_build_object('workspace_tip', $3::text, 'mirror_tip', $4::text),
             jsonb_build_object('baseline', $4::text),
             jsonb_build_object('workspace_tip', $3::text, 'mirror_tip', $4::text))
         RETURNING id",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .bind(&rewritten)
    .bind(&published)
    .bind(fx.org_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    run_scheduler_until(&app, &[fx.key()], 1).await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(sync_paused_reason(&app, fx.key()).await, None);
    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, blocking_reason FROM workspace_merge_runs WHERE id=$1")
            .bind(run_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "cancelled");
    assert_eq!(reason.as_deref(), Some("reclassified_workspace_rewrite"));
    assert_eq!(fx.remote_tip(), published);
    assert_eq!(wbr_head(&app, fx.key()).await, rewritten);
    assert_eq!(watermark(&fx).as_deref(), Some(published.as_str()));

    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), rewritten);
    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_push_requires_app_for_automatic_publication_and_keeps_explicit_push() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;
    sqlx::query("UPDATE remote_integrations SET auto_pull_enabled=false WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let _runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &workspace, user.id).await;
    let checkout = fx.tmp_root.join("agent");
    let header = format!("http.extraHeader=Authorization: Bearer {}", user.jwt);
    let cloned = tokio::process::Command::new("git")
        .args(["-c", &header, "clone", "--branch", &workspace.name])
        .arg(format!("{}/git/{}", app.base_url, fx.repo_id))
        .arg(&checkout)
        .output()
        .await
        .unwrap();
    assert!(
        cloned.status.success(),
        "{}",
        String::from_utf8_lossy(&cloned.stderr)
    );
    git(&checkout, &["config", "user.name", "Agent"]);
    git(&checkout, &["config", "user.email", "agent@test.local"]);

    for (mode, expected_status) in [("auto", "push_failed"), ("manual", "noop")] {
        sqlx::query("UPDATE workspaces SET push_mode=$2 WHERE id=$1")
            .bind(workspace.id)
            .bind(mode)
            .execute(&app.db)
            .await
            .unwrap();
        let before = fx.remote_tip();
        write_file(&checkout, mode, mode);
        git(&checkout, &["add", mode]);
        git(&checkout, &["commit", "-qm", mode]);
        let head = git(&checkout, &["rev-parse", "HEAD"]);
        let pushed = tokio::process::Command::new("git")
            .current_dir(&checkout)
            .args(["-c", &header, "push"])
            .output()
            .await
            .unwrap();
        assert!(
            pushed.status.success(),
            "{}",
            String::from_utf8_lossy(&pushed.stderr)
        );
        let signal: String = sqlx::query_scalar("SELECT trigger_kind FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2")
            .bind(workspace.id).bind(fx.repo_id).fetch_one(&app.db).await.unwrap();
        assert_eq!(
            signal, "commit",
            "native push must preserve policy, not override it"
        );
        drain_one_cycle(&app, fx.key()).await;
        assert_eq!(
            latest_run_for_trigger(&app, fx.key(), "commit")
                .await
                .status,
            expected_status
        );
        assert_eq!(fx.remote_tip(), before);
        assert_eq!(wbr_head(&app, fx.key()).await, head);
    }
    // A missed enqueue is repaired by the outbound sweep, even when both the
    // per-integration auto-pull toggle and global passive-pull switch are off.
    sqlx::query("UPDATE workspaces SET push_mode='auto' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    run_scheduler_with_policy(&app, &[fx.key()], 1, false).await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        fx.remote_tip(),
        fx.c1,
        "scheduler cannot bypass App-only publication"
    );
    fx.enqueue(&app, "push").await;
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(fx.remote_tip(), git(&checkout, &["rev-parse", "HEAD"]));

    git(&fx.bare, &["update-ref", "-d", "refs/heads/main"]);
    git(
        &checkout,
        &["commit", "--allow-empty", "-qm", "provider deleted"],
    );
    let pushed = tokio::process::Command::new("git")
        .current_dir(&checkout)
        .args(["-c", &header, "push"])
        .output()
        .await
        .unwrap();
    assert!(
        pushed.status.success(),
        "{}",
        String::from_utf8_lossy(&pushed.stderr)
    );
    drain_one_cycle(&app, fx.key()).await;
    assert_eq!(
        latest_run_for_trigger(&app, fx.key(), "commit")
            .await
            .status,
        "fetch_failed"
    );
    assert!(git2::Repository::open_bare(&fx.bare)
        .unwrap()
        .find_reference("refs/heads/main")
        .is_err());
    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// g5.governance.remote.sync-history — bounded attempt history (issue #565)
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct HistoryRow {
    trigger_kind: String,
    status: String,
    error_detail: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The membership's integration runs, newest first.
async fn integration_history(app: &TestApp, key: WorkspaceRepoKey) -> Vec<HistoryRow> {
    sqlx::query_as(
        "SELECT trigger_kind, status, error_detail, started_at, completed_at FROM remote_sync_runs
         WHERE workspace_id = $1 AND repo_id = $2 AND kind = 'integration'
         ORDER BY started_at DESC, id DESC",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_all(&app.db)
    .await
    .expect("read integration history")
}

async fn background_cycle(app: &TestApp, fx: &TrackingFixture) {
    fx.enqueue(app, "background_inbound").await;
    drain_one_cycle(app, fx.key()).await;
}

async fn set_remote_branch(app: &TestApp, fx: &TrackingFixture, branch: &str) {
    sqlx::query(
        "UPDATE remote_integrations SET remote_branch = $3 WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .bind(branch)
    .execute(&app.db)
    .await
    .expect("set remote branch");
}

/// The status summary and retry gate as the UI reads them.
async fn route_latest_run_status(app: &TestApp, user: &TestUser, fx: &TrackingFixture) -> String {
    let response = app
        .authenticated_client(user)
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/sync_runs",
            fx.workspace_id, fx.repo_id
        ))
        .await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("sync runs body");
    body["data"]["branch"]["repos"][0]["latestRunStatus"]
        .as_str()
        .expect("latest run status")
        .to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_background_observations_extend_one_run() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // An idle membership: every pass finds G5, provider and watermark equal.
    background_cycle(&app, &fx).await;
    let first = integration_history(&app, fx.key()).await;
    assert_eq!(first.len(), 1);
    assert_eq!(
        (first[0].trigger_kind.as_str(), first[0].status.as_str()),
        ("background_inbound", "noop")
    );
    background_cycle(&app, &fx).await;
    background_cycle(&app, &fx).await;
    let idle = integration_history(&app, fx.key()).await;
    assert_eq!(
        idle.len(),
        1,
        "idle background passes add no rows: {idle:?}"
    );
    assert_eq!(
        idle[0].started_at, first[0].started_at,
        "the first observation is kept"
    );
    assert!(
        idle[0].completed_at > first[0].completed_at,
        "the run's last-seen time advances with each repeated pass"
    );

    // A provider advance is an outcome, and the next no-op is a new observation.
    let c2 = fx.advance_remote("remote_only.txt", "remote work\n");
    background_cycle(&app, &fx).await;
    assert_eq!(wbr_head(&app, fx.key()).await, c2);
    background_cycle(&app, &fx).await;
    background_cycle(&app, &fx).await;
    let statuses: Vec<_> = integration_history(&app, fx.key())
        .await
        .into_iter()
        .map(|run| run.status)
        .collect();
    assert_eq!(statuses, ["noop", "imported", "noop"]);

    // Explicit requests are always recorded, and a later background pass does
    // not fold into them.
    fx.enqueue(&app, "manual").await;
    drain_one_cycle(&app, fx.key()).await;
    fx.enqueue(&app, "manual").await;
    drain_one_cycle(&app, fx.key()).await;
    background_cycle(&app, &fx).await;
    let triggers: Vec<_> = integration_history(&app, fx.key())
        .await
        .into_iter()
        .map(|run| (run.trigger_kind, run.status))
        .collect();
    assert_eq!(
        triggers,
        [
            ("background_inbound".to_string(), "noop".to_string()),
            ("manual".to_string(), "noop".to_string()),
            ("manual".to_string(), "noop".to_string()),
            ("background_inbound".to_string(), "noop".to_string()),
            ("background_inbound".to_string(), "imported".to_string()),
            ("background_inbound".to_string(), "noop".to_string()),
        ]
    );

    fx.cleanup();
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn persistent_background_fetch_failure_extends_one_run_until_recovery() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // The bound provider branch no longer exists.
    set_remote_branch(&app, &fx, "deleted-upstream").await;
    for _ in 0..3 {
        background_cycle(&app, &fx).await;
    }
    let failing = integration_history(&app, fx.key()).await;
    assert_eq!(
        failing.len(),
        1,
        "a repeated failure adds no rows: {failing:?}"
    );
    assert_eq!(failing[0].status, "fetch_failed");
    assert!(
        failing[0]
            .error_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("deleted-upstream")),
        "the failure keeps its diagnostic: {:?}",
        failing[0].error_detail
    );
    assert!(failing[0].completed_at > Some(failing[0].started_at));
    assert_eq!(
        route_latest_run_status(&app, &user, &fx).await,
        "fetch_failed"
    );

    // Recovery is recorded, so status and retry no longer see the old failure.
    set_remote_branch(&app, &fx, "main").await;
    background_cycle(&app, &fx).await;
    let statuses: Vec<_> = integration_history(&app, fx.key())
        .await
        .into_iter()
        .map(|run| run.status)
        .collect();
    assert_eq!(statuses, ["noop", "fetch_failed"]);
    assert_eq!(route_latest_run_status(&app, &user, &fx).await, "noop");
    let retry = app
        .authenticated_client(&user)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/sync_runs/retry",
                fx.workspace_id, fx.repo_id
            ),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(
        retry.status(),
        409,
        "a recovered membership has nothing to retry"
    );

    // A failure after recovery is a new run.
    set_remote_branch(&app, &fx, "deleted-upstream").await;
    background_cycle(&app, &fx).await;
    background_cycle(&app, &fx).await;
    let statuses: Vec<_> = integration_history(&app, fx.key())
        .await
        .into_iter()
        .map(|run| run.status)
        .collect();
    assert_eq!(statuses, ["fetch_failed", "noop", "fetch_failed"]);

    fx.cleanup();
    app.cleanup().await;
}

/// Inserts `count` runs `age_days` old, one second apart, the newest first.
async fn seed_runs(
    app: &TestApp,
    key: WorkspaceRepoKey,
    acting_user_id: Uuid,
    (kind, trigger_kind, status): (&str, &str, &str),
    count: i32,
    age_days: i32,
) {
    sqlx::query(
        "INSERT INTO remote_sync_runs (org_id, workspace_id, repo_id, acting_user_id, kind, trigger_kind, status, started_at, completed_at)
         SELECT $1, $2, $3, $4, $5, $6, $7, at, at
         FROM (SELECT now() - make_interval(days => $9, secs => n) AS at FROM generate_series(1, $8) AS n) seeded",
    )
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(acting_user_id)
    .bind(kind)
    .bind(trigger_kind)
    .bind(status)
    .bind(count)
    .bind(age_days)
    .execute(&app.db)
    .await
    .expect("seed sync runs");
}

async fn run_counts(app: &TestApp, key: WorkspaceRepoKey) -> Vec<(String, String, String, i64)> {
    sqlx::query_as(
        "SELECT kind, trigger_kind, status, COUNT(*)::BIGINT FROM remote_sync_runs
         WHERE workspace_id = $1 AND repo_id = $2
         GROUP BY kind, trigger_kind, status ORDER BY kind, trigger_kind, status",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_all(&app.db)
    .await
    .expect("count sync runs")
}

fn counts(expected: &[(&str, &str, &str, i64)]) -> Vec<(String, String, String, i64)> {
    expected
        .iter()
        .map(|(kind, trigger, status, n)| {
            (
                kind.to_string(),
                trigger.to_string(),
                status.to_string(),
                *n,
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retention_prunes_superseded_noops_and_expired_runs() {
    use g5_api::maintenance::remote_sync_run_retention::{sweep, RETENTION_DAYS, RUNS_TO_KEEP};
    use g5_api::maintenance::TenantMaintenanceDeps;
    assert_eq!((RETENTION_DAYS, RUNS_TO_KEEP), (30, 100));

    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // A real divergence opens a merge run; age its audit row far past retention.
    seed_rebase_divergence(&app, &fx).await;
    background_cycle(&app, &fx).await;
    assert_eq!(remote_tracking_merge_run_count(&app, fx.key()).await, 1);
    sqlx::query(
        "UPDATE remote_sync_runs SET started_at = now() - interval '90 days'
         WHERE workspace_id = $1 AND repo_id = $2 AND merge_run_id IS NOT NULL",
    )
    .bind(fx.workspace_id)
    .bind(fx.repo_id)
    .execute(&app.db)
    .await
    .expect("age merge-run audit row");

    let key = fx.key();
    let integration = |trigger, status| ("integration", trigger, status);
    // Superseded background no-ops, old and recent, from before the fold.
    seed_runs(
        &app,
        key,
        user.id,
        integration("background_inbound", "noop"),
        200,
        45,
    )
    .await;
    seed_runs(
        &app,
        key,
        user.id,
        integration("background_inbound", "noop"),
        50,
        1,
    )
    .await;
    // Expired runs beyond the newest 100, and young runs beyond them.
    seed_runs(
        &app,
        key,
        user.id,
        integration("background_inbound", "fetched"),
        150,
        40,
    )
    .await;
    seed_runs(
        &app,
        key,
        user.id,
        integration("background_inbound", "fetched"),
        120,
        2,
    )
    .await;
    seed_runs(&app, key, user.id, integration("commit", "pushed"), 5, 0).await;
    // A manual no-op is a displayed outcome, not a superseded background pass.
    seed_runs(&app, key, user.id, integration("manual", "noop"), 1, 0).await;
    // The latest run: a background no-op that nothing supersedes.
    sqlx::query(
        "INSERT INTO remote_sync_runs (org_id, workspace_id, repo_id, acting_user_id, kind, trigger_kind, status, started_at, completed_at)
         VALUES ($1, $2, $3, $4, 'integration', 'background_inbound', 'noop', now() + interval '1 minute', now() + interval '1 minute')",
    )
    .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).bind(user.id)
    .execute(&app.db).await.expect("seed latest run");

    // A second membership whose only integration run is old and outnumbered
    // by newer advisory pings.
    let other_workspace = TestWorkspace::create(&app, &org).await;
    let other = TrackingFixture::setup(&app, &org, &other_workspace, user.id).await;
    seed_runs(
        &app,
        other.key(),
        user.id,
        integration("background_inbound", "fetch_failed"),
        1,
        60,
    )
    .await;
    seed_runs(
        &app,
        other.key(),
        user.id,
        ("advisory", "ping", "advisory_updated"),
        110,
        40,
    )
    .await;

    let deps = TenantMaintenanceDeps {
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
    };
    let pruned = sweep(&deps).await.expect("retention sweep");
    assert_eq!(pruned, 250 + 150 + 10);

    assert_eq!(
        run_counts(&app, key).await,
        counts(&[
            ("integration", "background_inbound", "diverged", 1),
            ("integration", "background_inbound", "fetched", 120),
            ("integration", "background_inbound", "noop", 1),
            ("integration", "commit", "pushed", 5),
            ("integration", "manual", "noop", 1),
        ])
    );
    assert_eq!(integration_history(&app, key).await[0].status, "noop");
    assert_eq!(
        run_counts(&app, other.key()).await,
        counts(&[
            ("advisory", "ping", "advisory_updated", 100),
            ("integration", "background_inbound", "fetch_failed", 1),
        ])
    );
    assert_eq!(
        sweep(&deps).await.expect("repeat sweep"),
        0,
        "a second pass is a no-op"
    );

    other.cleanup();
    fx.cleanup();
    app.cleanup().await;
}

/// Seeds one membership with 20 superseded background no-ops, an unsuperseded
/// latest no-op and 109 `fetched` runs 40 days old. The latest run takes rank 1,
/// so 10 of the old runs fall beyond the newest 100: 30 prunable rows.
async fn seed_prunable_history(app: &TestApp, key: WorkspaceRepoKey, acting_user_id: Uuid) {
    let integration = |trigger, status| ("integration", trigger, status);
    seed_runs(
        app,
        key,
        acting_user_id,
        integration("background_inbound", "noop"),
        20,
        1,
    )
    .await;
    seed_runs(
        app,
        key,
        acting_user_id,
        integration("background_inbound", "fetched"),
        109,
        40,
    )
    .await;
    sqlx::query(
        "INSERT INTO remote_sync_runs (org_id, workspace_id, repo_id, acting_user_id, kind, trigger_kind, status, started_at, completed_at)
         VALUES ($1, $2, $3, $4, 'integration', 'background_inbound', 'noop', now() + interval '1 minute', now() + interval '1 minute')",
    )
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(acting_user_id)
    .execute(&app.db)
    .await
    .expect("seed latest run");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retention_pass_prunes_only_its_own_organization() {
    use g5_api::maintenance::remote_sync_run_retention::sweep_org;
    use g5_api::maintenance::TenantMaintenanceDeps;

    let app = spawn_worker_app().await;
    let mut user_a = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut user_a).await;
    let workspace_a = TestWorkspace::create(&app, &org_a).await;
    let a = TrackingFixture::setup(&app, &org_a, &workspace_a, user_a.id).await;
    let mut user_b = TestUser::create(&app).await;
    let org_b = TestOrg::create_with_admin(&app, &mut user_b).await;
    let workspace_b = TestWorkspace::create(&app, &org_b).await;
    let b = TrackingFixture::setup(&app, &org_b, &workspace_b, user_b.id).await;
    assert_ne!(a.org_id, b.org_id);

    // Identical histories: each organization's rules would prune the other's
    // rows if a pass escaped its tenant scope.
    seed_prunable_history(&app, a.key(), user_a.id).await;
    seed_prunable_history(&app, b.key(), user_b.id).await;
    let full = counts(&[
        ("integration", "background_inbound", "fetched", 109),
        ("integration", "background_inbound", "noop", 21),
    ]);
    let pruned = counts(&[
        ("integration", "background_inbound", "fetched", 99),
        ("integration", "background_inbound", "noop", 1),
    ]);

    // The tenant pool runs without BYPASSRLS, so row-level security applies.
    let bypasses_rls: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&app.state.tenant_pool)
    .await
    .expect("read tenant role");
    assert!(
        !bypasses_rls,
        "the retention pass must run under row-level security"
    );

    let deps = TenantMaintenanceDeps {
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
    };
    assert_eq!(sweep_org(&deps, a.org_id).await.expect("org A pass"), 30);
    assert_eq!(run_counts(&app, a.key()).await, pruned);
    assert_eq!(
        run_counts(&app, b.key()).await,
        full,
        "org A's pass must not touch org B's history"
    );

    assert_eq!(sweep_org(&deps, b.org_id).await.expect("org B pass"), 30);
    assert_eq!(run_counts(&app, b.key()).await, pruned);
    assert_eq!(run_counts(&app, a.key()).await, pruned);

    b.cleanup();
    a.cleanup();
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

/// Retirement removes the projection producer, not all possible PostgreSQL
/// blockers. Hold the real finalization boundary briefly and overlap actual
/// Base/other-workspace reads plus stale hints on deliberately small pools.
#[tokio::test]
async fn retirement_small_pools_serve_base_and_peer_during_background_finalization() {
    use g5_api::maintenance::{MaintenanceRunner, MaintenanceRunnerDeps};
    use g5_api::ws::{Channel, WsEvent};
    let mut config = helpers::test_config();
    config.database.max_connections = 4;
    config.tenant_database.max_connections = 4;
    config.authz_database.max_connections = 4;
    config.maintenance.remote_tracking_scheduler_interval = Duration::from_secs(60);
    config.maintenance.remote_tracking_advisory_interval = Duration::from_secs(60);
    let app = TestApp::spawn_with_config(config.clone()).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &base, owner.id).await;
    let head = fx.advance_remote("remote_only.txt", "Provider source changed\n");
    fx.enqueue(&app, "background_inbound").await;
    let mut gate = app.db.begin().await.unwrap();
    let gate_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *gate)
        .await
        .unwrap();
    sqlx::query(
        "SELECT repo_id FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2 FOR UPDATE",
    )
    .bind(base.id)
    .bind(fx.repo_id)
    .fetch_one(&mut *gate)
    .await
    .unwrap();
    let cancel = CancellationToken::new();
    let handles = MaintenanceRunner::new(
        MaintenanceRunnerDeps {
            system_pool: app.system_pool.clone(),
            authz_pool: app.authz_pool.clone(),
            tenant_pool: app.state.tenant_pool.clone(),
            authz: app.state.authz.clone(),
            repo_storage: app.repo_storage.clone(),
            event_bus: app.state.event_bus.clone(),
            git_bin: app.state.git_bin.clone(),
            repository_credentials: app.state.repository_credentials.clone(),
        },
        config.maintenance.clone(),
        cancel.clone(),
        config,
    )
    .spawn_all();
    let subscriber =
        g5_api::maintenance::workspace_pull_requests::spawn(app.state.clone(), cancel.clone());
    tokio::time::timeout(Duration::from_secs(20),async{
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid)) AND query LIKE '%FOR UPDATE OF w, m, r, i%')")
                .bind(gate_pid).fetch_one(&app.db).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("production worker did not reach its finalization policy lock");
    let start = std::sync::Arc::new(tokio::sync::Barrier::new(3));
    let (other_done, other_finished) = tokio::sync::oneshot::channel();
    let mut requests = tokio::task::JoinSet::new();
    for (workspace, done) in [(&base, None), (&other, Some(other_done))] {
        let client = app.authenticated_client(&owner);
        let path = format!(
            "/api/v1/workspaces/{}/repos/{}/files/README.md",
            workspace.id, workspace.repo_id
        );
        let start = start.clone();
        requests.spawn(async move {
            start.wait().await;
            let response = client.get(&path).await;
            let status = response.status();
            let body = response.text().await.unwrap();
            assert_eq!(status, reqwest::StatusCode::OK, "{path}: {body}");
            if let Some(done) = done {
                let _ = done.send(());
            }
        });
    }
    start.wait().await;
    for _ in 0..3 {
        app.state.event_bus.publish(
            &Channel::Workspace(base.id),
            &WsEvent::WorkspaceBranchUpdated {
                workspace_id: base.id,
                repo_id: fx.repo_id,
                branch: "main".into(),
                head_commit: fx.c1.clone(),
                hint: None,
            },
        );
    }
    // This HTTP completion, while the finalizer is blocked, is the overlap
    // barrier. There is no timing-only sleep pretending a request ran.
    tokio::time::timeout(Duration::from_secs(5), other_finished)
        .await
        .expect("unrelated workspace could not read with a finalizer held")
        .unwrap();
    gate.rollback().await.unwrap();
    tokio::time::timeout(Duration::from_secs(15),async{
        while let Some(result)=requests.join_next().await {result.unwrap();}
        loop {
            let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2")
                .bind(base.id).bind(fx.repo_id).fetch_one(&app.db).await.unwrap();
            if remaining==0 {break;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("requests and background finalization must complete after release");
    assert_eq!(wbr_head(&app, fx.key()).await, head);
    assert_eq!(
        latest_background_run(&app, fx.key()).await.status,
        "imported"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename IN ('ontology_nodes','projection_state','doc_node_links')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    cancel.cancel();
    for handle in handles.into_iter().chain([subscriber]) {
        tokio::time::timeout(Duration::from_secs(15), handle)
            .await
            .unwrap()
            .unwrap();
    }
    for pool in [&app.state.tenant_pool, &app.system_pool, &app.authz_pool] {
        let mut held = Vec::new();
        for _ in 0..4 {
            held.push(
                tokio::time::timeout(Duration::from_secs(3), pool.acquire())
                    .await
                    .expect("connection not returned")
                    .unwrap(),
            );
        }
        let lock_timeout: String = sqlx::query_scalar("SHOW lock_timeout")
            .fetch_one(&mut *held[0])
            .await
            .unwrap();
        assert_eq!(lock_timeout, "0", "no pool-wide lock timeout mitigation");
    }
    fx.cleanup();
    app.cleanup().await;
}
