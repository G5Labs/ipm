//! PR C contracts a populated PR B schema using the deployment role's authority.
use crate::helpers::TestDb;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

const MIGRATION: &str = "091_remove_projection_storage.sql";

async fn setup(can_set_definer: bool) -> (TestDb, tempfile::TempDir, PgPool, String) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "091"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    sqlx::raw_sql(include_str!("fixtures/projection_retirement_populated.sql"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO comment_mentions(workspace_id,comment_id,mentioned_user_id,mentioned_by_user_id,org_id,created_at) SELECT workspace_id,id,author_id,author_id,org_id,'2026-07-04' FROM comment_index")
        .execute(&db.pool).await.unwrap();
    // A historical dangling node reference must not discard its owning task or
    // document. Only the fixture author bypasses FK triggers to reproduce it.
    let mut seed = db.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *seed)
        .await
        .unwrap();
    sqlx::query("UPDATE tasks SET node_id='62000000-0000-4000-8000-000000000999' WHERE id='62000000-0000-4000-8000-000000000040'")
        .execute(&mut *seed).await.unwrap();
    sqlx::query("UPDATE docs SET scope_node_id='62000000-0000-4000-8000-000000000999' WHERE id='62000000-0000-4000-8000-000000000030'")
        .execute(&mut *seed).await.unwrap();
    seed.commit().await.unwrap();

    let role = format!("contract_projections_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS; GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET {can_set_definer}; GRANT USAGE,CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};"))
        .execute(&db.pool).await.unwrap();
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
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    (db, catalog, migrator, role)
}

async fn rows(pool: &PgPool, retained_only: bool) -> Vec<(String, Value)> {
    let mut tables = vec![
        "comment_index",
        "comment_replies",
        "comment_mentions",
        "docs",
        "tasks",
    ];
    if !retained_only {
        tables.extend(["ontology_nodes", "doc_node_links", "projection_state"]);
    }
    let mut result = Vec::new();
    for table in tables {
        let value = if retained_only {
            match table {
                "comment_index" | "tasks" => "to_jsonb(t) - 'node_id'",
                "docs" => "to_jsonb(t) - ARRAY['scope_node_id','scope_mode']",
                _ => "to_jsonb(t)",
            }
        } else {
            "to_jsonb(t)"
        };
        let value: Value = sqlx::query_scalar(&format!("SELECT coalesce(jsonb_agg(row ORDER BY row::text),'[]') FROM (SELECT {value} row FROM {table} t) rows"))
            .fetch_one(pool).await.unwrap();
        result.push((table.to_owned(), value));
    }
    result
}

async fn privileges(pool: &PgPool, role: &str) -> (Value, Value) {
    let membership = sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(grantor.rolname,m.admin_option,m.inherit_option,m.set_option) ORDER BY grantor.rolname) FROM pg_auth_members m JOIN pg_roles member ON member.oid=m.member JOIN pg_roles granted ON granted.oid=m.roleid JOIN pg_roles grantor ON grantor.oid=m.grantor WHERE member.rolname=$1 AND granted.rolname='authz_lookup_definer'")
        .bind(role).fetch_one(pool).await.unwrap();
    let grants = sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(a.attname,a.attacl::text) ORDER BY a.attname) FROM pg_attribute a WHERE a.attrelid='comment_index'::regclass AND a.attnum>0 AND NOT a.attisdropped AND a.attname<>'node_id'")
        .fetch_one(pool).await.unwrap();
    (membership, grants)
}

async fn schema(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('columns',(SELECT jsonb_agg(jsonb_build_array(table_name,column_name,data_type) ORDER BY table_name,ordinal_position) FROM information_schema.columns WHERE table_schema='public'),'constraints',(SELECT jsonb_agg(jsonb_build_array(conrelid::regclass::text,conname,pg_get_constraintdef(oid)) ORDER BY conrelid,conname) FROM pg_constraint WHERE connamespace='public'::regnamespace),'hooks',(SELECT jsonb_agg(jsonb_build_array(n.nspname,p.proname,pg_get_functiondef(p.oid)) ORDER BY n.nspname,p.proname) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE p.proname IN ('lookup_repo_org_for_projection','invalidate_workspace_repo_projections','invalidate_default_ref_projections')),'ledger',(SELECT jsonb_agg(to_jsonb(m) ORDER BY version) FROM _sqlx_migrations m))")
        .fetch_one(pool).await.unwrap()
}

async fn cleanup(db: TestDb, migrator: PgPool, role: &str) {
    migrator.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
}

