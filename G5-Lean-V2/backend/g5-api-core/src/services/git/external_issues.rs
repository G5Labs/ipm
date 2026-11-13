//! The issue service: issue domains, workspace issue links and issue reads,
//! for every tracker (`g5.issue.native.adapter`).
//!
//! An external tracker owns its issue content; G5 stores only the
//! organization's logical tracker (`issue_trackers`), its issue domains
//! (`issue_domains`) and workspace associations (`workspace_issue_links`)
//! from migration 027. See `docs/external-issue-integration-design.md`.
//! G5's own tracker stores its issues in Postgres
//! ([`crate::services::git::native_issues`]). This module keeps what is the
//! same for every tracker (domains, links and their primary and Base rules)
//! and sends content reads to the adapter of each domain's provider through
//! [`IssueBackends`].
//!
//! Every database function takes a tenant [`ScopedDb`], so RLS confines it to
//! the organization. GitHub reads run between transactions: route handlers
//! load domains and eligibility, commit, call GitHub, then open a new
//! transaction for any association work. No transaction is held across GitHub
//! I/O.
//!
//! Read authorization for domain issues is organization membership: the
//! administrator's domain configuration is the grant, and reads use the GitHub
//! App installation narrowed to the domain repository and Issues read. A link
//! is never a grant to read an issue outside a configured domain.

mod attachments;
pub use attachments::*;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use futures::future::join_all;
use g5_authz::ScopedDb;
use g5_contracts::error::{ExternalIssueErrorKind, G5Error};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::scoped::user_can_access_workspace;
use crate::services::git::github_app::{GithubAppClient, GithubAppConfig, GithubAuthenticatedUser};
use crate::services::git::github_app_binding::{
    lookup_repository_selection, mint_repository_token, GithubAppCapability,
    GithubAppIneligibleReason, GithubAppRepositorySelection, GithubAppSelectionPreflight,
};
use crate::services::git::github_issues::{
    self, fetch_assignable_users, fetch_issue, fetch_issue_comments, fetch_issue_comments_since,
    fetch_issue_timeline, fetch_issues_created_by, fetch_parent_issue, fetch_repository,
    parse_canonical_github_id, search_repository_issues, GithubIssue, GithubIssueComment,
    GithubIssueLabel, GithubIssueReference, GithubIssueUser, GithubTimelineItem,
    IssueAssigneeFilter, IssueError, IssueSearchFilter, IssueStateFilter, RepositoryAudience,
    INTERACTIVE_TIMEOUT, SEARCH_PAGE_SIZE,
};
use crate::services::git::native_issues::{
    self, NativeAssignee, NativeReader, NativeSite, NativeTimeline, PROVIDER_G5,
};

pub const PROVIDER_GITHUB: &str = "github";
pub const GITHUB_INSTANCE_KEY: &str = "github.com";
pub const CONTAINER_KIND_GITHUB_REPOSITORY: &str = "github_repository";
const GITHUB_DISPLAY_NAME: &str = "GitHub";

const LINK_ACTIVE_PAIR_INDEX: &str = "workspace_issue_links_active_pair";
const LINK_WORKSPACE_PRIMARY_INDEX: &str = "workspace_issue_links_one_primary_per_workspace";
const LINK_ISSUE_PRIMARY_INDEX: &str = "workspace_issue_links_one_primary_per_issue";
const DOMAIN_CONTAINER_KEY: &str = "issue_domains_container_key";

fn db_error(context: &str, error: &sqlx::Error) -> G5Error {
    if g5_contracts::error::sqlx_error_is_retryable(error) {
        return G5Error::DatabaseRetryable {
            message: format!("external issue {context} failed: {error}"),
        };
    }
    G5Error::Internal {
        message: format!("external issue {context} failed: {error}"),
    }
}

fn unique_violation_constraint(error: &sqlx::Error) -> Option<String> {
    match error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            Some(database.constraint().unwrap_or_default().to_string())
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Wire records (camelCase, IDs as strings)
// ---------------------------------------------------------------------------

/// Why a configured domain cannot currently be served (database-only check
/// against the latest GitHub App binding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueDomainUnavailableReason {
    NoBinding,
    BindingInactive,
    RepositoryUncovered,
    IssuesPermissionMissing,
}

