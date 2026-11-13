//! Integration tests for `GET .../branches/:branchId/merge_runs` (G2 of
//! the remote-tracking gap-closure plan).
//!
//! Verifies:
//!   - Active filter (default) returns waiting_for_human / running rows
//!     with `source_kind = 'remote_tracking'` and the new
//!     `sourceHeads`, `targetHeads`, `phase`, `repoId`,
//!     `startedAt` fields populated.
//!   - Terminal-state rows are excluded by default and included when
//!     `?status=all`.
//!   - Unknown status values return 400.
//!
//! SOT:
//! - [`remote_repo_tracking.arch_view.md`](../../views/arch_views/remote_repo_tracking.arch_view.md) D22, D23, D55
//! - [`remote_tracking_gap_closure.plan.md`](../../aux_files/future_implementation/remote_tracking_gap_closure.plan.md) Phase G2
//! - Chat: `remote-tracking-gap-closure-plan-2026-05-28` (C6)

use crate::helpers;

use g5_contracts::types::WorkspaceRepoKey;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use serde_json::{json, Value};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn enable_remote(
    app: &TestApp,
    workspace: &TestWorkspace,
    remote_branch: &str,
) -> WorkspaceRepoKey {
    let key = WorkspaceRepoKey {
        org_id: workspace.org_id,
        workspace_id: workspace.id,
        repo_id: workspace.repo_id,
    };
    sqlx::query("INSERT INTO remote_integrations (org_id,workspace_id,repo_id,remote_branch,last_sync_at) VALUES ($1,$2,$3,$4,now())")
        .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).bind(remote_branch).execute(&app.db).await.unwrap();
    let path = app.repo_path(key.repo_id).await;
    git2::Repository::open(path)
        .unwrap()
        .reference(
            &format!("refs/g5-tracking/{}/baseline", workspace.name),
            git2::Oid::from_str(&workspace.head_commit).unwrap(),
            true,
            "fixture watermark",
        )
        .unwrap();
    key
}

#[allow(clippy::too_many_arguments)]
async fn insert_merge_run(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    remote_branch: &str,
    workspace_tip: &str,
    mirror_tip: &str,
    baseline: &str,
    status: &str,
) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO workspace_merge_runs
            (org_id, workspace_id, source_kind,
             source_workspace_id, target_workspace_id, repo_id,
             source_branch_name, status, phase, source_heads, target_heads,
             pre_start_source_heads, started_at)
        VALUES ($1, $2, 'remote_tracking', NULL, NULL, $3, $4, $5,
                'workspace_rebase_preflight', $6, $7, $6, now())
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(repo_id)
    .bind(remote_branch)
    .bind(status)
    .bind(json!({ "workspace_tip": workspace_tip, "mirror_tip": mirror_tip }))
    .bind(json!({ "baseline": baseline }))
    .fetch_one(&app.db)
    .await
    .expect("insert merge run");
    id
}

// ---------------------------------------------------------------------------
// G2-1: active filter returns waiting_for_human rows with new fields populated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_merge_runs_active_returns_remote_tracking_rows() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let key = enable_remote(&app, &workspace, "main").await;

    let merge_run_id = insert_merge_run(
        &app,
        org.id,
        workspace.id,
        key.repo_id,
        "main",
        "aaaaaaaaaa",
        "bbbbbbbbbb",
        "00base",
        "waiting_for_human",
    )
    .await;

    let client = app.authenticated_client(&user);
    let resp = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/merge_runs",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.expect("json");
    let items = body["data"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "exactly one active run for the branch");

    let run = &items[0];
    assert_eq!(run["id"].as_str(), Some(merge_run_id.to_string().as_str()));
    assert_eq!(run["sourceKind"].as_str(), Some("remote_tracking"));
    assert_eq!(run["status"].as_str(), Some("waiting_for_human"));
    assert_eq!(run["phase"].as_str(), Some("workspace_rebase_preflight"));
    assert_eq!(
        run["repoId"].as_str(),
        Some(key.repo_id.to_string().as_str())
    );
    assert_eq!(run["sourceBranchName"].as_str(), Some("main"));
    assert_eq!(
        run["sourceHeads"]["workspace_tip"].as_str(),
        Some("aaaaaaaaaa")
    );
    assert_eq!(
        run["sourceHeads"]["mirror_tip"].as_str(),
        Some("bbbbbbbbbb")
    );
    assert_eq!(run["targetHeads"]["baseline"].as_str(), Some("00base"));
    assert!(run["startedAt"].is_string(), "startedAt present");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// G2-2: terminal-state rows excluded by default; included with status=all
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_merge_runs_status_all_includes_terminal_rows() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let key = enable_remote(&app, &workspace, "main").await;

    let _active = insert_merge_run(
        &app,
        org.id,
        workspace.id,
        key.repo_id,
        "main",
        "aaaa",
        "bbbb",
        "00base",
        "waiting_for_human",
    )
    .await;
    // Need to drop the active row to satisfy the unique-active-per-wbr
    // index before inserting the second one.
    sqlx::query("UPDATE workspace_merge_runs SET status = 'completed' WHERE id = $1")
        .bind(_active)
        .execute(&app.db)
        .await
        .expect("close first run");

    let _terminal = insert_merge_run(
        &app,
        org.id,
        workspace.id,
        key.repo_id,
        "main",
        "cccc",
        "dddd",
        "00base",
        "cancelled",
    )
    .await;

    let client = app.authenticated_client(&user);

    let active_resp = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/merge_runs",
            workspace.id, workspace.repo_id
        ))
        .await;
    let active_body: Value = active_resp.json().await.expect("json");
    let active_items = active_body["data"]["items"].as_array().expect("items");
    assert_eq!(
        active_items.len(),
        0,
        "no active runs after closing the only one"
    );

    let all_resp = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/merge_runs?status=all",
            workspace.id, workspace.repo_id
        ))
        .await;
    let all_body: Value = all_resp.json().await.expect("json");
    let all_items = all_body["data"]["items"].as_array().expect("items");
    assert_eq!(
        all_items.len(),
        2,
        "both terminal rows visible under status=all"
    );

    // Newest-first ordering: cancelled row (inserted second) first.
    assert_eq!(all_items[0]["status"].as_str(), Some("cancelled"));
    assert_eq!(all_items[1]["status"].as_str(), Some("completed"));

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// G2-3: unknown status filter is rejected with 4xx
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_merge_runs_unknown_status_is_rejected() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    enable_remote(&app, &workspace, "main").await;

    let client = app.authenticated_client(&user);
    let resp = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/merge_runs?status=bogus",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert!(
        resp.status().is_client_error(),
        "unknown status MUST return 4xx, got {}",
        resp.status()
    );

    app.cleanup().await;
}
