//! Migration 086 through the Helm/CI migration executable (ontology
//! `g5.quality.migrations.verification`,
//! `g5.data.configuration.org-admin-settings`,
//! `g5.architecture.host_demand_capacity`).
//!
//! 086 registers `min_free_slots` (0–100) in the `configs` validator and adds
//! `worker_hosts.capacity_floor_claim` with a metadata-only default. It is
//! schema only, so the real deployment migrator (NOSUPERUSER, NOBYPASSRLS,
//! owner of what it alters) applies it over populated tenants, rewrites no
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

async fn seed_org(pool: &PgPool) -> Uuid {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Free-slot floor migration',$2)")
        .bind(org)
        .bind(format!("free-slot-floor-{}", org.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO configs(org_id,scope,scope_id,key,value) VALUES($1,'org',$1,'idle_host_retire_secs',$2)",
    )
    .bind(org)
    .bind(json!(1800))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,automatic_capacity)
         VALUES($1,$2,'us-central1','c3-standard-4','provisioning',true)",
    )
    .bind(Uuid::new_v4())
    .bind(org)
    .execute(pool)
    .await
    .unwrap();
    org
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
async fn upgrade_registers_the_free_slot_floor_and_the_claim_marker_without_rewriting_rows() {
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
            && version(&entry.file_name()) < 86
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    let name = "086_min_free_slots.sql";
    std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();

    let org = seed_org(&app.pool).await;
    assert!(
        !accepts(&app.pool, org, "min_free_slots", json!(1)).await,
        "the floor is not registered before 086"
    );

    let role = format!("free_slot_floor_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'free_slot_floor_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    // The deployment migrator owns the validator, the capacity guard,
    // `configs` and `worker_hosts`; FORCE ROW LEVEL SECURITY still hides
    // tenant rows.
    sqlx::raw_sql(&format!(
        "ALTER FUNCTION public.g5_config_value_is_registered(text, text, jsonb) OWNER TO {role}; \
         ALTER FUNCTION public.g5_guard_host_capacity() OWNER TO {role}; \
         ALTER TABLE public.configs OWNER TO {role}; \
         ALTER TABLE public.worker_hosts OWNER TO {role}"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("free_slot_floor_migration_test"))
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
                    .password("free_slot_floor_migration_test"),
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
        "the production executable applies 086 as a non-BYPASSRLS owner: {migration:?}",
    );
    let replay = replay.unwrap();
    assert!(
        replay.status.success(),
        "replaying the completed migration is a no-op: {replay:?}"
    );
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 86 AND success")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(applied, 1, "replay records the migration exactly once");

    // No row was rewritten or seeded: the stored delay is untouched, no floor
    // is stored, and the existing automatic host is not a floor claim.
    let delay: Value = sqlx::query_scalar(
        "SELECT value FROM configs WHERE org_id = $1 AND key = 'idle_host_retire_secs'",
    )
    .bind(org)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(delay, json!(1800));
    let floor_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM configs WHERE key = 'min_free_slots'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(floor_rows, 0);
    let (hosts, floor_claims, presence): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE capacity_floor_claim),
                count(*) FILTER (WHERE capacity_floor_presence)
         FROM worker_hosts WHERE org_id = $1",
    )
    .bind(org)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!((hosts, floor_claims, presence), (1, 0, 0));

    // The replaced capacity guard keeps the manual cross-region stockout
    // retry off floor claims, and still schedules plain hosts.
    let floor_claim = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,automatic_capacity,capacity_floor_claim)
         VALUES($1,$2,'us-central1','c3-standard-4','provisioning',true,true)",
    )
    .bind(floor_claim)
    .bind(org)
    .execute(&app.pool)
    .await
    .unwrap();
    let scheduled: (Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "WITH f AS (UPDATE worker_hosts SET provision_next_retry_at=now() WHERE host_id=$1
                        RETURNING provision_next_retry_at),
                  p AS (UPDATE worker_hosts SET provision_next_retry_at=now() WHERE org_id=$2
                        AND NOT capacity_floor_claim RETURNING provision_next_retry_at)
             SELECT (SELECT provision_next_retry_at FROM f), (SELECT provision_next_retry_at FROM p)",
        )
        .bind(floor_claim)
        .bind(org)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert!(scheduled.0.is_none(), "a floor claim is never scheduled");
    assert!(scheduled.1.is_some(), "a plain host keeps its schedule");

    // The new bounds hold for every later write; older keys keep their rules.
    for (key, value, expected) in [
        ("min_free_slots", json!(-1), false),
        ("min_free_slots", json!(0), true),
        ("min_free_slots", json!(1), true),
        ("min_free_slots", json!(100), true),
        ("min_free_slots", json!(101), false),
        ("min_free_slots", json!(1.5), false),
        ("min_free_slots", json!("1"), false),
        ("idle_host_retire_secs", json!(899), false),
        ("idle_host_retire_secs", json!(900), true),
        ("max_running_vms", json!(100), true),
        ("max_running_vms", json!(101), false),
        ("auto_add_hosts", json!(false), true),
    ] {
        assert_eq!(
            accepts(&app.pool, org, key, value.clone()).await,
            expected,
            "{key} = {value}"
        );
    }
    assert!(!accepts(&app.pool, org, "unregistered_key", json!(1)).await);

    app.drop().await;
}
