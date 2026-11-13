//! Landing contracts of the branch-commit writer (P1.4).
//!
//! `workspace_authority_push_finalizer.rs` and
//! `workspace_authority_application_core.rs` proved these behaviours through
//! the head-operation registry, which P1.4 deletes. Every producer now
//! prepares and lands its cut through `services::branch_commit`, so the
//! retained assertions of those suites live here against that path: a
//! landing whose database session dies or hangs releases the workspace for a
//! valid successor, a plain-git rewrite that lands between the landing's
//! verification and git's compare-and-swap refuses the cut as stale, a
//! partial cross-repository ref write is a git error that a fresh cut
//! recovers, and the inventory, draft and task-overlay checks refuse a cut
//! whose authority moved after it was prepared.

use crate::helpers;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use g5_api::errors::G5Error;
use g5_api::services::branch_commit::{
    map_branch_commit_error, prepare_branch_commit, prepare_initial_branch_commit_in_tx,
    BranchCommitError, FenceMode, InitialWorkspaceHeadInput, LandedBranchCommit,
    PreparedBranchCommit,
};
use g5_contracts::types::{AuthorityTarget, WorkspaceHeadRepoInput, WorkspaceHeadTransitionKind};
use g5_scm::{GitAuthor, GitRepo};
use helpers::{TestApp, TestOrg, TestWorkspace};
use uuid::Uuid;

const ZERO_GIT_SHA: &str = "0000000000000000000000000000000000000000";
const ACTOR: &str = "branch_commit_landing_test";

struct AdditionalRepo {
    repo_id: Uuid,
    head_commit: String,
}

