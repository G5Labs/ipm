//! Exercise the deployed migration runner on populated, forced-RLS tables.
use crate::helpers;

use g5_authz::{AuthzService, InProcessAuthz};
use g5_contracts::types::ids::{OrgId, UserId};
use helpers::TestDb;
use sha2::{Digest, Sha384};
use sqlx::{postgres::PgPoolOptions, PgConnection, PgPool, Postgres, Transaction};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

const PASSWORD: &str = "migration_045_disposable_test";
const ORIGINAL_045: &str =
    include_str!("../src/db/migrations/045_drop_review_document_targets.sql");
const FILE_045: &str = "045_drop_review_document_targets.sql";
const HOST_MIGRATIONS: [(i64, &str, &str); 2] = [
    (
        47,
        "047_host_reported_vm_custody.sql",
        include_str!("../src/db/migrations/047_host_reported_vm_custody.sql"),
    ),
    (
        48,
        "048_host_custody_legacy_writers.sql",
        include_str!("../src/db/migrations/048_host_custody_legacy_writers.sql"),
    ),
];

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn catalog_through(last: i64) -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql") {
            let version: i64 = entry
                .file_name()
                .to_string_lossy()
                .split('_')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            if version <= last {
                std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
            }
        }
    }
    catalog
}

struct Fixture {
    db: TestDb,
    role: String,
    migrator: PgPool,
}

