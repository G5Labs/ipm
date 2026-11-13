//! Organization bindings to the G5-owned GitHub App installation.
//!
//! Durable state (migration 026) is append-only: every connection, coverage
//! change, installation refresh and disconnect appends a new binding
//! snapshot, and the newest
//! version per organization is authoritative. All reads and writes run in a
//! tenant [`ScopedDb`], so RLS confines them to the organization.
//!
//! The resolver at the bottom is the entry point for later integrations:
//! given an organization, a GitHub repository id and a [`GithubAppCapability`],
//! it verifies the latest binding is active, covers the repository and was
//! granted the permission, then mints an installation token narrowed to that
//! single repository and the permissions needed by the operation.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use serde_json::{json, Value};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::services::git::github_app::{
    connection_state_digest, installation_url, new_connection_state, unix_now,
    ConnectionAttemptRef, GithubAppClient, GithubAppConfig, GithubAppInstallation,
    GithubAppRepository, InstallationTokenRequest, CONNECTION_ATTEMPT_TTL,
};

pub const DISPOSITION_ACTIVE: &str = "active";
pub const DISPOSITION_DISCONNECTED: &str = "disconnected";
const PROVENANCE_COVERAGE_CHANGE: &str = "coverage_change";
const PROVENANCE_INSTALLATION_REFRESH: &str = "installation_refresh";
const IDENTITY_KIND_PLATFORM_MANAGED: &str = "platform_managed";

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// Operation-level GitHub capabilities. The binding's recorded permission
/// snapshot must grant every required permission. Minted tokens are narrowed
/// to that set plus each optional permission the snapshot also grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GithubAppCapability {
    IssuesRead,
    IssuesWrite,
    ContentsRead,
    ContentsWrite,
    PullRequestsRead,
    PullRequestChecksRead,
    PullRequestsWrite,
}

impl GithubAppCapability {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IssuesRead => "issues_read",
            Self::IssuesWrite => "issues_write",
            Self::ContentsRead => "contents_read",
            Self::ContentsWrite => "contents_write",
            Self::PullRequestsRead => "pull_requests_read",
            Self::PullRequestChecksRead => "pull_request_checks_read",
            Self::PullRequestsWrite => "pull_requests_write",
        }
    }

    fn required_permissions(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::IssuesRead => &[("issues", "read")],
            Self::IssuesWrite => &[("issues", "write")],
            Self::ContentsRead => &[("contents", "read")],
            Self::ContentsWrite => &[("contents", "write")],
            // Review and review-comment `commit{oid}` fields resolve Git
            // objects: without contents read GitHub answers the PR review
            // GraphQL reads with "Resource not accessible by integration".
            Self::PullRequestsRead => &[("pull_requests", "read"), ("contents", "read")],
            // The base branch's required checks come from its protection,
            // which GitHub serves under Contents read.
            Self::PullRequestChecksRead => &[
                ("pull_requests", "read"),
                ("contents", "read"),
                ("checks", "read"),
                ("statuses", "read"),
            ],
            // Creating a PR must also be able to read its head and base refs.
            // A PR-write-only installation token can list PRs but GitHub
            // rejects creation with 422 "not all refs are readable".
            Self::PullRequestsWrite => &[("pull_requests", "write"), ("contents", "read")],
        }
    }

    /// Permissions requested only when the snapshot grants them; they never
    /// make a capability ineligible. GitHub refuses any Git write whose new
    /// commits create or update `.github/workflows/*` (including a merge)
    /// without `workflows`, so the installation's grant decides whether G5
    /// may publish workflow changes (`g5.git.workflow-files`).
    fn optional_permissions(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::ContentsWrite => &[("workflows", "write")],
            _ => &[],
        }
    }

    /// Whether a recorded installation permission snapshot grants this
    /// capability. `write` satisfies `read`; anything else fails closed.
    #[must_use]
    pub fn is_satisfied_by(self, permissions: &Value) -> bool {
        self.required_permissions()
            .iter()
            .all(|(key, required)| grants(permissions, key, required))
    }

    /// Token-request permissions narrowed to this capability under the
    /// recorded `granted` snapshot: every required permission, plus each
    /// optional one the snapshot grants.
    #[must_use]
    pub fn token_permissions(self, granted: &Value) -> BTreeMap<String, String> {
        self.required_permissions()
            .iter()
            .chain(
                self.optional_permissions()
                    .iter()
                    .filter(|(key, level)| grants(granted, key, level)),
            )
            .map(|(key, level)| (key.to_string(), level.to_string()))
            .collect()
    }
}

/// `write` satisfies `read`; anything else fails closed.
fn grants(permissions: &Value, key: &str, required: &str) -> bool {
    let level = permissions
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    matches!(
        (required, level.as_deref()),
        ("write", Some("write")) | ("read", Some("read" | "write"))
    )
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// One binding snapshot.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GithubAppBindingRecord {
    pub id: Uuid,
    pub app_identity_id: Uuid,
    pub version: i64,
    pub disposition: String,
    pub installation_id: i64,
    pub account_id: i64,
    pub account_login: String,
    pub permissions: Value,
    pub provenance: String,
    pub created_at: DateTime<Utc>,
}

