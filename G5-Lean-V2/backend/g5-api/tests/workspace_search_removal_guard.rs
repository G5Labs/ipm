//! P8.9 boundary regressions for workspace-search removal.
//!
//! Workspace search is removed end to end: its public query endpoint, the
//! authenticated instance status endpoint, the internal reindex trigger, the
//! background indexer, the queue and document tables and the embedding
//! settings. This guard proves each removed surface is absent from the running
//! server and from production source, and that the retained capabilities it
//! must not have taken with it — chat-history search over session messages and
//! the docs/ontology projections — still answer.

use crate::helpers;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use g5_api::config::MaintenanceConfig;
use g5_api::services::EventBus;
use g5_api::ws::{WsHub, WsHubConfig};
use g5_auth::Scope;
use helpers::{EnvVarGuard, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::Value;
use wiremock::MockServer;

// ---------------------------------------------------------------------------
// Runtime boundary: removed endpoints are gone, retained ones still answer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn removed_workspace_search_endpoints_are_absent_and_chat_history_search_survives() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::SessionsRead])
            .await;
    let client = app.authenticated_client(&user);

    // Public workspace query surface.
    let removed = client
        .get(&format!(
            "/api/v1/workspaces/{}/search?q=anything",
            workspace.id
        ))
        .await;
    assert_eq!(
        removed.status(),
        StatusCode::NOT_FOUND,
        "GET /workspaces/:id/search must not be routed"
    );

    // Authenticated index-status surface.
    let status = client.get("/api/v1/instance/search/status").await;
    assert_eq!(
        status.status(),
        StatusCode::NOT_FOUND,
        "GET /instance/search/status must not be routed"
    );

    // The health payload no longer advertises a search subsystem, and the
    // instance feature list no longer advertises the capability.
    let health: Value = app
        .client
        .get(format!("{}/api/v1/instance/health", app.base_url))
        .send()
        .await
        .expect("health request")
        .json()
        .await
        .expect("health JSON");
    assert!(
        health["data"]["checks"].get("search").is_none(),
        "health checks must not report a search subsystem: {health}"
    );
    let info: Value = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .expect("info request")
        .json()
        .await
        .expect("info JSON");
    let features = info["data"]["features"]
        .as_array()
        .expect("instance feature list");
    assert!(
        !features.iter().any(|feature| feature == "search"),
        "instance info must not advertise the removed search feature: {info}"
    );

    // Retained: chat-history search over session titles/summaries.
    let retained = client
        .get(&format!(
            "/api/v1/workspaces/{}/sessions/search?q=anything",
            workspace.id
        ))
        .await;
    assert_eq!(
        retained.status(),
        StatusCode::OK,
        "chat-history session search must keep working"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_settings_no_longer_carry_embedding_configuration() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);

    // Creating and reading settings still works with the columns removed.
    let response = client
        .get(&format!("/api/v1/workspaces/{}/settings", workspace.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("settings JSON");
    for field in ["embeddingModel", "embeddingDimensions", "embeddingProvider"] {
        assert!(
            body["data"].get(field).is_none(),
            "settings must not expose {field}: {body}"
        );
    }

    // A write that still carries the retired key is accepted and ignored, not
    // persisted into a resurrected column.
    let update = client
        .put_json(
            &format!("/api/v1/workspaces/{}/settings", workspace.id),
            &serde_json::json!({ "embeddingProvider": "openai", "idleTimeoutSecs": 600 }),
        )
        .await;
    assert_eq!(update.status(), StatusCode::OK);
    let updated: Value = update.json().await.expect("updated settings JSON");
    assert_eq!(updated["data"]["idleTimeoutSecs"], 600);
    assert!(updated["data"].get("embeddingProvider").is_none());

    let columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE table_name = 'workspace_settings' AND column_name LIKE 'embedding%'",
    )
    .fetch_one(&app.db)
    .await
    .expect("column inventory");
    assert_eq!(columns, 0, "embedding_* columns must be dropped");

    app.cleanup().await;
}

#[tokio::test]
async fn search_schema_is_dropped_and_chat_search_remains() {
    let app = TestApp::spawn().await;

    for table in [
        "search_documents",
        "search_reindex_queue",
        "projection_state",
    ] {
        let present: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('public.' || $1)::text")
                .bind(table)
                .fetch_one(&app.db)
                .await
                .expect("table lookup");
        assert_eq!(present, None, "{table} must be dropped");
    }

    // No *object* may use pgvector any more. Whether the extension itself is
    // still installed is environment-dependent and deliberately not asserted:
    // migration 002 drops it only when its role owns it, because CI and Cloud
    // SQL pre-create it as the platform admin and the deployment migrator is a
    // non-superuser that cannot drop it (see the migration's section 5). What
    // must hold everywhere is that nothing references it.
    let vector_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns WHERE udt_name = 'vector'",
    )
    .fetch_one(&app.db)
    .await
    .expect("vector column lookup");
    assert_eq!(
        vector_columns, 0,
        "no column may still be typed with pgvector"
    );

    let vector_indexes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_indexes WHERE indexdef ILIKE '%vector\\_%\\_ops%'",
    )
    .fetch_one(&app.db)
    .await
    .expect("vector index lookup");
    assert_eq!(
        vector_indexes, 0,
        "no index may still use a pgvector operator class"
    );

    let vector_dependents: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_depend d \
           JOIN pg_extension e ON e.oid = d.refobjid \
          WHERE e.extname = 'vector' AND d.deptype = 'n' \
            AND d.classid <> 'pg_extension'::regclass",
    )
    .fetch_one(&app.db)
    .await
    .expect("extension dependency lookup");
    assert_eq!(
        vector_dependents, 0,
        "no schema object may still depend on the pgvector extension"
    );

    // Chat-history full-text indexes are core PostgreSQL, not pgvector, and stay.
    let chat_indexes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_indexes WHERE indexname IN ('idx_msg_fts', 'idx_session_search')",
    )
    .fetch_one(&app.db)
    .await
    .expect("chat index lookup");
    assert_eq!(chat_indexes, 2, "chat-history search indexes must survive");

    app.cleanup().await;
}

