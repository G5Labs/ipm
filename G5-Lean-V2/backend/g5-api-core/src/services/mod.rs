// ---------------------------------------------------------------------------
// g5-api/src/services/mod.rs — Service Layer Module Root
//
// Responsibilities:
// 1. Module declarations and re-exports for all service submodules
// 2. AppState struct — shared application state for axum handlers
// 3. EventBus struct — event publishing abstraction over WsHub
// 4. PaginationParams / PaginatedResult — shared pagination types
// ---------------------------------------------------------------------------

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::cloud::CloudProvider;
use crate::config::AppConfig;
use crate::middleware::{PostgresRateLimitStore, RateLimitStore};
use crate::ws::{Channel, WsEvent, WsHub};
use g5_auth::{JwtMinter, JwtValidator};

pub(crate) const SERVICES_LOG_TARGET: &str = "g5_api::services";

// ---------------------------------------------------------------------------
// Submodule declarations
// ---------------------------------------------------------------------------

// Bounded contexts, each depending only on the earlier ones; the order is
// pinned by backend/g5-api/tests/service_context_layering.rs.
pub mod agents;
pub mod email;
pub mod git;
pub mod hosts;
pub mod identity;
pub mod workspace;

// Every service keeps its `crate::services::<name>` path, the crate-private
// and private ones included (their callers name the context path today,
// hence the allow; the paths stay for the reach they had).
#[allow(unused_imports)]
use agents::session_file_sha256;
pub use agents::{
    agent_dispatch, approval_review_service, attempt_snapshot, chat_activity, chat_ontology,
    chat_service, chat_session_file_service, codex_auth, config_service, provider_account_service,
    task_lifecycle, task_service, workbench_config, workspace_messages,
};
#[cfg(feature = "test-helpers")]
pub use git::git_test_barrier;
pub use git::{
    advance_consumer, branch_commit, branch_integration_service, branch_sync_runs_service,
    coordinated_repo_publication, external_issues, git_conflict_resolution, git_policy, github_app,
    github_app_binding, github_issues, github_user_grants, issue_comments, issue_creations,
    issue_labels, native_issues, remote_integrations, remote_tracking_merge,
    source_control_change_graph_service, unified_diff, workspace_flush_service,
    workspace_merge_service, workspace_ops, workspace_pull_request_auto_merge,
    workspace_pull_request_create, workspace_pull_request_freshness,
    workspace_pull_request_provider_lifecycle, workspace_reader, workspace_repositories,
};
pub use hosts::{
    host_control_state, runtime_label_resolver, worker_host_service, workspace_vm_release,
};
#[allow(unused_imports)]
pub(crate) use identity::github_app_auth;
pub use identity::{
    api_key_service, auth_service, notification_service, org_admin_settings, org_service,
    permission_service, platform_issue_github, platform_issue_intake, platform_operator,
    provider_quota_service, self_hosted_license_service, spend_service, user_service,
    workspace_incarnation, workspace_mutation_policy, workspace_names,
};
pub use workspace::{
    approval_activity, approval_config_service, approval_definition_service,
    approval_request_email, base_reverification, comment_service, doc_reference_service,
    doc_service, home_service, intent_log_service, personal_workspace, preview_config,
    question_set_service, question_vote_service, review_edits, review_source_notifications,
    workspace_activity, workspace_file_save, workspace_fork_service, workspace_kind,
    workspace_owner_email, workspace_review_service, workspace_runtime, workspace_service,
    workspace_vm_service,
};

// ---------------------------------------------------------------------------
// Re-exports — allow consumers to import via `crate::services::*`
// ---------------------------------------------------------------------------

pub use api_key_service::ApiKeyService;
pub use approval_activity::ApprovalActivityService;
pub use approval_config_service::ApprovalConfigService;
pub use approval_definition_service::ApprovalDefinitionService;
pub use approval_review_service::ApprovalReviewService;
pub use auth_service::AuthService;
pub use chat_service::ChatService;
pub use chat_session_file_service::ChatSessionFileService;
pub use comment_service::CommentService;
pub use config_service::{
    AgentLaunchSettingsPreview, AgentLaunchSettingsPreviewScope, AssignmentWriteScope,
    ConfigService, EffectiveConfig, ScopeConfigView, ScopeUpdateInput,
};
pub use coordinated_repo_publication::CoordinatedRepoPublicationService;
pub use doc_service::DocService;
pub use email::Email;
pub use home_service::HomeService;
pub use intent_log_service::{
    IntentLogEntry, IntentLogMeta, IntentLogProjection, IntentLogQuery, IntentLogResponse,
    IntentLogService, ProvenanceBackfill,
};
pub use notification_service::NotificationService;
pub use org_service::OrgService;
pub use permission_service::PermissionService;
pub use platform_issue_github::{PlatformIssueGithubClient, PlatformIssueGithubConfig};
pub use platform_issue_intake::{
    PlatformIssueIntakeService, PlatformIssueReport, PlatformIssueScreenshot,
};
pub use provider_account_service::{
    CreateProviderAccount, ProviderAccountService, ProviderAccountView, ResolvedAccountCredential,
    UpdateProviderAccount,
};
pub use provider_quota_service::ProviderQuotaService;
pub use runtime_label_resolver::{OciRuntimeLabelResolver, RuntimeLabelResolver};
pub use self_hosted_license_service::SelfHostedLicenseService;
pub use source_control_change_graph_service::{
    ChangeGraphComparison, ChangeGraphNodeDetailsResponse, ChangeGraphResponse,
    RebaseChangeGraphNodeDetailsResponse, RebaseChangeGraphResponse,
    SourceControlChangeGraphService,
};
pub use spend_service::SpendService;
pub use task_service::TaskService;
pub use user_service::UserService;
pub use worker_host_service::WorkerHostService;
pub use workspace_fork_service::WorkspaceForkService;
pub use workspace_merge_service::WorkspaceMergeService;
pub use workspace_pull_request_provider_lifecycle::WorkspacePullRequestProviderLifecycleService;
pub use workspace_runtime::WorkspaceRuntime;
pub use workspace_service::{WorkspaceRepoContext, WorkspaceService};
pub use workspace_vm_service::{WorkspaceVmService, WorkspaceVmServiceConfig};