impl GithubAppBindingRecord {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.disposition == DISPOSITION_ACTIVE
    }

    /// Installation id as GitHub's unsigned identifier.
    #[must_use]
    pub fn github_installation_id(&self) -> u64 {
        u64::try_from(self.installation_id).unwrap_or_default()
    }
}

/// A GitHub repository covered by a binding snapshot.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GithubAppCoveredRepository {
    pub github_repo_id: i64,
    pub full_name: String,
}

/// The latest binding with its coverage, for the admin status view. This is
/// recorded state; it does not prove the installation is still reachable.
#[derive(Debug, Clone)]
pub struct GithubAppBindingStatus {
    pub binding: GithubAppBindingRecord,
    pub covered_repositories: Vec<GithubAppCoveredRepository>,
}

/// A newly created connection attempt. The raw state exists only in the
/// installation URL handed to the administrator's browser.
#[derive(Clone)]
pub struct GithubAppConnectionAttempt {
    pub attempt_id: Uuid,
    pub expires_at: i64,
    state: String,
}

impl std::fmt::Debug for GithubAppConnectionAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubAppConnectionAttempt")
            .field("attempt_id", &self.attempt_id)
            .field("expires_at", &self.expires_at)
            .field("state", &"[REDACTED]")
            .finish()
    }
}

impl GithubAppConnectionAttempt {
    /// The signed state (a single-use browser capability).
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }

    #[must_use]
    pub fn installation_url(&self, cfg: &GithubAppConfig) -> String {
        installation_url(cfg, &self.state)
    }
}

/// A connection attempt proven by signed state and matched against its
/// stored digest. It is still pending: unexpired and not yet consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConnectionAttempt {
    pub id: Uuid,
    pub org_id: Uuid,
    pub app_identity_id: Uuid,
    pub created_by: Uuid,
}

fn db_error(context: &str, error: &sqlx::Error) -> G5Error {
    G5Error::Internal {
        message: format!("github app binding {context} failed: {error}"),
    }
}