impl Fixture {
    async fn new(last: i64) -> Self {
        let baseline = catalog_through(last);
        let db = TestDb::create_from_migrations(baseline.path()).await;
        let role = format!("migration_045_{}", Uuid::new_v4().simple());
        // Provision the Cloud SQL-like role independently from the runtime
        // logins. ADMIN-only definer membership matches the CI bootstrap.
        sqlx::raw_sql(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{PASSWORD}' NOSUPERUSER NOBYPASSRLS CREATEDB CREATEROLE;
             GRANT CREATE ON DATABASE \"{database}\" TO {role};
             GRANT USAGE, CREATE ON SCHEMA public TO {role};
             GRANT authz_ctx_writer, authz_writer TO {role} WITH ADMIN OPTION;
             GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
             CREATE EXTENSION IF NOT EXISTS vector;
             CREATE EXTENSION IF NOT EXISTS pgcrypto;",
            database = db.db_name
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        if last > 0 {
            // The deployed migrator creates authz in the baseline and owns it.
            // Later migrations need that ownership to grant temporary CREATE
            // to the definer when replacing functions with new return types.
            // The deployed migrator also owns the baseline request guard,
            // which migration089 replaces while preserving its constraints.
            sqlx::raw_sql(&format!(
                "ALTER SCHEMA authz OWNER TO {role}; \
                 ALTER FUNCTION public.guard_approval_request_update() OWNER TO {role}"
            ))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        if (1..87).contains(&last) {
            // The baseline creates these bootstrap helpers as the deployment
            // owner; 087 moves them to the BYPASSRLS definer before forcing
            // RLS on their credential tables. Model their original ownership
            // when handing a superuser-seeded historical catalog to this role.
            sqlx::raw_sql(&format!(
                "ALTER FUNCTION authz.lookup_api_key_by_hash(text) OWNER TO {role};
                 ALTER FUNCTION authz.lookup_worker_token_by_hash(text) OWNER TO {role};
                 ALTER FUNCTION authz.touch_api_key_last_used(uuid,text) OWNER TO {role};"
            ))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
             WHERE n.nspname='public' AND c.relkind IN ('r','p')",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        for table in tables {
            sqlx::raw_sql(&format!(
                "ALTER TABLE public.\"{}\" OWNER TO {role}",
                table.replace('"', "\"\"")
            ))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        // The baseline creates these ordinary public functions as the migrator.
        // Hand them over with the seeded tables so upgrades through 129 can
        // retire them under the same ownership as a deployed migration run.
        let functions: Vec<String> = sqlx::query_scalar(
            "SELECT oid::regprocedure::text FROM pg_proc
             WHERE pronamespace='public'::regnamespace AND prokind='f'
               AND (proname LIKE 'approval_review_%' OR proname LIKE 'agent_approval_%'
                    OR proname IN ('reject_approval_context_update',
                        'validate_agent_approval_status_v3_run',
                        'validate_agent_approval_status_v4_run',
                        'invalidate_workspace_repo_projections',
                        'invalidate_default_ref_projections',
                        'repo_webhook_secret_for_current_org'))",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        for function in functions {
            sqlx::raw_sql(&format!("ALTER FUNCTION {function} OWNER TO {role}"))
                .execute(&db.pool)
                .await
                .unwrap();
        }
        if last >= 3 {
            // Migration 003 creates these ordinary functions as the migrator.
            // Match that ownership when handing the seeded catalog to the
            // non-superuser, so migrations 076, 083, 099, 140 and 150 can replace
            // their bodies.
            sqlx::raw_sql(&format!(
                "ALTER FUNCTION public.g5_provider_request_is_immutable() OWNER TO {role}; \
                 ALTER FUNCTION public.g5_config_value_is_registered(text, text, jsonb) OWNER TO {role}; \
                 ALTER FUNCTION public.g5_workspace_vm_identity_is_fenced() OWNER TO {role}; \
                 ALTER FUNCTION public.g5_start_intent_is_valid(jsonb) OWNER TO {role}; \
                 ALTER FUNCTION public.g5_resolve_eligible_workspace_creator() OWNER TO {role}"
            ))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        let migrator = PgPoolOptions::new()
            .max_connections(3)
            .connect_with(
                db.pool
                    .connect_options()
                    .as_ref()
                    .clone()
                    .username(&role)
                    .password(PASSWORD),
            )
            .await
            .unwrap();
        let privileges: (bool, bool) = sqlx::query_as(
            "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname=current_user",
        )
        .fetch_one(&migrator)
        .await
        .unwrap();
        assert_eq!(privileges, (false, false));
        Self { db, role, migrator }
    }

    async fn seed_review_rows(&self) -> TestResult {
        for i in 0..2 {
            let org = Uuid::new_v4();
            let user = Uuid::new_v4();
            let workspace = Uuid::new_v4();
            let publication = Uuid::new_v4();
            let repo = Uuid::new_v4();
            sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Historical review',$2)")
                .bind(org)
                .bind(format!("review-{org}"))
                .execute(&self.db.pool)
                .await?;
            sqlx::query(
                "INSERT INTO users(id,email,display_name) VALUES($1,$2,'Historical reviewer')",
            )
            .bind(user)
            .bind(format!("{user}@g5.test"))
            .execute(&self.db.pool)
            .await?;
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
                .bind(org)
                .bind(user)
                .execute(&self.db.pool)
                .await?;
            sqlx::query("INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) VALUES($1,$2,$3,false,$4,$4)")
                .bind(workspace).bind(org).bind(format!("review-{i}")).bind(user)
                .execute(&self.db.pool).await?;
            sqlx::query("INSERT INTO review_publications(id,org_id,source_workspace_id,reviewer_user_id,author_label,round) VALUES($1,$2,$3,$4,'reviewer',1)")
                .bind(publication).bind(org).bind(workspace).bind(user)
                .execute(&self.db.pool).await?;
            for (note, kind, before, after, target, applied, status) in [
                (
                    "accepted-target",
                    "new_file",
                    Some("old merge base"),
                    "proposal",
                    Some("docs/accepted.md"),
                    Some("owner edits"),
                    "accepted",
                ),
                (
                    "open-target",
                    "new_file",
                    Some("old merge base"),
                    "open proposal",
                    Some("docs/open.md"),
                    None,
                    "open",
                ),
                (
                    "plain-hunk",
                    "hunk",
                    Some("before"),
                    "after",
                    None,
                    None,
                    "accepted",
                ),
                (
                    "plain-new-file",
                    "new_file",
                    None,
                    "new file",
                    None,
                    None,
                    "open",
                ),
            ] {
                sqlx::query("INSERT INTO review_suggestions(org_id,publication_id,repo_id,path,kind,before_lines,after_lines,target_path,applied_content,author_label,note,status) VALUES($1,$2,$3,'docs/reviewer.md',$4,$5,$6,$7,$8,'reviewer',$9,$10)")
                    .bind(org).bind(publication).bind(repo).bind(kind).bind(before).bind(after)
                    .bind(target).bind(applied).bind(note).bind(status)
                    .execute(&self.db.pool).await?;
            }
        }
        assert_eq!(
            self.visible().await?,
            0,
            "the migrator begins blind under FORCE RLS"
        );
        Ok(())
    }

    async fn visible(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM public.review_suggestions")
            .fetch_one(&self.migrator)
            .await
    }

    async fn cleanup(self) {
        self.migrator.close().await;
        sqlx::raw_sql(&format!(
            "REASSIGN OWNED BY {0} TO CURRENT_USER; DROP OWNED BY {0}; DROP ROLE {0};",
            self.role
        ))
        .execute(&self.db.pool)
        .await
        .unwrap();
        self.db.drop().await;
    }
}

async fn assert_no_session_locks(f: &Fixture) -> TestResult {
    let locks: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a USING(pid) WHERE l.locktype='advisory' AND l.granted AND a.usename=$1")
        .bind(&f.role).fetch_one(&f.db.pool).await?;
    if locks != 0 {
        return Err(format!("migration runner leaked {locks} session advisory lock(s)").into());
    }
    Ok(())
}

async fn wait_for_no_session_locks(f: &Fixture) -> TestResult {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if assert_no_session_locks(f).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

async fn owner_update_gate(
    f: &Fixture,
) -> Result<PgConnection, Box<dyn std::error::Error + Send + Sync>> {
    sqlx::raw_sql("CREATE FUNCTION public.pause_045_update() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(45045); RETURN NEW; END $$;
        CREATE TRIGGER pause_045 BEFORE UPDATE ON public.review_suggestions FOR EACH ROW EXECUTE FUNCTION public.pause_045_update();")
        .execute(&f.db.pool).await?;
    let mut blocker = f.db.pool.acquire().await?.detach();
    sqlx::query("SELECT pg_advisory_lock(45045)")
        .execute(&mut blocker)
        .await?;
    Ok(blocker)
}

async fn wait_for_owner_update(f: &Fixture) -> TestResult {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_stat_activity a USING(pid) WHERE l.locktype='advisory' AND l.classid=0 AND l.objid=45045 AND NOT l.granted AND a.usename=$1)")
                .bind(&f.role).fetch_one(&f.db.pool).await?;
            if waiting { return Ok::<_,sqlx::Error>(()); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
    Ok(())
}

async fn assert_restored(f: &Fixture) -> TestResult {
    let flags: (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid='public.review_suggestions'::regclass",
    )
    .fetch_one(&f.db.pool)
    .await?;
    assert_eq!(flags, (true, true));
    let policy: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_policies WHERE schemaname='public' AND tablename='review_suggestions' AND policyname='tenant_isolation'",
    ).fetch_one(&f.db.pool).await?;
    assert_eq!(policy, 1);
    assert_eq!(f.visible().await?, 0);
    assert_no_session_locks(f).await
}

async fn assert_migrated_rows(f: &Fixture) -> TestResult {
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT note,before_lines,after_lines FROM public.review_suggestions ORDER BY note,org_id",
    )
    .fetch_all(&f.db.pool)
    .await?;
    assert_eq!(rows.len(), 8);
    for (note, before, after) in rows {
        let expected = match note.as_str() {
            "accepted-target" => (None, Some("owner edits")),
            "open-target" => (None, Some("open proposal")),
            "plain-hunk" => (Some("before"), Some("after")),
            "plain-new-file" => (None, Some("new file")),
            _ => panic!("unexpected review row"),
        };
        assert_eq!((before.as_deref(), after.as_deref()), expected, "{note}");
    }
    let columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='review_suggestions' AND column_name IN ('target_path','target_base','applied_content')",
    ).fetch_one(&f.db.pool).await?;
    assert_eq!(columns, 0);
    let constraint: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint WHERE conrelid='public.review_suggestions'::regclass AND conname='review_document_target_shape'",
    ).fetch_one(&f.db.pool).await?;
    assert_eq!(constraint, 0);
    let checksum: Vec<u8> = sqlx::query_scalar(
        "SELECT checksum FROM public._sqlx_migrations WHERE version=45 AND success",
    )
    .fetch_one(&f.db.pool)
    .await?;
    assert_eq!(checksum, Sha384::digest(ORIGINAL_045.as_bytes()).to_vec());
    assert_restored(f).await
}

/// Migration 137 (issues g5.issue.native.domain): every seeded tenant
/// organization had no issue domain before the upgrade and now tracks issues
/// with G5 issues, keyed from its slug, trusted, as its default creation
/// domain, configured by its original Owner. The full catalog reaches 137,
/// then 151 converts that person's membership to Admin without reattribution.
async fn assert_native_domains_backfilled(f: &Fixture) -> TestResult {
    let domains: Vec<(String, String, bool, bool, bool)> = sqlx::query_as(
        "SELECT d.provider, d.display_key, d.content_trusted, d.is_default_create_target, \
                EXISTS (SELECT 1 FROM public.org_memberships m \
                         WHERE m.org_id = d.org_id AND m.user_id = d.configured_by AND m.role = 'admin' \
                           AND EXISTS (SELECT 1 FROM public.workspaces w WHERE w.org_id=m.org_id \
                                       AND w.owner_user_id=m.user_id AND w.created_by=m.user_id)) \
           FROM public.issue_domains d JOIN public.orgs o ON o.id = d.org_id \
          WHERE o.name = 'Historical review' ORDER BY d.display_key",
    )
    .fetch_all(&f.db.pool)
    .await?;
    assert_eq!(domains.len(), 2, "{domains:?}");
    for (provider, key, trusted, is_default, by_original_person) in domains {
        assert_eq!(provider, "g5");
        assert!(key.starts_with("REVIEW") && key.len() == 10, "{key}");
        assert!(trusted && is_default && by_original_person);
    }
    Ok(())
}

#[tokio::test]
async fn populated_qa_upgrade_preserves_reviews_and_replays_without_bypassrls() {
    let f = Fixture::new(42).await;
    let result: TestResult = async {
        f.seed_review_rows().await?;
        seed_host_rows(&f, false).await?;
        let catalog = catalog_through(i64::MAX);
        let (first, concurrent) = tokio::join!(
            g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()),
            g5_api::db::run_migrations_from_path(&f.migrator, catalog.path())
        );
        first?;
        concurrent?;
        assert_migrated_rows(&f).await?;
        assert_native_domains_backfilled(&f).await?;
        // Embedded and runtime-directory entry points share the same owner.
        g5_api::db::run_migrations(&f.migrator).await?;
        assert_migrated_rows(&f).await?;
        assert_native_domains_backfilled(&f).await?;
        let pending: i64 =
            sqlx::query_scalar("SELECT max(version) FROM public._sqlx_migrations WHERE success")
                .fetch_one(&f.db.pool)
                .await?;
        let expected_last = sqlx::migrate::Migrator::new(catalog.path())
            .await?
            .iter()
            .map(|migration| migration.version)
            .max()
            .unwrap();
        assert_eq!(pending, expected_last);
        assert_host_rows(&f, 48, false).await?;
        Ok(())
    }
    .await;
    f.cleanup().await;
    result.unwrap();
}

#[tokio::test]
async fn owner_cleanup_failure_rolls_back_data_schema_force_and_ledger_then_retries() {
    let f = Fixture::new(44).await;
    let result: TestResult = async {
        f.seed_review_rows().await?;
        let catalog = catalog_through(45);
        // Test-only pending copy: fail after the UPDATE has seen both orgs.
        std::fs::write(catalog.path().join(FILE_045), ORIGINAL_045.replace(
            "DROP COLUMN applied_content", "DROP COLUMN nonexistent_column"
        ))?;
        let failed = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
        assert!(matches!(failed, Err(g5_api::db::DbError::Migration(_))), "{failed:?}");
        wait_for_no_session_locks(&f).await?;
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations WHERE version=45")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(pending, 0);
        let unchanged: i64 = sqlx::query_scalar("SELECT count(*) FROM public.review_suggestions WHERE note IN ('accepted-target','open-target') AND before_lines='old merge base'")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(unchanged, 4);
        let edited: i64 = sqlx::query_scalar("SELECT count(*) FROM public.review_suggestions WHERE note='accepted-target' AND after_lines='proposal' AND applied_content='owner edits'")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(edited, 2);
        let columns: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='review_suggestions' AND column_name IN ('target_path','target_base','applied_content')")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(columns, 3);
        let constraint: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE conrelid='public.review_suggestions'::regclass AND conname='review_document_target_shape'")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(constraint, 1);
        assert_restored(&f).await?;
        std::fs::write(catalog.path().join(FILE_045), ORIGINAL_045)?;
        g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
        assert_migrated_rows(&f).await
    }.await;
    f.cleanup().await;
    result.unwrap();
}

#[tokio::test]
async fn owner_cleanup_refuses_inheritance_and_unprotected_read_sources() {
    let f = Fixture::new(44).await;
    let result: TestResult = async {
        f.seed_review_rows().await?;
        let catalog = catalog_through(45);
        // SELECT permission without direct ownership is insufficient for
        // the preflight's owner exemption.
        sqlx::raw_sql("ALTER TABLE public.review_suggestions OWNER TO CURRENT_USER")
            .execute(&f.db.pool)
            .await?;
        sqlx::raw_sql(&format!(
            "GRANT SELECT ON public.review_suggestions TO {}",
            f.role
        ))
        .execute(&f.db.pool)
        .await?;
        let not_owner = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
        assert!(
            matches!(
                &not_owner,
                Err(g5_api::db::DbError::MigrationRlsVisibility(message))
                    if message.contains("migration 45 (")
                        && message.contains("public.review_suggestions")
            ),
            "{not_owner:?}"
        );
        sqlx::raw_sql(&format!(
            "ALTER TABLE public.review_suggestions OWNER TO {}",
            f.role
        ))
        .execute(&f.db.pool)
        .await?;
        sqlx::raw_sql("CREATE TABLE public.review_child () INHERITS(public.review_suggestions)")
            .execute(&f.db.pool).await?;
        let inherited = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
        assert!(matches!(inherited, Err(g5_api::db::DbError::MigrationRlsVisibility(_))));
        sqlx::raw_sql("DROP TABLE public.review_child").execute(&f.db.pool).await?;
        sqlx::raw_sql("CREATE TABLE public.review_partitioned (LIKE public.review_suggestions INCLUDING CONSTRAINTS) PARTITION BY LIST(org_id);
            CREATE TABLE public.review_partition PARTITION OF public.review_partitioned DEFAULT;
            INSERT INTO public.review_partitioned SELECT * FROM public.review_suggestions;
            ALTER TABLE public.review_partitioned ENABLE ROW LEVEL SECURITY;
            ALTER TABLE public.review_partitioned FORCE ROW LEVEL SECURITY;
            CREATE POLICY tenant_isolation ON public.review_partitioned USING (org_id=ANY(authz.current_org_ids()));")
            .execute(&f.db.pool).await?;
        sqlx::raw_sql(&format!("ALTER TABLE public.review_partitioned OWNER TO {}", f.role))
            .execute(&f.db.pool).await?;
        std::fs::write(catalog.path().join(FILE_045), ORIGINAL_045.replace("public.review_suggestions", "public.review_partitioned"))?;
        let partitioned = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
        assert!(matches!(partitioned, Err(g5_api::db::DbError::MigrationRlsVisibility(ref message)) if message.contains("public.review_partitioned")));
        sqlx::raw_sql("DROP TABLE public.review_partitioned CASCADE").execute(&f.db.pool).await?;
        std::fs::write(catalog.path().join(FILE_045), ORIGINAL_045.replace(
            "WHERE applied_content IS NOT NULL OR target_path IS NOT NULL;",
            "FROM public.workspaces WHERE (applied_content IS NOT NULL OR target_path IS NOT NULL);"
        ))?;
        let source = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
        assert!(matches!(source, Err(g5_api::db::DbError::MigrationRlsVisibility(ref message)) if message.contains("public.workspaces")));
        std::fs::write(catalog.path().join(FILE_045), format!("-- no-transaction\n{ORIGINAL_045}"))?;
        let no_tx = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
        assert!(matches!(no_tx, Err(g5_api::db::DbError::MigrationRlsVisibility(_))));
        assert_restored(&f).await
    }.await;
    f.cleanup().await;
    result.unwrap();
}

#[tokio::test]
async fn fresh_catalog_and_embedded_replay_run_as_restricted_migrator() {
    let f = Fixture::new(0).await;
    let result: TestResult = async {
        g5_api::db::run_migrations(&f.migrator).await?;
        let catalog = catalog_through(i64::MAX);
        g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
        assert_restored(&f).await
    }
    .await;
    f.cleanup().await;
    result.unwrap();
}

// These tests exercise the tenant RLS policy during historical migrations,
// before membership activity existed. Use the stable session writer and the
// catalog's original set_ctx function; current user admission and ScopedDb
// intentionally require the current schema. The caller selects this exact
// user/org pair from the fixture's membership table. Read-only transactions
// roll back below, removing their session context as well.
async fn historical_tenant_scope(
    service: &InProcessAuthz,
    tenant: &PgPool,
    org: Uuid,
    user: Uuid,
) -> Result<Transaction<'static, Postgres>, Box<dyn std::error::Error + Send + Sync>> {
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

#[tokio::test]
async fn runtime_tenant_cannot_observe_the_owner_cleanup_window() {
    let f = Fixture::new(44).await;
    let result: TestResult = async {
        f.seed_review_rows().await?;
        let system = PgPoolOptions::new().connect(&f.db.system_url).await?;
        let authz = PgPoolOptions::new().connect(&f.db.authz_url).await?;
        let tenant = PgPoolOptions::new().connect(&f.db.tenant_url).await?;
        let service = InProcessAuthz::new(system.clone(), authz.clone());
        let (org, user): (Uuid, Uuid) = sqlx::query_as("SELECT org_id,user_id FROM public.org_memberships ORDER BY org_id LIMIT 1")
            .fetch_one(&f.db.pool).await?;
        let mut before = historical_tenant_scope(&service, &tenant, org, user).await?;
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM public.review_suggestions")
            .fetch_one(&mut *before).await?;
        assert_eq!(visible, 4, "runtime role sees only its org's review rows");
        before.rollback().await?;
        // Establish tenant context before taking the barrier so setup errors
        // cannot leave a spawned migration waiting during fixture teardown.
        let mut scope = historical_tenant_scope(&service, &tenant, org, user).await?;
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *scope).await?;
        // A test-only trigger pauses the original, unchanged 045 UPDATE
        // after NO FORCE has acquired its real table lock. Its only effect is
        // this barrier; no production parser or runtime policy is replaced.
        let mut blocker = owner_update_gate(&f).await?;
        let catalog = catalog_through(45);
        let pool = f.migrator.clone();
        let path = catalog.path().to_path_buf();
        let migration = tokio::spawn(async move { g5_api::db::run_migrations_from_path(&pool, path).await });
        let paused = wait_for_owner_update(&f).await;
        let reader = tokio::spawn(async move {
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.review_suggestions")
                .fetch_one(&mut *scope).await?;
            scope.rollback().await?;
            Ok::<_,Box<dyn std::error::Error + Send + Sync>>(count)
        });
        let blocked: TestResult = async {
            paused?;
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='public.review_suggestions'::regclass AND mode='AccessShareLock' AND NOT granted)")
                        .bind(pid).fetch_one(&f.db.pool).await?;
                    if waiting { return Ok::<_,sqlx::Error>(()); }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await??;
            if reader.is_finished() { return Err("tenant reader escaped the owner cleanup table lock".into()); }
            Ok(())
        }.await;
        // Release the barrier even if a lock-observation probe failed.
        sqlx::query("SELECT pg_advisory_unlock(45045)").execute(&mut blocker).await?;
        drop(blocker);
        tokio::time::timeout(Duration::from_secs(20), migration).await???;
        let count = tokio::time::timeout(Duration::from_secs(20), reader).await???;
        blocked?;
        assert_eq!(count, 4, "runtime role never sees the other org after cleanup");
        assert_migrated_rows(&f).await?;
        system.close().await;
        authz.close().await;
        tenant.close().await;
        Ok(())
    }.await;
    f.cleanup().await;
    result.unwrap();
}

