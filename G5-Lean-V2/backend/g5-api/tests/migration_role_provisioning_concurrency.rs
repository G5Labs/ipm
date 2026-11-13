//! Cross-process regression for cluster-global role provisioning during migrations.
//!
//! PostgreSQL advisory locks are local to one database. Two migrators targeting
//! different databases therefore coordinate through a dedicated connection to
//! the shared `postgres` database, not through their target databases.

use crate::helpers;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use tempfile::TempDir;
use url::Url;
use uuid::Uuid;

const CHILD_MODE_ENV: &str = "G5_MIGRATION_ROLE_RACE_CHILD";
const CHILD_DATABASE_URL_ENV: &str = "G5_MIGRATION_ROLE_RACE_DATABASE_URL";
const CHILD_MIGRATION_PATH_ENV: &str = "G5_MIGRATION_ROLE_RACE_MIGRATION_PATH";
const CHILD_READY_PATH_ENV: &str = "G5_MIGRATION_ROLE_RACE_READY_PATH";
const CHILD_START_PATH_ENV: &str = "G5_MIGRATION_ROLE_RACE_START_PATH";
const MIGRATOR_PASSWORD: &str = "migration_role_race_password";

#[tokio::test]
async fn concurrent_fresh_database_migrations_serialize_cluster_role_creation() {
    let base_url = TestDb::resolve_base_url().await;
    let admin_url = database_url(&base_url, "postgres", None, None);
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_url)
        .await
        .expect("connect to the shared postgres database");

    let suffix = Uuid::new_v4().simple().to_string();
    let migrator_role = format!("mrole_migrator_{suffix}");
    let provisioned_role = format!("mrole_target_{suffix}");
    let database_a = format!("g5_mrole_a_{suffix}");
    let database_b = format!("g5_mrole_b_{suffix}");

    sqlx::raw_sql(&format!(
        "CREATE ROLE {migrator_role} LOGIN PASSWORD '{MIGRATOR_PASSWORD}' \
             NOSUPERUSER NOCREATEDB CREATEROLE INHERIT NOBYPASSRLS;"
    ))
    .execute(&admin_pool)
    .await
    .expect("create restricted migration role");

    for database in [&database_a, &database_b] {
        sqlx::query(&format!(
            "CREATE DATABASE \"{database}\" OWNER {migrator_role}"
        ))
        .execute(&admin_pool)
        .await
        .expect("create empty database owned by the restricted migrator");
    }

    let migrator_can_connect_to_postgres: bool =
        sqlx::query_scalar("SELECT has_database_privilege($1, 'postgres', 'CONNECT')")
            .bind(&migrator_role)
            .fetch_one(&admin_pool)
            .await
            .expect("inspect restricted migrator access to the coordination database");
    assert!(
        migrator_can_connect_to_postgres,
        "the supported migration runner fails closed unless the migrator can connect to postgres"
    );

    let migration_dir = TempDir::new().expect("create isolated migration directory");
    std::fs::write(
        migration_dir.path().join("001_cluster_role_probe.sql"),
        format!(
            r#"
DO $migration$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{provisioned_role}') THEN
        -- Widen the exact migration-017 check-to-create window so both child
        -- processes deterministically observe the absent cluster-global role.
        PERFORM pg_sleep(1);
        CREATE ROLE {provisioned_role} NOLOGIN;
    END IF;
END
$migration$;
"#
        ),
    )
    .expect("write isolated cluster-role migration");

    let coordination_dir = TempDir::new().expect("create child coordination directory");
    let start_path = coordination_dir.path().join("start");
    let ready_a = coordination_dir.path().join("ready-a");
    let ready_b = coordination_dir.path().join("ready-b");
    let child_exe = std::env::current_exe().expect("resolve current integration-test executable");

    let target_a = database_url(
        &base_url,
        &database_a,
        Some(&migrator_role),
        Some(MIGRATOR_PASSWORD),
    );
    let target_b = database_url(
        &base_url,
        &database_b,
        Some(&migrator_role),
        Some(MIGRATOR_PASSWORD),
    );

    let child_a = spawn_migration_child(
        &child_exe,
        &target_a,
        migration_dir.path(),
        &ready_a,
        &start_path,
    );
    let child_b = spawn_migration_child(
        &child_exe,
        &target_b,
        migration_dir.path(),
        &ready_b,
        &start_path,
    );

    wait_for_children_ready([&ready_a, &ready_b]);
    std::fs::write(&start_path, b"start").expect("release both migration children");

    let output_a = child_a
        .wait_with_output()
        .expect("wait for first migration child");
    let output_b = child_b
        .wait_with_output()
        .expect("wait for second migration child");

    let role_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_roles WHERE rolname = $1")
        .bind(&provisioned_role)
        .fetch_one(&admin_pool)
        .await
        .expect("count the cluster-global fixture role");

    cleanup_fixture(
        &admin_pool,
        [&database_a, &database_b],
        &provisioned_role,
        &migrator_role,
    )
    .await;
    admin_pool.close().await;

    assert_child_succeeded("first", &output_a);
    assert_child_succeeded("second", &output_b);
    assert_eq!(
        role_count, 1,
        "both target databases must converge on exactly one cluster-global role"
    );
}

