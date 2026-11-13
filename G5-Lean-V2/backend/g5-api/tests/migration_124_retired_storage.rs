//! Issue 903: populated contract migrations through the production executable.
//! Keep historical values here, not in fixtures against the latest schema.
use crate::helpers::TestDb;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{path::Path, process::Output};
use uuid::Uuid;

const GROUPS: &[(&str, &[&str])] = &[
    (
        "workspace_vms",
        &["retained_root_host_id", "retained_root_cache_generation"],
    ),
    (
        "worker_hosts",
        &["retained_roots", "libkrun_version", "libkrunfw_version"],
    ),
    ("workspace_settings", &["retain_hours"]),
    (
        "workspaces",
        &[
            "base_auto_pull_enabled",
            "base_auto_pull_actor_user_id",
            "remote_tracking_requires_resume",
        ],
    ),
    (
        "tasks",
        &[
            "max_retries",
            "pruned",
            "is_checkpoint",
            "agent_execution_receipt",
            "active_execution_attempt_id",
        ],
    ),
    ("chat_sessions", &["linked_nodes"]),
    ("review_suggestions", &["blob_id"]),
];

async fn run(url: &str, catalog: &Path) -> Output {
    let binary = std::env::var_os("NEXTEST_BIN_EXE_g5_db_migrate")
        .or_else(|| std::env::var_os("NEXTEST_BIN_EXE_g5-db-migrate"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_g5-db-migrate").into());
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new(binary)
            .env("DATABASE_URL", url)
            .env("G5_MIGRATION_SOURCE", catalog)
            .env("G5_MIGRATION_DATABASE_MAX_CONNECTIONS", "2")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("migration timeout")
    .expect("launch production migrator")
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn blocked_upgrade(pool: &PgPool, url: &str, catalog: &Path, table: &str, version: i64) {
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query(&format!("LOCK TABLE public.{table} IN ACCESS SHARE MODE"))
        .execute(&mut *blocker)
        .await
        .unwrap();
    // The runner's outer deadline also allows time for the shared cluster-role
    // lock; the PostgreSQL error below proves the DDL's shorter lock timeout.
    let failed = run(url, catalog).await;
    blocker.rollback().await.unwrap();
    assert!(!failed.status.success());
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(
        error.contains("canceling statement due to lock timeout"),
        "{error}"
    );
    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=$1")
        .bind(version)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(applied, 0, "a timed-out migration leaves no ledger entry");
}

async fn survivors(pool: &PgPool) -> Vec<Vec<Value>> {
    let mut result = Vec::new();
    for (table, removed) in GROUPS {
        let mut rows: Vec<Value> = sqlx::query_scalar(&format!(
            "SELECT to_jsonb(t) - $1::text[] FROM public.{table} t"
        ))
        .bind(*removed)
        .fetch_all(pool)
        .await
        .unwrap();
        rows.sort_by_key(Value::to_string);
        result.push(rows);
    }
    result
}

async fn security(pool: &PgPool) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('table', c.relname, 'enabled', c.relrowsecurity,
                   'forced', c.relforcerowsecurity, 'acl', c.relacl::text,
                   'policies', (SELECT jsonb_agg(to_jsonb(p) ORDER BY p.policyname)
                                FROM pg_policies p WHERE p.schemaname='public' AND p.tablename=c.relname))
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='public' AND c.relname=ANY($1) ORDER BY c.relname"
    ).bind(GROUPS.iter().map(|(table, _)| *table).collect::<Vec<_>>())
        .fetch_all(pool).await.unwrap()
}

async fn shared_trigger_functions(pool: &PgPool) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('oid', oid, 'owner', proowner, 'acl', proacl::text,
                   'definition', pg_get_functiondef(oid))
         FROM pg_proc WHERE oid IN ('public.set_approvals_governance_created_at()'::regprocedure,
                                   'public.set_org_id_from_session()'::regprocedure)
         ORDER BY proname",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn seed(db: &TestDb) -> (Uuid, Uuid) {
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    let workspace = Uuid::new_v4();
    let host = Uuid::new_v4();
    let repo = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES ($1,$2,'Owner')")
        .bind(user)
        .bind(format!("{user}@example.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES ($1,$2,$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES ($1,$2,'owner')")
        .bind(org)
        .bind(user)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by,base_auto_pull_enabled,base_auto_pull_actor_user_id,remote_tracking_requires_resume) VALUES ($1,$2,'audit',$3,$3,true,$3,true)")
        .bind(workspace).bind(org).bind(user).execute(&db.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO workspace_settings(workspace_id,org_id,retain_hours) VALUES ($1,$2,72)",
    )
    .bind(workspace)
    .bind(org)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,retained_roots,libkrun_version,libkrunfw_version,identity_hash,identity_changed_at) VALUES ($1,$2,'test',$3,'legacy-krun','legacy-fw',$4,now())")
        .bind(host).bind(org).bind(json!([{"workspace_id":workspace,"cache_generation":7,"expires_at":"2030-01-01T00:00:00Z"}]))
        .bind("a".repeat(64)).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(workspace_id,org_id,status,retained_root_host_id,retained_root_cache_generation) VALUES ($1,$2,'stopped',$3,7)")
        .bind(workspace).bind(org).bind(host).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO repos(id,org_id,name,slug,git_url,default_branch) VALUES ($1,$2,'audit',$3,'https://example.test/repo.git','main')")
        .bind(repo).bind(org).bind(repo.to_string()).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO repo_secrets(repo_id,org_id,webhook_secret_enc) VALUES ($1,$2,$3)")
        .bind(repo)
        .bind(org)
        .bind(b"historical secret".as_slice())
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES ($1,$2,$3,'','code')")
        .bind(org).bind(workspace).bind(repo).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled,sync_paused_reason) VALUES ($1,$2,$3,'main',true,'owner_transfer')")
        .bind(org).bind(workspace).bind(repo).execute(&db.pool).await.unwrap();
    let merge_run = Uuid::new_v4();
    sqlx::query("INSERT INTO workspace_merge_runs(id,org_id,workspace_id,source_workspace_id,target_workspace_id,source_branch_name,status) VALUES ($1,$2,$3,$3,$3,'legacy/retired-evaluation','completed')")
        .bind(merge_run).bind(org).bind(workspace).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO merge_evaluations(org_id,merge_run_id,source_workspace_id,target_workspace_id,outcome,schema_version,evaluation,evaluated_by) VALUES ($1,$2,$3,$3,'failed',3,'{\"historical\":true}',$4)")
        .bind(org).bind(merge_run).bind(workspace).bind(user).execute(&db.pool).await.unwrap();
    let task = Uuid::new_v4();
    let prerequisite = Uuid::new_v4();
    for id in [task, prerequisite] {
        sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,spec,attempt,created_by,max_retries,pruned,is_checkpoint,agent_execution_receipt,active_execution_attempt_id) VALUES ($1,$2,$3,'Audit task','command','queued','{}',1,$4,9,true,true,'{\"provider\":\"codex\"}',$5)")
            .bind(id).bind(org).bind(workspace).bind(user).bind(Uuid::new_v4())
            .execute(&db.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO task_attempts(org_id,task_id,attempt) VALUES ($1,$2,1)")
        .bind(org)
        .bind(task)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO chat_sessions(workspace_id,org_id,scope_type,title,linked_nodes,created_by) VALUES ($1,$2,'workspace','Historical session',ARRAY['old-node'],$3)")
        .bind(workspace).bind(org).bind(user).execute(&db.pool).await.unwrap();
    let publication: Uuid = sqlx::query_scalar("INSERT INTO review_publications(org_id,source_workspace_id,reviewer_user_id,author_kind,author_label,round) VALUES ($1,$2,$3,'human','Owner',1) RETURNING id")
        .bind(org).bind(workspace).bind(user).fetch_one(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO review_suggestions(org_id,publication_id,kind,author_kind,author_label,blob_id,blob_oid,blob_sha256,bytes,content_type,repo_id,path) VALUES ($1,$2,'artifact','human','Owner',$3,$4,$5,7,'text/plain',$6,'audit.txt')")
        .bind(org).bind(publication).bind(Uuid::new_v4()).bind("b".repeat(40)).bind("c".repeat(64)).bind(repo)
        .execute(&db.pool).await.unwrap();
    (task, prerequisite)
}

#[tokio::test]
async fn populated_upgrade_preserves_live_data_security_and_refuses_unknown_dependencies() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap().map(Result::unwrap) {
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "122"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let mut edges = Vec::new();
    for _ in 0..2 {
        edges.push(seed(&db).await);
    }
    let before = survivors(&db.pool).await;
    let merge_runs_before: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM workspace_merge_runs r ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let shared_functions_before = shared_trigger_functions(&db.pool).await;
    let role = format!("retired_storage_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
         GRANT USAGE, CREATE ON SCHEMA public TO {role};
         GRANT USAGE ON SCHEMA authz TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    for table in GROUPS.iter().map(|(table, _)| *table).chain([
        "repo_secrets",
        "merge_evaluations",
        "orgs",
        "session_inputs",
        "refresh_tokens",
        "approval_requests",
        "workspace_pull_request_events",
        "workspace_pull_request_provider_repos",
    ]) {
        sqlx::raw_sql(&format!("ALTER TABLE public.{table} OWNER TO {role}"))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    // Main's intervening merge-override migration replaces this trigger body.
    sqlx::raw_sql(&format!(
        "ALTER FUNCTION public.guard_approval_request_update() OWNER TO {role}"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    let security_before = security(&db.pool).await;
    let url = TestDb::url_for_database(&db.db_name).await;
    let mut url = url::Url::parse(&url).unwrap();
    url.set_username(&role).unwrap();
    url.set_password(Some("migration_test")).unwrap();
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(url.as_str())
        .await
        .unwrap();
    // Catch a panic so the cluster-global role is removed even on an assertion
    // failure, following the other populated-upgrade fixtures.
    use futures_util::FutureExt;
    let result = std::panic::AssertUnwindSafe(async {
        let privileges: (bool, bool) = sqlx::query_as("SELECT rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user")
            .fetch_one(&owner).await.unwrap();
        assert_eq!(privileges, (false, false));
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces").fetch_one(&owner).await.unwrap();
        assert_eq!(visible, 0, "FORCE RLS hides populated tenant rows from the owner");
        for (table, columns) in GROUPS {
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name=$1 AND column_name=ANY($2)")
                .bind(table).bind(*columns).fetch_one(&db.pool).await.unwrap();
            assert_eq!(count, columns.len() as i64, "precondition: {table}");
        }
        // Refuse an unexpected consumer, without CASCADE. This fails after
        // workspace_vms was altered, proving the whole feature group rolls back.
        sqlx::raw_sql("CREATE VIEW public.audit_root_consumer AS SELECT retained_roots FROM public.worker_hosts")
            .execute(&db.pool).await.unwrap();
        // Also cover the original audit's 121 snapshot. Both intervening main
        // migrations must commit before the first cleanup migration can fail.
        for intervening in ["122_platform_operator_org.sql", "123_chat_input_acceptance.sql"] {
            std::fs::copy(source.join(intervening), catalog.path().join(intervening)).unwrap();
        }
        let first = "124_drop_retained_root_columns.sql";
        std::fs::copy(source.join(first), catalog.path().join(first)).unwrap();
        let failed = run(url.as_str(), catalog.path()).await;
        assert!(!failed.status.success());
        let error = String::from_utf8_lossy(&failed.stderr);
        assert!(error.contains("cannot drop column retained_roots of table worker_hosts because other objects depend on it"), "{error}");
        let pair: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_name='workspace_vms' AND column_name IN ('retained_root_host_id','retained_root_cache_generation')")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(pair, 2, "the earlier DDL rolled back");
        assert_eq!(security_before, security(&db.pool).await);
        let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=124")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(applied, 0);
        let intervening: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version IN (122,123) AND success ORDER BY version")
            .fetch_all(&db.pool).await.unwrap();
        assert_eq!(intervening, vec![122, 123], "main's migrations precede the cleanup");
        sqlx::raw_sql("DROP VIEW public.audit_root_consumer").execute(&db.pool).await.unwrap();
        // A reader on the second table must also roll back the first ALTER.
        blocked_upgrade(&db.pool, url.as_str(), catalog.path(), "worker_hosts", 124).await;
        let pair: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_name='workspace_vms' AND column_name IN ('retained_root_host_id','retained_root_cache_generation')")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(pair, 2, "lock timeout rolls back the earlier DDL");
        assert_eq!(security_before, security(&db.pool).await);
        success(run(url.as_str(), catalog.path()).await);
        for entry in std::fs::read_dir(&source).unwrap().map(Result::unwrap) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.path().extension().is_some_and(|ext| ext == "sql") && name.as_ref() >= "125" && name.as_ref() < "132" {
                std::fs::copy(entry.path(), catalog.path().join(name.as_ref())).unwrap();
            }
        }
        blocked_upgrade(&db.pool, url.as_str(), catalog.path(), "tasks", 127).await;
        let historical_tasks = sqlx::query("SELECT max_retries,pruned,is_checkpoint,agent_execution_receipt,active_execution_attempt_id FROM tasks")
            .fetch_all(&db.pool).await.unwrap();
        assert_eq!(historical_tasks.len(), 4, "busy task storage remains intact");
        // Populated upgrades can have a different original helper owner.
        // Do not accept a successful migration that leaves a callable orphan.
        let failed = run(url.as_str(), catalog.path()).await;
        assert!(!failed.status.success());
        let error = String::from_utf8_lossy(&failed.stderr);
        assert!(error.contains("must be owner of function") && error.contains("repo_webhook_secret_for_current_org"), "{error}");
        let secrets: i64 = sqlx::query_scalar("SELECT count(*) FROM repo_secrets").fetch_one(&db.pool).await.unwrap();
        assert_eq!(secrets, 2);
        let evaluations: i64 = sqlx::query_scalar("SELECT count(*) FROM merge_evaluations WHERE evaluation->>'historical'='true'")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(evaluations, 2, "the populated table awaits its own retirement step");
        assert_eq!(security_before, security(&db.pool).await);
        // Explicitly simulate the deployment owner owning both objects. A
        // production ownership repair, if needed, is an operator prerequisite.
        sqlx::raw_sql(&format!("ALTER FUNCTION public.repo_webhook_secret_for_current_org(uuid) OWNER TO {role}"))
            .execute(&db.pool).await.unwrap();
        sqlx::raw_sql("CREATE VIEW public.audit_evaluation_consumer AS SELECT id FROM public.merge_evaluations")
            .execute(&db.pool).await.unwrap();
        let failed = run(url.as_str(), catalog.path()).await;
        assert!(!failed.status.success());
        let error = String::from_utf8_lossy(&failed.stderr);
        assert!(error.contains("merge_evaluations") && error.contains("other objects depend on it"), "{error}");
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=131")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(pending, 0, "an unexpected consumer prevents the table retirement");
        let evaluations: i64 = sqlx::query_scalar("SELECT count(*) FROM merge_evaluations WHERE evaluation->>'historical'='true'")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(evaluations, 2, "refusal preserves both tenants' historical evaluations");
        sqlx::raw_sql("DROP VIEW public.audit_evaluation_consumer").execute(&db.pool).await.unwrap();
        success(run(url.as_str(), catalog.path()).await);
        success(run(url.as_str(), catalog.path()).await);
        for intervening in ["132_session_client_kind.sql", "133_merge_override.sql"] {
            std::fs::copy(source.join(intervening), catalog.path().join(intervening)).unwrap();
        }
        success(run(url.as_str(), catalog.path()).await);
        // Apply the separate artifact contract migration from the catalog at
        // main's schema, with warm statements from both application
        // generations. Only the explicit projection survives the drop.
        use g5_api::services::workspace_review_service::{ReviewSuggestionRecord, REVIEW_SUGGESTION_COLUMNS};
        let mut warm = db.pool.acquire().await.unwrap();
        let explicit = format!("SELECT {REVIEW_SUGGESTION_COLUMNS} FROM review_suggestions ORDER BY id");
        let records: Vec<ReviewSuggestionRecord> = sqlx::query_as(&explicit)
            .fetch_all(&mut *warm).await.unwrap();
        let expected = serde_json::to_value(records).unwrap();
        sqlx::query("SELECT * FROM review_suggestions LIMIT 0").fetch_all(&mut *warm).await.unwrap();
        let artifact_migration = "134_drop_review_artifact_uuid.sql";
        std::fs::copy(source.join(artifact_migration), catalog.path().join(artifact_migration)).unwrap();
        sqlx::raw_sql("CREATE VIEW public.audit_artifact_uuid_consumer AS SELECT blob_id FROM public.review_suggestions")
            .execute(&db.pool).await.unwrap();
        let failed = run(url.as_str(), catalog.path()).await;
        assert!(!failed.status.success());
        let error = String::from_utf8_lossy(&failed.stderr);
        assert!(error.contains("blob_id") && error.contains("other objects depend on it"), "{error}");
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=134")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(pending, 0, "an unexpected UUID consumer prevents the drop");
        sqlx::raw_sql("DROP VIEW public.audit_artifact_uuid_consumer").execute(&db.pool).await.unwrap();
        blocked_upgrade(&db.pool, url.as_str(), catalog.path(), "review_suggestions", 134).await;
        let historical_uuids: i64 = sqlx::query_scalar("SELECT count(*) FROM review_suggestions WHERE blob_id IS NOT NULL")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(historical_uuids, 2, "refused upgrades preserve both tenants' UUIDs");
        assert_eq!(security_before, security(&db.pool).await);
        success(run(url.as_str(), catalog.path()).await);
        success(run(url.as_str(), catalog.path()).await);
        let records: Vec<ReviewSuggestionRecord> = sqlx::query_as(&explicit)
            .fetch_all(&mut *warm).await.unwrap();
        assert_eq!(serde_json::to_value(records).unwrap(), expected);
        let stale = sqlx::query("SELECT * FROM review_suggestions LIMIT 0")
            .fetch_all(&mut *warm).await.unwrap_err();
        assert_eq!(stale.as_database_error().unwrap().code().as_deref(), Some("0A000"),
            "the old prepared star projection prevents a same-release drop");
        drop(warm);
        assert_eq!(before, survivors(&db.pool).await, "every surviving value is preserved");
        assert_eq!(security_before, security(&db.pool).await, "RLS, policies and table ACLs unchanged");
        sqlx::query("SELECT authz.assert_tenant_tables_force_rls()")
            .execute(&owner).await.unwrap();
        for (table, columns) in GROUPS {
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND table_name=$1 AND column_name=ANY($2)")
                .bind(table).bind(*columns).fetch_one(&db.pool).await.unwrap();
            assert_eq!(count, 0, "removed {table} columns");
        }
        let retired: (Option<String>, Option<String>) = sqlx::query_as("SELECT to_regclass('public.repo_secrets')::text,to_regprocedure('public.repo_webhook_secret_for_current_org(uuid)')::text")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(retired, (None, None));
        let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version BETWEEN 124 AND 134 AND success ORDER BY version")
            .fetch_all(&db.pool).await.unwrap();
        assert_eq!(versions, (124..=134).collect::<Vec<_>>());
        let deferred: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND ((table_name='worker_hosts' AND column_name IN ('registration_token_hash','registration_expires_at','registration_redeemed_at','registration_revoked_at','identity_hash','identity_changed_at')) OR (table_name='workspace_flushes' AND column_name='concurrent_writers'))")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(deferred, 7);
        let evaluation_objects: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace='public'::regnamespace AND relname IN ('merge_evaluations','merge_evaluations_seq_seq','merge_evaluations_pkey','merge_evaluations_seq_key','uq_merge_evaluations_run','idx_merge_evaluations_source','idx_merge_evaluations_target')")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(evaluation_objects, 0, "the table and its owned sequence/indexes are retired");
        assert_eq!(shared_functions_before, shared_trigger_functions(&db.pool).await,
            "shared trigger functions, ownership and grants are preserved");
        let shared_users: i64 = sqlx::query_scalar("SELECT count(DISTINCT tgfoid) FROM pg_trigger WHERE NOT tgisinternal AND tgfoid IN ('public.set_approvals_governance_created_at()'::regprocedure,'public.set_org_id_from_session()'::regprocedure)")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(shared_users, 2, "both shared functions still have live trigger consumers");
        let merge_runs_after: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(r) FROM workspace_merge_runs r ORDER BY id")
            .fetch_all(&db.pool).await.unwrap();
        assert_eq!(merge_runs_before, merge_runs_after, "referenced merge runs are preserved");
        let retained: bool = sqlx::query_scalar("SELECT to_regclass('public.worker_tokens') IS NOT NULL")
            .fetch_one(&db.pool).await.unwrap();
        assert!(retained);
        let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM task_attempts")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(attempts, 2);
        let live_remote: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_integrations WHERE auto_pull_enabled AND sync_paused_reason='owner_transfer'")
            .fetch_one(&db.pool).await.unwrap();
        assert_eq!(live_remote, 2);
        // Execute the real seed script's SQL twice against the contracted
        // schema, including its RLS restoration, rather than duplicating it.
        let workspace: Uuid = sqlx::query_scalar("SELECT id FROM workspaces ORDER BY id LIMIT 1")
            .fetch_one(&db.pool).await.unwrap();
        let script = include_str!("../../../scripts/seed-timsystem-workspace.sh");
        let sql = script.split("<<'EOSQL' >/dev/null\n").nth(1).unwrap()
            .split("\nEOSQL").next().unwrap()
            .replace(":'workspace_id'", &format!("'{workspace}'"))
            .replace(":'artifact_path'", "'audit/runtime-artifact.txt'");
        for _ in 0..2 {
            sqlx::raw_sql(&sql).execute(&db.pool).await.unwrap();
        }
        let fixtures: i64 = sqlx::query_scalar("SELECT count(*) FROM chat_sessions s JOIN chat_messages m ON m.session_id=s.id WHERE s.workspace_id=$1 AND s.title='TimSystem runtime artifact fixture' AND m.artifacts->'artifacts'->0->>'path'='audit/runtime-artifact.txt'")
            .bind(workspace).fetch_one(&db.pool).await.unwrap();
        assert_eq!(fixtures, 1, "seed replay replaces its session and artifact");
        sqlx::query("SELECT authz.assert_tenant_tables_force_rls()")
            .execute(&owner).await.unwrap();
        for (task, prerequisite) in edges {
            sqlx::query("UPDATE tasks SET depends_on=$2 WHERE id=$1").bind(task).bind(vec![prerequisite])
                .execute(&db.pool).await.unwrap();
            let dependency: Uuid = sqlx::query_scalar("SELECT depends_on_task_id FROM task_dependencies WHERE task_id=$1")
                .bind(task).fetch_one(&db.pool).await.unwrap();
            assert_eq!(dependency, prerequisite, "the live dependency trigger survives");
        }
    }).catch_unwind().await;
    owner.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
