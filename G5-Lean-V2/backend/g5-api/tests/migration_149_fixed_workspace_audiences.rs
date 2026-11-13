//! Upgrade populated sharing state using the deployment runner without RLS bypass.
use crate::helpers::TestDb;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const MIGRATION: &str = "149_fixed_workspace_audiences.sql";

#[tokio::test]
async fn populated_upgrade_preserves_identity_history_and_formal_confinement() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "149"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let reader = Uuid::new_v4();
    let ordinary = Uuid::new_v4();
    let base = Uuid::new_v4();
    let personal = Uuid::new_v4();
    let deleted = Uuid::new_v4();
    let formal = Uuid::new_v4();
    let review = Uuid::new_v4();
    let definition = Uuid::new_v4();
    let request = Uuid::new_v4();
    sqlx::raw_sql(&format!(
        "INSERT INTO users(id,email,display_name) VALUES
            ('{owner}','{owner}@test.invalid','Owner'),('{reader}','{reader}@test.invalid','Reader');
         INSERT INTO orgs(id,name,slug) VALUES('{org}','Upgrade','{org}');
         INSERT INTO org_memberships(org_id,user_id,role) VALUES
            ('{org}','{owner}','owner'),('{org}','{reader}','member');
         INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by,visibility,is_base,personal_user_id,deleted_at) VALUES
            ('{ordinary}','{org}','ordinary','{owner}','{owner}','invite_only',false,NULL,NULL),
            ('{base}','{org}','Base','{owner}','{owner}','invite_only',true,NULL,NULL),
            ('{personal}','{org}','personal','{owner}','{owner}','invite_only',false,'{owner}',NULL),
            ('{deleted}','{org}','deleted','{owner}','{owner}','invite_only',false,NULL,now());
         INSERT INTO workspace_members(org_id,workspace_id,user_id,role,invited_by) VALUES
            ('{org}','{ordinary}','{reader}','viewer','{owner}'),
            ('{org}','{personal}','{reader}','viewer','{owner}');
         INSERT INTO workspace_pull_requests(org_id,source_workspace_id,target_workspace_id,local_merge_status,title,last_preflight_fingerprint,created_by)
            VALUES('{org}','{deleted}','{base}','ready','Retained history','before','{owner}');
         INSERT INTO approval_definitions(id,version,org_id,name,humans_list,created_by)
            VALUES('{definition}',1,'{org}','Review',ARRAY['{owner}'::uuid],'{owner}');
         INSERT INTO approval_requests(id,org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,requested_by)
            VALUES('{request}','{org}','{ordinary}','{definition}',1,repeat('a',64),'agent','{owner}');
         INSERT INTO agent_approval_reviews(id,org_id,source_workspace_id,approval_request_id,requested_by,target_workspace_id,review_workspace_id,definition_snapshot)
            VALUES('{review}','{org}','{ordinary}','{request}','{owner}','{base}','{formal}','{{}}');
         INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by,visibility,agent_approval_review_id)
            VALUES('{formal}','{org}','formal','{owner}','{owner}','org','{review}');"
    )).execute(&db.pool).await.unwrap();
    let rows_before: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(w)-'visibility' FROM workspaces w ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let history_before: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM workspace_pull_requests p")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let checksums_before: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let policies_before: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT policyname::text,qual,with_check FROM pg_policies WHERE schemaname='public' AND tablename='workspaces' ORDER BY policyname",
    ).fetch_all(&db.pool).await.unwrap();

    let role = format!("audience_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
         GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
         GRANT USAGE, CREATE ON SCHEMA public TO {role}; ALTER SCHEMA authz OWNER TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};
         ALTER TABLE public.workspaces OWNER TO {role};
         ALTER TABLE public.workspace_members OWNER TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    let probe: Result<((bool, bool), i64, bool), String> = async {
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
                "SELECT rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user",
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
            let can_set: bool =
                sqlx::query_scalar("SELECT pg_has_role(current_user,'authz_lookup_definer','SET')")
                    .fetch_one(&migrator)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok((privileges, visible, can_set))
        }
        .await;
        migrator.close().await;
        result
    }
    .await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        probe.expect("non-superuser coordinated upgrade and no-op replay"),
        ((false, false), 0, false)
    );
    let retired: bool = sqlx::query_scalar(
        "SELECT to_regclass('public.workspace_members') IS NULL AND NOT EXISTS
         (SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='workspaces' AND column_name='visibility')",
    ).fetch_one(&db.pool).await.unwrap();
    assert!(retired);
    let rows_after: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(w) FROM workspaces w ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        rows_before, rows_after,
        "identities and owner/lifecycle state are unchanged"
    );
    let history_after: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM workspace_pull_requests p")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(history_before, history_after);
    let checksums_after: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version,checksum FROM _sqlx_migrations WHERE version < 149 ORDER BY version",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(checksums_before, checksums_after);
    let policies_after: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT policyname::text,qual,with_check FROM pg_policies WHERE schemaname='public' AND tablename='workspaces' ORDER BY policyname",
    ).fetch_all(&db.pool).await.unwrap();
    assert_eq!(policies_before, policies_after);

    // The shared rule covers retained history too; caller liveness stays separate.
    for (actor, admin, expected) in [
        (reader, false, vec![ordinary, base, deleted]),
        (reader, true, vec![ordinary, base, personal, deleted]),
        (owner, false, vec![ordinary, base, personal, deleted]),
    ] {
        let actual: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM workspaces WHERE authz.workspace_audience_allows(personal_user_id,owner_user_id,agent_approval_review_id,$1,$2)",
        ).bind(actor).bind(admin).fetch_all(&db.pool).await.unwrap();
        assert_eq!(actual.len(), expected.len());
        for id in expected {
            assert!(actual.contains(&id));
        }
        assert!(
            !actual.contains(&formal),
            "formal identity precedes admin/owner"
        );
    }
    // Tenant grants remain usable and the formal RLS boundary remains restrictive.
    let mut tenant = db.pool.begin().await.unwrap();
    sqlx::query("INSERT INTO authz.session_ctx(pid,org_ids,user_id,scopes) VALUES(pg_backend_pid(),ARRAY[$1],$2,ARRAY['*']) ON CONFLICT(pid) DO UPDATE SET org_ids=EXCLUDED.org_ids,user_id=EXCLUDED.user_id,scopes=EXCLUDED.scopes")
        .bind(org).bind(owner).execute(&mut *tenant).await.unwrap();
    sqlx::query("SET LOCAL ROLE g5_tenant")
        .execute(&mut *tenant)
        .await
        .unwrap();
    let formal_visible: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE id=$1")
        .bind(formal)
        .fetch_one(&mut *tenant)
        .await
        .unwrap();
    assert_eq!(
        formal_visible, 0,
        "even the human owner cannot enter the formal runtime"
    );
    let allowed: bool =
        sqlx::query_scalar("SELECT authz.workspace_audience_allows(NULL,$1,NULL,$2,false)")
            .bind(owner)
            .bind(reader)
            .fetch_one(&mut *tenant)
            .await
            .unwrap();
    assert!(allowed);
    tenant.rollback().await.unwrap();
    db.drop().await;
}

#[tokio::test]
async fn fresh_database_has_no_workspace_sharing_state() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let db = TestDb::create_from_migrations(&source).await;
    let (retired, forced): (bool, bool) = sqlx::query_as(
        "SELECT to_regclass('public.workspace_members') IS NULL AND NOT EXISTS
         (SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='workspaces' AND column_name='visibility'),
         relrowsecurity AND relforcerowsecurity FROM pg_class WHERE oid='workspaces'::regclass",
    ).fetch_one(&db.pool).await.unwrap();
    assert!(retired && forced);
    g5_api::db::run_migrations(&db.pool).await.unwrap();
    db.drop().await;
}