impl From<&GithubAppIneligibleReason> for IssueDomainUnavailableReason {
    fn from(reason: &GithubAppIneligibleReason) -> Self {
        match reason {
            GithubAppIneligibleReason::NoBinding => Self::NoBinding,
            GithubAppIneligibleReason::BindingInactive => Self::BindingInactive,
            GithubAppIneligibleReason::RepositoryUncovered => Self::RepositoryUncovered,
            GithubAppIneligibleReason::CapabilityMissing { .. } => Self::IssuesPermissionMissing,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDomainDTO {
    pub id: Uuid,
    pub tracker_id: Uuid,
    pub provider: String,
    pub container_kind: String,
    pub external_container_id: String,
    /// `owner/name`, or the native issue key (`ACME`).
    pub display_key: String,
    pub is_default_create_target: bool,
    pub content_trusted: bool,
    /// An org admin allows G5 comments on this domain's issues (external
    /// domains; native writes publish nothing and ignore it).
    pub comments_enabled: bool,
    pub available: bool,
    pub unavailable_reason: Option<IssueDomainUnavailableReason>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueLocatorDTO {
    pub repository_id: String,
    pub number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueUserDTO {
    /// The GitHub login, or a G5 member's email.
    pub login: String,
    pub avatar_url: Option<String>,
    /// A G5 member (native issues).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// A label (`g5.issue.labels.contract`): its opaque id within the domain
/// (GitHub's label id, or the G5 label's), name, colour (six hex digits) and
/// description. Older GitHub reads may lack the id and description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueLabelDTO {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `{visible: true, workspaceId, name}` or `{visible: false}` when the viewer
/// cannot access the owning workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssuePrimaryWorkspaceDTO {
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ExternalIssuePrimaryWorkspaceDTO {
    fn visible(workspace_id: Uuid, name: String) -> Self {
        Self {
            visible: true,
            workspace_id: Some(workspace_id),
            name: Some(name),
        }
    }

    fn hidden() -> Self {
        Self {
            visible: false,
            workspace_id: None,
            name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueSummaryDTO {
    pub provider: &'static str,
    pub tracker_id: Uuid,
    pub domain_id: Uuid,
    pub external_issue_id: String,
    pub locator: ExternalIssueLocatorDTO,
    pub display_key: String,
    pub canonical_url: String,
    pub title: String,
    pub state: String,
    pub state_reason: Option<String>,
    pub assignees: Vec<ExternalIssueUserDTO>,
    pub labels: Vec<ExternalIssueLabelDTO>,
    pub updated_at: String,
    pub author: Option<ExternalIssueUserDTO>,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub comment_count: u64,
    pub content_trusted: bool,
    pub primary_workspace: Option<ExternalIssuePrimaryWorkspaceDTO>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueCommentDTO {
    pub id: String,
    pub author: Option<ExternalIssueUserDTO>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueLinkRefDTO {
    /// Omitted (with the name) when the viewer cannot access the workspace,
    /// matching `ExternalIssuePrimaryWorkspaceDTO::hidden()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    pub workspace_name: Option<String>,
    pub role: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueDetailDTO {
    #[serde(flatten)]
    pub summary: ExternalIssueSummaryDTO,
    pub body: Option<String>,
    pub comments: Vec<ExternalIssueCommentDTO>,
    pub comments_truncated: bool,
    pub links: Vec<ExternalIssueLinkRefDTO>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueDomainErrorDTO {
    pub domain_id: Uuid,
    pub display_key: String,
    pub issue_error: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalIssueSearchResultsDTO {
    pub results: Vec<ExternalIssueSummaryDTO>,
    pub domain_errors: Vec<ExternalIssueDomainErrorDTO>,
    pub domain_count: usize,
    /// More matches existed across domains than fit on this page.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueErrorDTO {
    pub issue_error: &'static str,
    pub message: String,
}

impl From<&IssueError> for IssueErrorDTO {
    fn from(error: &IssueError) -> Self {
        Self {
            issue_error: error.kind.as_str(),
            message: error.message.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIssueLinkDTO {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub tracker_id: Uuid,
    /// `github` or `g5`.
    pub provider: &'static str,
    pub external_issue_id: String,
    pub display_key: String,
    pub canonical_url: String,
    pub locator: ExternalIssueLocatorDTO,
    pub role: &'static str,
    pub linked_by: Uuid,
    pub linked_at: DateTime<Utc>,
    pub issue: Option<ExternalIssueSummaryDTO>,
    pub issue_error: Option<IssueErrorDTO>,
}

pub(crate) fn user_dto(user: GithubIssueUser) -> ExternalIssueUserDTO {
    ExternalIssueUserDTO {
        login: user.login,
        avatar_url: user.avatar_url,
        user_id: None,
        display_name: None,
    }
}

fn role_str(is_primary: bool) -> &'static str {
    if is_primary {
        "primary"
    } else {
        "related"
    }
}

// ---------------------------------------------------------------------------
// Domains
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IssueDomainRecord {
    pub id: Uuid,
    pub org_id: Uuid,
    pub tracker_id: Uuid,
    pub provider: String,
    pub container_kind: String,
    pub external_container_id: String,
    pub display_key: String,
    pub is_default_create_target: bool,
    pub content_trusted: bool,
    pub comments_enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl IssueDomainRecord {
    /// G5's own tracker (`g5.issue.native`).
    #[must_use]
    pub fn is_native(&self) -> bool {
        self.provider == PROVIDER_G5
    }

    /// The GitHub repository id this domain names.
    #[must_use]
    pub fn github_repository_id(&self) -> Option<u64> {
        (self.provider == PROVIDER_GITHUB)
            .then(|| parse_canonical_github_id(&self.external_container_id))
            .flatten()
    }
}

/// The tracker an administrator adds a domain for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewDomainProvider {
    /// A GitHub repository covered by the App binding.
    Github,
    /// The organization's native domain (`g5.issue.native.domain`).
    Native,
}

impl NewDomainProvider {
    /// A request's `provider`: absent or `github` names a GitHub repository,
    /// `g5` the native tracker; anything else is refused.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value.map(str::trim) {
            None | Some("github") => Some(Self::Github),
            Some(PROVIDER_G5) => Some(Self::Native),
            Some(_) => None,
        }
    }
}

/// How a domain's issues are reached.
#[derive(Debug, Clone)]
pub enum DomainEligibility {
    /// Through the GitHub App binding: eligible for Issues read, or why not.
    Github(Result<GithubAppRepositorySelection, GithubAppIneligibleReason>),
    /// G5's own store, always available to members.
    Native,
}

/// A configured domain with its database-only eligibility for Issues read.
#[derive(Debug, Clone)]
pub struct IssueDomainAccess {
    pub domain: IssueDomainRecord,
    pub eligibility: DomainEligibility,
}

impl IssueDomainAccess {
    #[must_use]
    pub fn is_native(&self) -> bool {
        matches!(self.eligibility, DomainEligibility::Native)
    }

    #[must_use]
    pub fn to_dto(&self) -> IssueDomainDTO {
        let unavailable_reason = match &self.eligibility {
            DomainEligibility::Github(eligibility) => eligibility
                .as_ref()
                .err()
                .map(IssueDomainUnavailableReason::from),
            DomainEligibility::Native => None,
        };
        IssueDomainDTO {
            id: self.domain.id,
            tracker_id: self.domain.tracker_id,
            provider: self.domain.provider.clone(),
            container_kind: self.domain.container_kind.clone(),
            external_container_id: self.domain.external_container_id.clone(),
            display_key: self.domain.display_key.clone(),
            is_default_create_target: self.domain.is_default_create_target,
            content_trusted: self.domain.content_trusted,
            comments_enabled: self.domain.comments_enabled,
            available: unavailable_reason.is_none(),
            unavailable_reason,
            created_at: self.domain.created_at,
        }
    }

    /// The GitHub repository the domain names, or `outside_domain`.
    pub fn github_repository_id(&self) -> Result<u64, IssueError> {
        self.domain.github_repository_id().ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::OutsideDomain,
                "the issue domain is not a GitHub repository",
            )
        })
    }
}

pub(crate) const DOMAIN_COLUMNS: &str = "id, org_id, tracker_id, provider, container_kind, \
     external_container_id, display_key, is_default_create_target, content_trusted, \
     comments_enabled, created_at";

async fn lock_org_domains(db: &mut ScopedDb, org_id: Uuid) -> Result<(), G5Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('issue_domains:' || $1::text))")
        .bind(org_id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("domain lock", &error))?;
    Ok(())
}

async fn access_for(
    db: &mut ScopedDb,
    org_id: Uuid,
    domain: IssueDomainRecord,
) -> Result<IssueDomainAccess, G5Error> {
    if domain.is_native() {
        return Ok(IssueDomainAccess {
            domain,
            eligibility: DomainEligibility::Native,
        });
    }
    let eligibility = match domain.github_repository_id() {
        Some(repository_id) => match lookup_repository_selection(
            db,
            org_id,
            repository_id,
            GithubAppCapability::IssuesRead,
        )
        .await?
        {
            GithubAppSelectionPreflight::Eligible(selection) => Ok(selection),
            GithubAppSelectionPreflight::Ineligible(reason) => Err(reason),
        },
        None => Err(GithubAppIneligibleReason::RepositoryUncovered),
    };
    Ok(IssueDomainAccess {
        domain,
        eligibility: DomainEligibility::Github(eligibility),
    })
}

/// The organization's issue domains, oldest first, with eligibility.
pub async fn list_domain_access(
    db: &mut ScopedDb,
    org_id: Uuid,
) -> Result<Vec<IssueDomainAccess>, G5Error> {
    let domains: Vec<IssueDomainRecord> = sqlx::query_as(&format!(
        "SELECT {DOMAIN_COLUMNS} FROM issue_domains WHERE org_id = $1 ORDER BY created_at, id"
    ))
    .bind(org_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("domain list", &error))?;
    let mut access = Vec::with_capacity(domains.len());
    for domain in domains {
        access.push(access_for(db, org_id, domain).await?);
    }
    Ok(access)
}

/// Get or create the organization's logical GitHub tracker.
pub async fn get_or_create_github_tracker(
    db: &mut ScopedDb,
    org_id: Uuid,
    cfg: &GithubAppConfig,
) -> Result<Uuid, G5Error> {
    sqlx::query(
        r#"
        INSERT INTO issue_trackers
            (org_id, provider, instance_key, display_name, web_base_url, api_base_url)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT ON CONSTRAINT issue_trackers_instance_key DO NOTHING
        "#,
    )
    .bind(org_id)
    .bind(PROVIDER_GITHUB)
    .bind(GITHUB_INSTANCE_KEY)
    .bind(GITHUB_DISPLAY_NAME)
    .bind(cfg.web_base_url.trim_end_matches('/'))
    .bind(cfg.api_base_url.trim_end_matches('/'))
    .execute(db.conn())
    .await
    .map_err(|error| db_error("tracker insert", &error))?;
    sqlx::query_scalar(
        "SELECT id FROM issue_trackers WHERE org_id = $1 AND provider = $2 AND instance_key = $3",
    )
    .bind(org_id)
    .bind(PROVIDER_GITHUB)
    .bind(GITHUB_INSTANCE_KEY)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("tracker lookup", &error))
}

/// Configure a GitHub repository as an issue domain. The org's latest GitHub
/// App binding must be active, cover the repository and grant Issues read;
/// the display name comes from that binding's coverage. The first domain of
/// the organization becomes the default create target.
pub async fn create_github_domain(
    db: &mut ScopedDb,
    cfg: &GithubAppConfig,
    org_id: Uuid,
    actor: Uuid,
    repository_id: u64,
    content_trusted: bool,
) -> Result<IssueDomainAccess, G5Error> {
    lock_org_domains(db, org_id).await?;
    let selection = match lookup_repository_selection(
        db,
        org_id,
        repository_id,
        GithubAppCapability::IssuesRead,
    )
    .await?
    {
        GithubAppSelectionPreflight::Eligible(selection) => selection,
        GithubAppSelectionPreflight::Ineligible(reason) => {
            return Err(IssueError::new(
                ExternalIssueErrorKind::BindingNeedsAttention,
                reason.to_string(),
            )
            .into())
        }
    };
    let repo_id = i64::try_from(repository_id).map_err(|_| G5Error::ValidationError {
        details: json!({"field": "githubRepoId", "error": "out of range"}),
    })?;
    let full_name: String = sqlx::query_scalar(
        r#"
        SELECT full_name FROM github_app_installation_binding_repositories
        WHERE org_id = $1 AND binding_id = $2 AND github_repo_id = $3
        "#,
    )
    .bind(org_id)
    .bind(selection.binding_id)
    .bind(repo_id)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("coverage name lookup", &error))?;
    let tracker_id = get_or_create_github_tracker(db, org_id, cfg).await?;
    let is_default: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM issue_domains WHERE org_id = $1 AND is_default_create_target)",
    )
    .bind(org_id)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("default domain check", &error))?;

    let domain: IssueDomainRecord = sqlx::query_as(&format!(
        r#"
        INSERT INTO issue_domains
            (org_id, tracker_id, provider, container_kind, external_container_id, display_key,
             locator, is_default_create_target, content_trusted, configured_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING {DOMAIN_COLUMNS}
        "#
    ))
    .bind(org_id)
    .bind(tracker_id)
    .bind(PROVIDER_GITHUB)
    .bind(CONTAINER_KIND_GITHUB_REPOSITORY)
    .bind(repository_id.to_string())
    .bind(&full_name)
    .bind(json!({"fullName": full_name}))
    .bind(is_default)
    .bind(content_trusted)
    .bind(actor)
    .fetch_one(db.conn())
    .await
    .map_err(
        |error| match unique_violation_constraint(&error).as_deref() {
            Some(DOMAIN_CONTAINER_KEY) => G5Error::ConflictDetails {
                reason: "the repository is already an issue domain".to_string(),
                details: json!({"code": "domain_exists"}),
            },
            _ => db_error("domain insert", &error),
        },
    )?;
    Ok(IssueDomainAccess {
        domain,
        eligibility: DomainEligibility::Github(Ok(selection)),
    })
}

/// Add the organization's native domain (`g5.issue.native.domain`) under
/// the organization's domain lock.
pub async fn create_native_domain(
    db: &mut ScopedDb,
    org_id: Uuid,
    actor: Uuid,
    key: &str,
    content_trusted: bool,
) -> Result<IssueDomainAccess, G5Error> {
    lock_org_domains(db, org_id).await?;
    let domain =
        native_issues::create_native_domain(db, org_id, actor, key, content_trusted).await?;
    Ok(IssueDomainAccess {
        domain,
        eligibility: DomainEligibility::Native,
    })
}

fn domain_not_found(domain_id: Uuid) -> G5Error {
    G5Error::NotFound {
        resource: format!("issue domain {domain_id}"),
    }
}

/// Change whether a domain's issue content is trusted for full rendering.
/// Change a domain's settings; `None` leaves a setting unchanged. Enabling
/// comments records who approved them.
pub async fn update_domain_settings(
    db: &mut ScopedDb,
    org_id: Uuid,
    domain_id: Uuid,
    actor: Uuid,
    content_trusted: Option<bool>,
    comments_enabled: Option<bool>,
) -> Result<IssueDomainAccess, G5Error> {
    if comments_enabled.is_some() {
        let native: Option<bool> = sqlx::query_scalar(
            "SELECT provider = $3 FROM issue_domains WHERE org_id = $1 AND id = $2",
        )
        .bind(org_id)
        .bind(domain_id)
        .bind(PROVIDER_G5)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| db_error("domain lookup", &error))?;
        if native == Some(true) {
            return Err(G5Error::ValidationError {
                details: json!({
                    "field": "commentsEnabled",
                    "error": "G5 issues publish nothing outside G5, so they have no comment switch",
                }),
            });
        }
    }
    let domain: IssueDomainRecord = sqlx::query_as(&format!(
        r#"
        UPDATE issue_domains
        SET content_trusted = COALESCE($3, content_trusted),
            comments_enabled = COALESCE($4, comments_enabled),
            comments_enabled_by = CASE
                WHEN $4 IS TRUE AND NOT comments_enabled THEN $5
                WHEN $4 IS FALSE THEN NULL
                ELSE comments_enabled_by END,
            comments_enabled_at = CASE
                WHEN $4 IS TRUE AND NOT comments_enabled THEN now()
                WHEN $4 IS FALSE THEN NULL
                ELSE comments_enabled_at END,
            updated_at = now()
        WHERE org_id = $1 AND id = $2
        RETURNING {DOMAIN_COLUMNS}
        "#
    ))
    .bind(org_id)
    .bind(domain_id)
    .bind(content_trusted)
    .bind(comments_enabled)
    .bind(actor)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("domain update", &error))?
    .ok_or_else(|| domain_not_found(domain_id))?;
    access_for(db, org_id, domain).await
}

/// Remove a domain. Existing workspace links are unaffected. When the default
/// create target is removed, the oldest remaining domain becomes the default.
/// A native domain that still has issues is refused: nothing else could
/// reach them (`g5.issue.native.domain`).
pub async fn delete_domain(
    db: &mut ScopedDb,
    org_id: Uuid,
    domain_id: Uuid,
) -> Result<(), G5Error> {
    lock_org_domains(db, org_id).await?;
    if native_issues::domain_has_issues(db, domain_id).await? {
        return Err(G5Error::ConflictDetails {
            reason:
                "G5 issues cannot be removed while the organization has issues; export them first"
                    .to_string(),
            details: json!({"code": "domain_has_issues"}),
        });
    }
    let was_default: bool = sqlx::query_scalar(
        "DELETE FROM issue_domains WHERE org_id = $1 AND id = $2 RETURNING is_default_create_target",
    )
    .bind(org_id)
    .bind(domain_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("domain delete", &error))?
    .ok_or_else(|| domain_not_found(domain_id))?;
    if was_default {
        sqlx::query(
            r#"
            UPDATE issue_domains SET is_default_create_target = true, updated_at = now()
            WHERE id = (
                SELECT id FROM issue_domains WHERE org_id = $1 ORDER BY created_at, id LIMIT 1
            )
            "#,
        )
        .bind(org_id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("default domain promotion", &error))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// GitHub reads (no transaction may be open)
// ---------------------------------------------------------------------------

/// A narrowed Issues-read token for an eligible domain, minted within the
/// interactive budget. An ineligible domain is `binding_needs_attention`.
async fn domain_token(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
) -> Result<String, IssueError> {
    let selection = match &access.eligibility {
        DomainEligibility::Github(eligibility) => eligibility.as_ref().map_err(|reason| {
            IssueError::new(
                ExternalIssueErrorKind::BindingNeedsAttention,
                reason.to_string(),
            )
        })?,
        DomainEligibility::Native => {
            return Err(IssueError::new(
                ExternalIssueErrorKind::OutsideDomain,
                "the issue domain is not a GitHub repository",
            ))
        }
    };
    let token = tokio::time::timeout(
        INTERACTIVE_TIMEOUT,
        mint_repository_token(client, selection),
    )
    .await
    .map_err(|_| IssueError::unavailable("GitHub did not issue an access token within 5 seconds"))?
    .map_err(|error| IssueError::unavailable(error.to_string()))?;
    Ok(token.token)
}

/// A GitHub label as every surface shows it.
#[must_use]
pub fn label_dto(label: &GithubIssueLabel) -> ExternalIssueLabelDTO {
    ExternalIssueLabelDTO {
        id: label.id.map(|id| id.to_string()),
        name: label.name.clone(),
        color: label.color.as_deref().map(str::to_ascii_lowercase),
        description: label.description.clone(),
    }
}

/// Build a summary; `primaryWorkspace` is filled in by [`attach_primary_owners`].
pub fn summary(
    access: &IssueDomainAccess,
    repository_id: u64,
    issue: &GithubIssue,
) -> ExternalIssueSummaryDTO {
    let repository = issue
        .repository_full_name
        .clone()
        .unwrap_or_else(|| access.domain.display_key.clone());
    ExternalIssueSummaryDTO {
        provider: PROVIDER_GITHUB,
        tracker_id: access.domain.tracker_id,
        domain_id: access.domain.id,
        external_issue_id: issue.id.clone(),
        locator: ExternalIssueLocatorDTO {
            repository_id: repository_id.to_string(),
            number: issue.number,
        },
        display_key: format!("{repository}#{}", issue.number),
        canonical_url: issue.html_url.clone(),
        title: issue.title.clone(),
        state: issue.state.clone(),
        state_reason: issue.state_reason.clone(),
        assignees: issue.assignees.iter().cloned().map(user_dto).collect(),
        labels: issue.labels.iter().map(label_dto).collect(),
        updated_at: issue.updated_at.clone(),
        author: issue.author.clone().map(user_dto),
        created_at: issue.created_at.clone(),
        closed_at: issue.closed_at.clone(),
        comment_count: issue.comment_count,
        content_trusted: access.domain.content_trusted,
        primary_workspace: None,
    }
}

// ---------------------------------------------------------------------------
// Backends: how each provider's issues are read
// ---------------------------------------------------------------------------

/// Opens the caller's tenant scope (the person's, or the agent owner's) for
/// a native read, in its own short transaction.
#[async_trait::async_trait]
pub trait TenantDbOpener: Send + Sync {
    async fn open(&self) -> Result<ScopedDb, G5Error>;
}

/// G5's own store, read under the caller's tenant scope.
pub struct NativeBackend<'a> {
    pub opener: &'a dyn TenantDbOpener,
    pub system_pool: &'a PgPool,
    pub site: NativeSite,
    pub attachments: &'a native_issues::IssueAttachmentStore,
}

impl NativeBackend<'_> {
    #[must_use]
    pub fn reader<'r>(&'r self, access: &'r IssueDomainAccess) -> NativeReader<'r> {
        NativeReader {
            domain: &access.domain,
            site: &self.site,
            system_pool: self.system_pool,
        }
    }

    async fn open(&self) -> Result<ScopedDb, IssueError> {
        self.opener
            .open()
            .await
            .map_err(|error| native_issues::read_error(&error))
    }
}

async fn finish(db: ScopedDb) -> Result<(), IssueError> {
    db.commit()
        .await
        .map_err(|error| IssueError::unavailable(format!("G5 issues could not be read: {error}")))
}

/// The adapters content reads are sent to (`g5.issue.native.adapter`).
/// `github` is absent on a deployment without the GitHub App; `native` when
/// the organization has no native domain. A domain whose adapter is absent
/// reports `unavailable` on its own.
pub struct IssueBackends<'a> {
    pub github: Option<&'a GithubAppClient>,
    pub native: Option<NativeBackend<'a>>,
}

fn integration_unavailable() -> IssueError {
    IssueError::unavailable("external issue integration is not configured")
}

impl IssueBackends<'_> {
    fn github(&self) -> Result<&GithubAppClient, IssueError> {
        self.github.ok_or_else(integration_unavailable)
    }

    fn native(&self) -> Result<&NativeBackend<'_>, IssueError> {
        self.native
            .as_ref()
            .ok_or_else(|| IssueError::unavailable("G5 issues are not available here"))
    }
}