/// Runtime boundary: an upgraded deployment can still carry rows a prior
/// version left behind. Starting the maintenance runner against such a
/// database must not consume them and must make no embedding network call.
///
/// The scenario is built so a surviving consumer would actually be caught:
///
/// * the migration has already dropped the queue, so the test recreates a
///   minimal legacy `search_reindex_queue` **with the baseline's `g5_app`
///   grants** — the runner's system pool logs in as that non-superuser role,
///   so without the grants a surviving indexer would only hit permission
///   errors and leave the rows alone;
/// * the `processing` row is genuinely stale (`started_at` 30 minutes ago), so
///   the deleted indexer's startup reaper, whose predicate was
///   `status = 'processing' AND started_at < now() - 10 minutes`, would reset
///   it on its very first pass;
/// * the rows are compared field by field, not just by status;
/// * every spawned task must shut down cleanly, so a panicking consumer cannot
///   be mistaken for an absent one.
///
/// `LOCAL_EMBEDDING_URL` is redirected at a recording mock, which covers the
/// `local` provider end to end. The `openai` provider's endpoint was
/// hard-coded in the deleted service, so it cannot be redirected at runtime;
/// its absence is asserted statically instead — `v1/embeddings` is one of the
/// forbidden markers in
/// `production_source_carries_no_workspace_search_symbols`.
#[tokio::test]
#[serial_test::serial]
async fn legacy_queue_rows_do_not_start_indexing_or_embedding_on_restart() {
    let embedding_endpoint = MockServer::start().await;
    // Both providers the deleted SearchService could have used. If any
    // embedding code survived, it would dial one of these.
    let _embedding_env = EnvVarGuard::set_many(&[
        ("OPENAI_API_KEY", "test-embedding-key-must-not-be-used"),
        (
            "LOCAL_EMBEDDING_URL",
            &format!("{}/embeddings", embedding_endpoint.uri()),
        ),
    ]);

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    sqlx::query(
        "CREATE TABLE search_reindex_queue ( \
             id uuid PRIMARY KEY DEFAULT gen_random_uuid(), \
             workspace_id uuid NOT NULL, \
             branch_id uuid NOT NULL, \
             org_id uuid NOT NULL, \
             commit_sha text NOT NULL, \
             status text NOT NULL DEFAULT 'pending', \
             attempts integer NOT NULL DEFAULT 0, \
             max_attempts integer NOT NULL DEFAULT 3, \
             started_at timestamptz, \
             completed_at timestamptz, \
             error_message text, \
             created_at timestamptz NOT NULL DEFAULT now() \
         )",
    )
    .execute(&app.db)
    .await
    .expect("recreate a legacy queue table as a prior version would have left it");
    // Without these the runner's non-superuser login could not touch the table
    // at all, and a surviving consumer would look identical to an absent one.
    sqlx::query("GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE search_reindex_queue TO g5_app")
        .execute(&app.db)
        .await
        .expect("restore the baseline grants the deleted indexer relied on");
    sqlx::query("GRANT INSERT ON TABLE search_reindex_queue TO g5_tenant")
        .execute(&app.db)
        .await
        .expect("restore the baseline tenant grant");

    sqlx::query(
        "INSERT INTO search_reindex_queue \
             (workspace_id, branch_id, org_id, commit_sha, status, attempts) \
         VALUES ($1, $2, $3, $4, 'pending', 1)",
    )
    .bind(workspace.id)
    .bind(uuid::Uuid::new_v4())
    .bind(org.id)
    .bind(&workspace.head_commit)
    .execute(&app.db)
    .await
    .expect("seed a queued row");
    sqlx::query(
        "INSERT INTO search_reindex_queue \
             (workspace_id, branch_id, org_id, commit_sha, status, attempts, started_at) \
         VALUES ($1, $2, $3, $4, 'processing', 1, now() - interval '30 minutes')",
    )
    .bind(workspace.id)
    .bind(uuid::Uuid::new_v4())
    .bind(org.id)
    .bind(&workspace.head_commit)
    .execute(&app.db)
    .await
    .expect("seed a row the deleted startup reaper would have reset");

    // Prove the runner's own login can read the table, so "nothing changed"
    // cannot be explained away by a permission error.
    let visible_to_runtime_role: i64 =
        sqlx::query_scalar("SELECT count(*) FROM search_reindex_queue")
            .fetch_one(&app.system_pool)
            .await
            .expect("the runtime system role must be able to read the recreated queue");
    assert_eq!(
        visible_to_runtime_role, 2,
        "the recreated queue must be reachable by the role the runner uses"
    );

    let cancel = tokio_util::sync::CancellationToken::new();
    let runner = g5_api::maintenance::MaintenanceRunner::new(
        g5_api::maintenance::MaintenanceRunnerDeps {
            system_pool: app.system_pool.clone(),
            authz_pool: app.authz_pool.clone(),
            tenant_pool: app.state.tenant_pool.clone(),
            authz: app.state.authz.clone(),
            repo_storage: app.repo_storage.clone(),
            event_bus: EventBus::new(Arc::new(WsHub::new(WsHubConfig::default()))),
            git_bin: app.state.git_bin.clone(),
            repository_credentials: app.state.repository_credentials.clone(),
        },
        MaintenanceConfig {
            invitation_gc_interval: Duration::from_secs(3600),
            stale_draft_timeout: Duration::from_secs(86400),
            draft_stale_retention: Duration::from_secs(7 * 24 * 3600),
            repo_gc_interval: Duration::from_secs(3600),
            repo_gc_loose_threshold: 500,
            remote_tracking_scheduler_interval: Duration::from_secs(60),
            remote_tracking_advisory_interval: Duration::from_secs(60),
            passive_auto_pull_enabled: true,
        },
        cancel.clone(),
        helpers::test_config(),
    );

    let handles = runner.spawn_all();
    // The deleted indexer reset stuck entries and drained a first batch
    // immediately at startup, before its 10s poll interval, so a short window
    // is enough for a surviving consumer to touch these rows.
    tokio::time::sleep(Duration::from_secs(3)).await;
    cancel.cancel();
    for handle in handles {
        tokio::time::timeout(Duration::from_secs(15), handle)
            .await
            .expect("every maintenance task must shut down within 15s")
            .expect("no maintenance task may panic");
    }

    let requests = embedding_endpoint
        .received_requests()
        .await
        .expect("mock server records requests");
    assert!(
        requests.is_empty(),
        "no maintenance task may make an embedding request: {:?}",
        requests
            .iter()
            .map(|request| request.url.to_string())
            .collect::<Vec<_>>()
    );

    // A claim would change status/attempts/started_at, a completion or failure
    // would set completed_at/error_message, and the startup reaper would null
    // out the stale started_at.
    let untouched: Vec<(String, i32, bool, bool, bool)> = sqlx::query_as(
        "SELECT status, attempts, started_at IS NULL, completed_at IS NULL, \
                error_message IS NULL \
           FROM search_reindex_queue ORDER BY status",
    )
    .fetch_all(&app.db)
    .await
    .expect("read back the stale queue rows");
    assert_eq!(
        untouched,
        vec![
            ("pending".to_string(), 1, true, true, true),
            ("processing".to_string(), 1, false, true, true),
        ],
        "no surviving consumer may claim, retry, complete or reap legacy queue rows"
    );

    app.cleanup().await;
}