#[tokio::test]
async fn populated_original_045_preserves_two_orgs_with_the_restricted_runner() {
    let f = Fixture::new(44).await;
    let result: TestResult = async {
        f.seed_review_rows().await?;
        let catalog = catalog_through(45);
        let (first, concurrent) = tokio::join!(
            g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()),
            g5_api::db::run_migrations_from_path(&f.migrator, catalog.path())
        );
        first?;
        concurrent?;
        assert_migrated_rows(&f).await?;
        // A checksum error also occurs after SQLx acquires its session lock.
        // Change only this disposable applied catalog copy, then replay the
        // exact original catalog after confirming no lock escaped.
        std::fs::write(
            catalog.path().join(FILE_045),
            format!("{ORIGINAL_045}\n-- test-only checksum mismatch\n"),
        )?;
        let mismatch = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
        assert!(
            matches!(
                &mismatch,
                Err(g5_api::db::DbError::Migration(
                    sqlx::migrate::MigrateError::VersionMismatch(45)
                ))
            ),
            "{mismatch:?}"
        );
        wait_for_no_session_locks(&f).await?;
        std::fs::write(catalog.path().join(FILE_045), ORIGINAL_045)?;
        g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
        assert_migrated_rows(&f).await
    }
    .await;
    f.cleanup().await;
    result.unwrap();
}

