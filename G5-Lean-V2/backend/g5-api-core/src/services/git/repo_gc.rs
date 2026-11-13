//! Repo GC (git pack-maintenance) background task.
//!
//! The API writes git objects two ways and neither one ever packs: in-process
//! **libgit2** commits/merges (`GitRepo::commit`, used by workspace/doc/ontology/
//! merge paths) and the **`git receive-pack`** subprocess on worker push. The
//! workspace seed scripts additionally set `git config gc.auto 0` and
//! `maintenance.auto false`, so git's own automatic gc never runs. As a result
//! a repository accumulates **loose objects** indefinitely.
//!
//! Serving a clone (`git upload-pack`) of a repo with thousands of loose
//! objects over the NFS Filestore mount requires a separate filesystem
//! round-trip per object, which becomes slow enough to exceed the worker's
//! clone timeout (observed at ~4610 loose objects / 0 packfiles). This task
//! periodically repacks repos (`git gc`) so loose objects are consolidated into
//! packfiles and clones stay fast.
//!
//! This task is spawned by `MaintenanceRunner::spawn_all()` in `mod.rs` and runs
//! only under `G5_ROLE=daemon` and `G5_ROLE=all` (not `api`). Each sweep is
//! guarded by a Postgres advisory lock so two `daemon` replicas never sweep
//! concurrently. Within a sweep, repos are repacked **one at a time** to bound
//! NFS IOPS.
//!
//! `repos` is a tenant table under forced row-level security with no system
//! access path, so the system role sees none of its rows (G5Labs/G5-Lean#1074
//! found every sweep scanning zero repos). The sweep therefore enumerates
//! organizations on the system pool and reads each organization's repos as the
//! maintenance actor through [`scoped_db_for_maintenance_org`], under the same
//! row rules as any tenant caller, as the other tenant-table maintenance jobs
//! do.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sqlx::{Connection, PgConnection};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::MaintenanceConfig;
use crate::db::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};
use crate::errors::AppError;
use crate::services::git::repo_store::{RepoKey, TenantRepoStorage};
use g5_scm::native::{
    FailureEffect, NativeCancellation, NativeGit, NativeGitError, ObjectMaintenance,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Advisory lock key — must be globally unique within the database and distinct
/// from every other maintenance task's key (e.g. `lease_reaper`'s `0x4C52_5052`).
/// Constructed from the ASCII values of "RGCP" (Repo GC Pack-maintenance).
///
/// Exposed (`pub`) so the DB-backed maintenance integration tests can hold the
/// same session-scoped advisory lock on a separate connection to exercise the
/// multi-replica guard (a sweep must skip when the lock is already held).
pub const ADVISORY_LOCK_KEY: i64 = 0x5247_4350;

/// Per-git-subprocess timeout. Generous so it only fires on a true NFS hang,
/// not a legitimately slow `gc` on a large repo; on expiry the child is killed
/// and reaped by the shared runner so a wedged repo can never stall the sweep, hold the
/// advisory lock, or block daemon shutdown.
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(600);

// ---------------------------------------------------------------------------
// Public API — repo GC loop
// ---------------------------------------------------------------------------

/// Runs the periodic git pack-maintenance (repo GC) loop.
///
/// Each advisory-locked sweep enumerates every organization's repos as the
/// maintenance actor, counts loose objects, and runs `git gc` on any repo
/// at/above `repo_gc_loose_threshold`, one repo at a time.
///
/// Designed to be spawned as an independent Tokio task by
/// `MaintenanceRunner::spawn_all()`. The loop respects the cancellation token
/// for graceful shutdown. Per-organization and per-repo errors are logged but
/// never abort the sweep or crash the task.
pub async fn run_repo_gc(
    deps: TenantMaintenanceDeps,
    git_bin: PathBuf,
    config: MaintenanceConfig,
    cancel: CancellationToken,
) {
    let mut interval = tokio::time::interval(config.repo_gc_interval);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match do_repo_gc_sweep(
                    &deps,
                    &git_bin,
                    config.repo_gc_loose_threshold,
                    &cancel,
                )
                .await
                {
                    Ok(stats) => {
                        // Always visible at info: a sweep that enumerates no
                        // repos was silent for months (#1074).
                        tracing::info!(
                            orgs_scanned = stats.orgs_scanned,
                            repos_scanned = stats.repos_scanned,
                            repos_packed = stats.repos_packed,
                            "repo_gc: sweep completed"
                        );
                        if stats.orgs_scanned > 0
                            && stats.repos_scanned == 0
                            && !cancel.is_cancelled()
                        {
                            // Organization creation provisions a docs repo, so
                            // organizations without any visible repo most
                            // likely mean the maintenance scope cannot read
                            // `repos` (the #1074 failure mode). A sweep cut
                            // short by shutdown is not that signal.
                            tracing::warn!(
                                orgs_scanned = stats.orgs_scanned,
                                "repo_gc: organizations exist but no repos were visible to the sweep"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "repo_gc: sweep failed");
                    }
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("repo_gc: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Private — single sweep pass
// ---------------------------------------------------------------------------

/// Outcome of a single repo-GC sweep.
///
/// Exposed (`pub`) so the DB-backed maintenance integration tests can call
/// [`do_repo_gc_sweep`] directly and assert on enumeration/packing counts
/// without racing the loop's interval/cancel `select!`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SweepStats {
    /// Number of organizations whose repos were enumerated this sweep.
    pub orgs_scanned: u64,
    /// Number of repos enumerated and inspected this sweep.
    pub repos_scanned: u64,
    /// Number of repos that were at/above threshold and were repacked.
    pub repos_packed: u64,
}

/// Performs one advisory-locked repo-GC sweep.
///
/// Lists organizations on the system pool, then acquires `pg_try_advisory_lock`
/// on a dedicated system-role session; if another replica holds it, the sweep
/// is skipped (debug-logged). Enumerates each organization's `repos` as the
/// maintenance actor, then for each repo (sequentially) resolves its on-disk
/// path, counts loose objects, and runs `git gc` if the loose count is at/above
/// the threshold. The advisory lock is always released before returning.
///
/// Exposed (`pub`) so the DB-backed maintenance integration tests can drive one
/// deterministic sweep without racing the loop's interval/cancel `select!`.
pub async fn do_repo_gc_sweep(
    deps: &TenantMaintenanceDeps,
    git_bin: &Path,
    loose_threshold: u64,
    cancel: &CancellationToken,
) -> Result<SweepStats, AppError> {
    // Resolve the organization list BEFORE taking the lock session, as the
    // sibling jobs do: the maintenance runner starts several lock holders at
    // once, and a held lock connection plus a second `system_pool` acquire can
    // exhaust a small pool.
    let org_ids = maintenance_org_ids(&deps.system_pool).await?;

    // Multi-replica guard on a dedicated connection rather than a pooled slot,
    // so no `system_pool` connection is reserved across a sweep's NFS work.
    // Skip if another replica is already sweeping.
    let mut lock_conn = PgConnection::connect_with(&deps.system_pool.connect_options()).await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(ADVISORY_LOCK_KEY)
        .fetch_one(&mut lock_conn)
        .await?;

    if !acquired {
        tracing::debug!("repo_gc: advisory lock held by another replica — skipping sweep");
        let _ = lock_conn.close().await;
        return Ok(SweepStats::default());
    }

    // Wrapped so the advisory lock is always released even if enumeration
    // fails.
    let sweep_result = run_sweep_locked(deps, org_ids, git_bin, loose_threshold, cancel).await;

    // Always release the advisory lock, then close the session. Closing alone
    // would release it too, but explicitly so the next sweep is never refused
    // by a session the server has not yet torn down.
    let unlocked: Result<bool, sqlx::Error> = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(ADVISORY_LOCK_KEY)
        .fetch_one(&mut lock_conn)
        .await;
    let _ = lock_conn.close().await;
    // The sweep's own error is the informative one; report it first.
    let stats = sweep_result?;
    unlocked?;
    Ok(stats)
}

/// Runs the enumerate-and-pack body while the advisory lock is held.
async fn run_sweep_locked(
    deps: &TenantMaintenanceDeps,
    org_ids: Vec<Uuid>,
    git_bin: &Path,
    loose_threshold: u64,
    cancel: &CancellationToken,
) -> Result<SweepStats, AppError> {
    let mut stats = SweepStats::default();
    let mut repo_ids: Vec<RepoKey> = Vec::new();
    for org_id in org_ids {
        if cancel.is_cancelled() {
            tracing::info!("repo_gc: sweep interrupted by shutdown");
            return Ok(stats);
        }
        match org_repo_ids(deps, org_id).await {
            Ok(ids) => {
                stats.orgs_scanned += 1;
                repo_ids.extend(ids);
            }
            // One organization's failure never hides the others' repos.
            Err(error) => {
                tracing::warn!(%org_id, %error, "repo_gc: organization skipped after error")
            }
        }
    }
    stats.repos_scanned = repo_ids.len() as u64;
    let repo_storage = &deps.repo_storage;

    let native_cancel = NativeCancellation::default();
    let runner = match NativeGit::api(git_bin, GIT_COMMAND_TIMEOUT) {
        Ok(runner) => runner.with_cancellation(native_cancel.clone()),
        Err(error) => {
            tracing::warn!(%error, "repo_gc: cannot initialize Git execution");
            return Ok(stats);
        }
    };

    // One repo at a time to bound NFS IOPS.
    for key in repo_ids {
        // Stay responsive to shutdown between repos — a sweep can be long.
        if cancel.is_cancelled() {
            tracing::info!("repo_gc: sweep interrupted by shutdown");
            break;
        }
        let repo_id = key.repo_id();
        let operation = gc_repo_if_needed(repo_storage, &runner, &key, loose_threshold);
        tokio::pin!(operation);
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                native_cancel.cancel();
                // Await the active operation so child cleanup precedes lock release.
                operation.await
            }
            result = &mut operation => result,
        };
        match result {
            Ok(true) => stats.repos_packed += 1,
            Ok(false) => {}
            Err(e) => {
                // Per-repo failures never abort the sweep.
                tracing::warn!(
                    repo_id = %repo_id,
                    error = %e,
                    "repo_gc: repo skipped after error"
                );
            }
        }
    }

    Ok(stats)
}

/// Reads one organization's repositories as the maintenance actor, under the
/// same row rules as any tenant caller, and keys them inside that scope. The
/// scoped transaction closes before any `git gc` runs so no tenant connection
/// is held across slow NFS work.
async fn org_repo_ids(
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
) -> Result<Vec<RepoKey>, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, org_id).await?;
    let ids = sqlx::query_scalar::<_, Uuid>("SELECT id FROM repos WHERE org_id = $1 ORDER BY id")
        .bind(org_id)
        .fetch_all(db.conn())
        .await?;
    let keys = ids
        .into_iter()
        .map(|repo_id| TenantRepoStorage::key(&db, org_id, repo_id))
        .collect::<Result<Vec<_>, _>>()?;
    db.commit().await?;
    Ok(keys)
}

