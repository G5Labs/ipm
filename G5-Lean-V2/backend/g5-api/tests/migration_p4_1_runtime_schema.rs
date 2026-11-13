//! Fresh, upgrade, and no-op coverage for the final Lean execution cutover.
//!
//! The upgrade test applies the frozen baseline, inserts representative legacy
//! rows, and then runs the pre-reset catalog. This exercises the compatibility
//! projection before migration 008 removes the retired stores, then verifies
//! that the repository-model cleanup refuses the populated installation without
//! altering it.

use crate::helpers;

use std::path::{Path, PathBuf};

use helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tempfile::TempDir;
use url::Url;
use uuid::Uuid;

const WORKSPACE_ID: Uuid = Uuid::from_u128(0x40000000000040008000000000000001);
const TASK_ID: Uuid = Uuid::from_u128(0x80000000000040008000000000000001);
const OWNER_ID: Uuid = Uuid::from_u128(0x10000000000040008000000000000001);

/// Include every historical migration before the cleanup, even when a rebase
/// renumbers that undeployed migration. SQLx commits each preceding file, so
/// rejection can preserve the ledger only after this complete prefix is applied.
fn pre_cleanup_catalog(full_migrations: &Path) -> TempDir {
    let files: Vec<PathBuf> = std::fs::read_dir(full_migrations)
        .expect("read migration catalog")
        .map(|entry| entry.expect("read migration entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    let version = |path: &Path| -> u64 {
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split_once('_')
            .expect("migration filename has a version prefix")
            .0
            .parse()
            .expect("migration version is numeric")
    };
    let cleanup = files
        .iter()
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("_workspace_repository_model.sql")
        })
        .expect("repository-model cleanup migration exists");
    let cleanup_version = version(cleanup);
    let catalog = TempDir::new().expect("create pre-cleanup catalog");
    for path in files.iter().filter(|path| version(path) < cleanup_version) {
        std::fs::copy(path, catalog.path().join(path.file_name().unwrap()))
            .expect("copy historical migration unchanged");
    }
    catalog
}

#[tokio::test]
async fn fresh_install_exposes_only_final_execution_authorities() {
    let db = TestDb::create().await;

    let retained: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = ANY($1) ORDER BY table_name",
    )
    .bind(vec![
        "workspace_vms",
        "workspace_flushes",
        "workspace_forks",
        "task_attempts",
        "task_dependencies",
        "task_events",
        "provider_requests",
        "session_inputs",
    ])
    .fetch_all(&db.pool)
    .await
    .expect("inspect final authority tables");
    assert_eq!(retained.len(), 8);

    let retired: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = ANY($1) ORDER BY table_name",
    )
    .bind(vec![
        "workers",
        "task_questions",
        "task_question_windows",
        "task_execution_attempts",
        "chat_sticky_runtimes",
        "runtime_attribution_events",
        "session_build_telemetry",
        "workspace_prebuild_state",
        "user_preferences",
    ])
    .fetch_all(&db.pool)
    .await
    .expect("inspect retired execution tables");
    assert!(
        retired.is_empty(),
        "retired execution tables remain: {retired:?}"
    );

    let host_lookup: (String, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(proowner), prosecdef \
         FROM pg_proc \
         WHERE oid = 'authz.lookup_worker_host_for_auth(uuid)'::regprocedure",
    )
    .fetch_one(&db.pool)
    .await
    .expect("inspect pre-RLS host authentication lookup");
    assert_eq!(host_lookup.0, "authz_lookup_definer");
    assert!(
        host_lookup.1,
        "host authentication lookup must remain SECURITY DEFINER"
    );
    let app_can_execute_lookup: bool = sqlx::query_scalar(
        "SELECT has_function_privilege(\
             'g5_app', 'authz.lookup_worker_host_for_auth(uuid)', 'EXECUTE'\
         )",
    )
    .fetch_one(&db.pool)
    .await
    .expect("inspect host authentication lookup grant");
    assert!(app_can_execute_lookup);

    let deferred_tenant_validators: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT proname::text, pg_get_userbyid(proowner), prosecdef \
         FROM pg_proc JOIN pg_namespace ON pg_namespace.oid = pronamespace \
         WHERE nspname = 'authz' AND proname = ANY($1) ORDER BY proname",
    )
    .bind(vec![
        "g5_validate_flush_repositories",
        "g5_validate_runtime_intent_binding",
        "g5_validate_session_input_binding",
        "g5_validate_workspace_fork_members",
    ])
    .fetch_all(&db.pool)
    .await
    .expect("inspect deferred cross-table tenant validators");
    assert_eq!(deferred_tenant_validators.len(), 4);
    for (name, owner, security_definer) in deferred_tenant_validators {
        assert_eq!(
            owner, "authz_lookup_definer",
            "{name} must retain its BYPASSRLS trigger owner"
        );
        assert!(
            security_definer,
            "{name} runs after ScopedDb clears tenant context and must remain SECURITY DEFINER"
        );
    }

    let task_columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'tasks' ORDER BY column_name",
    )
    .fetch_all(&db.pool)
    .await
    .expect("inspect final task columns");
    assert!(task_columns.contains(&"base_sha".to_string()));
    for retired in [
        "branch_name",
        "base_commit",
        "workspace_ref",
        "worker_id",
        "lease_expires_at",
        "agent_auth_worker_id",
        "async_decision_policy",
        "task_type",
    ] {
        assert!(
            !task_columns.iter().any(|column| column == retired),
            "retired tasks.{retired} remains"
        );
    }

    for retired_table in ["workspace_branches", "workspace_branch_repos"] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("public.{retired_table}"))
            .fetch_one(&db.pool)
            .await
            .expect("inspect removed relations");
        assert!(!exists, "retired relation {retired_table} remains");
    }
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'workspace_repos'",
    ).fetch_all(&db.pool).await.expect("inspect membership columns");
    for required in ["org_id", "workspace_id", "repo_id", "role", "root_path"] {
        assert!(columns.iter().any(|column| column == required));
    }
    for retired in ["base_branch", "head_sha_cache", "backfilled_from_legacy_at"] {
        assert!(!columns.iter().any(|column| column == retired));
    }

    db.drop().await;
}

