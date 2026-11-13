//! Database enforcement for issue #609, including populated upgrades through
//! the production executable as a non-superuser without BYPASSRLS.

use crate::helpers::{TestApp, TestDb};
use sqlx::{PgConnection, PgPool};
use std::path::Path;
use std::process::Output;
use tokio::process::Command;
use uuid::Uuid;

const TABLES: [&str; 3] = ["notifications", "usage_records", "provider_quota_snapshots"];
const CREDENTIAL_TABLES: [&str; 2] = ["api_keys", "worker_tokens"];

struct Tenant {
    org: Uuid,
    workspace: Uuid,
    user: Uuid,
}

impl Tenant {
    fn insert(&self, table: &str) -> String {
        let Self {
            org,
            workspace,
            user,
        } = self;
        match table {
            "notifications" => format!(
                "INSERT INTO notifications(workspace_id,recipient_user_id,title,message)
                 VALUES('{workspace}','{user}','Private notice','Private message')"
            ),
            "usage_records" => format!(
                "INSERT INTO usage_records(org_id,workspace_id,provider,model,cost_usd)
                 VALUES('{org}','{workspace}','anthropic','test-model',0.25)"
            ),
            "provider_quota_snapshots" => format!(
                "INSERT INTO provider_quota_snapshots(org_id,workspace_id,provider,auth_mode,captured_at)
                 VALUES('{org}','{workspace}','{}','oauth',now())",
                Uuid::new_v4()
            ),
            _ => unreachable!(),
        }
    }
}

async fn seed_tenant(pool: &PgPool) -> Tenant {
    let tenant = Tenant {
        org: Uuid::new_v4(),
        workspace: Uuid::new_v4(),
        user: Uuid::new_v4(),
    };
    let mut identity_tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'RLS regression',$2)")
        .bind(tenant.org)
        .bind(format!("rls-{}", tenant.org.simple()))
        .execute(&mut *identity_tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'RLS owner')")
        .bind(tenant.user)
        .bind(format!("{}@rls.test", tenant.user))
        .execute(&mut *identity_tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'admin')")
        .bind(tenant.org)
        .bind(tenant.user)
        .execute(&mut *identity_tx)
        .await
        .unwrap();
    identity_tx.commit().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by)
         VALUES($1,$2,'rls-test',$3,$3)",
    )
    .bind(tenant.workspace)
    .bind(tenant.org)
    .bind(tenant.user)
    .execute(pool)
    .await
    .unwrap();
    for table in TABLES {
        sqlx::query(&tenant.insert(table))
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO api_keys(id,org_id,user_id,name,key_prefix,key_hash)
         VALUES($1,$2,$1,'RLS key',$3,$3)",
    )
    .bind(tenant.user)
    .bind(tenant.org)
    .bind(tenant.user.to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO worker_tokens(id,org_id,scope,workspace_id,key_prefix,key_hash,created_by)
         VALUES($1,$2,'workspace',$3,$4,$4,$1)",
    )
    .bind(tenant.user)
    .bind(tenant.org)
    .bind(tenant.workspace)
    .bind(tenant.user.to_string())
    .execute(pool)
    .await
    .unwrap();
    tenant
}

async fn check_credential_helpers(system: &PgPool, tenant: &Tenant) {
    for function in [
        "authz.lookup_api_key_by_hash(text)",
        "authz.lookup_worker_token_by_hash(text)",
        "authz.touch_api_key_last_used(uuid,text)",
    ] {
        let tenant_can_call: bool =
            sqlx::query_scalar("SELECT has_function_privilege('g5_tenant',$1,'EXECUTE')")
                .bind(function)
                .fetch_one(system)
                .await
                .unwrap();
        assert!(!tenant_can_call, "{function} is a system-only entry point");
    }
    for function in ["lookup_api_key_by_hash", "lookup_worker_token_by_hash"] {
        let org: Uuid = sqlx::query_scalar(&format!("SELECT org_id FROM authz.{function}($1)"))
            .bind(tenant.user.to_string())
            .fetch_one(system)
            .await
            .unwrap();
        assert_eq!(org, tenant.org, "pre-context {function}");
        let missing: Option<Uuid> =
            sqlx::query_scalar(&format!("SELECT org_id FROM authz.{function}($1)"))
                .bind("unknown-hash")
                .fetch_optional(system)
                .await
                .unwrap();
        assert_eq!(missing, None);
    }
    let touched: bool = sqlx::query_scalar("SELECT authz.touch_api_key_last_used($1,'127.0.0.1')")
        .bind(tenant.user)
        .fetch_one(system)
        .await
        .unwrap();
    assert!(touched, "pre-context last-used metadata update");
    for table in CREDENTIAL_TABLES {
        let visible: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(system)
            .await
            .unwrap();
        assert_eq!(
            visible, 0,
            "system role must still use the narrow {table} helpers"
        );
    }
}

