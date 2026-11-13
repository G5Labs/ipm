//! Production-boundary tests for disposable remote-tip observations.
//!
//! The real pinger uses local Git remotes, a tenant-scoped PostgreSQL database
//! and the real event bus. Changed observations update the expiring hint and
//! write an operation receipt; unchanged observations emit no event or receipt.
//! Paused integrations are excluded. Pings never move local or watermark refs.

use crate::helpers;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use g5_api_core::services::remote_integrations;
use g5_contracts::types::WorkspaceRepoKey;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use g5_api::maintenance::remote_advisory_pinger::{
    run_remote_advisory_pinger, RemoteAdvisoryPingerDeps,
};

// ---------------------------------------------------------------------------
// git helpers — drive a real local bare remote + mirror clone
// (mirrors the worker-pipeline suite `remote_repo_tracking_integration.rs`)
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

// ---------------------------------------------------------------------------
// TrackedBranch — one dirty remote-tracking branch (own repo + bare + mirror)
// ---------------------------------------------------------------------------

/// One canonical repository member with remote policy. Its local branch and
/// Git synchronization watermark start at C1; there is no cached observation.
struct TrackedBranch {
    key: WorkspaceRepoKey,
    branch_name: String,
    /// Bare repo standing in for the customer's upstream (`repos.git_url`).
    bare: PathBuf,
    /// Working clone used to advance the upstream ("remote moved").
    seed: PathBuf,
    /// The mirror clone at `repo_storage.local_path(repo_id)`.
    mirror: PathBuf,
    /// Initial commit on `main` (workspace tip + baseline at setup time).
    c1: String,
    /// Root temp dir (bare + seed); cleaned up best-effort by the test.
    tmp_root: PathBuf,
}

impl TrackedBranch {
    /// Builds the bare remote (`main` @ C1), a seed clone, and the mirror clone,
    /// inserts canonical membership and remote policy. The pinger resolves
    /// credentials through the current workspace owner.
    async fn setup(
        app: &TestApp,
        org: &TestOrg,
        workspace: &TestWorkspace,
        created_by: Uuid,
    ) -> Self {
        sqlx::query("UPDATE workspaces SET created_by = $2 WHERE id = $1")
            .bind(workspace.id)
            .bind(created_by)
            .execute(&app.db)
            .await
            .expect("set workspace created_by");

        let tmp_root =
            std::env::temp_dir().join(format!("g5-rt-pinger-{}", Uuid::new_v4().simple()));
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
        write_file(&seed, "README.md", "remote advisory pinger fixture\n");
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

        // repos row — git_url points at the bare remote; provider stays NULL so
        // the pinger's auth path resolves GitAuth::None (no token needed for the
        // local transport) instead of misclassifying auth_required.
        let slug = format!("rt-pinger-repo-{}", repo_id.simple());
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

        let key = WorkspaceRepoKey {
            org_id: org.id,
            workspace_id: workspace.id,
            repo_id,
        };
        sqlx::query("INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role) VALUES ($1,$2,$3,$4,'code')")
            .bind(org.id).bind(workspace.id).bind(repo_id).bind(&slug).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO remote_integrations (org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled) VALUES ($1,$2,$3,'main',true)")
            .bind(org.id).bind(workspace.id).bind(repo_id).execute(&app.db).await.unwrap();
        git(
            &mirror,
            &["update-ref", &format!("refs/heads/{}", workspace.name), &c1],
        );
        git(
            &mirror,
            &[
                "update-ref",
                &format!("refs/g5-tracking/{}/baseline", workspace.name),
                &c1,
            ],
        );
        Self {
            key,
            branch_name: workspace.name.clone(),
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

    /// Reads the bare remote's current `main` tip.
    fn remote_tip(&self) -> String {
        git(&self.bare, &["rev-parse", "refs/heads/main"])
    }

    /// Pre-seeds the advisory SHA (so we can model "advisory already matches /
    /// is stale" without going through a ping). Leaves `observed_at` NULL.
    async fn set_advisory(&self, app: &TestApp, sha: &str) {
        let mut db = app.scoped_db_for_org(self.key.org_id).await;
        let policy = remote_integrations::load(db.conn(), self.key, false)
            .await
            .unwrap()
            .unwrap();
        remote_integrations::observe_tip(db.conn(), &policy, sha)
            .await
            .unwrap();
        db.commit().await.unwrap();
    }

    /// Writes the on-disk per-workspace baseline ref in the mirror:
    /// `refs/g5-tracking/main/baseline → sha` (per `GitRepo::write_baseline_ref`).
    fn write_baseline_ref(&self, sha: &str) {
        git(
            &self.mirror,
            &[
                "update-ref",
                &format!("refs/g5-tracking/{}/baseline", self.branch_name),
                sha,
            ],
        );
    }

    /// Reads the on-disk baseline ref, or `None` if it is absent.
    fn read_baseline_ref(&self) -> Option<String> {
        let out = Command::new("git")
            .args([
                "rev-parse",
                &format!("refs/g5-tracking/{}/baseline", self.branch_name),
            ])
            .current_dir(&self.mirror)
            .output()
            .expect("spawn git rev-parse baseline ref");
        if out.status.success() {
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            None
        }
    }

    async fn advisory(&self, app: &TestApp) -> Option<String> {
        observation(app, self.key).await
    }
    async fn observed_at_is_set(&self, app: &TestApp) -> bool {
        observation(app, self.key).await.is_some()
    }
    async fn head(&self, _app: &TestApp) -> String {
        git(
            &self.mirror,
            &["rev-parse", &format!("refs/heads/{}", self.branch_name)],
        )
    }
    async fn baseline(&self, _app: &TestApp) -> String {
        self.read_baseline_ref().unwrap()
    }

    /// Count of `advisory_updated` audit rows written for this branch.
    async fn advisory_run_count(&self, app: &TestApp) -> i64 {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::BIGINT
            FROM remote_sync_runs
            WHERE workspace_id = $1 AND repo_id = $2
              AND kind = 'advisory'
              AND status = 'advisory_updated'
            "#,
        )
        .bind(self.key.workspace_id)
        .bind(self.key.repo_id)
        .fetch_one(&app.db)
        .await
        .expect("advisory run count")
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.tmp_root);
        let _ = std::fs::remove_dir_all(&self.mirror);
    }
}

