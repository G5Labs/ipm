//! Unused metadata contraction preserves all document/discussion data still read.
use crate::helpers::TestDb;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

const MIGRATION: &str = "092_remove_unused_doc_metadata.sql";
const REMOVED: &[&str] = &[
    "persona",
    "instruction",
    "is_future",
    "review_iterations",
    "custom_persona_label",
    "source_files",
    "mapping_content",
];

async fn retained_rows(pool: &PgPool) -> Vec<(String, Value)> {
    let mut rows = Vec::new();
    for table in [
        "docs",
        "tasks",
        "comment_index",
        "comment_replies",
        "comment_mentions",
    ] {
        let removed = if table == "docs" { REMOVED } else { &[] };
        let value: Value = sqlx::query_scalar(&format!(
            "SELECT coalesce(jsonb_agg(row ORDER BY row::text),'[]') \
             FROM (SELECT to_jsonb(t) - $1::text[] AS row FROM {table} t) rows"
        ))
        .bind(removed)
        .fetch_one(pool)
        .await
        .unwrap();
        rows.push((table.to_owned(), value));
    }
    rows
}

#[tokio::test]
async fn unused_metadata_upgrade_preserves_retained_rows_with_restricted_migrator() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "092"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    sqlx::raw_sql(include_str!("fixtures/unused_doc_metadata_populated.sql"))
        .execute(&db.pool)
        .await
        .unwrap();
    let before = retained_rows(&db.pool).await;

    let role = format!("contract_doc_metadata_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS; \
         GRANT USAGE,CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
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
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM docs")
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(visible, 0, "the migrator cannot bypass forced RLS");

    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    for _ in 0..2 {
        g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap();
        assert_eq!(retained_rows(&db.pool).await, before);
    }
    let surviving: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE table_schema='public' AND table_name='docs' AND column_name=ANY($1)",
    )
    .bind(REMOVED)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(surviving, 0);
    let constraint: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='docs'::regclass AND conname='docs_persona_check')"
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!constraint);
    let docs = sqlx::query_as::<_, g5_api::models::Doc>("SELECT * FROM docs")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        docs.len(),
        4,
        "the application row reader accepts the final schema"
    );
    sqlx::query("SELECT authz.assert_tenant_tables_force_rls()")
        .execute(&migrator)
        .await
        .unwrap();

    migrator.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
}
