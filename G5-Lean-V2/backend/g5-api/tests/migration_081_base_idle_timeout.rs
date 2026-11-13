//! Populated migration 081 through the Helm/CI migration executable
//! (ontology `g5.quality.migrations.verification`,
//! `g5.lifecycle.idle.settings`).
//!
//! `workspace_settings` and `workspaces` are FORCE ROW LEVEL SECURITY tables,
//! so the real deployment migrator — NOSUPERUSER and NOBYPASSRLS — sees none
//! of their rows. 072 only changed the column default, which a fork never
//! consults because it copies its source's value; 081 raises Base rows still
//! on the previous default under the table-owner exemption. This test shows
//! the upgrade reaches every tenant's Base, keeps explicit Base values, leaves
//! working workspaces alone, restores FORCE on both tables and replays as a
//! no-op.

use crate::helpers;

use helpers::TestDb;
use sqlx::PgPool;
use std::path::Path;
use std::process::Output;
use tokio::process::Command;
use uuid::Uuid;

const OLD_DEFAULT: i32 = 1800;
const NEW_DEFAULT: i32 = 28800;

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

/// One org with its Base workspace on `base_timeout` and one working
/// workspace (a fork of that Base, holding the copied value) on
/// `working_timeout`, the way QA holds them before the upgrade. `Some` stores
/// the value explicitly (QA rows predate 072, so the column default in force
/// at seed time must not leak into them); `None` omits the column and takes
/// the current default. Returns `(base, working)`.
async fn seed_org(
    pool: &PgPool,
    base_timeout: Option<i32>,
    working_timeout: Option<i32>,
) -> (Uuid, Uuid) {
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Base idle migration',$2)")
        .bind(org)
        .bind(format!("base-idle-{}", org.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Base idle owner')")
        .bind(owner)
        .bind(format!("base-idle-owner-{owner}@example.test"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    let mut workspaces = Vec::new();
    for (is_base, timeout) in [(true, base_timeout), (false, working_timeout)] {
        let workspace = Uuid::new_v4();
        let name = if is_base {
            "Base".to_string()
        } else {
            format!("work-{}", workspace.simple())
        };
        sqlx::query(
            "INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) \
             VALUES($1,$2,$3,$4,$5,$5)",
        )
        .bind(workspace)
        .bind(org)
        .bind(name)
        .bind(is_base)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
        match timeout {
            None => {
                sqlx::query("INSERT INTO workspace_settings(org_id,workspace_id) VALUES($1,$2)")
                    .bind(org)
                    .bind(workspace)
            }
            Some(timeout) => sqlx::query(
                "INSERT INTO workspace_settings(org_id,workspace_id,idle_timeout_secs) \
                 VALUES($1,$2,$3)",
            )
            .bind(org)
            .bind(workspace)
            .bind(timeout),
        }
        .execute(pool)
        .await
        .unwrap();
        workspaces.push(workspace);
    }
    (workspaces[0], workspaces[1])
}

async fn idle_timeout(pool: &PgPool, workspace: Uuid) -> i32 {
    sqlx::query_scalar("SELECT idle_timeout_secs FROM workspace_settings WHERE workspace_id = $1")
        .bind(workspace)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn upgrade_raises_base_rows_on_the_previous_default_without_bypassrls() {
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
            && version(&entry.file_name()) < 81
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    let name = "081_raise_base_idle_timeout.sql";
    std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();

    // Two tenants. The first Base still holds the pre-072 default; the second
    // Base's owner chose an hour. Both working workspaces carry the old value
    // their fork copied.
    let (first_base, first_working) =
        seed_org(&app.pool, Some(OLD_DEFAULT), Some(OLD_DEFAULT)).await;
    let (second_base, second_working) = seed_org(&app.pool, Some(3600), Some(OLD_DEFAULT)).await;
    assert_eq!(idle_timeout(&app.pool, first_base).await, OLD_DEFAULT);

    let role = format!("base_idle_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'base_idle_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    // The owner exemption needs the migrator to own both tables the UPDATE
    // touches: the written settings and the Base flag it reads.
    sqlx::raw_sql(&format!(
        "ALTER TABLE public.workspace_settings OWNER TO {role}; \
         ALTER TABLE public.workspaces OWNER TO {role}"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("base_idle_migration_test"))
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
                    .password("base_idle_migration_test"),
            )
            .await?;
        let result = async {
            let privileges: (bool, bool) = sqlx::query_as(
                "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&migrator_pool)
            .await?;
            let visible: (i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM workspace_settings), \
                        (SELECT count(*) FROM workspaces)",
            )
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
    assert_eq!(
        visible,
        (0, 0),
        "runner starts without tenant row visibility"
    );
    assert!(
        migration.status.success(),
        "the production executable raises Base rows as the non-BYPASSRLS table owner: {migration:?}",
    );
    let replay = replay.unwrap();
    assert!(
        replay.status.success(),
        "replaying the completed migration is a no-op: {replay:?}"
    );
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 81 AND success")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(applied, 1, "replay records the migration exactly once");

    // Every tenant's Base on the old default reaches 8 hours, so forks made
    // from now on inherit it; an explicit Base value and every working
    // workspace keep what they hold (the owner's 2026-10-13 decision).
    assert_eq!(idle_timeout(&app.pool, first_base).await, NEW_DEFAULT);
    assert_eq!(idle_timeout(&app.pool, second_base).await, 3600);
    assert_eq!(idle_timeout(&app.pool, first_working).await, OLD_DEFAULT);
    assert_eq!(idle_timeout(&app.pool, second_working).await, OLD_DEFAULT);

    // A Base created after the upgrade still takes 072's column default.
    let (fresh_base, _) = seed_org(&app.pool, None, None).await;
    assert_eq!(idle_timeout(&app.pool, fresh_base).await, NEW_DEFAULT);

    // The owner exemption is temporary: tenant isolation is forced again on
    // both tables.
    let forced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class \
         WHERE relname IN ('workspace_settings', 'workspaces') \
           AND relrowsecurity AND relforcerowsecurity",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        forced, 2,
        "workspace_settings and workspaces keep FORCE ROW LEVEL SECURITY"
    );

    app.drop().await;
}
