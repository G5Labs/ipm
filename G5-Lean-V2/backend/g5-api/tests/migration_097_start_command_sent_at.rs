//! Migration 097 (`workspace_vms.start_command_sent_at`, G5-Lean#626) must
//! apply on a clean database and on one already migrated through main's 095,
//! and replay as a no-op. The upgrade path stages the catalog as main shipped
//! it (every migration before 097) and then runs the embedded migrator.

use crate::helpers::TestDb;

use std::path::Path;

const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/db/migrations");

async fn has_start_command_sent_at(pool: &sqlx::PgPool) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'workspace_vms'
           AND column_name = 'start_command_sent_at')",
    )
    .fetch_one(pool)
    .await
    .expect("read column catalog")
}

async fn applied_versions(pool: &sqlx::PgPool) -> Vec<i64> {
    sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
        .fetch_all(pool)
        .await
        .expect("read applied migrations")
}

/// Main's catalog: every migration before 097, in a scratch directory.
fn stage_main_catalog() -> tempfile::TempDir {
    let staged = tempfile::tempdir().expect("scratch catalog");
    for entry in std::fs::read_dir(Path::new(MIGRATIONS)).expect("list migrations") {
        let path = entry.expect("migration entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let version = name.split('_').next().and_then(|v| v.parse::<u32>().ok());
        if name.ends_with(".sql") && version.is_some_and(|v| v < 97) {
            std::fs::copy(&path, staged.path().join(name)).expect("stage migration");
        }
    }
    staged
}

#[tokio::test]
async fn migration_097_upgrades_a_database_migrated_through_main() {
    let staged = stage_main_catalog();
    let db = TestDb::create_from_migrations(staged.path()).await;
    let before = applied_versions(&db.pool).await;
    assert!(before.contains(&95), "main's catalog applied: {before:?}");
    assert!(!before.contains(&97));
    assert!(!has_start_command_sent_at(&db.pool).await);

    g5_api::db::run_migrations(&db.pool)
        .await
        .expect("apply 097 on top of main's catalog");
    assert!(has_start_command_sent_at(&db.pool).await);
    let after = applied_versions(&db.pool).await;
    assert_eq!(after.iter().filter(|v| **v == 97).count(), 1, "{after:?}");

    // A replay applies nothing.
    g5_api::db::run_migrations(&db.pool)
        .await
        .expect("replay is a no-op");
    assert_eq!(applied_versions(&db.pool).await, after);
    db.drop().await;
}

#[tokio::test]
async fn migration_097_applies_on_a_clean_database() {
    let db = TestDb::create().await;
    assert!(has_start_command_sent_at(&db.pool).await);
    assert!(applied_versions(&db.pool).await.contains(&97));
    db.drop().await;
}