// ---------------------------------------------------------------------------
// Pinger driver
// ---------------------------------------------------------------------------

/// Spawns a `TestApp` whose pools are sized for the pinger sweep (the global
/// advisory-lock connection is held for the whole sweep while each target's
/// `resolve_auth` acquires a second system-pool connection).
async fn spawn_pinger_app() -> TestApp {
    let mut config = helpers::test_config();
    config.database.max_connections = 8;
    config.tenant_database.max_connections = 8;
    config.authz_database.max_connections = 8;
    TestApp::spawn_with_config(config).await
}

/// Spawns the REAL pinger loop. `tokio::time::interval`'s first tick fires
/// immediately, so the first sweep runs without waiting out the interval.
fn spawn_pinger(app: &TestApp) -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let deps = RemoteAdvisoryPingerDeps {
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repository_credentials: app.state.repository_credentials.clone(),
        event_bus: app.state.event_bus.clone(),
        repo_storage: app.state.repo_storage.clone(),
    };
    let cancel = CancellationToken::new();
    let handle = tokio::spawn(run_remote_advisory_pinger(
        deps,
        Duration::from_millis(200),
        true,
        cancel.clone(),
    ));
    (cancel, handle)
}

/// Polls until the branch's advisory SHA equals `expected` (deterministic
/// synchronization — no fixed sleep on the pinger). The pinger publishes the
/// advisory event right after the same transaction commits, so once this
/// returns the event is already in flight.
async fn observation(app: &TestApp, key: WorkspaceRepoKey) -> Option<String> {
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
    observed
}

/// The hint as a reader in `policy`'s org sees it for exactly that binding.
async fn observed_for(
    app: &TestApp,
    policy: &remote_integrations::RemoteIntegration,
) -> Option<String> {
    let mut db = app.scoped_db_for_org(policy.org_id).await;
    let observed = remote_integrations::observed_tip(db.conn(), policy)
        .await
        .unwrap()
        .map(|o| o.oid);
    db.rollback().await.unwrap();
    observed
}

