//! Live integration test for **test-view D39** — "Integration queue rejects
//! new enqueues during active merge" — the merge-active short-circuit of the
//! branch integration worker (PR 217 gap-closure, Lane 2.A, chat C1).
//!
//! D39 (verbatim):
//!   Given an active `remote_tracking` merge run for a branch, when
//!   `advance_workspace_branch_after_*` runs on that branch, then the queue
//!   UPSERT is allowed but the next worker pickup short-circuits with audit
//!   row `status='dirty_skipped'` carrying `merge_active` in `error_detail`.
//!   Commits during the merge are NOT blocked, but fetch/push do not run.
//!
//! Design SOT: `designs/REMOTE_REPO_TRACKING.md` §8.2 — "while a merge run for
//! the branch is active … the integration queue rejects new entries for that
//! branch with `merge_active`. Commits on the workspace tip are still allowed
//! … but do not trigger fetch or push until the merge resolves."
//!
//! Discriminator (orchestrator-resolved, design §8.2): the worker sets
//! `sync_paused_reason ∈ {'diverged','watermark_violation'}` at the instant it
//! opens a `remote_tracking` `workspace_merge_runs` row, and clears it only on merge
//! completion — so a branch paused with either reason always has an active
//! merge run. (The `sync_paused_reason='merge_run:<id>'` format design §8/arch
//! describe is not currently set by the worker; that reconciliation is tracked
//! separately and is out of scope here.)
//!
//! Tier 1 discipline: no mocks/stubs of in-scope services. The git transport,
//! the worker loop, the Postgres tenant DB, and the merge-handoff write are all
//! real. The branch is driven into a TRUE divergence (workspace tip and remote
//! tip are siblings off a shared parent) so the FIRST worker pickup genuinely
//! pauses `diverged` and opens a real `remote_tracking` merge run; only THEN do
//! we simulate a commit landing during the merge and assert the second pickup's
//! `merge_active` audit row.

use crate::helpers;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use g5_contracts::types::WorkspaceRepoKey;
use g5_scm::GitRepo;
use helpers::{seed_workspace_authority_head, TestApp, TestOrg, TestUser, TestWorkspace};
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

