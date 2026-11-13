//! Upgrade populated review data without retaining agent comments or losing
//! human reviews. Exercise the same non-superuser ownership as the runner.

use crate::helpers::TestDb;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const MIGRATION: &str = "113_remove_agent_review_comments.sql";
const TABLES: &[&str] = &[
    "review_publications",
    "review_findings",
    "review_suggestions",
    "comment_index",
    "comment_replies",
];

#[tokio::test]
async fn removes_agent_data_across_tenants_and_preserves_people() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "113"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let mut survivors = Vec::new();
    let mut approval_survivors = Vec::new();
    for _ in 0..2 {
        let org = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Reviewer')")
            .bind(owner)
            .bind(format!("{owner}@example.test"))
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $2)")
            .bind(org)
            .bind(org.to_string())
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'owner')")
            .bind(org)
            .bind(owner)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id, org_id, name, owner_user_id, created_by) VALUES ($1, $2, 'Review', $3, $3)")
            .bind(workspace).bind(org).bind(owner).execute(&db.pool).await.unwrap();
        // Human approval decisions allocate their own workspace thread, using
        // the fresh status id. They never reuse a review suggestion's thread.
        let definition = Uuid::new_v4();
        let status = Uuid::new_v4();
        sqlx::query("INSERT INTO approval_definitions (id, version, org_id, name, humans_list, created_by) VALUES ($1, 1, $2, 'Human approval', $3, $4)")
            .bind(definition).bind(org).bind(vec![owner]).bind(owner)
            .execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO comment_index (id, org_id, workspace_id, author_id, scope, content, origin_branch, phase) VALUES ($1, $2, $3, $4, 'workspace', 'Approved', 'review', 'ie')")
            .bind(status).bind(org).bind(workspace).bind(owner)
            .execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO approval_statuses (id, org_id, source_workspace_id, approval_definition_id, approval_definition_version, outcome, comment, actor_type, actor_user_id, reviewed_git_heads, phase, comment_id) VALUES ($1, $2, $3, $4, 1, 'approved', 'Approved', 'human', $5, '[]', 'ie', $1)")
            .bind(status).bind(org).bind(workspace).bind(definition).bind(owner)
            .execute(&db.pool).await.unwrap();
        let approval: serde_json::Value = sqlx::query_scalar(
            "SELECT jsonb_build_object('status', to_jsonb(s), 'thread', to_jsonb(c)) FROM approval_statuses s JOIN comment_index c ON c.id = s.comment_id WHERE s.id = $1",
        ).bind(status).fetch_one(&db.pool).await.unwrap();
        approval_survivors.push((status, approval));
        let mut publications = Vec::new();
        for kind in ["human", "agent", "platform"] {
            let id: Uuid = sqlx::query_scalar("INSERT INTO review_publications (org_id, source_workspace_id, reviewer_user_id, author_kind, author_label, round) VALUES ($1, $2, $3, $4, $4, 1) RETURNING id")
                .bind(org).bind(workspace).bind(owner).bind(kind).fetch_one(&db.pool).await.unwrap();
            publications.push(id);
        }
        sqlx::query("INSERT INTO review_findings (org_id, publication_id, title) VALUES ($1, $2, 'Agent finding')")
            .bind(org).bind(publications[1]).execute(&db.pool).await.unwrap();
        let mut threads = Vec::new();
        for content in [
            "Human review",
            "Agent root",
            "Agent edit",
            "Agent in human round",
            "Legacy agent",
            "General comment",
            "Retired assessment",
        ] {
            let id: Uuid = sqlx::query_scalar("INSERT INTO comment_index (org_id, workspace_id, author_id, scope, content, origin_branch) VALUES ($1, $2, $3, $4, $5, 'review') RETURNING id")
                .bind(org).bind(workspace).bind(owner)
                .bind(if content == "General comment" { "workspace" } else { "suggestion" })
                .bind(content).fetch_one(&db.pool).await.unwrap();
            threads.push(id);
        }
        let mut human_suggestion = Uuid::nil();
        for (publication, author_kind, kind, thread, edits, task) in [
            (
                publications[0],
                "human",
                "comment",
                threads[0],
                json!([{"commentId":threads[2],"status":"open"}]),
                None,
            ),
            (
                publications[1],
                "human",
                "comment",
                threads[1],
                json!([{"commentId":threads[2]}]),
                None,
            ),
            (
                publications[0],
                "agent",
                "assessment",
                threads[3],
                json!([]),
                None,
            ),
            (
                publications[0],
                "human",
                "comment",
                threads[4],
                json!([]),
                Some(Uuid::new_v4()),
            ),
            // Assessments had no human producer, even if a legacy row lost
            // its agent attribution. Retire the kind and its thread too.
            (
                publications[0],
                "human",
                "assessment",
                threads[6],
                json!([]),
                None,
            ),
        ] {
            let id: Uuid = sqlx::query_scalar("INSERT INTO review_suggestions (org_id, publication_id, kind, author_kind, author_label, comment_id, edits, agent_task_id, repo_id, path) VALUES ($1, $2, $3, $4, $4, $5, $6, $7, $8, 'review.md') RETURNING id")
                .bind(org).bind(publication).bind(kind).bind(author_kind).bind(thread).bind(edits).bind(task)
                .bind(Uuid::new_v4())
                .fetch_one(&db.pool).await.unwrap();
            if thread == threads[0] {
                human_suggestion = id;
            }
        }
        for (thread, kind, content) in [
            (threads[0], "person", "Human reply"),
            (threads[0], "agent", "Agent reply in human thread"),
            (threads[1], "person", "Reply in removed thread"),
            (threads[2], "agent", "Agent edit reply"),
            (threads[5], "person", "General reply"),
        ] {
            sqlx::query("INSERT INTO comment_replies (org_id, comment_id, author_id, author_kind, content) VALUES ($1, $2, $3, $4, $5)")
                .bind(org).bind(thread).bind(owner).bind(kind).bind(content).execute(&db.pool).await.unwrap();
        }
        survivors.push((publications[0], human_suggestion, threads[0], threads[5]));
    }

    let role = format!("review_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
        GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
        GRANT USAGE, CREATE ON SCHEMA public TO {role};
        GRANT USAGE ON SCHEMA authz TO {role};
        GRANT ALL ON public._sqlx_migrations TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    for table in TABLES {
        sqlx::raw_sql(&format!("ALTER TABLE public.{table} OWNER TO {role}"))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let grants_before: Vec<(String, String)> = sqlx::query_as(
        "SELECT relname::text, relacl::text FROM pg_class WHERE relname = ANY($1) ORDER BY relname",
    )
    .bind(TABLES)
    .fetch_all(&db.pool)
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
    for _ in 0..2 {
        g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap();
    }
    let grants_after: Vec<(String, String)> = sqlx::query_as(
        "SELECT relname::text, relacl::text FROM pg_class WHERE relname = ANY($1) ORDER BY relname",
    )
    .bind(TABLES)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(grants_before, grants_after);
    let can_set: bool = sqlx::query_scalar("SELECT pg_has_role($1, 'authz_lookup_definer', 'SET')")
        .bind(&role)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(!can_set, "temporary definer membership must be restored");
    for (table, expected) in [
        ("review_publications", 4i64),
        ("review_findings", 0),
        ("review_suggestions", 2),
        ("comment_index", 6),
        ("comment_replies", 4),
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM public.{table}"))
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, expected, "{table}");
        let forced: bool =
            sqlx::query_scalar("SELECT relforcerowsecurity FROM pg_class WHERE oid = $1::regclass")
                .bind(format!("public.{table}"))
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert!(forced, "{table} must retain forced RLS");
    }
    let retired_columns: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema = 'public' AND ((table_name = 'comment_replies' AND column_name = 'author_kind') OR (table_name = 'review_suggestions' AND column_name IN ('agent_task_id', 'assessment')))")
        .fetch_one(&db.pool).await.unwrap();
    assert_eq!(retired_columns, 0);
    for (status, before) in approval_survivors {
        let after: serde_json::Value = sqlx::query_scalar(
            "SELECT jsonb_build_object('status', to_jsonb(s), 'thread', to_jsonb(c)) FROM approval_statuses s JOIN comment_index c ON c.id = s.comment_id WHERE s.id = $1",
        ).bind(status).fetch_one(&db.pool).await.unwrap();
        assert_eq!(
            before, after,
            "approval decision and thread must be unchanged"
        );
    }
    for (publication, suggestion, human_thread, general_thread) in survivors {
        let (thread, edits): (Uuid, serde_json::Value) =
            sqlx::query_as("SELECT comment_id, edits FROM review_suggestions WHERE id = $1")
                .bind(suggestion)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(thread, human_thread);
        assert_eq!(edits, json!([{"commentId":null,"status":"open"}]));
        let error = sqlx::query("UPDATE review_suggestions SET kind = 'assessment' WHERE id = $1")
            .bind(suggestion)
            .execute(&db.pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
        for (thread, expected) in [
            (human_thread, "Human reply"),
            (general_thread, "General reply"),
        ] {
            let content: String =
                sqlx::query_scalar("SELECT content FROM comment_replies WHERE comment_id = $1")
                    .bind(thread)
                    .fetch_one(&db.pool)
                    .await
                    .unwrap();
            assert_eq!(content, expected);
        }
        for (table, id) in [
            ("review_publications", publication),
            ("review_suggestions", suggestion),
        ] {
            let error = sqlx::query(&format!(
                "UPDATE {table} SET author_kind = 'agent' WHERE id = $1"
            ))
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap_err();
            assert_eq!(
                error.as_database_error().unwrap().code().as_deref(),
                Some("23514")
            );
        }
    }
    migrator.close().await;
    // Drop the database first so the temporary migration role owns no objects.
    let admin = db
        .pool
        .connect_options()
        .as_ref()
        .clone()
        .database("postgres");
    db.drop().await;
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(admin)
        .await
        .unwrap();
    sqlx::raw_sql(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
