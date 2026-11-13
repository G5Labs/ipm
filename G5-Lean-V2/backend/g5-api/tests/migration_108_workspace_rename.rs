//! Migration 108 (`g5.workspace.rename`): the branch column is backfilled from
//! the name, the kind and branch stay immutable, Base keeps its name, live
//! names are unique ignoring case, and the runtime lookups read the branch.
//! Applied as a non-superuser migrator that may set the definer role, like
//! the deployed runner.

use crate::helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const MIGRATION: &str = "108_workspace_rename.sql";

async fn seed(db: &TestDb) -> (Uuid, Uuid, Uuid) {
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
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
    let base = Uuid::new_v4();
    let working = Uuid::new_v4();
    for (id, name, is_base) in [(base, "Base", true), (working, "feature/x", false)] {
        sqlx::query(
            "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by)
             VALUES ($1, $2, $3, $4, $5, $5)",
        )
        .bind(id)
        .bind(org)
        .bind(name)
        .bind(is_base)
        .bind(owner)
        .execute(&db.pool)
        .await
        .unwrap();
    }
    (org, base, working)
}

#[tokio::test]
async fn backfills_the_branch_and_keeps_it_immutable() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "108"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let (org, base, working) = seed(&db).await;

    let role = format!("rename_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
         GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
         GRANT USAGE, CREATE ON SCHEMA public TO {role}; ALTER SCHEMA authz OWNER TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};
         ALTER TABLE public.workspaces OWNER TO {role};
         GRANT SELECT ON public.workspace_repos, public.repos, public.workspace_members,
               public.org_memberships, public.workspace_vms, public.workspace_flushes,
               public.workspace_ownership_transfers TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
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
        .unwrap();
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    // The migrator took SET-capable definer membership only for the data
    // section and gave it back.
    let can_set: bool =
        sqlx::query_scalar("SELECT pg_has_role(current_user,'authz_lookup_definer','SET')")
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert!(!can_set, "temporary migration membership restored");
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .expect("a repeated run is a no-op");

    let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, branch_name FROM workspaces WHERE org_id = $1 ORDER BY is_base",
    )
    .bind(org)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                working,
                "feature/x".to_string(),
                Some("feature/x".to_string())
            ),
            (base, "Base".to_string(), None),
        ]
    );

    // The runtime lookups answer with the branch, under the definer role.
    sqlx::query("UPDATE workspaces SET name = 'renamed' WHERE id = $1")
        .bind(working)
        .execute(&db.pool)
        .await
        .unwrap();
    let authority: Option<String> =
        sqlx::query_scalar("SELECT branch_name FROM authz.lookup_workspace_runtime_authority($1)")
            .bind(working)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(authority.as_deref(), Some("feature/x"));

    // The branch and kind are immutable; Base keeps its name.
    for (sql, id) in [
        (
            "UPDATE workspaces SET branch_name = 'other' WHERE id = $1",
            working,
        ),
        (
            "UPDATE workspaces SET is_base = true WHERE id = $1",
            working,
        ),
        ("UPDATE workspaces SET name = 'Root' WHERE id = $1", base),
    ] {
        let error = sqlx::query(sql)
            .bind(id)
            .execute(&db.pool)
            .await
            .expect_err(sql);
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("23514"),
            "{sql}"
        );
    }

    // Live names are unique ignoring case; a working insert without a branch
    // owns its name.
    let duplicate = sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by)
         SELECT gen_random_uuid(), org_id, 'RENAMED', false, owner_user_id, owner_user_id
         FROM workspaces WHERE id = $1",
    )
    .bind(working)
    .execute(&db.pool)
    .await
    .expect_err("duplicate live name");
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505")
    );
    let fresh = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by)
         SELECT $2, org_id, 'fresh', false, owner_user_id, owner_user_id
         FROM workspaces WHERE id = $1",
    )
    .bind(working)
    .bind(fresh)
    .execute(&db.pool)
    .await
    .unwrap();
    let fresh_branch: Option<String> =
        sqlx::query_scalar("SELECT branch_name FROM workspaces WHERE id = $1")
            .bind(fresh)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(fresh_branch.as_deref(), Some("fresh"));

    migrator.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
}

#[tokio::test]
async fn refuses_live_names_that_differ_only_in_case() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "108"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let (_, _, working) = seed(&db).await;
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by)
         SELECT gen_random_uuid(), org_id, 'FEATURE/X', false, owner_user_id, owner_user_id
         FROM workspaces WHERE id = $1",
    )
    .bind(working)
    .execute(&db.pool)
    .await
    .unwrap();
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    let error = g5_api::db::run_migrations_from_path(&db.pool, catalog.path())
        .await
        .expect_err("case-insensitive duplicates block the migration");
    assert!(error.to_string().contains("differ only in case"), "{error}");
    db.drop().await;
}
