//! Migration 083 through the Helm/CI migration executable (ontology
//! `g5.quality.migrations.verification`,
//! `g5.data.configuration.org-admin-settings`).
//!
//! 083 replaces the validator behind `configs_registered_key_value`: it
//! registers `default_idle_timeout_secs` (60–86400) and narrows
//! `max_running_vms` to 0–100, and registers the managed-capacity keys. It is
//! schema only, so the real deployment
//! migrator (NOSUPERUSER, NOBYPASSRLS, owner of the function it replaces)
//! applies it over populated tenants without the owner exemption, rewrites no
//! row, and replays as a no-op.

use crate::helpers;

use helpers::TestDb;
use serde_json::{json, Value};
use sqlx::PgPool;
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

async fn seed_org(pool: &PgPool, quota: Value) -> Uuid {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Admin settings migration',$2)")
        .bind(org)
        .bind(format!("admin-settings-{}", org.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'max_running_vms',$2)",
    )
    .bind(org)
    .bind(quota)
    .execute(pool)
    .await
    .unwrap();
    org
}

async fn quota(pool: &PgPool, org: Uuid) -> Value {
    sqlx::query_scalar("SELECT value FROM configs WHERE org_id = $1 AND key = 'max_running_vms'")
        .bind(org)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn accepts(pool: &PgPool, org: Uuid, key: &str, value: Value) -> bool {
    sqlx::query(
        "INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,$2,$3)
         ON CONFLICT (org_id,scope,scope_id,key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(org)
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .is_ok()
}

#[tokio::test]
async fn upgrade_registers_the_idle_default_and_narrows_the_quota_without_rewriting_rows() {
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
            && version(&entry.file_name()) < 83
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    let name = "083_org_admin_settings.sql";
    std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();

    // Two tenants: one on the seeded default, one on a quota 003 allowed but
    // 083 no longer does.
    let seeded = seed_org(&app.pool, json!(10)).await;
    let legacy = seed_org(&app.pool, json!(500)).await;
    assert!(
        !accepts(&app.pool, seeded, "default_idle_timeout_secs", json!(3600)).await,
        "the idle default is not registered before 083"
    );

    let role = format!("admin_settings_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'admin_settings_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    // The deployment migrator created the validator and `configs` in 003 and
    // owns both; FORCE ROW LEVEL SECURITY still hides every tenant row from it.
    sqlx::raw_sql(&format!(
        "ALTER FUNCTION public.g5_config_value_is_registered(text, text, jsonb) OWNER TO {role}; \
         ALTER TABLE public.configs OWNER TO {role}"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("admin_settings_migration_test"))
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
                    .password("admin_settings_migration_test"),
            )
            .await?;
        let result = async {
            let privileges: (bool, bool) = sqlx::query_as(
                "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&migrator_pool)
            .await?;
            let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM configs")
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
        "the production executable replaces the validator as a non-BYPASSRLS owner: {migration:?}",
    );
    let replay = replay.unwrap();
    assert!(
        replay.status.success(),
        "replaying the completed migration is a no-op: {replay:?}"
    );
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 83 AND success")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(applied, 1, "replay records the migration exactly once");

    // No row was rewritten or seeded (the owner chose no data migration).
    assert_eq!(quota(&app.pool, seeded).await, json!(10));
    assert_eq!(quota(&app.pool, legacy).await, json!(500));
    let idle_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM configs WHERE key = 'default_idle_timeout_secs'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(idle_rows, 0);

    // The new bounds hold for every later write.
    for (key, value, expected) in [
        ("default_idle_timeout_secs", json!(59), false),
        ("default_idle_timeout_secs", json!(60), true),
        ("default_idle_timeout_secs", json!(86400), true),
        ("default_idle_timeout_secs", json!(86401), false),
        ("default_idle_timeout_secs", json!(600.5), false),
        ("default_idle_timeout_secs", json!("600"), false),
        ("max_running_vms", json!(-1), false),
        ("max_running_vms", json!(0), true),
        ("max_running_vms", json!(100), true),
        ("max_running_vms", json!(101), false),
        ("auto_add_hosts", json!(false), true),
        ("auto_add_hosts", json!("false"), false),
        ("auto_retire_idle_hosts", json!(true), true),
        ("auto_retire_idle_hosts", json!(1), false),
        ("auto_host_instance_type", json!("c3-standard-8"), true),
        (
            "auto_host_instance_type",
            json!("n2-standard-8/global/networks/x"),
            false,
        ),
        ("auto_host_instance_type", json!(8), false),
        ("idle_host_retire_secs", json!(899), false),
        ("idle_host_retire_secs", json!(900), true),
        ("idle_host_retire_secs", json!(86400), true),
        ("idle_host_retire_secs", json!(86401), false),
    ] {
        assert_eq!(
            accepts(&app.pool, seeded, key, value.clone()).await,
            expected,
            "{key} = {value}"
        );
    }
    // Other registered keys keep their rules.
    assert!(accepts(&app.pool, seeded, "worker_host_region", json!("us-east1")).await);
    assert!(!accepts(&app.pool, seeded, "unregistered_key", json!(1)).await);

    app.drop().await;
}