// ---------------------------------------------------------------------------
// EventBus — event publishing abstraction
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct EventBus {
    ws_hub: Arc<WsHub>,
    branch_updates: tokio::sync::broadcast::Sender<WsEvent>,
}

impl EventBus {
    /// Create a new `EventBus` wrapping the given [`WsHub`].
    #[must_use]
    pub fn new(ws_hub: Arc<WsHub>) -> Self {
        let (branch_updates, _) = tokio::sync::broadcast::channel(256);
        Self {
            ws_hub,
            branch_updates,
        }
    }

    /// Publish an event to a specific channel.
    ///
    /// This fans out the event to all WebSocket clients subscribed to the
    /// channel. The call is non-blocking — it delegates to `WsHub::publish`
    /// which uses `try_send` internally.
    pub fn publish(&self, channel: &Channel, event: &WsEvent) {
        self.ws_hub.publish(channel, event);
        self.publish_branch_hint(event);
    }

    /// Publish an owner-only live event without widening the subscription
    /// authority of the containing workspace channel.
    pub fn publish_to_user(&self, channel: &Channel, user_id: Uuid, event: &WsEvent) {
        self.ws_hub.publish_to_user(channel, user_id, event);
        self.publish_branch_hint(event);
    }

    pub fn publish_to_many(&self, channels: &[Channel], event: &WsEvent) {
        self.publish_branch_hint(event);
        for channel in channels {
            self.ws_hub.publish(channel, event);
        }
    }

    fn publish_branch_hint(&self, event: &WsEvent) {
        if matches!(event, WsEvent::WorkspaceBranchUpdated { .. }) {
            let _ = self.branch_updates.send(event.clone());
        }
    }

    /// Subscribe only to branch-update hints. Consumers recover from lag by scanning authority.
    pub fn subscribe_branch_updates(&self) -> tokio::sync::broadcast::Receiver<WsEvent> {
        self.branch_updates.subscribe()
    }

    /// Returns a reference to the underlying `WsHub`.
    ///
    /// Used by the WebSocket route adapter in `routes/mod.rs` to pass the hub
    /// to the per-connection handler.
    #[must_use]
    pub fn ws_hub(&self) -> &Arc<WsHub> {
        &self.ws_hub
    }
}

// ---------------------------------------------------------------------------
// PaginationParams — query string extraction for list endpoints
// ---------------------------------------------------------------------------

/// Default page number (1-based).
fn default_page() -> i64 {
    1
}

/// Default page size.
fn default_per_page() -> i64 {
    50
}

/// Pagination parameters extracted from query strings.
///
/// Used by all list endpoints across all services for consistent pagination
/// behavior. The `perPage` field is deserialized from camelCase to match
/// the frontend API convention.
///
/// # Clamping
///
/// - `page` is clamped to a minimum of 1.
/// - `per_page` is clamped to the range `[1, 100]`.
#[derive(Debug, Clone, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page", rename = "perPage")]
    pub per_page: i64,
}

impl PaginationParams {
    /// Returns the SQL `OFFSET` for this page.
    ///
    /// Computed as `(page - 1) * limit` where both `page` and `limit` are
    /// clamped to valid ranges.
    #[must_use]
    pub fn offset(&self) -> i64 {
        (self.page.max(1) - 1) * self.per_page.clamp(1, 100)
    }

    /// Returns the SQL `LIMIT` for this page, clamped to `[1, 100]`.
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.per_page.clamp(1, 100)
    }
}

// ---------------------------------------------------------------------------
// PaginatedResult — response envelope for list endpoints
// ---------------------------------------------------------------------------

/// Paginated response wrapper used by all list endpoints.
///
/// The `per_page` field is serialized as `perPage` (camelCase) to match
/// the frontend API convention.
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResult<T: Serialize> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    #[serde(rename = "perPage")]
    pub per_page: i64,
}

impl<T: Serialize> PaginatedResult<T> {
    /// Construct a new paginated result from the data, total count, and
    /// the pagination parameters used for the query.
    ///
    /// `page` and `per_page` are clamped to valid ranges for consistency.
    #[must_use]
    pub fn new(data: Vec<T>, total: i64, params: &PaginationParams) -> Self {
        Self {
            data,
            total,
            page: params.page.max(1),
            per_page: params.per_page.clamp(1, 100),
        }
    }
}

// ---------------------------------------------------------------------------
// AppState — shared application state
// ---------------------------------------------------------------------------

