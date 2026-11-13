use clap::Parser;
use std::time::Duration;

/// Canonicalize `G5_PREVIEW_HOST_SUFFIX` as a bare IDNA-ASCII domain.
///
/// The configuration parser and preview middleware share this pure helper so
/// startup normalization and same-site validation cannot drift.
pub fn canonical_preview_host_suffix(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("G5_PREVIEW_HOST_SUFFIX is empty".to_string());
    }
    match url::Host::parse(trimmed) {
        Ok(url::Host::Domain(domain)) => {
            if domain.split('.').any(str::is_empty) {
                return Err(format!(
                    "G5_PREVIEW_HOST_SUFFIX {trimmed:?} has an empty DNS label \
                     (leading/trailing/double dot)"
                ));
            }
            Ok(domain.to_ascii_lowercase())
        }
        Ok(other) => Err(format!(
            "G5_PREVIEW_HOST_SUFFIX must be a domain name, not an IP literal: {other}"
        )),
        Err(error) => Err(format!(
            "G5_PREVIEW_HOST_SUFFIX {trimmed:?} is not a bare domain \
             (no scheme, userinfo, port, or path allowed): {error}"
        )),
    }
}

// ---------------------------------------------------------------------------
// LogFormat enum
// ---------------------------------------------------------------------------

/// Log output format for tracing-subscriber configuration.
///
/// - `Pretty`: Human-readable colored logs for local development.
/// - `Json`: Structured JSON logs for production/cloud environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LogFormat {
    Pretty,
    Json,
}

/// Outbound email transport (`G5_EMAIL_TRANSPORT`), see
/// `.g5/ontology/software_architecture/email.g5.md#g5.email.provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum EmailTransportKind {
    /// No delivery requests are recorded and no worker runs.
    #[default]
    Disabled,
    /// Messages are kept in memory and logged instead of sent (tests, local
    /// development).
    Capture,
    /// Messages are sent through the Resend HTTP API.
    Resend,
}

/// Whether message kinds' per-organization volume limits apply
/// (`G5_EMAIL_VOLUME_LIMITS`, `g5.email.volume-limits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum EmailVolumeLimits {
    /// Every limited kind is capped per organization (the default).
    #[default]
    Enforce,
    /// No volume limits, for local development stacks whose mail cannot
    /// reach real recipients.
    Off,
}

// ---------------------------------------------------------------------------
// CliArgs — clap-derived argument parser
// ---------------------------------------------------------------------------

/// CLI arguments for the G5 API server.
///
/// Environment variable bindings generally use the `G5_` prefix. The
/// product-level `ENABLE_CLOUD_FEATURES` gate is intentionally unprefixed so
/// deployments can share one switch across API and UI surfaces. CLI arguments
/// override environment variables, which override hardcoded defaults.
#[derive(Parser)]
#[command(name = "g5-api", about = "G5 API Server")]
pub struct CliArgs {
    // -- Server ---------------------------------------------------------------
    #[arg(long, env = "G5_HOST", default_value = "0.0.0.0")]
    pub host: String,

    #[arg(long, env = "G5_PORT", default_value_t = 8080)]
    pub port: u16,

    #[arg(long, env = "G5_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "G5_LOG_FORMAT", default_value = "pretty")]
    pub log_format: LogFormat,

    // -- Database -------------------------------------------------------------
    #[arg(long, env = "G5_DATABASE_URL")]
    pub database_url: String,

    #[arg(long, env = "G5_DATABASE_MAX_CONNECTIONS", default_value_t = 20)]
    pub database_max_connections: u32,

    #[arg(long, env = "G5_DATABASE_MIN_CONNECTIONS", default_value_t = 5)]
    pub database_min_connections: u32,

    #[arg(long, env = "G5_DATABASE_CONNECT_TIMEOUT_SECS", default_value_t = 30)]
    pub database_connect_timeout_secs: u64,

    #[arg(long, env = "G5_DATABASE_IDLE_TIMEOUT_SECS", default_value_t = 600)]
    pub database_idle_timeout_secs: u64,

    // -- Authz Database (optional, falls back to G5_DATABASE_URL) --------------
    /// Separate database URL for the authz service (session management).
    /// If not set, uses G5_DATABASE_URL.
    #[arg(long, env = "G5_AUTHZ_DATABASE_URL")]
    pub authz_database_url: Option<String>,

    #[arg(long, env = "G5_AUTHZ_DATABASE_MAX_CONNECTIONS", default_value_t = 5)]
    pub authz_database_max_connections: u32,

    #[arg(long, env = "G5_AUTHZ_DATABASE_MIN_CONNECTIONS", default_value_t = 1)]
    pub authz_database_min_connections: u32,

    // -- Tenant Database (optional, falls back to G5_DATABASE_URL) -------------
    /// Separate database URL for tenant queries (ScopedDb / RLS).
    /// Should connect with a restricted Postgres role (`g5_tenant`) that
    /// cannot modify authz tables or disable RLS. Falls back to
    /// G5_DATABASE_URL when unset (dev mode — no role isolation).
    #[arg(long, env = "G5_TENANT_DATABASE_URL")]
    pub tenant_database_url: Option<String>,

    #[arg(long, env = "G5_TENANT_DATABASE_MAX_CONNECTIONS", default_value_t = 20)]
    pub tenant_database_max_connections: u32,

    #[arg(long, env = "G5_TENANT_DATABASE_MIN_CONNECTIONS", default_value_t = 5)]
    pub tenant_database_min_connections: u32,

    /// Require runtime pools to use distinct URLs for system, authz, and
    /// tenant access. Intended for CI and production-like environments.
    #[arg(long, env = "G5_REQUIRE_RUNTIME_DB_SPLIT", default_value_t = false)]
    pub require_runtime_db_split: bool,

    // -- Auth / JWT -----------------------------------------------------------
    #[arg(long, env = "G5_JWT_SECRET")]
    pub jwt_secret: Option<String>,

    #[arg(long, env = "G5_JWT_PUBLIC_KEY")]
    pub jwt_public_key: Option<String>,

    #[arg(long, env = "G5_JWT_ISSUER", default_value = "g5-api")]
    pub jwt_issuer: String,

    #[arg(long, env = "G5_JWT_AUDIENCE", default_value = "g5")]
    pub jwt_audience: String,

    #[arg(long, env = "G5_JWT_ACCESS_TOKEN_TTL_SECS", default_value_t = 3600)]
    pub jwt_access_token_ttl_secs: u64,

    #[arg(long, env = "G5_JWT_REFRESH_TOKEN_TTL_DAYS", default_value_t = 30)]
    pub jwt_refresh_token_ttl_days: u64,