#[tokio::test]
async fn historical_upgrade_preserves_history_but_cleanup_requires_reset() {
    let database = UpgradeDatabase::create().await;
    let baseline_dir = TempDir::new().expect("create baseline-only migration directory");
    std::fs::write(
        baseline_dir.path().join("001_lean_baseline.sql"),
        include_bytes!("../src/db/migrations/001_lean_baseline.sql"),
    )
    .expect("write frozen baseline migration");
    g5_api::db::run_migrations_from_path(&database.pool, baseline_dir.path())
        .await
        .expect("apply frozen baseline alone");

    sqlx::raw_sql(
        r#"
        INSERT INTO users(id, email, display_name) VALUES
            ('10000000-0000-4000-8000-000000000001', 'lean-owner@example.test', 'Lean Owner');
        INSERT INTO orgs(id, name, slug) VALUES
            ('20000000-0000-4000-8000-000000000001', 'Lean Fixture Org', 'lean-fixture-org');
        INSERT INTO org_memberships(org_id, user_id, role) VALUES
            ('20000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000001', 'member');
        INSERT INTO repos(id, org_id, name, slug, git_url, default_branch, protected_branches) VALUES
            ('30000000-0000-4000-8000-000000000001', '20000000-0000-4000-8000-000000000001',
             'Code', 'code', 'https://example.test/code.git', 'main', ARRAY['main']);
        INSERT INTO workspaces(id, org_id, name, is_base, status, visibility, created_by) VALUES
            ('40000000-0000-4000-8000-000000000001', '20000000-0000-4000-8000-000000000001',
             'feature/lean-cutover', false, 'active', 'invite_only',
             '10000000-0000-4000-8000-000000000001');
        INSERT INTO workspace_settings(workspace_id, org_id) VALUES
            ('40000000-0000-4000-8000-000000000001', '20000000-0000-4000-8000-000000000001');
        INSERT INTO workspace_branches(
            id, workspace_id, repo_id, branch_name, base_branch, head_commit,
            tracking_mode, org_id, authority_scope_kind
        ) VALUES (
            '50000000-0000-4000-8000-000000000001', '40000000-0000-4000-8000-000000000001',
            '30000000-0000-4000-8000-000000000001', 'feature/lean-cutover', 'main',
            '1111111111111111111111111111111111111111', 'pinned',
            '20000000-0000-4000-8000-000000000001', 'workspace'
        );
        INSERT INTO jobs(id, workspace_id, name, job_type, org_id, created_by) VALUES
            ('70000000-0000-4000-8000-000000000001', '40000000-0000-4000-8000-000000000001',
             'Legacy templated workflow', 'custom', '20000000-0000-4000-8000-000000000001',
             '10000000-0000-4000-8000-000000000001');
        INSERT INTO tasks(
            id, job_id, name, task_type, status, branch_name, base_commit,
            org_id, max_duration_secs
        ) VALUES (
            '80000000-0000-4000-8000-000000000001', '70000000-0000-4000-8000-000000000001',
            'Legacy agent turn', 'regular', 'running', 'legacy-private-ref',
            '2222222222222222222222222222222222222222',
            '20000000-0000-4000-8000-000000000001', 120
        );
        INSERT INTO chat_sessions(
            id, workspace_id, session_type, task_id, scope_type, title, org_id, created_by
        ) VALUES (
            '90000000-0000-4000-8000-000000000001', '40000000-0000-4000-8000-000000000001',
            'task', '80000000-0000-4000-8000-000000000001', 'task', 'Legacy workflow session',
            '20000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000001'
        );
        INSERT INTO task_questions(
            id, org_id, task_id, kind, payload, status, answer, expires_at, answered_by, answered_at
        ) VALUES (
            'a0000000-0000-4000-8000-000000000001', '20000000-0000-4000-8000-000000000001',
            '80000000-0000-4000-8000-000000000001', 'question',
            '{"question":"Historical prompt"}', 'answered', '{"answer":"Historical answer"}',
            now() + interval '1 hour', '10000000-0000-4000-8000-000000000001', now()
        );
        "#,
    )
    .execute(&database.pool)
    .await
    .expect("seed representative baseline rows");

    // Apply the complete compatible prefix before checking the cleanup guard.
    let full_migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let historical = pre_cleanup_catalog(&full_migrations);
    let migrations = historical.path();
    g5_api::db::run_migrations_from_path(&database.pool, &migrations)
        .await
        .expect("upgrade baseline rows through the final cutover");

    let workspace: (Uuid, String) =
        sqlx::query_as("SELECT owner_user_id, branch_name FROM workspaces WHERE id = $1")
            .bind(WORKSPACE_ID)
            .fetch_one(&database.pool)
            .await
            .expect("read migrated workspace authority");
    assert_eq!(workspace, (OWNER_ID, "feature/lean-cutover".to_string()));

    let retained_workspace_branch: (i64, bool) = sqlx::query_as(
        "SELECT count(*), bool_or(is_transient) \
         FROM workspace_branches WHERE workspace_id = $1",
    )
    .bind(WORKSPACE_ID)
    .fetch_one(&database.pool)
    .await
    .expect("read migrated workspace branch authority");
    assert_eq!(retained_workspace_branch, (1, false));

    let task: (String, String, String, Uuid) =
        sqlx::query_as("SELECT kind, status, base_sha, chat_session_id FROM tasks WHERE id = $1")
            .bind(TASK_ID)
            .fetch_one(&database.pool)
            .await
            .expect("read migrated logical task");
    assert_eq!(task.0, "agent");
    assert_eq!(task.1, "waiting");
    assert_eq!(task.2, "2222222222222222222222222222222222222222");
    assert_eq!(task.3, Uuid::from_u128(0x90000000000040008000000000000001));

    let updated_status: String =
        sqlx::query_scalar("UPDATE tasks SET status = 'failed' WHERE id = $1 RETURNING status")
            .bind(TASK_ID)
            .fetch_one(&database.pool)
            .await
            .expect("final task status transitions do not call the retired agent-auth trigger");
    assert_eq!(updated_status, "failed");

    let audit: (String, serde_json::Value) =
        sqlx::query_as("SELECT content, metadata FROM task_events WHERE id = $1")
            .bind(Uuid::from_u128(0xa0000000000040008000000000000001))
            .fetch_one(&database.pool)
            .await
            .expect("read immutable historical question event");
    assert_eq!(audit.0, "Historical prompt");
    assert_eq!(audit.1["kind"], "legacy_question_audit");
    assert_eq!(audit.1["answer"]["answer"], "Historical answer");

    sqlx::query(
        "INSERT INTO chat_messages(\
             session_id, role, content, status, message_index, org_id\
         ) VALUES ($1, 'user', 'Retained user input', 'complete', 0, $2)",
    )
    .bind(Uuid::from_u128(0x90000000000040008000000000000001))
    .bind(Uuid::from_u128(0x20000000000040008000000000000001))
    .execute(&database.pool)
    .await
    .expect("retained shared sessions accept user history after legacy columns are removed");

    let before: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&database.pool)
            .await
            .expect("read migration ledger");
    g5_api::db::run_migrations_from_path(&database.pool, &migrations)
        .await
        .expect("second final migrator run is a no-op");
    let after: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&database.pool)
            .await
            .expect("read no-op migration ledger");
    assert_eq!(before, after);
    let migration_count = std::fs::read_dir(migrations)
        .expect("read final migration directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
        .count();
    assert_eq!(after.len(), migration_count);

    for _ in 0..2 {
        let error = g5_api::db::run_migrations_from_path(&database.pool, &full_migrations)
            .await
            .expect_err("populated installations require the coordinated reset");
        assert!(format!("{error:#}").contains("coordinated database reset"));
        // Check every pooled session before retrying. Reusing the same session
        // can hide a leaked advisory lock because PostgreSQL locks are reentrant;
        // a retry on another session would wait forever instead.
        let mut connections = Vec::new();
        for _ in 0..database.pool.options().get_max_connections() {
            let mut connection = database.pool.acquire().await.unwrap();
            let locks: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_locks \
                 WHERE pid = pg_backend_pid() AND locktype = 'advisory'",
            )
            .fetch_one(&mut *connection)
            .await
            .unwrap();
            assert_eq!(
                locks, 0,
                "failed migration leaked a lock in {}",
                database.name
            );
            connections.push(connection);
        }
        drop(connections);
        let ledger: Vec<(i64, Vec<u8>)> =
            sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&database.pool)
                .await
                .unwrap();
        assert_eq!(
            ledger, after,
            "rejected reset and retry must leave the ledger unchanged"
        );
    }
    let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE id = $1")
        .bind(WORKSPACE_ID)
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(retained, 1);
    database.drop().await;
}

