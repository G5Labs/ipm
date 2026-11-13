//! Maintenance module for g5-api.
//!
//! Spawns background Tokio tasks for database housekeeping operations:
//! - Usage record aggregation for billing dashboards
//! - Expired invitation garbage collection
//! - Old timeline entry pruning
//!
//! All tasks run independently, respect CancellationToken for graceful shutdown,
//! and log errors without crashing.

use crate::services::git::repo_store::TenantRepoStorage;
use std::sync::Arc;

use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::{AppConfig, MaintenanceConfig};

/// Tracing target of the runner's own events (see `SERVICES_LOG_TARGET`): a
/// `g5_api::maintenance` directive selects them and the jobs, now physically
/// under their service contexts, through the legacy target table.
pub(crate) const MAINTENANCE_LOG_TARGET: &str = "g5_api::maintenance";
pub use crate::db::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
    SYSTEM_MAINTENANCE_ACTOR_UUID,
};

// Each job lives in the service context it serves and keeps its
// `crate::maintenance::<job>` path.
pub use crate::services::agents::{agent_attachment_retention, agent_tasks, timeline_cleanup};
pub use crate::services::git::{
    branch_integration_scheduler, branch_integration_worker, remote_advisory_pinger,
    remote_sync_run_retention, repo_gc, workspace_flush_retention, workspace_pull_requests,
};
pub use crate::services::hosts::host_registry;
pub use crate::services::identity::{
    invitation_gc, platform_issue_outbox, session_pruning, usage_aggregation,
};
pub use crate::services::workspace::{approval_request_retention, workspace_lifecycle};

#[derive(Clone)]
pub struct MaintenanceRunnerDeps {
    pub system_pool: PgPool,
    pub authz_pool: PgPool,
    pub tenant_pool: PgPool,
    pub authz: Arc<dyn g5_authz::AuthzService>,
    pub repo_storage: TenantRepoStorage,
    pub event_bus: crate::services::EventBus,
    /// Path to the `git` binary, resolved once at startup. Used by the repo-GC
    /// (git pack-maintenance) task to run `git count-objects`/`git gc`.
    pub git_bin: std::path::PathBuf,
    /// Shared repository credential policy for every provider operation.
    pub repository_credentials: Arc<crate::auth::RepositoryCredentials>,
}

/// Central coordinator for all background maintenance tasks.
///
/// Constructed during server startup in `main.rs` and used to spawn
/// all maintenance tasks as independent Tokio tasks.
pub struct MaintenanceRunner {
    system_pool: PgPool,
    authz_pool: PgPool,
    tenant_pool: PgPool,
    authz: Arc<dyn g5_authz::AuthzService>,
    config: MaintenanceConfig,
    cancel: CancellationToken,
    repo_storage: TenantRepoStorage,
    event_bus: crate::services::EventBus,
    git_bin: std::path::PathBuf,
    repository_credentials: Arc<crate::auth::RepositoryCredentials>,
    /// Platform bug-reporting flag + GitHub delivery config; gates the
    /// platform-issue outbox worker (only spawned when fully configured).
    platform_issue: crate::config::PlatformIssueConfig,
    /// Outbound email handle; its worker runs only when email is enabled.
    email: crate::services::Email,
}

impl MaintenanceRunner {
    /// Creates a new MaintenanceRunner.
    ///
    /// # Arguments
    ///
    /// * `pool` — PostgreSQL connection pool (cloned into each spawned task)
    /// * `config` — Maintenance configuration with intervals and thresholds
    /// * `cancel` — Cancellation token for graceful shutdown coordination
    /// * `app_config` — Full application config for maintenance-owned services
    /// * `repo_storage` — Repository storage backend
    pub fn new(
        deps: MaintenanceRunnerDeps,
        config: MaintenanceConfig,
        cancel: CancellationToken,
        app_config: AppConfig,
    ) -> Self {
        Self {
            system_pool: deps.system_pool,
            authz_pool: deps.authz_pool,
            tenant_pool: deps.tenant_pool,
            authz: deps.authz,
            config,
            cancel,
            repo_storage: deps.repo_storage,
            event_bus: deps.event_bus,
            git_bin: deps.git_bin,
            repository_credentials: deps.repository_credentials,
            platform_issue: app_config.platform_issue.clone(),
            // Startup validation already rejected an incomplete configuration.
            email: crate::services::Email::from_config(
                &app_config.email,
                &app_config.server.external_url,
            )
            .unwrap_or_else(|message| panic!("outbound email: {message}")),
        }
    }