    /// Absolute refresh-token lifetime from sign-in; refreshing slides the
    /// idle expiry above but never past this (issue #459).
    #[arg(
        long,
        env = "G5_JWT_REFRESH_TOKEN_ABSOLUTE_TTL_DAYS",
        default_value_t = 90
    )]
    pub jwt_refresh_token_absolute_ttl_days: u64,

    // -- OAuth Providers ------------------------------------------------------
    #[arg(long, env = "G5_GITHUB_CLIENT_ID")]
    pub github_client_id: Option<String>,

    #[arg(long, env = "G5_GITHUB_CLIENT_SECRET")]
    pub github_client_secret: Option<String>,

    /// Microsoft Entra ID (OIDC) platform login. Enabled only when the client
    /// id, client secret and tenant id are all present.
    #[arg(long, env = "G5_MICROSOFT_CLIENT_ID")]
    pub microsoft_client_id: Option<String>,

    #[arg(long, env = "G5_MICROSOFT_CLIENT_SECRET")]
    pub microsoft_client_secret: Option<String>,

    /// Entra directory UUID used for endpoints and exact token tenant validation.
    /// Tenant domains and multi-tenant aliases leave Microsoft login disabled.
    #[arg(long, env = "G5_MICROSOFT_TENANT_ID")]
    pub microsoft_tenant_id: Option<String>,

    #[arg(long, env = "G5_SCM_GITLAB_CLIENT_ID")]
    pub scm_gitlab_client_id: Option<String>,

    #[arg(long, env = "G5_SCM_GITLAB_CLIENT_SECRET")]
    pub scm_gitlab_client_secret: Option<String>,

    // -- OAuth Provider URL Overrides (for local OAuth stub) ------------------
    /// Override GitHub authorize URL (default: https://github.com/login/oauth/authorize)
    #[arg(long, env = "G5_GITHUB_AUTHORIZE_URL")]
    pub github_authorize_url: Option<String>,

    /// Override GitHub token URL (default: https://github.com/login/oauth/access_token)
    #[arg(long, env = "G5_GITHUB_TOKEN_URL")]
    pub github_token_url: Option<String>,

    /// Override GitHub userinfo URL (default: https://api.github.com/user)
    #[arg(long, env = "G5_GITHUB_USERINFO_URL")]
    pub github_userinfo_url: Option<String>,

    /// Override Microsoft authorize URL
    /// (default: https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize)
    #[arg(long, env = "G5_MICROSOFT_AUTHORIZE_URL")]
    pub microsoft_authorize_url: Option<String>,

    /// Override Microsoft token URL
    /// (default: https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token)
    #[arg(long, env = "G5_MICROSOFT_TOKEN_URL")]
    pub microsoft_token_url: Option<String>,

    /// Override Microsoft signing-key URL for a trusted local OAuth stub
    /// (default: https://login.microsoftonline.com/{tenant}/discovery/v2.0/keys)
    #[arg(long, env = "G5_MICROSOFT_JWKS_URL")]
    pub microsoft_jwks_url: Option<String>,

    /// Override GitLab SCM authorize URL
    #[arg(long, env = "G5_SCM_GITLAB_AUTHORIZE_URL")]
    pub scm_gitlab_authorize_url: Option<String>,

    /// Override GitLab SCM token URL
    #[arg(long, env = "G5_SCM_GITLAB_TOKEN_URL")]
    pub scm_gitlab_token_url: Option<String>,

    /// Override GitLab SCM userinfo URL
    #[arg(long, env = "G5_SCM_GITLAB_USERINFO_URL")]
    pub scm_gitlab_userinfo_url: Option<String>,

    /// GitHub API base URL for fine-grained PAT validation
    /// (default: https://api.github.com). Point at a hermetic stub in dev/CI.
    #[arg(long, env = "G5_SCM_GITHUB_API_BASE_URL")]
    pub scm_github_api_base_url: Option<String>,

    /// GitHub SCM web base URL, including any Enterprise path prefix.
    #[arg(long, env = "G5_SCM_GITHUB_WEB_BASE_URL")]
    pub scm_github_web_base_url: Option<String>,

    // -- SCM Token Encryption -------------------------------------------------
    #[arg(long, env = "G5_SCM_ENCRYPTION_KEY")]
    pub scm_encryption_key: Option<String>,

    // -- Cookie Security -------------------------------------------------------
    /// Set to `false` to omit the `Secure` flag on auth cookies (e.g. for
    /// CI / local development over plain HTTP). Default: `true`.
    #[arg(long, env = "G5_COOKIE_SECURE", default_value_t = true)]
    pub cookie_secure: bool,

    /// Base name of the httpOnly refresh-token cookie; secure deployments add
    /// `__Host-` unless already present and reject names used by login, SCM or
    /// GitHub App attempt cookies. A G5 stack served through a
    /// G5 preview needs a name outside `g5_`, which the preview relay
    /// reserves for the platform (`g5.workspace.preview-origin-security`).
    #[arg(long, env = "G5_REFRESH_COOKIE_NAME", default_value = "g5_refresh")]
    pub refresh_cookie_name: String,

    // -- CORS -----------------------------------------------------------------
    #[arg(
        long,
        env = "G5_CORS_ALLOWED_ORIGINS",
        default_value = "http://localhost:5173"
    )]
    pub cors_allowed_origins: String,

    // -- Maintenance ----------------------------------------------------------
    #[arg(long, env = "G5_INVITATION_GC_INTERVAL_SECS", default_value_t = 3600)]
    pub invitation_gc_interval_secs: u64,

    #[arg(long, env = "G5_STALE_DRAFT_TIMEOUT_SECS", default_value_t = 86400)]
    pub stale_draft_timeout_secs: u64,

    #[arg(long, env = "G5_DRAFT_STALE_RETENTION_SECS", default_value_t = 604800)]
    pub draft_stale_retention_secs: u64,

    #[arg(long, env = "G5_REPO_GC_INTERVAL_SECS", default_value_t = 3600)]
    pub repo_gc_interval_secs: u64,

    #[arg(long, env = "G5_REPO_GC_LOOSE_THRESHOLD", default_value_t = 500)]
    pub repo_gc_loose_threshold: u64,

    /// Remote-repo-tracking integration scheduler cadence. Each tick UPSERTs
    /// a `background_inbound` queue row per unpaused remote-tracking
    /// workspace/repository policy in `remote_integrations`.
    #[arg(
        long,
        env = "G5_REMOTE_TRACKING_SCHEDULER_INTERVAL_SECS",
        default_value_t = 30
    )]
    pub remote_tracking_scheduler_interval_secs: u64,

    /// Runtime kill switch for the per-child passive auto-pull policy (Option 1,
    /// G5 #1198). When `false`, the background integration scheduler and the
    /// advisory pinger passive sweeps are skipped, so no `background_inbound`
    /// rows are enqueued and no advisory `ls-remote` is issued. Disabling does
    /// NOT erase the `auto_pull_enabled` column or any remote-tracking state;
    /// explicit sync (Pull/Push/commit-coupled) remains functional.
    #[arg(
        long,
        env = "G5_PASSIVE_AUTO_PULL_ENABLED",
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        default_value_t = true
    )]
    pub passive_auto_pull_enabled: bool,

    /// Remote-repo-tracking advisory pinger cadence. Each tick ls-remotes
    /// every DIRTY remote-tracking branch and refreshes
    /// its bounded, expiring in-process remote-tip hint.
    #[arg(
        long,
        env = "G5_REMOTE_TRACKING_ADVISORY_INTERVAL_SECS",
        default_value_t = 60
    )]
    pub remote_tracking_advisory_interval_secs: u64,

    // -- Feature gates --------------------------------------------------------
    /// Customer self-hosted deployment-mode boundary. This is stricter than
    /// disabling cloud features: it removes G5-hosted delivery/admin surfaces
    /// from customer installs.
    #[arg(long, env = "SELF_HOSTED", default_value_t = false)]
    pub self_hosted: bool,

    /// Product-level cloud workflow gate. When false, g5-api suppresses cloud
    /// worker side effects such as automatic sync tasks while keeping local
    /// publication and indexing paths available.
    #[arg(long, env = "ENABLE_CLOUD_FEATURES")]
    pub enable_cloud_features: Option<bool>,

    /// Make-Remote local/bare git remote allow-list relaxation
    /// (make-remote-tracking D46 item 1). Deploy-scoped, global (not per-org).
    /// When false (default, fail-closed) an Adopt-Remote re-point rejects
    /// `file://`, `ext::`, and bare filesystem paths with `MAKE_REMOTE_URL_INVALID`
    /// so a tenant-scoped org-admin cannot point the shared `repos` row at a
    /// server-local location. Enable ONLY for self-hosted/dev/testing deployments
    /// that legitimately use local git remotes.
    #[arg(long, env = "G5_ALLOW_LOCAL_GIT_REMOTES", default_value_t = false)]
    pub allow_local_git_remotes: bool,

    /// Platform bug-reporting feature gate. When true (and the GitHub delivery
    /// config below is present) g5-api advertises the `platform_issue_reporting`
    /// instance feature, mounts `POST /api/v1/platform/issues`, and runs the
    /// GitHub-issue delivery outbox worker. Orthogonal to issue tracking and
    /// fail-closed when off or when delivery config is missing.
    #[arg(
        long,
        env = "G5_PLATFORM_ISSUE_REPORTING_ENABLED",
        default_value_t = false
    )]
    pub platform_issue_reporting_enabled: bool,

    /// Target GitHub repository (`"owner/repo"`) that platform bug reports are
    /// filed into. Server-side only.
    #[arg(long, env = "G5_PLATFORM_ISSUE_GITHUB_REPO")]
    pub platform_issue_github_repo: Option<String>,

    /// GitHub App id used to mint installation tokens for platform-issue delivery.
    #[arg(long, env = "G5_PLATFORM_ISSUE_GITHUB_APP_ID")]
    pub platform_issue_github_app_id: Option<String>,

    /// GitHub App installation id (the App's installation on the target repo).
    #[arg(long, env = "G5_PLATFORM_ISSUE_GITHUB_APP_INSTALLATION_ID")]
    pub platform_issue_github_app_installation_id: Option<String>,

    /// GitHub App private key (PEM). Server-only secret; never logged or sent to
    /// the client.
    #[arg(long, env = "G5_PLATFORM_ISSUE_GITHUB_APP_PRIVATE_KEY")]
    pub platform_issue_github_app_private_key: Option<String>,

    /// GitHub REST API base URL for platform-issue delivery
    /// (default: https://api.github.com). Point at a GitHub Enterprise host or a
    /// hermetic stub in dev/CI.
    #[arg(long, env = "G5_PLATFORM_ISSUE_GITHUB_API_BASE_URL")]
    pub platform_issue_github_api_base_url: Option<String>,

    /// Browser origin/prefix for the independent report destination.
    #[arg(long, env = "G5_PLATFORM_ISSUE_GITHUB_WEB_BASE_URL")]
    pub platform_issue_github_web_base_url: Option<String>,

    // -- Outbound email -------------------------------------------------------
    /// Outbound email transport: `disabled` (default), `capture` or `resend`.
    #[arg(
        long,
        env = "G5_EMAIL_TRANSPORT",
        value_enum,
        default_value_t = EmailTransportKind::Disabled
    )]
    pub email_transport: EmailTransportKind,

    /// Verified sending domain for outbound email, for example
    /// `mail.example.com`; each message kind names its own mailbox there.
    /// Required unless email is disabled.
    #[arg(long, env = "G5_EMAIL_FROM_DOMAIN")]
    pub email_from_domain: Option<String>,

    /// Display name on every outbound email (default `G5`).
    #[arg(long, env = "G5_EMAIL_FROM_NAME")]
    pub email_from_name: Option<String>,

    /// Resend API key. Server-only secret; required for the `resend` transport.
    #[arg(long, env = "G5_EMAIL_RESEND_API_KEY")]
    pub email_resend_api_key: Option<String>,

    /// Resend API base URL (default `https://api.resend.com`). Point at the
    /// hermetic provider stub in dev/CI.
    #[arg(long, env = "G5_EMAIL_RESEND_API_URL")]
    pub email_resend_api_url: Option<String>,

    /// Per-organization email volume limits: `enforce` (default) or `off`.
    /// `off` is for local development and is refused when mail would go
    /// through Resend's public API.
    #[arg(
        long,
        env = "G5_EMAIL_VOLUME_LIMITS",
        value_enum,
        default_value_t = EmailVolumeLimits::Enforce
    )]
    pub email_volume_limits: EmailVolumeLimits,

    // -- G5-owned GitHub App (organization installation bindings) ------------
    /// Numeric id of the single G5-owned GitHub App that organizations install.
    /// The binding feature is unavailable unless every required
    /// `G5_GITHUB_APP_*` value below is present and valid.
    #[arg(long, env = "G5_GITHUB_APP_ID")]
    pub github_app_id: Option<String>,

    /// GitHub App private key (PEM). Server-only secret; never logged.
    #[arg(long, env = "G5_GITHUB_APP_PRIVATE_KEY")]
    pub github_app_private_key: Option<String>,

    /// GitHub App URL slug (`https://github.com/apps/<slug>`).
    #[arg(long, env = "G5_GITHUB_APP_SLUG")]
    pub github_app_slug: Option<String>,

    /// OAuth client id of the GitHub App, used to verify that the installing
    /// administrator can access the installation.
    #[arg(long, env = "G5_GITHUB_APP_OAUTH_CLIENT_ID")]
    pub github_app_oauth_client_id: Option<String>,

    /// OAuth client secret of the GitHub App. Server-only secret.
    #[arg(long, env = "G5_GITHUB_APP_OAUTH_CLIENT_SECRET")]
    pub github_app_oauth_client_secret: Option<String>,

    /// Absolute URL of `/api/v1/github-app/oauth/callback` as registered on
    /// the GitHub App.
    #[arg(long, env = "G5_GITHUB_APP_OAUTH_CALLBACK_URL")]
    pub github_app_oauth_callback_url: Option<String>,

    /// HMAC secret (at least 32 bytes) that signs connection-attempt state,
    /// OAuth state and the PKCE cookie. Server-only secret.
    #[arg(long, env = "G5_GITHUB_APP_CONNECTION_STATE_SECRET")]
    pub github_app_connection_state_secret: Option<String>,

    /// GitHub REST API base URL for the App (default https://api.github.com).
    #[arg(long, env = "G5_GITHUB_APP_API_BASE_URL")]
    pub github_app_api_base_url: Option<String>,

    /// GitHub web base URL for installation and OAuth pages
    /// (default https://github.com).
    #[arg(long, env = "G5_GITHUB_APP_WEB_BASE_URL")]
    pub github_app_web_base_url: Option<String>,

    // -- Self-hosted license and registry delivery ---------------------------
    /// SHA-256 hex digest of the admin bearer token accepted by
    /// `POST /api/v1/admin/licenses`.
    #[arg(long, env = "G5_LICENSE_ADMIN_TOKEN_SHA256")]
    pub license_admin_token_sha256: Option<String>,

    /// Artifact Registry location used when a license provisioning request
    /// omits an explicit registry location.
    #[arg(
        long,
        env = "G5_SELF_HOSTED_REGISTRY_LOCATION",
        default_value = "us-central1"
    )]
    pub self_hosted_registry_location: String,

    /// Artifact Registry project used when a license provisioning request
    /// omits an explicit registry project.
    #[arg(long, env = "G5_SELF_HOSTED_REGISTRY_PROJECT")]
    pub self_hosted_registry_project: Option<String>,

    /// Artifact Registry repository used when a license provisioning request
    /// omits an explicit registry repository.
    #[arg(long, env = "G5_SELF_HOSTED_REGISTRY_REPOSITORY", default_value = "g5")]
    pub self_hosted_registry_repository: String,

    /// Pull-only service account impersonated when issuing short-lived Docker
    /// credentials for self-hosted installs.
    #[arg(long, env = "G5_SELF_HOSTED_REGISTRY_PULLER_SERVICE_ACCOUNT")]
    pub self_hosted_registry_puller_service_account: Option<String>,

    /// Lifetime for generated Artifact Registry access tokens.
    #[arg(
        long,
        env = "G5_SELF_HOSTED_REGISTRY_TOKEN_LIFETIME_SECS",
        default_value_t = 3600
    )]
    pub self_hosted_registry_token_lifetime_secs: u64,

    // -- Git / repos -----------------------------------------------------------
    /// Legacy local-storage base directory for repository working copies.
    /// When explicit `G5_STORAGE_*` settings are present, those drive the
    /// runtime storage backend instead.
    #[arg(long, env = "G5_REPOS_BASE_DIR", default_value = "/data/repos")]
    pub repos_base_dir: String,

    /// Repository-storage backend. Unset retains the legacy local fallback;
    /// an explicit value must be one of the supported providers.
    #[arg(
        long,
        env = "G5_STORAGE_PROVIDER",
        value_parser = ["", "local", "filestore"]
    )]
    pub storage_provider: Option<String>,

    #[arg(long, env = "G5_STORAGE_BASE_DIR")]
    pub storage_base_dir: Option<String>,

    #[arg(long, env = "G5_STORAGE_WORKTREE_DIR")]
    pub storage_worktree_dir: Option<String>,

    #[arg(long, env = "G5_STORAGE_MOUNT_POINT")]
    pub storage_mount_point: Option<String>,

    #[arg(long, env = "G5_STORAGE_TENANT_PREFIX")]
    pub storage_tenant_prefix: Option<String>,

    /// Directory of the attachment store (`g5.issue.native.attachment-store`):
    /// a dedicated mount on hosted deployments. Unset, local storage keeps it
    /// in `.attachments` under its base directory and Filestore has none.
    #[arg(long, env = "G5_ATTACHMENT_STORE_DIR")]
    pub attachment_store_dir: Option<String>,

    // -- JWT private key (for minting worker session tokens) ----------------
    /// PEM-encoded ES256 (ECDSA P-256) private key. When set, g5-api mints
    /// short-lived worker session JWTs at registration and heartbeat.
    #[arg(long, env = "G5_JWT_PRIVATE_KEY")]
    pub jwt_private_key: Option<String>,

    // -- Rate limiting --------------------------------------------------------
    /// Set to "false" to disable rate limiting (useful for tests).
    #[arg(long, env = "G5_RATE_LIMIT_ENABLED", default_value_t = true)]
    pub rate_limit_enabled: bool,

    /// Per-IP requests per minute (default 100).
    #[arg(long, env = "G5_RATE_LIMIT_RPM", default_value_t = 100)]
    pub rate_limit_rpm: u32,

    /// TCP peer CIDRs allowed to attest client addresses. Unset trusts no proxy.
    #[arg(long, env = "G5_TRUSTED_PROXY_CIDRS", value_delimiter = ',')]
    pub trusted_proxy_cidrs: Vec<ipnet::IpNet>,

    /// Number of right-hand X-Forwarded-For addresses the trusted proxy supplies.
    /// Caddy supplies 1; Google external Application Load Balancer supplies 2.
    #[arg(long, env = "G5_TRUSTED_PROXY_XFF_HOPS", default_value_t = 1,
        value_parser = clap::value_parser!(u8).range(1..=16))]
    pub trusted_proxy_xff_hops: u8,

    // -- Chat -----------------------------------------------------------------
    /// Seconds to wait for a worker to complete an assistant message before
    /// the API marks it as timed-out. Default 300 (5 minutes).
    #[arg(long, env = "G5_CHAT_RESPONSE_TIMEOUT_SECS", default_value_t = 300)]
    pub chat_response_timeout_secs: u64,

    // -- External URL ---------------------------------------------------------
    #[arg(long, env = "G5_EXTERNAL_URL", default_value = "http://localhost:8080")]
    pub external_url: String,

    /// Optional override for server-generated smart HTTP git URLs.
    ///
    /// Defaults to `external_url` when unset. This is useful when the browser
    /// is served from a different host than the API, but workers still need a
    /// direct API-reachable git endpoint.
    #[arg(long, env = "G5_GIT_EXTERNAL_URL")]
    pub git_external_url: Option<String>,

    /// Optional per-preview subdomain origin suffix (issue #935), e.g.
    /// `preview.dev.g5labs.ai`. When set, previews serve at
    /// `https://<workspace-id>.<suffix>/` (requires wildcard DNS + TLS for
    /// `*.<suffix>` pointing at this API). Unset/blank keeps the legacy
    /// `/api/v1/preview/<workspaceId>/` path-prefix serving only.
    #[arg(long, env = "G5_PREVIEW_HOST_SUFFIX")]
    pub preview_host_suffix: Option<String>,
}