#[tokio::test]
async fn retiring_restart_preserves_inflight_identity_and_rejects_new_restart_state() {
    let database = UpgradeDatabase::create().await;
    let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    // Restart retirement is a historical upgrade contract. The later
    // repository-model cleanup deliberately requires an empty installation.
    let pre_reset = pre_cleanup_catalog(&migrations);
    let migrations = pre_reset.path();
    let previous = TempDir::new().expect("create pre-retirement catalog");
    for entry in std::fs::read_dir(migrations).unwrap() {
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
                < 20
        {
            std::fs::copy(entry.path(), previous.path().join(entry.file_name())).unwrap();
        }
    }
    g5_api::db::run_migrations_from_path(&database.pool, previous.path())
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("fixtures/workspace_restart_upgrade.sql"))
        .execute(&database.pool)
        .await
        .expect("seed admitted restart operations");
    let privilege_sql = "SELECT jsonb_agg(jsonb_build_object('table', c.relname, 'column', a.attname, 'acl', a.attacl::text) ORDER BY c.relname, a.attname)
        FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid
        WHERE c.oid IN ('workspace_vms'::regclass, 'tasks'::regclass) AND a.attnum > 0";
    let initial_privileges: serde_json::Value = sqlx::query_scalar(privilege_sql)
        .fetch_one(&database.pool)
        .await
        .unwrap();
    let vm_sql = "SELECT to_jsonb(vm) FROM workspace_vms vm ORDER BY workspace_id";
    let task_sql = "SELECT to_jsonb(task) FROM tasks task ORDER BY id";
    let mut expected_vms: Vec<serde_json::Value> = sqlx::query_scalar(vm_sql)
        .fetch_all(&database.pool)
        .await
        .unwrap();
    let mut expected_tasks: Vec<serde_json::Value> = sqlx::query_scalar(task_sql)
        .fetch_all(&database.pool)
        .await
        .unwrap();
    assert_eq!(expected_vms.len(), 5);
    for vm in &mut expected_vms {
        for field in ["stop_reason", "pending_stop_reason"] {
            if vm[field] == "restart" {
                vm[field] = "user".into();
            }
        }
        if vm["start_intent"]["kind"] == "restart" {
            vm["start_intent"]["kind"] = "resume".into();
        }
    }
    for task in &mut expected_tasks {
        if task["spec"]["lifecycle"]["interrupt_reason"] == "restart" {
            task["spec"]["lifecycle"]["interrupt_reason"] = "drain".into();
        }
    }
    // updated_at is maintained by database triggers; everything else, including
    // in-flight operation IDs and fences, must remain byte-for-byte equivalent.
    fn without_updated_at(mut rows: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        for row in &mut rows {
            row.as_object_mut().unwrap().remove("updated_at");
        }
        rows
    }
    g5_api::db::run_migrations_from_path(&database.pool, &migrations)
        .await
        .expect("retire restart with populated tenant tables");
    let final_privileges: serde_json::Value = sqlx::query_scalar(privilege_sql)
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(
        final_privileges, initial_privileges,
        "existing column grants must survive the backfill"
    );
    let actual_vms = sqlx::query_scalar(vm_sql)
        .fetch_all(&database.pool)
        .await
        .unwrap();
    let actual_tasks = sqlx::query_scalar(task_sql)
        .fetch_all(&database.pool)
        .await
        .unwrap();
    assert_eq!(
        without_updated_at(actual_vms),
        without_updated_at(expected_vms)
    );
    assert_eq!(
        without_updated_at(actual_tasks),
        without_updated_at(expected_tasks)
    );

    let trigger_enabled: String = sqlx::query_scalar(
        "SELECT tgenabled::text FROM pg_trigger WHERE tgrelid = 'workspace_vms'::regclass AND tgname = 'workspace_vms_validate_start_intent'",
    ).fetch_one(&database.pool).await.unwrap();
    assert_eq!(
        trigger_enabled, "O",
        "normal admission validation must be restored"
    );

    for assignment in [
        "stop_reason = 'restart'",
        "pending_stop_reason = 'restart', pending_stop_deadline_at = now(), nudged_at = now()",
        "start_intent = jsonb_set(start_intent, '{kind}', '\"restart\"'::jsonb)",
    ] {
        let error = sqlx::query(&format!("UPDATE workspace_vms SET {assignment}"))
            .execute(&database.pool)
            .await
            .expect_err("retired restart state must be refused");
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
    }
    g5_api::db::run_migrations_from_path(&database.pool, &migrations)
        .await
        .expect("migration rerun is a no-op");
    database.drop().await;
}

