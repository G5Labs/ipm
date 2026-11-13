//! Populated cutover with the deployment migrator's NOSUPERUSER/NOBYPASSRLS
//! ownership and non-inherited definer membership. PR B preserves all tables.
use crate::helpers::TestDb;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

async fn rows(pool: &PgPool) -> Vec<(String, Value)> {
    let mut result = Vec::new();
    for table in [
        "comment_index",
        "comment_replies",
        "docs",
        "tasks",
        "ontology_nodes",
        "doc_node_links",
        "projection_state",
    ] {
        let value: Value = sqlx::query_scalar(&format!("SELECT coalesce(jsonb_agg(row ORDER BY row::text),'[]') FROM (SELECT to_jsonb(t) row FROM {table} t) rows"))
            .fetch_one(pool).await.unwrap();
        result.push((table.to_owned(), value));
    }
    result
}

async fn checkpoints(pool: &PgPool) -> Vec<(Uuid, String, String, String)> {
    sqlx::query_as(
        "SELECT repo_id,projection,ref,applied_oid FROM projection_state ORDER BY repo_id,projection,ref",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn cutover_refuses_node_history_and_preserves_all_other_rows_as_migrator() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "090"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    sqlx::raw_sql(include_str!("fixtures/projection_retirement_populated.sql"))
        .execute(&db.pool)
        .await
        .unwrap();
    let role = format!("retire_projections_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS; GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE; GRANT USAGE,CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};")).execute(&db.pool).await.unwrap();
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT tablename FROM pg_tables WHERE schemaname='public'")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    for table in tables {
        sqlx::raw_sql(&format!("ALTER TABLE public.\"{table}\" OWNER TO {role}"))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    for function in [
        "invalidate_workspace_repo_projections",
        "invalidate_default_ref_projections",
    ] {
        sqlx::raw_sql(&format!(
            "ALTER FUNCTION public.{function}() OWNER TO {role}"
        ))
        .execute(&db.pool)
        .await
        .unwrap();
    }
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
    let privilege_snapshot: Vec<(String, bool)> = sqlx::query_as("SELECT column_name,has_column_privilege('authz_lookup_definer','public.comment_index',column_name,'SELECT') FROM (VALUES ('scope'),('node_id')) columns(column_name) ORDER BY column_name")
        .fetch_all(&db.pool).await.unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_index")
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(
        visible, 0,
        "the migrator cannot silently treat RLS-hidden rows as absent"
    );
    std::fs::copy(
        source.join("090_retire_projection_triggers.sql"),
        catalog.path().join("090_retire_projection_triggers.sql"),
    )
    .unwrap();
    let ledger: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(m) ORDER BY version) FROM _sqlx_migrations m",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    // Exercise both guard predicates independently; no identity conversion is permitted.
    for (scope, node) in [
        ("node", None),
        ("line", Some("62000000-0000-4000-8000-000000000020")),
    ] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO comment_index(id,org_id,workspace_id,author_id,scope,node_id,content,origin_branch) VALUES($1,'62000000-0000-4000-8000-000000000001','62000000-0000-4000-8000-000000000003','62000000-0000-4000-8000-000000000002',$2,$3,'Do not lose this history','main')")
            .bind(id).bind(scope).bind(node.map(|s|Uuid::parse_str(s).unwrap())).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO comment_replies(org_id,comment_id,author_id,content) VALUES('62000000-0000-4000-8000-000000000001',$1,'62000000-0000-4000-8000-000000000002','Keep the reply')").bind(id).execute(&db.pool).await.unwrap();
        let before = rows(&db.pool).await;
        let err = g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("node comments remain"),
            "{err:?}"
        );
        assert_eq!(rows(&db.pool).await, before, "refusal rolls back every row");
        let after_ledger: Value = sqlx::query_scalar(
            "SELECT jsonb_agg(to_jsonb(m) ORDER BY version) FROM _sqlx_migrations m",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(after_ledger, ledger);
        let privileges: Vec<(String, bool)> = sqlx::query_as("SELECT column_name,has_column_privilege('authz_lookup_definer','public.comment_index',column_name,'SELECT') FROM (VALUES ('scope'),('node_id')) columns(column_name) ORDER BY column_name")
            .fetch_all(&db.pool).await.unwrap();
        assert_eq!(
            privileges, privilege_snapshot,
            "refusal rolls back borrowed privileges"
        );
        let shape:(bool,i64)=sqlx::query_as("SELECT relforcerowsecurity,(SELECT count(*) FROM pg_trigger WHERE tgname IN ('invalidate_workspace_repo_projections','invalidate_default_ref_projections')) FROM pg_class WHERE oid='comment_index'::regclass").fetch_one(&db.pool).await.unwrap();
        assert_eq!(
            shape,
            (true, 2),
            "failed migration retains RLS and both triggers"
        );
        // Only the fixture author removes its deliberately incompatible seed.
        sqlx::query("DELETE FROM comment_replies WHERE comment_id=$1")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM comment_index WHERE id=$1")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let before = rows(&db.pool).await;
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    assert_eq!(
        rows(&db.pool).await,
        before,
        "B preserves all history and rollback tables byte for byte"
    );
    let objects:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM pg_trigger WHERE tgname IN ('invalidate_workspace_repo_projections','invalidate_default_ref_projections'))+(SELECT count(*) FROM pg_proc WHERE pronamespace='public'::regnamespace AND proname IN ('invalidate_workspace_repo_projections','invalidate_default_ref_projections'))").fetch_one(&db.pool).await.unwrap();
    assert_eq!(objects, 4, "rollback retains both invalidation hooks");
    // The old image's branch-hint path runs this exact query on its system
    // connection. Keep the function and its execution grant until PR C.
    let mut old_image = db.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE g5_app")
        .execute(&mut *old_image)
        .await
        .unwrap();
    let orgs: Vec<Uuid> =
        sqlx::query_scalar("SELECT org_id FROM authz.lookup_repo_org_for_projection($1)")
            .bind(Uuid::parse_str("62000000-0000-4000-8000-000000000010").unwrap())
            .fetch_all(&mut *old_image)
            .await
            .unwrap();
    assert_eq!(
        orgs,
        vec![Uuid::parse_str("62000000-0000-4000-8000-000000000001").unwrap()]
    );
    old_image.rollback().await.unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_index")
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(visible, 0, "FORCE restored on success");
    let membership:(bool,bool,bool)=sqlx::query_as("SELECT admin_option,inherit_option,set_option FROM pg_auth_members m JOIN pg_roles r ON r.oid=m.member JOIN pg_roles g ON g.oid=m.roleid WHERE r.rolname=$1 AND g.rolname='authz_lookup_definer'").bind(&role).fetch_one(&db.pool).await.unwrap();
    assert_eq!(membership, (true, false, false));
    let privileges: Vec<(String, bool)> = sqlx::query_as("SELECT column_name,has_column_privilege('authz_lookup_definer','public.comment_index',column_name,'SELECT') FROM (VALUES ('scope'),('node_id')) columns(column_name) ORDER BY column_name")
        .fetch_all(&db.pool).await.unwrap();
    assert_eq!(
        privileges, privilege_snapshot,
        "success restores prior column grants"
    );
    // The old image skips reconciliation while the checkpoint matches the
    // unchanged Git OID. Binding and policy edits must still clear that gate.
    let repo = Uuid::parse_str("62000000-0000-4000-8000-000000000010").unwrap();
    let mut expected = checkpoints(&db.pool).await;
    assert!(expected.iter().any(|row| row
        == &(
            repo,
            "docs".into(),
            "refs/heads/retirement-0".into(),
            "b".repeat(40)
        )));
    sqlx::query("UPDATE workspace_repos SET root_path='changed-root' WHERE repo_id=$1")
        .bind(repo)
        .execute(&db.pool)
        .await
        .unwrap();
    expected.retain(|row| !(row.0 == repo && row.2 == "refs/heads/retirement-0"));
    assert_eq!(checkpoints(&db.pool).await, expected);

    for branch in ["next", "unrelated"] {
        let reference = format!("refs/heads/{branch}");
        sqlx::query("INSERT INTO projection_state(projection,repo_id,ref,applied_oid) VALUES('ontology',$1,$2,$3)")
            .bind(repo).bind(&reference).bind("a".repeat(40)).execute(&db.pool).await.unwrap();
        expected.push((repo, "ontology".into(), reference, "a".repeat(40)));
    }
    sqlx::query("UPDATE repos SET default_branch='next' WHERE id=$1")
        .bind(repo)
        .execute(&db.pool)
        .await
        .unwrap();
    expected.retain(|row| {
        !(row.0 == repo && matches!(row.2.as_str(), "refs/heads/main" | "refs/heads/next"))
    });
    expected.sort();
    assert_eq!(
        checkpoints(&db.pool).await,
        expected,
        "both authority refs are invalidated; other refs and repositories survive"
    );
    let mut after = rows(&db.pool).await;
    let mut history = before;
    after.retain(|(table, _)| table != "projection_state");
    history.retain(|(table, _)| table != "projection_state");
    assert_eq!(
        after, history,
        "invalidation does not edit projected data or discussion history"
    );
    migrator.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
}
