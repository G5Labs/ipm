//! Integration tests for `GET .../sync_runs` and `POST .../sync_runs/retry`
//! after the G1 envelope extension.
//!
//! Verifies:
//! - The list endpoint returns the new branch tracking summary
//!   (`workspaceId`, `repos[].syncPausedReason`, `repos[].trackingMode`,
//!   `repos[].latestRunStatus`, advisory columns) plus the existing paged
//!   `runs` block.
//! - The retry endpoint returns 409 when the latest run status is
//!   non-retryable (e.g. `noop`) and 200 with outcome `reenqueued` when
//!   the latest is `auth_required` / `push_failed` / `fetch_failed`.
//!
//! Source artifacts:
//! - [`designs/REMOTE_REPO_TRACKING.md`](../../designs/REMOTE_REPO_TRACKING.md) §9.3
//! - [`remote_repo_tracking.arch_view.md`](../../views/arch_views/remote_repo_tracking.arch_view.md) D34, D37
//! - [`remote_tracking_gap_closure.plan.md`](../../aux_files/future_implementation/remote_tracking_gap_closure.plan.md) Phase G1
//! - Chat: `remote-tracking-gap-closure-plan-2026-05-28` (C2)

use crate::helpers;

use g5_contracts::types::WorkspaceRepoKey;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use serde_json::Value;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a freshly-created workspace into a remote-tracking shape: take
/// the fixture's existing `workspace_branch_repos` row and flip it to
/// `tracking_mode = 'remote_tracking'` with the required nullable
/// columns set. Returns `(workspace_branch_id, workspace_branch_repo_id)`.
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

async fn insert_sync_run(
    app: &TestApp,
    org_id: Uuid,
    key: WorkspaceRepoKey,
    acting_user_id: Uuid,
    status: &str,
    trigger_kind: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs
            (org_id, workspace_id, repo_id, acting_user_id,
             kind, trigger_kind, status, started_at)
        VALUES ($1, $2, $3, $4, 'integration', $5, $6, now())
        "#,
    )
    .bind(org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(acting_user_id)
    .bind(trigger_kind)
    .bind(status)
    .execute(&app.db)
    .await
    .expect("insert branch_sync_runs row");
}

// ---------------------------------------------------------------------------
// G1-1: list_branch_sync_runs returns branch tracking summary alongside paged runs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_sync_runs_returns_branch_tracking_summary() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let key = enable_remote(&app, &workspace, "main").await;

    insert_sync_run(&app, org.id, key, user.id, "noop", "commit").await;
    insert_sync_run(&app, org.id, key, user.id, "imported", "background_inbound").await;
    insert_sync_run(&app, org.id, key, user.id, "push_failed", "commit").await;

    let client = app.authenticated_client(&user);
    let resp = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/sync_runs",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.expect("json body");
    let data = &body["data"];

    // Branch summary
    assert_eq!(
        data["branch"]["workspaceId"].as_str(),
        Some(workspace.id.to_string().as_str())
    );
    let repos = data["branch"]["repos"]
        .as_array()
        .expect("repos array present");
    assert_eq!(repos.len(), 1, "single child row for fixture workspace");
    let repo = &repos[0];
    assert_eq!(repo["trackingMode"].as_str(), Some("remote_tracking"));
    assert_eq!(repo["remoteBranch"].as_str(), Some("main"));
    assert_eq!(
        repo["baselineCommit"].as_str(),
        Some(workspace.head_commit.as_str())
    );
    // No pause reason set in this scenario.
    assert!(
        repo.get("syncPausedReason").is_none() || repo["syncPausedReason"].is_null(),
        "syncPausedReason must be absent or null when not paused; got {:?}",
        repo.get("syncPausedReason")
    );
    // Latest run status reflects the newest insert.
    assert_eq!(repo["latestRunStatus"].as_str(), Some("push_failed"));

    // Paged runs
    let runs = data["runs"]["data"].as_array().expect("runs.data array");
    assert_eq!(runs.len(), 3, "expected three audit rows");

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// G1-2: syncPausedReason surfaces when the worker has paused the branch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_sync_runs_surfaces_sync_paused_reason() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let key = enable_remote(&app, &workspace, "main").await;

    sqlx::query(
        r#"
        UPDATE remote_integrations
        SET sync_paused_reason = 'diverged'
        WHERE workspace_id = $1 AND repo_id = $2
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .execute(&app.db)
    .await
    .expect("set pause reason");

    insert_sync_run(&app, org.id, key, user.id, "diverged", "commit").await;

    let client = app.authenticated_client(&user);
    let resp = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/sync_runs",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.expect("json body");
    let repo = &body["data"]["branch"]["repos"][0];
    assert_eq!(repo["syncPausedReason"].as_str(), Some("diverged"));
    assert_eq!(repo["latestRunStatus"].as_str(), Some("diverged"));

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// G1-3: retry returns 409 when the latest run status is non-retryable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_sync_runs_returns_409_for_non_retryable_last_status() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let key = enable_remote(&app, &workspace, "main").await;

    // `noop` is a terminal-success status — not retryable.
    insert_sync_run(&app, org.id, key, user.id, "noop", "background_inbound").await;

    let client = app.authenticated_client(&user);
    let resp = client
        .post_json::<Value>(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/sync_runs/retry",
                workspace.id, workspace.repo_id
            ),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(
        resp.status(),
        409,
        "non-retryable last status must surface 409"
    );

    let body: Value = resp.json().await.expect("json body");
    assert_eq!(body["data"]["outcome"].as_str(), Some("non_retryable"));

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// G1-4: retry returns 200 + enqueued when the latest run status is retryable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_sync_runs_reenqueues_when_last_status_is_auth_required() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let key = enable_remote(&app, &workspace, "main").await;

    insert_sync_run(&app, org.id, key, user.id, "auth_required", "commit").await;
    // Reflect reality: an auth_required cycle PAUSES the child
    // (`set_sync_paused_reason(..., "auth_required")`). The retry must resume a
    // paused branch — clearing the pause so the worker re-attempts (D3).
    sqlx::query(
        "UPDATE remote_integrations SET sync_paused_reason = 'auth_required' WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(key.workspace_id).bind(key.repo_id)
    .execute(&app.db)
    .await
    .expect("pause the child auth_required");

    let client = app.authenticated_client(&user);
    let resp = client
        .post_json::<Value>(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/sync_runs/retry",
                workspace.id, workspace.repo_id
            ),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.expect("json body");
    assert_eq!(body["data"]["outcome"].as_str(), Some("reenqueued"));
    assert_eq!(body["data"]["enqueued"].as_i64(), Some(1));

    // Queue row landed.
    let queued: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM remote_integration_queue
        WHERE workspace_id = $1 AND repo_id = $2
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("count queue rows");
    assert_eq!(
        queued, 1,
        "retry must UPSERT one branch_integration_queue row"
    );

    // D3: the retryable pause MUST be cleared so the next worker cycle runs a
    // fresh attempt (a still-paused row is drained silently, never resuming).
    let paused_after: Option<String> =
        sqlx::query_scalar("SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2")
            .bind(key.workspace_id).bind(key.repo_id)
            .fetch_one(&app.db)
            .await
            .expect("read sync_paused_reason");
    assert!(
        paused_after.is_none(),
        "retry must clear the auth_required pause so the worker re-attempts; got {paused_after:?}"
    );

    app.cleanup().await;
}