async fn reject_write(conn: &mut PgConnection, statement: &str) {
    sqlx::query("SAVEPOINT rejected_write")
        .execute(&mut *conn)
        .await
        .unwrap();
    let error = sqlx::query(statement)
        .execute(&mut *conn)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501"),
        "{error}"
    );
    assert!(error.to_string().contains("row-level security"), "{error}");
    sqlx::query("ROLLBACK TO SAVEPOINT rejected_write")
        .execute(conn)
        .await
        .unwrap();
}

#[tokio::test]
async fn tenant_queries_without_filters_cannot_read_or_mutate_other_organizations() {
    let app = TestApp::spawn().await;
    let a = seed_tenant(&app.db).await;
    let b = seed_tenant(&app.db).await;
    check_credential_helpers(&app.state.system_pool, &a).await;
    check_credential_helpers(&app.state.system_pool, &b).await;

    for (own, other) in [(&a, &b), (&b, &a)] {
        let mut db = app.scoped_db_for_org(own.org).await;
        for table in TABLES {
            let visible: Vec<Uuid> =
                sqlx::query_scalar(&format!("SELECT workspace_id FROM {table}"))
                    .fetch_all(db.conn())
                    .await
                    .unwrap();
            assert_eq!(visible, vec![own.workspace], "unfiltered read of {table}");
            let updated = sqlx::query(&format!("UPDATE {table} SET workspace_id = workspace_id"))
                .execute(db.conn())
                .await
                .unwrap();
            assert_eq!(updated.rows_affected(), 1, "unfiltered update of {table}");

            reject_write(db.conn(), &other.insert(table)).await;
            let changed_scope = if table == "notifications" {
                format!("workspace_id = '{}'", other.workspace)
            } else {
                format!("org_id = '{}'", other.org)
            };
            reject_write(db.conn(), &format!("UPDATE {table} SET {changed_scope}")).await;

            // Legitimate same-tenant inserts remain supported, including the
            // notification writer's existing shape without an org_id column.
            sqlx::query(&own.insert(table))
                .execute(db.conn())
                .await
                .unwrap();
            if table != "provider_quota_snapshots" {
                let deleted = sqlx::query(&format!("DELETE FROM {table}"))
                    .execute(db.conn())
                    .await
                    .unwrap();
                assert_eq!(deleted.rows_affected(), 2, "unfiltered delete of {table}");
            }
        }
        db.rollback().await.unwrap();
    }

    let mut unscoped = app.state.tenant_pool.begin().await.unwrap();
    let mut system = app.state.system_pool.begin().await.unwrap();
    let system_privileges: (bool, bool) =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
            .fetch_one(&mut *system)
            .await
            .unwrap();
    assert_eq!(
        system_privileges,
        (false, false),
        "system access must use policies"
    );
    for table in TABLES {
        let count = format!("SELECT count(*) FROM {table}");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&count)
                .fetch_one(&mut *unscoped)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&count)
                .fetch_one(&mut *system)
                .await
                .unwrap(),
            2
        );
        reject_write(&mut unscoped, &a.insert(table)).await;
        // Internal writers (including flush-block notifications) still work
        // on the system pool without inventing a tenant authz context.
        sqlx::query(&a.insert(table))
            .execute(&mut *system)
            .await
            .unwrap();
        sqlx::query(&b.insert(table))
            .execute(&mut *system)
            .await
            .unwrap();
    }
    unscoped.rollback().await.unwrap();
    system.rollback().await.unwrap();
    app.cleanup().await;
}

async fn run_migrations(database_url: &str, source: &Path) -> Output {
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
    .expect("migration timed out")
    .expect("run migration executable")
}

