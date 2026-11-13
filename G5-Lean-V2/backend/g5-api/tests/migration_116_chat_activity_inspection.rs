//! Migration 116 (chat `g5.chat.inspection.acknowledge`): an existing chat's
//! inspection floor is the rollout time, so its historical results count as
//! inspected, while a chat created afterwards gets its creation time. Applied
//! twice as a non-superuser migrator, like the deployed runner.

use crate::helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const MIGRATION: &str = "116_chat_activity_inspection.sql";

async fn insert_chat(db: &TestDb, org: Uuid, workspace: Uuid, owner: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions (id, org_id, workspace_id, created_by, created_at, updated_at)
         VALUES ($1, $2, $3, $4, now() - interval '3 days', now() - interval '3 days')",
    )
    .bind(id)
    .bind(org)
    .bind(workspace)
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn existing_chats_get_the_rollout_floor_and_new_chats_their_creation() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "116"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Owner')")
        .bind(owner)
        .bind(format!("owner-{owner}@example.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $2)")
        .bind(org)
        .bind(format!("org-{}", org.simple()))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org)
        .bind(owner)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, owner_user_id, created_by)
         VALUES ($1, $2, 'Chats', $3, $3)",
    )
    .bind(workspace)
    .bind(org)
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    let existing = insert_chat(&db, org, workspace, owner).await;

    let role = format!("chat_activity_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
        GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
        GRANT USAGE, CREATE ON SCHEMA public TO {role};
        GRANT USAGE ON SCHEMA authz TO {role};
        GRANT ALL ON public._sqlx_migrations TO {role};
        GRANT REFERENCES ON public.users TO {role};
        GRANT g5_tenant, g5_app TO {role} WITH INHERIT FALSE, SET FALSE;
        ALTER TABLE public.chat_sessions OWNER TO {role};
        ALTER TABLE public.tasks OWNER TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    let migrator = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            db.pool
                .connect_options()
                .as_ref()
                .clone()
                .username(&role)
                .password("migration_test"),
        )
        .await
        .unwrap();
    let before_migration: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    for _ in 0..2 {
        g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap();
    }

    let (created, floor): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT created_at, inspection_floor_at FROM chat_sessions WHERE id = $1")
            .bind(existing)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(
        floor >= before_migration,
        "an existing chat's floor is the rollout time, not its creation ({created})"
    );

    let fresh = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions (id, org_id, workspace_id, created_by)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(fresh)
    .bind(org)
    .bind(workspace)
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    let (created, floor): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT created_at, inspection_floor_at FROM chat_sessions WHERE id = $1")
            .bind(fresh)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(floor, created, "a new chat's floor is its creation");

    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class WHERE oid = 'public.chat_session_inspections'::regclass",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(forced);
    let can_set: bool = sqlx::query_scalar("SELECT pg_has_role($1, 'authz_lookup_definer', 'SET')")
        .bind(&role)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(!can_set);
    db.drop().await;
}
