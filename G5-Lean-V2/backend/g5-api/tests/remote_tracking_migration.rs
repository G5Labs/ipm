//! Remote policies, queue admission and exact attempt evidence use member keys.
use crate::helpers;

use g5_contracts::types::WorkspaceRepoKey;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};

async fn fixture() -> (TestApp, TestUser, TestWorkspace, WorkspaceRepoKey) {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let key = WorkspaceRepoKey {
        org_id: org.id,
        workspace_id: workspace.id,
        repo_id: workspace.repo_id,
    };
    (app, user, workspace, key)
}

async fn add_policy(app: &TestApp, key: WorkspaceRepoKey, branch: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO remote_integrations (org_id, workspace_id, repo_id, remote_branch) VALUES ($1, $2, $3, $4)")
        .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).bind(branch).execute(&app.db).await?;
    Ok(())
}

#[tokio::test]
async fn policy_needs_no_cached_git_head_or_baseline() {
    let (app, _, _, key) = fixture().await;
    add_policy(&app, key, "feature/remote").await.unwrap();
    let branch: String = sqlx::query_scalar(
        "SELECT remote_branch FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(branch, "feature/remote");
    app.cleanup().await;
}

#[tokio::test]
async fn policy_rejects_missing_or_invalid_remote_ref() {
    let (app, _, _, key) = fixture().await;
    for branch in ["", "bad..branch", "refs/heads/a.lock", "with space"] {
        let error = add_policy(&app, key, branch).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
    }
    let error = sqlx::query(
        "INSERT INTO remote_integrations (org_id, workspace_id, repo_id) VALUES ($1, $2, $3)",
    )
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .execute(&app.db)
    .await
    .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23502")
    );
    app.cleanup().await;
}

#[tokio::test]
async fn member_has_at_most_one_remote_policy() {
    let (app, _, _, key) = fixture().await;
    add_policy(&app, key, "main").await.unwrap();
    let error = add_policy(&app, key, "feature/other").await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
    app.cleanup().await;
}

#[tokio::test]
async fn sync_attempt_enum_constraints_are_retained() {
    let (app, user, _, key) = fixture().await;
    for (kind, trigger, status, valid) in [
        ("integration", "commit", "noop", true),
        ("advisory", "ping", "advisory_updated", true),
        ("invalid", "commit", "noop", false),
        ("integration", "invalid", "noop", false),
        ("integration", "commit", "invalid", false),
    ] {
        let result = sqlx::query("INSERT INTO remote_sync_runs (org_id, workspace_id, repo_id, acting_user_id, kind, trigger_kind, status) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).bind(user.id)
            .bind(kind).bind(trigger).bind(status).execute(&app.db).await;
        assert_eq!(
            result.is_ok(),
            valid,
            "{kind}/{trigger}/{status}: {result:?}"
        );
        if !valid {
            assert_eq!(
                result
                    .unwrap_err()
                    .as_database_error()
                    .unwrap()
                    .code()
                    .as_deref(),
                Some("23514")
            );
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn queue_coalesces_without_demoting_explicit_push() {
    let (app, user, _, key) = fixture().await;
    add_policy(&app, key, "main").await.unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    for (trigger, expected) in [
        ("background_inbound", "background_inbound"),
        ("commit", "commit"),
        ("push", "push"),
        ("commit", "push"),
        ("manual", "push"),
    ] {
        use g5_api_data::repositories::branch_integration_queue::{
            enqueue_authorized_branch_integration, enqueue_branch_integration, PushAuthority,
        };
        if trigger == "push" {
            let authority = PushAuthority::capture(&mut conn, key.workspace_id, user.id)
                .await
                .unwrap();
            enqueue_authorized_branch_integration(
                &mut conn,
                key,
                trigger,
                chrono::Utc::now(),
                authority,
            )
            .await
            .unwrap();
        } else {
            enqueue_branch_integration(&mut conn, key, trigger)
                .await
                .unwrap();
        }
        let rows: Vec<String> = sqlx::query_scalar("SELECT trigger_kind FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2")
            .bind(key.workspace_id).bind(key.repo_id).fetch_all(&mut *conn).await.unwrap();
        assert_eq!(rows, vec![expected]);
    }
    drop(conn);
    app.cleanup().await;
}

#[tokio::test]
async fn queue_rejects_advisory_only_triggers() {
    let (app, _, _, key) = fixture().await;
    add_policy(&app, key, "main").await.unwrap();
    for trigger in ["ping", "webhook"] {
        let error = sqlx::query("INSERT INTO remote_integration_queue (org_id, workspace_id, repo_id, trigger_kind) VALUES ($1, $2, $3, $4)")
            .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).bind(trigger).execute(&app.db).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn several_workspaces_can_track_the_same_remote_ref() {
    let (app, user, workspace, key) = fixture().await;
    add_policy(&app, key, "main").await.unwrap();
    let peer = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, org_id, name, owner_user_id, created_by) VALUES ($1, $2, $3, $4, $4)")
        .bind(peer).bind(key.org_id).bind(format!("peer-{peer}")).bind(user.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path) SELECT org_id, $2, repo_id, role, root_path FROM workspace_repos WHERE workspace_id = $1")
        .bind(workspace.id).bind(peer).execute(&app.db).await.unwrap();
    add_policy(
        &app,
        WorkspaceRepoKey {
            workspace_id: peer,
            ..key
        },
        "main",
    )
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM remote_integrations WHERE repo_id = $1 AND remote_branch = 'main'",
    )
    .bind(key.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 2);
    app.cleanup().await;
}
