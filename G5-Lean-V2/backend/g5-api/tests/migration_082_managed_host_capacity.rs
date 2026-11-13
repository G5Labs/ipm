//! Populated capacity migration coverage through the Helm/CI migration executable
//! (ontology `g5.quality.migrations.verification`).
//!
//! Additive columns and compatibility guards must install with forced RLS and
//! no tenant visibility; existing hosts must start with unknown idle evidence.

use crate::helpers;

use helpers::TestDb;
use std::path::Path;
use std::process::Output;
use tokio::process::Command;
use uuid::Uuid;

async fn run_migration_command(database_url: &str, source: &Path) -> Result<Output, sqlx::Error> {
    // Nextest remaps executable paths when running a build archive.
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
            "g5-db-migrate exceeded the populated upgrade timeout",
        ))
    })?
    .map_err(sqlx::Error::Io)
}

#[tokio::test]
async fn upgrade_adds_managed_capacity_guards_without_bypassrls() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    let version = |name: &std::ffi::OsStr| -> u64 {
        name.to_string_lossy()
            .split('_')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && version(&entry.file_name()) < 82
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    std::fs::copy(
        source.join("082_managed_host_capacity.sql"),
        catalog.path().join("082_managed_host_capacity.sql"),
    )
    .unwrap();

    // An existing operator-provisioned host on the pre-082 schema.
    let org = Uuid::new_v4();
    let host = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Host capacity migration',$2)")
        .bind(org)
        .bind(format!("host-capacity-{}", org.simple()))
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,concurrent_jobs) \
         VALUES($1,$2,'us-central1','c3-standard-22','ready',5)",
    )
    .bind(host)
    .bind(org)
    .execute(&app.pool)
    .await
    .unwrap();

    let role = format!("host_capacity_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'host_capacity_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(&format!("ALTER TABLE public.worker_hosts OWNER TO {role}; ALTER TABLE public.workspace_vms OWNER TO {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("host_capacity_migration_test"))
        .unwrap();

    let probe: Result<_, sqlx::Error> = async {
        let migrator_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect_with(
                app.pool
                    .connect_options()
                    .as_ref()
                    .clone()
                    .username(&role)
                    .password("host_capacity_migration_test"),
            )
            .await?;
        let result = async {
            let privileges: (bool, bool) = sqlx::query_as(
                "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&migrator_pool)
            .await?;
            let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts")
                .fetch_one(&migrator_pool)
                .await?;
            let migration = run_migration_command(migration_url.as_str(), catalog.path()).await?;
            let replay = if migration.status.success() {
                Some(run_migration_command(migration_url.as_str(), catalog.path()).await?)
            } else {
                None
            };
            Ok::<_, sqlx::Error>((privileges, visible, migration, replay))
        }
        .await;
        migrator_pool.close().await;
        result
    }
    .await;
    // Clean the cluster-global test role even if a probe query fails.
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&app.pool)
    .await
    .unwrap();
    let (privileges, visible, migration, replay) = probe.expect("migration probe succeeds");
    assert_eq!(privileges, (false, false));
    assert_eq!(visible, 0, "runner starts without tenant row visibility");
    assert!(
        migration.status.success(),
        "the production executable adds the column as the non-BYPASSRLS table owner: {migration:?}",
    );
    let replay = replay.unwrap();
    assert!(
        replay.status.success(),
        "replaying the completed migrations is a no-op: {replay:?}"
    );
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 82 AND success")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(applied, 1, "replay records the migration exactly once");

    let row: (bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT idle_since IS NULL, NOT automatic_capacity, capacity_start_intent_id IS NULL,
            cloud_deleted_at IS NULL, cloud_delete_attempted_at IS NULL FROM worker_hosts WHERE host_id=$1",
    )
    .bind(host)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (true, true, true, true, true),
        "existing capacity starts with unknown idleness and manual origin"
    );
    let quota: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT (value #>> '{}')::bigint FROM configs
        WHERE org_id=$1 AND key='max_running_vms'),10)",
    )
    .bind(org)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(quota, 10, "migration does not change the VM quota");

    // The additive DDL must leave the tenant-isolation flags untouched.
    let forced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class WHERE relname = 'worker_hosts' \
         AND relrowsecurity AND relforcerowsecurity",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(forced, 1, "worker_hosts keeps FORCE ROW LEVEL SECURITY");

    app.drop().await;
}