impl std::fmt::Debug for CliArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliArgs")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("log_level", &self.log_level)
            .field("log_format", &self.log_format)
            .field("database_url", &"[REDACTED]")
            .field("database_max_connections", &self.database_max_connections)
            .field(
                "authz_database_url",
                &self.authz_database_url.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "tenant_database_url",
                &self.tenant_database_url.as_ref().map(|_| "[REDACTED]"),
            )
            .field("require_runtime_db_split", &self.require_runtime_db_split)
            .field(
                "jwt_secret",
                &self.jwt_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "jwt_public_key",
                &self.jwt_public_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("jwt_issuer", &self.jwt_issuer)
            .field("jwt_audience", &self.jwt_audience)
            .field("github_client_id", &self.github_client_id)
            .field(
                "github_client_secret",
                &self.github_client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("microsoft_client_id", &self.microsoft_client_id)
            .field(
                "microsoft_client_secret",
                &self.microsoft_client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("microsoft_tenant_id", &self.microsoft_tenant_id)
            .field(
                "scm_encryption_key",
                &self.scm_encryption_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "jwt_private_key",
                &self.jwt_private_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("rate_limit_enabled", &self.rate_limit_enabled)
            .field("rate_limit_rpm", &self.rate_limit_rpm)
            .field("trusted_proxy_cidrs", &self.trusted_proxy_cidrs)
            .field("trusted_proxy_xff_hops", &self.trusted_proxy_xff_hops)
            .field("self_hosted", &self.self_hosted)
            .field(
                "license_admin_token_sha256",
                &self
                    .license_admin_token_sha256
                    .as_ref()
                    .map(|_| "[REDACTED]"),
            )
            .field(
                "self_hosted_registry_location",
                &self.self_hosted_registry_location,
            )
            .field(
                "self_hosted_registry_project",
                &self.self_hosted_registry_project,
            )
            .field(
                "self_hosted_registry_repository",
                &self.self_hosted_registry_repository,
            )
            .field(
                "self_hosted_registry_puller_service_account",
                &self
                    .self_hosted_registry_puller_service_account
                    .as_ref()
                    .map(|_| "[REDACTED]"),
            )
            .field(
                "self_hosted_registry_token_lifetime_secs",
                &self.self_hosted_registry_token_lifetime_secs,
            )
            .field("external_url", &self.external_url)
            .field("git_external_url", &self.git_external_url)
            .field("preview_host_suffix", &self.preview_host_suffix)
            .field("github_app_id", &self.github_app_id)
            .field(
                "github_app_private_key",
                &self.github_app_private_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("github_app_slug", &self.github_app_slug)
            .field(
                "github_app_oauth_client_id",
                &self.github_app_oauth_client_id,
            )
            .field(
                "github_app_oauth_client_secret",
                &self
                    .github_app_oauth_client_secret
                    .as_ref()
                    .map(|_| "[REDACTED]"),
            )
            .field(
                "github_app_oauth_callback_url",
                &self.github_app_oauth_callback_url,
            )
            .field(
                "github_app_connection_state_secret",
                &self
                    .github_app_connection_state_secret
                    .as_ref()
                    .map(|_| "[REDACTED]"),
            )
            .field("github_app_api_base_url", &self.github_app_api_base_url)
            .field("github_app_web_base_url", &self.github_app_web_base_url)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Sub-configuration structs
// ---------------------------------------------------------------------------

/// Server binding and logging configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub log_format: LogFormat,
    pub external_url: String,
    pub git_external_url: String,
    /// Per-preview subdomain origin suffix (issue #935), normalized to
    /// lowercase without leading dot; `None` disables subdomain previews.
    pub preview_host_suffix: Option<String>,
    pub rate_limit_enabled: bool,
    pub rate_limit_rpm: u32,
    pub trusted_proxy_cidrs: Vec<ipnet::IpNet>,
    pub trusted_proxy_xff_hops: u8,
}

impl ServerConfig {
    /// Returns the socket address string for binding the HTTP server.
    #[must_use]
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// PostgreSQL connection pool configuration.
#[derive(Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
}

impl std::fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("min_connections", &self.min_connections)
            .field("connect_timeout", &self.connect_timeout)
            .field("idle_timeout", &self.idle_timeout)
            .finish()
    }
}

/// Per-provider OAuth credentials and optional URL overrides.
///
/// Constructed only when **both** `client_id` and `client_secret` are provided
/// for a given provider. If either is missing the provider is disabled (`None`).
/// URL fields override the hardcoded provider defaults (for local OAuth testing).
#[derive(Clone)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
}

impl std::fmt::Debug for OAuthProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthProviderConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .finish()
    }
}

/// JWT and OAuth configuration.
#[derive(Clone)]
pub struct AuthConfig {
    pub jwt_secret: Option<String>,
    pub jwt_public_key: Option<String>,
    pub jwt_issuer: String,
    pub jwt_audience: String,
    pub access_token_ttl: Duration,
    /// Idle refresh-token lifetime, renewed by each refresh.
    pub refresh_token_ttl: Duration,
    /// Absolute refresh-token lifetime from issue; never renewed.
    pub refresh_token_absolute_ttl: Duration,
    pub github: Option<OAuthProviderConfig>,
    /// Microsoft Entra ID login; present only with a tenant id.
    pub microsoft: Option<OAuthProviderConfig>,
    /// Entra tenant substituted into default Microsoft endpoints.
    pub microsoft_tenant_id: Option<String>,
    /// Trusted signing-key endpoint override; never selected by a token.
    pub microsoft_jwks_url: Option<String>,
    pub scm_gitlab: Option<OAuthProviderConfig>,
    /// Preserve the trusted GitLab API origin even when OAuth is disabled.
    pub scm_gitlab_userinfo_url: Option<String>,
    /// GitHub API base URL for fine-grained PAT validation (resolved; default
    /// `https://api.github.com`). Dev/CI may point this at a hermetic stub.
    pub scm_github_api_base_url: String,
    /// Trusted GitHub SCM web origin and repository path prefix.
    pub scm_github_web_base_url: String,
    pub scm_encryption_key: Option<String>,
    /// PEM-encoded ES256 private key for minting worker session tokens.
    pub jwt_private_key: Option<String>,
    /// Whether to set the `Secure` flag on auth cookies.
    /// Set to `false` for CI / local dev over plain HTTP.
    pub cookie_secure: bool,
    /// Refresh-cookie base name (`g5_refresh` by default); secure mode adds `__Host-`.
    pub refresh_cookie_name: String,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field(
                "jwt_secret",
                &self.jwt_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "jwt_public_key",
                &self.jwt_public_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("jwt_issuer", &self.jwt_issuer)
            .field("jwt_audience", &self.jwt_audience)
            .field("access_token_ttl", &self.access_token_ttl)
            .field("refresh_token_ttl", &self.refresh_token_ttl)
            .field(
                "refresh_token_absolute_ttl",
                &self.refresh_token_absolute_ttl,
            )
            .field("github", &self.github)
            .field("microsoft", &self.microsoft)
            .field("microsoft_tenant_id", &self.microsoft_tenant_id)
            .field("scm_gitlab", &self.scm_gitlab)
            .field(
                "scm_encryption_key",
                &self.scm_encryption_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "jwt_private_key",
                &self.jwt_private_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("cookie_secure", &self.cookie_secure)
            .field("refresh_cookie_name", &self.refresh_cookie_name)
            .finish()
    }
}

impl AuthConfig {
    /// Shared API/transport origin source, independent of OAuth enablement.
    #[must_use]
    pub fn scm_gitlab_api_url(&self) -> &str {
        self.scm_gitlab
            .as_ref()
            .and_then(|gl| gl.userinfo_url.as_deref())
            .or(self.scm_gitlab_userinfo_url.as_deref())
            .unwrap_or("https://gitlab.com/api/v4/user")
    }

    /// Returns the list of enabled OAuth provider names.
    #[must_use]
    pub fn enabled_providers(&self) -> Vec<&str> {
        let mut providers = Vec::new();
        if self.github.is_some() {
            providers.push("github");
        }
        if self.microsoft.is_some() {
            providers.push("microsoft");
        }
        providers
    }

    /// Returns `true` if at least one OAuth provider is configured.
    #[must_use]
    pub fn has_oauth(&self) -> bool {
        self.github.is_some() || self.microsoft.is_some()
    }
}

/// CORS allowed-origins configuration.
#[derive(Debug, Clone)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

/// Background maintenance task configuration.
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    pub invitation_gc_interval: Duration,
    pub stale_draft_timeout: Duration,
    pub draft_stale_retention: Duration,
    /// How often the repo-GC (git pack-maintenance) sweep runs.
    pub repo_gc_interval: Duration,
    /// Loose-object count at or above which a repo is repacked (`git gc`).
    pub repo_gc_loose_threshold: u64,
    /// Cadence for the remote-tracking background scheduler. See
    /// `g5-api-core/src/services/git/branch_integration_scheduler.rs`.
    pub remote_tracking_scheduler_interval: Duration,
    /// Cadence for the remote-tracking advisory pinger. See
    /// `g5-api-core/src/services/git/remote_advisory_pinger.rs`.
    pub remote_tracking_advisory_interval: Duration,
    /// Runtime kill switch for the per-child passive auto-pull policy (Option 1,
    /// G5 #1198). When `false`, the background scheduler and advisory pinger
    /// passive sweeps are skipped without erasing `auto_pull_enabled` or remote
    /// state. See `g5-api-core/src/services/git/branch_integration_scheduler.rs` D7.
    pub passive_auto_pull_enabled: bool,
}

/// Git repository storage configuration.
#[derive(Debug, Clone)]
pub struct ReposConfig {
    /// Base directory for repository working copies.
    /// Each repository lives at `{base_dir}/{org_id}/{repo_id}/`.
    pub base_dir: std::path::PathBuf,
}

/// Chat-specific configuration.
#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// How long the API waits for an agent turn before marking it timed out.
    pub response_timeout: Duration,
}

/// Self-hosted license provisioning and Artifact Registry delivery settings.
#[derive(Debug, Clone)]
pub struct SelfHostedConfig {
    pub license_admin_token_sha256: Option<String>,
    pub registry_location: String,
    pub registry_project: Option<String>,
    pub registry_repository: String,
    pub registry_puller_service_account: Option<String>,
    pub registry_token_lifetime: Duration,
}

// ---------------------------------------------------------------------------
// AppConfig — top-level configuration aggregate
// ---------------------------------------------------------------------------

/// Top-level application configuration that aggregates all sub-configs.
///
/// Resolved platform bug-reporting configuration.
///
/// Holds the independent feature flag and the server-only GitHub App delivery
/// credentials for filing platform bug reports as GitHub issues. The App private
/// key is a server-only secret and is redacted in the custom [`std::fmt::Debug`]
/// impl per `security_policy/secret_custody_and_redaction#D8`.
#[derive(Clone)]
pub struct PlatformIssueConfig {
    /// Independent feature gate (`G5_PLATFORM_ISSUE_REPORTING_ENABLED`),
    /// orthogonal to issue tracking.
    pub enabled: bool,
    /// Target GitHub repository as `"owner/repo"`.
    pub github_repo: Option<String>,
    /// GitHub App id used to mint installation tokens.
    pub github_app_id: Option<String>,
    /// GitHub App installation id for the target repo.
    pub github_app_installation_id: Option<String>,
    /// GitHub App private key (PEM). Server-only secret.
    pub github_app_private_key: Option<String>,
    /// GitHub REST API base URL (default `https://api.github.com`).
    pub github_api_base_url: String,
    /// Independent browser base (default `https://github.com`).
    pub github_web_base_url: String,
}

impl PlatformIssueConfig {
    /// True only when the feature is enabled AND every required GitHub delivery
    /// field is present. This is the fail-closed precondition for advertising the
    /// `platform_issue_reporting` feature and for accepting reports on the route.
    #[must_use]
    pub fn is_deliverable(&self) -> bool {
        self.enabled
            && self.github_repo.is_some()
            && self.github_app_id.is_some()
            && self.github_app_installation_id.is_some()
            && self.github_app_private_key.is_some()
    }
}

impl std::fmt::Debug for PlatformIssueConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformIssueConfig")
            .field("enabled", &self.enabled)
            .field("github_repo", &self.github_repo)
            .field("github_app_id", &self.github_app_id)
            .field(
                "github_app_installation_id",
                &self.github_app_installation_id,
            )
            .field(
                "github_app_private_key",
                &self.github_app_private_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("github_api_base_url", &self.github_api_base_url)
            .field("github_web_base_url", &self.github_web_base_url)
            .finish()
    }
}

/// Resolved outbound email configuration (`g5.email.provider`).
///
/// The Resend API key is a server-only secret and is redacted by the custom
/// [`std::fmt::Debug`] impl. [`EmailConfig::validate`] is checked at startup so
/// an incomplete non-disabled configuration fails instead of dropping mail.
#[derive(Clone)]
pub struct EmailConfig {
    pub transport: EmailTransportKind,
    /// Verified sending domain; message kinds choose the mailbox name
    /// (`g5.email.sender`).
    pub from_domain: Option<String>,
    /// Display name on every message.
    pub from_name: String,
    /// Resend API key. Server-only secret.
    pub resend_api_key: Option<String>,
    /// Resend API base URL (default `https://api.resend.com`).
    pub resend_api_url: String,
    /// Whether per-organization volume limits apply.
    pub volume_limits: EmailVolumeLimits,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            transport: EmailTransportKind::Disabled,
            from_domain: None,
            from_name: DEFAULT_EMAIL_FROM_NAME.to_string(),
            resend_api_key: None,
            resend_api_url: DEFAULT_RESEND_API_URL.to_string(),
            volume_limits: EmailVolumeLimits::Enforce,
        }
    }
}