#[tokio::test]
async fn explicit_push_authority_survives_repository_cleanup() {
    let database = UpgradeDatabase::create().await;
    let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let previous = pre_cleanup_catalog(&migrations);
    g5_api::db::run_migrations_from_path(&database.pool, previous.path())
        .await
        .unwrap();
    let inherited_constraint: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint
         WHERE conrelid='branch_integration_queue'::regclass AND conname='branch_integration_queue_authority_shape'"
    ).fetch_one(&database.pool).await.unwrap();
    g5_api::db::run_migrations_from_path(&database.pool, &migrations)
        .await
        .unwrap();
    let current_constraint: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint
         WHERE conrelid='remote_integration_queue'::regclass AND conname='branch_integration_queue_authority_shape'"
    ).fetch_one(&database.pool).await.unwrap();
    assert_eq!(
        inherited_constraint, current_constraint,
        "repository cleanup must preserve the authority constraint from migration 036"
    );
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let mut identity_tx = database.pool.begin().await.unwrap();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Push upgrade','push-upgrade')")
        .bind(org)
        .execute(&mut *identity_tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,'upgrade@g5.test','Owner')")
        .bind(owner)
        .execute(&mut *identity_tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'admin')")
        .bind(org)
        .bind(owner)
        .execute(&mut *identity_tx)
        .await
        .unwrap();
    identity_tx.commit().await.unwrap();
    for trigger in ["commit", "push", "approved_rebase_publish"] {
        let workspace = Uuid::new_v4();
        let repo = Uuid::new_v4();
        sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,$3,$4,$4)")
            .bind(workspace).bind(org).bind(trigger).bind(owner).execute(&database.pool).await.unwrap();
        sqlx::query("INSERT INTO repos(id,org_id,name,slug,git_url) VALUES($1,$2,$3,$3,'https://github.com/example/repo.git')")
            .bind(repo).bind(org).bind(trigger).execute(&database.pool).await.unwrap();
        sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,role,root_path) VALUES($1,$2,$3,'code','code')")
            .bind(org).bind(workspace).bind(repo).execute(&database.pool).await.unwrap();
        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,'main')")
            .bind(org).bind(workspace).bind(repo).execute(&database.pool).await.unwrap();
        sqlx::query("INSERT INTO remote_integration_queue(org_id,workspace_id,repo_id,trigger_kind) VALUES($1,$2,$3,$4)")
            .bind(org).bind(workspace).bind(repo).bind(trigger).execute(&database.pool).await.unwrap();
    }
    let read_rows = "SELECT to_jsonb(q) - 'requested_by_user_id' - 'lifecycle_generation' FROM remote_integration_queue q ORDER BY trigger_kind";
    let before: Vec<serde_json::Value> = sqlx::query_scalar(read_rows)
        .fetch_all(&database.pool)
        .await
        .unwrap();
    g5_api::db::run_migrations_from_path(&database.pool, &migrations)
        .await
        .unwrap();
    let after: Vec<serde_json::Value> = sqlx::query_scalar(read_rows)
        .fetch_all(&database.pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "migration replay preserves existing work without assigning consent"
    );
    let unattributed: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_integration_queue WHERE requested_by_user_id IS NULL AND lifecycle_generation IS NULL")
        .fetch_one(&database.pool).await.unwrap();
    assert_eq!(unattributed, 3);
    let error = sqlx::query("UPDATE remote_integration_queue SET requested_by_user_id=$1")
        .bind(owner)
        .execute(&database.pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    g5_api::db::run_migrations_from_path(&database.pool, &migrations)
        .await
        .expect("upgrade replay is a no-op");
    database.drop().await;
}