/// Who an issue search keeps: `Me` is the searching person (or the agent's
/// owner), matched by their connected GitHub account on GitHub domains and
/// by their G5 account on native ones.
#[derive(Debug, Clone, Copy)]
pub enum SearchAssignee<'a> {
    Any,
    Unassigned,
    Me {
        user_id: Option<Uuid>,
        github: Option<&'a GithubAuthenticatedUser>,
    },
}

/// One domain's search results and whether the provider held back more.
async fn search_github_domain(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    query: &str,
    filter: IssueSearchFilter<'_>,
) -> Result<(Vec<ExternalIssueSummaryDTO>, bool), IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    let repository = fetch_repository(client, &token, repository_id).await?;
    // A pasted reference finds that issue (`g5.issue.search.one-path`);
    // GitHub search would read it as words.
    let reference = client
        .endpoints()
        .ok()
        .and_then(|endpoints| github_issues::parse_issue_reference(query, &endpoints).ok());
    if let Some(reference) = reference {
        if !reference.names_repository(&repository.full_name) {
            return Ok((Vec::new(), false));
        }
        let issue = match fetch_issue(client, &token, repository_id, reference.number).await {
            Ok(issue) => issue,
            Err(error)
                if matches!(
                    error.kind,
                    ExternalIssueErrorKind::NotFound | ExternalIssueErrorKind::NotAnIssue
                ) =>
            {
                return Ok((Vec::new(), false));
            }
            Err(error) => return Err(error),
        };
        let found = search_filter_keeps(&issue, filter)
            .then(|| summary(access, repository_id, &issue))
            .into_iter()
            .collect();
        return Ok((found, false));
    }
    let page =
        search_repository_issues(client, &token, &repository.full_name, query, filter).await?;
    let results = page
        .issues
        .iter()
        .map(|issue| summary(access, repository_id, issue))
        .collect();
    Ok((results, page.has_more))
}

/// The State and assignee filters of a search, applied to an issue found by
/// reference.
fn search_filter_keeps(issue: &GithubIssue, filter: IssueSearchFilter<'_>) -> bool {
    let state = match filter.state {
        IssueStateFilter::Open => issue.state == "open",
        IssueStateFilter::Closed => issue.state == "closed",
        IssueStateFilter::All => true,
    };
    state
        && match filter.assignee {
            IssueAssigneeFilter::Any => true,
            IssueAssigneeFilter::Account(account) => issue
                .assignees
                .iter()
                .any(|user| user.id == Some(account.id)),
            IssueAssigneeFilter::Unassigned => issue.assignees.is_empty(),
        }
}

async fn search_domain(
    backends: &IssueBackends<'_>,
    access: &IssueDomainAccess,
    query: &str,
    state: IssueStateFilter,
    assignee: SearchAssignee<'_>,
    viewer: Option<Uuid>,
) -> Result<(Vec<ExternalIssueSummaryDTO>, bool), IssueError> {
    if access.is_native() {
        let native = backends.native()?;
        let assignee = match assignee {
            SearchAssignee::Any => NativeAssignee::Any,
            SearchAssignee::Unassigned => NativeAssignee::Unassigned,
            SearchAssignee::Me {
                user_id: Some(user_id),
                ..
            } => NativeAssignee::User(user_id),
            SearchAssignee::Me { user_id: None, .. } => {
                return Err(IssueError::new(
                    ExternalIssueErrorKind::InvalidQuery,
                    "assignee me needs a person",
                ))
            }
        };
        let mut db = native.open().await?;
        let found = native_issues::search(
            &mut db,
            &native.reader(access),
            query,
            state,
            assignee,
            viewer,
        )
        .await?;
        finish(db).await?;
        return Ok(found);
    }
    let assignee = match assignee {
        SearchAssignee::Any => IssueAssigneeFilter::Any,
        SearchAssignee::Unassigned => IssueAssigneeFilter::Unassigned,
        SearchAssignee::Me {
            github: Some(account),
            ..
        } => IssueAssigneeFilter::Account(account),
        SearchAssignee::Me { github: None, .. } => {
            return Err(IssueError::new(
                ExternalIssueErrorKind::AccessDenied,
                "connect your GitHub account to see GitHub issues assigned to you",
            ))
        }
    };
    search_github_domain(
        backends.github()?,
        access,
        query,
        IssueSearchFilter { state, assignee },
    )
    .await
}

/// Order results from every domain by recency (ISO-8601 sorts as text) so no
/// single domain monopolizes the page, keep `page_size`, and report whether
/// any matches were cut off.
fn merge_search_page(results: &mut Vec<ExternalIssueSummaryDTO>, page_size: usize) -> bool {
    results.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.display_key.cmp(&right.display_key))
    });
    let truncated = results.len() > page_size;
    results.truncate(page_size);
    truncated
}

/// Federated search across every domain. Each domain settles independently;
/// a failing domain is reported in `domainErrors`, never as zero matches.
pub async fn search_domains(
    backends: &IssueBackends<'_>,
    domains: &[IssueDomainAccess],
    query: &str,
    state: IssueStateFilter,
    assignee: SearchAssignee<'_>,
    viewer: Option<Uuid>,
) -> ExternalIssueSearchResultsDTO {
    let outcomes = join_all(
        domains
            .iter()
            .map(|access| search_domain(backends, access, query, state, assignee, viewer)),
    )
    .await;
    let mut results = Vec::new();
    let mut domain_errors = Vec::new();
    let mut provider_truncated = false;
    for (access, outcome) in domains.iter().zip(outcomes) {
        match outcome {
            Ok((found, has_more)) => {
                results.extend(found);
                provider_truncated |= has_more;
            }
            Err(error) => domain_errors.push(ExternalIssueDomainErrorDTO {
                domain_id: access.domain.id,
                display_key: access.domain.display_key.clone(),
                issue_error: error.kind.as_str(),
                message: error.message,
            }),
        }
    }
    let truncated = merge_search_page(&mut results, SEARCH_PAGE_SIZE) || provider_truncated;
    ExternalIssueSearchResultsDTO {
        results,
        domain_errors,
        domain_count: domains.len(),
        truncated,
    }
}

/// Agent searches one workspace may run per minute.
pub const AGENT_SEARCHES_PER_WORKSPACE: u32 = 6;

/// GitHub search requests per minute that all agents of an organization may
/// spend together: half of GitHub's 30 authenticated searches a minute, so
/// people keep the rest.
pub const AGENT_SEARCH_REQUESTS_PER_ORG: u32 = 15;

