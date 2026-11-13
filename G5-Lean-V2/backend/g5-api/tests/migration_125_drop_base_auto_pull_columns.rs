//! Migration 125 (issue #291, the contract step for #281): the retired
//! workspace-level Base auto-pull columns are dropped from a populated
//! `workspaces` table, with their check constraint and actor foreign key,
//! while the tenant policies and FORCE RLS flag stay. Applied twice as a
//! non-superuser, non-BYPASSRLS table owner, like the deployed runner.

use crate::helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const MIGRATION: &str = "125_drop_base_auto_pull_columns.sql";
const RETIRED_COLUMNS: [&str; 2] = ["base_auto_pull_actor_user_id", "base_auto_pull_enabled"];
const RETIRED_CONSTRAINTS: [&str; 2] = [
    "workspaces_base_auto_pull_actor_required_check",
    "workspaces_base_auto_pull_actor_user_id_fkey",
];

async fn retired_columns(db: &TestDb) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'workspaces'
           AND column_name = ANY($1) ORDER BY column_name",
    )
    .bind(&RETIRED_COLUMNS[..])
    .fetch_all(&db.pool)
    .await
    .unwrap()
}

async fn retired_constraints(db: &TestDb) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT conname::text FROM pg_constraint
         WHERE conrelid = 'public.workspaces'::regclass AND conname = ANY($1)
         ORDER BY conname",
    )
    .bind(&RETIRED_CONSTRAINTS[..])
    .fetch_all(&db.pool)
    .await
    .unwrap()
}

async fn workspace_policies(db: &TestDb) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT policyname::text FROM pg_policies
         WHERE schemaname = 'public' AND tablename = 'workspaces' ORDER BY policyname",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn populated_upgrade_drops_the_retired_base_auto_pull_columns_without_bypassrls() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "125"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    assert_eq!(
        retired_columns(&db).await,
        RETIRED_COLUMNS,
        "the schema before 125 still carries the retired columns"
    );
    assert_eq!(retired_constraints(&db).await, RETIRED_CONSTRAINTS);
    let policies_before = workspace_policies(&db).await;
    assert!(
        !policies_before.is_empty(),
        "workspaces has tenant policies"
    );

    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let base = Uuid::new_v4();
    let working = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Owner')")
        .bind(owner)
        .bind(format!("owner-{owner}@example.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $2)")
        .bind(org)
        .bind(format!("org-{}", org.simple()))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org)
        .bind(owner)
        .execute(&db.pool)
        .await
        .unwrap();
    // Reproduce a stale writer from the expand-only rollout, which could
    // repopulate the retired setting after 056 had cleared it.
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by,
                                 base_auto_pull_enabled, base_auto_pull_actor_user_id)
         VALUES ($1, $2, 'main', true, $3, $3, true, $3)",
    )
    .bind(base)
    .bind(org)
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, owner_user_id, created_by)
         VALUES ($1, $2, 'working', $3, $3)",
    )
    .bind(working)
    .bind(org)
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();

    let role = format!("base_auto_pull_drop_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
        GRANT USAGE, CREATE ON SCHEMA public TO {role};
        GRANT USAGE ON SCHEMA authz TO {role};
        GRANT ALL ON public._sqlx_migrations TO {role};
        ALTER TABLE public.workspaces OWNER TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    let probe: Result<((bool, bool), i64), String> = async {
        let migrator = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(
                db.pool
                    .connect_options()
                    .as_ref()
                    .clone()
                    .username(&role)
                    .password("migration_test"),
            )
            .await
            .map_err(|e| e.to_string())?;
        let result = async {
            let privileges: (bool, bool) = sqlx::query_as(
                "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&migrator)
            .await
            .map_err(|e| e.to_string())?;
            let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
                .fetch_one(&migrator)
                .await
                .map_err(|e| e.to_string())?;
            for _ in 0..2 {
                g5_api::db::run_migrations_from_path(&migrator, catalog.path())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok((privileges, visible))
        }
        .await;
        migrator.close().await;
        result
    }
    .await;
    // Clean the cluster-global test role even if the probe fails.
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    let (privileges, visible) = probe.expect("the owner applies migration 125 twice");
    assert_eq!(privileges, (false, false));
    assert_eq!(visible, 0, "the owner runs without tenant row visibility");

    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = 125 AND success")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(applied, 1, "replay records the migration exactly once");
    assert!(
        retired_columns(&db).await.is_empty(),
        "both columns are dropped"
    );
    assert!(
        retired_constraints(&db).await.is_empty(),
        "the actor check and foreign key go with the columns"
    );
    let ledger: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.repo_mirror_refs')::text")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        ledger, None,
        "the repo_mirror_refs ledger is gone (migration 063)"
    );

    let rows: Vec<(Uuid, bool, Uuid)> = sqlx::query_as(
        "SELECT id, is_base, owner_user_id FROM workspaces WHERE org_id = $1 ORDER BY is_base DESC",
    )
    .bind(org)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(rows, vec![(base, true, owner), (working, false, owner)]);
    let (row_security, forced): (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid = 'public.workspaces'::regclass",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(
        row_security && forced,
        "workspaces keeps FORCE ROW LEVEL SECURITY"
    );
    assert_eq!(
        workspace_policies(&db).await,
        policies_before,
        "tenant policies are untouched"
    );
    db.drop().await;
}