#[test]
fn migration_role_race_child_process() {
    if std::env::var(CHILD_MODE_ENV).as_deref() != Ok("1") {
        return;
    }

    let database_url =
        std::env::var(CHILD_DATABASE_URL_ENV).expect("child database URL must be provided");
    let migration_path = PathBuf::from(
        std::env::var(CHILD_MIGRATION_PATH_ENV).expect("child migration path must be provided"),
    );
    let ready_path = PathBuf::from(
        std::env::var(CHILD_READY_PATH_ENV).expect("child ready path must be provided"),
    );
    let start_path = PathBuf::from(
        std::env::var(CHILD_START_PATH_ENV).expect("child start path must be provided"),
    );

    std::fs::write(&ready_path, b"ready").expect("announce child readiness");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !start_path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the parent to release migration children"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build child Tokio runtime");
    runtime.block_on(async move {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .expect("connect child to its empty target database");
        let result = g5_api::db::run_migrations_from_path(&pool, &migration_path).await;
        pool.close().await;
        result.expect("cluster-role migration should complete");
    });
}

fn spawn_migration_child(
    executable: &Path,
    database_url: &str,
    migration_path: &Path,
    ready_path: &Path,
    start_path: &Path,
) -> std::process::Child {
    Command::new(executable)
        .arg("--exact")
        .arg(helpers::exact_test_name(
            module_path!(),
            "migration_role_race_child_process",
        ))
        .arg("--nocapture")
        .env(CHILD_MODE_ENV, "1")
        .env(CHILD_DATABASE_URL_ENV, database_url)
        .env(CHILD_MIGRATION_PATH_ENV, migration_path)
        .env(CHILD_READY_PATH_ENV, ready_path)
        .env(CHILD_START_PATH_ENV, start_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn migration child process")
}

fn wait_for_children_ready(paths: [&Path; 2]) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !paths.iter().all(|path| path.exists()) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for both migration children to become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn assert_child_succeeded(label: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{label} migration child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn database_url(
    base_url: &str,
    database: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> String {
    let mut url = Url::parse(base_url).expect("parse integration-test database URL");
    url.set_path(&format!("/{database}"));
    if let Some(username) = username {
        url.set_username(username)
            .expect("set restricted migrator username");
    }
    if let Some(password) = password {
        url.set_password(Some(password))
            .expect("set restricted migrator password");
    }
    url.to_string()
}

async fn cleanup_fixture(
    admin_pool: &sqlx::PgPool,
    databases: [&str; 2],
    provisioned_role: &str,
    migrator_role: &str,
) {
    for database in databases {
        sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1")
            .bind(database)
            .execute(admin_pool)
            .await
            .expect("terminate fixture database connections");
        sqlx::query(&format!("DROP DATABASE IF EXISTS \"{database}\""))
            .execute(admin_pool)
            .await
            .expect("drop fixture database");
    }

    sqlx::query(&format!("DROP ROLE IF EXISTS {provisioned_role}"))
        .execute(admin_pool)
        .await
        .expect("drop provisioned fixture role");
    sqlx::query(&format!("DROP ROLE IF EXISTS {migrator_role}"))
        .execute(admin_pool)
        .await
        .expect("drop restricted migrator fixture role");
}
