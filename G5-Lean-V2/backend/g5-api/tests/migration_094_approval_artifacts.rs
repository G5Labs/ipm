//! Restored output storage: real migration-role execution and runtime tenant boundaries.
use crate::helpers::TestDb;
use g5_authz::{AuthzService, InProcessAuthz};
use g5_contracts::types::ids::{OrgId, UserId};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

const MIGRATION: &str = "094_restore_agent_approval_artifacts.sql";
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Clone, Copy)]
struct Assignment {
    org: Uuid,
    user: Uuid,
    workspace: Uuid,
    request: Uuid,
    task: Uuid,
}

async fn seed(pool: &PgPool) -> Result<Assignment, sqlx::Error> {
    let a = Assignment {
        org: Uuid::new_v4(),
        user: Uuid::new_v4(),
        workspace: Uuid::new_v4(),
        request: Uuid::new_v4(),
        task: Uuid::new_v4(),
    };
    let definition = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Artifact limits',$1::text)")
        .bind(a.org)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$1::text || '@test.invalid','Reviewer')").bind(a.user).execute(pool).await?;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(a.org)
        .bind(a.user)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,'review-source',$3,$3)").bind(a.workspace).bind(a.org).bind(a.user).execute(pool).await?;
    sqlx::query("INSERT INTO approval_definitions(id,version,org_id,name,humans_list,created_by) VALUES($1,1,$2,'Approval',ARRAY[$3]::uuid[],$3)").bind(definition).bind(a.org).bind(a.user).execute(pool).await?;
    sqlx::query("INSERT INTO approval_requests(id,org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,requested_by,phase) VALUES($1,$2,$3,$4,1,$5,'agent',$6,'ie')").bind(a.request).bind(a.org).bind(a.workspace).bind(definition).bind(format!("{:064x}",a.request.as_u128())).bind(a.user).execute(pool).await?;
    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
        .bind(session)
        .bind(a.org)
        .bind(a.workspace)
        .bind(a.user)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by,chat_session_id) VALUES($1,$2,$3,'Review','agent','queued',$4,$5)").bind(a.task).bind(a.org).bind(a.workspace).bind(a.user).bind(session).execute(pool).await?;
    Ok(a)
}

// This catalog predates membership activity, which current user admission and
// ScopedDb require. Use the stable session writer and the catalog's original
// set_ctx, and clear that context before commit as ScopedDb does.
async fn historical_tenant_scope(
    service: &InProcessAuthz,
    tenant: &PgPool,
    org: Uuid,
    user: Uuid,
) -> Result<Transaction<'static, Postgres>, Box<dyn std::error::Error>> {
    let cap = service
        .mint_service_db_capability(
            UserId::from_uuid(user),
            OrgId::from_uuid(org),
            &["*".into()],
        )
        .await?;
    let mut tx = tenant.begin().await?;
    sqlx::query("SELECT authz.set_ctx($1)")
        .bind(cap.session_id.into_uuid())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