/// Reserve one agent search for the workspace and its organization
/// (`g5.external-issues.cli.search-budget`). The organization's window is
/// sized from all `domains_configured`, even for a search of one domain, so
/// every search in a window counts against the same limit and together they
/// cannot exceed [`AGENT_SEARCH_REQUESTS_PER_ORG`] GitHub requests. Both
/// windows are shared by every API replica; a refusal charges neither, and a
/// store that cannot decide refuses the search.
pub async fn reserve_agent_search(
    store: &dyn crate::middleware::RateLimitStore,
    org_id: Uuid,
    workspace_id: Uuid,
    domains_configured: usize,
) -> Result<(), G5Error> {
    use crate::middleware::{RateLimitConfig, RateLimitError};
    let window = |requests_per_minute: u32| RateLimitConfig {
        requests_per_minute,
        burst_size: requests_per_minute,
        enabled: true,
    };
    let domains = u32::try_from(domains_configured.max(1)).unwrap_or(u32::MAX);
    let org_searches = (AGENT_SEARCH_REQUESTS_PER_ORG / domains).max(1);
    match store
        .try_acquire_pair(
            &format!("g5:rate-limit:{{agent-issue-search}}:workspace:{workspace_id}"),
            &window(AGENT_SEARCHES_PER_WORKSPACE),
            &format!("g5:rate-limit:{{agent-issue-search}}:org:{org_id}"),
            &window(org_searches),
        )
        .await
    {
        Ok(()) => Ok(()),
        Err(RateLimitError::Limited(retry_after)) => Err(G5Error::RateLimited {
            retry_after_secs: Some(retry_after.as_secs().max(1)),
        }),
        Err(RateLimitError::Unavailable) => Err(G5Error::ServiceUnavailable {
            message: "agent issue search is temporarily unavailable".to_string(),
        }),
    }
}

/// The organization's external issue domains: what [`reserve_agent_search`]
/// sizes its organization window from. A PR search (`g5.git.pr-list.search`)
/// passes the same count, so both searches share one window and one meaning.
pub async fn count_external_domains(db: &mut ScopedDb, org_id: Uuid) -> Result<usize, G5Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM issue_domains WHERE org_id = $1 AND provider <> $2",
    )
    .bind(org_id)
    .bind(PROVIDER_G5)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("domain count", &error))?;
    Ok(usize::try_from(count).unwrap_or(0))
}

/// An issue resolved inside a configured domain.
#[derive(Debug, Clone)]
pub struct ResolvedIssue {
    pub domain_index: usize,
    pub summary: ExternalIssueSummaryDTO,
}

/// Resolve a reference against the configured domains: a native key
/// reference (`ACME-12`) or issue-page URL names the native domain; a GitHub
/// URL or `owner/name#n` a GitHub one. A container that is not a domain is
/// `outside_domain`.
pub async fn resolve_reference(
    backends: &IssueBackends<'_>,
    domains: &[IssueDomainAccess],
    reference: &str,
) -> Result<ResolvedIssue, IssueError> {
    if let Some(native) = &backends.native {
        let named = domains.iter().enumerate().find_map(|(index, access)| {
            access
                .is_native()
                .then(|| {
                    native_issues::parse_reference(
                        reference,
                        &access.domain.display_key,
                        &native.site,
                    )
                })
                .flatten()
                .map(|number| (index, number))
        });
        if let Some((domain_index, number)) = named {
            let mut db = native.open().await?;
            let summary = native_issues::summary_by_number(
                &mut db,
                &native.reader(&domains[domain_index]),
                number,
            )
            .await?;
            finish(db).await?;
            return Ok(ResolvedIssue {
                domain_index,
                summary,
            });
        }
    }
    let github = domains
        .iter()
        .enumerate()
        .filter(|(_, access)| !access.is_native())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let client = match backends.github {
        Some(client) => client,
        None if looks_native(reference) => {
            return Err(IssueError::invalid_reference(format!(
                "{reference} does not name an issue of this organization"
            )))
        }
        None => return Err(integration_unavailable()),
    };
    let github_domains: Vec<IssueDomainAccess> =
        github.iter().map(|index| domains[*index].clone()).collect();
    let (index, repository_id, issue) =
        resolve_github_reference(client, &github_domains, reference).await?;
    let domain_index = github[index];
    Ok(ResolvedIssue {
        domain_index,
        summary: summary(&domains[domain_index], repository_id, &issue),
    })
}

/// A key reference (`ACME-12`) that no native domain matched.
fn looks_native(reference: &str) -> bool {
    reference
        .trim()
        .rsplit_once('-')
        .is_some_and(|(key, number)| {
            !key.is_empty()
                && key.chars().all(|c| c.is_ascii_alphanumeric())
                && !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
        })
}

/// Resolve a URL or `owner/name#n` reference against GitHub `domains`,
/// matching owner/name case-insensitively against each domain repository's
/// current name. A repository that is not a domain is `outside_domain`; when
/// a domain could not be checked, its failure is reported instead.
pub async fn resolve_github_reference(
    client: &GithubAppClient,
    domains: &[IssueDomainAccess],
    reference: &str,
) -> Result<(usize, u64, GithubIssue), IssueError> {
    let endpoints = client
        .endpoints()
        .map_err(|_| IssueError::unavailable("the GitHub integration endpoints are invalid"))?;
    let reference = github_issues::parse_issue_reference(reference, &endpoints)?;
    let checks = join_all(domains.iter().map(|access| async {
        let repository_id = access.github_repository_id()?;
        let token = domain_token(client, access).await?;
        let repository = fetch_repository(client, &token, repository_id).await?;
        Ok::<_, IssueError>((repository_id, repository.full_name, token))
    }))
    .await;
    let mut first_error = None;
    let mut matched = None;
    for (index, check) in checks.into_iter().enumerate() {
        match check {
            Ok((repository_id, full_name, token)) if reference.names_repository(&full_name) => {
                matched = Some((index, repository_id, token));
                break;
            }
            Ok(_) => {}
            Err(error) => {
                // A failure for the domain the reference names by its
                // recorded key takes precedence.
                if first_error.is_none()
                    || reference.names_repository(&domains[index].domain.display_key)
                {
                    first_error = Some(error);
                }
            }
        }
    }
    let Some((domain_index, repository_id, token)) = matched else {
        return Err(first_error.unwrap_or_else(|| outside_domain(&reference)));
    };
    let issue = fetch_issue(client, &token, repository_id, reference.number).await?;
    Ok((domain_index, repository_id, issue))
}

fn outside_domain(reference: &GithubIssueReference) -> IssueError {
    IssueError::new(
        ExternalIssueErrorKind::OutsideDomain,
        format!("{} is not a configured issue domain", reference.full_name()),
    )
}

/// The configured domain for a GitHub repository id, or `outside_domain`.
pub fn domain_for_repository(
    domains: &[IssueDomainAccess],
    repository_id: u64,
) -> Result<usize, IssueError> {
    domains
        .iter()
        .position(|access| access.domain.github_repository_id() == Some(repository_id))
        .ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::OutsideDomain,
                "the repository is not a configured issue domain",
            )
        })
}

/// The organization's native domain, or `outside_domain`.
pub fn native_domain(domains: &[IssueDomainAccess]) -> Result<&IssueDomainAccess, IssueError> {
    domains
        .iter()
        .find(|access| access.is_native())
        .ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::OutsideDomain,
                "the organization does not use G5 issues",
            )
        })
}

/// Issue numbered `number` in a domain and, when `with_comments`, its first
/// comment page for a detail view.
pub async fn fetch_issue_detail(
    backends: &IssueBackends<'_>,
    access: &IssueDomainAccess,
    number: u64,
    with_comments: bool,
) -> Result<ExternalIssueDetailDTO, IssueError> {
    if access.is_native() {
        let native = backends.native()?;
        let mut db = native.open().await?;
        let detail =
            native_issues::detail_by_number(&mut db, &native.reader(access), number, with_comments)
                .await?;
        finish(db).await?;
        return Ok(detail);
    }
    let client = backends.github()?;
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    let (issue, comments) = tokio::join!(
        fetch_issue(client, &token, repository_id, number),
        optional_comments(client, &token, repository_id, number, with_comments),
    );
    let issue = issue?;
    let comments = comments?;
    Ok(detail(access, repository_id, &issue, comments))
}

async fn optional_comments(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
    wanted: bool,
) -> Result<Option<Vec<GithubIssueComment>>, IssueError> {
    if !wanted {
        return Ok(None);
    }
    fetch_issue_comments(client, token, repository_id, number)
        .await
        .map(Some)
}

fn comment_dto(comment: GithubIssueComment) -> ExternalIssueCommentDTO {
    ExternalIssueCommentDTO {
        id: comment.id,
        author: comment.author.map(user_dto),
        body: comment.body,
        created_at: comment.created_at,
        updated_at: comment.updated_at,
        url: comment.html_url,
    }
}

