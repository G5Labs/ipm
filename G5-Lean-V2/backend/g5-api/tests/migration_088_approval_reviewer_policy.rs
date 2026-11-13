//! Real upgrade: immutable pre-policy definitions retain all-reviewer semantics;
//! subsequently inserted versions receive any-reviewer/self-off defaults.
use crate::helpers;
use helpers::TestDb;
use std::path::Path;
use uuid::Uuid;

#[tokio::test]
async fn upgrade_preserves_old_versions_and_defaults_only_new_versions() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let old_catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "101_"
        {
            std::fs::copy(entry.path(), old_catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(old_catalog.path()).await;
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    let definition = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Policy upgrade',$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Reviewer')")
        .bind(user)
        .bind(format!("{user}@g5.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(user)
        .execute(&db.pool)
        .await
        .unwrap();
    for version in [1, 2] {
        sqlx::query("INSERT INTO approval_definitions(id,version,org_id,name,humans_list,created_by) VALUES($1,$2,$3,'Existing policy',ARRAY[$4]::uuid[],$4)")
            .bind(definition).bind(version).bind(org).bind(user).execute(&db.pool).await.unwrap();
    }
    sqlx::migrate::Migrator::new(source.as_path())
        .await
        .unwrap()
        .run(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO approval_definitions(id,version,org_id,name,humans_list,created_by) VALUES($1,3,$2,'New policy',ARRAY[$3]::uuid[],$3)")
        .bind(definition).bind(org).bind(user).execute(&db.pool).await.unwrap();
    let versions: Vec<(i32, bool, bool, bool, String)> = sqlx::query_as("SELECT version,require_all_reviewers,allow_self_approval,human_approvals_override_agent,name FROM approval_definitions WHERE id=$1 ORDER BY version")
        .bind(definition).fetch_all(&db.pool).await.unwrap();
    assert_eq!(
        versions,
        vec![
            (1, true, false, true, "Existing policy".into()),
            (2, true, false, true, "Existing policy".into()),
            (3, false, false, true, "New policy".into())
        ]
    );
    db.drop().await;
}