impl EmailConfig {
    /// Reject a non-disabled configuration that could not deliver mail.
    pub fn validate(&self) -> Result<(), String> {
        if self.transport == EmailTransportKind::Disabled {
            return Ok(());
        }
        let domain = self.from_domain.as_deref().ok_or_else(|| {
            "G5_EMAIL_FROM_DOMAIN must be set when outbound email is enabled".to_string()
        })?;
        if !is_plausible_domain(domain) {
            return Err(
                "G5_EMAIL_FROM_DOMAIN must be a domain name such as mail.example.com".to_string(),
            );
        }
        if !is_plain_display_name(&self.from_name) {
            return Err(
                "G5_EMAIL_FROM_NAME must be 1-64 letters, digits, spaces, dots, hyphens or underscores"
                    .to_string(),
            );
        }
        if self.transport == EmailTransportKind::Resend {
            if self.resend_api_key.is_none() {
                return Err(
                    "G5_EMAIL_RESEND_API_KEY must be set for the resend email transport"
                        .to_string(),
                );
            }
            let url = url::Url::parse(&self.resend_api_url)
                .map_err(|_| "G5_EMAIL_RESEND_API_URL must be a valid URL".to_string())?;
            if !matches!(url.scheme(), "https" | "http")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(
                    "G5_EMAIL_RESEND_API_URL must be an http(s) URL without userinfo".to_string(),
                );
            }
            if self.volume_limits == EmailVolumeLimits::Off
                && url.host_str() == Some(RESEND_PUBLIC_API_HOST)
            {
                return Err(
                    "G5_EMAIL_VOLUME_LIMITS=off is for local stacks; it is refused while mail \
                     goes through Resend's public API"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}

/// Resend's public API, used when `G5_EMAIL_RESEND_API_URL` is unset.
pub const DEFAULT_RESEND_API_URL: &str = "https://api.resend.com";

/// Mail sent through this host reaches real recipients.
const RESEND_PUBLIC_API_HOST: &str = "api.resend.com";

/// Display name used when `G5_EMAIL_FROM_NAME` is unset.
pub const DEFAULT_EMAIL_FROM_NAME: &str = "G5";

/// A DNS name of at least two labels, each of letters, digits and inner
/// hyphens.
fn is_plausible_domain(value: &str) -> bool {
    let labels: Vec<&str> = value.split('.').collect();
    value.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// A display name that needs no quoting in a `From` header.
fn is_plain_display_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.trim() == value
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_'))
}

impl std::fmt::Debug for EmailConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailConfig")
            .field("transport", &self.transport)
            .field("from_domain", &self.from_domain)
            .field("from_name", &self.from_name)
            .field(
                "resend_api_key",
                &self.resend_api_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("resend_api_url", &self.resend_api_url)
            .field("volume_limits", &self.volume_limits)
            .finish()
    }
}

/// Resolved configuration of the single G5-owned GitHub App that
/// organizations install to bind their GitHub repositories.
///
/// It exists only when every required value is present and valid; a partial
/// configuration resolves to `None` so the feature fails closed instead of
/// exposing a flow that cannot verify its callback. The private key, OAuth
/// client secret and state-signing secret are redacted in `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct GithubAppConfig {
    pub app_id: String,
    pub private_key_pem: String,
    pub app_slug: String,
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
    pub oauth_callback_url: String,
    pub connection_state_secret: String,
    /// GitHub REST API base URL without a trailing slash.
    pub api_base_url: String,
    /// GitHub web base URL without a trailing slash.
    pub web_base_url: String,
}

/// Raw, possibly incomplete GitHub App settings before validation.
#[derive(Default)]
pub struct GithubAppConfigInput<'a> {
    pub app_id: Option<&'a str>,
    pub private_key_pem: Option<&'a str>,
    pub app_slug: Option<&'a str>,
    pub oauth_client_id: Option<&'a str>,
    pub oauth_client_secret: Option<&'a str>,
    pub oauth_callback_url: Option<&'a str>,
    pub connection_state_secret: Option<&'a str>,
    pub api_base_url: Option<&'a str>,
    pub web_base_url: Option<&'a str>,
}

/// Minimum length of the HMAC connection-state secret.
pub const GITHUB_APP_STATE_SECRET_MIN_BYTES: usize = 32;

impl GithubAppConfig {
    /// Validate a complete configuration.
    ///
    /// Returns `Ok(None)` when no required value is set (feature not
    /// configured), `Ok(Some)` when all are valid, and `Err` naming the
    /// missing or invalid settings (never their values) otherwise.
    pub fn resolve(input: &GithubAppConfigInput<'_>) -> Result<Option<Self>, String> {
        fn value(raw: Option<&str>) -> Option<String> {
            raw.map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        }
        fn http_url(raw: &str) -> Option<String> {
            let normalized = raw.trim().trim_end_matches('/').to_string();
            let parsed = url::Url::parse(&normalized).ok()?;
            (matches!(parsed.scheme(), "https" | "http")
                && parsed.host().is_some()
                && parsed.username().is_empty()
                && parsed.password().is_none())
            .then_some(normalized)
        }

        let required = [
            ("G5_GITHUB_APP_ID", value(input.app_id)),
            ("G5_GITHUB_APP_PRIVATE_KEY", value(input.private_key_pem)),
            ("G5_GITHUB_APP_SLUG", value(input.app_slug)),
            (
                "G5_GITHUB_APP_OAUTH_CLIENT_ID",
                value(input.oauth_client_id),
            ),
            (
                "G5_GITHUB_APP_OAUTH_CLIENT_SECRET",
                value(input.oauth_client_secret),
            ),
            (
                "G5_GITHUB_APP_OAUTH_CALLBACK_URL",
                value(input.oauth_callback_url),
            ),
            (
                "G5_GITHUB_APP_CONNECTION_STATE_SECRET",
                value(input.connection_state_secret),
            ),
        ];
        if required.iter().all(|(_, value)| value.is_none()) {
            return Ok(None);
        }
        let missing: Vec<&str> = required
            .iter()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| *name)
            .collect();
        if !missing.is_empty() {
            return Err(format!("missing {}", missing.join(", ")));
        }
        let [app_id, private_key_pem, app_slug, oauth_client_id, oauth_client_secret, oauth_callback_url, connection_state_secret] =
            required.map(|(_, value)| value.unwrap_or_default());

        let mut invalid = Vec::new();
        if app_id.parse::<u64>().map_or(true, |id| id == 0) {
            invalid.push("G5_GITHUB_APP_ID");
        }
        if !app_slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            invalid.push("G5_GITHUB_APP_SLUG");
        }
        let oauth_callback_url = http_url(&oauth_callback_url).unwrap_or_else(|| {
            invalid.push("G5_GITHUB_APP_OAUTH_CALLBACK_URL");
            String::new()
        });
        if connection_state_secret.len() < GITHUB_APP_STATE_SECRET_MIN_BYTES {
            invalid.push("G5_GITHUB_APP_CONNECTION_STATE_SECRET");
        }
        let api_base_url = http_url(
            &value(input.api_base_url).unwrap_or_else(|| "https://api.github.com".to_string()),
        )
        .unwrap_or_else(|| {
            invalid.push("G5_GITHUB_APP_API_BASE_URL");
            String::new()
        });
        let web_base_url = http_url(
            &value(input.web_base_url).unwrap_or_else(|| "https://github.com".to_string()),
        )
        .unwrap_or_else(|| {
            invalid.push("G5_GITHUB_APP_WEB_BASE_URL");
            String::new()
        });
        if !invalid.is_empty() {
            return Err(format!("invalid {}", invalid.join(", ")));
        }

        Ok(Some(Self {
            app_id,
            private_key_pem,
            app_slug,
            oauth_client_id,
            oauth_client_secret,
            oauth_callback_url,
            connection_state_secret,
            api_base_url,
            web_base_url,
        }))
    }
}

impl std::fmt::Debug for GithubAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubAppConfig")
            .field("app_id", &self.app_id)
            .field("private_key_pem", &"[REDACTED]")
            .field("app_slug", &self.app_slug)
            .field("oauth_client_id", &self.oauth_client_id)
            .field("oauth_client_secret", &"[REDACTED]")
            .field("oauth_callback_url", &self.oauth_callback_url)
            .field("connection_state_secret", &"[REDACTED]")
            .field("api_base_url", &self.api_base_url)
            .field("web_base_url", &self.web_base_url)
            .finish()
    }
}

/// Constructed from [`CliArgs`] via [`AppConfig::from_cli`]. Designed to be
/// shared across the application as `Arc<AppConfig>` in the axum state.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    /// Authz-writer pool config — used for session management in
    /// `authz.user_sessions`. Falls back to main `database` URL
    /// when `G5_AUTHZ_DATABASE_URL` is not set.
    pub authz_database: DatabaseConfig,
    /// Tenant pool config — used for ScopedDb (RLS-scoped queries).
    /// Should connect with a restricted Postgres role (`g5_tenant`).
    /// Falls back to main `database` URL when unset (dev mode).
    pub tenant_database: DatabaseConfig,
    /// Enforce distinct runtime URLs for system/authz/tenant pools.
    pub require_runtime_db_split: bool,
    pub auth: AuthConfig,
    pub cors: CorsConfig,
    pub maintenance: MaintenanceConfig,
    pub repos: ReposConfig,
    pub storage: g5_storage::StorageConfig,
    /// Explicit attachment store directory; see [`AppConfig::attachment_store_root`].
    pub attachment_store_dir: Option<std::path::PathBuf>,
    pub chat: ChatConfig,
    pub self_hosted: SelfHostedConfig,
    pub is_self_hosted: bool,
    pub enable_cloud_features: bool,
    /// Deploy-scoped relaxation allowing local/bare git remotes for Adopt-Remote
    /// (make-remote-tracking D46 item 1). Default false / fail-closed.
    pub allow_local_git_remotes: bool,
    /// Platform bug-reporting feature flag + server-only GitHub delivery config.
    pub platform_issue: PlatformIssueConfig,
    /// Outbound email transport, sender and provider credentials.
    pub email: EmailConfig,
    /// G5-owned GitHub App used for organization installation bindings.
    /// `None` (feature unavailable) unless completely and validly configured.
    pub github_app: Option<GithubAppConfig>,
    /// Approval request/status policy. Not deployment configuration: the CLI
    /// always selects the constants; tests inject the alternatives.
    pub approval_policy: g5_contracts::types::ApprovalPolicy,
}

impl AppConfig {
    /// Construct a validated [`AppConfig`] from parsed CLI arguments.
    ///
    /// This is the sole construction path. OAuth providers are enabled only
    /// when **both** `client_id` and `client_secret` are supplied with
    /// non-empty values. CORS origins are parsed from a comma-separated
    /// string. Duration fields are converted from integer CLI args
    /// (seconds or milliseconds as
    /// appropriate).
    #[must_use]
    pub fn from_cli(args: CliArgs) -> Self {
        Self::try_from_cli(args).expect("CLI parsing must reject invalid storage configuration")
    }

    /// Construct an [`AppConfig`] without panicking on invalid typed storage
    /// inputs. Production startup uses this path so configuration errors are
    /// returned to the process entrypoint with their original context.
    pub fn try_from_cli(args: CliArgs) -> Result<Self, g5_storage::StorageError> {
        let storage = Self::storage_config_from_sources(&args)?;
        Ok(Self::from_cli_with_storage(args, storage))
    }

    fn from_cli_with_storage(args: CliArgs, storage: g5_storage::StorageConfig) -> Self {
        let github_app = Self::github_app_config_from_cli(&args);
        fn normalized_optional(value: &Option<String>) -> Option<String> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        }

        let repos_base_dir = Self::repos_base_dir_for_storage(&storage, &args.repos_base_dir);

        fn provider_config(
            client_id: &Option<String>,
            client_secret: &Option<String>,
            authorize_url: &Option<String>,
            token_url: &Option<String>,
            userinfo_url: &Option<String>,
        ) -> Option<OAuthProviderConfig> {
            match (
                normalized_optional(client_id),
                normalized_optional(client_secret),
            ) {
                (Some(id), Some(secret)) => Some(OAuthProviderConfig {
                    client_id: id,
                    client_secret: secret,
                    authorize_url: normalized_optional(authorize_url),
                    token_url: normalized_optional(token_url),
                    userinfo_url: normalized_optional(userinfo_url),
                }),
                _ => None,
            }
        }

        // Build OAuth provider configs — only when both halves are present.
        let github = provider_config(
            &args.github_client_id,
            &args.github_client_secret,
            &args.github_authorize_url,
            &args.github_token_url,
            &args.github_userinfo_url,
        );
        // A directory UUID is required for an exact issuer/tid comparison.
        let microsoft_tenant_id = normalized_optional(&args.microsoft_tenant_id).and_then(|tenant| {
            match uuid::Uuid::parse_str(&tenant).ok().filter(|id| !id.is_nil()) {
                Some(id) => Some(id.to_string()),
                None => {
                    tracing::warn!("G5_MICROSOFT_TENANT_ID must be a directory UUID; Microsoft login is disabled");
                    None
                }
            }
        });
        let microsoft = if microsoft_tenant_id.is_some() {
            provider_config(
                &args.microsoft_client_id,
                &args.microsoft_client_secret,
                &args.microsoft_authorize_url,
                &args.microsoft_token_url,
                &None,
            )
        } else {
            None
        };
        let scm_gitlab = provider_config(
            &args.scm_gitlab_client_id,
            &args.scm_gitlab_client_secret,
            &args.scm_gitlab_authorize_url,
            &args.scm_gitlab_token_url,
            &args.scm_gitlab_userinfo_url,
        );