async fn add_materialized_repo(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
) -> AdditionalRepo {
    let repo_id = Uuid::new_v4();
    let name = format!("push-repo-{}", repo_id.simple());
    let repo_path = app.init_repo(org.id, repo_id).await;
    let raw_repo = git2::Repository::init(&repo_path).expect("initialize Git repository");
    raw_repo
        .set_head("refs/heads/main")
        .expect("select main branch");
    drop(raw_repo);
    let repo = GitRepo::open(&repo_path).expect("open additional repository");
    std::fs::write(repo_path.join("README.md"), "additional repository\n")
        .expect("write seed file");
    repo.add_all().expect("stage seed file");
    let head_commit = repo
        .commit(
            "seed additional repository",
            &GitAuthor::from_profile(Some("G5 Test"), Some("g5@test.local")),
        )
        .expect("commit seed file");
    repo.create_branch(&workspace.name, &head_commit)
        .expect("create exact workspace branch in additional repository");

    sqlx::query(
        r#"
        INSERT INTO repos (id, org_id, name, slug, git_url, default_branch)
        VALUES ($1, $2, $3, $3, $4, 'main')
        "#,
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(&name)
    .bind(repo_path.display().to_string())
    .execute(&app.db)
    .await
    .expect("register additional repository");

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
    .bind(&name)
    .execute(&app.db)
    .await
    .expect("register additional workspace repository mapping");

    AdditionalRepo {
        repo_id,
        head_commit,
    }
}

/// Register an unborn workspace member (no storage, all-zero head). Returns
/// its repository id and workspace branch id.
async fn add_unborn_repo(app: &TestApp, org: &TestOrg, workspace: &TestWorkspace) -> Uuid {
    let repo_id = Uuid::new_v4();
    let name = format!("pending-push-repo-{}", repo_id.simple());
    sqlx::query(
        r#"
        INSERT INTO repos (id, org_id, name, slug, git_url, default_branch)
        VALUES ($1, $2, $3, $3, $4, 'main')
        "#,
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(&name)
    .bind(format!("pending://{repo_id}"))
    .execute(&app.db)
    .await
    .expect("register pending repository");

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
    .bind(&name)
    .execute(&app.db)
    .await
    .expect("register pending workspace repository mapping");

    repo_id
}

async fn create_detached_candidate(
    app: &TestApp,
    repo_id: Uuid,
    canonical_branch: &str,
    expected_head: &str,
    file_name: &str,
) -> String {
    let repo_path = app.repo_path(repo_id).await;
    let repo = GitRepo::open(&repo_path).expect("open repository");
    match repo.resolve_branch_head(canonical_branch) {
        Ok(actual_head) => assert_eq!(actual_head, expected_head),
        Err(_) => repo
            .create_branch(canonical_branch, expected_head)
            .expect("materialize canonical branch at its registered head"),
    }
    let candidate_branch = format!("candidate-{}", Uuid::new_v4().simple());
    repo.create_branch(&candidate_branch, expected_head)
        .expect("create candidate branch");
    repo.checkout_branch_force(&candidate_branch)
        .expect("checkout candidate branch");
    std::fs::write(repo_path.join(file_name), "candidate\n").expect("write candidate file");
    repo.add_all().expect("stage candidate file");
    let candidate = repo
        .commit(
            "prepare push candidate",
            &GitAuthor::from_profile(Some("G5 Test"), Some("g5@test.local")),
        )
        .expect("commit candidate");
    repo.checkout_branch_force(canonical_branch)
        .expect("restore canonical branch");
    assert_eq!(
        repo.resolve_branch_head(canonical_branch)
            .expect("canonical head"),
        expected_head
    );
    candidate
}

async fn assert_repo_cut(app: &TestApp, workspace_id: Uuid, repo_id: Uuid, expected: &str) {
    let selected: String = sqlx::query_scalar("SELECT CASE WHEN w.is_base THEN r.default_branch ELSE w.name END FROM workspace_repos m JOIN workspaces w ON w.id=m.workspace_id JOIN repos r ON r.id=m.repo_id WHERE m.workspace_id=$1 AND m.repo_id=$2")
        .bind(workspace_id).bind(repo_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(branch_head(app, repo_id, &selected).await, expected);
}

async fn mirror_path(app: &TestApp, repo_id: Uuid) -> PathBuf {
    app.repo_path(repo_id).await
}

/// The commit `refs/heads/<branch>` names in the internal mirror.
async fn branch_head(app: &TestApp, repo_id: Uuid, branch: &str) -> String {
    GitRepo::open(&mirror_path(app, repo_id).await)
        .expect("open repository")
        .resolve_branch_head(branch)
        .expect("branch ref")
}

/// Move `refs/heads/<branch>` with plain git, exactly as an operator would,
/// from `expected` to `target`. Nothing in Postgres changes.
fn move_ref_with_plain_git(repo_path: &Path, branch: &str, target: &str, expected: &str) {
    let output = Command::new("git")
        .args([
            "update-ref",
            &format!("refs/heads/{branch}"),
            target,
            expected,
        ])
        .current_dir(repo_path)
        .output()
        .expect("run git update-ref");
    assert!(
        output.status.success(),
        "git update-ref failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Number of reflog entries of `refs/heads/<branch>`: every write to the
/// ref, by the landing or by plain git, adds one, so an unchanged count
/// proves the ref was not written, not even transiently.
fn reflog_len(repo_path: &Path, branch: &str) -> usize {
    git2::Repository::open(repo_path)
        .expect("open mirror")
        .reflog(&format!("refs/heads/{branch}"))
        .map(|reflog| reflog.len())
        .unwrap_or(0)
}

/// Replaces a branch reflog with a directory, so git can lock the ref but
/// cannot write its log entry and therefore refuses to commit the ref.
/// `remove` puts the original log back, keeping reflog counts comparable.
struct ReflogObstruction {
    reflog: PathBuf,
    saved: Vec<u8>,
}

impl ReflogObstruction {
    fn install(repo_path: &Path, branch: &str) -> Self {
        let git_dir = git2::Repository::open(repo_path)
            .expect("open mirror")
            .path()
            .to_path_buf();
        let reflog = git_dir.join("logs/refs/heads").join(branch);
        let saved = std::fs::read(&reflog).expect("the obstructed branch has a reflog");
        std::fs::remove_file(&reflog).expect("remove the branch reflog");
        std::fs::create_dir_all(&reflog).expect("create the reflog obstruction");
        std::fs::write(reflog.join("obstruction"), "refuse reflog writes")
            .expect("fill the reflog obstruction");
        Self { reflog, saved }
    }

    fn remove(self) {
        std::fs::remove_dir_all(&self.reflog).expect("remove the reflog obstruction");
        std::fs::write(&self.reflog, &self.saved).expect("restore the branch reflog");
    }
}

fn workspace_target(workspace: &TestWorkspace) -> AuthorityTarget {
    AuthorityTarget::Workspace {
        workspace_id: workspace.id,
    }
}

/// Capture a cut through the production preparation from a scope that is
/// closed again before the caller does anything else.
async fn prepare_cut(
    app: &TestApp,
    org_id: Uuid,
    target: AuthorityTarget,
    repos: Vec<WorkspaceHeadRepoInput>,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    let capture = app.scoped_db_for_org(org_id).await;
    let scope = capture.reopener();
    capture.rollback().await.expect("release the capture scope");
    prepare_branch_commit(&scope, &app.repo_storage, target, repos, ACTOR).await
}

/// Land a prepared cut in a fresh scope, looking repositories up through
/// `repo_storage`.
async fn land_cut(
    app: &TestApp,
    org_id: Uuid,
    prepared: PreparedBranchCommit,
    repo_storage: &g5_api_core::services::git::repo_store::TenantRepoStorage,
) -> Result<LandedBranchCommit, BranchCommitError> {
    let db = app.scoped_db_for_org(org_id).await;
    prepared.land(db, &app.state.event_bus, repo_storage).await
}

/// The error of a result the test expects to fail.
fn refused<T, E>(result: Result<T, E>, expectation: &str) -> E {
    match result {
        Ok(_) => panic!("{expectation}"),
        Err(error) => error,
    }
}

/// `(repo_id, expected_old, current_head)` of every mismatch of a typed
/// stale-cut refusal, or a panic naming the other error.
fn stale_cut_mismatches(error: BranchCommitError) -> Vec<(String, String, String)> {
    let BranchCommitError::Database(error) = error else {
        panic!("expected a stale-cut refusal, got {error:?}");
    };
    match map_branch_commit_error(error) {
        G5Error::AuthorityCutStale { details } => details
            .mismatches
            .iter()
            .map(|mismatch| {
                (
                    mismatch.repo_id.clone(),
                    mismatch.expected_old.clone(),
                    mismatch.current_head.clone(),
                )
            })
            .collect(),
        other => panic!("expected a typed stale-cut conflict, got {other:?}"),
    }
}

/// The message of a refused branch ref update (git could not lock, inspect
/// or write a ref), which every caller sees as a git error, or a panic
/// naming the other error.
fn git_refusal_message(error: BranchCommitError) -> String {
    let BranchCommitError::Database(error) = error else {
        panic!("expected a refused branch ref update, got {error:?}");
    };
    match map_branch_commit_error(error) {
        G5Error::GitError { message } => message,
        other => panic!("expected a refused branch ref update, got {other:?}"),
    }
}

/// Holds a landing on an advisory gate the test owns when the landing
/// runs its domain callback: inside the landing's fenced transaction
/// and before any ref is applied.
struct LandingHold {
    gate_key: i64,
    gate: sqlx::pool::PoolConnection<sqlx::Postgres>,
}

impl LandingHold {
    async fn install(app: &TestApp) -> Self {
        let gate_key = (Uuid::new_v4().as_u128() as i64) & i64::MAX;
        let mut gate = app.db.acquire().await.expect("acquire the landing gate");
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(gate_key)
            .execute(&mut *gate)
            .await
            .expect("hold the landing gate");
        Self { gate_key, gate }
    }

    /// The backend of the held landing: waiting on the gate while it holds
    /// the workspace commit fence.
    async fn held_backend(app: &TestApp) -> i32 {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let pid: Option<i32> = sqlx::query_scalar(
                    r#"
                    SELECT activity.pid
                    FROM pg_stat_activity activity
                    WHERE activity.datname = current_database()
                      AND activity.wait_event_type = 'Lock'
                      AND activity.wait_event = 'advisory'
                      AND activity.query LIKE '%landing_test_hold%'
                      AND EXISTS (
                          SELECT 1
                          FROM pg_locks held
                          WHERE held.pid = activity.pid
                            AND held.locktype = 'advisory'
                            AND held.granted
                      )
                    ORDER BY activity.pid
                    LIMIT 1
                    "#,
                )
                .fetch_optional(&app.db)
                .await
                .expect("inspect the held landing backend");
                if let Some(pid) = pid {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("the landing never reached its domain callback under the workspace fence")
    }

    async fn release(mut self, _app: &TestApp) {
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(self.gate_key)
            .execute(&mut *self.gate)
            .await
            .expect("release the landing gate");
    }
}

async fn run_killed_landing_recovery(multi_code: bool) {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let abandoned_candidate = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "abandoned-landing-candidate.txt",
    )
    .await;
    let recovery_candidate = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "recovery-landing-candidate.txt",
    )
    .await;
    let mut abandoned_inputs = vec![WorkspaceHeadRepoInput {
        repo_id: workspace.repo_id,
        expected_old_head: workspace.head_commit.clone(),
        proposed_new_head: abandoned_candidate,
    }];
    let mut recovery_inputs = vec![WorkspaceHeadRepoInput {
        repo_id: workspace.repo_id,
        expected_old_head: workspace.head_commit.clone(),
        proposed_new_head: recovery_candidate.clone(),
    }];
    let mut additional = Vec::new();
    if multi_code {
        for name in ["beta", "docs"] {
            let member = add_materialized_repo(&app, &org, &workspace).await;
            let abandoned_head = if name == "docs" {
                member.head_commit.clone()
            } else {
                create_detached_candidate(
                    &app,
                    member.repo_id,
                    &workspace.name,
                    &member.head_commit,
                    "abandoned-beta.txt",
                )
                .await
            };
            let recovered_head = if name == "docs" {
                sqlx::query("UPDATE workspaces SET docs_repo_id=$2 WHERE id=$1")
                    .bind(workspace.id)
                    .bind(member.repo_id)
                    .execute(&app.db)
                    .await
                    .unwrap();
                member.head_commit.clone()
            } else {
                create_detached_candidate(
                    &app,
                    member.repo_id,
                    &workspace.name,
                    &member.head_commit,
                    "recovered-beta.txt",
                )
                .await
            };
            abandoned_inputs.push(WorkspaceHeadRepoInput {
                repo_id: member.repo_id,
                expected_old_head: member.head_commit.clone(),
                proposed_new_head: abandoned_head,
            });
            recovery_inputs.push(WorkspaceHeadRepoInput {
                repo_id: member.repo_id,
                expected_old_head: member.head_commit.clone(),
                proposed_new_head: recovered_head.clone(),
            });
            additional.push((member, recovered_head));
        }
    }
    let target = workspace_target(&workspace);
    // The abandoned cut is captured twice from the old base: one capture
    // lands and loses its session, the other is kept to prove that a cut
    // captured before the death cannot land over the recovery.
    let abandoned = prepare_cut(&app, org.id, target.clone(), abandoned_inputs.clone())
        .await
        .expect("capture the abandoned cut");
    let old_base_capture = prepare_cut(&app, org.id, target.clone(), abandoned_inputs)
        .await
        .expect("capture the abandoned cut again");

    let hold = LandingHold::install(&app).await;
    let gate_key = hold.gate_key;
    let landing_db = app.scoped_db_for_org(org.id).await;
    let abandoned_event_bus = app.state.event_bus.clone();
    let abandoned_repo_storage = app.repo_storage.clone();
    let mut abandoned_task = tokio::spawn(async move {
        abandoned
            .land_with(
                landing_db,
                &abandoned_event_bus,
                &abandoned_repo_storage,
                FenceMode::Blocking,
                move |db, _| {
                    Box::pin(async move {
                        sqlx::query("SELECT pg_advisory_xact_lock($1) /* landing_test_hold */")
                            .bind(gate_key)
                            .execute(db.conn())
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
    });

    let abandoned_backend_pid = LandingHold::held_backend(&app).await;
    assert!(!abandoned_task.is_finished());
    let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(abandoned_backend_pid)
        .fetch_one(&app.db)
        .await
        .expect("terminate the abandoned landing's database session");
    assert!(
        terminated,
        "PostgreSQL must terminate the live landing session"
    );
    let abandoned_error = tokio::time::timeout(Duration::from_secs(10), &mut abandoned_task)
        .await
        .expect("the abandoned landing did not observe its session death")
        .expect("abandoned landing task")
        .expect_err("a terminated landing must not report a landed cut");
    assert!(
        matches!(abandoned_error, BranchCommitError::App(_))
            && abandoned_error
                .to_string()
                .contains("terminating connection"),
        "{abandoned_error:?}"
    );
    hold.release(&app).await;

    // Server-session death before COMMIT rolled the landing back, and the
    // landing never reached the refs.
    assert_repo_cut(
        &app,
        workspace.id,
        workspace.repo_id,
        &workspace.head_commit,
    )
    .await;
    assert_eq!(
        branch_head(&app, workspace.repo_id, &workspace.name).await,
        workspace.head_commit
    );
    for (member, _) in &additional {
        assert_repo_cut(&app, workspace.id, member.repo_id, &member.head_commit).await;
        assert_eq!(
            branch_head(&app, member.repo_id, &workspace.name).await,
            member.head_commit
        );
    }

    let recovered = tokio::time::timeout(Duration::from_secs(10), async {
        let prepared = prepare_cut(&app, org.id, target.clone(), recovery_inputs.clone())
            .await
            .expect("capture the recovery cut");
        land_cut(&app, org.id, prepared, &app.repo_storage).await
    })
    .await
    .expect("a valid successor cut remained blocked after the landing session died")
    .expect("a different valid cut must recover workspace liveness");
    assert_eq!(recovered.members.len(), if multi_code { 3 } else { 1 });
    assert!(
        recovered.projection_warnings.is_empty(),
        "a clean landing leaves no pending projection work: {:?}",
        recovered.projection_warnings
    );
    for (member, head) in &additional {
        assert_repo_cut(&app, workspace.id, member.repo_id, head).await;
        assert_eq!(
            branch_head(&app, member.repo_id, &workspace.name).await,
            *head
        );
    }
    assert_repo_cut(&app, workspace.id, workspace.repo_id, &recovery_candidate).await;
    assert_eq!(
        branch_head(&app, workspace.repo_id, &workspace.name).await,
        recovery_candidate
    );

    // A producer that lost the acknowledgement replays the complete cut: the
    // refs already carry it and nothing is left pending.
    let replay = prepare_cut(&app, org.id, target, recovery_inputs)
        .await
        .expect("capture the replayed recovery cut");
    let replayed = land_cut(&app, org.id, replay, &app.repo_storage)
        .await
        .expect("replaying the recovered cut is idempotent");
    assert_ne!(replayed.advance_id, recovered.advance_id);
    assert!(
        replayed.projection_warnings.is_empty(),
        "a replay leaves no pending projection work: {:?}",
        replayed.projection_warnings
    );
    for (member, head) in &additional {
        assert_repo_cut(&app, workspace.id, member.repo_id, head).await;
    }
    assert_repo_cut(&app, workspace.id, workspace.repo_id, &recovery_candidate).await;

    // The cut captured from the old base before the death is refused now
    // that the recovery moved the refs, instead of landing over them.
    let error = refused(
        land_cut(&app, org.id, old_base_capture, &app.repo_storage).await,
        "the abandoned old-base cut must be refused after the recovery",
    );
    let mismatches = stale_cut_mismatches(error);
    assert!(
        mismatches.contains(&(
            workspace.repo_id.to_string(),
            workspace.head_commit.clone(),
            recovery_candidate.clone(),
        )),
        "{mismatches:?}"
    );
    assert_repo_cut(&app, workspace.id, workspace.repo_id, &recovery_candidate).await;
    assert_eq!(
        branch_head(&app, workspace.repo_id, &workspace.name).await,
        recovery_candidate
    );
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_killed_landing_session_releases_the_workspace_for_a_different_valid_cut() {
    run_killed_landing_recovery(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_killed_multi_repo_landing_preserves_every_member_then_a_complete_cut_recovers() {
    run_killed_landing_recovery(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hung_landing_times_out_and_releases_the_workspace_for_a_valid_successor() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let hung_candidate = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "hung-landing-candidate.txt",
    )
    .await;
    let recovery_candidate = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "timeout-recovery-candidate.txt",
    )
    .await;
    let target = workspace_target(&workspace);
    let hung = prepare_cut(
        &app,
        org.id,
        target.clone(),
        vec![WorkspaceHeadRepoInput {
            repo_id: workspace.repo_id,
            expected_old_head: workspace.head_commit.clone(),
            proposed_new_head: hung_candidate,
        }],
    )
    .await
    .expect("capture the hung cut");

    let hold = LandingHold::install(&app).await;
    let gate_key = hold.gate_key;
    let landing_db = app.scoped_db_for_org(org.id).await;
    let hung_event_bus = app.state.event_bus.clone();
    let hung_repo_storage = app.repo_storage.clone();
    let started = Instant::now();
    let mut hung_task = tokio::spawn(async move {
        hung.land_with(
            landing_db,
            &hung_event_bus,
            &hung_repo_storage,
            FenceMode::Blocking,
            move |db, _| {
                Box::pin(async move {
                    sqlx::query("SELECT pg_advisory_xact_lock($1) /* landing_test_hold */")
                        .bind(gate_key)
                        .execute(db.conn())
                        .await?;
                    Ok(())
                })
            },
        )
        .await
    });
    LandingHold::held_backend(&app).await;

    let hung_error = tokio::time::timeout(Duration::from_secs(18), &mut hung_task)
        .await
        .expect("the hung landing exceeded its 15-second database deadline")
        .expect("hung landing task")
        .expect_err("a hung landing must not land");
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(14) && elapsed < Duration::from_secs(18),
        "the landing deadline drifted from 15 seconds: {elapsed:?}"
    );
    assert!(
        hung_error.to_string().contains("statement timeout"),
        "unexpected landing timeout error: {hung_error}"
    );
    hold.release(&app).await;
    assert_repo_cut(
        &app,
        workspace.id,
        workspace.repo_id,
        &workspace.head_commit,
    )
    .await;
    assert_eq!(
        branch_head(&app, workspace.repo_id, &workspace.name).await,
        workspace.head_commit
    );

    let recovered = tokio::time::timeout(Duration::from_secs(10), async {
        let prepared = prepare_cut(
            &app,
            org.id,
            target,
            vec![WorkspaceHeadRepoInput {
                repo_id: workspace.repo_id,
                expected_old_head: workspace.head_commit.clone(),
                proposed_new_head: recovery_candidate.clone(),
            }],
        )
        .await
        .expect("capture the successor cut");
        land_cut(&app, org.id, prepared, &app.repo_storage).await
    })
    .await
    .expect("a valid successor cut remained blocked after the landing timed out")
    .expect("a different valid cut must recover workspace liveness");
    assert!(
        recovered.projection_warnings.is_empty(),
        "{:?}",
        recovered.projection_warnings
    );
    assert_repo_cut(&app, workspace.id, workspace.repo_id, &recovery_candidate).await;
    assert_eq!(
        branch_head(&app, workspace.repo_id, &workspace.name).await,
        recovery_candidate
    );
    app.cleanup().await;
}

#[tokio::test]
async fn a_plain_git_rewrite_during_a_multi_repo_landing_wins_and_the_cut_is_refused() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let docs = add_materialized_repo(&app, &org, &workspace).await;
    let code_repo_path = mirror_path(&app, workspace.repo_id).await;
    let docs_repo_path = mirror_path(&app, docs.repo_id).await;
    let code_candidate = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "code-candidate.txt",
    )
    .await;
    let rewrite = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "plain-git-rewrite.txt",
    )
    .await;
    let docs_candidate = create_detached_candidate(
        &app,
        docs.repo_id,
        &workspace.name,
        &docs.head_commit,
        "docs-candidate.txt",
    )
    .await;
    let prepared = prepare_cut(
        &app,
        org.id,
        workspace_target(&workspace),
        vec![
            WorkspaceHeadRepoInput {
                repo_id: workspace.repo_id,
                expected_old_head: workspace.head_commit.clone(),
                proposed_new_head: code_candidate,
            },
            WorkspaceHeadRepoInput {
                repo_id: docs.repo_id,
                expected_old_head: docs.head_commit.clone(),
                proposed_new_head: docs_candidate,
            },
        ],
    )
    .await
    .expect("capture the two-repository cut");
    let code_reflog = reflog_len(&code_repo_path, &workspace.name);
    let docs_reflog = reflog_len(&docs_repo_path, &workspace.name);

    // The landing looks each repository up once to re-read the refs it
    // verifies in its transaction, and once more to assemble the ref updates
    // it hands to git. The rewrite of the code ref lands on the second lookup
    // of the docs repository: after that verification passed and before git
    // locks the refs for its compare-and-swap.
    // Another writer, acting on the path lookups the landing makes.
    let lookups = Arc::new(AtomicUsize::new(0));
    let fired = Arc::new(AtomicBool::new(false));
    let storage = {
        let (lookups, fired) = (lookups.clone(), fired.clone());
        let trigger_repo_id = docs.repo_id;
        let rewrite_repo_path = code_repo_path.clone();
        let branch = workspace.name.clone();
        let from = workspace.head_commit.clone();
        let to = rewrite.clone();
        app.repo_storage.observed_for_test(Arc::new(move |key| {
            if key.repo_id() == trigger_repo_id && lookups.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
                move_ref_with_plain_git(&rewrite_repo_path, &branch, &to, &from);
                fired.store(true, Ordering::SeqCst);
            }
        }))
    };
    let error = refused(
        land_cut(&app, org.id, prepared, &storage).await,
        "the rewritten ref refuses the cut",
    );
    assert!(
        fired.load(Ordering::SeqCst),
        "the rewrite must have landed during the landing"
    );

    assert_eq!(
        stale_cut_mismatches(error),
        vec![(
            workspace.repo_id.to_string(),
            workspace.head_commit.clone(),
            rewrite.clone(),
        )]
    );
    // The accepted rewrite survives; the docs ref, whose expectation held,
    // was never written; neither ref carries a landing write.
    assert_eq!(
        branch_head(&app, workspace.repo_id, &workspace.name).await,
        rewrite
    );
    assert_eq!(
        branch_head(&app, docs.repo_id, &workspace.name).await,
        docs.head_commit
    );
    assert_eq!(
        reflog_len(&code_repo_path, &workspace.name),
        code_reflog + 1,
        "exactly the plain-git rewrite was written to the code ref"
    );
    assert_eq!(
        reflog_len(&docs_repo_path, &workspace.name),
        docs_reflog,
        "the docs ref was not written, not even transiently"
    );
    assert_repo_cut(&app, workspace.id, workspace.repo_id, &rewrite).await;
    assert_repo_cut(&app, workspace.id, docs.repo_id, &docs.head_commit).await;
    app.cleanup().await;
}

#[tokio::test]
async fn a_partial_cross_repository_ref_write_is_restored_under_the_fence_and_a_fresh_cut_lands_it()
{
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let docs = add_materialized_repo(&app, &org, &workspace).await;
    let code_repo_path = mirror_path(&app, workspace.repo_id).await;
    let docs_repo_path = mirror_path(&app, docs.repo_id).await;
    let code_candidate = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "code-candidate.txt",
    )
    .await;
    let docs_candidate = create_detached_candidate(
        &app,
        docs.repo_id,
        &workspace.name,
        &docs.head_commit,
        "docs-candidate.txt",
    )
    .await;
    // Git commits the locked ref updates in repository order: the reflog of
    // the member it writes last is obstructed, so that write fails after the
    // first member's ref was already written.
    let code_first = workspace.repo_id < docs.repo_id;
    let (first_repo_id, first_path, first_expected, first_candidate) = if code_first {
        (
            workspace.repo_id,
            &code_repo_path,
            &workspace.head_commit,
            &code_candidate,
        )
    } else {
        (
            docs.repo_id,
            &docs_repo_path,
            &docs.head_commit,
            &docs_candidate,
        )
    };
    let (second_repo_id, second_path, second_expected, second_candidate) = if code_first {
        (
            docs.repo_id,
            &docs_repo_path,
            &docs.head_commit,
            &docs_candidate,
        )
    } else {
        (
            workspace.repo_id,
            &code_repo_path,
            &workspace.head_commit,
            &code_candidate,
        )
    };
    let inputs = vec![
        WorkspaceHeadRepoInput {
            repo_id: workspace.repo_id,
            expected_old_head: workspace.head_commit.clone(),
            proposed_new_head: code_candidate.clone(),
        },
        WorkspaceHeadRepoInput {
            repo_id: docs.repo_id,
            expected_old_head: docs.head_commit.clone(),
            proposed_new_head: docs_candidate.clone(),
        },
    ];
    let prepared = prepare_cut(&app, org.id, workspace_target(&workspace), inputs.clone())
        .await
        .expect("capture the two-repository cut");
    let first_reflog = reflog_len(first_path, &workspace.name);
    let second_reflog = reflog_len(second_path, &workspace.name);

    let obstruction = ReflogObstruction::install(second_path, &workspace.name);
    let error = refused(
        land_cut(&app, org.id, prepared, &app.repo_storage).await,
        "an obstructed reflog fails git's write of the member it writes last",
    );
    obstruction.remove();
    let message = git_refusal_message(error);
    assert!(
        message.contains(&format!(
            "refs/heads/{} of repository {second_repo_id}",
            workspace.name
        )),
        "{message}"
    );
    assert!(
        message.contains("were restored"),
        "the refusal must say the refs git had already written were put back: {message}"
    );
    // The landing still held the fence, so the member git had already written
    // is restored before any reader can capture a partial cut
    // (`g5.git.landing.commit-point`); the refused member is untouched;
    // nothing landed in the database.
    assert_eq!(
        branch_head(&app, first_repo_id, &workspace.name).await,
        *first_expected
    );
    assert_eq!(reflog_len(first_path, &workspace.name), first_reflog + 2);
    assert_eq!(
        branch_head(&app, second_repo_id, &workspace.name).await,
        *second_expected
    );
    assert_eq!(reflog_len(second_path, &workspace.name), second_reflog);
    assert_repo_cut(&app, workspace.id, first_repo_id, first_expected).await;
    assert_repo_cut(&app, workspace.id, second_repo_id, second_expected).await;

    // A fresh cut with the same expectations and results lands both members.
    let fresh = prepare_cut(&app, org.id, workspace_target(&workspace), inputs)
        .await
        .expect("a fresh cut captures the restored refs");
    let landed = land_cut(&app, org.id, fresh, &app.repo_storage)
        .await
        .expect("a fresh cut lands the whole cut");
    assert!(
        landed.projection_warnings.is_empty(),
        "{:?}",
        landed.projection_warnings
    );
    assert_eq!(
        branch_head(&app, first_repo_id, &workspace.name).await,
        *first_candidate
    );
    assert_eq!(reflog_len(first_path, &workspace.name), first_reflog + 3);
    assert_eq!(
        branch_head(&app, second_repo_id, &workspace.name).await,
        *second_candidate
    );
    assert_eq!(reflog_len(second_path, &workspace.name), second_reflog + 1);
    assert_repo_cut(&app, workspace.id, workspace.repo_id, &code_candidate).await;
    assert_repo_cut(&app, workspace.id, docs.repo_id, &docs_candidate).await;
    app.cleanup().await;
}

#[tokio::test]
async fn preparing_a_cut_that_omits_a_materialized_member_fails_closed() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _additional = add_materialized_repo(&app, &org, &workspace).await;

    let error = refused(
        prepare_cut(
            &app,
            org.id,
            workspace_target(&workspace),
            vec![WorkspaceHeadRepoInput {
                repo_id: workspace.repo_id,
                expected_old_head: workspace.head_commit.clone(),
                proposed_new_head: workspace.head_commit.clone(),
            }],
        )
        .await,
        "partial workspace vectors must fail closed",
    );
    assert!(
        error
            .to_string()
            .contains("does not match its registered authority transition"),
        "{error}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn preparing_a_working_cut_rejects_a_missing_member_ref() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    add_unborn_repo(&app, &org, &workspace).await;
    let error = refused(
        prepare_cut(
            &app,
            org.id,
            workspace_target(&workspace),
            vec![WorkspaceHeadRepoInput {
                repo_id: workspace.repo_id,
                expected_old_head: workspace.head_commit.clone(),
                proposed_new_head: workspace.head_commit.clone(),
            }],
        )
        .await,
        "a missing working ref must not be omitted from a complete cut",
    );
    assert!(error.to_string().contains("missing"), "{error}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_member_materialized_after_preparation_refuses_the_landing() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let candidate = create_detached_candidate(
        &app,
        workspace.repo_id,
        &workspace.name,
        &workspace.head_commit,
        "rejected-candidate.txt",
    )
    .await;
    let prepared = prepare_cut(
        &app,
        org.id,
        workspace_target(&workspace),
        vec![WorkspaceHeadRepoInput {
            repo_id: workspace.repo_id,
            expected_old_head: workspace.head_commit.clone(),
            proposed_new_head: candidate,
        }],
    )
    .await
    .expect("capture the complete one-repository cut");

    // The cut captured a complete one-repository vector. Another
    // authoritative member materializes before it lands.
    let additional = add_materialized_repo(&app, &org, &workspace).await;
    let error = refused(
        land_cut(&app, org.id, prepared, &app.repo_storage).await,
        "a cut missing a member that materialized after preparation must be refused",
    );
    assert!(
        error
            .to_string()
            .contains("it materialized after the cut was prepared"),
        "{error}"
    );
    assert_repo_cut(
        &app,
        workspace.id,
        workspace.repo_id,
        &workspace.head_commit,
    )
    .await;
    assert_eq!(
        branch_head(&app, workspace.repo_id, &workspace.name).await,
        workspace.head_commit
    );
    assert_repo_cut(
        &app,
        workspace.id,
        additional.repo_id,
        &additional.head_commit,
    )
    .await;
    assert_eq!(
        branch_head(&app, additional.repo_id, &workspace.name).await,
        additional.head_commit
    );
    app.cleanup().await;
}

#[tokio::test]
async fn first_materialization_selects_unborn_members_without_blocking_other_pending_repos() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create_base(&app, &org).await;
    let initializing_repo_id = add_unborn_repo(&app, &org, &workspace).await;
    let pending_repo_id = add_unborn_repo(&app, &org, &workspace).await;
    let target = workspace_target(&workspace);
    let initial = |repo_id: Uuid, proposed: &str| InitialWorkspaceHeadInput {
        repo_id,
        proposed_new_head: proposed.to_string(),
    };
    let first_commit = "b".repeat(40);
    let second_commit = "c".repeat(40);
    let mut scope = app.scoped_db_for_org(org.id).await;

    // Only the selected unborn member initializes; the materialized sibling
    // stays an unchanged member and the other pending repository stays out.
    let prepared = prepare_initial_branch_commit_in_tx(
        &mut scope,
        &app.repo_storage,
        target.clone(),
        vec![initial(initializing_repo_id, &first_commit)],
        ACTOR,
    )
    .await
    .expect("a selected unborn repository initializes");
    assert_eq!(prepared.members().len(), 2);
    let initializing = prepared
        .member(initializing_repo_id)
        .expect("the selected unborn member is in the cut");
    assert_eq!(
        initializing.transition,
        WorkspaceHeadTransitionKind::Initialize
    );
    assert_eq!(initializing.expected_old_head, ZERO_GIT_SHA);
    assert_eq!(initializing.proposed_new_head, first_commit);
    let sibling = prepared
        .member(workspace.repo_id)
        .expect("the materialized sibling is in the cut");
    assert_eq!(sibling.transition, WorkspaceHeadTransitionKind::Advance);
    assert_eq!(sibling.expected_old_head, workspace.head_commit);
    assert_eq!(sibling.proposed_new_head, workspace.head_commit);
    assert!(prepared.member(pending_repo_id).is_none());

    // Several selected unborn members initialize atomically.
    let prepared = prepare_initial_branch_commit_in_tx(
        &mut scope,
        &app.repo_storage,
        target.clone(),
        vec![
            initial(initializing_repo_id, &first_commit),
            initial(pending_repo_id, &second_commit),
        ],
        ACTOR,
    )
    .await
    .expect("multiple selected unborn repositories initialize atomically");
    assert_eq!(prepared.members().len(), 3);
    assert_eq!(
        prepared
            .member(pending_repo_id)
            .map(|member| &member.transition),
        Some(&WorkspaceHeadTransitionKind::Initialize)
    );

    // Selecting an already materialized member is refused: its ref is not
    // the all-zero head the initialization expects.
    let error = refused(
        prepare_initial_branch_commit_in_tx(
            &mut scope,
            &app.repo_storage,
            target.clone(),
            vec![initial(workspace.repo_id, &"d".repeat(40))],
            ACTOR,
        )
        .await,
        "a materialized member cannot be initialized",
    );
    match map_branch_commit_error(error) {
        G5Error::AuthorityCutStale { details } => assert!(
            details.mismatches.iter().any(|mismatch| {
                mismatch.repo_id == workspace.repo_id.to_string()
                    && mismatch.expected_old == ZERO_GIT_SHA
                    && mismatch.current_head == workspace.head_commit
            }),
            "{details:?}"
        ),
        other => panic!("expected a typed stale-cut conflict, got {other:?}"),
    }

    // The same member cannot be named twice, and an initial head must be an
    // exact non-zero commit.
    let error = refused(
        prepare_initial_branch_commit_in_tx(
            &mut scope,
            &app.repo_storage,
            target.clone(),
            vec![
                initial(initializing_repo_id, &first_commit),
                initial(initializing_repo_id, &second_commit),
            ],
            ACTOR,
        )
        .await,
        "a duplicate member must be refused",
    );
    assert!(
        error
            .to_string()
            .contains("cannot name one repository member twice"),
        "{error}"
    );
    let error = refused(
        prepare_initial_branch_commit_in_tx(
            &mut scope,
            &app.repo_storage,
            target,
            vec![initial(initializing_repo_id, ZERO_GIT_SHA)],
            ACTOR,
        )
        .await,
        "an all-zero initial head must be refused",
    );
    assert!(
        error
            .to_string()
            .contains("must propose exact non-zero Git SHAs"),
        "{error}"
    );
    scope.rollback().await.expect("close the preparation scope");
    app.cleanup().await;
}

/// A root commit object in `repo` that no ref names yet.
fn root_commit_without_ref(repo: &git2::Repository, message: &str) -> String {
    let tree_id = repo
        .treebuilder(None)
        .expect("tree builder")
        .write()
        .expect("write the empty tree");
    let tree = repo.find_tree(tree_id).expect("empty tree");
    let signature = git2::Signature::now("G5 Test", "g5@test.local").expect("signature");
    repo.commit(None, &signature, &signature, message, &tree, &[])
        .expect("create a root commit without a ref")
        .to_string()
}

/// The workspace half of the rule: a workspace cut names its unborn member
/// with the all-zero expectation, types it `Initialize`, and lands the
/// member's first branch ref and head caches.
#[tokio::test]
async fn a_workspace_cut_initializes_an_unborn_member_and_lands_its_first_ref() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create_base(&app, &org).await;
    let unborn_repo_id = add_unborn_repo(&app, &org, &workspace).await;
    let repo_path = app.init_repo(org.id, unborn_repo_id).await;
    let root_commit = root_commit_without_ref(
        &git2::Repository::init(&repo_path).expect("initialize Git repository"),
        "first workspace commit",
    );

    let prepared = prepare_cut(
        &app,
        org.id,
        workspace_target(&workspace),
        vec![
            WorkspaceHeadRepoInput {
                repo_id: workspace.repo_id,
                expected_old_head: workspace.head_commit.clone(),
                proposed_new_head: workspace.head_commit.clone(),
            },
            WorkspaceHeadRepoInput {
                repo_id: unborn_repo_id,
                expected_old_head: ZERO_GIT_SHA.to_string(),
                proposed_new_head: root_commit.clone(),
            },
        ],
    )
    .await
    .expect("a workspace cut may initialize its unborn member");
    assert_eq!(
        prepared
            .member(unborn_repo_id)
            .map(|member| member.transition),
        Some(WorkspaceHeadTransitionKind::Initialize)
    );
    land_cut(&app, org.id, prepared, &app.repo_storage)
        .await
        .expect("the workspace initialization lands");

    assert_eq!(
        branch_head(&app, unborn_repo_id, &workspace.name).await,
        root_commit
    );
    assert_repo_cut(&app, workspace.id, unborn_repo_id, &root_commit).await;
    app.cleanup().await;
}