async fn load_policy(
    app: &TestApp,
    key: WorkspaceRepoKey,
) -> remote_integrations::RemoteIntegration {
    let mut db = app.scoped_db_for_org(key.org_id).await;
    let policy = remote_integrations::load(db.conn(), key, false)
        .await
        .unwrap()
        .unwrap();
    db.rollback().await.unwrap();
    policy
}

async fn wait_for_advisory(app: &TestApp, key: WorkspaceRepoKey, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let cur = observation(app, key).await;
        if cur.as_deref() == Some(expected) {
            return;
        }
        if Instant::now() > deadline {
            let diag = dump_advisory_runs(app, key).await;
            panic!("advisory for {key} did not reach {expected} within 45s; cur={cur:?}; runs={diag:?}");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Polls until the deployment-wide count of `advisory_updated` audit rows
/// reaches `target`.
async fn wait_for_total_advisory_runs(app: &TestApp, target: i64) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let n = total_advisory_runs(app).await;
        if n >= target {
            return;
        }
        if Instant::now() > deadline {
            panic!("only {n}/{target} advisory_updated rows within 60s");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

async fn total_advisory_runs(app: &TestApp) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_sync_runs WHERE kind = 'advisory' AND status = 'advisory_updated'",
    )
    .fetch_one(&app.db)
    .await
    .expect("total advisory run count")
}

/// Returns `(kind, status, advisory_sha, trigger_kind)` for the latest audit row.
async fn latest_advisory_run(
    app: &TestApp,
    key: WorkspaceRepoKey,
) -> (String, String, Option<String>, String) {
    sqlx::query_as(
        r#"
        SELECT kind, status, new_mirror_tip, trigger_kind
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

async fn dump_advisory_runs(
    app: &TestApp,
    key: WorkspaceRepoKey,
) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT kind, status, error_detail FROM remote_sync_runs WHERE workspace_id = $1 AND repo_id = $2 ORDER BY started_at DESC",
    )
    .bind(key.workspace_id).bind(key.repo_id)
    .fetch_all(&app.db)
    .await
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// D24 — batched sweep refreshes changed branches + emits per-branch event;
//        unchanged branches are visited but silent
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn advisory_sweep_updates_changed_branch_and_emits_event_unchanged_silent() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // Branch A (CHANGED): the remote moved to C2 while the advisory is stale at C1.
    let branch_a = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    let a_c2 = branch_a.advance_remote("a.txt", "A remote moved\n");
    branch_a.set_advisory(&app, &branch_a.c1).await;
    assert_ne!(a_c2, branch_a.c1);

    // Branch B (UNCHANGED): the remote is still C1 and the advisory already equals C1.
    let branch_b = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    branch_b.set_advisory(&app, &branch_b.c1).await;
    assert_eq!(
        branch_b.remote_tip(),
        branch_b.c1,
        "precondition: B's remote did not move"
    );

    // Subscribe to the workspace channel and barrier on the `subscribed` ack so
    // there is no race between the subscription registering and the pinger's
    // publish (no sleeps — the ack is the synchronization point).
    let channel = format!("workspace:{}", workspace.id);
    let mut ws = app.ws_connect(&user, &channel).await;
    let ack = ws.expect_subscribed().await;
    assert_eq!(ack["channel"], channel);

    // One batched sweep refreshes A (changed) and visits B (unchanged).
    let (cancel, handle) = spawn_pinger(&app);
    wait_for_advisory(&app, branch_a.key, &a_c2).await;
    cancel.cancel();
    let _ = handle.await;

    // A: advisory advanced to the observed remote tip; exactly one audit row.
    assert_eq!(
        branch_a.advisory(&app).await.as_deref(),
        Some(a_c2.as_str())
    );
    assert_eq!(
        branch_a.advisory_run_count(&app).await,
        1,
        "the changed branch records exactly one advisory_updated row"
    );

    // B: advisory unchanged at C1; visited (observed_at touched) but no audit row.
    assert_eq!(
        branch_b.advisory(&app).await.as_deref(),
        Some(branch_b.c1.as_str()),
        "an unchanged branch keeps its advisory SHA"
    );
    assert!(
        branch_b.observed_at_is_set(&app).await,
        "an unchanged branch is still visited — observed_at is touched"
    );
    assert_eq!(
        branch_b.advisory_run_count(&app).await,
        0,
        "an unchanged branch writes NO advisory_updated row"
    );

    // Exactly ONE advisory event reached the client — for the CHANGED branch A.
    let evt = ws
        .expect_event(
            "workspace.remote_tip_advisory_updated",
            Duration::from_secs(5),
        )
        .await;
    assert_eq!(
        evt["repoId"],
        branch_a.key.repo_id.to_string(),
        "the advisory event identifies the changed branch"
    );
    assert_eq!(evt["remoteTipAdvisorySha"], a_c2);
    // ...and the UNCHANGED branch B emitted none (no second event).
    ws.expect_silence(Duration::from_millis(750)).await;

    ws.close().await;
    branch_a.cleanup();
    branch_b.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D25 — the dirty-branch loop pings a dirty branch, then STOPS once it is clean
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn advisory_ping_never_moves_head_or_baseline() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let fx = TrackedBranch::setup(&app, &org, &workspace, user.id).await;

    // The remote SHA moves ahead to C2; the advisory was last observed at C1.
    let c2 = fx.advance_remote("remote_only.txt", "remote moved ahead\n");
    assert_ne!(c2, fx.c1);
    fx.set_advisory(&app, &fx.c1).await;
    fx.write_baseline_ref(&fx.c1);

    // Snapshot every durable tracked-state field BEFORE the ping.
    let head_before = fx.head(&app).await;
    let baseline_before = fx.baseline(&app).await;
    assert_eq!(head_before, fx.c1);
    assert_eq!(baseline_before, fx.c1);
    assert_eq!(
        fx.read_baseline_ref().as_deref(),
        Some(fx.c1.as_str()),
        "precondition: baseline ref seeded at C1"
    );

    let (cancel, handle) = spawn_pinger(&app);
    wait_for_advisory(&app, fx.key, &c2).await;
    cancel.cancel();
    let _ = handle.await;

    // The advisory advanced to the observed remote tip...
    assert_eq!(fx.advisory(&app).await.as_deref(), Some(c2.as_str()));

    // ...but NOTHING that tracks durable branch state moved (design §6.4, arch D19).
    assert_eq!(
        fx.head(&app).await,
        head_before,
        "advisory must not move the wbr head_commit"
    );
    assert_eq!(
        fx.baseline(&app).await,
        baseline_before,
        "advisory must not advance baseline_commit"
    );
    assert_eq!(
        fx.read_baseline_ref().as_deref(),
        Some(fx.c1.as_str()),
        "advisory must not rewrite the on-disk refs/g5-tracking/<branch>/baseline ref"
    );

    // The audit row is kind='advisory', status='advisory_updated', trigger='ping'.
    let (kind, status, advisory_sha, trigger) = latest_advisory_run(&app, fx.key).await;
    assert_eq!(kind, "advisory");
    assert_eq!(status, "advisory_updated");
    assert_eq!(advisory_sha.as_deref(), Some(c2.as_str()));
    assert_eq!(trigger, "ping");

    fx.cleanup();
    app.cleanup().await;
}

/// g5.workspace.remote.base-parity: the advisory pinger observes Base's
/// provider tip like any workspace, so Base can read "behind" before import.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn advisory_ping_observes_base_like_any_workspace() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
        .bind(base.id)
        .bind(base.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let fx = TrackedBranch::setup(&app, &org, &base, user.id).await;

    let c2 = fx.advance_remote("remote_only.txt", "remote moved ahead\n");
    fx.set_advisory(&app, &fx.c1).await;
    fx.write_baseline_ref(&fx.c1);

    let (cancel, handle) = spawn_pinger(&app);
    wait_for_advisory(&app, fx.key, &c2).await;
    cancel.cancel();
    let _ = handle.await;

    assert_eq!(fx.advisory(&app).await.as_deref(), Some(c2.as_str()));
    assert_eq!(fx.head(&app).await, fx.c1, "the advisory never imports");

    fx.cleanup();
    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// D27 — a sweep over many dirty branches pings each exactly once, within the
//        deployment concurrency ceiling (upper-bound only; see arch D20 / C24)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn advisory_sweep_pings_each_dirty_branch_once_within_ceiling() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // N dirty remote-tracking branches (each its own repo → satisfies
    // UNIQUE(workspace_id, repo_id)). N exceeds the documented concurrency
    // ceiling of 10 so the sweep handles a more-than-ceiling batch in one pass.
    const N: usize = 12;
    let mut branches = Vec::with_capacity(N);
    for _ in 0..N {
        branches.push(TrackedBranch::setup(&app, &org, &workspace, user.id).await);
    }

    let (cancel, handle) = spawn_pinger(&app);
    wait_for_total_advisory_runs(&app, N as i64).await;
    cancel.cancel();
    let _ = handle.await;

    // Each dirty branch was pinged exactly once: N advisory_updated rows, one per
    // wbr — no starvation, no duplication. This is the observable upper-bound
    // contract of arch D20 (reconciled in chat C24): the pinger is single-flight
    // (sequential under the global advisory lock per leaf D4/I4), so the observed
    // number of concurrent ls-remote calls never exceeds 1 — well under the
    // ceiling of 10. A real bounded-concurrency Semaphore + in-flight gauge
    // (test view Q4) is deferred to the design §11 back-pressure work, so D27
    // stays `partial` in test_view §7 (Rule 10): this asserts the upper bound,
    // not that parallelism reaches the ceiling.
    for b in &branches {
        assert_eq!(
            b.advisory_run_count(&app).await,
            1,
            "every dirty branch is pinged exactly once per sweep"
        );
        assert_eq!(b.advisory(&app).await.as_deref(), Some(b.c1.as_str()));
    }
    assert_eq!(
        total_advisory_runs(&app).await,
        N as i64,
        "exactly one advisory ping per dirty branch — the sweep is bounded"
    );
    const _: () = assert!(
        N <= 64,
        "the dirty target set stays within MAX_BRANCHES_PER_SWEEP (64)"
    );

    for b in &branches {
        b.cleanup();
    }
    app.cleanup().await;
}

/// The producer exits before the HTTP consumer reads: process-local caches
/// cannot satisfy this contract, even if same-process pinger tests pass.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remote_observation_survives_producer_process_and_is_read_by_api() {
    const CHILD_INPUT: &str = "G5_REMOTE_OBSERVATION_CHILD_INPUT";
    if let Ok(input) = std::env::var(CHILD_INPUT) {
        let (db_name, policy, oid): (String, remote_integrations::RemoteIntegration, String) =
            serde_json::from_str(&input).unwrap();
        // The parent's database through this process's own superuser
        // connection: nothing here shares memory with the API that reads.
        let pool = sqlx::PgPool::connect(&helpers::TestDb::url_for_database(&db_name).await)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(remote_integrations::observe_tip(&mut tx, &policy, &oid)
            .await
            .unwrap());
        tx.commit().await.unwrap();
        return;
    }
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    let tip = branch.advance_remote("new.txt", "provider moved\n");
    let mut db = app.scoped_db_for_org(org.id).await;
    let policy = remote_integrations::load(db.conn(), branch.key, false)
        .await
        .unwrap()
        .unwrap();
    db.rollback().await.unwrap();
    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(helpers::exact_test_name(
            module_path!(),
            "remote_observation_survives_producer_process_and_is_read_by_api",
        ))
        .arg("--nocapture")
        .env(
            CHILD_INPUT,
            serde_json::to_string(&(db_name, policy.clone(), tip.clone())).unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/sync_runs",
        workspace.id, branch.key.repo_id
    );
    let client = app.authenticated_client(&user);
    let response = client.get(&route).await;
    assert!(response.status().is_success());
    let response: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        response["data"]["branch"]["repos"][0]["remoteTipAdvisorySha"],
        tip
    );
    // A hint for a previous remote binding must disappear immediately.
    sqlx::query(
        "UPDATE remote_integrations SET remote_branch='other' WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(workspace.id)
    .bind(branch.key.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let changed: serde_json::Value = client.get(&route).await.json().await.unwrap();
    assert!(changed["data"]["branch"]["repos"][0]["remoteTipAdvisorySha"].is_null());
    // The shared table also fails closed for another membership or URL.
    let mut other = policy.clone();
    other.workspace_id = Uuid::new_v4();
    assert!(observed_for(&app, &other).await.is_none());
    other = policy.clone();
    other.git_url.push_str("-different");
    assert!(observed_for(&app, &other).await.is_none());
    branch.cleanup();
    app.cleanup().await;
}

/// `observe_tip` reports a change exactly when no current hint matches the
/// observation: no row, an expired row, or a different OID, URL or branch.
/// Readers never see an expired or rebound row, with no sweep involved.
#[tokio::test]
async fn observe_tip_reports_changes_and_readers_ignore_expired_or_rebound_hints() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    let policy = load_policy(&app, branch.key).await;
    let first = "1".repeat(40);
    let second = "2".repeat(40);

    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(
        remote_integrations::observe_tip(db.conn(), &policy, &first)
            .await
            .unwrap(),
        "the first observation is a change"
    );
    assert!(
        !remote_integrations::observe_tip(db.conn(), &policy, &first)
            .await
            .unwrap(),
        "repeating the current tip is not a change"
    );
    assert!(
        remote_integrations::observe_tip(db.conn(), &policy, &second)
            .await
            .unwrap(),
        "a new tip is a change"
    );
    assert!(
        remote_integrations::observe_tip(db.conn(), &policy, &"0".repeat(40))
            .await
            .is_err(),
        "the zero OID is never recorded"
    );
    db.commit().await.unwrap();
    assert_eq!(
        observed_for(&app, &policy).await.as_deref(),
        Some(second.as_str())
    );

    // A rebound remote (branch or URL) hides the hint until it is observed
    // again under the new binding; the old binding's row is simply ignored.
    let mut rebound = policy.clone();
    rebound.remote_branch = "other".into();
    assert!(observed_for(&app, &rebound).await.is_none());
    let mut rebound = policy.clone();
    rebound.git_url.push_str("-moved");
    assert!(observed_for(&app, &rebound).await.is_none());

    // Expiry: age the row past the TTL. Readers see nothing, and the next
    // observation of the very same tip counts as a change again, exactly as a
    // lapsed cache entry did.
    sqlx::query(
        "UPDATE remote_tip_observations SET observed_at = now() - interval '301 seconds' \
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace.id)
    .bind(branch.key.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    assert!(observed_for(&app, &policy).await.is_none());
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(
        remote_integrations::observe_tip(db.conn(), &policy, &second)
            .await
            .unwrap()
    );
    db.commit().await.unwrap();
    assert_eq!(
        observed_for(&app, &policy).await.as_deref(),
        Some(second.as_str())
    );

    branch.cleanup();
    app.cleanup().await;
}

/// Hints are tenant rows: one org never reads or overwrites another org's
/// observation. (Moved here from `remote_tracking_auth_boundary.rs` when the
/// store moved from Redis keys to Postgres rows bound to real policies.)
#[tokio::test]
async fn advisory_observations_cannot_be_read_or_overwritten_across_orgs() {
    let app = spawn_pinger_app().await;
    let mut user_a = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut user_a).await;
    let workspace_a = TestWorkspace::create(&app, &org_a).await;
    let branch_a = TrackedBranch::setup(&app, &org_a, &workspace_a, user_a.id).await;
    let mut user_b = TestUser::create(&app).await;
    let org_b = TestOrg::create_with_admin(&app, &mut user_b).await;
    let workspace_b = TestWorkspace::create(&app, &org_b).await;
    let branch_b = TrackedBranch::setup(&app, &org_b, &workspace_b, user_b.id).await;
    let policy_a = load_policy(&app, branch_a.key).await;
    let policy_b = load_policy(&app, branch_b.key).await;
    let tip_a = "1".repeat(40);
    let tip_b = "3".repeat(40);

    branch_a.set_advisory(&app, &tip_a).await;
    // Org B's scope neither sees A's hint nor can write one for A's policy.
    let mut db_b = app.scoped_db_for_org(org_b.id).await;
    assert!(remote_integrations::observed_tip(db_b.conn(), &policy_a)
        .await
        .unwrap()
        .is_none());
    assert!(
        remote_integrations::observe_tip(db_b.conn(), &policy_a, &"2".repeat(40))
            .await
            .is_err(),
        "row-level security refuses a cross-org write"
    );
    db_b.rollback().await.unwrap();
    branch_b.set_advisory(&app, &tip_b).await;
    assert_eq!(
        observed_for(&app, &policy_a).await.as_deref(),
        Some(tip_a.as_str())
    );
    assert_eq!(
        observed_for(&app, &policy_b).await.as_deref(),
        Some(tip_b.as_str())
    );

    branch_a.cleanup();
    branch_b.cleanup();
    app.cleanup().await;
}

/// Two concurrent first observations of one
/// integration serialize on its policy row, so exactly one reports a change.
/// Without that lock a missing observation row cannot be locked and both
/// would report `true`, duplicating the advisory audit row and event.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_first_observations_report_exactly_one_change() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    let policy = load_policy(&app, branch.key).await;
    let tip = "4".repeat(40);

    let mut first = app.scoped_db_for_org(org.id).await;
    let first_changed = remote_integrations::observe_tip(first.conn(), &policy, &tip)
        .await
        .unwrap();
    assert!(
        first_changed,
        "the first observer sees no row and reports a change"
    );

    // A second observer starts while the first transaction is still open.
    let second = tokio::spawn({
        let state = app.state.clone();
        let policy = policy.clone();
        let tip = tip.clone();
        async move {
            let capability = state
                .authz
                .mint_service_db_capability(
                    g5_contracts::types::ids::UserId::from_uuid(Uuid::nil()),
                    g5_contracts::types::ids::OrgId::from_uuid(policy.org_id),
                    &["*".to_string()],
                )
                .await
                .unwrap();
            let mut db =
                g5_authz::scoped_db::ScopedDb::from_capability(&state.tenant_pool, capability)
                    .await
                    .unwrap();
            let changed = remote_integrations::observe_tip(db.conn(), &policy, &tip)
                .await
                .unwrap();
            db.commit().await.unwrap();
            changed
        }
    });
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        !second.is_finished(),
        "the second observer waits on the policy row until the first commits"
    );
    first.commit().await.unwrap();
    let second_changed = second.await.unwrap();
    assert!(
        !second_changed,
        "the second observer sees the committed row with the same tip and reports no change"
    );
    assert_eq!(
        observed_for(&app, &policy).await.as_deref(),
        Some(tip.as_str())
    );

    branch.cleanup();
    app.cleanup().await;
}

