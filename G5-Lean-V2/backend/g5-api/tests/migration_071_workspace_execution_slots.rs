//! Populated migrations 070–071 through the Helm/CI migration executable
//! (ontology `g5.quality.migrations.verification`,
//! `g5.agents.admission.default-slots`).
//!
//! `workspace_settings` is a FORCE ROW LEVEL SECURITY table, so the real
//! deployment migrator — NOSUPERUSER and NOBYPASSRLS — sees none of its rows.
//! 070 only changes the column default; 071 raises rows still on the previous
//! default under the table-owner exemption. This test shows the upgrade
//! reaches every tenant's rows, keeps explicit values, restores FORCE, gives
//! new rows the new default and replays as a no-op.

use crate::helpers;

use helpers::TestDb;
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

/// One org with a workspace on the column default and one whose owner chose
/// `explicit` slots. Returns `(defaulted, explicit)` workspace ids.
async fn seed_org(pool: &PgPool, explicit: i32) -> (Uuid, Uuid) {
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Slot default migration',$2)")
        .bind(org)
        .bind(format!("slot-default-{}", org.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Slot owner')")
        .bind(owner)
        .bind(format!("slot-owner-{owner}@example.test"))
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
    for slots in [None, Some(explicit)] {
        let workspace = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) \
             VALUES($1,$2,$3,false,$4,$4)",
        )
        .bind(workspace)
        .bind(org)
        .bind(format!("slots-{}", workspace.simple()))
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
        match slots {
            None => {
                sqlx::query("INSERT INTO workspace_settings(org_id,workspace_id) VALUES($1,$2)")
                    .bind(org)
                    .bind(workspace)
            }
            Some(slots) => sqlx::query(
                "INSERT INTO workspace_settings(org_id,workspace_id,bounded_execution_slots) \
                 VALUES($1,$2,$3)",
            )
            .bind(org)
            .bind(workspace)
            .bind(slots),
        }
        .execute(pool)
        .await
        .unwrap();
        workspaces.push(workspace);
    }
    (workspaces[0], workspaces[1])
}

async fn slots(pool: &PgPool, workspace: Uuid) -> i32 {
    sqlx::query_scalar(
        "SELECT bounded_execution_slots FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(workspace)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn upgrade_raises_rows_on_the_previous_default_without_bypassrls() {
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
            && version(&entry.file_name()) < 70
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    for name in [
        "070_workspace_execution_slots_default.sql",
        "071_raise_default_workspace_execution_slots.sql",
    ] {
        std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();
    }

    // Two tenants, each with a workspace on the pre-070 column default and one
    // with an owner-chosen value, the way QA holds them before the upgrade.
    let (first_default, first_explicit) = seed_org(&app.pool, 5).await;
    let (second_default, second_explicit) = seed_org(&app.pool, 1).await;
    assert_eq!(slots(&app.pool, first_default).await, 2);
    assert_eq!(slots(&app.pool, second_default).await, 2);

    let role = format!("slot_default_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'slot_default_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(&format!(
        "ALTER TABLE public.workspace_settings OWNER TO {role}"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("slot_default_migration_test"))
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
                    .password("slot_default_migration_test"),
            )
            .await?;
        let result = async {
            let privileges: (bool, bool) = sqlx::query_as(
                "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&migrator_pool)
            .await?;
            let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_settings")
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
        "the production executable raises the rows as the non-BYPASSRLS table owner: {migration:?}",
    );
    let replay = replay.unwrap();
    assert!(
        replay.status.success(),
        "replaying the completed migrations is a no-op: {replay:?}"
    );
    let applied: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM _sqlx_migrations WHERE version IN (70, 71) AND success",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(applied, 2, "replay records each migration exactly once");

    // Every tenant's default row reaches the new default; explicit owner
    // choices, including one below the old default, are kept.
    assert_eq!(slots(&app.pool, first_default).await, 16);
    assert_eq!(slots(&app.pool, second_default).await, 16);
    assert_eq!(slots(&app.pool, first_explicit).await, 5);
    assert_eq!(slots(&app.pool, second_explicit).await, 1);

    // A settings row created after the upgrade takes the new column default.
    let (fresh_default, _) = seed_org(&app.pool, 3).await;
    assert_eq!(slots(&app.pool, fresh_default).await, 16);

    // The owner exemption is temporary: tenant isolation is forced again.
    let forced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class WHERE relname = 'workspace_settings' \
         AND relrowsecurity AND relforcerowsecurity",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        forced, 1,
        "workspace_settings keeps FORCE ROW LEVEL SECURITY"
    );

    app.drop().await;
}
