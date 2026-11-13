//! g5-api — shared G5 API server and maintenance daemon entry point.
//!
//! Parses configuration from CLI args / environment variables, initialises
//! the database pool, runs migrations, builds the application state and
//! middleware stack, then starts the HTTP server and/or maintenance daemon.
//!
//! ## Role dispatch (`G5_ROLE`)
//!
//! The process role is controlled by the `G5_ROLE` environment variable:
//!
//! - `api`     — HTTP server only, no background maintenance tasks.
//! - `daemon`  — `MaintenanceRunner` only, **no** HTTP listener.
//! - `all`     — Both HTTP and maintenance (dev / single-node mode, default).

use std::sync::Arc;

use clap::Parser;
use g5_contracts::tracing_compat::{
    default_ansi_enabled, legacy_compatible_env_filter, LegacyTargetFormatter,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::fmt;

use g5_api::config::{AppConfig, CliArgs, LogFormat};
use g5_api::db;
use g5_api::maintenance::{MaintenanceRunner, MaintenanceRunnerDeps};
use g5_api::middleware::{self, InMemoryRateLimitStore};
use g5_api::routes::build_router;
use g5_api::services::AppState;

use g5_auth::{JwtMinter, JwtValidator};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Explicitly size the Tokio runtime instead of relying on the default.
    //
    // Blocking git / NFS work is offloaded to the blocking pool via
    // `tokio::task::spawn_blocking`, but we still keep a healthy floor of async
    // worker threads so the runtime — and the trivial `/healthz` liveness probe
    // — is never starved by a few in-flight requests. Both pools are
    // overridable via env so capacity can be raised in an incident without a
    // code change:
    //   • G5_TOKIO_WORKER_THREADS       — async worker threads (default: max(cores, 4))
    //   • G5_TOKIO_MAX_BLOCKING_THREADS — blocking pool for spawn_blocking (default: 512)
    let worker_threads = std::env::var("G5_TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(4)
                .max(4)
        });
    let max_blocking_threads = std::env::var("G5_TOKIO_MAX_BLOCKING_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(512);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(max_blocking_threads)
        .thread_name("g5-api-worker")
        .enable_all()
        .build()?;

    runtime.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // Early breadcrumb — prints before any parsing so it shows even if
    // clap/tracing init fails.  Visible in `docker compose logs g5-api`.
    eprintln!("[g5-api] process starting (pid {})", std::process::id());

    // --- Parse CLI / env configuration ---
    let args = CliArgs::parse();
    eprintln!(
        "[g5-api] config parsed — host={} port={}",
        args.host, args.port
    );
    let config = AppConfig::try_from_cli(args)?;

    // --- Initialise tracing ---
    let env_filter = legacy_compatible_env_filter(
        &config.server.log_level,
        g5_api::LEGACY_TRACING_TARGET_FAMILIES,
    );

    match config.server.log_format {
        LogFormat::Pretty => {
            let ansi = default_ansi_enabled();
            fmt()
                .with_ansi(ansi)
                .event_format(LegacyTargetFormatter::new(
                    tracing_subscriber::fmt::format().with_ansi(ansi),
                    g5_api::LEGACY_TRACING_TARGET_FAMILIES,
                ))
                .with_env_filter(env_filter)
                .init();
        }
        LogFormat::Json => {
            fmt()
                .json()
                .event_format(LegacyTargetFormatter::new(
                    tracing_subscriber::fmt::format().json(),
                    g5_api::LEGACY_TRACING_TARGET_FAMILIES,
                ))
                .with_env_filter(env_filter)
                .init();
        }
    }

    tracing::info!("Starting g5-api");

    // --- Startup validation ---
    // Fail hard on critical config issues before any resources are allocated.
    config.validate_startup();
    // --- Database ---
    eprintln!("[g5-api] connecting to database...");
    let pool = db::init_pool(&config.database).await?;
    eprintln!("[g5-api] database pool ready");

    // --- Authz pool ---
    // Separate pool for authz session management (authz.user_sessions).
    // Falls back to the same database URL when G5_AUTHZ_DATABASE_URL is unset.
    eprintln!("[g5-api] connecting authz pool...");
    let authz_pool = db::init_pool(&config.authz_database).await?;
    eprintln!("[g5-api] authz pool ready");

    // --- Tenant pool ---
    // Separate pool for ScopedDb (RLS-scoped tenant queries).
    // In production, connects as a restricted Postgres role (`g5_tenant`)
    // that cannot modify authz tables or disable RLS.
    // Falls back to the system pool URL when G5_TENANT_DATABASE_URL is unset.
    eprintln!("[g5-api] connecting tenant pool...");
    let tenant_pool = db::init_pool(&config.tenant_database).await?;
    eprintln!("[g5-api] tenant pool ready");

    // --- Retired login allowlist (g5.user.admission) ---
    // Sign-in admits only pending invitations and existing memberships. A
    // stale deployment value is reported rather than silently trusted.
    if std::env::var_os("G5_AUTH_ALLOWED_EMAILS").is_some() {
        tracing::warn!(
            "G5_AUTH_ALLOWED_EMAILS is set but ignored: the login allowlist is retired; \
             sign-in admits only pending invitations and existing memberships"
        );
    }

    // --- Promoted-image defaults (worker_hosts ontology [D52]) ---
    // Refresh tracking orgs' default_base_image_ref from the digests this
    // deploy promoted (env injected by the helm-deploy step, infra [D27]).
    // Absent/blank env — the normal deploy-only promotion case — is a no-op;
    // any failure logs and never blocks startup (the previous defaults stay).
    let promoted_images = std::env::var("G5_PROMOTED_BASE_IMAGES").ok();
    let new_org_track = std::env::var("G5_NEW_ORG_DEFAULT_IMAGE_TRACK").ok();
    let default_image_policy = g5_api::services::org_service::new_org_image_policy(
        new_org_track.as_deref(),
        promoted_images.as_deref(),
    )?;
    let worker_host_service = g5_api::services::worker_host_service::WorkerHostService::new();
    if let Some(raw) = promoted_images {
        let entries = g5_api::services::worker_host_service::parse_promoted_base_images(&raw);
        if !entries.is_empty() {
            match worker_host_service
                .seed_tracked_org_defaults(&pool, &entries)
                .await
            {
                Ok(updated) => tracing::info!(
                    target: "g5_audit",
                    event = "org_default_seed.applied",
                    images = entries.len(),
                    orgs_updated = updated,
                    "Seeded tracking orgs' default base image from promoted digests"
                ),
                Err(e) => tracing::error!(
                    target: "g5_audit",
                    event = "org_default_seed.failed",
                    error = %e,
                    "Promoted-image seeding failed; keeping existing org defaults"
                ),
            }
        }
    }
    if let Some((track, image_ref)) = default_image_policy {
        let updated = worker_host_service
            .reconcile_unconfigured_org_defaults(&pool, &track, &image_ref)
            .await
            .map_err(|error| error.inner)?;
        tracing::info!(
            target: "g5_audit",
            event = "org_default_reconcile.adopted",
            image_track = track,
            orgs_updated = updated,
            "Applied the configured default image to unconfigured organizations"
        );
    }

    // --- Connection budget logging ---
    // Log total connection budget across all pools and warn if it exceeds
    // 80% of Postgres max_connections (§4.6).
    {
        let system_max = config.database.max_connections;
        let authz_max = config.authz_database.max_connections;
        let tenant_max = config.tenant_database.max_connections;
        let total_budget = system_max + authz_max + tenant_max;

        tracing::info!(
            system_pool = system_max,
            authz_pool = authz_max,
            tenant_pool = tenant_max,
            total = total_budget,
            "Connection budget"
        );

        // Query Postgres max_connections for budget ratio check
        match sqlx::query!(r#"SELECT current_setting('max_connections') AS "max_connections!""#)
            .fetch_one(&pool)
            .await
        {
            Ok(row) => {
                if let Ok(pg_max) = row.max_connections.parse::<u32>() {
                    let ratio = (total_budget as f64) / (pg_max as f64) * 100.0;
                    if ratio > 80.0 {
                        tracing::warn!(
                            total_budget,
                            pg_max_connections = pg_max,
                            ratio_pct = format!("{ratio:.1}"),
                            "Connection budget exceeds 80% of Postgres max_connections!"
                        );
                    } else {
                        tracing::info!(
                            total_budget,
                            pg_max_connections = pg_max,
                            ratio_pct = format!("{ratio:.1}"),
                            "Connection budget within safe limits"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Could not query Postgres max_connections");
            }
        }
    }

    // --- Seed well-known worker service user (E2E / worker bootstrap flows) ---
    // --- WebSocket hub ---
    let ws_hub = Arc::new(g5_api::ws::WsHub::from_app_config(&config));

    // --- Application state ---
    // InProcessAuthz uses the main pool for membership queries (read-only)
    // and the authz_pool for session management (authz.user_sessions).
    let authz: Arc<dyn g5_authz::AuthzService> = Arc::new(g5_authz::InProcessAuthz::new(
        pool.clone(),
        authz_pool.clone(),
    ));
    let authz_for_maintenance = authz.clone();
    let repo_storage =
        g5_api_core::services::git::repo_store::TenantRepoStorage::from_config(&config.storage)
            .map_err(|e| format!("Failed to initialize storage: {e}"))?;
    eprintln!("[g5-api] storage initialized ({:?})", config.storage);
    let authz_pool_for_maintenance = authz_pool.clone();
    let tenant_pool_for_maintenance = tenant_pool.clone();

    // --- JWT minter for worker session tokens ---
    let jwt_minter: Option<Arc<JwtMinter>> = match &config.auth.jwt_private_key {
        Some(pem) => {
            let minter = JwtMinter::new(pem, &config.auth.jwt_issuer, &config.auth.jwt_audience)
                .map_err(|e| format!("Failed to create JwtMinter: {e}"))?;
            eprintln!("[g5-api] JWT minter initialized (worker session tokens enabled)");
            Some(Arc::new(minter))
        }
        None => {
            tracing::warn!("G5_JWT_PRIVATE_KEY not set — worker session tokens disabled");
            None
        }
    };

    let mut app_state = AppState::new(
        pool.clone(),
        tenant_pool,
        &config,
        ws_hub,
        authz,
        repo_storage.clone(),
    );
    app_state.jwt_minter = jwt_minter;
    if !app_state.install_host_control_backend() {
        tracing::warn!("host control stream disabled because JWT signing is unavailable");
    }

    let purged_codex_auth_rows = app_state
        .config_service
        .purge_retired_codex_auth_json(&pool)
        .await
        .map_err(|error| {
            std::io::Error::other(format!(
                "failed to purge retired CODEX_AUTH_JSON Runtime Config values: {error}"
            ))
        })?;
    if purged_codex_auth_rows > 0 {
        tracing::info!(
            target: "g5_audit",
            event = "runtime_config.retired_codex_auth_purged",
            rows_updated = purged_codex_auth_rows,
            "Removed retired CODEX_AUTH_JSON values from Runtime Config"
        );
    }

    // --- JWT validator (needs ES256 public key PEM) ---
    // Set on app_state for WebSocket auth; also used by run_http_server.
    {
        let expected_iss = Some(config.auth.jwt_issuer.as_str());
        let expected_aud = Some(config.auth.jwt_audience.as_str());
        if let Some(ref pem) = config.auth.jwt_public_key {
            let v = JwtValidator::new(pem, expected_iss, expected_aud)
                .map_err(|e| format!("Failed to create JwtValidator: {e}"))?;
            app_state.jwt_validator = Some(Arc::new(v));
        } else {
            tracing::warn!("G5_JWT_PUBLIC_KEY not set — HTTP will reject all JWTs");
        }
    }

    // --- Role dispatch ---
    let role = std::env::var("G5_ROLE").unwrap_or_else(|_| "all".into());
    eprintln!("[g5-api] G5_ROLE={role}");

    let cancel = CancellationToken::new();

    // Single SIGINT/SIGTERM watcher per process: when a signal fires, cancel
    // the shared `cancel` token so every background task (maintenance runner,
    // worker-host reconciler) and the HTTP server can shut down cooperatively.
    // The watcher is per-process, not per-role, so the cancellation contract is
    // identical across `api`/`daemon`/`all` — the only difference is which
    // subscribers are subscribed (ontology [D24], rev-7 §5.7).
    spawn_shutdown_watcher(cancel.clone());

    match role.as_str() {
        "api" => {
            // HTTP server only — no maintenance tasks.
            eprintln!("[g5-api] role=api — starting HTTP server only");
            run_http_server(app_state, &config, pool, cancel.clone()).await?;
        }
        "daemon" => {
            // Maintenance tasks only — NO TcpListener, NO axum::Router.
            // This node never accepts inbound connections.
            eprintln!(
                "[g5-daemon] role=daemon — starting MaintenanceRunner only (no HTTP listener)"
            );
            let runner = MaintenanceRunner::new(
                MaintenanceRunnerDeps {
                    system_pool: pool.clone(),
                    authz_pool: authz_pool_for_maintenance.clone(),
                    tenant_pool: tenant_pool_for_maintenance.clone(),
                    authz: authz_for_maintenance.clone(),
                    repo_storage: repo_storage.clone(),
                    event_bus: app_state.event_bus.clone(),
                    git_bin: app_state.git_bin.clone(),
                    repository_credentials: app_state.repository_credentials.clone(),
                },
                config.maintenance.clone(),
                cancel.clone(),
                config.clone(),
            );
            let handles = runner.spawn_all();

            // Block until the shared shutdown watcher fires `cancel`. The
            // watcher is installed once in `main()`; we just wait on it here so
            // every role uses the same cancellation surface.
            cancel.cancelled().await;
            eprintln!("[g5-daemon] shutdown signal observed — joining maintenance tasks");
            futures::future::join_all(handles).await;
            eprintln!("[g5-daemon] all maintenance tasks stopped — exiting");
        }
        "all" => {
            // Both HTTP + maintenance — dev mode / single-node.
            eprintln!("[g5-api] role=all — starting HTTP server + MaintenanceRunner");
            let runner = MaintenanceRunner::new(
                MaintenanceRunnerDeps {
                    system_pool: pool.clone(),
                    authz_pool: authz_pool_for_maintenance.clone(),
                    tenant_pool: tenant_pool_for_maintenance.clone(),
                    authz: authz_for_maintenance.clone(),
                    repo_storage: repo_storage.clone(),
                    event_bus: app_state.event_bus.clone(),
                    git_bin: app_state.git_bin.clone(),
                    repository_credentials: app_state.repository_credentials.clone(),
                },
                config.maintenance.clone(),
                cancel.clone(),
                config.clone(),
            );
            let _maintenance_handles = runner.spawn_all();
            run_http_server(app_state, &config, pool, cancel.clone()).await?;
        }
        other => {
            return Err(format!("Unknown G5_ROLE: {other}. Expected: api | daemon | all").into());
        }
    }

    Ok(())
}

/// Spawn a process-wide SIGINT/SIGTERM watcher that fires `cancel` when either
/// signal is observed. SIGTERM is the kubernetes/systemd shutdown signal; SIGINT
/// is the interactive Ctrl-C. The watcher is fire-once — re-firing the signal
/// after `cancel.cancel()` falls through to the default handler, so a second
/// Ctrl-C still kills a hung process.
fn spawn_shutdown_watcher(cancel: CancellationToken) {
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to install SIGTERM handler; ctrl-c only");
                    let _ = tokio::signal::ctrl_c().await;
                    cancel.cancel();
                    return;
                }
            };
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    eprintln!("[g5-api] received SIGINT — initiating graceful shutdown");
                }
                _ = term.recv() => {
                    eprintln!("[g5-api] received SIGTERM — initiating graceful shutdown");
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("[g5-api] received ctrl-c — initiating graceful shutdown");
        }
        cancel.cancel();
    });
}