struct UpgradeDatabase {
    pool: PgPool,
    name: String,
    admin_url: String,
}

impl UpgradeDatabase {
    async fn create() -> Self {
        let base_url = TestDb::resolve_base_url().await;
        let admin_url = database_url(&base_url, "postgres");
        let admin_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&admin_url)
            .await
            .expect("connect to Postgres admin database");
        let name = format!("g5_lean_cutover_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE \"{name}\""))
            .execute(&admin_pool)
            .await
            .expect("create isolated cutover database");
        admin_pool.close().await;

        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&database_url(&base_url, &name))
            .await
            .expect("connect to isolated cutover database");
        Self {
            pool,
            name,
            admin_url,
        }
    }

    async fn drop(self) {
        self.pool.close().await;
        let admin_pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&self.admin_url)
            .await
            .expect("reconnect for cutover database cleanup");
        sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1")
            .bind(&self.name)
            .execute(&admin_pool)
            .await
            .expect("terminate cutover database connections");
        sqlx::query(&format!("DROP DATABASE \"{}\"", self.name))
            .execute(&admin_pool)
            .await
            .expect("drop isolated cutover database");
        admin_pool.close().await;
    }
}

fn database_url(base_url: &str, database: &str) -> String {
    let mut parsed = Url::parse(base_url).expect("test database URL must parse");
    parsed.set_path(&format!("/{database}"));
    parsed.to_string()
}

#[test]
fn fixture_and_migration_paths_stay_repository_local() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(root
        .join("src/db/migrations/001_lean_baseline.sql")
        .is_file());
    assert!(root
        .join("src/db/migrations/008_lean_mvp_execution_cutover.sql")
        .is_file());
    assert!(root
        .join("src/db/migrations/009_remove_legacy_chat_input_triggers.sql")
        .is_file());
}