fn binding_changed() -> G5Error {
    G5Error::Conflict {
        reason: "github app binding changed or is unavailable; reload and retry".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Connection attempts
// ---------------------------------------------------------------------------

/// Get or create the organization's platform-managed App identity.
pub async fn ensure_platform_managed_identity(
    db: &mut ScopedDb,
    org_id: Uuid,
) -> Result<Uuid, G5Error> {
    sqlx::query(
        r#"
        INSERT INTO github_app_identities (org_id, kind)
        VALUES ($1, $2)
        ON CONFLICT (org_id, kind) DO NOTHING
        "#,
    )
    .bind(org_id)
    .bind(IDENTITY_KIND_PLATFORM_MANAGED)
    .execute(db.conn())
    .await
    .map_err(|error| db_error("identity insert", &error))?;

    sqlx::query_scalar("SELECT id FROM github_app_identities WHERE org_id = $1 AND kind = $2")
        .bind(org_id)
        .bind(IDENTITY_KIND_PLATFORM_MANAGED)
        .fetch_one(db.conn())
        .await
        .map_err(|error| db_error("identity lookup", &error))
}

/// Record an organization administrator's connection attempt and issue its
/// signed state. Only the SHA-256 digest of the state is stored.
pub async fn create_connection_attempt(
    db: &mut ScopedDb,
    cfg: &GithubAppConfig,
    org_id: Uuid,
    created_by: Uuid,
) -> Result<GithubAppConnectionAttempt, G5Error> {
    let app_identity_id = ensure_platform_managed_identity(db, org_id).await?;
    let attempt_id = Uuid::new_v4();
    let ttl = i64::try_from(CONNECTION_ATTEMPT_TTL.as_secs()).unwrap_or(600);
    let expires_at = unix_now() + ttl;
    let state = new_connection_state(cfg, org_id, attempt_id, expires_at);
    let digest = connection_state_digest(&state);
    sqlx::query(
        r#"
        INSERT INTO github_app_connection_attempts
            (id, org_id, app_identity_id, state_digest, created_by, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(attempt_id)
    .bind(org_id)
    .bind(app_identity_id)
    .bind(digest.as_slice())
    .bind(created_by)
    .bind(
        DateTime::<Utc>::from_timestamp(expires_at, 0).ok_or_else(|| G5Error::Internal {
            message: "github app attempt expiry out of range".to_string(),
        })?,
    )
    .execute(db.conn())
    .await
    .map_err(|error| db_error("connection attempt insert", &error))?;
    Ok(GithubAppConnectionAttempt {
        attempt_id,
        expires_at,
        state,
    })
}

#[derive(sqlx::FromRow)]
struct AttemptRow {
    id: Uuid,
    org_id: Uuid,
    app_identity_id: Uuid,
    created_by: Uuid,
    state_digest: Vec<u8>,
    live: bool,
    consumed: bool,
}

/// Load a signed-state attempt under the org's RLS scope. Returns `None`
/// unless its stored digest matches (constant time), it is unexpired, and
/// no binding has consumed it.
pub async fn load_pending_attempt(
    db: &mut ScopedDb,
    attempt: &ConnectionAttemptRef,
) -> Result<Option<PendingConnectionAttempt>, G5Error> {
    let row: Option<AttemptRow> = sqlx::query_as(
        r#"
        SELECT attempt.id, attempt.org_id, attempt.app_identity_id, attempt.created_by,
               attempt.state_digest,
               attempt.expires_at > now() AS live,
               EXISTS (
                   SELECT 1 FROM github_app_installation_bindings AS binding
                   WHERE binding.org_id = attempt.org_id
                     AND binding.connection_attempt_id = attempt.id
               ) AS consumed
        FROM github_app_connection_attempts AS attempt
        WHERE attempt.org_id = $1 AND attempt.id = $2
        "#,
    )
    .bind(attempt.org_id)
    .bind(attempt.attempt_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("connection attempt lookup", &error))?;

    Ok(row.and_then(|row| {
        let digest_matches: bool = row
            .state_digest
            .as_slice()
            .ct_eq(attempt.state_digest.as_slice())
            .into();
        (digest_matches && row.live && !row.consumed).then_some(PendingConnectionAttempt {
            id: row.id,
            org_id: row.org_id,
            app_identity_id: row.app_identity_id,
            created_by: row.created_by,
        })
    }))
}

/// Whether `user_id` is currently an admin of `org_id`. Membership
/// rows are not visible to the tenant role, so this reads the system pool by
/// exact key.
pub async fn is_org_admin(
    system_pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<bool, G5Error> {
    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL")
            .bind(org_id)
            .bind(user_id)
            .fetch_optional(system_pool)
            .await
            .map_err(|error| db_error("org role lookup", &error))?;
    Ok(matches!(role.as_deref(), Some("admin")))
}

/// The organization's URL slug, used to return the browser to its settings.
pub async fn org_slug(system_pool: &PgPool, org_id: Uuid) -> Result<Option<String>, G5Error> {
    sqlx::query_scalar("SELECT slug FROM orgs WHERE id = $1")
        .bind(org_id)
        .fetch_optional(system_pool)
        .await
        .map_err(|error| db_error("org slug lookup", &error))
}

// ---------------------------------------------------------------------------
// Binding snapshots
// ---------------------------------------------------------------------------

async fn lock_org_bindings(db: &mut ScopedDb, org_id: Uuid) -> Result<(), G5Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('github_app_binding:' || $1::text))")
        .bind(org_id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("version lock", &error))?;
    Ok(())
}

/// The newest binding snapshot, if any.
pub async fn load_latest_binding(
    db: &mut ScopedDb,
    org_id: Uuid,
) -> Result<Option<GithubAppBindingRecord>, G5Error> {
    sqlx::query_as(
        r#"
        SELECT id, app_identity_id, version, disposition, installation_id, account_id,
               account_login, permissions, provenance, created_at
        FROM github_app_installation_bindings
        WHERE org_id = $1
        ORDER BY version DESC
        LIMIT 1
        "#,
    )
    .bind(org_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("latest binding lookup", &error))
}

/// Repositories covered by one snapshot, ordered by name.
pub async fn load_binding_coverage(
    db: &mut ScopedDb,
    org_id: Uuid,
    binding_id: Uuid,
) -> Result<Vec<GithubAppCoveredRepository>, G5Error> {
    sqlx::query_as(
        r#"
        SELECT github_repo_id, full_name
        FROM github_app_installation_binding_repositories
        WHERE org_id = $1 AND binding_id = $2
        ORDER BY lower(full_name), github_repo_id
        "#,
    )
    .bind(org_id)
    .bind(binding_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("coverage lookup", &error))
}

/// The newest snapshot and its coverage for the admin status view.
pub async fn load_binding_status(
    db: &mut ScopedDb,
    org_id: Uuid,
) -> Result<Option<GithubAppBindingStatus>, G5Error> {
    let Some(binding) = load_latest_binding(db, org_id).await? else {
        return Ok(None);
    };
    let covered_repositories = load_binding_coverage(db, org_id, binding.id).await?;
    Ok(Some(GithubAppBindingStatus {
        binding,
        covered_repositories,
    }))
}

#[allow(clippy::too_many_arguments)]
async fn append_snapshot(
    db: &mut ScopedDb,
    org_id: Uuid,
    app_identity_id: Uuid,
    version: i64,
    disposition: &str,
    installation_id: i64,
    account_id: i64,
    account_login: &str,
    permissions: &Value,
    provenance: &str,
    connection_attempt_id: Option<Uuid>,
    created_by: Uuid,
    coverage: &[GithubAppCoveredRepository],
) -> Result<GithubAppBindingRecord, G5Error> {
    let record: GithubAppBindingRecord = sqlx::query_as(
        r#"
        INSERT INTO github_app_installation_bindings
            (org_id, app_identity_id, version, disposition, installation_id, account_id,
             account_login, permissions, provenance, connection_attempt_id, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, app_identity_id, version, disposition, installation_id, account_id,
                  account_login, permissions, provenance, created_at
        "#,
    )
    .bind(org_id)
    .bind(app_identity_id)
    .bind(version)
    .bind(disposition)
    .bind(installation_id)
    .bind(account_id)
    .bind(account_login)
    .bind(permissions)
    .bind(provenance)
    .bind(connection_attempt_id)
    .bind(created_by)
    .fetch_one(db.conn())
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => binding_changed(),
        _ => db_error("snapshot append", &error),
    })?;

    for repository in coverage {
        sqlx::query(
            r#"
            INSERT INTO github_app_installation_binding_repositories
                (org_id, binding_id, github_repo_id, full_name)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(org_id)
        .bind(record.id)
        .bind(repository.github_repo_id)
        .bind(&repository.full_name)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("coverage append", &error))?;
    }
    Ok(record)
}

/// Append the snapshot for a verified installation. Coverage starts empty
/// (fail closed) until an administrator selects repositories. The attempt is
/// re-checked under the org lock and consumed by the binding's unique
/// `connection_attempt_id`. Provenance is `reinstall` when the org already
/// had a binding, otherwise `installation_callback`.
pub async fn append_verified_binding(
    db: &mut ScopedDb,
    attempt: &PendingConnectionAttempt,
    installation: &GithubAppInstallation,
) -> Result<GithubAppBindingRecord, G5Error> {
    lock_org_bindings(db, attempt.org_id).await?;
    let still_pending: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM github_app_connection_attempts AS attempt
            WHERE attempt.org_id = $1 AND attempt.id = $2 AND attempt.expires_at > now()
              AND NOT EXISTS (
                  SELECT 1 FROM github_app_installation_bindings AS binding
                  WHERE binding.org_id = attempt.org_id
                    AND binding.connection_attempt_id = attempt.id
              )
        )
        "#,
    )
    .bind(attempt.org_id)
    .bind(attempt.id)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("connection attempt recheck", &error))?;
    if !still_pending {
        return Err(G5Error::Unauthorized);
    }
    let installation_id =
        i64::try_from(installation.installation_id).map_err(|_| G5Error::ValidationError {
            details: json!({"field": "installationId", "error": "out of range"}),
        })?;
    let latest = load_latest_binding(db, attempt.org_id).await?;
    let (version, provenance) = match &latest {
        Some(binding) => (binding.version + 1, "reinstall"),
        None => (1, "installation_callback"),
    };
    append_snapshot(
        db,
        attempt.org_id,
        attempt.app_identity_id,
        version,
        DISPOSITION_ACTIVE,
        installation_id,
        installation.account_id,
        &installation.account_login,
        &json!(installation.permissions),
        provenance,
        Some(attempt.id),
        attempt.created_by,
        &[],
    )
    .await
}