/// Central shared state for the G5 API application.
///
/// Constructed once during server startup in `main.rs` and shared across
/// all axum route handlers via `State<Arc<AppState>>`.
///
/// # Fields
///
/// - Each `*_service` field holds an instance of the corresponding service
///   struct. Services are constructed with a `PgPool` (and optionally other
///   dependencies) during startup.
/// - `event_bus` — The event publishing abstraction. Services call
///   `event_bus.publish(channel, event)` to push events to WebSocket clients.
/// - `system_pool` — The privileged `PgPool` for system-level queries
///   (health checks, maintenance, schema migrations). Route handlers
///   should use `ScopedDb` (via `tenant_pool`) for tenant-scoped work.
pub struct AppState {
    pub workspace_service: WorkspaceService,
    pub api_key_service: ApiKeyService,
    pub workspace_merge_service: WorkspaceMergeService,
    pub coordinated_repo_publication_service: CoordinatedRepoPublicationService,
    pub workspace_pull_request_provider_lifecycle_service:
        WorkspacePullRequestProviderLifecycleService,
    pub task_service: TaskService,
    pub chat_service: ChatService,
    pub intent_log_service: IntentLogService,
    pub source_control_change_graph_service: SourceControlChangeGraphService,
    pub comment_service: CommentService,
    pub notification_service: NotificationService,
    pub auth_service: AuthService,
    pub approval_config_service: ApprovalConfigService,
    pub approval_activity_service: ApprovalActivityService,
    pub approval_definition_service: ApprovalDefinitionService,
    pub approval_review_service: ApprovalReviewService,
    pub org_service: OrgService,
    /// Outbound transactional email (`software_architecture/email.g5.md`).
    pub email: Email,
    pub user_service: UserService,
    pub permission_service: PermissionService,
    pub spend_service: SpendService,
    pub provider_quota_service: ProviderQuotaService,
    pub provider_account_service: ProviderAccountService,
    pub workspace_fork_service: WorkspaceForkService,
    /// Sends chat messages to workspaces for their agents to work on.
    pub workspace_messenger: workspace_messages::WorkspaceMessenger,
    pub config_service: ConfigService,
    pub doc_service: DocService,
    pub home_service: HomeService,
    pub worker_host_service: WorkerHostService,
    pub workspace_runtime: Arc<WorkspaceRuntime>,
    /// Agents work the workspace and host lifecycles settle: the task service
    /// behind the hosts and workspace seams.
    pub agent_work: Arc<dyn workspace_vm_service::WorkspaceAgentWork>,
    /// Starts the agent task that resolves a Git conflict.
    pub conflict_resolution: Arc<dyn git_conflict_resolution::ConflictResolutionLauncher>,
    pub self_hosted_license_service: SelfHostedLicenseService,
    pub chat_session_file_service: ChatSessionFileService,
    pub event_bus: EventBus,
    /// In-memory per-host preview reverse-tunnel registry (PR4 proxy carrier 4b).
    /// Keyed by `worker_hosts.host_id`; populated when a host dials the outbound
    /// tunnel-attach WebSocket, read by the browser-facing preview proxy route.
    pub preview_tunnels: Arc<crate::ws::preview_tunnel::TunnelRegistry>,
    /// Wake-ups from hosts' `vm.module_events` hints to the attempt followers
    /// that stream a live attempt's journal (`g5.chat.live-delivery`).
    pub live_hints: Arc<hosts::live_hints::LiveHints>,
    /// The attempts this process streams live and stores in batches
    /// (`g5.chat.live-delivery`).
    pub followers: Arc<agents::attempt_follower::FollowerRegistry>,
    /// Privileged pool for system-level queries (health checks,
    /// maintenance, schema migrations).  Route handlers should
    /// prefer `tenant_pool` via `ScopedDb`.
    pub system_pool: PgPool,
    /// Tenant-restricted pool for ScopedDb (RLS-scoped queries).
    ///
    /// In production, this connects with a restricted Postgres role
    /// (`g5_tenant`) that cannot modify `authz.*` tables or disable RLS.
    /// In dev mode (no `G5_TENANT_DATABASE_URL`), this is a clone of `db`.
    pub tenant_pool: PgPool,
    /// Authorization service — org membership checks and capability minting.
    pub authz: Arc<dyn g5_authz::AuthzService>,
    /// Repository storage, reached only through its tenancy guard
    /// (`g5.repo.storage-tenancy`).
    pub repo_storage: crate::services::git::repo_store::TenantRepoStorage,
    /// G5 issue attachments' bytes (`g5.issue.native.attachment-store`).
    pub issue_attachments: native_issues::IssueAttachmentStore,
    /// Public API base URL used for workspace manifest materialization metadata.
    pub external_url: String,
    /// Public git smart-HTTP base URL used for repo mirror URLs.
    pub git_external_url: String,
    pub preview_host_suffix: Option<String>,
    /// Browser origins allowed to exchange the `g5_refresh` cookie for
    /// tokens (`g5.security.login.refresh-exchange`): the canonical platform
    /// origin (`G5_EXTERNAL_URL`) plus the CORS allow-list, each as an ASCII
    /// origin serialization. Never derived from request headers.
    pub trusted_browser_origins: Vec<String>,
    /// Process-local consumed-`jti` set for single-use preview handoff
    /// tickets (`middleware/preview_host` [D2]). Sound process-local because
    /// the preview data path is pinned to the carrier-holding instance
    /// (`routes/preview` [D7]) and hosted envs run a single API replica.
    pub preview_handoff_jtis: Arc<crate::middleware::preview_host::ConsumedJtiStore>,
    /// Customer self-hosted deployment mode. When true, G5-hosted delivery and
    /// strict cloud-provider lifecycle surfaces fail closed.
    pub is_self_hosted: bool,
    /// Product-level cloud workflow gate.
    pub cloud_features_enabled: bool,
    /// Make-Remote local/bare git remote allow-list relaxation, sourced from
    /// `AppConfig.allow_local_git_remotes` (env `G5_ALLOW_LOCAL_GIT_REMOTES`,
    /// default off / fail-closed). Read by the promote-remote-tracking route and
    /// passed into the Adopt-Remote scheme check so `file://`/`ext::`/bare-path
    /// remotes are permitted ONLY on deployments that opt in (workspace_service
    /// D46 item 1). Threaded through config → AppState, never re-read at
    /// request time.
    pub allow_local_git_remotes: bool,
    /// Platform bug-reporting gate (independent of issue tracking).
    pub platform_issue_reporting_enabled: bool,
    /// Platform bug-report GitHub-issue intake service. `Some` only when the
    /// feature is enabled and the GitHub delivery config is fully present; its
    /// shared quota lives in Postgres (`rate_limit_windows`). `None` means the
    /// route and feature fail closed.
    pub platform_issue_intake: Option<PlatformIssueIntakeService>,
    /// G5-owned GitHub App client for organization installation bindings.
    /// `Some` only when the complete App configuration resolved at startup;
    /// `None` means the binding routes fail closed and the feature is not
    /// advertised.
    pub github_app: Option<Arc<github_app::GithubAppClient>>,
    pub repository_credentials: Arc<crate::auth::RepositoryCredentials>,
    /// Optional JWT validator for the WebSocket upgrade handler.
    ///
    /// `None` when running in CLI/non-server contexts. When present,
    /// `build_router()` mounts the `/api/v1/ws` WebSocket endpoint.
    pub jwt_validator: Option<Arc<JwtValidator>>,
    /// JWT minter for creating worker session tokens.
    ///
    /// `None` when `G5_JWT_PRIVATE_KEY` is not set (dev/E2E mode).
    pub jwt_minter: Option<Arc<JwtMinter>>,
    /// Per-host `host_jwt` lifetime, resolved once at `AppState` construction
    /// from `G5_HOST_JWT_TTL_SECS` (default 24h, floored at 60s per
    /// [`worker_hosts.semlift.md`] [D10]/[I4]). Read by the
    /// `POST /api/v1/orgs/:orgId/worker-hosts` handler on every request — no
    /// per-request env work and no mid-process drift.
    pub host_jwt_ttl: Duration,
    /// SCM token manager — decrypts stored SCM tokens for git operations.
    /// `None` when `G5_SCM_ENCRYPTION_KEY` is not set.
    pub scm_token_manager: Option<Arc<crate::auth::ScmTokenManager>>,
    /// Path to the `git` binary, resolved once at startup.
    pub git_bin: std::path::PathBuf,
    /// Cloud backend for worker-host bring-up/teardown (PR-rev-7-9c).
    ///
    /// `None` when no cloud is wired (dev without GCP, and the default in
    /// `new_for_test*`), which preserves the PR-rev-7-9b DB-only provisioning
    /// contract bit-for-bit — POST/DELETE make no cloud call and the
    /// `vpc_id`/`cloud_instance_id` columns stay NULL. `Some` in production when
    /// GCP is configured (`GcpCloudProvider::from_env`), or when a test injects
    /// a `MockCloudProvider`. See `worker_hosts.semlift.md` [D24].
    pub cloud_provider: Option<Arc<dyn CloudProvider>>,
    /// Resolver for worker-host runtime labels (PR2, ontology [D43]).
    ///
    /// Always set: production defaults to `OciRuntimeLabelResolver::https()`,
    /// and `new_for_test_with_pools` defaults to the same `https()` resolver.
    /// Integration tests that exercise the refresh endpoint override
    /// `state.runtime_label_resolver` after build with a wiremock-pointed
    /// `for_tests` resolver (mirrors the `cloud_provider` injection pattern).
    pub runtime_label_resolver: Arc<dyn RuntimeLabelResolver>,
}