#[tokio::test]
async fn contraction_preserves_documents_tasks_discussion_and_authority() {
    for can_set_definer in [false, true] {
        let (db, catalog, migrator, role) = setup(can_set_definer).await;
        let before = rows(&db.pool, true).await;
        let authority = privileges(&db.pool, &role).await;
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_index")
            .fetch_one(&migrator)
            .await
            .unwrap();
        assert_eq!(visible, 0, "the deployment role cannot bypass forced RLS");
        for _ in 0..2 {
            g5_api::db::run_migrations_from_path(&migrator, catalog.path())
                .await
                .unwrap();
        }
        assert_eq!(
            rows(&db.pool, true).await,
            before,
            "all retained fields survive byte for byte"
        );
        assert_eq!(
            privileges(&db.pool, &role).await,
            authority,
            "restore borrowed authority for either initial membership"
        );
        let removed: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename IN ('ontology_nodes','doc_node_links','projection_state'))+(SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND ((table_name IN ('comment_index','tasks') AND column_name='node_id') OR (table_name='docs' AND column_name IN ('scope_node_id','scope_mode'))))+(SELECT count(*) FROM pg_proc WHERE pronamespace IN ('public'::regnamespace,'authz'::regnamespace) AND proname IN ('lookup_repo_org_for_projection','invalidate_workspace_repo_projections','invalidate_default_ref_projections'))+(SELECT count(*) FROM pg_trigger WHERE tgname IN ('invalidate_workspace_repo_projections','invalidate_default_ref_projections'))+(SELECT count(*) FROM pg_indexes WHERE schemaname='public' AND indexname='idx_comment_node')")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(removed, 0);
        let docs = sqlx::query_as::<_, g5_api::models::Doc>("SELECT * FROM docs")
            .fetch_all(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            docs.len(),
            4,
            "the application row reader works after contraction"
        );
        let error = sqlx::query("INSERT INTO comment_index(org_id,workspace_id,author_id,scope,content,origin_branch) VALUES('62000000-0000-4000-8000-000000000001','62000000-0000-4000-8000-000000000003','62000000-0000-4000-8000-000000000002','node','Forbidden new node thread','main')")
            .execute(&db.pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().constraint(),
            Some("comment_index_scope_check")
        );
        sqlx::query("UPDATE workspace_repos SET root_path='changed-root' WHERE repo_id='62000000-0000-4000-8000-000000000010'")
            .execute(&db.pool).await.unwrap();
        sqlx::query("UPDATE repos SET default_branch='trunk' WHERE id='62000000-0000-4000-8000-000000000010'")
            .execute(&db.pool).await.unwrap();
        sqlx::query("SELECT authz.assert_tenant_tables_force_rls()")
            .execute(&migrator)
            .await
            .unwrap();
        cleanup(db, migrator, &role).await;
    }
}

#[tokio::test]
async fn contraction_refuses_hidden_node_history_without_any_schema_or_ledger_change() {
    let (db, catalog, migrator, role) = setup(false).await;
    for (scope, node) in [
        ("node", None),
        ("line", Some("62000000-0000-4000-8000-000000000020")),
    ] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO comment_index(id,org_id,workspace_id,author_id,scope,node_id,content,origin_branch) VALUES($1,'62000000-0000-4000-8000-000000000001','62000000-0000-4000-8000-000000000003','62000000-0000-4000-8000-000000000002',$2,$3,'Do not lose this history','main')")
            .bind(id).bind(scope).bind(node.map(|s| Uuid::parse_str(s).unwrap())).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO comment_replies(org_id,comment_id,author_id,content) VALUES('62000000-0000-4000-8000-000000000001',$1,'62000000-0000-4000-8000-000000000002','Keep the reply')")
            .bind(id).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO comment_mentions(workspace_id,comment_id,mentioned_user_id,mentioned_by_user_id,org_id) SELECT workspace_id,id,author_id,author_id,org_id FROM comment_index WHERE id=$1")
            .bind(id).execute(&db.pool).await.unwrap();
        let before = rows(&db.pool, false).await;
        let shape = schema(&db.pool).await;
        let authority = privileges(&db.pool, &role).await;
        let error = g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap_err();
        assert!(
            format!("{error:?}").contains("node comments remain"),
            "{error:?}"
        );
        assert_eq!(rows(&db.pool, false).await, before);
        assert_eq!(schema(&db.pool).await, shape);
        assert_eq!(privileges(&db.pool, &role).await, authority);
        let forced: bool = sqlx::query_scalar(
            "SELECT relforcerowsecurity FROM pg_class WHERE oid='comment_index'::regclass",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(forced);
        // Only the fixture author removes its deliberately incompatible thread.
        sqlx::query("DELETE FROM comment_index WHERE id=$1")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    cleanup(db, migrator, &role).await;
}