/// Expiry is judged on the wall clock, not the
/// transaction start. A reader whose transaction has been open for a while
/// must not see a hint that expired meanwhile, and a writer that waited across
/// the TTL boundary must report the tip as changed and stamp a fresh time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expiry_is_judged_on_the_wall_clock_inside_open_transactions() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    let policy = load_policy(&app, branch.key).await;
    let tip = "5".repeat(40);
    branch.set_advisory(&app, &tip).await;
    let age_past_ttl = || {
        sqlx::query(
            "UPDATE remote_tip_observations SET observed_at = clock_timestamp() - interval '301 seconds' \
             WHERE workspace_id = $1 AND repo_id = $2",
        )
        .bind(workspace.id)
        .bind(branch.key.repo_id)
        .execute(&app.db)
    };

    // Reader: the transaction opens while the hint is current, then the hint
    // expires. With the transaction-start clock the aged row would still
    // read as current.
    let mut reader = app.scoped_db_for_org(org.id).await;
    assert!(remote_integrations::observed_tip(reader.conn(), &policy)
        .await
        .unwrap()
        .is_some());
    tokio::time::sleep(Duration::from_millis(2_200)).await;
    age_past_ttl().await.unwrap();
    assert!(
        remote_integrations::observed_tip(reader.conn(), &policy)
            .await
            .unwrap()
            .is_none(),
        "a hint that expired while the reader's transaction was open reads as absent"
    );
    reader.rollback().await.unwrap();

    // Writer: the transaction opens, then waits long enough for the hint to
    // expire; observing the same tip again is a change and stamps fresh.
    branch.set_advisory(&app, &tip).await;
    let mut writer = app.scoped_db_for_org(org.id).await;
    sqlx::query("SELECT 1")
        .execute(writer.conn())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(2_200)).await;
    age_past_ttl().await.unwrap();
    assert!(
        remote_integrations::observe_tip(writer.conn(), &policy, &tip)
            .await
            .unwrap(),
        "the expired previous row counts as absent, so the same tip is a change"
    );
    writer.commit().await.unwrap();
    let fresh: bool = sqlx::query_scalar(
        "SELECT observed_at > clock_timestamp() - interval '2 seconds' \
           FROM remote_tip_observations WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace.id)
    .bind(branch.key.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        fresh,
        "the refreshed stamp is current when the writer commits"
    );

    branch.cleanup();
    app.cleanup().await;
}