fn detail(
    access: &IssueDomainAccess,
    repository_id: u64,
    issue: &GithubIssue,
    comments: Option<Vec<GithubIssueComment>>,
) -> ExternalIssueDetailDTO {
    // Comments that were not asked for are not truncated, just absent.
    let fetched = comments.is_some();
    let comments: Vec<ExternalIssueCommentDTO> = comments
        .unwrap_or_default()
        .into_iter()
        .map(comment_dto)
        .collect();
    ExternalIssueDetailDTO {
        summary: summary(access, repository_id, issue),
        body: issue.body.clone(),
        comments_truncated: fetched
            && issue.comment_count > u64::try_from(comments.len()).unwrap_or(u64::MAX),
        comments,
        links: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Association reads
// ---------------------------------------------------------------------------

/// Whether `viewer` may see `workspace_id`; non-user actors see nothing.
async fn viewer_can_access(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    viewer: Option<Uuid>,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<bool, G5Error> {
    let Some(viewer) = viewer else {
        return Ok(false);
    };
    user_can_access_workspace(system_pool, db, viewer, org_id, workspace_id)
        .await
        .map_err(|error| error.inner)
}

#[derive(sqlx::FromRow)]
struct PrimaryOwnerRow {
    tracker_id: Uuid,
    external_issue_id: String,
    workspace_id: Uuid,
    workspace_name: String,
}

/// Fill `primaryWorkspace` on each summary from the active primary links.
pub async fn attach_primary_owners(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    viewer: Option<Uuid>,
    org_id: Uuid,
    summaries: &mut [&mut ExternalIssueSummaryDTO],
) -> Result<(), G5Error> {
    if summaries.is_empty() {
        return Ok(());
    }
    let trackers: Vec<Uuid> = summaries.iter().map(|summary| summary.tracker_id).collect();
    let issues: Vec<String> = summaries
        .iter()
        .map(|summary| summary.external_issue_id.clone())
        .collect();
    let rows: Vec<PrimaryOwnerRow> = sqlx::query_as(
        r#"
        SELECT link.tracker_id, link.external_issue_id, link.workspace_id,
               workspace.name AS workspace_name
        FROM workspace_issue_links AS link
        JOIN workspaces AS workspace
          ON workspace.id = link.workspace_id AND workspace.org_id = link.org_id
        JOIN unnest($2::uuid[], $3::text[]) AS wanted(tracker_id, external_issue_id)
          ON wanted.tracker_id = link.tracker_id
         AND wanted.external_issue_id = link.external_issue_id
        WHERE link.org_id = $1 AND link.is_primary AND link.unlinked_at IS NULL
        "#,
    )
    .bind(org_id)
    .bind(&trackers)
    .bind(&issues)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("primary owner lookup", &error))?;

    let mut visibility = HashMap::<Uuid, bool>::new();
    let mut owners = HashMap::<(Uuid, String), ExternalIssuePrimaryWorkspaceDTO>::new();
    for row in rows {
        let visible = match visibility.get(&row.workspace_id) {
            Some(visible) => *visible,
            None => {
                let visible =
                    viewer_can_access(db, system_pool, viewer, org_id, row.workspace_id).await?;
                visibility.insert(row.workspace_id, visible);
                visible
            }
        };
        owners.insert(
            (row.tracker_id, row.external_issue_id),
            if visible {
                ExternalIssuePrimaryWorkspaceDTO::visible(row.workspace_id, row.workspace_name)
            } else {
                ExternalIssuePrimaryWorkspaceDTO::hidden()
            },
        );
    }
    for summary in summaries.iter_mut() {
        summary.primary_workspace = owners
            .get(&(summary.tracker_id, summary.external_issue_id.clone()))
            .cloned();
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct IssueLinkRow {
    workspace_id: Uuid,
    workspace_name: String,
    is_primary: bool,
}

/// Active links for one issue, primary first. A workspace the viewer cannot
/// access is listed with neither its name nor its id.
pub async fn issue_link_refs(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    viewer: Option<Uuid>,
    org_id: Uuid,
    tracker_id: Uuid,
    external_issue_id: &str,
) -> Result<Vec<ExternalIssueLinkRefDTO>, G5Error> {
    let rows: Vec<IssueLinkRow> = sqlx::query_as(
        r#"
        SELECT link.workspace_id, workspace.name AS workspace_name, link.is_primary
        FROM workspace_issue_links AS link
        JOIN workspaces AS workspace
          ON workspace.id = link.workspace_id AND workspace.org_id = link.org_id
        WHERE link.org_id = $1 AND link.tracker_id = $2 AND link.external_issue_id = $3
          AND link.unlinked_at IS NULL
        ORDER BY link.is_primary DESC, link.linked_at, link.id
        "#,
    )
    .bind(org_id)
    .bind(tracker_id)
    .bind(external_issue_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("issue link lookup", &error))?;
    let mut links = Vec::with_capacity(rows.len());
    for row in rows {
        let visible = viewer_can_access(db, system_pool, viewer, org_id, row.workspace_id).await?;
        links.push(ExternalIssueLinkRefDTO {
            workspace_id: visible.then_some(row.workspace_id),
            workspace_name: visible.then_some(row.workspace_name),
            role: role_str(row.is_primary),
        });
    }
    Ok(links)
}

// ---------------------------------------------------------------------------
// Workspace links
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceIssueLinkRecord {
    pub id: Uuid,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub tracker_id: Uuid,
    pub external_issue_id: String,
    pub display_key: String,
    pub canonical_url: String,
    pub locator: Value,
    pub is_primary: bool,
    pub linked_by: Uuid,
    pub linked_at: DateTime<Utc>,
}

impl WorkspaceIssueLinkRecord {
    pub(crate) fn locator_dto(&self) -> ExternalIssueLocatorDTO {
        ExternalIssueLocatorDTO {
            repository_id: self
                .locator
                .get("repositoryId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            number: self
                .locator
                .get("number")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        }
    }

    /// The link's tracker kind, from its locator: G5 issues record the
    /// non-numeric repository `g5`.
    #[must_use]
    pub fn provider(&self) -> &'static str {
        if self.locator_dto().repository_id == native_issues::NATIVE_LOCATOR_REPOSITORY {
            PROVIDER_G5
        } else {
            PROVIDER_GITHUB
        }
    }

    #[must_use]
    pub fn to_dto(
        &self,
        issue: Option<ExternalIssueSummaryDTO>,
        issue_error: Option<IssueErrorDTO>,
    ) -> WorkspaceIssueLinkDTO {
        WorkspaceIssueLinkDTO {
            id: self.id,
            workspace_id: self.workspace_id,
            tracker_id: self.tracker_id,
            provider: self.provider(),
            external_issue_id: self.external_issue_id.clone(),
            display_key: self.display_key.clone(),
            canonical_url: self.canonical_url.clone(),
            locator: self.locator_dto(),
            role: role_str(self.is_primary),
            linked_by: self.linked_by,
            linked_at: self.linked_at,
            issue,
            issue_error,
        }
    }
}

const LINK_COLUMNS: &str = "id, org_id, workspace_id, tracker_id, external_issue_id, display_key, \
     canonical_url, locator, is_primary, linked_by, linked_at";

/// Organization switch for external issues (GitHub App bindings aside: issue
/// domains, links, reads, comments and the agent's `g5d issue` commands).
/// A boolean set only on the organization's own settings; workspace and user
/// settings cannot turn it on, so it is read from the organization scope, not
/// from merged effective settings.
pub const EXTERNAL_ISSUES_ENABLED_SETTING: &str = "external_issues_enabled";

/// Whether the organization has issues on: its own `external_issues_enabled`
/// setting (never a workspace or user override). On unless set to `false`:
/// an organization that never touched the switch tracks issues with G5
/// issues from the start (`g5.issue.native.domain`).
pub async fn org_external_issues_enabled(db: &mut ScopedDb, org_id: Uuid) -> Result<bool, G5Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE((SELECT settings_json -> $2 IS DISTINCT FROM 'false'::jsonb FROM org_configs \
                          WHERE org_id = $1), true)",
    )
    .bind(org_id)
    .bind(EXTERNAL_ISSUES_ENABLED_SETTING)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("external issues setting", &error))
}

/// Refuse external-issue work for an organization that has not turned it on.
pub async fn ensure_external_issues_enabled(
    db: &mut ScopedDb,
    org_id: Uuid,
) -> Result<(), G5Error> {
    if org_external_issues_enabled(db, org_id).await? {
        Ok(())
    } else {
        Err(G5Error::ConflictDetails {
            reason: "external issues are not enabled for this organization".to_string(),
            details: serde_json::json!({ "code": "external_issues_disabled" }),
        })
    }
}

/// The organization that owns a live workspace (under the caller's RLS scope).
pub async fn workspace_org_id(db: &mut ScopedDb, workspace_id: Uuid) -> Result<Uuid, G5Error> {
    sqlx::query_scalar("SELECT org_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL")
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| db_error("workspace lookup", &error))?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })
}

/// Active links of a workspace, primary first.
pub async fn list_workspace_links(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<Vec<WorkspaceIssueLinkRecord>, G5Error> {
    sqlx::query_as(&format!(
        r#"
        SELECT {LINK_COLUMNS} FROM workspace_issue_links
        WHERE org_id = $1 AND workspace_id = $2 AND unlinked_at IS NULL
        ORDER BY is_primary DESC, linked_at, id
        "#
    ))
    .bind(org_id)
    .bind(workspace_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("workspace link list", &error))
}

/// What an agent's issue argument names among a workspace's active links.
#[derive(Debug, Clone, Copy)]
pub enum LinkSelection<'a> {
    /// One of the links.
    Link(&'a WorkspaceIssueLinkRecord),
    /// A bare number (`12`, `#12`) that no link has.
    Unlinked(u64),
    /// A reference or URL that no link records.
    Reference,
}

/// Match an issue argument against `links` (active, primary first): none
/// means the primary link; a link id, a recorded reference
/// (`acme/web#12`, case-insensitive), its URL or a number selects that link.
/// A number several links share is refused rather than guessed.
pub fn select_workspace_link<'a>(
    links: &'a [WorkspaceIssueLinkRecord],
    argument: Option<&str>,
) -> Result<LinkSelection<'a>, G5Error> {
    let keys = || {
        if links.is_empty() {
            "none".to_string()
        } else {
            links
                .iter()
                .map(|link| link.display_key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let Some(wanted) = argument.map(str::trim).filter(|value| !value.is_empty()) else {
        return links
            .iter()
            .find(|link| link.is_primary)
            .map(LinkSelection::Link)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!(
                    "a primary issue: this workspace has none (linked issues: {})",
                    keys()
                ),
            });
    };
    if let Ok(id) = Uuid::parse_str(wanted) {
        return links
            .iter()
            .find(|link| link.id == id)
            .map(LinkSelection::Link)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("issue link {id} of this workspace"),
            });
    }
    let bare = wanted.strip_prefix('#').unwrap_or(wanted);
    if !bare.is_empty() && bare.bytes().all(|byte| byte.is_ascii_digit()) {
        let number = bare.parse::<u64>().ok();
        let found: Vec<_> = links
            .iter()
            .filter(|link| link.locator.get("number").and_then(Value::as_u64) == number)
            .collect();
        return match (found.as_slice(), number) {
            ([link], _) => Ok(LinkSelection::Link(link)),
            ([], Some(number)) => Ok(LinkSelection::Unlinked(number)),
            ([], None) => Err(G5Error::ValidationError {
                details: json!({"field": "ref", "error": "the issue number is not valid"}),
            }),
            (several, _) => Err(G5Error::ValidationError {
                details: json!({
                    "field": "ref",
                    "code": "ambiguous_issue",
                    "error": format!(
                        "#{bare} matches several linked issues ({}); use the full reference",
                        several
                            .iter()
                            .map(|link| link.display_key.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }),
            }),
        };
    }
    let url = wanted.trim_end_matches('/');
    Ok(links
        .iter()
        .find(|link| {
            link.display_key.eq_ignore_ascii_case(wanted)
                || link
                    .canonical_url
                    .trim_end_matches('/')
                    .eq_ignore_ascii_case(url)
        })
        .map_or(LinkSelection::Reference, LinkSelection::Link))
}

/// The active link of `links` recording `issue` (tracker and issue id).
#[must_use]
pub fn link_for_issue<'a>(
    links: &'a [WorkspaceIssueLinkRecord],
    summary: &ExternalIssueSummaryDTO,
) -> Option<&'a WorkspaceIssueLinkRecord> {
    links.iter().find(|link| {
        link.tracker_id == summary.tracker_id && link.external_issue_id == summary.external_issue_id
    })
}

/// Where a link's issue is read: its configured domain and the issue's
/// address there. The domain must still be configured for the link's
/// tracker: a link alone is not a read grant.
#[derive(Debug, Clone, Copy)]
pub struct LinkedIssueRef<'a> {
    pub access: &'a IssueDomainAccess,
    pub number: u64,
    pub location: LinkedLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedLocation {
    Github { repository_id: u64 },
    Native { issue_id: i64 },
}

/// The configured domain a link is read through, with the issue's address.
pub fn link_domain<'a>(
    domains: &'a [IssueDomainAccess],
    link: &WorkspaceIssueLinkRecord,
) -> Result<LinkedIssueRef<'a>, IssueError> {
    let locator = link.locator_dto();
    if let Some(access) = domains
        .iter()
        .find(|access| access.is_native() && access.domain.tracker_id == link.tracker_id)
    {
        return Ok(LinkedIssueRef {
            access,
            number: locator.number,
            location: LinkedLocation::Native {
                issue_id: native_issues::parse_issue_id(&link.external_issue_id)?,
            },
        });
    }
    if locator.repository_id == native_issues::NATIVE_LOCATOR_REPOSITORY {
        return Err(IssueError::new(
            ExternalIssueErrorKind::OutsideDomain,
            "the organization no longer uses G5 issues",
        ));
    }
    let repository_id = parse_canonical_github_id(&locator.repository_id).ok_or_else(|| {
        IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the link has no GitHub repository locator",
        )
    })?;
    let access = &domains[domain_for_repository(domains, repository_id)?];
    if access.domain.tracker_id != link.tracker_id {
        return Err(IssueError::new(
            ExternalIssueErrorKind::OutsideDomain,
            "the link belongs to another tracker",
        ));
    }
    Ok(LinkedIssueRef {
        access,
        number: locator.number,
        location: LinkedLocation::Github { repository_id },
    })
}

