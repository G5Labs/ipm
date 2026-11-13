//! Membership is unique; temporary Git transport refs require no database rows.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestWorkspace};

#[tokio::test]
async fn membership_is_unique_and_transport_refs_are_git_only() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let duplicate = sqlx::query(
        "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path)
         SELECT org_id, workspace_id, repo_id, role, root_path FROM workspace_repos
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace.id)
    .bind(workspace.repo_id)
    .execute(&app.db)
    .await
    .expect_err("one workspace cut cannot contain the same repository twice");
    assert_eq!(
        duplicate.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
    let path = app.repo_path(workspace.repo_id).await;
    let repo = git2::Repository::open(path).unwrap();
    let tip = git2::Oid::from_str(&workspace.head_commit).unwrap();
    repo.reference(
        "refs/g5/resolutions/fixture/remote",
        tip,
        false,
        "temporary operation pin",
    )
    .expect("Git transport refs need no database branch row");
    assert_eq!(
        repo.find_reference("refs/g5/resolutions/fixture/remote")
            .unwrap()
            .target(),
        Some(tip)
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace.id)
    .bind(workspace.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
    app.cleanup().await;
}