// ---------------------------------------------------------------------------
// Static boundary: no production source may resurrect the removed surface
// ---------------------------------------------------------------------------

fn backend_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api lives directly under backend/")
        .to_path_buf()
}

/// Every `.rs` file under `backend/<crate>/src`, i.e. production source only —
/// tests, fixtures and SQL migrations are excluded on purpose.
fn production_sources() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![backend_dir()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name == "target" || name == "tests" || name == "benches" {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let is_src = path
                    .components()
                    .any(|component| component.as_os_str() == "src");
                if is_src {
                    files.push(path);
                }
            }
        }
    }
    files
}

#[test]
fn production_source_carries_no_workspace_search_symbols() {
    // Agents keep their providers' own local code search, so this guard names
    // the removed platform symbols rather than the word "search".
    const FORBIDDEN: &[&str] = &[
        "search_documents",
        "search_reindex_queue",
        "SearchService",
        "search_indexer",
        "embedding_provider",
        "embedding_dimensions",
        "v1/embeddings",
        "SearchStatusDTO",
        "SearchResponseDTO",
        "SearchUnavailable",
        "LOCAL_EMBEDDING_URL",
    ];

    let mut offenders: BTreeSet<String> = BTreeSet::new();
    for path in production_sources() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for needle in FORBIDDEN {
            if text.contains(needle) {
                offenders.insert(format!("{}: {needle}", path.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "workspace search must not be reintroduced into production source:\n{}",
        offenders.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_removal_migration_is_present_and_checksummed() {
    let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let migration = migrations.join("002_remove_workspace_search.sql");
    let sql = fs::read_to_string(&migration).expect("the removal migration must exist");
    for dropped in [
        "DROP TABLE IF EXISTS public.search_reindex_queue",
        "DROP TABLE IF EXISTS public.search_documents",
        // The extension drop is privilege-aware (see the migration's section
        // 5): it runs only when this migration's role owns the extension, so
        // a non-superuser deployment migrator can still apply the migration.
        "DROP EXTENSION vector;",
        "WHEN insufficient_privilege THEN",
    ] {
        assert!(
            sql.contains(dropped),
            "the removal migration must contain `{dropped}`"
        );
    }
    let checksums =
        fs::read_to_string(migrations.join("checksums.sha384")).expect("checksum catalog");
    assert!(
        checksums.contains("002_remove_workspace_search.sql"),
        "the removal migration needs a reviewed checksum entry"
    );
}