/// The latest snapshot when it is active at `expected_version` and records
/// the same GitHub installation as `installation`.
async fn latest_active_for_installation(
    db: &mut ScopedDb,
    org_id: Uuid,
    expected_version: i64,
    installation: &GithubAppInstallation,
) -> Result<GithubAppBindingRecord, G5Error> {
    let latest = load_latest_binding(db, org_id)
        .await?
        .filter(|binding| binding.is_active() && binding.version == expected_version)
        .ok_or_else(binding_changed)?;
    if latest.github_installation_id() != installation.installation_id {
        return Err(G5Error::Conflict {
            reason: "github app installation changed; reconnect the binding".to_string(),
        });
    }
    Ok(latest)
}

/// Whether GitHub's current installation details differ from a snapshot.
/// Permission levels and the account are compared; GitHub owns both and they
/// change outside G5 (for example when an owner accepts new permissions).
fn installation_differs(
    latest: &GithubAppBindingRecord,
    installation: &GithubAppInstallation,
) -> bool {
    latest.account_id != installation.account_id
        || latest.account_login != installation.account_login
        || latest.permissions != json!(installation.permissions)
}

/// Append a snapshot with exactly `coverage` and GitHub's current
/// installation details when `expected_version` is the current active
/// binding. Callers validate `coverage` against the installation's live
/// repository list and fetch `installation` first. Unchanged coverage and
/// details are a no-op; unchanged coverage with changed details records an
/// installation refresh.
pub async fn append_coverage_change(
    db: &mut ScopedDb,
    org_id: Uuid,
    created_by: Uuid,
    expected_version: i64,
    installation: &GithubAppInstallation,
    coverage: &[GithubAppCoveredRepository],
) -> Result<GithubAppBindingRecord, G5Error> {
    lock_org_bindings(db, org_id).await?;
    let latest = latest_active_for_installation(db, org_id, expected_version, installation).await?;
    let current: BTreeSet<i64> = load_binding_coverage(db, org_id, latest.id)
        .await?
        .into_iter()
        .map(|repository| repository.github_repo_id)
        .collect();
    let requested: BTreeSet<i64> = coverage.iter().map(|repo| repo.github_repo_id).collect();
    let coverage_changed = current != requested;
    if !coverage_changed && !installation_differs(&latest, installation) {
        return Ok(latest);
    }
    let provenance = if coverage_changed {
        PROVENANCE_COVERAGE_CHANGE
    } else {
        PROVENANCE_INSTALLATION_REFRESH
    };
    append_snapshot(
        db,
        org_id,
        latest.app_identity_id,
        latest.version + 1,
        DISPOSITION_ACTIVE,
        latest.installation_id,
        installation.account_id,
        &installation.account_login,
        &json!(installation.permissions),
        provenance,
        None,
        created_by,
        coverage,
    )
    .await
}

