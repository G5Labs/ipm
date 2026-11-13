//! Populated migration 037 coverage through the Helm/CI migration executable.

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

async fn make_remote_tracking_branch(db: &TestDb, org: Uuid, owner: Uuid) -> (Uuid, Uuid) {
    let workspace = Uuid::new_v4();
    let repo = Uuid::new_v4();
    let branch = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces(id,org_id,name,branch_name,owner_user_id,created_by) VALUES($1,$2,$3,$3,$4,$4)",
    )
    .bind(workspace)
    .bind(org)
    .bind(format!("historical-{workspace}"))
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO repos(id,org_id,name,slug,git_url) VALUES($1,$2,$3,$3,'https://github.com/example/repo.git')")
        .bind(repo).bind(org).bind(format!("historical-{repo}")).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO workspace_branches(id,org_id,workspace_id,repo_id,branch_name,head_commit) VALUES($1,$2,$3,$4,'main',repeat('a',40))")
        .bind(branch).bind(org).bind(workspace).bind(repo).execute(&db.pool).await.unwrap();
    let wbr = sqlx::query_scalar("UPDATE workspace_branch_repos SET tracking_mode='remote_tracking',remote_branch='main',baseline_commit=repeat('a',40) WHERE workspace_branch_id=$1 AND repo_id=$2 RETURNING id")
        .bind(branch).bind(repo).fetch_one(&db.pool).await.unwrap();
    (workspace, wbr)
}

#[tokio::test]
async fn sec_h07_upgrade_pauses_historical_transfers_without_bypassrls() {
    // The current application has retired these tables. Exercise migration 037
    // against its historical schema, before the repository cleanup runs.
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
                < 37
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    std::fs::copy(
        source.join("037_pause_legacy_transferred_remote_tracking.sql"),
        catalog
            .path()
            .join("037_pause_legacy_transferred_remote_tracking.sql"),
    )
    .unwrap();
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orgs(id,name,slug) VALUES($1,'Historical migration','historical-migration')",
    )
    .bind(org)
    .execute(&app.pool)
    .await
    .unwrap();
    for user in [owner, recipient] {
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Historical owner')")
            .bind(user)
            .bind(format!("{user}@g5.test"))
            .execute(&app.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
            .bind(org)
            .bind(user)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    let (workspace, wbr) = make_remote_tracking_branch(&app, org, recipient).await;
    let (untouched, untouched_wbr) = make_remote_tracking_branch(&app, org, owner).await;
    sqlx::query("INSERT INTO workspace_ownership_transfers(org_id,workspace_id,from_user_id,to_user_id,requested_by,idempotency_key,request_digest,status,completed_at) VALUES($1,$2,$3,$4,$3,'historical',repeat('a',64),'completed',now())")
        .bind(org).bind(workspace).bind(owner).bind(recipient).execute(&app.pool).await.unwrap();
    let mut tx = app.pool.begin().await.unwrap();
    // Reconstruct the settings retained by pre-fix completed transfers.
    sqlx::query("UPDATE workspaces SET push_mode = 'auto', remote_tracking_requires_resume = false WHERE id = ANY($1)")
        .bind(vec![workspace, untouched]).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE workspace_branch_repos SET auto_pull_enabled = true, sync_paused_reason = 'diverged' WHERE id = ANY($1)")
        .bind(vec![wbr, untouched_wbr]).execute(&mut *tx).await.unwrap();
    for id in [wbr, untouched_wbr] {
        sqlx::query("INSERT INTO branch_integration_queue (workspace_branch_repo_id, org_id, trigger_kind) VALUES ($1, $2, 'push')")
            .bind(id).bind(org).execute(&mut *tx).await.unwrap();
    }
    // Seeded rows represent data committed before the migration starts.
    sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE; SET CONSTRAINTS ALL DEFERRED;")
        .execute(&mut *tx)
        .await
        .unwrap();
    let role = format!("sec_h07_upgrade_{}", Uuid::new_v4().simple());
    let tables = [
        "workspaces",
        "workspace_ownership_transfers",
        "workspace_branches",
        "workspace_branch_repos",
        "branch_integration_queue",
    ];
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'sec_h07_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
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
        .set_password(Some("sec_h07_migration_test"))
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
                    .password("sec_h07_migration_test"),
            )
            .await?;
        let result = async {
            let privileges: (bool, bool) = sqlx::query_as(
                "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&migrator_pool)
            .await?;
            let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
                .fetch_one(&migrator_pool)
                .await?;
            let migrations = catalog.path();
            // A role with SQL privileges but without direct ownership must still be rejected.
            sqlx::raw_sql("ALTER TABLE public.workspaces OWNER TO CURRENT_USER")
                .execute(&app.pool)
                .await?;
            sqlx::raw_sql(&format!("GRANT SELECT ON public.workspaces TO {role}"))
                .execute(&app.pool)
                .await?;
            let not_owner = run_migration_command(migration_url.as_str(), migrations).await?;
            let applied_before_retry: i64 =
                sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 37")
                    .fetch_one(&app.pool)
                    .await?;
            sqlx::raw_sql(&format!("ALTER TABLE public.workspaces OWNER TO {role}"))
                .execute(&app.pool)
                .await?;
            let migration = run_migration_command(migration_url.as_str(), migrations).await?;
            let replay = if migration.status.success() {
                Some(run_migration_command(migration_url.as_str(), migrations).await?)
            } else {
                None
            };
            Ok::<_, sqlx::Error>((
                privileges,
                visible,
                not_owner,
                applied_before_retry,
                migration,
                replay,
            ))
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
    let (privileges, visible, not_owner, applied_before_retry, result, replay) =
        probe.expect("migration probe succeeds");
    assert_eq!(privileges, (false, false));
    assert_eq!(visible, 0, "runner starts without tenant row visibility");
    assert!(
        !not_owner.status.success()
            && String::from_utf8_lossy(&not_owner.stderr)
                .contains("Migration row-visibility guard failed"),
        "{not_owner:?}"
    );
    assert_eq!(
        applied_before_retry, 0,
        "guard refusal writes no ledger entry"
    );
    assert!(
        result.status.success(),
        "production executable remediates populated FORCE RLS rows as their non-BYPASSRLS owner: {result:?}",
    );
    let replay = replay.unwrap();
    assert!(
        replay.status.success(),
        "replaying the completed migration is a no-op: {replay:?}"
    );
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 37 AND success")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        applied, 1,
        "retry and replay record migration 037 exactly once"
    );
    for (id, repo, paused, mode, auto_pull, queued) in [
        (workspace, wbr, true, "manual", false, 0_i64),
        (untouched, untouched_wbr, false, "auto", true, 1_i64),
    ] {
        let state: (bool, String, bool, Option<String>, i64) = sqlx::query_as(
            "SELECT w.remote_tracking_requires_resume, w.push_mode, r.auto_pull_enabled, r.sync_paused_reason,
             (SELECT count(*) FROM branch_integration_queue WHERE workspace_branch_repo_id = r.id)
             FROM workspaces w, workspace_branch_repos r WHERE w.id = $1 AND r.id = $2",
        ).bind(id).bind(repo).fetch_one(&app.pool).await.unwrap();
        assert_eq!(
            state,
            (
                paused,
                mode.into(),
                auto_pull,
                Some("diverged".into()),
                queued
            )
        );
    }
    let forced: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relname = ANY($1) AND relrowsecurity AND relforcerowsecurity")
        .bind(tables.as_slice()).fetch_one(&app.pool).await.unwrap();
    assert_eq!(forced, 5, "migration restores every FORCE RLS flag");
    app.drop().await;
}
