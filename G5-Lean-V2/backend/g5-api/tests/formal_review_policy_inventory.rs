//! Inventory the final schema, including tables introduced after migration 095.
use crate::helpers::TestDb;
use sqlx::PgPool;

async fn missing_policies(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        r#"
        WITH bindings AS (
            SELECT c.table_name, c.column_name
            FROM information_schema.columns c
            JOIN pg_tables t ON t.schemaname = c.table_schema
                AND t.tablename = c.table_name AND t.rowsecurity
            WHERE c.table_schema = 'public' AND (
                (c.column_name = 'workspace_id' AND c.table_name <> 'workspaces')
                OR (c.column_name = 'task_id' AND c.table_name NOT IN
                    ('tasks','agent_approval_reviews','agent_approval_artifacts','approval_request_tasks'))
                OR (c.column_name IN ('session_id','chat_session_id')
                    AND c.table_name NOT IN ('tasks','chat_sessions')))
            UNION ALL SELECT 'workspaces', 'id'
            UNION ALL SELECT 'workspace_forks', 'id'
        ), commands AS (
            SELECT * FROM (VALUES ('r','select'),('a','insert'),('w','update'),('d','delete')) c(code,name)
        )
        SELECT b.table_name || '.' || b.column_name || ':' || cmd.name
        FROM bindings b CROSS JOIN commands cmd
        WHERE NOT EXISTS (
            SELECT 1 FROM pg_policy p JOIN pg_class t ON t.oid = p.polrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = 'public' AND t.relname = b.table_name
                AND p.polname LIKE 'formal_review_private%'
                AND NOT p.polpermissive AND p.polcmd::text = cmd.code
                AND p.polroles = ARRAY['g5_tenant'::regrole::oid]
                AND (cmd.code = 'a' OR (
                    pg_get_expr(p.polqual,p.polrelid) ~ ('\m' || b.column_name || '\M')
                    AND pg_get_expr(p.polqual,p.polrelid) LIKE '%authz.formal_review_%'
                    AND pg_get_expr(p.polqual,p.polrelid) LIKE
                        CASE WHEN cmd.code = 'r' THEN '%false%' ELSE '%true%' END))
                AND (cmd.code NOT IN ('a','w') OR (
                    pg_get_expr(p.polwithcheck,p.polrelid) ~ ('\m' || b.column_name || '\M')
                    AND pg_get_expr(p.polwithcheck,p.polrelid) LIKE '%authz.formal_review_%'
                    AND pg_get_expr(p.polwithcheck,p.polrelid) LIKE '%true%'))
        ) ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn every_final_schema_ownership_link_has_command_specific_review_policies() {
    let db = TestDb::create().await;
    assert_eq!(missing_policies(&db.pool).await, Vec::<String>::new());
    // Each independent nullable link needs its own protection. Merely having
    // another restrictive policy on this table cannot satisfy the inventory.
    sqlx::raw_sql(
        r#"
        CREATE TABLE public.formal_policy_probe (workspace_id uuid, task_id uuid);
        ALTER TABLE public.formal_policy_probe ENABLE ROW LEVEL SECURITY;
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let missing = missing_policies(&db.pool).await;
    assert_eq!(
        missing.len(),
        8,
        "new ownership links must fail: {missing:?}"
    );
    for (column, expression) in [
        ("workspace_id", "workspace_id"),
        ("task_id", "authz.formal_review_task_workspace(task_id)"),
    ] {
        for (command, predicate) in [
            ("SELECT", format!("USING (authz.formal_review_allows({expression},false))")),
            ("INSERT", format!("WITH CHECK (authz.formal_review_allows({expression},true))")),
            ("UPDATE", format!("USING (authz.formal_review_allows({expression},true)) WITH CHECK (authz.formal_review_allows({expression},true))")),
            ("DELETE", format!("USING (authz.formal_review_allows({expression},true))")),
        ] {
            sqlx::raw_sql(&format!(
                "CREATE POLICY formal_review_private_{column}_{command} ON public.formal_policy_probe AS RESTRICTIVE FOR {command} TO g5_tenant {predicate}"
            ))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        let missing = missing_policies(&db.pool).await;
        if column == "workspace_id" {
            assert_eq!(missing.len(), 4);
            assert!(missing.iter().all(|finding| finding.contains(".task_id:")));
        } else {
            assert!(missing.is_empty(), "{missing:?}");
        }
    }
    db.drop().await;
}

#[tokio::test]
async fn cleanup_overdue_uses_terminal_time_and_requires_pending_failure() {
    let db = TestDb::create().await;
    let result: (bool,bool,bool,bool,bool,bool) = sqlx::query_as(
        r#"
        SELECT public.formal_review_cleanup_overdue(NULL,NULL,'failed'),
            public.formal_review_cleanup_overdue(clock_timestamp()-interval '24 hours',NULL,'failed'),
            public.formal_review_cleanup_overdue(clock_timestamp()-interval '24 hours'+interval '1 second',NULL,'failed'),
            public.formal_review_cleanup_overdue(now()-interval '25 hours',now(),'failed'),
            public.formal_review_cleanup_overdue(now()-interval '25 hours',NULL,NULL),
            public.formal_review_cleanup_overdue(now(),NULL,'failed')
        "#,
    ).fetch_one(&db.pool).await.unwrap();
    assert_eq!(result, (false, true, false, false, false, false));
    db.drop().await;
}