/// A hint stamped in the future (the database
/// clock stepped backward after it was written) is not current: readers see
/// no hint, and the next observation of the same tip counts as a change.
#[tokio::test]
async fn a_future_stamped_hint_is_not_current() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    let policy = load_policy(&app, branch.key).await;
    let tip = "6".repeat(40);
    branch.set_advisory(&app, &tip).await;
    sqlx::query(
        "UPDATE remote_tip_observations SET observed_at = clock_timestamp() + interval '10 minutes' \
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace.id)
    .bind(branch.key.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    assert!(observed_for(&app, &policy).await.is_none());
    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(remote_integrations::observe_tip(db.conn(), &policy, &tip)
        .await
        .unwrap());
    db.commit().await.unwrap();
    assert_eq!(
        observed_for(&app, &policy).await.as_deref(),
        Some(tip.as_str())
    );
    branch.cleanup();
    app.cleanup().await;
}

/// The tracking summary's hint read is
/// non-authoritative, so a failed read (the hint table missing, as during a
/// rollout) degrades to "no hint" and the durable run history stays readable.
#[tokio::test]
async fn a_failed_hint_read_never_hides_the_tracking_summary() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    branch.set_advisory(&app, &"7".repeat(40)).await;
    sqlx::query("ALTER TABLE remote_tip_observations RENAME TO remote_tip_observations_missing")
        .execute(&app.db)
        .await
        .unwrap();
    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/sync_runs",
        workspace.id, branch.key.repo_id
    );
    let response = app.authenticated_client(&user).get(&route).await;
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status, reqwest::StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(
        body["data"]["branch"]["repos"][0]["remoteTipAdvisorySha"].is_null(),
        "a failed hint read degrades to no hint: {body}"
    );
    assert_eq!(
        body["data"]["branch"]["repos"][0]["repoId"],
        branch.key.repo_id.to_string(),
        "the durable tracking state is still served: {body}"
    );
    branch.cleanup();
    app.cleanup().await;
}

