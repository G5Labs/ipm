//! Populated migration 056 coverage through the Helm/CI migration executable.

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

/// Insert one workspace with a remote-tracked repository whose auto-pull is off.
async fn tracked_workspace(db: &TestDb, org: Uuid, owner: Uuid, is_base: bool) -> Uuid {
    let workspace = Uuid::new_v4();
    let repo = Uuid::new_v4();
    let name = if is_base {
        "main".to_owned()
    } else {
        format!("working-{}", workspace.simple())
    };
    sqlx::query(
        "INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) VALUES($1,$2,$3,$4,$5,$5)",
    )
    .bind(workspace)
    .bind(org)
    .bind(&name)
    .bind(is_base)
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO repos(id,org_id,name,slug,git_url,default_branch) VALUES($1,$2,$3,$3,'https://github.com/example/repo.git','main')")
        .bind(repo).bind(org).bind(format!("repo-{}", repo.simple())).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,'','code')")
        .bind(org).bind(workspace).bind(repo).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled) VALUES($1,$2,$3,'main',false)")
        .bind(org).bind(workspace).bind(repo).execute(&db.pool).await.unwrap();
    workspace
}

#[tokio::test]
async fn upgrade_enables_base_auto_pull_and_retires_base_only_state_without_bypassrls() {
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
            && version(&entry.file_name()) < 56
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    std::fs::copy(
        source.join("056_base_remote_tracking_parity.sql"),
        catalog.path().join("056_base_remote_tracking_parity.sql"),
    )
    .unwrap();

    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Base parity migration',$2)")
        .bind(org)
        .bind(format!("base-parity-{}", org.simple()))
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Base owner')")
        .bind(owner)
        .bind(format!("{owner}@g5.test"))
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(owner)
        .execute(&app.pool)
        .await
        .unwrap();
    let base = tracked_workspace(&app, org, owner, true).await;
    let working = tracked_workspace(&app, org, owner, false).await;
    // A previously opted-in Base keeps a populated actor until the drop.
    sqlx::query("UPDATE workspaces SET base_auto_pull_enabled=true, base_auto_pull_actor_user_id=$2 WHERE id=$1")
        .bind(base)
        .bind(owner)
        .execute(&app.pool)
        .await
        .unwrap();

    let role = format!("base_parity_upgrade_{}", Uuid::new_v4().simple());
    let tables = ["workspaces", "remote_integrations"];
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'base_parity_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    for table in tables {
        sqlx::raw_sql(&format!("ALTER TABLE public.{table} OWNER TO {role}"))
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("base_parity_migration_test"))
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
                    .password("base_parity_migration_test"),
            )
            .await?;
        let result = async {
            let privileges: (bool, bool) = sqlx::query_as(
                "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&migrator_pool)
            .await?;
            let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_integrations")
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
        "production executable updates populated FORCE RLS rows as their non-BYPASSRLS owner: {migration:?}",
    );
    let replay = replay.unwrap();
    assert!(
        replay.status.success(),
        "replaying the completed migrations is a no-op: {replay:?}"
    );
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 56 AND success")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(applied, 1, "replay records the migration exactly once");

    for (workspace, expected) in [(base, true), (working, false)] {
        let enabled: bool = sqlx::query_scalar(
            "SELECT auto_pull_enabled FROM remote_integrations WHERE workspace_id = $1",
        )
        .bind(workspace)
        .fetch_one(&app.pool)
        .await
        .unwrap();
        assert_eq!(
            enabled,
            expected,
            "only Base bindings switch auto-pull on (base={})",
            workspace == base
        );
    }
    // Expand step only: the retired setting is cleared for API pods still
    // running the previous image; a later contract migration drops it.
    let cleared: (bool, Option<Uuid>) = sqlx::query_as(
        "SELECT base_auto_pull_enabled, base_auto_pull_actor_user_id FROM workspaces WHERE id = $1",
    )
    .bind(base)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        cleared,
        (false, None),
        "the retired Base setting is cleared"
    );
    for (workspace, expected) in [(base, "manual"), (working, "auto")] {
        let push_mode: String =
            sqlx::query_scalar("SELECT push_mode FROM workspaces WHERE id = $1")
                .bind(workspace)
                .fetch_one(&app.pool)
                .await
                .unwrap();
        assert_eq!(
            push_mode,
            expected,
            "Base never publishes, so only Base switches to manual (base={})",
            workspace == base
        );
    }
    let forced: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relname = ANY($1) AND relrowsecurity AND relforcerowsecurity")
        .bind(&tables[..]).fetch_one(&app.pool).await.unwrap();
    assert_eq!(forced, 2, "migration restores every FORCE RLS flag");
    app.drop().await;
}