#[tokio::test]
async fn cancelled_owner_cleanup_releases_sessions_rolls_back_and_retries() {
    let f = Fixture::new(44).await;
    let result: TestResult = async {
        f.seed_review_rows().await?;
        let mut blocker = owner_update_gate(&f).await?;
        let catalog = catalog_through(45);
        let pool = f.migrator.clone();
        let path = catalog.path().to_path_buf();
        let migration = tokio::spawn(async move { g5_api::db::run_migrations_from_path(&pool, path).await });
        let paused = wait_for_owner_update(&f).await;
        migration.abort();
        let cancelled = migration.await;
        // PostgreSQL may notice a closed client only when its blocked
        // statement resumes; release the test barrier before probing cleanup.
        sqlx::query("SELECT pg_advisory_unlock(45045)").execute(&mut blocker).await?;
        drop(blocker);
        paused?;
        assert!(cancelled.is_err_and(|error| error.is_cancelled()));
        wait_for_no_session_locks(&f).await?;
        assert_restored(&f).await?;
        let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations WHERE version=45")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(applied, 0);
        let unchanged: i64 = sqlx::query_scalar("SELECT count(*) FROM public.review_suggestions WHERE note IN ('accepted-target','open-target') AND before_lines='old merge base'")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(unchanged, 4);
        g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
        assert_migrated_rows(&f).await
    }.await;
    f.cleanup().await;
    result.unwrap();
}