/// Fixture topology for one remote-tracking branch under test. Self-contained
/// (mirrors `remote_repo_tracking_integration.rs`'s `TrackingFixture`) so this
/// D39 test keeps its boundary off the shared integration file.
#[allow(dead_code)] // `workspace_id` / `repo_id` document the fixture identity.
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
        // The worker stamps branch_sync_runs with the current workspace owner
        // and resolves that owner's SCM credentials.
        sqlx::query("UPDATE workspaces SET owner_user_id = $2, push_mode = 'manual' WHERE id = $1")
            .bind(workspace.id)
            .bind(owner_user_id)
            .execute(&app.db)
            .await
            .expect("set current workspace owner");

        let tmp_root = std::env::temp_dir().join(format!("g5-rt-d39-{}", Uuid::new_v4().simple()));
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

        // repos row — git_url points at the bare remote; provider stays NULL so
        // the worker's auth path resolves GitAuth::None (no token needed for the
        // local transport) instead of misclassifying auth_required.
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

        // workspace_branches row — the trigger synthesizes the child
        // workspace_branch_repos row as `pinned`; flip it to remote_tracking.
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

    /// Advances the local mirror's exact workspace branch. Returns
    /// the new local tip SHA. Used to make the workspace diverge from upstream.
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

    /// Seeds one exact committed workspace cut for worker classification.
    async fn set_workspace_tip(&self, app: &TestApp, sha: &str) {
        seed_workspace_authority_head(app, self.workspace_id, self.repo_id, sha).await;
    }

    /// Inserts a pending integration-queue row the same way the production
    /// commit enqueue tail does (`INSERT … trigger_kind='commit'`). This is the
    /// "queue UPSERT is allowed during a merge" half of D39 — the enqueue path
    /// is unchanged and does NOT filter on `sync_paused_reason`.
    async fn enqueue(&self, app: &TestApp, trigger_kind: &str) {
        let mut db = app.scoped_db_for_org(self.org_id).await;
        enqueue_test_request(db.conn(), self.key(), trigger_kind).await;
        db.commit().await.unwrap();
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
// Worker driver + assertion helpers
// ---------------------------------------------------------------------------

async fn spawn_worker_app() -> TestApp {
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    TestApp::spawn_with_config(config).await
}

/// Drives the REAL worker loop until it drains the given queue row, then shuts
/// it down. The first `tokio::time::interval` tick fires immediately, so the
/// first sweep runs without waiting out `SWEEP_INTERVAL`. We poll the queue
/// table (no fixed sleep) so the test synchronizes on the cycle committing.
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

/// The single most-recent audit row's `(status, error_detail)`.
async fn latest_run_status_error(app: &TestApp, key: WorkspaceRepoKey) -> (String, Option<String>) {
    sqlx::query_as(
        r#"
        SELECT status, error_detail
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

async fn audit_row_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_sync_runs WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count audit rows")
}

async fn merge_active_row_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM remote_sync_runs
        WHERE workspace_id = $1 AND repo_id = $2
          AND status = 'dirty_skipped'
          AND error_detail = 'merge_active'
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count merge_active rows")
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

async fn wbr_paused_reason(app: &TestApp, key: WorkspaceRepoKey) -> Option<String> {
    sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2")
        .bind(key.workspace_id).bind(key.repo_id)
        .fetch_one(&app.db)
        .await
        .expect("wbr paused reason")
}

async fn open_merge_run_count(app: &TestApp, key: WorkspaceRepoKey) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM workspace_merge_runs
        WHERE workspace_id = $1 AND repo_id = $2
          AND source_kind = 'remote_tracking'
          AND status IN ('waiting_for_human', 'running')
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count open merge runs")
}

// ---------------------------------------------------------------------------
// D39 — merge-active short-circuit
// ---------------------------------------------------------------------------

/// test-view **D39**: an active `remote_tracking` merge run on a branch →
/// a commit during the merge re-enqueues (queue UPSERT allowed) → the next
/// worker pickup writes exactly ONE `branch_sync_runs` row with
/// `status='dirty_skipped'` / `error_detail='merge_active'`, deletes the queue
/// row, performs NO fetch/push, and leaves `head_commit` unchanged.
#[tokio::test]
async fn merge_active_pickup_writes_dirty_skipped_audit_and_skips_fetch_push() {
    let app = spawn_worker_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackingFixture::setup(&app, &org, &workspace, user.id).await;

    // ----- Phase 1: drive a TRUE divergence so the FIRST pickup pauses
    // `diverged` and opens a real remote_tracking merge run. -----
    //
    // Upstream advances to C2 (child of C1); the local mirror advances to a
    // SIBLING C2' (also a child of C1). Anchor the workspace tip at C2'.
    let remote_c2 = fx.advance_remote("remote_only.txt", "remote work\n");
    let local_c2 = fx.advance_mirror("local_only.txt", "local work\n");
    assert_ne!(remote_c2, local_c2, "tips must diverge (siblings off C1)");
    fx.set_workspace_tip(&app, &local_c2).await;

    fx.enqueue(&app, "commit").await;
    drain_one_cycle(&app, fx.key()).await;

    // The worker classified `diverged`, paused the branch, and opened the
    // merge run. This is the precondition for D39: "an active merge run."
    let (status1, _err1) = latest_run_status_error(&app, fx.key()).await;
    assert_eq!(status1, "diverged", "phase 1 must classify diverged");
    assert_eq!(
        wbr_paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged"),
        "divergence must pause the branch (merge run active)"
    );
    assert_eq!(
        open_merge_run_count(&app, fx.key()).await,
        1,
        "divergence must open exactly one active remote_tracking merge run"
    );

    let head_before_merge_active = wbr_head(&app, fx.key()).await;
    assert_eq!(
        head_before_merge_active, local_c2,
        "workspace tip stays at the diverged local commit after pause"
    );
    let remote_before = fx.remote_tip();
    assert_eq!(
        remote_before, remote_c2,
        "remote ref is the upstream C2 before the merge-active pickup"
    );
    let audit_count_after_phase1 = audit_row_count(&app, fx.key()).await;

    // ----- Phase 2: a commit lands DURING the merge. Per design §8.2 commits
    // are NOT blocked; the production commit tail re-enqueues a `commit` row
    // (the enqueue path does not filter on `sync_paused_reason`). Simulate the
    // workspace tip advancing (a new local draft commit) + the unchanged
    // enqueue. -----
    let local_c3 = fx.advance_mirror("local_only.txt", "local work 2\n");
    fx.set_workspace_tip(&app, &local_c3).await;

    // The queue UPSERT is ALLOWED even though the branch is paused `diverged`.
    fx.enqueue(&app, "commit").await;
    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(fx.workspace_id).bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count queue rows after re-enqueue");
    assert_eq!(
        queued, 1,
        "commit during merge re-enqueues (UPSERT allowed)"
    );

    // The next worker pickup must short-circuit with merge_active.
    drain_one_cycle(&app, fx.key()).await;

    // (1) Exactly ONE merge_active audit row, and it is the most recent row.
    assert_eq!(
        merge_active_row_count(&app, fx.key()).await,
        1,
        "the merge-active pickup must write exactly one dirty_skipped/merge_active row"
    );
    let (status2, err2) = latest_run_status_error(&app, fx.key()).await;
    assert_eq!(status2, "dirty_skipped", "merge-active pickup status");
    assert_eq!(
        err2.as_deref(),
        Some("merge_active"),
        "merge-active pickup error_detail (design §8.2 / D39)"
    );

    // (2) Exactly ONE new audit row was written (the merge_active one) — the
    // short-circuit does not double-audit.
    assert_eq!(
        audit_row_count(&app, fx.key()).await,
        audit_count_after_phase1 + 1,
        "merge-active pickup adds exactly one audit row"
    );

    // (3) Queue row deleted.
    let queued_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(fx.workspace_id).bind(fx.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count queue rows after pickup");
    assert_eq!(queued_after, 0, "merge-active pickup deletes the queue row");

    // (4) No fetch/push, head_commit unchanged: the workspace tip is still the
    // local C3 we set (the worker did not import the remote or advance it), and
    // the remote ref is untouched (no push of the diverged local tip).
    assert_eq!(
        wbr_head(&app, fx.key()).await,
        local_c3,
        "merge-active pickup must NOT move head_commit"
    );
    assert_eq!(
        fx.remote_tip(),
        remote_before,
        "merge-active pickup must NOT push to the remote"
    );

    // (5) Branch stays paused with the merge run still active.
    assert_eq!(
        wbr_paused_reason(&app, fx.key()).await.as_deref(),
        Some("diverged"),
        "branch stays paused — merge run is unresolved"
    );
    assert_eq!(
        open_merge_run_count(&app, fx.key()).await,
        1,
        "the merge-active short-circuit must not close or duplicate the merge run"
    );

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