/// Readers that take the hint as optional run the read under a savepoint. A
/// failed read must release that savepoint after rolling back to it, or a
/// request that reads one hint per member in one transaction (fork
/// advisories) keeps one open subtransaction per failed read.
#[tokio::test]
async fn a_failed_hint_read_leaves_no_savepoint_open() {
    let app = spawn_pinger_app().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = TrackedBranch::setup(&app, &org, &workspace, user.id).await;
    let policy = load_policy(&app, branch.key).await;
    sqlx::query("ALTER TABLE remote_tip_observations RENAME TO remote_tip_observations_missing")
        .execute(&app.db)
        .await
        .unwrap();

    let mut db = app.scoped_db_for_org(policy.org_id).await;
    for _ in 0..3 {
        assert!(
            remote_integrations::observed_tip_or_none(db.conn(), &policy)
                .await
                .is_none(),
            "a failed read degrades to no hint"
        );
    }
    sqlx::query("SELECT 1")
        .execute(db.conn())
        .await
        .expect("the transaction stays usable after failed hint reads");
    let error = sqlx::query("RELEASE SAVEPOINT g5_remote_tip_hint")
        .execute(db.conn())
        .await
        .expect_err("no hint savepoint may outlive the failed read");
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("3B001"),
        "releasing the helper's savepoint must fail as unknown: {error}"
    );
    db.rollback().await.unwrap();
    branch.cleanup();
    app.cleanup().await;
}