async fn seed_host_rows(f: &Fixture, inventory_exists: bool) -> TestResult {
    for _ in 0..2 {
        let org = Uuid::new_v4();
        let user = Uuid::new_v4();
        sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Host upgrade',$2)")
            .bind(org)
            .bind(format!("host-{org}"))
            .execute(&f.db.pool)
            .await?;
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Host owner')")
            .bind(user)
            .bind(format!("{user}@g5.test"))
            .execute(&f.db.pool)
            .await?;
        sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
            .bind(org)
            .bind(user)
            .execute(&f.db.pool)
            .await?;
        for status in ["ready", "terminated", "provisioning"] {
            sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat) VALUES($1,$2,$3,$3,CASE WHEN $3='provisioning' THEN NULL ELSE now() END)")
                .bind(Uuid::new_v4()).bind(org).bind(status).execute(&f.db.pool).await?;
        }
    }
    if inventory_exists {
        sqlx::query("UPDATE worker_hosts SET reported_live_vm_ids=$1")
            .bind(vec![Uuid::nil()])
            .execute(&f.db.pool)
            .await?;
    }
    Ok(())
}

async fn assert_host_security(f: &Fixture) -> TestResult {
    let flags: (bool, bool) = sqlx::query_as("SELECT relrowsecurity,relforcerowsecurity FROM pg_class WHERE oid='public.worker_hosts'::regclass")
        .fetch_one(&f.db.pool).await?;
    assert_eq!(flags, (true, true));
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts")
        .fetch_one(&f.migrator)
        .await?;
    assert_eq!(visible, 0);
    let policies: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_policies WHERE schemaname='public' AND tablename='worker_hosts' AND policyname='tenant_isolation'")
        .fetch_one(&f.db.pool).await?;
    assert_eq!(policies, 1);
    assert_no_session_locks(f).await
}

async fn assert_host_rows(f: &Fixture, version: i64, had_inventory: bool) -> TestResult {
    let rows: Vec<(String, bool, Option<Vec<Uuid>>)> = sqlx::query_as(
        "SELECT status,slot_size_reported,reported_live_vm_ids FROM worker_hosts ORDER BY host_id",
    )
    .fetch_all(&f.db.pool)
    .await?;
    assert_eq!(rows.len(), 6);
    for (status, slot_size_reported, inventory) in rows {
        assert!(
            !slot_size_reported,
            "historical inventory cannot qualify host slots"
        );
        let expected = if status == "ready" {
            None
        } else if had_inventory {
            Some(vec![Uuid::nil()])
        } else {
            Some(vec![])
        };
        assert_eq!(inventory, expected, "{status} in migration {version}");
    }
    for (v, _, source) in HOST_MIGRATIONS.iter().filter(|(v, _, _)| *v <= version) {
        let checksum: Vec<u8> = sqlx::query_scalar(
            "SELECT checksum FROM _sqlx_migrations WHERE version=$1 AND success",
        )
        .bind(v)
        .fetch_one(&f.db.pool)
        .await?;
        assert_eq!(checksum, Sha384::digest(source.as_bytes()).to_vec());
    }
    assert_host_security(f).await
}