/// Record GitHub's current installation permissions and account while
/// keeping coverage, when `expected_version` is the current active binding.
/// Returns `None` when nothing changed, so a refresh never adds an empty
/// snapshot.
pub async fn append_installation_refresh(
    db: &mut ScopedDb,
    org_id: Uuid,
    created_by: Uuid,
    expected_version: i64,
    installation: &GithubAppInstallation,
) -> Result<Option<GithubAppBindingRecord>, G5Error> {
    lock_org_bindings(db, org_id).await?;
    let latest = latest_active_for_installation(db, org_id, expected_version, installation).await?;
    if !installation_differs(&latest, installation) {
        return Ok(None);
    }
    let coverage = load_binding_coverage(db, org_id, latest.id).await?;
    append_snapshot(
        db,
        org_id,
        latest.app_identity_id,
        latest.version + 1,
        DISPOSITION_ACTIVE,
        latest.installation_id,
        installation.account_id,
        &installation.account_login,
        &json!(installation.permissions),
        PROVENANCE_INSTALLATION_REFRESH,
        None,
        created_by,
        &coverage,
    )
    .await
    .map(Some)
}

/// Append a disconnected snapshot with no coverage. Older snapshots remain as
/// history; the resolver refuses every request while this is the latest.
pub async fn append_disconnect(
    db: &mut ScopedDb,
    org_id: Uuid,
    created_by: Uuid,
    expected_version: i64,
) -> Result<GithubAppBindingRecord, G5Error> {
    lock_org_bindings(db, org_id).await?;
    let latest = load_latest_binding(db, org_id)
        .await?
        .filter(|binding| binding.is_active() && binding.version == expected_version)
        .ok_or_else(binding_changed)?;
    append_snapshot(
        db,
        org_id,
        latest.app_identity_id,
        latest.version + 1,
        DISPOSITION_DISCONNECTED,
        latest.installation_id,
        latest.account_id,
        &latest.account_login,
        &latest.permissions,
        "disconnect",
        None,
        created_by,
        &[],
    )
    .await
}

// ---------------------------------------------------------------------------
// Coverage validation
// ---------------------------------------------------------------------------

/// Why a requested coverage set was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageSelectionError {
    DuplicateRepository(i64),
    RepositoryNotAccessible(i64),
}

impl From<CoverageSelectionError> for G5Error {
    fn from(error: CoverageSelectionError) -> Self {
        let (repo, message) = match error {
            CoverageSelectionError::DuplicateRepository(id) => {
                (id, "repository ids must be unique")
            }
            CoverageSelectionError::RepositoryNotAccessible(id) => (
                id,
                "every selected repository must be accessible to the GitHub App installation",
            ),
        };
        G5Error::ValidationError {
            details: json!({
                "field": "githubRepoIds",
                "githubRepoId": repo,
                "error": message,
            }),
        }
    }
}

/// Validate requested GitHub repository ids against the installation's live
/// repository list. Returns the coverage with GitHub's current full names.
pub fn validate_coverage_selection(
    requested: &[i64],
    accessible: &[GithubAppRepository],
) -> Result<Vec<GithubAppCoveredRepository>, CoverageSelectionError> {
    let mut seen = BTreeSet::new();
    let mut coverage = Vec::with_capacity(requested.len());
    for &github_repo_id in requested {
        if !seen.insert(github_repo_id) {
            return Err(CoverageSelectionError::DuplicateRepository(github_repo_id));
        }
        let repository = accessible
            .iter()
            .find(|repository| repository.github_repo_id == github_repo_id)
            .ok_or(CoverageSelectionError::RepositoryNotAccessible(
                github_repo_id,
            ))?;
        coverage.push(GithubAppCoveredRepository {
            github_repo_id,
            full_name: repository.full_name.clone(),
        });
    }
    coverage.sort_by_key(|repository| repository.github_repo_id);
    Ok(coverage)
}

// ---------------------------------------------------------------------------
// Repository token resolver
// ---------------------------------------------------------------------------

/// An eligible (binding, repository) pair for one capability. Contains no
/// credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubAppRepositorySelection {
    pub binding_id: Uuid,
    pub binding_version: i64,
    pub installation_id: u64,
    pub github_repo_id: u64,
    pub capability: GithubAppCapability,
    /// The exact token permissions, resolved from the snapshot that made
    /// this selection eligible.
    pub permissions: BTreeMap<String, String>,
}

/// Why the organization's binding cannot serve a repository/capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubAppIneligibleReason {
    NoBinding,
    BindingInactive,
    RepositoryUncovered,
    CapabilityMissing { required: GithubAppCapability },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubAppSelectionPreflight {
    Eligible(GithubAppRepositorySelection),
    Ineligible(GithubAppIneligibleReason),
}

/// A minted installation token narrowed to one repository and capability.
#[derive(Clone)]
pub struct GithubAppRepositoryToken {
    pub token: String,
    pub selection: GithubAppRepositorySelection,
}

impl std::fmt::Debug for GithubAppRepositoryToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubAppRepositoryToken")
            .field("token", &"[REDACTED]")
            .field("selection", &self.selection)
            .finish()
    }
}

impl std::fmt::Display for GithubAppIneligibleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoBinding => write!(f, "the organization has no GitHub App binding"),
            Self::BindingInactive => {
                write!(f, "the organization's GitHub App binding is not active")
            }
            Self::RepositoryUncovered => {
                write!(f, "the repository is not covered by the GitHub App binding")
            }
            // The snapshot records permissions as of the last connect,
            // coverage change or refresh; an owner may since have accepted
            // new permissions on GitHub.
            Self::CapabilityMissing { required } => {
                write!(
                f,
                "the GitHub App binding does not record the required {} permissions; if granted \
                 on GitHub, refresh the binding from GitHub",
                required.required_permissions().iter()
                    .map(|(key, level)| format!("{key} {level}"))
                    .collect::<Vec<_>>().join(" and ")
            )
            }
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SelectionRow {
    binding_id: Uuid,
    binding_version: i64,
    installation_id: i64,
    disposition: String,
    permissions: Value,
    repository_covered: bool,
}