    /// Spawns all background maintenance tasks as independent Tokio tasks.
    ///
    /// Returns a `Vec<JoinHandle<()>>` containing one handle per spawned task.
    /// The caller (`main.rs`) should await these handles during graceful shutdown
    /// to ensure all tasks have exited cleanly.
    ///
    /// Each task runs an infinite loop on its configured interval, logs errors
    /// without crashing, and exits when the cancellation token is cancelled.
    pub fn spawn_all(&self) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::with_capacity(18);
        // Terminal approval-request coordination retention
        {
            let deps = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let cancel = self.cancel.clone();
            let config = approval_request_retention::ApprovalRequestRetentionConfig::default();
            handles.push(tokio::spawn(async move {
                approval_request_retention::run_approval_request_retention_loop(
                    deps, config, cancel,
                )
                .await;
            }));
        }
        // Agent screenshot attachment retention (chat_service D63)
        {
            let deps = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let cancel = self.cancel.clone();
            let config = agent_attachment_retention::AgentAttachmentRetentionConfig::default();
            handles.push(tokio::spawn(async move {
                agent_attachment_retention::run_agent_attachment_retention_loop(
                    deps, config, cancel,
                )
                .await;
            }));
        }

        // Usage Aggregation
        {
            let deps = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let config = self.config.clone();
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                usage_aggregation::run_usage_aggregation_loop(deps, config, cancel).await;
            }));
        }

        // Invitation GC
        {
            let pool = self.system_pool.clone();
            let config = self.config.clone();
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                invitation_gc::run_invitation_gc(pool, config, cancel).await;
            }));
        }

        // Timeline Cleanup
        {
            let deps = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let cancel = self.cancel.clone();
            let config = timeline_cleanup::TimelineCleanupConfig::default();
            handles.push(tokio::spawn(async move {
                timeline_cleanup::run_timeline_cleanup_loop(deps, config, cancel).await;
            }));
        }

        // Session Pruning (authz.user_sessions + authz.session_ctx)
        {
            let pool = self.authz_pool.clone();
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                session_pruning::run_session_pruning(pool, cancel).await;
            }));
        }

        // Repo Fetcher intentionally not spawned.
        // Tracking branches now advance only via explicit user-triggered sync.

        // Repo GC — git pack-maintenance: repack repos with too many loose
        // objects so NFS-served clones (`git upload-pack`) stay fast.
        {
            let deps = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let git_bin = self.git_bin.clone();
            let config = self.config.clone();
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                repo_gc::run_repo_gc(deps, git_bin, config, cancel).await;
            }));
        }

        // Workspace flush retention — prune immutable receipt refs only after
        // current, fork, restore, and reader pins have been excluded.
        {
            let tenant = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let deps = workspace_flush_retention::WorkspaceFlushRetentionDeps {
                tenant,
                repo_storage: self.repo_storage.clone(),
            };
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                workspace_flush_retention::run_workspace_flush_retention(deps, cancel).await;
            }));
        }

        // Platform bug-report delivery outbox — deliver reports to GitHub with
        // retry/backoff. Only spawned when the feature is enabled AND the GitHub
        // delivery config is fully present (fail-closed); otherwise no worker runs.
        let platform_issue_github =
            match crate::services::PlatformIssueGithubClient::provider_clients() {
                Ok(http) => crate::services::PlatformIssueGithubClient::from_platform_config(
                    http,
                    &self.platform_issue,
                ),
                Err(error) => {
                    tracing::error!(target: MAINTENANCE_LOG_TARGET, %error, "platform issue reporting HTTP client initialization failed");
                    None
                }
            };
        let platform_issue_outbox_enabled = platform_issue_github.is_some();
        if let Some(github) = platform_issue_github {
            let pool = self.system_pool.clone();
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                platform_issue_outbox::run_platform_issue_outbox_loop(pool, github, cancel).await;
            }));
        }

        // Outbound email delivery — drains `email_deliveries` with leases and
        // bounded retries. Not spawned when email is disabled.
        if let Some(worker) = self
            .email
            .worker(self.system_pool.clone(), crate::services::email_gates())
        {
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                worker.run(cancel).await;
            }));
        }

        // Branch Integration Worker — drains the remote-tracking queue.
        // Phase 2.B of the remote-repo-tracking work.
        {
            let deps = branch_integration_worker::BranchIntegrationWorkerDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
                repository_credentials: self.repository_credentials.clone(),
                event_bus: self.event_bus.clone(),
                landed_commit_notifier: Arc::new(
                    crate::services::review_source_notifications::ReviewSourceNotifier::new(
                        self.system_pool.clone(),
                        self.tenant_pool.clone(),
                        self.authz.clone(),
                        self.event_bus.clone(),
                    ),
                ),
            };
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                branch_integration_worker::run_branch_integration_worker(deps, cancel).await;
            }));
        }

        // Branch Integration Scheduler — enqueues `background_inbound`
        // rows for unpaused remote-tracking branches. Phase 3.B.
        {
            let deps = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let interval = self.config.remote_tracking_scheduler_interval;
            let passive_enabled = self.config.passive_auto_pull_enabled;
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                branch_integration_scheduler::run_branch_integration_scheduler(
                    deps,
                    interval,
                    passive_enabled,
                    cancel,
                )
                .await;
            }));
        }

        // Remote Advisory Pinger — ls-remotes dirty remote-tracking
        // branches to refresh the advisory SHA. Phase 3.C.
        {
            let deps = remote_advisory_pinger::RemoteAdvisoryPingerDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repository_credentials: self.repository_credentials.clone(),
                event_bus: self.event_bus.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let interval = self.config.remote_tracking_advisory_interval;
            let passive_enabled = self.config.passive_auto_pull_enabled;
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                remote_advisory_pinger::run_remote_advisory_pinger(
                    deps,
                    interval,
                    passive_enabled,
                    cancel,
                )
                .await;
            }));
        }

        // Remote sync run retention — bounds the remote integration attempt
        // history (`g5.governance.remote.sync-history`).
        {
            let deps = TenantMaintenanceDeps {
                system_pool: self.system_pool.clone(),
                tenant_pool: self.tenant_pool.clone(),
                authz: self.authz.clone(),
                repo_storage: self.repo_storage.clone(),
            };
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                remote_sync_run_retention::run_remote_sync_run_retention(deps, cancel).await;
            }));
        }

        // Rate-limit window trim — the shared quota table is one row per key
        // and resets windows in place; this only drops keys idle for a day.
        {
            let pool = self.system_pool.clone();
            let cancel = self.cancel.clone();
            handles.push(tokio::spawn(async move {
                crate::middleware::run_rate_limit_window_trim(pool, cancel).await;
            }));
        }

        tracing::info!(target: MAINTENANCE_LOG_TARGET,
            task_count = handles.len(),
            platform_issue_outbox_enabled,
            email_delivery_enabled = self.email.is_enabled(),
            "Maintenance tasks spawned: approval_request_retention, usage_aggregation, \
             invitation_gc, timeline_cleanup, session_pruning, \
             repo_gc, workspace_flush_retention, \
             branch_integration_worker, branch_integration_scheduler, \
             remote_advisory_pinger, remote_sync_run_retention, rate_limit_window_trim"
        );

        handles
    }
}