        // Parse comma-separated CORS origins, trimming whitespace and
        // filtering out empty segments.
        let cors_origins: Vec<String> = args
            .cors_allowed_origins
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let chat_response_timeout_secs = args.chat_response_timeout_secs.clamp(10, 600);
        let git_external_url = args
            .git_external_url
            .clone()
            .unwrap_or_else(|| args.external_url.clone());
        // Normalize the preview suffix (issue #935): blank disables. When it
        // canonicalizes to a bare IDNA-ASCII domain, store that canonical
        // lowercase-punycode form so the mint/reauth handoff URLs and the
        // same-site check reuse one value (routes/preview [D15], PR #1283
        // review P1). A malformed suffix is kept as its trimmed raw value so
        // startup validation (`validate_preview_suffix_same_site`) rejects it
        // loudly rather than this normalizer silently dropping it.
        let preview_host_suffix = args
            .preview_host_suffix
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| canonical_preview_host_suffix(s).unwrap_or_else(|_| s.to_string()));

        AppConfig {
            server: ServerConfig {
                host: args.host,
                port: args.port,
                log_level: args.log_level,
                log_format: args.log_format,
                external_url: args.external_url,
                git_external_url,
                preview_host_suffix,
                rate_limit_enabled: args.rate_limit_enabled,
                rate_limit_rpm: args.rate_limit_rpm,
                trusted_proxy_cidrs: args.trusted_proxy_cidrs,
                trusted_proxy_xff_hops: args.trusted_proxy_xff_hops,
            },
            database: DatabaseConfig {
                url: args.database_url.clone(),
                max_connections: args.database_max_connections,
                min_connections: args.database_min_connections,
                connect_timeout: Duration::from_secs(args.database_connect_timeout_secs),
                idle_timeout: Duration::from_secs(args.database_idle_timeout_secs),
            },
            authz_database: DatabaseConfig {
                url: args
                    .authz_database_url
                    .unwrap_or_else(|| args.database_url.clone()),
                max_connections: args.authz_database_max_connections,
                min_connections: args.authz_database_min_connections,
                connect_timeout: Duration::from_secs(args.database_connect_timeout_secs),
                idle_timeout: Duration::from_secs(args.database_idle_timeout_secs),
            },
            tenant_database: DatabaseConfig {
                url: args
                    .tenant_database_url
                    .unwrap_or_else(|| args.database_url.clone()),
                max_connections: args.tenant_database_max_connections,
                min_connections: args.tenant_database_min_connections,
                connect_timeout: Duration::from_secs(args.database_connect_timeout_secs),
                idle_timeout: Duration::from_secs(args.database_idle_timeout_secs),
            },
            require_runtime_db_split: args.require_runtime_db_split,
            auth: AuthConfig {
                jwt_secret: args.jwt_secret,
                jwt_public_key: args.jwt_public_key,
                jwt_issuer: args.jwt_issuer,
                jwt_audience: args.jwt_audience,
                access_token_ttl: Duration::from_secs(args.jwt_access_token_ttl_secs),
                refresh_token_ttl: Duration::from_secs(args.jwt_refresh_token_ttl_days * 86400),
                refresh_token_absolute_ttl: Duration::from_secs(
                    args.jwt_refresh_token_absolute_ttl_days * 86400,
                ),
                github,
                microsoft,
                microsoft_tenant_id,
                microsoft_jwks_url: normalized_optional(&args.microsoft_jwks_url),
                scm_gitlab,
                scm_gitlab_userinfo_url: normalized_optional(&args.scm_gitlab_userinfo_url),
                scm_github_api_base_url: args
                    .scm_github_api_base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.github.com".to_string()),
                scm_github_web_base_url: normalized_optional(&args.scm_github_web_base_url)
                    .unwrap_or_else(|| "https://github.com".into()),
                scm_encryption_key: args.scm_encryption_key,
                jwt_private_key: args.jwt_private_key,
                cookie_secure: args.cookie_secure,
                refresh_cookie_name: args.refresh_cookie_name.trim().to_string(),
            },
            cors: CorsConfig {
                allowed_origins: cors_origins,
            },
            maintenance: MaintenanceConfig {
                invitation_gc_interval: Duration::from_secs(args.invitation_gc_interval_secs),
                stale_draft_timeout: Duration::from_secs(args.stale_draft_timeout_secs),
                draft_stale_retention: Duration::from_secs(args.draft_stale_retention_secs),
                repo_gc_interval: Duration::from_secs(args.repo_gc_interval_secs),
                repo_gc_loose_threshold: args.repo_gc_loose_threshold,
                remote_tracking_scheduler_interval: Duration::from_secs(
                    args.remote_tracking_scheduler_interval_secs,
                ),
                remote_tracking_advisory_interval: Duration::from_secs(
                    args.remote_tracking_advisory_interval_secs,
                ),
                passive_auto_pull_enabled: args.passive_auto_pull_enabled,
            },
            repos: ReposConfig {
                base_dir: repos_base_dir,
            },
            storage,
            attachment_store_dir: args
                .attachment_store_dir
                .as_deref()
                .map(str::trim)
                .filter(|dir| !dir.is_empty())
                .map(std::path::PathBuf::from),
            chat: ChatConfig {
                response_timeout: Duration::from_secs(chat_response_timeout_secs),
            },
            self_hosted: SelfHostedConfig {
                license_admin_token_sha256: normalized_optional(&args.license_admin_token_sha256)
                    .map(|value| value.to_lowercase()),
                registry_location: args.self_hosted_registry_location.trim().to_string(),
                registry_project: normalized_optional(&args.self_hosted_registry_project),
                registry_repository: args.self_hosted_registry_repository.trim().to_string(),
                registry_puller_service_account: normalized_optional(
                    &args.self_hosted_registry_puller_service_account,
                ),
                registry_token_lifetime: Duration::from_secs(
                    args.self_hosted_registry_token_lifetime_secs
                        .clamp(60, 3600),
                ),
            },
            is_self_hosted: args.self_hosted,
            enable_cloud_features: args.enable_cloud_features.unwrap_or(false),
            allow_local_git_remotes: args.allow_local_git_remotes,
            platform_issue: PlatformIssueConfig {
                enabled: args.platform_issue_reporting_enabled,
                github_repo: normalized_optional(&args.platform_issue_github_repo),
                github_app_id: normalized_optional(&args.platform_issue_github_app_id),
                github_app_installation_id: normalized_optional(
                    &args.platform_issue_github_app_installation_id,
                ),
                github_app_private_key: normalized_optional(
                    &args.platform_issue_github_app_private_key,
                ),
                github_api_base_url: normalized_optional(&args.platform_issue_github_api_base_url)
                    .unwrap_or_else(|| "https://api.github.com".to_string()),
                github_web_base_url: normalized_optional(&args.platform_issue_github_web_base_url)
                    .unwrap_or_else(|| "https://github.com".to_string()),
            },
            email: EmailConfig {
                transport: args.email_transport,
                from_domain: normalized_optional(&args.email_from_domain)
                    .map(|domain| domain.to_ascii_lowercase()),
                from_name: normalized_optional(&args.email_from_name)
                    .unwrap_or_else(|| DEFAULT_EMAIL_FROM_NAME.to_string()),
                resend_api_key: normalized_optional(&args.email_resend_api_key),
                resend_api_url: normalized_optional(&args.email_resend_api_url)
                    .map(|url| url.trim_end_matches('/').to_string())
                    .unwrap_or_else(|| DEFAULT_RESEND_API_URL.to_string()),
                volume_limits: args.email_volume_limits,
            },
            github_app,
            approval_policy: g5_contracts::types::ApprovalPolicy::default(),
        }
    }

    fn github_app_config_from_cli(args: &CliArgs) -> Option<GithubAppConfig> {
        let input = GithubAppConfigInput {
            app_id: args.github_app_id.as_deref(),
            private_key_pem: args.github_app_private_key.as_deref(),
            app_slug: args.github_app_slug.as_deref(),
            oauth_client_id: args.github_app_oauth_client_id.as_deref(),
            oauth_client_secret: args.github_app_oauth_client_secret.as_deref(),
            oauth_callback_url: args.github_app_oauth_callback_url.as_deref(),
            connection_state_secret: args.github_app_connection_state_secret.as_deref(),
            api_base_url: args.github_app_api_base_url.as_deref(),
            web_base_url: args.github_app_web_base_url.as_deref(),
        };
        match GithubAppConfig::resolve(&input) {
            Ok(config) => config,
            Err(problem) => {
                tracing::warn!(
                    %problem,
                    "GitHub App configuration is incomplete or invalid; GitHub App bindings are disabled"
                );
                None
            }
        }
    }

    fn storage_config_from_sources(
        args: &CliArgs,
    ) -> Result<g5_storage::StorageConfig, g5_storage::StorageError> {
        fn normalized(value: &Option<String>) -> Option<&str> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        }

        let storage_provider = normalized(&args.storage_provider);
        let storage_base_dir = normalized(&args.storage_base_dir);
        let storage_worktree_dir = normalized(&args.storage_worktree_dir);
        let storage_mount_point = normalized(&args.storage_mount_point);
        let storage_tenant_prefix = normalized(&args.storage_tenant_prefix);
        let has_explicit_storage = storage_provider.is_some()
            || storage_base_dir.is_some()
            || storage_worktree_dir.is_some()
            || storage_mount_point.is_some()
            || storage_tenant_prefix.is_some();
        if has_explicit_storage {
            g5_storage::StorageConfig::from_values(
                storage_provider,
                storage_base_dir,
                storage_worktree_dir,
                storage_mount_point,
                storage_tenant_prefix,
            )
        } else {
            Ok(g5_storage::StorageConfig::local(std::path::PathBuf::from(
                &args.repos_base_dir,
            )))
        }
    }

    /// Where attachment bytes live: the explicit directory, else
    /// `.attachments` under local storage's base directory. Filestore has no
    /// default, so a missing mount cannot silently land on a container's disk.
    #[must_use]
    pub fn attachment_store_root(&self) -> Option<std::path::PathBuf> {
        if let Some(dir) = &self.attachment_store_dir {
            return Some(dir.clone());
        }
        match &self.storage {
            g5_storage::StorageConfig::Local { base_dir, .. } => {
                Some(base_dir.join(".attachments"))
            }
            _ => None,
        }
    }

    fn repos_base_dir_for_storage(
        storage: &g5_storage::StorageConfig,
        legacy_repos_base_dir: &str,
    ) -> std::path::PathBuf {
        match storage {
            g5_storage::StorageConfig::Local { base_dir, .. } => base_dir.clone(),
            g5_storage::StorageConfig::Filestore {
                mount_point,
                tenant_prefix,
                ..
            } => mount_point.join(tenant_prefix),
            _ => std::path::PathBuf::from(legacy_repos_base_dir),
        }
    }

    /// Validate configuration at startup and panic on critical issues.
    ///
    /// Called early in `main()` after `from_cli()`. In production, missing
    /// or default-only values for security-critical fields should cause a
    /// hard startup failure rather than silently running with insecure
    /// defaults.
    ///
    /// Currently validates:
    /// - `jwt_issuer` and `jwt_audience` are explicitly set (not defaults)
    /// - `database.url` is non-empty
    /// - `jwt_public_key` is present when `jwt_secret` is absent
    /// - the GitLab API endpoint uses HTTPS without userinfo, query or fragment
    ///
    /// # Panics
    ///
    /// Panics with a descriptive message if validation fails.
    pub fn validate_startup(&self) {
        // Database URL must be set
        assert!(!self.database.url.is_empty(), "G5_DATABASE_URL must be set");

        self.email
            .validate()
            .unwrap_or_else(|message| panic!("{message}"));

        g5_auth::validate_deployment_keys(
            self.auth.jwt_secret.as_deref(),
            self.auth.jwt_private_key.as_deref(),
            self.auth.jwt_public_key.as_deref(),
            self.auth.scm_encryption_key.as_deref(),
        )
        .unwrap_or_else(|message| panic!("{message}"));

        // Stored connections outlive OAuth enablement. Validate the retained
        // API endpoint before any service can send a bearer token to it.
        let gitlab_api_url = self.auth.scm_gitlab_api_url();
        let has_clean_authority = gitlab_api_url.split_once("://").is_some_and(|(_, rest)| {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            !authority.is_empty() && !authority.contains('@')
        });
        assert!(
            has_clean_authority
                && !gitlab_api_url.contains('\\')
                && !gitlab_api_url.chars().any(|c| c.is_whitespace() || c.is_control())
                && url::Url::parse(gitlab_api_url).is_ok_and(|endpoint| {
                    endpoint.scheme() == "https"
                        && endpoint.host_str().is_some()
                        && endpoint.username().is_empty()
                        && endpoint.password().is_none()
                        && endpoint.query().is_none()
                        && endpoint.fragment().is_none()
                }),
            "G5_SCM_GITLAB_USERINFO_URL must be a valid HTTPS URL without userinfo, query or fragment"
        );

        // The refresh cookie's name is a bare RFC 6265 token, so it can be
        // interpolated into Set-Cookie and matched in Cookie headers as is.
        assert!(
            !self.auth.refresh_cookie_name.is_empty()
                && self.auth.refresh_cookie_name.len() <= 64
                && self
                    .auth
                    .refresh_cookie_name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "G5_REFRESH_COOKIE_NAME must be 1-64 ASCII letters, digits, '_' or '-'"
        );
        let lower_cookie_name = self.auth.refresh_cookie_name.to_ascii_lowercase();
        assert!(
            self.auth.cookie_secure
                || !(lower_cookie_name.starts_with("__host-")
                    || lower_cookie_name.starts_with("__secure-")),
            "G5_REFRESH_COOKIE_NAME cannot use a secure prefix when G5_COOKIE_SECURE=false"
        );

        // Transport-security invariant for auth cookies: when the deployment is
        // HTTPS (external_url scheme `https`), reject an operator-supplied
        // `G5_COOKIE_SECURE=false`. Omitting `Secure` would let the `g5_refresh`
        // session cookie and the email-bearing `g5_login_denied` denied-context
        // cookie cross a plaintext hop (CWE-319). The relaxed override stays
        // valid only for plain-HTTP dev/CI deployments. See
        // software_architecture/g5-api/config#D4.
        let deployment_is_https = self
            .server
            .external_url
            .trim_start()
            .get(..8)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"));
        if !self.auth.cookie_secure && deployment_is_https {
            panic!(
                "G5_COOKIE_SECURE=false is not allowed under an HTTPS deployment \
                 (G5_EXTERNAL_URL={}): omitting Secure would let auth cookies, \
                 including the email-bearing g5_login_denied cookie, travel over \
                 plaintext. Set G5_COOKIE_SECURE=true, or use an http:// \
                 G5_EXTERNAL_URL for local/CI over plain HTTP.",
                self.server.external_url
            );
        }

        if self.require_runtime_db_split {
            assert!(
                self.database.url != self.authz_database.url,
                "G5_REQUIRE_RUNTIME_DB_SPLIT=true requires G5_AUTHZ_DATABASE_URL to differ from G5_DATABASE_URL"
            );
            assert!(
                self.database.url != self.tenant_database.url,
                "G5_REQUIRE_RUNTIME_DB_SPLIT=true requires G5_TENANT_DATABASE_URL to differ from G5_DATABASE_URL"
            );
            assert!(
                self.authz_database.url != self.tenant_database.url,
                "G5_REQUIRE_RUNTIME_DB_SPLIT=true requires G5_AUTHZ_DATABASE_URL to differ from G5_TENANT_DATABASE_URL"
            );
        }

        // Warn (not panic) if jwt_issuer/jwt_audience are using defaults.
        // In a future release these will become required (no default_value).
        if self.auth.jwt_issuer == "g5-api" {
            tracing::warn!(
                "G5_JWT_ISSUER is using the default value 'g5-api'. \
                 Set it explicitly in production."
            );
        }
        if self.auth.jwt_audience == "g5" {
            tracing::warn!(
                "G5_JWT_AUDIENCE is using the default value 'g5'. \
                 Set it explicitly in production."
            );
        }

        // At least one auth mechanism must be configured
        if self.auth.jwt_secret.is_none() && self.auth.jwt_public_key.is_none() {
            tracing::warn!(
                "No auth mechanism configured (G5_JWT_SECRET or G5_JWT_PUBLIC_KEY). \
                 All authenticated endpoints will reject requests."
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a CliArgs with all required fields and sensible defaults.
    fn default_cli_args() -> CliArgs {
        CliArgs {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            log_format: LogFormat::Pretty,
            database_url: "postgres://user:pass@localhost:5432/g5".to_string(),
            database_max_connections: 20,
            database_min_connections: 5,
            database_connect_timeout_secs: 30,
            database_idle_timeout_secs: 600,
            jwt_secret: None,
            jwt_public_key: None,
            jwt_issuer: "g5-api".to_string(),
            jwt_audience: "g5".to_string(),
            jwt_access_token_ttl_secs: 3600,
            jwt_refresh_token_ttl_days: 30,
            jwt_refresh_token_absolute_ttl_days: 90,
            github_client_id: None,
            github_client_secret: None,
            microsoft_client_id: None,
            microsoft_client_secret: None,
            microsoft_tenant_id: None,
            scm_gitlab_client_id: None,
            scm_gitlab_client_secret: None,
            github_authorize_url: None,
            github_token_url: None,
            github_userinfo_url: None,
            microsoft_authorize_url: None,
            microsoft_token_url: None,
            microsoft_jwks_url: None,
            scm_gitlab_authorize_url: None,
            scm_gitlab_token_url: None,
            scm_gitlab_userinfo_url: None,
            scm_github_api_base_url: None,
            scm_github_web_base_url: None,
            scm_encryption_key: None,
            cors_allowed_origins: "http://localhost:5173".to_string(),
            invitation_gc_interval_secs: 3600,
            stale_draft_timeout_secs: 86400,
            draft_stale_retention_secs: 604800,
            repo_gc_interval_secs: 3600,
            repo_gc_loose_threshold: 500,
            remote_tracking_scheduler_interval_secs: 30,
            passive_auto_pull_enabled: true,
            remote_tracking_advisory_interval_secs: 60,
            self_hosted: false,
            enable_cloud_features: None,
            allow_local_git_remotes: false,
            platform_issue_reporting_enabled: false,
            platform_issue_github_repo: None,
            platform_issue_github_app_id: None,
            platform_issue_github_app_installation_id: None,
            platform_issue_github_app_private_key: None,
            platform_issue_github_api_base_url: None,
            platform_issue_github_web_base_url: None,
            email_transport: EmailTransportKind::Disabled,
            email_from_domain: None,
            email_from_name: None,
            email_resend_api_key: None,
            email_resend_api_url: None,
            email_volume_limits: EmailVolumeLimits::Enforce,
            github_app_id: None,
            github_app_private_key: None,
            github_app_slug: None,
            github_app_oauth_client_id: None,
            github_app_oauth_client_secret: None,
            github_app_oauth_callback_url: None,
            github_app_connection_state_secret: None,
            github_app_api_base_url: None,
            github_app_web_base_url: None,
            license_admin_token_sha256: None,
            self_hosted_registry_location: "us-central1".to_string(),
            self_hosted_registry_project: None,
            self_hosted_registry_repository: "g5".to_string(),
            self_hosted_registry_puller_service_account: None,
            self_hosted_registry_token_lifetime_secs: 3600,
            jwt_private_key: None,
            rate_limit_enabled: true,
            rate_limit_rpm: 100,
            trusted_proxy_cidrs: vec![],
            trusted_proxy_xff_hops: 1,
            chat_response_timeout_secs: 300,
            external_url: "http://localhost:8080".to_string(),
            git_external_url: None,
            preview_host_suffix: None,
            repos_base_dir: "/tmp/g5-test-repos".to_string(),
            storage_provider: None,
            storage_base_dir: None,
            storage_worktree_dir: None,
            storage_mount_point: None,
            storage_tenant_prefix: None,
            attachment_store_dir: None,
            authz_database_url: None,
            authz_database_max_connections: 5,
            authz_database_min_connections: 1,
            tenant_database_url: None,
            tenant_database_max_connections: 20,
            tenant_database_min_connections: 5,
            require_runtime_db_split: false,
            cookie_secure: true,
            refresh_cookie_name: "g5_refresh".to_string(),
        }
    }

    #[test]
    #[serial_test::serial]
    fn proxy_authority_requires_valid_cidrs_and_bounded_suffix_length() {
        use clap::{CommandFactory, FromArgMatches};
        // Exercise production Clap defaults independently of the caller's env.
        let matches = CliArgs::command()
            .mut_arg("trusted_proxy_cidrs", |arg| arg.env(None::<&str>))
            .mut_arg("trusted_proxy_xff_hops", |arg| arg.env(None::<&str>))
            .try_get_matches_from(["g5-api", "--database-url", "postgres://test/test"])
            .unwrap();
        let args = CliArgs::from_arg_matches(&matches).unwrap();
        assert!(args.trusted_proxy_cidrs.is_empty());
        assert_eq!(args.trusted_proxy_xff_hops, 1);
        let parse = |cidrs: &str, hops: &str| {
            CliArgs::try_parse_from([
                "g5-api",
                "--database-url",
                "postgres://test/test",
                "--trusted-proxy-cidrs",
                cidrs,
                "--trusted-proxy-xff-hops",
                hops,
            ])
        };
        let valid = parse("130.211.0.0/22,35.191.0.0/16", "2").unwrap();
        let config = AppConfig::from_cli(valid);
        assert_eq!(config.server.trusted_proxy_cidrs.len(), 2);
        assert_eq!(config.server.trusted_proxy_xff_hops, 2);
        assert!(parse("2001:db8::/64", "1").is_ok());
        for invalid in ["", "private", "127.0.0.1", "10.0.0.0/33", "10.0.0.0/8,"] {
            assert!(parse(invalid, "1").is_err(), "accepted {invalid:?}");
        }
        for invalid in ["0", "17", "-1", "anything"] {
            assert!(
                parse("127.0.0.1/32", invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn scm_github_web_endpoint_defaults_and_preserves_enterprise_prefix() {
        assert_eq!(
            AppConfig::from_cli(default_cli_args())
                .auth
                .scm_github_web_base_url,
            "https://github.com"
        );
        let mut args = default_cli_args();
        args.scm_github_web_base_url = Some(" https://enterprise.test:8443/git ".into());
        assert_eq!(
            AppConfig::from_cli(args).auth.scm_github_web_base_url,
            "https://enterprise.test:8443/git"
        );
    }

    #[test]
    fn test_platform_issue_config_requires_every_delivery_field() {
        let complete = PlatformIssueConfig {
            enabled: true,
            github_repo: Some("g5labs/platform-bugs".to_string()),
            github_app_id: Some("123".to_string()),
            github_app_installation_id: Some("987".to_string()),
            github_app_private_key: Some("test-private-key".to_string()),
            github_api_base_url: "https://api.github.com".to_string(),
            github_web_base_url: "https://github.com".to_string(),
        };
        assert!(complete.is_deliverable());

        let mut missing = complete.clone();
        missing.enabled = false;
        assert!(!missing.is_deliverable());

        let mut missing = complete.clone();
        missing.github_repo = None;
        assert!(!missing.is_deliverable());

        let mut missing = complete.clone();
        missing.github_app_id = None;
        assert!(!missing.is_deliverable());

        let mut missing = complete.clone();
        missing.github_app_installation_id = None;
        assert!(!missing.is_deliverable());

        let mut missing = complete;
        missing.github_app_private_key = None;
        assert!(!missing.is_deliverable());
    }

    #[test]
    fn test_platform_issue_config_normalizes_cli_values_and_redacts_the_private_key() {
        let mut args = default_cli_args();
        args.platform_issue_reporting_enabled = true;
        args.platform_issue_github_repo = Some(" g5labs/platform-bugs ".to_string());
        args.platform_issue_github_app_id = Some(" 123 ".to_string());
        args.platform_issue_github_app_installation_id = Some(" 987 ".to_string());
        args.platform_issue_github_app_private_key = Some(" test-private-key ".to_string());
        args.platform_issue_github_api_base_url = Some(" \t ".to_string());

        let config = AppConfig::from_cli(args);

        assert!(config.platform_issue.is_deliverable());
        assert_eq!(
            config.platform_issue.github_repo.as_deref(),
            Some("g5labs/platform-bugs")
        );
        assert_eq!(config.platform_issue.github_app_id.as_deref(), Some("123"));
        assert_eq!(
            config.platform_issue.github_app_installation_id.as_deref(),
            Some("987")
        );
        assert_eq!(
            config.platform_issue.github_api_base_url,
            "https://api.github.com"
        );

        let debug = format!("{:?}", config.platform_issue);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("test-private-key"));
    }

    #[test]
    fn email_is_disabled_by_default_and_needs_no_settings() {
        let config = AppConfig::from_cli(default_cli_args());
        assert_eq!(config.email.transport, EmailTransportKind::Disabled);
        assert_eq!(config.email.validate(), Ok(()));
    }

    #[test]
    fn enabled_email_requires_a_valid_sender_and_resend_needs_its_key() {
        let mut args = default_cli_args();
        args.email_transport = EmailTransportKind::Capture;
        assert!(AppConfig::from_cli(args).email.validate().is_err());

        let mut args = default_cli_args();
        args.email_transport = EmailTransportKind::Capture;
        args.email_from_domain = Some(" Mail.G5Labs.ai ".to_string());
        let config = AppConfig::from_cli(args);
        assert_eq!(config.email.validate(), Ok(()));
        assert_eq!(config.email.from_domain.as_deref(), Some("mail.g5labs.ai"));
        assert_eq!(config.email.from_name, "G5");

        for bad in [
            "localhost",
            "invitations@mail.g5labs.ai",
            "mail..g5labs.ai",
            "-mail.g5labs.ai",
            "mail.g5labs.ai\r\nBcc: x",
            "G5 <mail.g5labs.ai>",
        ] {
            let mut args = default_cli_args();
            args.email_transport = EmailTransportKind::Capture;
            args.email_from_domain = Some(bad.to_string());
            assert!(
                AppConfig::from_cli(args).email.validate().is_err(),
                "domain {bad:?} must be rejected"
            );
        }
        for bad in ["G5 <x@y.z>", "G5, Inc", "G5\r\nBcc", &"x".repeat(65)] {
            let mut args = default_cli_args();
            args.email_transport = EmailTransportKind::Capture;
            args.email_from_domain = Some("mail.g5labs.ai".to_string());
            args.email_from_name = Some(bad.to_string());
            assert!(
                AppConfig::from_cli(args).email.validate().is_err(),
                "display name {bad:?} must be rejected"
            );
        }

        let mut args = default_cli_args();
        args.email_transport = EmailTransportKind::Resend;
        args.email_from_domain = Some("mail.g5labs.ai".to_string());
        assert!(AppConfig::from_cli(args).email.validate().is_err());

        let mut args = default_cli_args();
        args.email_transport = EmailTransportKind::Resend;
        args.email_from_domain = Some("mail.g5labs.ai".to_string());
        args.email_from_name = Some("G5 Labs".to_string());
        args.email_resend_api_key = Some(" re_test_secret ".to_string());
        args.email_resend_api_url = Some("http://oauth-stub:9876/".to_string());
        let config = AppConfig::from_cli(args);
        assert_eq!(config.email.validate(), Ok(()));
        assert_eq!(config.email.from_name, "G5 Labs");
        assert_eq!(config.email.resend_api_url, "http://oauth-stub:9876");
        let debug = format!("{:?}", config.email);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("re_test_secret"));
    }

    #[test]
    fn email_volume_limits_are_enforced_by_default_and_off_only_for_local_stacks() {
        assert_eq!(
            AppConfig::from_cli(default_cli_args()).email.volume_limits,
            EmailVolumeLimits::Enforce
        );
        let resend = |url: Option<&str>| {
            let mut args = default_cli_args();
            args.email_transport = EmailTransportKind::Resend;
            args.email_from_domain = Some("mail.g5labs.ai".to_string());
            args.email_resend_api_key = Some("re_test_secret".to_string());
            args.email_resend_api_url = url.map(str::to_string);
            args.email_volume_limits = EmailVolumeLimits::Off;
            args
        };
        // The dev stack's provider stub.
        let config = AppConfig::from_cli(resend(Some("http://oauth-stub:9876")));
        assert_eq!(config.email.volume_limits, EmailVolumeLimits::Off);
        assert_eq!(config.email.validate(), Ok(()));
        // Real recipients through Resend's public API.
        for url in [None, Some("https://api.resend.com/")] {
            assert!(
                AppConfig::from_cli(resend(url)).email.validate().is_err(),
                "{url:?} must refuse volume limits off"
            );
        }
        let mut args = default_cli_args();
        args.email_transport = EmailTransportKind::Capture;
        args.email_from_domain = Some("mail.g5labs.ai".to_string());
        args.email_volume_limits = EmailVolumeLimits::Off;
        assert_eq!(AppConfig::from_cli(args).email.validate(), Ok(()));
    }

    fn complete_github_app_args() -> CliArgs {
        let mut args = default_cli_args();
        args.github_app_id = Some(" 123456 ".to_string());
        args.github_app_private_key = Some("test-github-app-private-key".to_string());
        args.github_app_slug = Some("g5-test-app".to_string());
        args.github_app_oauth_client_id = Some("Iv1.test".to_string());
        args.github_app_oauth_client_secret = Some("test-oauth-client-secret".to_string());
        args.github_app_oauth_callback_url =
            Some("https://g5.example.test/api/v1/github-app/oauth/callback".to_string());
        args.github_app_connection_state_secret =
            Some("test-state-secret-that-is-at-least-32-bytes".to_string());
        args
    }

    #[test]
    fn github_app_config_is_absent_by_default() {
        assert!(AppConfig::from_cli(default_cli_args()).github_app.is_none());
    }

    #[test]
    fn github_app_config_resolves_defaults_and_redacts_secrets() {
        let mut args = complete_github_app_args();
        args.github_app_api_base_url = Some(" https://ghe.example.test/api/v3/ ".to_string());
        let config = AppConfig::from_cli(args)
            .github_app
            .expect("complete configuration resolves");
        assert_eq!(config.app_id, "123456");
        assert_eq!(config.api_base_url, "https://ghe.example.test/api/v3");
        assert_eq!(config.web_base_url, "https://github.com");

        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("test-github-app-private-key"));
        assert!(!debug.contains("test-oauth-client-secret"));
        assert!(!debug.contains("test-state-secret"));

        let debug = format!("{:?}", complete_github_app_args());
        assert!(!debug.contains("test-github-app-private-key"));
        assert!(!debug.contains("test-oauth-client-secret"));
        assert!(!debug.contains("test-state-secret"));
    }

    #[test]
    fn github_app_config_fails_closed_when_partial_or_invalid() {
        let mut missing = complete_github_app_args();
        missing.github_app_oauth_client_secret = Some("  ".to_string());
        assert!(AppConfig::from_cli(missing).github_app.is_none());

        let mut short_secret = complete_github_app_args();
        short_secret.github_app_connection_state_secret = Some("too-short".to_string());
        assert!(AppConfig::from_cli(short_secret).github_app.is_none());

        let mut bad_slug = complete_github_app_args();
        bad_slug.github_app_slug = Some("../evil".to_string());
        assert!(AppConfig::from_cli(bad_slug).github_app.is_none());

        let mut bad_callback = complete_github_app_args();
        bad_callback.github_app_oauth_callback_url = Some("javascript:alert(1)".to_string());
        assert!(AppConfig::from_cli(bad_callback).github_app.is_none());

        let mut bad_app_id = complete_github_app_args();
        bad_app_id.github_app_id = Some("not-a-number".to_string());
        assert!(AppConfig::from_cli(bad_app_id).github_app.is_none());

        let error = GithubAppConfig::resolve(&GithubAppConfigInput {
            app_id: Some("1"),
            ..GithubAppConfigInput::default()
        })
        .expect_err("partial configuration is an error");
        assert!(error.contains("G5_GITHUB_APP_PRIVATE_KEY"));
    }

    #[test]
    fn test_from_cli_defaults() {
        let args = default_cli_args();
        let config = AppConfig::from_cli(args);

        // Server
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.log_level, "info");
        assert_eq!(config.server.log_format, LogFormat::Pretty);
        assert_eq!(config.server.external_url, "http://localhost:8080");
        assert_eq!(config.server.git_external_url, "http://localhost:8080");

        // Database
        assert_eq!(
            config.database.url,
            "postgres://user:pass@localhost:5432/g5"
        );
        assert_eq!(config.database.max_connections, 20);
        assert_eq!(config.database.min_connections, 5);
        assert_eq!(config.database.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.database.idle_timeout, Duration::from_secs(600));

        // Auth — no secret, no OAuth providers
        assert!(config.auth.jwt_secret.is_none());
        assert_eq!(config.auth.jwt_issuer, "g5-api");
        assert_eq!(config.auth.jwt_audience, "g5");
        assert_eq!(config.auth.access_token_ttl, Duration::from_secs(3600));
        assert_eq!(
            config.auth.refresh_token_ttl,
            Duration::from_secs(30 * 86400)
        );
        assert_eq!(
            config.auth.refresh_token_absolute_ttl,
            Duration::from_secs(90 * 86400)
        );
        assert!(config.auth.github.is_none());
        assert!(config.auth.microsoft.is_none());
        assert!(config.auth.microsoft_tenant_id.is_none());
        assert!(config.auth.scm_encryption_key.is_none());
        assert!(!config.auth.has_oauth());
        assert!(config.auth.enabled_providers().is_empty());

        // CORS
        assert_eq!(config.cors.allowed_origins, vec!["http://localhost:5173"]);

        // Maintenance
        assert_eq!(
            config.maintenance.invitation_gc_interval,
            Duration::from_secs(3600)
        );
        assert_eq!(
            config.maintenance.stale_draft_timeout,
            Duration::from_secs(86400)
        );
        assert_eq!(
            config.maintenance.draft_stale_retention,
            Duration::from_secs(604800)
        );
        assert!(config.maintenance.passive_auto_pull_enabled);

        assert!(!config.is_self_hosted);
        assert!(!config.enable_cloud_features);
        assert!(config.self_hosted.license_admin_token_sha256.is_none());
        assert_eq!(config.self_hosted.registry_location, "us-central1");
        assert!(config.self_hosted.registry_project.is_none());
        assert_eq!(config.self_hosted.registry_repository, "g5");
        assert!(config.self_hosted.registry_puller_service_account.is_none());
        assert_eq!(
            config.self_hosted.registry_token_lifetime,
            Duration::from_secs(3600)
        );
        assert_eq!(
            config.repos.base_dir,
            std::path::PathBuf::from("/tmp/g5-test-repos")
        );
        match config.storage {
            g5_storage::StorageConfig::Local {
                base_dir,
                worktree_dir,
            } => {
                assert_eq!(base_dir, std::path::PathBuf::from("/tmp/g5-test-repos"));
                assert_eq!(
                    worktree_dir,
                    std::path::PathBuf::from("/tmp/g5-test-repos/.worktrees")
                );
            }
            _ => panic!("expected local storage"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn platform_issue_reporting_defaults_off_when_env_is_unset() {
        let prior = std::env::var("G5_PLATFORM_ISSUE_REPORTING_ENABLED").ok();

        std::env::remove_var("G5_PLATFORM_ISSUE_REPORTING_ENABLED");
        let args = CliArgs::try_parse_from([
            "g5-api",
            "--database-url",
            "postgres://user:pass@localhost:5432/g5",
        ])
        .expect("minimal CLI arguments should parse");

        assert!(
            !args.platform_issue_reporting_enabled,
            "an unset platform-issue flag must remain opt-in"
        );
        assert!(
            !AppConfig::from_cli(args).platform_issue.enabled,
            "the resolved application configuration must keep reporting disabled"
        );

        match prior {
            Some(value) => std::env::set_var("G5_PLATFORM_ISSUE_REPORTING_ENABLED", value),
            None => std::env::remove_var("G5_PLATFORM_ISSUE_REPORTING_ENABLED"),
        }
    }

    #[test]
    fn passive_auto_pull_cli_accepts_explicit_false() {
        let args = CliArgs::try_parse_from([
            "g5-api",
            "--database-url",
            "postgres://user:pass@localhost:5432/g5",
            "--passive-auto-pull-enabled=false",
        ])
        .expect("explicit passive auto-pull false should parse");

        assert!(!args.passive_auto_pull_enabled);
    }

    #[test]
    fn test_from_cli_respects_cloud_feature_gate() {
        let mut args = default_cli_args();
        args.enable_cloud_features = Some(false);

        let config = AppConfig::from_cli(args);

        assert!(!config.enable_cloud_features);
    }

    #[test]
    fn test_from_cli_respects_self_hosted_deployment_mode() {
        let mut args = default_cli_args();
        args.self_hosted = true;
        args.enable_cloud_features = Some(true);

        let config = AppConfig::from_cli(args);

        assert!(config.is_self_hosted);
        assert!(config.enable_cloud_features);
    }

    #[test]
    fn test_from_cli_prefers_explicit_filestore_storage_settings() {
        let mut args = default_cli_args();
        args.storage_provider = Some("filestore".to_string());
        args.storage_mount_point = Some("/mnt/g5-repos".to_string());
        args.storage_tenant_prefix = Some("repos".to_string());
        args.storage_worktree_dir = Some("/data/worktrees".to_string());
        let config = AppConfig::from_cli(args);

        assert_eq!(
            config.repos.base_dir,
            std::path::PathBuf::from("/mnt/g5-repos/repos")
        );
        match config.storage {
            g5_storage::StorageConfig::Filestore {
                mount_point,
                tenant_prefix,
                worktree_dir,
            } => {
                assert_eq!(mount_point, std::path::PathBuf::from("/mnt/g5-repos"));
                assert_eq!(tenant_prefix, "repos");
                assert_eq!(worktree_dir, std::path::PathBuf::from("/data/worktrees"));
            }
            _ => panic!("expected filestore storage"),
        }
    }

    #[test]
    fn cli_rejects_unknown_storage_provider() {
        let result = CliArgs::try_parse_from([
            "g5-api",
            "--database-url",
            "postgres://user:pass@localhost:5432/g5",
            "--storage-provider",
            "object-store",
        ]);
        let error = result.expect_err("unknown storage providers must fail during CLI parsing");
        assert!(error.to_string().contains("invalid value 'object-store'"));
    }

    #[test]
    fn empty_storage_values_retain_the_legacy_repository_root() {
        let mut args = default_cli_args();
        args.repos_base_dir = "/legacy/repos".to_string();
        args.storage_provider = Some("  ".to_string());
        args.storage_base_dir = Some(String::new());
        args.storage_worktree_dir = Some("  ".to_string());
        args.storage_mount_point = Some(String::new());
        args.storage_tenant_prefix = Some("  ".to_string());

        let config = AppConfig::try_from_cli(args)
            .expect("empty storage values must retain the legacy fallback");
        assert!(matches!(
            config.storage,
            g5_storage::StorageConfig::Local { ref base_dir, .. }
                if base_dir == &std::path::PathBuf::from("/legacy/repos")
        ));
    }

    #[test]
    #[serial_test::serial]
    fn cli_accepts_an_empty_storage_provider_as_unset() {
        let storage_env_names = [
            "G5_STORAGE_PROVIDER",
            "G5_STORAGE_BASE_DIR",
            "G5_STORAGE_WORKTREE_DIR",
            "G5_STORAGE_MOUNT_POINT",
            "G5_STORAGE_TENANT_PREFIX",
        ];
        let previous = storage_env_names.map(|name| std::env::var(name).ok());
        for name in storage_env_names {
            std::env::set_var(name, "");
        }
        let args = CliArgs::try_parse_from([
            "g5-api",
            "--database-url",
            "postgres://user:pass@localhost:5432/g5",
            "--repos-base-dir",
            "/legacy/repos",
            "--storage-provider",
            "",
        ])
        .expect("an empty storage provider must preserve legacy environment semantics");
        for (name, value) in storage_env_names.into_iter().zip(previous) {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        let config = AppConfig::try_from_cli(args)
            .expect("empty storage provider must select the legacy local fallback");
        assert!(matches!(
            config.storage,
            g5_storage::StorageConfig::Local { ref base_dir, .. }
                if base_dir == &std::path::PathBuf::from("/legacy/repos")
        ));
    }

    #[test]
    fn try_from_cli_reports_invalid_typed_storage_without_panicking() {
        let mut args = default_cli_args();
        args.storage_provider = Some("object-store".to_string());
        let error = AppConfig::try_from_cli(args)
            .expect_err("invalid typed storage input must be returned to the caller");
        assert!(error.to_string().contains("unsupported storage provider"));
    }

    #[test]
    #[serial_test::serial]
    fn storage_cli_values_override_environment_values() {
        let prior_provider = std::env::var("G5_STORAGE_PROVIDER").ok();
        let prior_mount = std::env::var("G5_STORAGE_MOUNT_POINT").ok();
        std::env::set_var("G5_STORAGE_PROVIDER", "filestore");
        std::env::set_var("G5_STORAGE_MOUNT_POINT", "/env/filestore");

        let args = CliArgs::try_parse_from([
            "g5-api",
            "--database-url",
            "postgres://user:pass@localhost:5432/g5",
            "--storage-provider",
            "local",
            "--storage-base-dir",
            "/cli/repos",
        ])
        .expect("supported CLI storage values should parse");
        let config = AppConfig::from_cli(args);
        assert!(matches!(
            config.storage,
            g5_storage::StorageConfig::Local { ref base_dir, .. }
                if base_dir == &std::path::PathBuf::from("/cli/repos")
        ));

        match prior_provider {
            Some(value) => std::env::set_var("G5_STORAGE_PROVIDER", value),
            None => std::env::remove_var("G5_STORAGE_PROVIDER"),
        }
        match prior_mount {
            Some(value) => std::env::set_var("G5_STORAGE_MOUNT_POINT", value),
            None => std::env::remove_var("G5_STORAGE_MOUNT_POINT"),
        }
    }

    #[test]
    fn test_bind_addr() {
        let args = default_cli_args();
        let config = AppConfig::from_cli(args);
        assert_eq!(config.server.bind_addr(), "0.0.0.0:8080");
    }

    #[test]
    fn test_bind_addr_custom() {
        let mut args = default_cli_args();
        args.host = "127.0.0.1".to_string();
        args.port = 3000;
        let config = AppConfig::from_cli(args);
        assert_eq!(config.server.bind_addr(), "127.0.0.1:3000");
    }

    #[test]
    fn test_validate_startup_rejects_invalid_gitlab_api_with_or_without_oauth() {
        for oauth_enabled in [true, false] {
            for endpoint in [
                "http://gitlab.internal/api/v4/user",
                "ftp://gitlab.internal/api/v4/user",
                "not-a-url",
                "https://",
                "https://user:password@gitlab.internal/api/v4/user",
                "https://@gitlab.internal/api/v4/user",
                "https:gitlab.internal/api/v4/user",
                "https://gitlab.internal\\api/v4/user",
                "https://gitlab.internal/api/\tv4/user",
                "https://gitlab.internal/api/v4/user?alternate=1",
                "https://gitlab.internal/api/v4/user#fragment",
            ] {
                let mut args = default_cli_args();
                args.scm_gitlab_userinfo_url = Some(endpoint.into());
                if oauth_enabled {
                    args.scm_gitlab_client_id = Some("client".into());
                    args.scm_gitlab_client_secret = Some("secret".into());
                }
                let config = AppConfig::from_cli(args);
                assert!(
                    std::panic::catch_unwind(|| config.validate_startup()).is_err(),
                    "startup accepted {endpoint} with OAuth enabled={oauth_enabled}"
                );
            }
        }
    }

    #[test]
    fn test_validate_startup_accepts_https_gitlab_api_with_or_without_oauth() {
        for oauth_enabled in [true, false] {
            for endpoint in [
                None,
                Some("https://gitlab.com/api/v4/user"),
                Some("https://gitlab.internal:8443/gitlab/api/v4/user"),
            ] {
                let mut args = default_cli_args();
                args.scm_gitlab_userinfo_url = endpoint.map(str::to_string);
                if oauth_enabled {
                    args.scm_gitlab_client_id = Some("client".into());
                    args.scm_gitlab_client_secret = Some("secret".into());
                }
                AppConfig::from_cli(args).validate_startup();
            }
        }
    }

    #[test]
    fn test_validate_startup_requires_runtime_db_split() {
        let mut args = default_cli_args();
        args.authz_database_url = Some("postgres://authz:pass@localhost:5432/g5".to_string());
        args.tenant_database_url = Some("postgres://tenant:pass@localhost:5432/g5".to_string());
        args.require_runtime_db_split = true;

        let config = AppConfig::from_cli(args);
        config.validate_startup();
    }

    #[test]
    #[should_panic(
        expected = "G5_REQUIRE_RUNTIME_DB_SPLIT=true requires G5_TENANT_DATABASE_URL to differ from G5_DATABASE_URL"
    )]
    fn test_validate_startup_rejects_collapsed_runtime_db_split() {
        let mut args = default_cli_args();
        args.authz_database_url = Some("postgres://authz:pass@localhost:5432/g5".to_string());
        args.require_runtime_db_split = true;

        let config = AppConfig::from_cli(args);
        config.validate_startup();
    }

    #[test]
    #[should_panic(expected = "G5_COOKIE_SECURE=false is not allowed under an HTTPS deployment")]
    fn test_validate_startup_rejects_insecure_cookie_under_https() {
        let mut args = default_cli_args();
        args.external_url = "https://app.example.com".to_string();
        args.cookie_secure = false;

        let config = AppConfig::from_cli(args);
        config.validate_startup();
    }

    #[test]
    fn test_validate_startup_allows_insecure_cookie_under_http() {
        // Plain-HTTP dev/CI deployments may relax the Secure flag.
        let mut args = default_cli_args();
        args.external_url = "http://localhost:8080".to_string();
        args.cookie_secure = false;

        let config = AppConfig::from_cli(args);
        config.validate_startup();
    }

    #[test]
    fn test_validate_startup_allows_secure_cookie_under_https() {
        let mut args = default_cli_args();
        args.external_url = "https://app.example.com".to_string();
        args.cookie_secure = true;

        let config = AppConfig::from_cli(args);
        config.validate_startup();
    }

    #[test]
    fn test_refresh_cookie_name_defaults_and_accepts_a_preview_name() {
        let config = AppConfig::from_cli(default_cli_args());
        assert_eq!(config.auth.refresh_cookie_name, "g5_refresh");
        config.validate_startup();

        let mut args = default_cli_args();
        args.refresh_cookie_name = " preview_refresh ".to_string();
        let config = AppConfig::from_cli(args);
        assert_eq!(config.auth.refresh_cookie_name, "preview_refresh");
        config.validate_startup();
    }

    #[test]
    #[should_panic(expected = "G5_REFRESH_COOKIE_NAME must be 1-64 ASCII letters")]
    fn test_validate_startup_rejects_a_refresh_cookie_name_that_is_not_a_token() {
        let mut args = default_cli_args();
        args.refresh_cookie_name = "bad;name".to_string();
        AppConfig::from_cli(args).validate_startup();
    }

    #[test]
    fn test_insecure_refresh_cookie_rejects_reserved_secure_prefixes() {
        for name in [
            "__Host-refresh",
            "__Secure-refresh",
            "__hOsT-refresh",
            "__sEcUrE-refresh",
        ] {
            let mut args = default_cli_args();
            args.cookie_secure = false;
            args.refresh_cookie_name = name.to_string();
            let config = AppConfig::from_cli(args);
            assert!(std::panic::catch_unwind(|| config.validate_startup()).is_err());
        }
    }

    #[test]
    fn test_git_external_url_override() {
        let mut args = default_cli_args();
        args.git_external_url = Some("http://g5-api:7500".to_string());

        let config = AppConfig::from_cli(args);
        assert_eq!(config.server.external_url, "http://localhost:8080");
        assert_eq!(config.server.git_external_url, "http://g5-api:7500");
    }

    #[test]
    fn test_oauth_provider_both_present() {
        let mut args = default_cli_args();
        args.github_client_id = Some("gh-id".to_string());
        args.github_client_secret = Some("gh-secret".to_string());

        let config = AppConfig::from_cli(args);
        let gh = config.auth.github.as_ref().unwrap();
        assert_eq!(gh.client_id, "gh-id");
        assert_eq!(gh.client_secret, "gh-secret");
        assert!(config.auth.has_oauth());
        assert_eq!(config.auth.enabled_providers(), vec!["github"]);
    }

    #[test]
    fn test_microsoft_oauth_requires_tenant() {
        let mut args = default_cli_args();
        args.microsoft_client_id = Some("ms-id".to_string());
        args.microsoft_client_secret = Some("ms-secret".to_string());

        let config = AppConfig::from_cli(args);

        assert!(config.auth.microsoft.is_none());
        assert!(config.auth.microsoft_tenant_id.is_none());
        assert!(!config.auth.has_oauth());
        assert!(config.auth.enabled_providers().is_empty());
    }

    #[test]
    fn test_microsoft_oauth_rejects_multi_tenant_aliases() {
        for alias in [
            "common",
            "Organizations",
            " /consumers/ ",
            "tenant.example.com",
            "tenant",
            "00000000-0000-0000-0000-000000000000",
        ] {
            let mut args = default_cli_args();
            args.microsoft_client_id = Some("ms-id".to_string());
            args.microsoft_client_secret = Some("ms-secret".to_string());
            args.microsoft_tenant_id = Some(alias.to_string());

            let config = AppConfig::from_cli(args);

            assert!(config.auth.microsoft.is_none(), "{alias}");
            assert!(config.auth.microsoft_tenant_id.is_none(), "{alias}");
            assert!(config.auth.enabled_providers().is_empty(), "{alias}");
        }
    }

    #[test]
    fn test_microsoft_oauth_provider_with_tenant() {
        let mut args = default_cli_args();
        args.microsoft_client_id = Some(" ms-id ".to_string());
        args.microsoft_client_secret = Some(" ms-secret ".to_string());
        args.microsoft_tenant_id = Some(" 72f988bf-86f1-41af-91ab-2d7cd011db47 ".to_string());
        args.microsoft_authorize_url = Some(" ".to_string());
        args.microsoft_token_url = Some("\t".to_string());
        args.microsoft_jwks_url = Some(String::new());

        let config = AppConfig::from_cli(args);
        let microsoft = config.auth.microsoft.as_ref().unwrap();

        assert_eq!(microsoft.client_id, "ms-id");
        assert_eq!(microsoft.client_secret, "ms-secret");
        assert_eq!(microsoft.authorize_url, None);
        assert_eq!(microsoft.token_url, None);
        assert_eq!(microsoft.userinfo_url, None);
        assert_eq!(
            config.auth.microsoft_tenant_id.as_deref(),
            Some("72f988bf-86f1-41af-91ab-2d7cd011db47")
        );
        assert!(config.auth.has_oauth());
        assert_eq!(config.auth.enabled_providers(), vec!["microsoft"]);
    }

    #[test]
    fn test_github_and_microsoft_login_are_both_enabled() {
        let mut args = default_cli_args();
        args.github_client_id = Some("gh-id".to_string());
        args.github_client_secret = Some("gh-secret".to_string());
        args.microsoft_client_id = Some("ms-id".to_string());
        args.microsoft_client_secret = Some("ms-secret".to_string());
        args.microsoft_tenant_id = Some("72f988bf-86f1-41af-91ab-2d7cd011db47".to_string());

        let config = AppConfig::from_cli(args);

        assert!(config.auth.github.is_some());
        assert!(config.auth.microsoft.is_some());
        assert_eq!(config.auth.enabled_providers(), vec!["github", "microsoft"]);
    }

    #[test]
    fn test_scm_credentials_do_not_enable_platform_login() {
        let mut args = default_cli_args();
        args.scm_gitlab_client_id = Some("scm-id".into());
        args.scm_gitlab_client_secret = Some("scm-secret".into());
        args.scm_gitlab_authorize_url = Some("https://scm.example/oauth/authorize".into());
        args.scm_gitlab_token_url = Some("https://scm.example/oauth/token".into());
        args.scm_gitlab_userinfo_url = Some("https://scm.example/api/v4/user".into());
        let config = AppConfig::from_cli(args);
        assert!(!config.auth.has_oauth());
        assert!(config.auth.enabled_providers().is_empty());
        let scm = config.auth.scm_gitlab.unwrap();
        assert_eq!(scm.client_id, "scm-id");
        assert_eq!(
            scm.authorize_url.as_deref(),
            Some("https://scm.example/oauth/authorize")
        );
        assert_eq!(
            scm.token_url.as_deref(),
            Some("https://scm.example/oauth/token")
        );
        assert_eq!(
            scm.userinfo_url.as_deref(),
            Some("https://scm.example/api/v4/user")
        );
    }

    #[test]
    fn gitlab_origin_survives_missing_oauth_credentials() {
        for credentials in [(None, None), (Some("id"), None), (None, Some("secret"))] {
            let mut args = default_cli_args();
            args.scm_gitlab_client_id = credentials.0.map(str::to_string);
            args.scm_gitlab_client_secret = credentials.1.map(str::to_string);
            args.scm_gitlab_userinfo_url = Some("https://gitlab.internal:8443/api/v4/user".into());
            let config = AppConfig::from_cli(args);
            assert!(config.auth.scm_gitlab.is_none());
            assert_eq!(
                config.auth.scm_gitlab_api_url(),
                "https://gitlab.internal:8443/api/v4/user"
            );
        }
    }

    #[test]
    fn test_oauth_provider_only_id_present() {
        let mut args = default_cli_args();
        args.github_client_id = Some("gh-id".to_string());
        // github_client_secret is None

        let config = AppConfig::from_cli(args);
        assert!(config.auth.github.is_none());
        assert!(!config.auth.has_oauth());
    }

    #[test]
    fn test_oauth_provider_only_secret_present() {
        let mut args = default_cli_args();
        args.github_client_secret = Some("gh-secret".to_string());
        // github_client_id is None

        let config = AppConfig::from_cli(args);
        assert!(config.auth.github.is_none());
    }

    #[test]
    fn test_oauth_provider_blank_values_ignored() {
        let mut args = default_cli_args();
        args.github_client_id = Some(" gh-id ".to_string());
        args.github_client_secret = Some(" gh-secret ".to_string());

        let config = AppConfig::from_cli(args);

        let gh = config.auth.github.as_ref().unwrap();
        assert_eq!(gh.client_id, "gh-id");
        assert_eq!(gh.client_secret, "gh-secret");
        assert_eq!(config.auth.enabled_providers(), vec!["github"]);
    }

    #[test]
    fn test_cors_comma_separated() {
        let mut args = default_cli_args();
        args.cors_allowed_origins =
            "http://localhost:5173, https://app.g5.dev , https://staging.g5.dev".to_string();

        let config = AppConfig::from_cli(args);
        assert_eq!(
            config.cors.allowed_origins,
            vec![
                "http://localhost:5173",
                "https://app.g5.dev",
                "https://staging.g5.dev",
            ]
        );
    }

    #[test]
    fn test_cors_empty_segments_filtered() {
        let mut args = default_cli_args();
        args.cors_allowed_origins = "http://localhost:5173,,, ".to_string();

        let config = AppConfig::from_cli(args);
        assert_eq!(config.cors.allowed_origins, vec!["http://localhost:5173"]);
    }

    #[test]
    fn test_refresh_token_ttl_days_to_seconds() {
        let mut args = default_cli_args();
        args.jwt_refresh_token_ttl_days = 7;

        let config = AppConfig::from_cli(args);
        assert_eq!(
            config.auth.refresh_token_ttl,
            Duration::from_secs(7 * 86400)
        );
    }

    #[test]
    fn test_app_config_is_clone() {
        let args = default_cli_args();
        let config = AppConfig::from_cli(args);
        let _cloned = config.clone();
    }

    #[test]
    fn test_app_config_is_debug() {
        let args = default_cli_args();
        let config = AppConfig::from_cli(args);
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("AppConfig"));
    }

    #[test]
    fn test_cli_args_is_debug() {
        let args = default_cli_args();
        let debug_str = format!("{:?}", args);
        assert!(debug_str.contains("CliArgs"));
    }

    #[test]
    fn test_scm_encryption_key_propagated() {
        let mut args = default_cli_args();
        args.scm_encryption_key = Some("aes256-key-here".to_string());

        let config = AppConfig::from_cli(args);
        assert_eq!(
            config.auth.scm_encryption_key.as_deref(),
            Some("aes256-key-here")
        );
    }

    #[test]
    fn test_chat_response_timeout_default() {
        let args = default_cli_args();
        let config = AppConfig::from_cli(args);
        assert_eq!(config.chat.response_timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_chat_response_timeout_custom() {
        let mut args = default_cli_args();
        args.chat_response_timeout_secs = 60;
        let config = AppConfig::from_cli(args);
        assert_eq!(config.chat.response_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_chat_response_timeout_minimum_ten_seconds() {
        let mut args = default_cli_args();
        args.chat_response_timeout_secs = 0;
        let config = AppConfig::from_cli(args);
        assert_eq!(config.chat.response_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_authz_database_defaults_to_main_url() {
        let args = default_cli_args();
        let config = AppConfig::from_cli(args);
        // When authz_database_url is None, should fall back to database_url
        assert_eq!(config.authz_database.url, config.database.url);
        assert_eq!(config.authz_database.max_connections, 5);
        assert_eq!(config.authz_database.min_connections, 1);
    }

    #[test]
    fn test_authz_database_explicit_url() {
        let mut args = default_cli_args();
        args.authz_database_url = Some("postgres://authz@localhost:5432/g5".to_string());
        args.authz_database_max_connections = 10;
        let config = AppConfig::from_cli(args);
        assert_eq!(
            config.authz_database.url,
            "postgres://authz@localhost:5432/g5"
        );
        assert_eq!(config.authz_database.max_connections, 10);
        // main database URL should be unaffected
        assert_eq!(
            config.database.url,
            "postgres://user:pass@localhost:5432/g5"
        );
    }
}
