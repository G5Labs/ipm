//! Populated migration 069 coverage through the Helm/CI migration executable
//! (ontology `g5.quality.migrations.verification`).
//!
//! `worker_hosts` is a FORCE ROW LEVEL SECURITY table, so the real deployment
//! migrator — NOSUPERUSER and NOBYPASSRLS — sees zero of its rows. 069 is
//! additive DDL with no data section precisely so that cannot matter, and this
//! test is what shows it: an existing host row survives the upgrade with a NULL
//! override, the lower-bound CHECK is live afterwards, and replaying is a no-op.

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
async fn upgrade_adds_a_nullable_boot_disk_override_without_bypassrls() {
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
            && version(&entry.file_name()) < 69
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    std::fs::copy(
        source.join("069_worker_host_boot_disk.sql"),
        catalog.path().join("069_worker_host_boot_disk.sql"),
    )
    .unwrap();

    // An existing host on the pre-069 schema, i.e. one of the 150 GB hosts the
    // change exists to replace.
    let org = Uuid::new_v4();
    let host = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Boot disk migration',$2)")
        .bind(org)
        .bind(format!("boot-disk-{}", org.simple()))
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

    let role = format!("boot_disk_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'boot_disk_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(&format!("ALTER TABLE public.worker_hosts OWNER TO {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("boot_disk_migration_test"))
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
                    .password("boot_disk_migration_test"),
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
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 69 AND success")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(applied, 1, "replay records the migration exactly once");

    // The existing host keeps its budget and takes a NULL override, so its
    // replacement derives the size from the current rule rather than inheriting
    // a frozen one (g5.runtime.placement.host-disk).
    let row: (Option<i32>, Option<i32>) =
        sqlx::query_as("SELECT boot_disk_gb, concurrent_jobs FROM worker_hosts WHERE host_id = $1")
            .bind(host)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        row,
        (None, Some(5)),
        "an existing row takes a NULL override and keeps its pinned budget"
    );

    // The lower bound is a provider constraint (GCE cannot create a disk below
    // the source image), so it is enforced in SQL. The 2000 GB cost cap
    // deliberately is NOT: it lives only in WorkerHostBootDiskSizing, so
    // raising it stays a one-constant change and needs no second migration.
    let too_small = sqlx::query("UPDATE worker_hosts SET boot_disk_gb = 29 WHERE host_id = $1")
        .bind(host)
        .execute(&app.pool)
        .await;
    assert!(
        too_small.is_err(),
        "the CHECK must reject a disk smaller than the source image"
    );
    for accepted in [30_i32, 2_500_i32] {
        sqlx::query("UPDATE worker_hosts SET boot_disk_gb = $2 WHERE host_id = $1")
            .bind(host)
            .bind(accepted)
            .execute(&app.pool)
            .await
            .unwrap_or_else(|e| panic!("{accepted} GB must be storable: {e}"));
    }

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