/// The browser origins trusted to exchange the `g5_refresh` cookie: the
/// platform origin and every CORS-allowed origin, canonicalized the same way
/// a browser serializes `Origin`. Entries that are not plain origins are
/// skipped with a warning, matching the CORS layer.
fn trusted_browser_origins(external_url: &str, cors_allowed_origins: &[String]) -> Vec<String> {
    let mut origins = Vec::new();
    for raw in std::iter::once(external_url).chain(cors_allowed_origins.iter().map(String::as_str))
    {
        match crate::middleware::canonical_main_origin(raw) {
            Ok(origin) => {
                if !origins.contains(&origin) {
                    origins.push(origin);
                }
            }
            Err(e) => tracing::warn!(origin = raw, "Skipping untrusted browser origin: {e}"),
        }
    }
    origins
}

impl AppState {
    /// Install the PostgreSQL authority for host control after startup has
    /// resolved the optional JWT signing key.
    pub fn install_host_control_backend(&self) -> bool {
        let Some(minter) = self.jwt_minter.clone() else {
            return false;
        };
        self.preview_tunnels.set_control_backend(Arc::new(
            host_control_state::PostgresHostControlState::new(
                self.system_pool.clone(),
                self.tenant_pool.clone(),
                minter,
                self.event_bus.clone(),
                self.authz.clone(),
                self.agent_work.clone(),
            )
            .with_source_preparation(self.repo_storage.clone(), self.git_external_url.clone())
            .with_live_hints(self.live_hints.clone()),
        ));
        true
    }

    /// The single startup-resolved platform-report capability predicate.
    ///
    /// `platform_issue_intake` is constructed only after the enabled feature
    /// flag and complete GitHub delivery configuration have been resolved. Keeping this predicate on state ensures instance
    /// feature advertisement and request admission cannot drift.
    #[must_use]
    pub fn platform_issue_reporting_available(&self) -> bool {
        self.platform_issue_reporting_enabled && self.platform_issue_intake.is_some()
    }