fn http_jwt_validator(configured: &mut Option<Arc<JwtValidator>>) -> Arc<JwtValidator> {
    configured
        .get_or_insert_with(|| {
            tracing::warn!("JWT validator not set on AppState — rejecting all JWTs");
            Arc::new(JwtValidator::reject_all())
        })
        .clone()
}

/// Build the Axum router with middleware and start serving HTTP requests.
async fn run_http_server(
    mut app_state: AppState,
    config: &AppConfig,
    pool: sqlx::PgPool,
    cancel: CancellationToken,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[g5-api] building router and middleware...");

    let jwt_validator = http_jwt_validator(&mut app_state.jwt_validator);

    let state = Arc::new(app_state);

    // Host reachability and durable drain delivery are tied to this process's
    // live tunnel registry, so the reconciler runs on every HTTP-serving role.
    let _host_registry_reconciler =
        g5_api::maintenance::host_registry::spawn(state.clone(), cancel.clone());
    let _workspace_lifecycle_reconciler =
        g5_api::maintenance::workspace_lifecycle::spawn(state.clone(), cancel.clone());
    let _agent_task_reconciler =
        g5_api::maintenance::agent_tasks::spawn(state.clone(), cancel.clone());
    let _formal_approval_handle =
        g5_api::services::approval_review_service::workflow::spawn(state.clone(), cancel.clone());

    let _pull_request_reconciler =
        g5_api::maintenance::workspace_pull_requests::spawn(state.clone(), cancel.clone());
    let _auto_merge_pass = g5_api::auto_merge::spawn(state.clone(), cancel.clone());

    // --- Worker-host status reconciler (PR-rev-7-9c) ---
    // Drives `provisioning → ready`/`terminated` by polling the cloud backend.
    // Only spawned when a cloud backend is wired; a no-op otherwise (ontology
    // [D20]/[D24]). Shares the process-wide `cancel` token so SIGTERM →
    // `cancel.cancel()` flips the reconciler's `cancel.cancelled()` select arm
    // and the loop exits cleanly — mirroring the maintenance-task shutdown
    // contract.
    if state.cloud_provider.is_some() {
        g5_api::cloud::reconciler::spawn_reconciler(state.clone(), cancel.clone());
        eprintln!("[g5-api] worker-host reconciler started");
    }

    // --- Idle worker-host rotation loop (ontology [D47]) ---
    // Recreates aged idle hosts so the 24h host_jwt is re-minted before it
    // expires. Opt-in via G5_HOST_ROTATION_ENABLED (a no-op otherwise); same
    // cancellation contract as the reconciler above.
    if state.cloud_provider.is_some() && g5_api::cloud::rotation::rotation_enabled() {
        g5_api::cloud::rotation::spawn_rotation_loop(state.clone(), cancel.clone());
        eprintln!("[g5-api] worker-host idle-rotation loop started");
    }

    // --- Stale-host detector loop (ontology [D65]/[D66]) ---
    // Converges live GCE instances with live worker_hosts rows in BOTH
    // directions (never-registered reap #782, silent-ready reap #783,
    // cloud-orphan sweep #1464) and drives the bounded stockout retry
    // (#1880). Opt-in via G5_STALE_HOST_DETECTOR_ENABLED (a no-op otherwise);
    // same cancellation contract as the loops above.
    if state.cloud_provider.is_some()
        && state.cloud_features_enabled
        && g5_api::cloud::stale_host::stale_detector_enabled()
    {
        g5_api::cloud::stale_host::spawn_stale_host_detector(state.clone(), cancel.clone());
        eprintln!("[g5-api] worker-host stale-host detector started");
    }

    let state_for_preview_host = state.clone();
    let router = build_router(state);

    // --- Rate limit store ---
    let rate_limit_store = Arc::new(InMemoryRateLimitStore::new());

    // Spawn periodic cleanup of stale rate-limit key state (every 5 min;
    // governor drops state old enough to no longer affect admission)
    {
        let store = Arc::clone(&rate_limit_store);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            interval.tick().await; // skip the immediate first tick
            loop {
                interval.tick().await;
                store.cleanup_stale();
                tracing::debug!("Rate-limit store: cleaned up stale key state");
            }
        });
    }

    // --- Assemble middleware ---
    // Wire the worker_hosts DB-liveness validator so every host_jwt is
    // checked against the live registry (rev-7 §5.7 [D5]/[D7]). The
    // validator uses the system pool because the check runs before any
    // tenant org binding has been set up.
    let host_validator: std::sync::Arc<dyn g5_api::auth::HostAuthValidator> =
        std::sync::Arc::new(g5_api::auth::PgHostAuthValidator::new(pool.clone()));
    let router = middleware::build_middleware_stack(
        router,
        config,
        jwt_validator,
        rate_limit_store,
        Some(pool.clone()),
        Some(host_validator),
    );

    // --- Per-preview subdomain host translation (issue #935) ---
    // Wrapped AROUND the finished router as an outer tower service — NOT via
    // Router::layer — so the URI rewrite runs BEFORE axum resolves the route
    // (middleware/preview_host [D1]). Axum matches the route first and only
    // then runs Router::layer middleware around the matched service, so a
    // rewrite attached that way can never change which route matches: every
    // translated preview-origin request fell through to the empty-404
    // fallback (issue G5Labs/G5#1123). Requests on
    // https://<workspace-id>.<G5_PREVIEW_HOST_SUFFIX>/ are rewritten to the
    // /api/v1/preview/<workspaceId>/ route shape and then flow through the FULL
    // normal stack (routes/preview [I3]). A pure pass-through when the suffix
    // is not configured, in which case previews are unavailable
    // (`g5.security.mvp-exposure.preview`).
    if state_for_preview_host.preview_host_suffix.is_some() {
        eprintln!(
            "[g5-api] per-preview subdomain origins enabled (suffix: {})",
            state_for_preview_host
                .preview_host_suffix
                .as_deref()
                .unwrap_or("")
        );
    }
    // --- Bind and serve ---
    let bind_addr = config.server.bind_addr();
    eprintln!("[g5-api] binding to {bind_addr}");
    tracing::info!(%bind_addr, "Listening");

    let listener = TcpListener::bind(&bind_addr).await?;
    eprintln!("[g5-api] listening — ready to accept connections");
    // Graceful shutdown: the same `cancel` token that stops the reconciler also
    // signals axum to stop accepting new connections and drain in-flight ones.
    g5_api::http_server::serve(listener, router, state_for_preview_host, async move {
        cancel.cancelled().await;
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn absent_verifier_is_shared_with_git_and_websocket_routes() {
        use g5_api::middleware::AuthLayer;

        let directory = tempfile::TempDir::new().unwrap();
        let mut config = AppConfig::from_cli(
            CliArgs::try_parse_from(["g5-api", "--database-url", "postgres://unused/unused"])
                .unwrap(),
        );
        config.storage = g5_storage::StorageConfig::Local {
            base_dir: directory.path().join("repos"),
            worktree_dir: directory.path().join("worktrees"),
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let mut state = AppState::new_for_test(pool, &config).await;
        let validator = http_jwt_validator(&mut state.jwt_validator);
        let router = build_router(Arc::new(state)).layer(AuthLayer::new(validator));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let id = uuid::Uuid::new_v4();
        for path in [
            "/api/v1/ws?token=untrusted".to_string(),
            format!("/api/v1/workspaces/{id}/terminal?token=untrusted"),
        ] {
            let response = client
                .get(format!("http://{address}{path}"))
                .header("connection", "upgrade")
                .header("upgrade", "websocket")
                .header("sec-websocket-version", "13")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                reqwest::StatusCode::UNAUTHORIZED,
                "{path}"
            );
        }
        for (method, path) in [
            (reqwest::Method::GET, "info/refs?service=git-upload-pack"),
            (reqwest::Method::POST, "git-upload-pack"),
            (reqwest::Method::POST, "git-receive-pack"),
        ] {
            for basic in [true, false] {
                let request =
                    client.request(method.clone(), format!("http://{address}/git/{id}/{path}"));
                let request = if basic {
                    request.basic_auth("g5", Some("untrusted"))
                } else {
                    request.bearer_auth("untrusted")
                };
                let response = request.send().await.unwrap();
                assert_eq!(
                    response.status(),
                    reqwest::StatusCode::UNAUTHORIZED,
                    "{path}, basic={basic}"
                );
            }
        }
        server.abort();
    }

    #[tokio::test]
    async fn http_without_a_verifier_rejects_fixture_signed_jwts() {
        use axum::{routing::get, Router};
        use g5_api::middleware::AuthLayer;
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        let now = chrono::Utc::now().timestamp();
        let claims = serde_json::json!({
            "sub": uuid::Uuid::new_v4(), "org": uuid::Uuid::new_v4(),
            "scopes": [], "iat": now, "exp": now + 60,
        });
        let private = include_str!("../../../scripts/ci/e2e-ci-jwt-private.pem");
        let public = include_str!("../../../scripts/ci/e2e-ci-jwt-public.pem");
        let token = encode(
            &Header::new(Algorithm::ES256),
            &claims,
            &EncodingKey::from_ec_pem(private.as_bytes()).unwrap(),
        )
        .unwrap();
        for (mut configured, expected) in [
            (None, reqwest::StatusCode::UNAUTHORIZED),
            // Isolated harness control: the request is otherwise authenticatable.
            (
                Some(Arc::new(JwtValidator::new(public, None, None).unwrap())),
                reqwest::StatusCode::OK,
            ),
        ] {
            let router = Router::new()
                .route("/protected", get(|| async { "authenticated" }))
                .layer(AuthLayer::new(http_jwt_validator(&mut configured)));
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let response = reqwest::Client::new()
                .get(format!("http://{address}/protected"))
                .bearer_auth(&token)
                .send()
                .await;
            server.abort();
            assert_eq!(response.unwrap().status(), expected);
        }
    }
}