/// The issue read at the link's locator must still be the linked issue.
pub fn ensure_linked_issue(
    link: &WorkspaceIssueLinkRecord,
    issue: &GithubIssue,
) -> Result<(), IssueError> {
    if issue.id != link.external_issue_id {
        return Err(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the linked GitHub issue has moved",
        ));
    }
    Ok(())
}

/// A linked issue with its description and, when `with_comments`, its first
/// comment page, read inside its configured domain under the same checks as
/// link summaries.
pub async fn fetch_linked_issue_detail(
    backends: &IssueBackends<'_>,
    domains: &[IssueDomainAccess],
    link: &WorkspaceIssueLinkRecord,
    with_comments: bool,
) -> Result<ExternalIssueDetailDTO, IssueError> {
    let linked = link_domain(domains, link)?;
    match linked.location {
        LinkedLocation::Native { issue_id } => {
            let native = backends.native()?;
            let mut db = native.open().await?;
            let detail = native_issues::detail_by_id(
                &mut db,
                &native.reader(linked.access),
                issue_id,
                with_comments,
            )
            .await?;
            finish(db).await?;
            Ok(detail)
        }
        LinkedLocation::Github { repository_id } => {
            let client = backends.github()?;
            let token = domain_token(client, linked.access).await?;
            let (issue, comments) = tokio::join!(
                fetch_issue(client, &token, repository_id, linked.number),
                optional_comments(client, &token, repository_id, linked.number, with_comments),
            );
            let issue = issue?;
            ensure_linked_issue(link, &issue)?;
            Ok(detail(linked.access, repository_id, &issue, comments?))
        }
    }
}

/// Live summaries for links: GitHub ones fetched concurrently within the
/// interactive budget, native ones in one transaction. A link whose domain
/// is no longer configured is not read (a link is not a read grant).
pub async fn fetch_link_summaries(
    backends: &IssueBackends<'_>,
    domains: &[IssueDomainAccess],
    links: &[WorkspaceIssueLinkRecord],
) -> Vec<Result<ExternalIssueSummaryDTO, IssueError>> {
    let located: Vec<Result<LinkedIssueRef<'_>, IssueError>> = links
        .iter()
        .map(|link| link_domain(domains, link))
        .collect();
    let github = join_all(links.iter().zip(&located).map(|(link, linked)| async move {
        let linked = linked.as_ref().map_err(Clone::clone)?;
        let LinkedLocation::Github { repository_id } = linked.location else {
            return Ok(None);
        };
        let client = backends.github()?;
        let token = domain_token(client, linked.access).await?;
        let issue = fetch_issue(client, &token, repository_id, linked.number).await?;
        ensure_linked_issue(link, &issue)?;
        Ok(Some(summary(linked.access, repository_id, &issue)))
    }))
    .await;
    let wants_native = located
        .iter()
        .any(|linked| matches!(linked, Ok(linked) if matches!(linked.location, LinkedLocation::Native { .. })));
    let mut native_db = None;
    let mut native_error = None;
    if wants_native {
        match backends.native() {
            Ok(native) => match native.open().await {
                Ok(db) => native_db = Some((native, db)),
                Err(error) => native_error = Some(error),
            },
            Err(error) => native_error = Some(error),
        }
    }
    let mut outcomes = Vec::with_capacity(links.len());
    for (linked, github) in located.iter().zip(github) {
        let outcome = match (linked, github) {
            (_, Ok(Some(found))) => Ok(found),
            (_, Err(error)) => Err(error),
            (Ok(linked), Ok(None)) => {
                let LinkedLocation::Native { issue_id } = linked.location else {
                    unreachable!("GitHub links are read above")
                };
                match (&mut native_db, &native_error) {
                    (Some((native, db)), _) => {
                        native_issues::summary_by_id(db, &native.reader(linked.access), issue_id)
                            .await
                    }
                    (None, Some(error)) => Err(error.clone()),
                    (None, None) => Err(integration_unavailable()),
                }
            }
            (Err(error), Ok(None)) => Err(error.clone()),
        };
        outcomes.push(outcome);
    }
    if let Some((_, db)) = native_db {
        if let Err(error) = finish(db).await {
            for outcome in &mut outcomes {
                if outcome.is_ok() {
                    *outcome = Err(error.clone());
                }
            }
        }
    }
    outcomes
}

/// The discussion of a workspace's linked issue, as its tracker gives it.
#[derive(Debug, Clone)]
pub enum IssueDiscussion {
    /// GitHub's first timeline page, after the issue itself (whose
    /// description opens the discussion).
    Github {
        issue: GithubIssue,
        items: Vec<GithubTimelineItem>,
    },
    Native(NativeTimeline),
}

/// The first timeline page of a workspace's linked issue.
#[derive(Debug, Clone)]
pub struct LinkedIssueTimeline {
    pub discussion: IssueDiscussion,
    /// The tracker has more timeline entries than the first page.
    pub truncated: bool,
    /// From the issue's domain: governs Markdown rendering of comments.
    pub content_trusted: bool,
    /// The domain the issue was read in.
    pub domain_id: Uuid,
    /// An org admin allows G5 comments on the domain.
    pub comments_enabled: bool,
    /// The repository's audience as GitHub reports it now; `Unknown` when it
    /// could not be read. Native issues stay inside G5 (`NonPublic`).
    pub audience: RepositoryAudience,
}

impl LinkedIssueTimeline {
    #[must_use]
    pub fn is_native(&self) -> bool {
        matches!(self.discussion, IssueDiscussion::Native(_))
    }

    /// Whether the composer can post here, before the viewer's own account
    /// is considered: `ready`, `comments_disabled`, `public_destination` or
    /// `visibility_unknown`. Native comments publish nothing outside G5.
    #[must_use]
    pub fn composer_state(&self) -> &'static str {
        if self.is_native() {
            return "ready";
        }
        if !self.comments_enabled {
            return "comments_disabled";
        }
        match self.audience {
            RepositoryAudience::Public => "public_destination",
            RepositoryAudience::Unknown => "visibility_unknown",
            RepositoryAudience::NonPublic => "ready",
        }
    }
}

/// Timeline of a linked issue, read inside its configured domain. The same
/// checks as link summaries apply: the domain must still be configured for
/// the link's tracker, and the issue must still be the linked one.
pub async fn fetch_linked_issue_timeline(
    backends: &IssueBackends<'_>,
    domains: &[IssueDomainAccess],
    link: &WorkspaceIssueLinkRecord,
) -> Result<LinkedIssueTimeline, IssueError> {
    let linked = link_domain(domains, link)?;
    let access = linked.access;
    let repository_id = match linked.location {
        LinkedLocation::Native { issue_id } => {
            let native = backends.native()?;
            let mut db = native.open().await?;
            let timeline =
                native_issues::timeline_by_id(&mut db, &native.reader(access), issue_id).await?;
            finish(db).await?;
            return Ok(LinkedIssueTimeline {
                truncated: timeline.truncated,
                discussion: IssueDiscussion::Native(timeline),
                content_trusted: access.domain.content_trusted,
                domain_id: access.domain.id,
                comments_enabled: true,
                audience: RepositoryAudience::NonPublic,
            });
        }
        LinkedLocation::Github { repository_id } => repository_id,
    };
    let client = backends.github()?;
    let number = linked.number;
    let token = domain_token(client, access).await?;
    let (issue, timeline, repository) = tokio::join!(
        fetch_issue(client, &token, repository_id, number),
        fetch_issue_timeline(client, &token, repository_id, number),
        fetch_repository(client, &token, repository_id),
    );
    let issue = issue?;
    ensure_linked_issue(link, &issue)?;
    let (items, truncated) = timeline?;
    Ok(LinkedIssueTimeline {
        discussion: IssueDiscussion::Github { issue, items },
        truncated,
        content_trusted: access.domain.content_trusted,
        domain_id: access.domain.id,
        comments_enabled: access.domain.comments_enabled,
        audience: repository.map_or(RepositoryAudience::Unknown, |repository| {
            repository.audience
        }),
    })
}

/// The domain repository's audience as GitHub reports it now.
pub async fn domain_repository_audience(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
) -> Result<RepositoryAudience, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    Ok(fetch_repository(client, &token, repository_id)
        .await?
        .audience)
}

/// A comment on the issue, updated since `since`, whose body contains
/// `marker`: how an ambiguous G5 post is found.
pub async fn find_marked_comment(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    number: u64,
    since: &str,
    marker: &str,
) -> Result<Option<GithubIssueComment>, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    // `since` filters by update time, so edits to older comments share the
    // window: page until a short page or the cap.
    for page in 1..=MAX_RECONCILE_PAGES {
        let comments =
            fetch_issue_comments_since(client, &token, repository_id, number, since, page).await?;
        let last_page = comments.len() < github_issues::COMMENT_PAGE_SIZE;
        if let Some(found) = comments
            .into_iter()
            .find(|comment| comment.body.contains(marker))
        {
            return Ok(Some(found));
        }
        if last_page {
            break;
        }
    }
    Ok(None)
}

/// Comment pages searched when reconciling one submission.
const MAX_RECONCILE_PAGES: u32 = 10;

/// An issue `creator` opened in the domain since `since` whose description
/// ends with `marker`: how an ambiguous G5 creation is found.
pub async fn find_marked_issue(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    creator: &str,
    since: &str,
    marker: &str,
) -> Result<Option<GithubIssue>, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    for page in 1..=MAX_RECONCILE_PAGES {
        let (issues, more) =
            fetch_issues_created_by(client, &token, repository_id, creator, since, page).await?;
        if let Some(found) = issues
            .into_iter()
            .find(|issue| crate::services::git::issue_creations::carries_marker(issue, marker))
        {
            return Ok(Some(found));
        }
        if !more {
            break;
        }
    }
    Ok(None)
}

/// The parent of the domain's issue `number`, or `None` when it has none.
pub async fn domain_parent_issue(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    number: u64,
) -> Result<Option<GithubIssue>, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    fetch_parent_issue(client, &token, repository_id, number).await
}

/// The domain's issue `number`, read with the installation token.
pub async fn domain_issue(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    number: u64,
) -> Result<GithubIssue, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    fetch_issue(client, &token, repository_id, number).await
}

/// One page (1-based) of the domain's GitHub labels, read with the
/// installation token, and whether more exist.
pub async fn domain_label_page(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    page: u32,
) -> Result<(Vec<GithubIssueLabel>, bool), IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    github_issues::fetch_label_page(client, &token, repository_id, page).await
}

/// The domain's GitHub label named `name` (without regard to case), if any.
pub async fn domain_label(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    name: &str,
) -> Result<Option<GithubIssueLabel>, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    github_issues::fetch_label(client, &token, repository_id, name).await
}