async fn commit_historical_scope(
    mut tx: Transaction<'static, Postgres>,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("SELECT authz.clear_ctx()")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn insert(
    conn: &mut PgConnection,
    a: Assignment,
    document: &str,
    result: &str,
    digest: Option<&str>,
) -> Result<(), sqlx::Error> {
    let correct = format!("{:x}", Sha256::digest(document.as_bytes()));
    sqlx::query("INSERT INTO agent_approval_artifacts(org_id,source_workspace_id,approval_request_id,task_id,result,document,content_sha256) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(a.org).bind(a.workspace).bind(a.request).bind(a.task).bind(result).bind(document).bind(digest.unwrap_or(&correct)).execute(conn).await?;
    Ok(())
}
async fn reject(
    conn: &mut PgConnection,
    a: Assignment,
    document: &str,
    result: &str,
    digest: Option<&str>,
    code: &str,
    constraint: Option<&str>,
) -> TestResult {
    sqlx::query("SAVEPOINT rejected_artifact")
        .execute(&mut *conn)
        .await?;
    let error = insert(conn, a, document, result, digest).await.unwrap_err();
    let db_error = error.as_database_error().unwrap();
    assert_eq!(db_error.code().as_deref(), Some(code), "{error}");
    if let Some(name) = constraint {
        assert_eq!(db_error.constraint(), Some(name), "{error}");
    }
    sqlx::query("ROLLBACK TO SAVEPOINT rejected_artifact")
        .execute(conn)
        .await?;
    Ok(())
}
fn document(fm: &str, body: &str) -> String {
    format!("---\n{fm}\n---\n{body}")
}

#[tokio::test]
async fn populated_upgrade_replay_limits_digest_and_tenant_isolation() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|e| e == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "094"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let a = seed(&db.pool).await.unwrap();
    let b = seed(&db.pool).await.unwrap();
    let role = format!("artifact_migrator_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'disposable_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE,CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};")).execute(&db.pool).await.unwrap();
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
                .password("disposable_test"),
        )
        .await
        .unwrap();
    let system = PgPool::connect(&db.system_url).await.unwrap();
    let authz = PgPool::connect(&db.authz_url).await.unwrap();
    let tenant = PgPool::connect(&db.tenant_url).await.unwrap();
    let result:TestResult=async {
        let privileges:(bool,bool)=sqlx::query_as("SELECT rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&migrator).await?;
        assert_eq!(privileges,(false,false));
        std::fs::copy(source.join(MIGRATION),catalog.path().join(MIGRATION))?;
        for _ in 0..2 { g5_api::db::run_migrations_from_path(&migrator,catalog.path()).await?; }
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM approval_requests").fetch_one(&db.pool).await?,2);
        sqlx::query("SELECT authz.assert_tenant_tables_force_rls()").execute(&migrator).await?;
        let service=InProcessAuthz::new(system.clone(),authz.clone());
        let mut scope=historical_tenant_scope(&service,&tenant,a.org,a.user).await?;
        let fm="schemaVersion: 1\nresult: approved";
        let exact=document(fm,&"é".repeat((262144-document(fm,"").len())/2));
        let exact=exact.clone()+&"x".repeat(262144-exact.len());
        assert_eq!(exact.len(),262144);
        insert(&mut scope,a,&exact,"approved",None).await?;
        reject(&mut scope,a,&(exact+"x"),"approved",None,"23514",Some("agent_approval_artifacts_output_size")).await?;
        let padded=format!("{fm}\n#{}","é".repeat((32768-fm.len()-2)/2));
        let padded=padded.clone()+&"x".repeat(32768-padded.len());
        insert(&mut scope,a,&document(&padded,"Reviewed."),"rejected",None).await?;
        reject(&mut scope,a,&document(&(padded+"x"),"Reviewed."),"approved",None,"23514",Some("agent_approval_artifacts_frontmatter_size")).await?;
        let valid=document(fm,"Reviewed.");
        reject(&mut scope,a,&valid,"approved",Some(&"0".repeat(64)),"23514",Some("agent_approval_artifacts_digest")).await?;
        reject(&mut scope,a,"missing frontmatter","approved",None,"23514",Some("agent_approval_artifacts_frontmatter_size")).await?;
        reject(&mut scope,a,&valid,"deferred",None,"23514",None).await?;
        reject(&mut scope,Assignment{request:b.request,..a},&valid,"approved",None,"23503",None).await?;
        reject(&mut scope,Assignment{task:b.task,..a},&valid,"approved",None,"23503",None).await?;
        reject(&mut scope,b,&valid,"approved",None,"42501",None).await?;
        let rows:Vec<Uuid>=sqlx::query_scalar("SELECT org_id FROM agent_approval_artifacts").fetch_all(&mut *scope).await?;
        assert_eq!(rows,vec![a.org,a.org]);
        commit_historical_scope(scope).await?;
        let mut conn=db.pool.acquire().await?;
        insert(&mut conn,b,&valid,"approved",None).await?;
        let mut scope=historical_tenant_scope(&service,&tenant,b.org,b.user).await?;
        let rows:Vec<Uuid>=sqlx::query_scalar("SELECT org_id FROM agent_approval_artifacts").fetch_all(&mut *scope).await?;
        assert_eq!(rows,vec![b.org]);
        commit_historical_scope(scope).await?;
        for pool in [&tenant,&migrator,&system] { assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM agent_approval_artifacts").fetch_one(pool).await?,0,"unscoped roles fail closed"); }
        let grants:(bool,bool)=sqlx::query_as("SELECT has_table_privilege('g5_tenant','agent_approval_artifacts','UPDATE'),has_table_privilege('g5_tenant','agent_approval_artifacts','DELETE')").fetch_one(&db.pool).await?;
        assert_eq!(grants,(false,false));
        let stored:(i32,String,String)=sqlx::query_as("SELECT octet_length(document),document,content_sha256 FROM agent_approval_artifacts WHERE org_id=$1 ORDER BY octet_length(document) DESC LIMIT 1").bind(a.org).fetch_one(&db.pool).await?;
        assert_eq!(stored.0,262144);
        assert_eq!(stored.2,format!("{:x}",Sha256::digest(stored.1.as_bytes())));
        Ok(())
    }.await;
    tenant.close().await;
    authz.close().await;
    system.close().await;
    migrator.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
    result.unwrap();
}

#[tokio::test]
async fn formal_review_upgrade_fresh_populated_and_replay_with_restricted_migrator() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    for populated in [false, true] {
        let catalog = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            if entry.path().extension().is_some_and(|e| e == "sql")
                && entry.file_name().to_string_lossy().as_ref() < "095"
            {
                std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
            }
        }
        let db = TestDb::create_from_migrations(catalog.path()).await;
        if populated {
            seed(&db.pool).await.unwrap();
        }
        let role = format!("formal_migrator_{}", Uuid::new_v4().simple());
        sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'disposable_test' NOSUPERUSER NOBYPASSRLS; GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE,INHERIT FALSE,SET FALSE; GRANT USAGE,CREATE ON SCHEMA public TO {role}; GRANT USAGE,CREATE ON SCHEMA authz TO {role} WITH GRANT OPTION;")).execute(&db.pool).await.unwrap();
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
                    .password("disposable_test"),
            )
            .await
            .unwrap();
        let result:TestResult=async {
            let powers:(bool,bool)=sqlx::query_as("SELECT rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&migrator).await?;assert_eq!(powers,(false,false));
            std::fs::copy(source.join("095_formal_agent_approvals.sql"),catalog.path().join("095_formal_agent_approvals.sql"))?;
            for _ in 0..2 {g5_api::db::run_migrations_from_path(&migrator,catalog.path()).await?;}
            sqlx::query("SELECT authz.assert_tenant_tables_force_rls()").execute(&migrator).await?;
            let rows:i64=sqlx::query_scalar("SELECT count(*) FROM approval_requests").fetch_one(&db.pool).await?;assert_eq!(rows,i64::from(populated));
            let membership:(bool,bool,bool)=sqlx::query_as("SELECT admin_option,inherit_option,set_option FROM pg_auth_members m JOIN pg_roles r ON r.oid=m.member JOIN pg_roles granted ON granted.oid=m.roleid WHERE r.rolname=$1 AND granted.rolname='authz_lookup_definer'").bind(&role).fetch_one(&db.pool).await?;assert_eq!(membership,(true,false,false));
            // Every schema-wide denial must follow a real ownership FK, including
            // composite/transitive chains through attempts, files and repo state.
            let unowned: Vec<String> = sqlx::query_scalar(r#"
                WITH RECURSIVE roots AS (
                    SELECT c.oid AS rel, c.relname, p.polname,
                        CASE WHEN p.polname LIKE 'formal_review_private_task_%' THEN 'task_id'
                        WHEN p.polname LIKE 'formal_review_private_session_id_%' THEN 'session_id'
                        WHEN p.polname LIKE 'formal_review_private_chat_session_id_%' THEN 'chat_session_id'
                        ELSE 'workspace_id' END AS col,
                        CASE WHEN p.polname LIKE 'formal_review_private_task_%' THEN 'tasks'
                        WHEN p.polname LIKE 'formal_review_private_session_id_%' OR p.polname LIKE 'formal_review_private_chat_session_id_%' THEN 'chat_sessions' ELSE 'workspaces' END AS owner
                    FROM pg_policy p JOIN pg_class c ON c.oid=p.polrelid
                    WHERE p.polname LIKE 'formal_review_private%'
                      AND c.relname NOT IN ('workspaces','workspace_forks')
                ), edges AS (
                    SELECT conrelid AS src, a.attname AS col, confrelid AS dst, b.attname AS dstcol
                    FROM pg_constraint k CROSS JOIN LATERAL unnest(k.conkey,k.confkey) keys(src,dst)
                    JOIN pg_attribute a ON a.attrelid=k.conrelid AND a.attnum=keys.src
                    JOIN pg_attribute b ON b.attrelid=k.confrelid AND b.attnum=keys.dst
                    WHERE k.contype='f'
                ), walk AS (
                    SELECT relname,polname,owner,rel,col COLLATE "C",ARRAY[rel::text||'.'||col] COLLATE "C" AS visited FROM roots
                    UNION ALL
                    SELECT w.relname,w.polname,w.owner,e.dst,e.dstcol,w.visited||(e.dst::text||'.'||e.dstcol)
                    FROM walk w JOIN edges e ON e.src=w.rel AND e.col=w.col
                    WHERE NOT (e.dst::text||'.'||e.dstcol)=ANY(w.visited)
                )
                SELECT r.relname||'.'||r.col FROM roots r WHERE NOT EXISTS(
                    SELECT 1 FROM walk w JOIN pg_class c ON c.oid=w.rel
                    WHERE w.relname=r.relname AND w.polname=r.polname AND c.relname=r.owner AND w.col='id')
                ORDER BY 1
            "#).fetch_all(&db.pool).await?;
            assert!(unowned.is_empty(),"private audience policy lacks an ownership FK: {unowned:?}");
            let policies: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_policy WHERE polname LIKE 'formal_review_private%' AND NOT polpermissive").fetch_one(&db.pool).await?;
            assert_eq!(policies,192,"review every audience-policy inventory change");
            let wrong_roles: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_policy WHERE polname LIKE 'formal_review_private%' AND polroles <> ARRAY['g5_tenant'::regrole::oid]").fetch_one(&db.pool).await?;
            assert_eq!(wrong_roles,0,"human privacy must preserve existing infrastructure-role authority");
            let hidden:i64=sqlx::query_scalar("SELECT count(*) FROM agent_approval_reviews").fetch_one(&migrator).await?;assert_eq!(hidden,0);
            Ok(())
        }.await;
        migrator.close().await;
        sqlx::raw_sql(&format!(
            "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        db.drop().await;
        result.unwrap();
    }
}
