//! Final-schema and authorization contracts after the reset-only model cutover.
use crate::helpers;
use g5_api_core::services::git::repo_store::TestOrgScope;

use std::collections::BTreeMap;

use g5_api::services::{remote_integrations, workspace_repositories};
use g5_contracts::types::WorkspaceRepoKey;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use uuid::Uuid;

async fn parent(app: &TestApp, child: &TestWorkspace, owner: Uuid, is_base: bool) -> Uuid {
    let id = Uuid::new_v4();
    let name = if is_base {
        "Base".to_owned()
    } else {
        format!("parent-{id}")
    };
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by)
                 VALUES ($1,$2,$3,$4,$5,$5)",
    )
    .bind(id)
    .bind(child.org_id)
    .bind(&name)
    .bind(is_base)
    .bind(owner)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role)
                 SELECT org_id,$2,repo_id,root_path,role FROM workspace_repos WHERE workspace_id=$1")
        .bind(child.id).bind(id).execute(&app.db).await.unwrap();
    if !is_base {
        helpers::seed_branch_ref(app, child.repo_id, &name, &child.head_commit).await;
    }
    sqlx::query("UPDATE workspaces SET cloned_from_id=$2 WHERE id=$1")
        .bind(child.id)
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn schema_has_canonical_membership_and_no_current_git_registry() {
    let app = TestApp::spawn().await;
    for table in [
        "workspace_branches",
        "workspace_branch_repos",
        "branch_integration_queue",
        "branch_sync_runs",
    ] {
        let exists: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("public.{table}"))
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(exists, None, "legacy relation {table}");
    }
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
        WHERE table_schema='public' AND table_name='workspace_repos' ORDER BY column_name",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(
        columns,
        ["org_id", "repo_id", "role", "root_path", "workspace_id"]
    );
    let forbidden: Vec<(String, String)> = sqlx::query_as("SELECT table_name,column_name FROM information_schema.columns
        WHERE table_schema='public' AND table_name IN ('workspaces','workspace_repos','remote_integrations','tasks')
        AND column_name IN ('workspace_branch_id','workspace_branch_repo_id','base_branch','base_head_commit',
                            'diff_base_commit','head_commit','head_sha_cache','baseline_commit',
                            'oid','provider_oid','previous_oid','remote_tip_advisory_sha')")
        .fetch_all(&app.db).await.unwrap();
    assert!(
        forbidden.is_empty(),
        "unexpected Git registry: {forbidden:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn member_and_remote_policy_enforce_tenant_scope_and_cascade_together() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let other_org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &other_org).await;
    let invalid = sqlx::query(
        "INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch)
        VALUES ($1,$2,$3,'trunk')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(other.repo_id)
    .execute(&app.db)
    .await;
    assert!(
        invalid.is_err(),
        "a foreign repository cannot become integration policy"
    );
    sqlx::query(
        "INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch)
        VALUES ($1,$2,$3,'trunk')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(workspace.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let key = WorkspaceRepoKey {
        org_id: org.id,
        workspace_id: workspace.id,
        repo_id: workspace.repo_id,
    };
    let mut db = app.scoped_db_for_org(other_org.id).await;
    assert!(remote_integrations::load(db.conn(), key, false)
        .await
        .unwrap()
        .is_none());
    let forbidden =
        sqlx::query("UPDATE remote_integrations SET remote_branch='stolen' WHERE workspace_id=$1")
            .bind(workspace.id)
            .execute(db.conn())
            .await
            .unwrap();
    assert_eq!(forbidden.rows_affected(), 0);
    db.rollback().await.unwrap();
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
        .bind(workspace.id)
        .bind(workspace.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_integrations WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn ref_selection_uses_the_owned_branch_and_repository_default_without_overrides() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let child = TestWorkspace::create(&app, &org).await;
    let base = parent(&app, &child, user.id, true).await;
    let mut db = app.scoped_db_for_org(org.id).await;
    let working = workspace_repositories::load_workspace_repositories(db.conn(), child.id, false)
        .await
        .unwrap();
    assert_eq!(working[0].branch_name, child.name);
    let base_before = workspace_repositories::load_workspace_repositories(db.conn(), base, false)
        .await
        .unwrap();
    assert_eq!(base_before[0].branch_name, "main");
    db.rollback().await.unwrap();
    sqlx::query("UPDATE repos SET default_branch='trunk' WHERE id=$1")
        .bind(child.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let after = workspace_repositories::load_workspace_repositories(db.conn(), base, false)
        .await
        .unwrap();
    assert_eq!(after[0].branch_name, "trunk");
    db.rollback().await.unwrap();
    // A working workspace may be renamed (g5.workspace.rename); the branch it
    // owns is what ref selection uses and it never changes.
    sqlx::query("UPDATE workspaces SET name='renamed' WHERE id=$1")
        .bind(child.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let renamed = workspace_repositories::load_workspace_repositories(db.conn(), child.id, false)
        .await
        .unwrap();
    assert_eq!(renamed[0].branch_name, child.name);
    db.rollback().await.unwrap();
    assert!(
        sqlx::query("UPDATE workspaces SET branch_name='renamed' WHERE id=$1")
            .bind(child.id)
            .execute(&app.db)
            .await
            .is_err()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn parent_permission_and_lifecycle_never_silently_reparent_the_child() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let child = TestWorkspace::create(&app, &org).await;
    let parent_id = parent(&app, &child, owner.id, false).await;
    let mut db = app.scoped_db_for_user_org(reader.id, org.id).await;
    assert!(
        workspace_repositories::readable_parent_workspace(db.conn(), child.id, reader.id)
            .await
            .is_ok()
    );
    assert!(
        workspace_repositories::readable_parent_workspace(db.conn(), child.id, owner.id)
            .await
            .is_err(),
        "a reader cannot impersonate the parent owner"
    );
    db.rollback().await.unwrap();
    let mut db = app.scoped_db_for_user_org(owner.id, org.id).await;
    assert_eq!(
        workspace_repositories::readable_parent_workspace(db.conn(), child.id, owner.id)
            .await
            .unwrap(),
        parent_id
    );
    db.rollback().await.unwrap();
    assert!(
        sqlx::query("DELETE FROM workspaces WHERE id=$1")
            .bind(parent_id)
            .execute(&app.db)
            .await
            .is_err(),
        "parent hard deletion must not orphan a child"
    );
    sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
        .bind(parent_id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_user_org(owner.id, org.id).await;
    assert!(
        workspace_repositories::readable_parent_workspace(db.conn(), child.id, owner.id)
            .await
            .is_err()
    );
    db.rollback().await.unwrap();
    let retained: Option<Uuid> =
        sqlx::query_scalar("SELECT cloned_from_id FROM workspaces WHERE id=$1")
            .bind(child.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(retained, Some(parent_id));
    // Deleting the complete family together must not fail on the parent link.
    sqlx::query("DELETE FROM workspaces WHERE org_id=$1")
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn comparison_uses_captured_git_versions_and_requires_identical_membership() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let start = workspace.head_commit.clone();
    let captured_child = workspace.commit_file(&app, "a.txt", "child\n").await;
    workspace.commit_file(&app, "b.txt", "later\n").await;
    let source = BTreeMap::from([(workspace.repo_id, captured_child.clone())]);
    let parent = BTreeMap::from([(workspace.repo_id, start.clone())]);
    let result = workspace_repositories::comparison_bases(
        &app.repo_storage.scoped(&TestOrgScope(org.id)),
        org.id,
        &source,
        &parent,
    )
    .await
    .unwrap();
    assert_eq!(result[&workspace.repo_id], start);
    assert!(workspace_repositories::comparison_bases(
        &app.repo_storage.scoped(&TestOrgScope(org.id)),
        org.id,
        &source,
        &BTreeMap::new()
    )
    .await
    .is_err());
    // Advancing the selected parent changes the merge base; no persisted fork base wins.
    let advanced_parent = BTreeMap::from([(workspace.repo_id, captured_child.clone())]);
    let result = workspace_repositories::comparison_bases(
        &app.repo_storage.scoped(&TestOrgScope(org.id)),
        org.id,
        &source,
        &advanced_parent,
    )
    .await
    .unwrap();
    assert_eq!(result[&workspace.repo_id], captured_child);
    app.cleanup().await;
}

#[tokio::test]
async fn document_repository_is_optional_but_bound_ownership_cannot_cross_workspaces() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    sqlx::query("INSERT INTO docs(org_id,workspace_id,name,doc_type,content) VALUES ($1,$2,'Unbound','design','text')")
        .bind(org.id).bind(workspace.id).execute(&app.db).await.unwrap();
    assert!(sqlx::query(
        "INSERT INTO docs(org_id,workspace_id,repo_id,name,doc_type,file_path)
            VALUES ($1,$2,$3,'Wrong owner','design','docs/wrong.md')"
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(other.repo_id)
    .execute(&app.db)
    .await
    .is_err());
    app.cleanup().await;
}

#[tokio::test]
async fn captured_repository_timestamps_do_not_follow_later_branch_updates() {
    use g5_api::services::workspace_reader::CommittedRepoReadView;
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let path = app.repo_path(workspace.repo_id).await;
    let raw = git2::Repository::open(&path).unwrap();
    let original = raw
        .find_commit(git2::Oid::from_str(&workspace.head_commit).unwrap())
        .unwrap();
    let author = git2::Signature::new(
        "Author",
        "author@example.test",
        &git2::Time::new(1_600_000_000, 60),
    )
    .unwrap();
    let committer = git2::Signature::new(
        "Committer",
        "committer@example.test",
        &git2::Time::new(1_700_000_000, -60),
    )
    .unwrap();
    let reference = format!("refs/heads/{}", workspace.name);
    let first = raw
        .commit(
            Some(&reference),
            &author,
            &committer,
            "captured",
            &original.tree().unwrap(),
            &[&original],
        )
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let members =
        workspace_repositories::load_workspace_repositories(db.conn(), workspace.id, false)
            .await
            .unwrap();
    db.rollback().await.unwrap();
    let captured = workspace_repositories::read_repository_versions(
        &app.repo_storage.scoped(&TestOrgScope(org.id)),
        members,
    )
    .await
    .unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].head_commit, first.to_string());
    assert_eq!(captured[0].committed_at.timestamp(), 1_700_000_000);
    let view = CommittedRepoReadView::from_landed_head(
        &TestOrgScope(org.id),
        org.id,
        workspace.repo_id,
        &captured[0].head_commit,
    )
    .unwrap();
    let later_time = git2::Signature::new(
        "Committer",
        "committer@example.test",
        &git2::Time::new(1_800_000_000, 0),
    )
    .unwrap();
    raw.commit(
        Some(&reference),
        &author,
        &later_time,
        "later",
        &original.tree().unwrap(),
        &[&raw.find_commit(first).unwrap()],
    )
    .unwrap();
    assert_eq!(
        view.committed_at(&app.repo_storage)
            .await
            .unwrap()
            .timestamp(),
        1_700_000_000
    );
    let missing = CommittedRepoReadView::from_landed_head(
        &TestOrgScope(org.id),
        org.id,
        workspace.repo_id,
        &"f".repeat(40),
    )
    .unwrap();
    assert!(missing.committed_at(&app.repo_storage).await.is_err());
    let blob = raw.blob(b"not a commit").unwrap();
    let invalid = CommittedRepoReadView::from_landed_head(
        &TestOrgScope(org.id),
        org.id,
        workspace.repo_id,
        &blob.to_string(),
    )
    .unwrap();
    assert!(invalid.committed_at(&app.repo_storage).await.is_err());
    app.cleanup().await;
}

#[tokio::test]
async fn sql_parent_lookup_preserves_personal_privacy_and_current_admin_authority() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let reader =
        TestUser::create_with_scopes(&app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let personal = TestWorkspace::create_personal(&app, &org, owner.id).await;
    let child = TestWorkspace::create(&app, &org).await;
    // A retained registry edge must not bypass privacy even though the normal
    // fork API refuses a personal source before creating such an edge.
    sqlx::query("UPDATE workspaces SET cloned_from_id=$2 WHERE id=$1")
        .bind(child.id)
        .bind(personal.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_user_org(reader.id, org.id).await;
    assert!(
        workspace_repositories::readable_parent_workspace(db.conn(), child.id, reader.id)
            .await
            .is_err()
    );
    db.rollback().await.unwrap();
    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(reader.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_user_org(reader.id, org.id).await;
    assert_eq!(
        workspace_repositories::readable_parent_workspace(db.conn(), child.id, reader.id)
            .await
            .unwrap(),
        personal.id
    );
    db.rollback().await.unwrap();
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(reader.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_user_org(reader.id, org.id).await;
    assert!(
        workspace_repositories::readable_parent_workspace(db.conn(), child.id, reader.id)
            .await
            .is_err()
    );
    db.rollback().await.unwrap();
    app.cleanup().await;
}
