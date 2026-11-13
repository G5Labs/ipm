//! The steering schema upgrade preserves populated inboxes under FORCE RLS.
//! Validation runs after the schema transaction and permits concurrent writes.
use crate::helpers;

use helpers::TestDb;
use sqlx::PgPool;
use std::path::Path;
use std::process::Output;
use tokio::process::Command;
use uuid::Uuid;

async fn migrate(database_url: &str, source: &Path) -> Result<Output, sqlx::Error> {
    let binary = std::env::var_os("NEXTEST_BIN_EXE_g5_db_migrate")
        .or_else(|| std::env::var_os("NEXTEST_BIN_EXE_g5-db-migrate"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_g5-db-migrate").into());
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        Command::new(binary)
            .env("DATABASE_URL", database_url)
            .env("G5_MIGRATION_SOURCE", source)
            .env("G5_MIGRATION_DATABASE_MAX_CONNECTIONS", "2")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| {
        sqlx::Error::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "steering migration blocked concurrent writes",
        ))
    })?
    .map_err(sqlx::Error::Io)
}

async fn seed_inbox(pool: &PgPool) -> Uuid {
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let workspace = Uuid::new_v4();
    let session = Uuid::new_v4();
    let input = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Steering upgrade',$2)")
        .bind(org)
        .bind(format!("steer-{}", org.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Steering owner')")
        .bind(owner)
        .bind(format!("steer-{owner}@example.test"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,'steering-upgrade',$3,$3)")
        .bind(workspace).bind(org).bind(owner).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
        .bind(session)
        .bind(org)
        .bind(workspace)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO session_inputs(id,org_id,session_id,input_index,content,input_origin) VALUES($1,$2,$3,0,'preserved input','human')")
        .bind(input).bind(org).bind(session).execute(pool).await.unwrap();
    input
}

async fn validated(pool: &PgPool) -> bool {
    sqlx::query_scalar("SELECT convalidated FROM pg_constraint WHERE conrelid='public.session_inputs'::regclass AND conname='session_inputs_reservation_shape'")
        .fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn populated_steering_upgrade_validates_after_commit_without_blocking_writes() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry
                .file_name()
                .to_string_lossy()
                .split('_')
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                < 84
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let first = seed_inbox(&db.pool).await;
    let second = seed_inbox(&db.pool).await;
    let role = format!("steering_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'steering_migration_test' NOSUPERUSER NOBYPASSRLS;
        GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};
        GRANT ALL ON public._sqlx_migrations TO {role};
        ALTER TABLE public.session_inputs OWNER TO {role}; ALTER TABLE public.chat_messages OWNER TO {role};"))
        .execute(&db.pool).await.unwrap();
    let mut url = url::Url::parse(&db.system_url).unwrap();
    url.set_username(&role).unwrap();
    url.set_password(Some("steering_migration_test")).unwrap();
    let probe: Result<(), sqlx::Error> = async {
        let migrator = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect_with(
                db.pool
                    .connect_options()
                    .as_ref()
                    .clone()
                    .username(&role)
                    .password("steering_migration_test"),
            )
            .await?;
        let privileges: (bool, bool) =
            sqlx::query_as("SELECT rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user")
                .fetch_one(&migrator)
                .await?;
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM session_inputs")
            .fetch_one(&migrator)
            .await?;
        migrator.close().await;
        assert_eq!(privileges, (false, false));
        assert_eq!(visible, 0, "tenant rows are hidden from the migration role");
        let name = "084_claude_native_steering.sql";
        std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();
        let result = migrate(url.as_str(), catalog.path()).await?;
        assert!(result.status.success(), "schema upgrade: {result:?}");
        assert!(
            !validated(&db.pool).await,
            "084 commits before the existing-row scan"
        );
        assert!(
            sqlx::query("UPDATE session_inputs SET reservation_id=gen_random_uuid() WHERE id=$1")
                .bind(first)
                .execute(&db.pool)
                .await
                .is_err(),
            "NOT VALID still checks new writes"
        );
        let mut writer = db.pool.begin().await?;
        sqlx::query("UPDATE session_inputs SET content='concurrent write' WHERE id=$1")
            .bind(first)
            .execute(&mut *writer)
            .await?;
        let name = "085_validate_claude_steering_input_shape.sql";
        std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();
        // The open UPDATE holds ROW EXCLUSIVE until after migration 085.
        // An ACCESS EXCLUSIVE validation scan would wait until the deadline.
        let result = migrate(url.as_str(), catalog.path()).await?;
        assert!(
            result.status.success(),
            "validation coexists with the writer: {result:?}"
        );
        assert!(validated(&db.pool).await);
        writer.commit().await?;
        let replay = migrate(url.as_str(), catalog.path()).await?;
        assert!(replay.status.success(), "migration replay: {replay:?}");
        let rows: Vec<(Uuid, String, String, bool)> =
            sqlx::query_as("SELECT id,content,delivery,steered FROM session_inputs ORDER BY id")
                .fetch_all(&db.pool)
                .await?;
        assert_eq!(rows.len(), 2);
        for (id, content, delivery, steered) in rows {
            assert!(id == first || id == second);
            assert_eq!(
                content,
                if id == first {
                    "concurrent write"
                } else {
                    "preserved input"
                }
            );
            assert_eq!(delivery, "queued");
            assert!(!steered);
        }
        let applied: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM _sqlx_migrations WHERE version IN (84,85) AND success",
        )
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(applied, 2);
        Ok(())
    }
    .await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    probe.expect("populated upgrade through the production migrator");
    db.drop().await;
}