fn classify_selection(
    row: Option<SelectionRow>,
    github_repo_id: u64,
    capability: GithubAppCapability,
) -> GithubAppSelectionPreflight {
    use GithubAppIneligibleReason as Reason;
    let Some(row) = row else {
        return GithubAppSelectionPreflight::Ineligible(Reason::NoBinding);
    };
    if row.disposition != DISPOSITION_ACTIVE {
        return GithubAppSelectionPreflight::Ineligible(Reason::BindingInactive);
    }
    if !row.repository_covered {
        return GithubAppSelectionPreflight::Ineligible(Reason::RepositoryUncovered);
    }
    if !capability.is_satisfied_by(&row.permissions) {
        return GithubAppSelectionPreflight::Ineligible(Reason::CapabilityMissing {
            required: capability,
        });
    }
    let Ok(installation_id) = u64::try_from(row.installation_id) else {
        return GithubAppSelectionPreflight::Ineligible(Reason::BindingInactive);
    };
    GithubAppSelectionPreflight::Eligible(GithubAppRepositorySelection {
        binding_id: row.binding_id,
        binding_version: row.binding_version,
        installation_id,
        github_repo_id,
        capability,
        permissions: capability.token_permissions(&row.permissions),
    })
}

/// Database-only preflight: is the org's latest binding active, covering
/// `github_repo_id`, with a permission snapshot granting `capability`?
pub async fn lookup_repository_selection(
    db: &mut ScopedDb,
    org_id: Uuid,
    github_repo_id: u64,
    capability: GithubAppCapability,
) -> Result<GithubAppSelectionPreflight, G5Error> {
    let Ok(repo_id) = i64::try_from(github_repo_id) else {
        return Ok(GithubAppSelectionPreflight::Ineligible(
            GithubAppIneligibleReason::RepositoryUncovered,
        ));
    };
    let row: Option<SelectionRow> = sqlx::query_as(
        r#"
        WITH latest AS (
            SELECT id, org_id, version, installation_id, disposition, permissions
            FROM github_app_installation_bindings
            WHERE org_id = $1
            ORDER BY version DESC
            LIMIT 1
        )
        SELECT latest.id AS binding_id,
               latest.version AS binding_version,
               latest.installation_id,
               latest.disposition,
               latest.permissions,
               EXISTS (
                   SELECT 1 FROM github_app_installation_binding_repositories AS coverage
                   WHERE coverage.org_id = latest.org_id
                     AND coverage.binding_id = latest.id
                     AND coverage.github_repo_id = $2
               ) AS repository_covered
        FROM latest
        "#,
    )
    .bind(org_id)
    .bind(repo_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("selection lookup", &error))?;
    Ok(classify_selection(row, github_repo_id, capability))
}