/// The domain's GitHub labels named `names` (each without regard to case),
/// in order, `None` for a name the catalog lacks: one installation token,
/// the lookups in parallel.
pub async fn domain_labels_named(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    names: &[String],
) -> Result<Vec<Option<GithubIssueLabel>>, IssueError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    join_all(
        names
            .iter()
            .map(|name| github_issues::fetch_label(client, &token, repository_id, name)),
    )
    .await
    .into_iter()
    .collect()
}

/// What a scan of a GitHub domain's catalog for a label id found.
#[derive(Debug, Clone)]
pub enum LabelScan {
    Found(GithubIssueLabel),
    Missing,
    /// The catalog has more pages than `max_pages`.
    TooLarge,
}

/// The domain's GitHub label with id `id`, read page by page (GitHub has no
/// read by id) with one installation token, through at most `max_pages`.
pub async fn domain_label_by_id(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    id: u64,
    max_pages: u32,
) -> Result<LabelScan, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    for page in 1..=max_pages {
        let (labels, more) =
            github_issues::fetch_label_page(client, &token, repository_id, page).await?;
        if let Some(found) = labels.into_iter().find(|label| label.id == Some(id)) {
            return Ok(LabelScan::Found(found));
        }
        if !more {
            return Ok(LabelScan::Missing);
        }
    }
    Ok(LabelScan::TooLarge)
}

/// Whether `login` can be assigned to the domain's issues.
pub async fn domain_login_assignable(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    login: &str,
) -> Result<bool, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    github_issues::is_assignable(client, &token, repository_id, login).await
}

/// Accounts that can be assigned to the domain's issues (first page).
pub async fn domain_assignable_users(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
) -> Result<Vec<ExternalIssueUserDTO>, IssueError> {
    let repository_id = access.github_repository_id()?;
    let token = domain_token(client, access).await?;
    Ok(fetch_assignable_users(client, &token, repository_id)
        .await?
        .into_iter()
        .map(user_dto)
        .collect())
}

/// Requested association role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRole {
    Primary,
    Related,
}

impl LinkRole {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "primary" => Some(Self::Primary),
            "related" => Some(Self::Related),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        role_str(self == Self::Primary)
    }
}

/// Identity and routing metadata of a resolved issue, as an association
/// stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueLinkTarget {
    pub tracker_id: Uuid,
    pub external_issue_id: String,
    pub display_key: String,
    pub canonical_url: String,
    pub locator: Value,
}

impl IssueLinkTarget {
    #[must_use]
    pub fn from_summary(summary: &ExternalIssueSummaryDTO) -> Self {
        Self {
            tracker_id: summary.tracker_id,
            external_issue_id: summary.external_issue_id.clone(),
            display_key: summary.display_key.clone(),
            canonical_url: summary.canonical_url.clone(),
            locator: json!({
                "repositoryId": summary.locator.repository_id,
                "number": summary.locator.number,
            }),
        }
    }
}

fn link_conflict(code: &str, reason: &str, extra: Option<(&str, Value)>) -> G5Error {
    let mut details = json!({"code": code});
    if let Some((key, value)) = extra {
        details[key] = value;
    }
    G5Error::ConflictDetails {
        reason: reason.to_string(),
        details,
    }
}

async fn issue_has_primary_conflict(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    viewer: Option<Uuid>,
    org_id: Uuid,
    owner: Option<(Uuid, String)>,
) -> Result<G5Error, G5Error> {
    let owner_workspace = match owner {
        Some((workspace_id, name))
            if viewer_can_access(db, system_pool, viewer, org_id, workspace_id).await? =>
        {
            ExternalIssuePrimaryWorkspaceDTO::visible(workspace_id, name)
        }
        _ => ExternalIssuePrimaryWorkspaceDTO::hidden(),
    };
    Ok(link_conflict(
        "issue_has_primary",
        "the issue already has a primary workspace",
        Some((
            "ownerWorkspace",
            serde_json::to_value(owner_workspace).unwrap_or_else(|_| json!({"visible": false})),
        )),
    ))
}

async fn active_primary_owner(
    db: &mut ScopedDb,
    org_id: Uuid,
    tracker_id: Uuid,
    external_issue_id: &str,
) -> Result<Option<(Uuid, String)>, G5Error> {
    sqlx::query_as(
        r#"
        SELECT link.workspace_id, workspace.name
        FROM workspace_issue_links AS link
        JOIN workspaces AS workspace
          ON workspace.id = link.workspace_id AND workspace.org_id = link.org_id
        WHERE link.org_id = $1 AND link.tracker_id = $2 AND link.external_issue_id = $3
          AND link.is_primary AND link.unlinked_at IS NULL
        "#,
    )
    .bind(org_id)
    .bind(tracker_id)
    .bind(external_issue_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("primary owner lookup", &error))
}

