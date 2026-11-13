//! Final-state source guards for workspace-only Git authority.
//!
//! The former inventory in this file described the transitional task/job
//! branch writers. Those writers are deliberately absent after the Lean MVP
//! cutover, so the durable regression boundary is now the remaining workspace
//! landing path and the absence of private-branch execution entry points.

use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("g5-api must be nested under backend")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repository_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn rust_sources(root: &Path, output: &mut Vec<PathBuf>) {
    for entry in
        fs::read_dir(root).unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
    {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            rust_sources(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}

fn production_rust_source() -> String {
    let root = repository_root();
    let mut paths = Vec::new();
    for relative in [
        "backend/g5-api-core/src",
        "backend/g5-api-routes/src",
        "backend/g5d/src/cli",
        "backend/g5d-host/src",
    ] {
        rust_sources(&root.join(relative), &mut paths);
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| fs::read_to_string(path).expect("read production Rust source"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn retired_private_branch_execution_entry_points_are_absent() {
    let source = production_rust_source();
    for retired in [
        "initialize_job_fork",
        "materialize_captured_job_fork",
        "initialize_job_authority_members",
        "initialize_task_binding_from_configured_scope",
        "prepare_direct_clean_job_merge",
        "prepare_job_merge_start",
        "ready_merge_job_publication",
        "published_job_rollback",
        "run_worker_task_transition_with_checkpoint",
    ] {
        assert!(
            !source.contains(retired),
            "retired private-branch execution entry point remains: {retired}"
        );
    }
}

#[test]
fn workspace_merge_prepares_and_lands_complete_workspace_cuts() {
    let service = read("backend/g5-api-core/src/services/git/workspace_merge_service.rs");
    for retained in [
        "prepare_target_workspace_merge_application",
        "prepare_source_workspace_merge_application",
        "resolve_committed_snapshot",
        "prepare_branch_commit",
    ] {
        assert!(
            service.contains(retained),
            "workspace merge lost required exact-cut step {retained}"
        );
    }
    for retired in [
        "initialize_job_fork",
        "prepare_direct_clean_job_merge_application",
        "prepare_captured_ready_merge_job_publication",
        "prepare_published_job_rollback",
    ] {
        assert!(
            !service.contains(retired),
            "workspace merge service retained private-job path {retired}"
        );
    }
}

#[test]
fn workspace_landing_validates_and_restores_the_complete_git_cut() {
    let landing = read("backend/g5-api-core/src/services/git/branch_commit.rs");
    for required in [
        "acquire_workspace_commit_fence",
        "apply_member_ref_updates",
        "verify_members_against_inventory",
        "restore_applied_branch_refs",
        "SET CONSTRAINTS ALL IMMEDIATE",
    ] {
        assert!(
            landing.contains(required),
            "workspace landing lost required fenced step {required}"
        );
    }
    // Refs are never retracted after the fence is released
    // (`g5.git.landing.commit-point`): no restore follows a failed COMMIT.
    let commit_failure = &landing[landing
        .find("if let Err(commit_error) = db.commit().await")
        .expect("landing commits its database side")..];
    let commit_failure = &commit_failure[..commit_failure.find("let committed_at").unwrap()];
    assert!(!commit_failure.contains("restore_applied_branch_refs"));
}

#[test]
fn final_migration_removes_private_branch_authority_columns() {
    let migration = read("backend/g5-api/src/db/migrations/008_lean_mvp_execution_cutover.sql");
    for required in [
        "DROP COLUMN authority_scope_id",
        "DROP COLUMN authority_scope_kind",
        "DROP COLUMN job_id",
        "DROP COLUMN task_id",
    ] {
        assert!(
            migration.contains(required),
            "final migration must remove workspace_branches private authority: {required}"
        );
    }
}

#[test]
fn p4_8_markers_are_resolved_from_active_backend_source() {
    let source = production_rust_source();
    assert!(
        !source.contains("LEAN-TODO(P4.8)"),
        "active backend source still contains an unresolved P4.8 marker"
    );
}