// ---------------------------------------------------------------------------
// Private — per-repo helper
// ---------------------------------------------------------------------------

/// The storage owner selects the repository; SCM owns layout and GC mechanics.
async fn gc_repo_if_needed(
    repo_storage: &TenantRepoStorage,
    runner: &NativeGit,
    key: &RepoKey,
    loose_threshold: u64,
) -> Result<bool, NativeGitError> {
    let repo_id = key.repo_id();
    let Ok(repo_path) = repo_storage.path(key) else {
        tracing::debug!(repo_id = %repo_id, "repo_gc: no local path — skipping");
        return Ok(false);
    };
    let started = Instant::now();
    // Partial is emitted by this operation only after GC succeeded and its
    // final inspection failed. Preserve that completion in the sweep count.
    let outcome = match runner.collect_garbage(&repo_path, loose_threshold).await {
        Ok(outcome) => outcome,
        Err(error) if error.effect == FailureEffect::Partial => {
            tracing::warn!(repo_id = %repo_id, %error,
                "repo_gc: collection completed; final inspection unavailable");
            return Ok(true);
        }
        Err(error) => return Err(error),
    };
    match outcome {
        ObjectMaintenance::NotMaterialized => {
            tracing::debug!(repo_id = %repo_id, path = %repo_path.display(), "repo_gc: repo not materialized locally — skipping");
            Ok(false)
        }
        ObjectMaintenance::BelowThreshold { loose_objects } => {
            tracing::debug!(repo_id = %repo_id, loose = loose_objects, threshold = loose_threshold, "repo_gc: below threshold — skipping");
            Ok(false)
        }
        ObjectMaintenance::Collected {
            loose_before,
            loose_after,
        } => {
            tracing::info!(repo_id = %repo_id, loose_before, loose_after,
                duration_ms = started.elapsed().as_millis() as u64, "repo_gc: repacked repo");
            Ok(true)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    const GIT_SUBPROCESS_PATH: &str = "/usr/bin:/usr/local/bin";

    async fn count_loose_objects(git_bin: &Path, repo_path: &Path) -> Result<u64, NativeGitError> {
        match NativeGit::api(git_bin, GIT_COMMAND_TIMEOUT)?
            .collect_garbage(repo_path, u64::MAX)
            .await?
        {
            ObjectMaintenance::BelowThreshold { loose_objects } => Ok(loose_objects),
            other => panic!("expected a materialized repository below threshold, got {other:?}"),
        }
    }

    /// Resolves a usable `git` binary for tests (mirrors `which_git`).
    fn test_git_bin() -> PathBuf {
        if let Ok(bin) = std::env::var("G5_GIT_BIN") {
            return PathBuf::from(bin);
        }
        if let Ok(out) = StdCommand::new("which").arg("git").output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return PathBuf::from(p);
                }
            }
        }
        PathBuf::from("/usr/bin/git")
    }

    /// Initializes a real non-bare git repo in `dir` with `commits` commits,
    /// each touching a distinct file so every commit produces fresh loose
    /// objects (blob + tree + commit). Auto-gc is disabled so nothing packs
    /// implicitly, matching the seeded-repo configuration in production.
    fn init_repo_with_commits(git_bin: &Path, dir: &Path, commits: usize) {
        let run = |args: &[&str]| {
            let status = StdCommand::new(git_bin)
                .args(["-c", "safe.directory=*"])
                .arg("-C")
                .arg(dir)
                .args(args)
                .env_clear()
                .env("PATH", GIT_SUBPROCESS_PATH)
                .env("GIT_AUTHOR_NAME", "test")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "test")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .status()
                .expect("failed to run git");
            assert!(status.success(), "git {args:?} failed");
        };

        run(&["init", "-q"]);
        // Disable auto-gc so commits never pack implicitly (matches seed config).
        run(&["config", "gc.auto", "0"]);
        run(&["config", "maintenance.auto", "false"]);

        for i in 0..commits {
            let file = dir.join(format!("file_{i}.txt"));
            std::fs::write(&file, format!("content {i}\n")).expect("write file");
            run(&["add", "."]);
            run(&["commit", "-q", "-m", &format!("commit {i}")]);
        }
    }

    /// Returns the number of packfiles in `<dir>/.git/objects/pack` (counting
    /// `*.pack` files).
    fn packfile_count(dir: &Path) -> usize {
        let pack_dir = dir.join(".git/objects/pack");
        let Ok(entries) = std::fs::read_dir(&pack_dir) else {
            return 0;
        };
        entries
            .filter_map(Result::ok)
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "pack")
                    .unwrap_or(false)
            })
            .count()
    }

    #[tokio::test]
    async fn count_objects_reflects_loose_objects() {
        let git_bin = test_git_bin();
        let tmp = tempfile::tempdir().expect("tempdir");
        init_repo_with_commits(&git_bin, tmp.path(), 5);

        let loose = count_loose_objects(&git_bin, tmp.path())
            .await
            .expect("count loose objects");

        // 5 commits, each adding a new file → blob + tree + commit per commit.
        // The exact number varies slightly by git version, but it must be well
        // above zero and there must be no packfiles yet.
        assert!(loose > 0, "expected loose objects, got {loose}");
        assert_eq!(packfile_count(tmp.path()), 0, "no packfiles before gc");
    }

    #[tokio::test]
    async fn gc_runs_when_loose_count_at_or_above_threshold() {
        let git_bin = test_git_bin();
        let tmp = tempfile::tempdir().expect("tempdir");
        init_repo_with_commits(&git_bin, tmp.path(), 8);

        let loose_before = count_loose_objects(&git_bin, tmp.path())
            .await
            .expect("count before");
        assert!(loose_before > 0);
        assert_eq!(packfile_count(tmp.path()), 0, "no packfiles before gc");

        // Threshold at/below loose count → gc must run and pack the objects.
        let packed = gc_repo_via_path(&git_bin, tmp.path(), 1)
            .await
            .expect("gc helper");
        assert!(packed, "repo should have been repacked");

        let loose_after = count_loose_objects(&git_bin, tmp.path())
            .await
            .expect("count after");
        assert!(
            loose_after < loose_before,
            "loose count should drop after gc: before={loose_before} after={loose_after}"
        );
        assert!(
            packfile_count(tmp.path()) >= 1,
            "a packfile should exist after gc"
        );
    }

    #[tokio::test]
    async fn gc_skipped_when_below_threshold() {
        let git_bin = test_git_bin();
        let tmp = tempfile::tempdir().expect("tempdir");
        init_repo_with_commits(&git_bin, tmp.path(), 3);

        let loose = count_loose_objects(&git_bin, tmp.path())
            .await
            .expect("count");
        assert!(loose > 0);

        // Threshold above the loose count → gc must be skipped (no packfile).
        let packed = gc_repo_via_path(&git_bin, tmp.path(), loose + 1)
            .await
            .expect("gc helper");
        assert!(!packed, "repo should NOT have been repacked");
        assert_eq!(
            packfile_count(tmp.path()),
            0,
            "no packfile should be created when below threshold"
        );
    }

    #[tokio::test]
    async fn missing_git_dir_is_skipped() {
        let git_bin = test_git_bin();
        let tmp = tempfile::tempdir().expect("tempdir");
        // Empty dir, no `.git` — must be skipped, not error.
        let packed = gc_repo_via_path(&git_bin, tmp.path(), 1)
            .await
            .expect("missing .git should be a skip, not an error");
        assert!(!packed);
    }

    #[tokio::test]
    async fn bare_repo_is_processed_not_skipped() {
        // A bare repo's path IS the git dir (no `.git` child); it must still be
        // recognized and processed, not skipped as "not materialized".
        let git_bin = test_git_bin();
        let tmp = tempfile::tempdir().expect("tempdir");
        let bare = tmp.path().join("bare.git");
        std::fs::create_dir_all(&bare).expect("mkdir");
        let status = StdCommand::new(&git_bin)
            .args(["-c", "safe.directory=*", "-C"])
            .arg(&bare)
            .args(["init", "--bare", "-q"])
            .env_clear()
            .env("PATH", GIT_SUBPROCESS_PATH)
            .status()
            .expect("git init --bare");
        assert!(status.success());
        // threshold 0 forces the gc branch; on an empty bare repo `git gc` is a
        // no-op that still succeeds — proving the bare repo was NOT skipped at
        // the materialization check (the regression codex caught).
        let packed = gc_repo_via_path(&git_bin, &bare, 0)
            .await
            .expect("gc helper on bare repo");
        assert!(
            packed,
            "bare repo should be processed (gc attempted), not skipped"
        );
    }

    // -- test helpers that exercise the real per-repo gc path -----------------

    /// Thin wrapper that drives `gc_repo_if_needed` against a fixed on-disk
    /// path, so the threshold branch can be tested without a DB.
    async fn gc_repo_via_path(
        git_bin: &Path,
        repo_path: &Path,
        loose_threshold: u64,
    ) -> Result<bool, NativeGitError> {
        // The fixture repository is reached through the guarded store, at its
        // keyed location under a throwaway base.
        let base = tempfile::tempdir().expect("storage base");
        let storage = TenantRepoStorage::local_for_test(base.path().to_path_buf());
        let key = TenantRepoStorage::test_key(Uuid::new_v4(), Uuid::new_v4());
        let keyed = storage.path(&key).expect("keyed path");
        if repo_path.exists() {
            std::fs::create_dir_all(keyed.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(repo_path, &keyed).unwrap();
        }
        let runner = NativeGit::api(git_bin, GIT_COMMAND_TIMEOUT)?;
        gc_repo_if_needed(&storage, &runner, &key, loose_threshold).await
    }
}