    /// Construct a new `AppState` by creating all service instances.
    ///
    /// This is the sole construction path. All service constructors are
    /// synchronous — no async work happens here.
    ///
    /// # Service constructor patterns
    ///
    /// - All services receive a `PgPool` clone as their primary dependency.
    /// - Services that emit WebSocket events (task, chat, draft, comment,
    ///   spend) also receive an `EventBus` clone.
    /// - `AuthService` receives `AuthConfig` for JWT/OAuth configuration.
    pub fn new(
        db: PgPool,
        tenant_pool: PgPool,
        config: &AppConfig,
        ws_hub: Arc<WsHub>,
        authz: Arc<dyn g5_authz::AuthzService>,
        repo_storage: crate::services::git::repo_store::TenantRepoStorage,
    ) -> Self {
        let event_bus = EventBus::new(ws_hub);
        // Startup validation already rejected an incomplete configuration.
        let email = Email::from_config(&config.email, &config.server.external_url)
            .unwrap_or_else(|message| panic!("outbound email: {message}"));

        let scm_github_api_base_url = config
            .github_app
            .as_ref()
            .map(|cfg| cfg.api_base_url.clone())
            .unwrap_or_else(|| config.auth.scm_github_api_base_url.clone());
        let scm_github_web_base_url = config
            .github_app
            .as_ref()
            .map(|cfg| cfg.web_base_url.clone())
            .unwrap_or_else(|| config.auth.scm_github_web_base_url.clone());
        let scm_origin_policy = crate::auth::configured_scm_origin_policy(&config.auth)
            .with_github_web_url(&scm_github_web_base_url);
        // Every new organization gets its native issue domain here, where
        // Identity and Git are composed (issues g5.issue.native.domain).
        let org_created_hook: Arc<dyn org_service::OrganizationCreated> =
            Arc::new(NativeIssueDomainOnCreate);
        let auth_service = AuthService::new(
            db.clone(),
            tenant_pool.clone(),
            authz.clone(),
            config.auth.clone(),
            &config.server.external_url,
        )
        .with_repository_integration(scm_origin_policy.clone(), scm_github_api_base_url.clone())
        .with_org_created_hook(org_created_hook.clone());
        if auth_service.cookie_secure() {
            // Protected cookies share Path=/. Check their owners' actual names
            // here, where Identity and Git are composed, before serving requests.
            use crate::auth::oauth_attempt::OAuthFlow;
            assert!(
                ![
                    github_app::pkce_cookie_name(true),
                    OAuthFlow::Login.cookie_name(true),
                    OAuthFlow::ScmConnect.cookie_name(true),
                ]
                .contains(&auth_service.refresh_cookie_name()),
                "G5_REFRESH_COOKIE_NAME collides with an authorization-attempt cookie in secure mode: {}",
                auth_service.refresh_cookie_name()
            );
        }
        if scm_origin_policy
            .effective_provider(Some("gitlab"), config.auth.scm_gitlab_api_url())
            .is_none()
        {
            tracing::warn!(target: SERVICES_LOG_TARGET, "Configured GitLab SCM API URL has no supported HTTPS origin; GitLab Git credentials are disabled");
        }

        // Build ScmTokenManager if the encryption key is configured.
        let scm_token_manager = config
            .auth
            .scm_encryption_key
            .as_deref()
            .and_then(|hex_key| {
                let bytes = hex::decode(hex_key).ok()?;
                if bytes.len() != 32 {
                    tracing::warn!(target: SERVICES_LOG_TARGET,
                        key_len = bytes.len(),
                        "SCM encryption key is not 32 bytes — SCM token decryption disabled"
                    );
                    return None;
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                Some(Arc::new(
                    crate::auth::ScmTokenManager::new(
                        key,
                        g5_scm::providers::ProviderClients::new(),
                    )
                    .with_origin_policy(scm_origin_policy.clone())
                    .with_gitlab_refresh(
                        AuthService::gitlab_connection_refresh(
                            &config.auth,
                            &config.server.external_url,
                        ),
                    ),
                ))
            });

        let source_control_change_graph_service =
            SourceControlChangeGraphService::new(repo_storage.clone());

        let config_service = ConfigService::new(scm_token_manager.clone());

        // Platform bug-report intake: build the GitHub delivery client + durable
        // intake service only when the feature is enabled AND fully configured
        // (fail-closed). `None` means the route and feature advertise off.
        let platform_issue_intake = match PlatformIssueGithubClient::provider_clients() {
            Ok(http) => {
                PlatformIssueGithubClient::from_platform_config(http, &config.platform_issue).map(
                    |github| {
                        let rate_limits: Arc<dyn RateLimitStore> =
                            Arc::new(PostgresRateLimitStore::new(db.clone()));
                        PlatformIssueIntakeService::new(db.clone(), github, Some(rate_limits))
                    },
                )
            }
            Err(error) => {
                tracing::error!(target: SERVICES_LOG_TARGET, %error, "platform issue reporting HTTP client initialization failed");
                None
            }
        };

        let github_app = config.github_app.clone().and_then(|github_app_config| {
            match github_app::GithubAppClient::provider_clients() {
                Ok(http) => Some(Arc::new(github_app::GithubAppClient::new(
                    http,
                    github_app_config,
                ))),
                Err(error) => {
                    tracing::error!(target: SERVICES_LOG_TARGET, %error, "GitHub App HTTP client initialization failed");
                    None
                }
            }
        });

        let repository_credentials = Arc::new(
            crate::auth::RepositoryCredentials::new(
                db.clone(),
                tenant_pool.clone(),
                authz.clone(),
                scm_token_manager.clone(),
                github_app.clone(),
            )
            .with_origin_policy(scm_origin_policy.clone()),
        );

        let preview_tunnels = Arc::new(crate::ws::preview_tunnel::TunnelRegistry::new());
        let live_hints = Arc::new(hosts::live_hints::LiveHints::default());
        let followers = Arc::new(agents::attempt_follower::FollowerRegistry::for_pool(
            db.options().get_max_connections(),
        ));
        let workspace_runtime = Arc::new(WorkspaceRuntime::new(
            db.clone(),
            preview_tunnels.clone(),
            repo_storage.clone(),
        ));
        let task_service = TaskService::new(
            event_bus.clone(),
            repo_storage.clone(),
            db.clone(),
            config_service.clone(),
            scm_token_manager.clone(),
            workspace_runtime.clone(),
            config.server.git_external_url.clone(),
        )
        .with_followers(followers.clone());
        let messenger = workspace_messages::WorkspaceMessenger::new(
            ChatService::new(event_bus.clone(), config.chat.response_timeout),
            task_service.clone(),
            config_service.clone(),
            ProviderAccountService::new(db.clone(), config_service.clone()),
        );
        // Lower contexts reach later ones only through these seams.
        let merge_admission: Arc<dyn workspace_merge_service::MergeAdmissionGate> =
            Arc::new(approval_config_service::ApprovalMergeAdmission {
                policy: config.approval_policy,
            });
        let agent_work: Arc<dyn workspace_vm_service::WorkspaceAgentWork> =
            Arc::new(task_service.clone());
        let conflict_resolution: Arc<dyn git_conflict_resolution::ConflictResolutionLauncher> =
            Arc::new(messenger.clone());
        Self {
            workspace_service: WorkspaceService::new(
                repo_storage.clone(),
                db.clone(),
                tenant_pool.clone(),
                authz.clone(),
                repository_credentials.clone(),
            )
            .with_approval_policy(config.approval_policy),
            workspace_merge_service: WorkspaceMergeService::new(
                repo_storage.clone(),
                merge_admission.clone(),
            ),
            coordinated_repo_publication_service: CoordinatedRepoPublicationService::new(
                merge_admission.clone(),
                Arc::new(git::pull_request_adapters::PullRequestAdapters::new(
                    scm_origin_policy.clone(),
                    &scm_github_api_base_url,
                    &scm_github_web_base_url,
                )),
                db.clone(),
            ),
            workspace_pull_request_provider_lifecycle_service:
                WorkspacePullRequestProviderLifecycleService::new(
                    db.clone(),
                    repo_storage.clone(),
                    scm_github_api_base_url.clone(),
                    scm_github_web_base_url,
                    config.server.external_url.clone(),
                    repository_credentials.clone(),
                    merge_admission,
                ),
            task_service: task_service.clone(),
            chat_service: ChatService::new(event_bus.clone(), config.chat.response_timeout),
            comment_service: CommentService::new(
                event_bus.clone(),
                repo_storage.clone(),
                db.clone(),
            ),
            notification_service: NotificationService::new(event_bus.clone(), db.clone()),
            intent_log_service: IntentLogService::new(repo_storage.clone()),
            source_control_change_graph_service,
            auth_service,
            approval_config_service: ApprovalConfigService::new(
                repo_storage.clone(),
                config.approval_policy,
            ),
            approval_activity_service: ApprovalActivityService,
            approval_definition_service: ApprovalDefinitionService::new(),
            approval_review_service: ApprovalReviewService::with_policy(config.approval_policy),
            org_service: OrgService::new()
                .with_email(email.clone())
                .with_membership_retirement(Arc::new(PersonalWorkspaceRetirement))
                .with_created_hook(org_created_hook),
            email,
            user_service: UserService,
            permission_service: PermissionService,
            spend_service: SpendService::new(event_bus.clone()),
            provider_quota_service: ProviderQuotaService::new(),
            provider_account_service: ProviderAccountService::new(
                db.clone(),
                config_service.clone(),
            ),
            workspace_fork_service: WorkspaceForkService::new(
                db.clone(),
                repo_storage.clone(),
                repository_credentials.clone(),
                workspace_runtime.clone(),
                Arc::new(messenger.clone()),
            )
            .with_github_app(github_app.clone())
            .with_issue_site(&config.server.external_url),
            workspace_messenger: messenger,
            config_service,
            doc_service: DocService::new_with_branch_reconciliation(
                repo_storage.clone(),
                db.clone(),
                event_bus.clone(),
            ),
            home_service: HomeService::new(),
            worker_host_service: WorkerHostService::new(),
            workspace_runtime,
            agent_work,
            conflict_resolution,
            self_hosted_license_service: SelfHostedLicenseService::new_with_issuer(
                db.clone(),
                config.self_hosted.clone(),
                crate::cloud::gcp::registry_token_issuer_from_env(),
            ),
            chat_session_file_service: ChatSessionFileService::new(),
            api_key_service: ApiKeyService::new(),
            event_bus,
            preview_tunnels,
            live_hints,
            followers,
            system_pool: db,
            tenant_pool,
            authz,
            repo_storage,
            issue_attachments: native_issues::IssueAttachmentStore::new(
                config.attachment_store_root().map(|root| {
                    let store = g5_storage::DirectoryAttachmentStore::new(root)
                        .unwrap_or_else(|error| panic!("attachment store: {error}"));
                    Arc::new(store) as Arc<dyn g5_storage::AttachmentStore>
                }),
            ),
            external_url: config.server.external_url.clone(),
            git_external_url: config.server.git_external_url.clone(),
            preview_host_suffix: config.server.preview_host_suffix.clone(),
            trusted_browser_origins: trusted_browser_origins(
                &config.server.external_url,
                &config.cors.allowed_origins,
            ),
            preview_handoff_jtis: Arc::new(crate::middleware::preview_host::ConsumedJtiStore::new()),
            is_self_hosted: config.is_self_hosted,
            cloud_features_enabled: config.enable_cloud_features,
            allow_local_git_remotes: config.allow_local_git_remotes,
            platform_issue_reporting_enabled: config.platform_issue.enabled,
            platform_issue_intake,
            github_app,
            repository_credentials,
            jwt_validator: None,
            jwt_minter: None,
            host_jwt_ttl: resolve_host_jwt_ttl(),
            scm_token_manager,
            git_bin: crate::git_runtime::api_git_binary().to_path_buf(),
            // Production cloud backend: GCP when configured, else None (cloud
            // disabled, DB-only provisioning). Construction never panics and
            // never blocks startup on GCP reachability (ontology [D24]).
            cloud_provider: crate::cloud::gcp::GcpCloudProvider::from_env()
                .map(|p| Arc::new(p) as Arc<dyn CloudProvider>),
            // Always-present runtime-label resolver (https in prod/test default;
            // tests override after build, ontology [D43]).
            runtime_label_resolver: Arc::new(
                runtime_label_resolver::OciRuntimeLabelResolver::https(),
            ),
        }
    }

    /// Construct an `AppState` suitable for integration tests.
    ///
    /// Creates a default `WsHub` internally so callers don't need to supply
    /// one. The method is async for future flexibility (e.g. seeding data).
    pub async fn new_for_test(db: PgPool, config: &AppConfig) -> Self {
        Self::new_for_test_with_pools(db.clone(), db.clone(), db, config).await
    }

    /// Construct an `AppState` suitable for integration tests using explicit
    /// system/authz/tenant pools.
    pub async fn new_for_test_with_pools(
        system_pool: PgPool,
        authz_pool: PgPool,
        tenant_pool: PgPool,
        config: &AppConfig,
    ) -> Self {
        let ws_hub = Arc::new(crate::ws::WsHub::from_app_config(config));
        // Use real in-process authz in tests so ScopedDb session minting and
        // authz.set_ctx() behave exactly like production. StubAuthz returns
        // synthetic session IDs that do not exist in authz.user_sessions and
        // causes widespread 401s once handlers rely on ScopedDb.
        let authz: Arc<dyn g5_authz::AuthzService> = Arc::new(g5_authz::InProcessAuthz::new(
            system_pool.clone(),
            authz_pool,
        ));
        let repo_storage =
            crate::services::git::repo_store::TenantRepoStorage::from_config(&config.storage)
                .expect("repo storage must build — local storage creates its base automatically");
        let mut state = Self::new(
            system_pool.clone(),
            tenant_pool,
            config,
            ws_hub,
            authz,
            repo_storage,
        );
        // Tests are offline: never let an ambient GCP credential in CI wire a
        // live cloud backend. Tests that exercise cloud wire-in inject a
        // `MockCloudProvider` explicitly (ontology [D24]).
        state.cloud_provider = None;
        state.self_hosted_license_service = SelfHostedLicenseService::new_with_issuer(
            system_pool,
            config.self_hosted.clone(),
            None,
        );
        state
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Every tenant organization tracks issues with G5 issues from the start
/// (issues `g5.issue.native.domain`): the Git context's native domain, keyed
/// from the slug and trusted, added to each new organization inside its
/// creation transaction. Installed here, where Identity and Git are
/// composed; the operator organization gets none.
struct NativeIssueDomainOnCreate;

/// Compose Identity's complete membership transition with Workspace's
/// retirement owner while retaining the caller's single transaction.
struct PersonalWorkspaceRetirement;

#[async_trait::async_trait]
impl org_service::MembershipRetirement for PersonalWorkspaceRetirement {
    async fn retire_personal_workspace(
        &self,
        conn: &mut sqlx::PgConnection,
        org_id: uuid::Uuid,
        user_id: uuid::Uuid,
        actor_id: uuid::Uuid,
    ) -> Result<Option<org_service::RetiredPersonalWorkspace>, g5_contracts::G5Error> {
        personal_workspace::retire_in_tx(conn, org_id, user_id, actor_id).await
    }
}

#[async_trait::async_trait]
impl org_service::OrganizationCreated for NativeIssueDomainOnCreate {
    async fn organization_created(
        &self,
        db: &mut g5_authz::ScopedDb,
        org: &org_service::CreatedOrganization,
    ) -> Result<(), g5_contracts::G5Error> {
        if !matches!(org.kind, org_service::OrgKind::Tenant) {
            return Ok(());
        }
        native_issues::create_native_domain(
            db,
            org.org_id,
            org.owner_user_id,
            &native_issues::default_issue_key(&org.slug),
            true,
        )
        .await?;
        Ok(())
    }
}

/// Every email purpose kind's validity check, owned by its consumer and
/// installed here, the composition root (`g5.email.consumers`).
#[must_use]
pub fn email_gates() -> email::EmailGates {
    email::EmailGates {
        org_invitation: Arc::new(org_service::InvitationDeliveryGate),
        workspace_owner_message: Arc::new(workspace_owner_email::WorkspaceOwnerEmailGate),
        approval_request: Arc::new(approval_request_email::ApprovalEmailGate),
        approval_decision: Arc::new(approval_request_email::ApprovalEmailGate),
    }
}

/// Env var name for the `host_jwt` lifetime override (see
/// [`worker_hosts.semlift.md`] [D10]/[I4]). Resolved once at `AppState`
/// construction so per-request handlers never touch the environment.
pub const HOST_JWT_TTL_ENV: &str = "G5_HOST_JWT_TTL_SECS";

/// Default `host_jwt` lifetime: 24 hours. Matches `worker_hosts.semlift.md`
/// [D10]; V1 mints long-lived tokens and rotates before expiry.
pub const DEFAULT_HOST_JWT_TTL_SECS: u64 = 24 * 60 * 60;

/// Floor for the minted lifetime, matching `JwtMinter::MIN_HOST_JWT_LIFETIME_SECS`
/// in `g5-common`. Below this the minter rejects the token (replay-via-clock-skew
/// defense — see [I4]).
pub const MIN_HOST_JWT_TTL_SECS: u64 = 60;

/// Resolve the per-process `host_jwt` lifetime from `G5_HOST_JWT_TTL_SECS`,
/// defaulting to 24h and flooring at 60s. Called exactly once during
/// `AppState::new` so the cached value travels with the request handler state
/// rather than a route-module static.
fn resolve_host_jwt_ttl() -> Duration {
    let secs = std::env::var(HOST_JWT_TTL_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|configured| configured.max(MIN_HOST_JWT_TTL_SECS))
        .unwrap_or(DEFAULT_HOST_JWT_TTL_SECS);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn branch_hint_buffer_ignores_other_events_and_hierarchical_fanout() {
        let bus = EventBus::new(Arc::new(WsHub::new(Default::default())));
        let workspace_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let channel = Channel::Workspace(workspace_id);
        let mut hints = bus.subscribe_branch_updates();
        let branch = WsEvent::WorkspaceBranchUpdated {
            workspace_id,
            repo_id: branch_id,
            branch: "main".into(),
            head_commit: "hint".into(),
            hint: None,
        };
        bus.publish(&channel, &branch);
        for _ in 0..300 {
            let other = WsEvent::DocContentChanged {
                workspace_id,
                doc_id: Uuid::new_v4().to_string(),
            };
            bus.publish(&channel, &other);
            bus.publish_to_many(&[channel.clone(), Channel::Task(Uuid::new_v4())], &other);
        }
        bus.publish_to_many(&[channel.clone(), Channel::Task(Uuid::new_v4())], &branch);
        for _ in 0..2 {
            assert!(matches!(
                hints.try_recv(),
                Ok(WsEvent::WorkspaceBranchUpdated { .. })
            ));
        }
        assert!(matches!(
            hints.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn test_pagination_defaults() {
        let json = "{}";
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 50);
    }

    #[test]
    fn test_pagination_custom_values() {
        let json = r#"{"page": 3, "perPage": 25}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.page, 3);
        assert_eq!(params.per_page, 25);
    }

    #[test]
    fn test_pagination_offset_and_limit() {
        let json = r#"{"page": 3, "perPage": 25}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.offset(), 50); // (3 - 1) * 25
        assert_eq!(params.limit(), 25);
    }

    #[test]
    fn test_pagination_offset_page_one() {
        let json = r#"{"page": 1, "perPage": 50}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.offset(), 0);
        assert_eq!(params.limit(), 50);
    }

    #[test]
    fn test_pagination_clamp_per_page_max() {
        let json = r#"{"page": 1, "perPage": 500}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.limit(), 100); // clamped to max 100
        assert_eq!(params.offset(), 0);
    }

    #[test]
    fn test_pagination_clamp_per_page_min() {
        let json = r#"{"page": 1, "perPage": 0}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.limit(), 1); // clamped to min 1
    }

    #[test]
    fn test_pagination_clamp_per_page_negative() {
        let json = r#"{"page": 1, "perPage": -5}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.limit(), 1); // clamped to min 1
    }

    #[test]
    fn test_pagination_clamp_page_zero() {
        let json = r#"{"page": 0, "perPage": 50}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.offset(), 0); // page clamped to 1, so (1-1)*50 = 0
    }

    #[test]
    fn test_pagination_clamp_page_negative() {
        let json = r#"{"page": -3, "perPage": 50}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.offset(), 0); // page clamped to 1
    }

    #[test]
    fn test_paginated_result_new() {
        let params = PaginationParams {
            page: 2,
            per_page: 10,
        };
        let result = PaginatedResult::new(vec!["a", "b", "c"], 25, &params);
        assert_eq!(result.data, vec!["a", "b", "c"]);
        assert_eq!(result.total, 25);
        assert_eq!(result.page, 2);
        assert_eq!(result.per_page, 10);
    }

    #[test]
    fn test_paginated_result_clamps_values() {
        let params = PaginationParams {
            page: 0,
            per_page: 200,
        };
        let result = PaginatedResult::<String>::new(vec![], 0, &params);
        assert_eq!(result.page, 1); // clamped from 0
        assert_eq!(result.per_page, 100); // clamped from 200
    }

    #[test]
    fn test_paginated_result_serialization() {
        let params = PaginationParams {
            page: 1,
            per_page: 50,
        };
        let result = PaginatedResult::new(vec![1, 2, 3], 3, &params);
        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["data"], serde_json::json!([1, 2, 3]));
        assert_eq!(json["total"], 3);
        assert_eq!(json["page"], 1);
        // Verify camelCase serialization
        assert_eq!(json["perPage"], 50);
        // Should NOT have snake_case key
        assert!(json.get("per_page").is_none());
    }

    #[test]
    fn test_event_bus_is_clone() {
        // EventBus wraps Arc<WsHub>, so cloning should be O(1).
        // We can't construct a real WsHub in unit tests without a full
        // config, but we can verify the Clone derive is present by
        // checking the type implements Clone at compile time.
        fn assert_clone<T: Clone>() {}
        assert_clone::<EventBus>();
    }

    #[test]
    fn test_pagination_offset_with_clamped_per_page() {
        // When per_page exceeds 100, offset should use clamped limit
        let params = PaginationParams {
            page: 3,
            per_page: 200,
        };
        // offset = (3-1) * min(200, 100).max(1) = 2 * 100 = 200
        assert_eq!(params.offset(), 200);
        assert_eq!(params.limit(), 100);
    }

    #[test]
    #[serial_test::serial]
    fn host_jwt_ttl_resolves_default_when_env_unset() {
        let prior = std::env::var(HOST_JWT_TTL_ENV).ok();
        std::env::remove_var(HOST_JWT_TTL_ENV);
        assert_eq!(
            resolve_host_jwt_ttl(),
            Duration::from_secs(DEFAULT_HOST_JWT_TTL_SECS)
        );
        if let Some(value) = prior {
            std::env::set_var(HOST_JWT_TTL_ENV, value);
        }
    }

    #[test]
    #[serial_test::serial]
    fn host_jwt_ttl_honours_env_override() {
        let prior = std::env::var(HOST_JWT_TTL_ENV).ok();
        std::env::set_var(HOST_JWT_TTL_ENV, "3600");
        assert_eq!(resolve_host_jwt_ttl(), Duration::from_secs(3600));
        match prior {
            Some(value) => std::env::set_var(HOST_JWT_TTL_ENV, value),
            None => std::env::remove_var(HOST_JWT_TTL_ENV),
        }
    }

    #[test]
    #[serial_test::serial]
    fn host_jwt_ttl_floors_below_minimum() {
        // An override below the 60s floor must clamp up, never below — the
        // minter rejects sub-60s tokens (replay-via-clock-skew defense).
        let prior = std::env::var(HOST_JWT_TTL_ENV).ok();
        std::env::set_var(HOST_JWT_TTL_ENV, "5");
        assert_eq!(
            resolve_host_jwt_ttl(),
            Duration::from_secs(MIN_HOST_JWT_TTL_SECS)
        );
        match prior {
            Some(value) => std::env::set_var(HOST_JWT_TTL_ENV, value),
            None => std::env::remove_var(HOST_JWT_TTL_ENV),
        }
    }
}