#[tokio::test]
async fn populated_upgrade_and_future_migrations_enforce_rls_as_the_deployment_owner() {
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
                < 87
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let a = seed_tenant(&db.pool).await;
    let b = seed_tenant(&db.pool).await;
    let name = "087_tenant_table_rls.sql";
    std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();

    let role = format!("tenant_rls_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'tenant_rls_test' NOSUPERUSER NOBYPASSRLS;
         GRANT USAGE, CREATE ON SCHEMA public, authz TO {role} WITH GRANT OPTION;
         GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
         GRANT SELECT ON public.workspaces TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};
         ALTER TABLE public.notifications OWNER TO {role};
         ALTER TABLE public.usage_records OWNER TO {role};
         ALTER TABLE public.provider_quota_snapshots OWNER TO {role};
         ALTER TABLE public.api_keys OWNER TO {role};
         ALTER TABLE public.worker_tokens OWNER TO {role};
         ALTER FUNCTION authz.lookup_api_key_by_hash(text) OWNER TO {role};
         ALTER FUNCTION authz.lookup_worker_token_by_hash(text) OWNER TO {role};
         ALTER FUNCTION authz.touch_api_key_last_used(uuid,text) OWNER TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    let mut url = url::Url::parse(&db.system_url).unwrap();
    url.set_username(&role).unwrap();
    url.set_password(Some("tenant_rls_test")).unwrap();
    let owner = PgPool::connect(url.as_str()).await.unwrap();
    let privileges: (bool, bool) =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(privileges, (false, false));
    for table in TABLES.into_iter().chain(CREDENTIAL_TABLES) {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&owner)
                .await
                .unwrap(),
            2,
            "pre-upgrade rows in {table}"
        );
    }
    let applied = run_migrations(url.as_str(), catalog.path()).await;
    assert!(applied.status.success(), "populated upgrade: {applied:?}");
    let replay = run_migrations(url.as_str(), catalog.path()).await;
    assert!(replay.status.success(), "no-op replay: {replay:?}");
    for table in TABLES.into_iter().chain(CREDENTIAL_TABLES) {
        let count = format!("SELECT count(*) FROM {table}");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&count)
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            2,
            "rows preserved in {table}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&count)
                .fetch_one(&owner)
                .await
                .unwrap(),
            0,
            "FORCE applies to the owner of {table}"
        );
    }
    let standing_bypass: bool =
        sqlx::query_scalar("SELECT pg_has_role(current_user,'authz_lookup_definer','SET')")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert!(
        !standing_bypass,
        "migration must hand back its temporary SET capability"
    );
    let system = PgPool::connect(&db.system_url).await.unwrap();
    check_credential_helpers(&system, &a).await;
    check_credential_helpers(&system, &b).await;
    system.close().await;

    // Future migrations must fail atomically for either flag, table-level
    // grants, and column-only grants, even in a different application schema.
    let future = catalog.path().join("088_rls_regression_probe.sql");
    for statement in [
        "CREATE TABLE public.rls_probe(id uuid); GRANT SELECT ON public.rls_probe TO g5_tenant;",
        "CREATE TABLE public.rls_probe(id uuid); GRANT SELECT(id) ON public.rls_probe TO g5_tenant;",
        "CREATE TABLE public.rls_probe(id uuid); GRANT SELECT ON public.rls_probe TO PUBLIC;",
        "CREATE TABLE authz.rls_probe(id uuid) PARTITION BY HASH(id); GRANT INSERT ON authz.rls_probe TO g5_tenant;",
        "ALTER TABLE public.usage_records NO FORCE ROW LEVEL SECURITY;",
        "ALTER TABLE public.usage_records DISABLE ROW LEVEL SECURITY;",
    ] {
        std::fs::write(&future, statement).unwrap();
        let rejected = run_migrations(url.as_str(), catalog.path()).await;
        assert!(!rejected.status.success(), "accepted {statement}: {rejected:?}");
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("must enable and force row-level security"), "{rejected:?}");
        let probe_exists: bool = sqlx::query_scalar(
            "SELECT to_regclass('public.rls_probe') IS NOT NULL OR to_regclass('authz.rls_probe') IS NOT NULL",
        ).fetch_one(&db.pool).await.unwrap();
        assert!(!probe_exists, "failed migration left an unprotected table behind");
        let recorded: bool = sqlx::query_scalar("SELECT EXISTS(SELECT FROM _sqlx_migrations WHERE version = 88)")
            .fetch_one(&db.pool).await.unwrap();
        assert!(!recorded, "failed migration must not leave a ledger entry");
        sqlx::query("SELECT authz.assert_tenant_tables_force_rls()")
            .execute(&owner).await.unwrap();
    }
    std::fs::write(
        &future,
        "CREATE TABLE public.rls_probe(id uuid);
         ALTER TABLE public.rls_probe ENABLE ROW LEVEL SECURITY;
         ALTER TABLE public.rls_probe FORCE ROW LEVEL SECURITY;
         GRANT SELECT ON public.rls_probe TO g5_tenant;",
    )
    .unwrap();
    let valid = run_migrations(url.as_str(), catalog.path()).await;
    assert!(valid.status.success(), "protected table: {valid:?}");
    std::fs::write(
        catalog.path().join("089_rls_index_probe.sql"),
        "-- no-transaction\nCREATE INDEX CONCURRENTLY rls_probe_id ON public.rls_probe(id);",
    )
    .unwrap();
    let concurrent = run_migrations(url.as_str(), catalog.path()).await;
    assert!(
        concurrent.status.success(),
        "nontransactional migration: {concurrent:?}"
    );

    // No pending migration is not a reason to overlook deployed schema drift.
    sqlx::query("ALTER TABLE public.notifications NO FORCE ROW LEVEL SECURITY")
        .execute(&owner)
        .await
        .unwrap();
    let drift = run_migrations(url.as_str(), catalog.path()).await;
    assert!(
        !drift.status.success(),
        "no-op replay accepted drift: {drift:?}"
    );
    assert!(
        String::from_utf8_lossy(&drift.stderr).contains("public.notifications"),
        "{drift:?}"
    );
    sqlx::query("ALTER TABLE public.notifications FORCE ROW LEVEL SECURITY")
        .execute(&owner)
        .await
        .unwrap();
    let repaired = run_migrations(url.as_str(), catalog.path()).await;
    assert!(repaired.status.success(), "repaired replay: {repaired:?}");

    owner.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
}