/// Mint a token narrowed to the selection's single repository and resolved
/// permissions. The client caches per request, so a refreshed grant mints a
/// new token rather than reusing one without it.
pub async fn mint_repository_token(
    client: &GithubAppClient,
    selection: &GithubAppRepositorySelection,
) -> Result<GithubAppRepositoryToken, G5Error> {
    let request = InstallationTokenRequest {
        repository_ids: BTreeSet::from([selection.github_repo_id]),
        permissions: selection.permissions.clone(),
        ..InstallationTokenRequest::default()
    };
    let token = client
        .installation_token(selection.installation_id, &request)
        .await?;
    Ok(GithubAppRepositoryToken {
        token,
        selection: selection.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::github_app::tests::test_cfg;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn row(disposition: &str, covered: bool, permissions: Value) -> SelectionRow {
        SelectionRow {
            binding_id: Uuid::new_v4(),
            binding_version: 3,
            installation_id: 77,
            disposition: disposition.to_string(),
            permissions,
            repository_covered: covered,
        }
    }

    fn record(permissions: Value) -> GithubAppBindingRecord {
        GithubAppBindingRecord {
            id: Uuid::new_v4(),
            app_identity_id: Uuid::new_v4(),
            version: 2,
            disposition: DISPOSITION_ACTIVE.to_string(),
            installation_id: 77,
            account_id: 44,
            account_login: "acme".to_string(),
            permissions,
            provenance: PROVENANCE_COVERAGE_CHANGE.to_string(),
            created_at: Utc::now(),
        }
    }

    fn installation(issues: &str, account_login: &str) -> GithubAppInstallation {
        GithubAppInstallation {
            installation_id: 77,
            account_id: 44,
            account_login: account_login.to_string(),
            permissions: BTreeMap::from([
                ("issues".to_string(), issues.to_string()),
                ("metadata".to_string(), "read".to_string()),
            ]),
            html_url: None,
        }
    }

    #[test]
    fn installation_differs_on_permission_level_or_account_only() {
        let latest = record(json!({"metadata": "read", "issues": "read"}));
        assert!(!installation_differs(
            &latest,
            &installation("read", "acme")
        ));
        assert!(installation_differs(
            &latest,
            &installation("write", "acme")
        ));
        assert!(installation_differs(
            &latest,
            &installation("read", "acme-renamed")
        ));
    }

    #[test]
    fn missing_capability_error_points_at_refresh() {
        let message = GithubAppIneligibleReason::CapabilityMissing {
            required: GithubAppCapability::IssuesWrite,
        }
        .to_string();
        assert!(message.contains("issues write"), "{message}");
        assert!(
            message.contains("refresh the binding from GitHub"),
            "{message}"
        );
    }

    #[test]
    fn capabilities_map_to_permissions_and_levels() {
        let write = json!({"issues": "write", "contents": "read"});
        let read = json!({"issues": "read"});
        assert!(GithubAppCapability::IssuesRead.is_satisfied_by(&write));
        assert!(GithubAppCapability::IssuesWrite.is_satisfied_by(&write));
        assert!(GithubAppCapability::IssuesRead.is_satisfied_by(&read));
        assert!(!GithubAppCapability::IssuesWrite.is_satisfied_by(&read));
        assert!(GithubAppCapability::ContentsRead.is_satisfied_by(&write));
        assert!(!GithubAppCapability::ContentsWrite.is_satisfied_by(&write));
        assert!(!GithubAppCapability::PullRequestsRead.is_satisfied_by(&write));
        assert!(!GithubAppCapability::IssuesRead.is_satisfied_by(&json!({"issues": "admin"})));
        assert!(!GithubAppCapability::IssuesRead.is_satisfied_by(&json!({"issues": 1})));
        assert!(!GithubAppCapability::IssuesRead.is_satisfied_by(&json!([])));
        assert_eq!(
            GithubAppCapability::IssuesWrite.token_permissions(&write),
            BTreeMap::from([("issues".to_string(), "write".to_string())])
        );
        assert_eq!(
            GithubAppCapability::IssuesRead.token_permissions(&write),
            BTreeMap::from([("issues".to_string(), "read".to_string())])
        );
    }

    #[test]
    fn pull_request_read_requires_readable_review_commits_in_the_same_token() {
        let capability = GithubAppCapability::PullRequestsRead;
        assert_eq!(
            capability.token_permissions(&json!({})),
            BTreeMap::from([
                ("pull_requests".to_string(), "read".to_string()),
                ("contents".to_string(), "read".to_string()),
            ])
        );
        assert!(!capability.is_satisfied_by(&json!({"pull_requests": "write"})));
        assert!(!capability.is_satisfied_by(&json!({"contents": "write"})));
        for pulls in ["read", "write"] {
            for contents in ["read", "write"] {
                assert!(capability
                    .is_satisfied_by(&json!({"pull_requests": pulls, "contents": contents})));
            }
        }
        let message = GithubAppIneligibleReason::CapabilityMissing {
            required: capability,
        }
        .to_string();
        assert!(message.contains("pull_requests read and contents read"));
    }

    #[test]
    fn pull_request_write_requires_readable_refs_in_the_same_token() {
        let capability = GithubAppCapability::PullRequestsWrite;
        assert_eq!(
            capability.token_permissions(&json!({})),
            BTreeMap::from([
                ("pull_requests".to_string(), "write".to_string()),
                ("contents".to_string(), "read".to_string()),
            ])
        );
        assert!(!capability.is_satisfied_by(&json!({"pull_requests": "write"})));
        assert!(!capability.is_satisfied_by(&json!({"contents": "write"})));
        assert!(!capability.is_satisfied_by(&json!({"pull_requests": "read", "contents": "write"})));
        for level in ["read", "write"] {
            assert!(
                capability.is_satisfied_by(&json!({"pull_requests": "write", "contents": level}))
            );
        }
        let message = GithubAppIneligibleReason::CapabilityMissing {
            required: capability,
        }
        .to_string();
        assert!(message.contains("pull_requests write and contents read"));
    }

    #[test]
    fn selection_fails_closed_in_order() {
        use GithubAppIneligibleReason as Reason;
        let grant = json!({"issues": "write"});
        let cap = GithubAppCapability::IssuesWrite;
        assert_eq!(
            classify_selection(None, 5, cap),
            GithubAppSelectionPreflight::Ineligible(Reason::NoBinding)
        );
        assert_eq!(
            classify_selection(Some(row("disconnected", true, grant.clone())), 5, cap),
            GithubAppSelectionPreflight::Ineligible(Reason::BindingInactive)
        );
        assert_eq!(
            classify_selection(Some(row("active", false, grant.clone())), 5, cap),
            GithubAppSelectionPreflight::Ineligible(Reason::RepositoryUncovered)
        );
        assert_eq!(
            classify_selection(Some(row("active", true, json!({"issues": "read"}))), 5, cap),
            GithubAppSelectionPreflight::Ineligible(Reason::CapabilityMissing { required: cap })
        );
        let eligible = row("active", true, grant);
        let binding_id = eligible.binding_id;
        assert_eq!(
            classify_selection(Some(eligible), 5, cap),
            GithubAppSelectionPreflight::Eligible(GithubAppRepositorySelection {
                binding_id,
                binding_version: 3,
                installation_id: 77,
                github_repo_id: 5,
                capability: cap,
                permissions: BTreeMap::from([("issues".to_string(), "write".to_string())]),
            })
        );
    }

    /// `g5.git.workflow-files`: Git writes request `workflows: write` exactly
    /// when the recorded installation grants it, and never need it.
    #[test]
    fn contents_write_requests_workflows_only_when_the_snapshot_grants_it() {
        let cap = GithubAppCapability::ContentsWrite;
        let contents = BTreeMap::from([("contents".to_string(), "write".to_string())]);
        let mut with_workflows = contents.clone();
        with_workflows.insert("workflows".to_string(), "write".to_string());
        for (snapshot, expected) in [
            (json!({"contents": "write"}), &contents),
            (json!({"contents": "write", "workflows": "read"}), &contents),
            (json!({"contents": "write", "workflows": 1}), &contents),
            (
                json!({"contents": "write", "workflows": "write"}),
                &with_workflows,
            ),
            (
                json!({"contents": "write", "workflows": "WRITE"}),
                &with_workflows,
            ),
        ] {
            assert!(cap.is_satisfied_by(&snapshot), "{snapshot}");
            assert_eq!(&cap.token_permissions(&snapshot), expected, "{snapshot}");
            let GithubAppSelectionPreflight::Eligible(selection) =
                classify_selection(Some(row("active", true, snapshot.clone())), 5, cap)
            else {
                panic!("contents write is eligible without workflows: {snapshot}");
            };
            assert_eq!(&selection.permissions, expected, "{snapshot}");
        }
        // The optional grant never substitutes for the required one.
        assert!(!cap.is_satisfied_by(&json!({"workflows": "write"})));
        // Other capabilities never ask for it, even when granted.
        let everything = json!({"contents":"write","workflows":"write","pull_requests":"write"});
        for other in [
            GithubAppCapability::ContentsRead,
            GithubAppCapability::PullRequestsRead,
            GithubAppCapability::PullRequestsWrite,
        ] {
            assert!(!other
                .token_permissions(&everything)
                .contains_key("workflows"));
        }
    }

    fn repo(id: i64, name: &str) -> GithubAppRepository {
        GithubAppRepository {
            github_repo_id: id,
            full_name: name.to_string(),
            private: true,
        }
    }

    #[test]
    fn coverage_selection_requires_unique_accessible_repositories() {
        let accessible = vec![repo(10, "acme/api"), repo(20, "acme/issues")];
        assert_eq!(
            validate_coverage_selection(&[20, 10], &accessible),
            Ok(vec![
                GithubAppCoveredRepository {
                    github_repo_id: 10,
                    full_name: "acme/api".to_string()
                },
                GithubAppCoveredRepository {
                    github_repo_id: 20,
                    full_name: "acme/issues".to_string()
                },
            ])
        );
        assert_eq!(validate_coverage_selection(&[], &accessible), Ok(vec![]));
        assert_eq!(
            validate_coverage_selection(&[10, 10], &accessible),
            Err(CoverageSelectionError::DuplicateRepository(10))
        );
        assert_eq!(
            validate_coverage_selection(&[10, 30], &accessible),
            Err(CoverageSelectionError::RepositoryNotAccessible(30))
        );
        assert!(matches!(
            G5Error::from(CoverageSelectionError::RepositoryNotAccessible(30)),
            G5Error::ValidationError { .. }
        ));
    }

    #[tokio::test]
    async fn repository_token_is_narrowed_to_one_repository_and_permission() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/app/installations/77/access_tokens"))
            .and(body_json(json!({
                "repository_ids": [4242],
                "permissions": {"issues": "read"}
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token": "ghs_narrow"})))
            .expect(1)
            .mount(&server)
            .await;
        let client = GithubAppClient::new(
            g5_scm::providers::ProviderClients::new().unwrap(),
            test_cfg(&server.uri()),
        );
        let selection = GithubAppRepositorySelection {
            binding_id: Uuid::new_v4(),
            binding_version: 2,
            installation_id: 77,
            github_repo_id: 4242,
            capability: GithubAppCapability::IssuesRead,
            permissions: BTreeMap::from([("issues".to_string(), "read".to_string())]),
        };
        let token = mint_repository_token(&client, &selection).await.unwrap();
        assert_eq!(token.token, "ghs_narrow");
        assert!(!format!("{token:?}").contains("ghs_narrow"));

        // A Git write under a workflow grant asks for exactly that addition.
        Mock::given(method("POST"))
            .and(path("/app/installations/77/access_tokens"))
            .and(body_json(json!({
                "repository_ids": [4242],
                "permissions": {"contents": "write", "workflows": "write"}
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token": "ghs_wf"})))
            .expect(1)
            .mount(&server)
            .await;
        let GithubAppSelectionPreflight::Eligible(selection) = classify_selection(
            Some(row(
                "active",
                true,
                json!({"contents": "write", "workflows": "write", "issues": "write"}),
            )),
            4242,
            GithubAppCapability::ContentsWrite,
        ) else {
            panic!("eligible");
        };
        let token = mint_repository_token(&client, &selection).await.unwrap();
        assert_eq!(token.token, "ghs_wf");
    }

    #[test]
    fn connection_attempt_debug_redacts_state() {
        let attempt = GithubAppConnectionAttempt {
            attempt_id: Uuid::new_v4(),
            expires_at: 1,
            state: "g5ga1.secret-state".to_string(),
        };
        assert!(!format!("{attempt:?}").contains("secret-state"));
    }
}