async fn host_update_gate(
    f: &Fixture,
    version: i64,
) -> Result<PgConnection, Box<dyn std::error::Error + Send + Sync>> {
    sqlx::raw_sql(&format!("CREATE FUNCTION public.pause_host_update() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock({version}); RETURN NEW; END $$;
        CREATE TRIGGER pause_host BEFORE UPDATE ON public.worker_hosts FOR EACH ROW EXECUTE FUNCTION public.pause_host_update();"))
        .execute(&f.db.pool).await?;
    let mut blocker = f.db.pool.acquire().await?.detach();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(version)
        .execute(&mut blocker)
        .await?;
    Ok(blocker)
}

async fn wait_for_host_update(f: &Fixture, version: i64) -> TestResult {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_stat_activity a USING(pid) WHERE l.locktype='advisory' AND l.classid=0 AND l.objid=$1::bigint::oid AND NOT l.granted AND a.usename=$2)")
                .bind(version).bind(&f.role).fetch_one(&f.db.pool).await?;
            if waiting { return Ok::<_,sqlx::Error>(()); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
    Ok(())
}

#[tokio::test]
async fn historical_host_context_rolls_back_original_backfill_and_ledger_then_retries() {
    for (version, _, _) in HOST_MIGRATIONS {
        let f = Fixture::new(version - 1).await;
        let result: TestResult = async {
            seed_host_rows(&f, version == 48).await?;
            let catalog = catalog_through(version);
            // Inject failure after the unchanged original backfill, at SQLx's
            // real ledger boundary. No checksum/payload substitution is used.
            sqlx::raw_sql(&format!("CREATE FUNCTION public.fail_host_ledger() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'host ledger regression failure'; END $$;
                CREATE TRIGGER fail_host_ledger BEFORE INSERT ON public._sqlx_migrations FOR EACH ROW WHEN (NEW.version={version}) EXECUTE FUNCTION public.fail_host_ledger();"))
                .execute(&f.db.pool).await?;
            let failed = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
            assert!(matches!(&failed, Err(g5_api::db::DbError::Migration(error)) if error.to_string().contains("host ledger regression failure")), "{failed:?}");
            wait_for_no_session_locks(&f).await?;
            assert_host_security(&f).await?;
            let ledger: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=$1")
                .bind(version).fetch_one(&f.db.pool).await?;
            assert_eq!(ledger, 0);
            if version == 47 {
                let columns: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='worker_hosts' AND column_name='reported_live_vm_ids'")
                    .fetch_one(&f.db.pool).await?;
                assert_eq!(columns, 0);
            } else {
                let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts WHERE reported_live_vm_ids=$1")
                    .bind(vec![Uuid::nil()]).fetch_one(&f.db.pool).await?;
                assert_eq!(retained, 6);
                let triggers: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_trigger WHERE tgrelid='worker_hosts'::regclass AND tgname='worker_hosts_liveness_invalidates_inventory'")
                    .fetch_one(&f.db.pool).await?;
                assert_eq!(triggers, 0);
            }
            sqlx::query("DROP TRIGGER fail_host_ledger ON _sqlx_migrations").execute(&f.db.pool).await?;
            g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
            assert_host_rows(&f, version, version == 48).await?;
            if version == 48 {
                let triggers: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_trigger WHERE tgrelid='worker_hosts'::regclass AND tgname='worker_hosts_liveness_invalidates_inventory'")
                    .fetch_one(&f.db.pool).await?;
                assert_eq!(triggers, 1);
                let (id, known): (Uuid,Option<Vec<Uuid>>) = sqlx::query_as("SELECT host_id,reported_live_vm_ids FROM worker_hosts WHERE status='terminated' ORDER BY host_id LIMIT 1")
                    .fetch_one(&f.db.pool).await?;
                assert_eq!(known, Some(vec![Uuid::nil()]));
                // An old writer updates liveness without supplying inventory.
                // Exercise the original installed trigger, not a test double.
                sqlx::query("UPDATE worker_hosts SET last_heartbeat=now() WHERE host_id=$1")
                    .bind(id).execute(&f.db.pool).await?;
                let invalidated: Option<Vec<Uuid>> = sqlx::query_scalar("SELECT reported_live_vm_ids FROM worker_hosts WHERE host_id=$1")
                    .bind(id).fetch_one(&f.db.pool).await?;
                assert_eq!(invalidated,None,"legacy liveness must invalidate stale inventory");
                // Restore this disposable fixture's inventory for the no-op
                // replay assertion; this is not a claim about API hello.
                sqlx::query("UPDATE worker_hosts SET reported_live_vm_ids=$1 WHERE host_id=$2")
                    .bind(known).bind(id).execute(&f.db.pool).await?;
            }
            g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
            assert_host_rows(&f, version, version == 48).await
        }.await;
        f.cleanup().await;
        result.unwrap();
    }
}

#[tokio::test]
async fn cancelled_historical_host_context_rolls_back_and_retries() {
    for (version, _, _) in HOST_MIGRATIONS {
        let f = Fixture::new(version - 1).await;
        let result: TestResult = async {
            seed_host_rows(&f, version == 48).await?;
            let mut blocker = host_update_gate(&f, version).await?;
            let catalog = catalog_through(version);
            let pool = f.migrator.clone();
            let path = catalog.path().to_path_buf();
            let migration =
                tokio::spawn(
                    async move { g5_api::db::run_migrations_from_path(&pool, path).await },
                );
            let paused = wait_for_host_update(&f, version).await;
            migration.abort();
            let cancelled = migration.await;
            sqlx::query("SELECT pg_advisory_unlock($1)")
                .bind(version)
                .execute(&mut blocker)
                .await?;
            drop(blocker);
            paused?;
            assert!(cancelled.is_err_and(|error| error.is_cancelled()));
            wait_for_no_session_locks(&f).await?;
            assert_host_security(&f).await?;
            let ledger: i64 =
                sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=$1")
                    .bind(version)
                    .fetch_one(&f.db.pool)
                    .await?;
            assert_eq!(ledger, 0);
            g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
            assert_host_rows(&f, version, version == 48).await
        }
        .await;
        f.cleanup().await;
        result.unwrap();
    }
}

#[tokio::test]
async fn historical_host_context_refuses_changed_payload_and_unqualified_owner() {
    for (version, file, source) in HOST_MIGRATIONS {
        let f = Fixture::new(version - 1).await;
        let result: TestResult = async {
            seed_host_rows(&f, version == 48).await?;
            let catalog = catalog_through(version);
            for sql in [format!("{source}\n-- different checksum\n"), format!("-- no-transaction\n{source}"), source.replace("public.worker_hosts", "worker_hosts")] {
                std::fs::write(catalog.path().join(file), sql)?;
                let refused = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
                assert!(matches!(&refused, Err(g5_api::db::DbError::MigrationRlsVisibility(message)) if message.contains(&format!("migration {version} (")) && message.contains("public.worker_hosts")), "{refused:?}");
            }
            std::fs::write(catalog.path().join(file), source)?;
            sqlx::raw_sql("ALTER TABLE worker_hosts OWNER TO CURRENT_USER").execute(&f.db.pool).await?;
            sqlx::raw_sql(&format!("GRANT SELECT ON worker_hosts TO {}", f.role)).execute(&f.db.pool).await?;
            let refused = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
            assert!(matches!(refused, Err(g5_api::db::DbError::MigrationRlsVisibility(_))), "{refused:?}");
            sqlx::raw_sql(&format!("ALTER TABLE worker_hosts OWNER TO {}", f.role)).execute(&f.db.pool).await?;
            sqlx::query("CREATE TABLE host_child () INHERITS(worker_hosts)").execute(&f.db.pool).await?;
            let refused = g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await;
            assert!(matches!(refused, Err(g5_api::db::DbError::MigrationRlsVisibility(_))), "{refused:?}");
            sqlx::query("DROP TABLE host_child").execute(&f.db.pool).await?;
            // Preflight sees rows when FORCE is already absent, so only the
            // execution connection's qualification can refuse this state.
            sqlx::query("ALTER TABLE worker_hosts NO FORCE ROW LEVEL SECURITY").execute(&f.db.pool).await?;
            let refused = g5_api::db::run_migrations_from_path(&f.migrator,catalog.path()).await;
            assert!(matches!(&refused, Err(g5_api::db::DbError::Migration(sqlx::migrate::MigrateError::ExecuteMigration(error,v))) if *v == version && error.to_string().contains("direct ownership")), "{refused:?}");
            wait_for_no_session_locks(&f).await?;
            sqlx::query("ALTER TABLE worker_hosts FORCE ROW LEVEL SECURITY").execute(&f.db.pool).await?;
            g5_api::db::run_migrations_from_path(&f.migrator,catalog.path()).await?;
            assert_host_rows(&f,version,version == 48).await?;
            // The adapter retains SQLx's original dirty and checksum checks.
            sqlx::query("UPDATE _sqlx_migrations SET success=false WHERE version=$1").bind(version).execute(&f.db.pool).await?;
            let dirty = g5_api::db::run_migrations_from_path(&f.migrator,catalog.path()).await;
            assert!(matches!(dirty, Err(g5_api::db::DbError::Migration(sqlx::migrate::MigrateError::Dirty(v))) if v == version), "{dirty:?}");
            wait_for_no_session_locks(&f).await?;
            sqlx::query("UPDATE _sqlx_migrations SET success=true WHERE version=$1").bind(version).execute(&f.db.pool).await?;
            std::fs::write(catalog.path().join(file),format!("{source}\n-- changed applied checksum\n"))?;
            let mismatch = g5_api::db::run_migrations_from_path(&f.migrator,catalog.path()).await;
            assert!(matches!(mismatch, Err(g5_api::db::DbError::Migration(sqlx::migrate::MigrateError::VersionMismatch(v))) if v == version), "{mismatch:?}");
            wait_for_no_session_locks(&f).await?;
            assert_host_security(&f).await
        }.await;
        f.cleanup().await;
        result.unwrap();
    }
}

#[tokio::test]
async fn runtime_tenant_cannot_observe_historical_host_owner_contexts() {
    for (version, _, _) in HOST_MIGRATIONS {
        let f = Fixture::new(version - 1).await;
        let result: TestResult = async {
            seed_host_rows(&f, version == 48).await?;
            let system = PgPoolOptions::new().connect(&f.db.system_url).await?;
            let authz = PgPoolOptions::new().connect(&f.db.authz_url).await?;
            let tenant = PgPoolOptions::new().connect(&f.db.tenant_url).await?;
            let service = InProcessAuthz::new(system.clone(), authz.clone());
            let (org,user): (Uuid,Uuid) = sqlx::query_as("SELECT org_id,user_id FROM org_memberships ORDER BY org_id LIMIT 1")
                .fetch_one(&f.db.pool).await?;
            let mut before = historical_tenant_scope(&service, &tenant, org, user).await?;
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts").fetch_one(&mut *before).await?;
            assert_eq!(count,3);
            before.rollback().await?;
            let mut scope = historical_tenant_scope(&service, &tenant, org, user).await?;
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *scope).await?;
            let mut blocker = host_update_gate(&f,version).await?;
            let catalog = catalog_through(version);
            let pool = f.migrator.clone();
            let path = catalog.path().to_path_buf();
            let migration = tokio::spawn(async move { g5_api::db::run_migrations_from_path(&pool,path).await });
            let paused = wait_for_host_update(&f,version).await;
            let reader = tokio::spawn(async move {
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_hosts").fetch_one(&mut *scope).await?;
                scope.rollback().await?;
                Ok::<_,Box<dyn std::error::Error + Send + Sync>>(count)
            });
            let blocked: TestResult = async {
                paused?;
                tokio::time::timeout(Duration::from_secs(10),async {
                    loop {
                        let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='public.worker_hosts'::regclass AND mode='AccessShareLock' AND NOT granted)")
                            .bind(pid).fetch_one(&f.db.pool).await?;
                        if waiting { return Ok::<_,sqlx::Error>(()); }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }).await??;
                if reader.is_finished() { return Err("tenant reader escaped host owner-context lock".into()); }
                Ok(())
            }.await;
            sqlx::query("SELECT pg_advisory_unlock($1)").bind(version).execute(&mut blocker).await?;
            drop(blocker);
            tokio::time::timeout(Duration::from_secs(20),migration).await???;
            let count = tokio::time::timeout(Duration::from_secs(20),reader).await???;
            blocked?;
            assert_eq!(count,3,"tenant must not see the other org during migration {version}");
            assert_host_rows(&f,version,version == 48).await?;
            system.close().await;
            authz.close().await;
            tenant.close().await;
            Ok(())
        }.await;
        f.cleanup().await;
        result.unwrap();
    }
}

/// The same restricted deployment role must be able to replace both definer
/// boundaries without retaining temporary SET/CREATE authority after 099.
#[tokio::test]
async fn membership_activity_upgrade_retains_owners_and_restores_definer_privileges() {
    let f = Fixture::new(98).await;
    let result:TestResult=async {
        sqlx::raw_sql(&format!(
            "GRANT authz_ctx_writer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;",
            role=f.role)).execute(&f.db.pool).await?;
        let memberships_before:serde_json::Value=sqlx::query_scalar(
            "SELECT jsonb_agg(to_jsonb(m) ORDER BY roleid,grantor) FROM pg_auth_members m WHERE member=(SELECT oid FROM pg_roles WHERE rolname=$1)")
            .bind(&f.role).fetch_one(&f.db.pool).await?;
        let mut ids=Vec::new();
        for _ in 0..2 {
            let org=Uuid::new_v4();let user=Uuid::new_v4();let workspace=Uuid::new_v4();
            sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Before deactivation',$2)")
                .bind(org).bind(org.to_string()).execute(&f.db.pool).await?;
            sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Retained member')")
                .bind(user).bind(format!("{user}@g5.test")).execute(&f.db.pool).await?;
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
                .bind(org).bind(user).execute(&f.db.pool).await?;
            sqlx::query("INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) VALUES($1,$2,'retained-workspace',false,$3,$3)")
                .bind(workspace).bind(org).bind(user).execute(&f.db.pool).await?;
            ids.push((org,user,workspace));
        }
        let workspaces_before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(w) ORDER BY id) FROM workspaces w")
            .fetch_one(&f.db.pool).await?;
        let policies_before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(p) - 'polqual' - 'polwithcheck' ORDER BY oid) FROM pg_policy p")
            .fetch_one(&f.db.pool).await?;
        // Compare policy identities across membership migration 099 only;
        // later review migrations intentionally add privacy policies and tables.
        let catalog = catalog_through(99);
        g5_api::db::run_migrations_from_path(&f.migrator,catalog.path()).await?;
        g5_api::db::run_migrations_from_path(&f.migrator,catalog.path()).await?;
        let memberships_after:serde_json::Value=sqlx::query_scalar(
            "SELECT jsonb_agg(to_jsonb(m) ORDER BY roleid,grantor) FROM pg_auth_members m WHERE member=(SELECT oid FROM pg_roles WHERE rolname=$1)")
            .bind(&f.role).fetch_one(&f.db.pool).await?;
        assert_eq!(memberships_before,memberships_after);
        let workspaces_after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(w) - 'member_cleanup_retry_id' - 'member_cleanup_retry_at' ORDER BY id) FROM workspaces w")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(workspaces_before,workspaces_after,"ownership and history stay byte-for-byte intact");
        let unscheduled: bool = sqlx::query_scalar("SELECT bool_and(member_cleanup_retry_id IS NULL AND member_cleanup_retry_at IS NULL) FROM workspaces")
            .fetch_one(&f.db.pool).await?;
        assert!(unscheduled,"new retry metadata does not manufacture cleanup work");
        let policies_after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(p) - 'polqual' - 'polwithcheck' ORDER BY oid) FROM pg_policy p")
            .fetch_one(&f.db.pool).await?;
        assert_eq!(policies_before,policies_after,"RLS policy identity, roles, commands and permissiveness are retained");
        for (org,user,workspace) in ids {
            let active:(bool,i64,bool)=sqlx::query_as("SELECT deactivated_at IS NULL,activity_revision,deactivation_cleanup_id IS NULL FROM org_memberships WHERE org_id=$1 AND user_id=$2")
                .bind(org).bind(user).fetch_one(&f.db.pool).await?;
            assert_eq!(active,(true,0,true));
            sqlx::query("UPDATE org_memberships SET deactivated_at=now(),activity_revision=1,deactivation_cleanup_id=$3 WHERE org_id=$1 AND user_id=$2")
                .bind(org).bind(user).bind(Uuid::new_v4()).execute(&f.db.pool).await?;
            assert_eq!(sqlx::query_scalar::<_,Uuid>("SELECT owner_user_id FROM workspaces WHERE id=$1")
                .bind(workspace).fetch_one(&f.db.pool).await?,user);
        }
        let visible:i64=sqlx::query_scalar("SELECT count(*) FROM workspaces").fetch_one(&f.migrator).await?;
        assert_eq!(visible,0,"migration leaves FORCE RLS intact");
        let privileged:bool=sqlx::query_scalar("SELECT has_function_privilege('g5_tenant','authz.lock_workspace_member_activity(uuid)','EXECUTE')")
            .fetch_one(&f.db.pool).await?;
        assert!(!privileged,"cross-tenant runtime lookup is not tenant callable");
        Ok(())
    }.await;
    f.cleanup().await;
    result.unwrap();
}