/// Create an association for a resolved domain issue.
///
/// The workspace row is locked `FOR SHARE` while it is still live, in the
/// same transaction as the insert, so deletion (which updates that row and
/// then releases links) cannot interleave with link creation. The partial
/// unique indexes arbitrate concurrent claims; each violation maps to the
/// contract's conflict code. `issue_has_primary` names the owner only when
/// `viewer` can access it. Base takes no links: it is read-only and not a
/// unit of work (`g5.issue.base-links`), so the refusal is the read-only one.
#[allow(clippy::too_many_arguments)]
pub async fn create_workspace_link(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    viewer: Option<Uuid>,
    org_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    summary: &ExternalIssueSummaryDTO,
    role: LinkRole,
) -> Result<WorkspaceIssueLinkRecord, G5Error> {
    let live: Option<bool> = sqlx::query_scalar(
        "SELECT is_base FROM workspaces WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL FOR SHARE",
    )
    .bind(workspace_id)
    .bind(org_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("workspace lock", &error))?;
    crate::services::workspace_mutation_policy::refuse_base_workspace(
        live,
        workspace_id,
        "issue.link",
    )?;

    let is_primary = role == LinkRole::Primary;
    let existing: Option<bool> = sqlx::query_scalar(
        r#"
        SELECT true FROM workspace_issue_links
        WHERE workspace_id = $1 AND tracker_id = $2 AND external_issue_id = $3
          AND unlinked_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .bind(summary.tracker_id)
    .bind(&summary.external_issue_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("existing link check", &error))?;
    if existing.is_some() {
        return Err(already_linked());
    }
    if is_primary {
        let workspace_primary: Option<bool> = sqlx::query_scalar(
            "SELECT true FROM workspace_issue_links \
             WHERE workspace_id = $1 AND is_primary AND unlinked_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| db_error("workspace primary check", &error))?;
        if workspace_primary.is_some() {
            return Err(workspace_has_primary());
        }
        if let Some(owner) =
            active_primary_owner(db, org_id, summary.tracker_id, &summary.external_issue_id).await?
        {
            return Err(
                issue_has_primary_conflict(db, system_pool, viewer, org_id, Some(owner)).await?,
            );
        }
    }

    let target = IssueLinkTarget::from_summary(summary);
    match insert_link(db, org_id, workspace_id, actor, &target, is_primary).await? {
        Ok(record) => Ok(record),
        // A concurrent claim committed first; the unique index is the
        // arbiter.
        Err(constraint) => match constraint.as_str() {
            LINK_ACTIVE_PAIR_INDEX => Err(already_linked()),
            LINK_WORKSPACE_PRIMARY_INDEX => Err(workspace_has_primary()),
            LINK_ISSUE_PRIMARY_INDEX => {
                let owner =
                    active_primary_owner(db, org_id, target.tracker_id, &target.external_issue_id)
                        .await?;
                Err(issue_has_primary_conflict(db, system_pool, viewer, org_id, owner).await?)
            }
            other => Err(G5Error::Internal {
                message: format!("workspace issue link insert violated {other}"),
            }),
        },
    }
}

/// Insert an association under a savepoint. A unique-index violation rolls
/// back to the savepoint, leaving the transaction usable, and is returned as
/// the violated index name.
async fn insert_link(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    target: &IssueLinkTarget,
    is_primary: bool,
) -> Result<Result<WorkspaceIssueLinkRecord, String>, G5Error> {
    sqlx::query("SAVEPOINT workspace_issue_link_insert")
        .execute(db.conn())
        .await
        .map_err(|error| db_error("link savepoint", &error))?;
    let inserted = sqlx::query_as::<_, WorkspaceIssueLinkRecord>(&format!(
        r#"
        INSERT INTO workspace_issue_links
            (org_id, workspace_id, tracker_id, external_issue_id, display_key, canonical_url,
             locator, is_primary, linked_by, last_resolved_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
        RETURNING {LINK_COLUMNS}
        "#
    ))
    .bind(org_id)
    .bind(workspace_id)
    .bind(target.tracker_id)
    .bind(&target.external_issue_id)
    .bind(&target.display_key)
    .bind(&target.canonical_url)
    .bind(&target.locator)
    .bind(is_primary)
    .bind(actor)
    .fetch_one(db.conn())
    .await;
    match inserted {
        Ok(record) => {
            sqlx::query("RELEASE SAVEPOINT workspace_issue_link_insert")
                .execute(db.conn())
                .await
                .map_err(|error| db_error("link savepoint release", &error))?;
            Ok(Ok(record))
        }
        Err(error) => {
            let Some(constraint) = unique_violation_constraint(&error) else {
                return Err(db_error("link insert", &error));
            };
            sqlx::query("ROLLBACK TO SAVEPOINT workspace_issue_link_insert")
                .execute(db.conn())
                .await
                .map_err(|error| db_error("link savepoint rollback", &error))?;
            Ok(Err(constraint))
        }
    }
}

/// Refuse a primary claim on an issue that already has an active primary
/// workspace, with the same conflict as linking. A fork checks this when it
/// is admitted, before any work.
pub async fn ensure_primary_available(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    viewer: Option<Uuid>,
    org_id: Uuid,
    target: &IssueLinkTarget,
) -> Result<(), G5Error> {
    match active_primary_owner(db, org_id, target.tracker_id, &target.external_issue_id).await? {
        Some(owner) => {
            Err(issue_has_primary_conflict(db, system_pool, viewer, org_id, Some(owner)).await?)
        }
        None => Ok(()),
    }
}

/// Associate the workspace a fork just created, in the fork's completion
/// transaction. The workspace is new, so only the issue's primary claim can
/// conflict: when another workspace claimed it while the fork ran, the issue
/// is linked as related instead so the fork still completes.
pub async fn link_forked_workspace(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    actor: Uuid,
    target: &IssueLinkTarget,
    role: LinkRole,
) -> Result<WorkspaceIssueLinkRecord, G5Error> {
    let primary_available = role == LinkRole::Primary
        && active_primary_owner(db, org_id, target.tracker_id, &target.external_issue_id)
            .await?
            .is_none();
    if primary_available {
        match insert_link(db, org_id, workspace_id, actor, target, true).await? {
            Ok(record) => return Ok(record),
            Err(constraint) if constraint == LINK_ISSUE_PRIMARY_INDEX => {}
            Err(constraint) => {
                return Err(G5Error::Internal {
                    message: format!("forked workspace issue link violated {constraint}"),
                })
            }
        }
    }
    insert_link(db, org_id, workspace_id, actor, target, false)
        .await?
        .map_err(|constraint| G5Error::Internal {
            message: format!("forked workspace issue link violated {constraint}"),
        })
}

fn already_linked() -> G5Error {
    link_conflict(
        "already_linked",
        "the issue is already linked to this workspace",
        None,
    )
}

fn workspace_has_primary() -> G5Error {
    link_conflict(
        "workspace_has_primary",
        "the workspace already has a primary issue",
        None,
    )
}

/// Unlink an association of `workspace_id`. The external issue is never
/// touched. Unlinking an already-unlinked association of the same workspace
/// is a no-op; an unknown link is not found.
pub async fn unlink_workspace_link(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    link_id: Uuid,
    actor: Uuid,
) -> Result<(), G5Error> {
    let exists: Option<bool> = sqlx::query_scalar(
        r#"
        WITH released AS (
            UPDATE workspace_issue_links
            SET unlinked_at = now(), unlinked_by = $4
            WHERE org_id = $1 AND workspace_id = $2 AND id = $3 AND unlinked_at IS NULL
            RETURNING id
        )
        SELECT true FROM workspace_issue_links
        WHERE org_id = $1 AND workspace_id = $2 AND id = $3
        "#,
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(link_id)
    .bind(actor)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("link unlink", &error))?;
    if exists.is_none() {
        return Err(G5Error::NotFound {
            resource: format!("issue link {link_id}"),
        });
    }
    Ok(())
}

/// Release every active association of a workspace being deleted. Callers
/// run this in the transaction that tombstones the workspace, so the claims
/// are released exactly when deletion commits.
pub async fn release_workspace_issue_links(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    actor: Uuid,
) -> Result<u64, G5Error> {
    sqlx::query(
        r#"
        UPDATE workspace_issue_links
        SET unlinked_at = now(), unlinked_by = $2
        WHERE workspace_id = $1 AND unlinked_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .bind(actor)
    .execute(db.conn())
    .await
    .map(|result| result.rows_affected())
    .map_err(|error| db_error("workspace link release", &error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(domain: Uuid, key: &str, updated_at: &str) -> ExternalIssueSummaryDTO {
        ExternalIssueSummaryDTO {
            provider: "github",
            tracker_id: Uuid::nil(),
            domain_id: domain,
            external_issue_id: "1".to_string(),
            locator: ExternalIssueLocatorDTO {
                repository_id: "1".to_string(),
                number: 1,
            },
            display_key: key.to_string(),
            canonical_url: "https://github.example/x".to_string(),
            title: key.to_string(),
            state: "open".to_string(),
            state_reason: None,
            assignees: Vec::new(),
            labels: Vec::new(),
            updated_at: updated_at.to_string(),
            author: None,
            created_at: updated_at.to_string(),
            closed_at: None,
            comment_count: 0,
            content_trusted: false,
            primary_workspace: None,
        }
    }

    fn record(key: &str, number: u64, primary: bool) -> WorkspaceIssueLinkRecord {
        WorkspaceIssueLinkRecord {
            id: Uuid::new_v4(),
            org_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            tracker_id: Uuid::nil(),
            external_issue_id: format!("{key}-id"),
            display_key: key.to_string(),
            canonical_url: format!(
                "https://github.com/{}/issues/{number}",
                key.split('#').next().unwrap()
            ),
            locator: json!({"repositoryId": "1", "number": number}),
            is_primary: primary,
            linked_by: Uuid::nil(),
            linked_at: Utc::now(),
        }
    }

    #[test]
    fn an_agent_argument_selects_a_link_by_primary_id_reference_url_or_number() {
        let links = vec![
            record("acme/web#12", 12, true),
            record("acme/api#12", 12, false),
            record("acme/web#15", 15, false),
        ];
        let selected = |argument: Option<&str>| match select_workspace_link(&links, argument) {
            Ok(LinkSelection::Link(link)) => link.display_key.clone(),
            other => panic!("{argument:?}: {other:?}"),
        };
        assert_eq!(selected(None), "acme/web#12");
        assert_eq!(selected(Some(" ")), "acme/web#12");
        assert_eq!(selected(Some("ACME/web#15")), "acme/web#15");
        assert_eq!(selected(Some("#15")), "acme/web#15");
        assert_eq!(
            selected(Some("https://github.com/acme/api/issues/12/")),
            "acme/api#12"
        );
        let id = links[1].id.to_string();
        assert_eq!(selected(Some(&id)), "acme/api#12");

        // Unlinked numbers and references are left to the caller.
        assert!(matches!(
            select_workspace_link(&links, Some("40")),
            Ok(LinkSelection::Unlinked(40))
        ));
        assert!(matches!(
            select_workspace_link(&links, Some("acme/web#40")),
            Ok(LinkSelection::Reference)
        ));
    }

    #[test]
    fn an_agent_argument_never_guesses_between_links() {
        let links = vec![
            record("acme/web#12", 12, false),
            record("acme/api#12", 12, false),
        ];
        let error = select_workspace_link(&links, Some("#12")).unwrap_err();
        assert!(
            matches!(&error, G5Error::ValidationError { details }
                if details["code"] == "ambiguous_issue"
                && details["error"].as_str().unwrap().contains("acme/web#12, acme/api#12")),
            "{error:?}"
        );
        // No primary, and an unknown link id, are not found.
        let error = select_workspace_link(&links, None).unwrap_err();
        assert!(
            error.to_string().contains("acme/web#12, acme/api#12"),
            "{error}"
        );
        assert!(matches!(
            select_workspace_link(&links, Some(&Uuid::new_v4().to_string())),
            Err(G5Error::NotFound { .. })
        ));
        let error = select_workspace_link(&[], None).unwrap_err();
        assert!(error.to_string().contains("linked issues: none"), "{error}");
    }

    #[test]
    fn merged_search_page_interleaves_domains_by_recency_and_reports_truncation() {
        let (older, newer) = (Uuid::new_v4(), Uuid::new_v4());
        // The first domain alone fills a page with older matches.
        let mut results: Vec<_> = (0..3)
            .map(|index| {
                result(
                    older,
                    &format!("a/old#{index}"),
                    &format!("2026-01-0{index}T00:00:00Z"),
                )
            })
            .collect();
        results.push(result(newer, "b/new#1", "2026-09-01T00:00:00Z"));
        assert!(merge_search_page(&mut results, 3));
        let keys: Vec<_> = results
            .iter()
            .map(|item| item.display_key.as_str())
            .collect();
        assert_eq!(keys, vec!["b/new#1", "a/old#2", "a/old#1"]);

        let mut exact = vec![result(older, "a/one#1", "2026-01-01T00:00:00Z")];
        assert!(!merge_search_page(&mut exact, 3));
    }

    #[test]
    fn unavailable_reasons_map_from_binding_ineligibility() {
        use GithubAppIneligibleReason as Reason;
        assert_eq!(
            IssueDomainUnavailableReason::from(&Reason::NoBinding),
            IssueDomainUnavailableReason::NoBinding
        );
        assert_eq!(
            IssueDomainUnavailableReason::from(&Reason::BindingInactive),
            IssueDomainUnavailableReason::BindingInactive
        );
        assert_eq!(
            IssueDomainUnavailableReason::from(&Reason::RepositoryUncovered),
            IssueDomainUnavailableReason::RepositoryUncovered
        );
        assert_eq!(
            IssueDomainUnavailableReason::from(&Reason::CapabilityMissing {
                required: GithubAppCapability::IssuesRead
            }),
            IssueDomainUnavailableReason::IssuesPermissionMissing
        );
        assert_eq!(
            serde_json::to_value(IssueDomainUnavailableReason::IssuesPermissionMissing).unwrap(),
            json!("issues_permission_missing")
        );
    }

    #[test]
    fn primary_workspace_serializes_visible_and_hidden_forms() {
        let id = Uuid::nil();
        assert_eq!(
            serde_json::to_value(ExternalIssuePrimaryWorkspaceDTO::visible(id, "ws".into()))
                .unwrap(),
            json!({"visible": true, "workspaceId": id, "name": "ws"})
        );
        assert_eq!(
            serde_json::to_value(ExternalIssuePrimaryWorkspaceDTO::hidden()).unwrap(),
            json!({"visible": false})
        );
    }

    /// Records the windows a reservation asks for; admits everything.
    #[derive(Default)]
    struct RecordingStore(std::sync::Mutex<Vec<(String, u32)>>);

    #[async_trait::async_trait]
    impl crate::middleware::RateLimitStore for RecordingStore {
        async fn try_acquire(
            &self,
            _key: &str,
            _config: &crate::middleware::RateLimitConfig,
        ) -> Result<(), crate::middleware::RateLimitError> {
            unreachable!("agent search reserves its windows as a pair")
        }

        async fn try_acquire_pair(
            &self,
            first_key: &str,
            first_config: &crate::middleware::RateLimitConfig,
            second_key: &str,
            second_config: &crate::middleware::RateLimitConfig,
        ) -> Result<(), crate::middleware::RateLimitError> {
            let mut seen = self.0.lock().unwrap();
            seen.push((first_key.to_string(), first_config.requests_per_minute));
            seen.push((second_key.to_string(), second_config.requests_per_minute));
            Ok(())
        }
    }

    /// Only single keys: a pair cannot be reserved atomically.
    struct SingleKeyStore;

    #[async_trait::async_trait]
    impl crate::middleware::RateLimitStore for SingleKeyStore {
        async fn try_acquire(
            &self,
            _key: &str,
            _config: &crate::middleware::RateLimitConfig,
        ) -> Result<(), crate::middleware::RateLimitError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn agent_search_windows_are_sized_from_the_configured_domains_and_fail_closed() {
        let (org, workspace) = (Uuid::new_v4(), Uuid::new_v4());
        for (domains, org_limit) in [(0, 15), (1, 15), (2, 7), (4, 3), (20, 1)] {
            let store = RecordingStore::default();
            reserve_agent_search(&store, org, workspace, domains)
                .await
                .unwrap();
            assert_eq!(
                *store.0.lock().unwrap(),
                vec![
                    (
                        format!("g5:rate-limit:{{agent-issue-search}}:workspace:{workspace}"),
                        AGENT_SEARCHES_PER_WORKSPACE
                    ),
                    (
                        format!("g5:rate-limit:{{agent-issue-search}}:org:{org}"),
                        org_limit
                    ),
                ],
                "{domains} domains"
            );
        }
        assert!(matches!(
            reserve_agent_search(&SingleKeyStore, org, workspace, 1).await,
            Err(G5Error::ServiceUnavailable { .. })
        ));
    }

    #[test]
    fn link_conflicts_carry_codes_and_owner() {
        let response = already_linked().to_error_response(String::new());
        assert_eq!(response.code, "CONFLICT");
        assert_eq!(response.details, json!({"code": "already_linked"}));
        let conflict = link_conflict(
            "issue_has_primary",
            "x",
            Some(("ownerWorkspace", json!({"visible": false}))),
        );
        assert_eq!(conflict.status_code().as_u16(), 409);
        assert_eq!(
            conflict.to_error_response(String::new()).details,
            json!({"code": "issue_has_primary", "ownerWorkspace": {"visible": false}})
        );
        assert_eq!(LinkRole::parse("primary"), Some(LinkRole::Primary));
        assert_eq!(LinkRole::parse("related"), Some(LinkRole::Related));
        assert_eq!(LinkRole::parse("owner"), None);
    }
}
