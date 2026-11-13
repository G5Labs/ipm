//! G5's native issue tracker (`g5.issue.native`): the adapter that stores
//! issues in Postgres instead of an external tracker.
//!
//! The organization's native domain is an `issue_domains` row like any other
//! (provider `g5`); its issues, assignees, comments and state events live in
//! the `native_issue*` tables of migration 112, limited to the common
//! envelope (`g5.issue.native.scope`). Every table read or written here goes
//! through the caller's tenant [`ScopedDb`]: row-level security confines it
//! to the organization, and there is no system access path to issue content.
//! Member profiles and memberships are G5 identity data the tenant role
//! cannot read; they come from the system pool, filtered by organization.
//!
//! Nothing a native write does leaves G5, so the external publication guards
//! (comment switch, audience, connected account, secret screen, markers,
//! publication records) do not apply (`g5.issue.native.writes`). Creation and
//! comments are keyed by the author's idempotency key instead: a retry
//! returns the first result, exactly once. Edits apply only to the version
//! the editor saw, checked in the same update.

mod attachments;
pub use attachments::*;

use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use chrono::{DateTime, SecondsFormat, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::{ExternalIssueErrorKind, G5Error};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::git::external_issues::{
    ExternalIssueCommentDTO, ExternalIssueDetailDTO, ExternalIssueLocatorDTO,
    ExternalIssueSummaryDTO, ExternalIssueUserDTO, IssueDomainRecord, DOMAIN_COLUMNS,
};
use crate::services::git::github_issues::{IssueError, IssueStateFilter, SEARCH_PAGE_SIZE};
use crate::services::git::issue_comments::MAX_COMMENT_CHARS;
use crate::services::git::issue_creations::{MAX_ASSIGNEES, MAX_TITLE_CHARS};
use crate::services::git::native_issue_query::{
    person_matches, state_word, Clause, NativeQuery, Person, Term,
};

pub mod labels;

pub const PROVIDER_G5: &str = "g5";
pub const NATIVE_INSTANCE_KEY: &str = "g5";
pub const CONTAINER_KIND_G5_PROJECT: &str = "g5_project";
const NATIVE_DISPLAY_NAME: &str = "G5";
/// The native domain's container id: one native domain per organization.
const NATIVE_CONTAINER_ID: &str = "1";
/// The locator's `repositoryId` for a native issue. Not numeric, so it can
/// never be mistaken for a GitHub repository id; clients key issue pages and
/// tabs by `(repositoryId, number)`.
pub const NATIVE_LOCATOR_REPOSITORY: &str = "g5";
/// Comments and events one Activity read returns, as GitHub's first page.
pub const NATIVE_TIMELINE_LIMIT: i64 = 100;
/// The assignee standing for the caller, as in `gh`.
const ASSIGN_SELF: &str = "@me";

fn db_error(context: &str, error: &sqlx::Error) -> G5Error {
    if g5_contracts::error::sqlx_error_is_retryable(error) {
        return G5Error::DatabaseRetryable {
            message: format!("native issue {context} failed: {error}"),
        };
    }
    G5Error::Internal {
        message: format!("native issue {context} failed: {error}"),
    }
}

fn invalid(field: &str, error: impl Into<String>) -> G5Error {
    G5Error::ValidationError {
        details: json!({"field": field, "error": error.into()}),
    }
}

fn conflict(code: &str, reason: impl Into<String>) -> G5Error {
    G5Error::ConflictDetails {
        reason: reason.into(),
        details: json!({"code": code}),
    }
}

fn issue_not_found(number: u64) -> IssueError {
    IssueError::new(
        ExternalIssueErrorKind::NotFound,
        format!("issue {number} does not exist"),
    )
}

/// A read failure reported per issue or domain (`g5.issue.read-failure`).
#[must_use]
pub fn read_error(error: &G5Error) -> IssueError {
    match error {
        G5Error::NotFound { resource } => {
            IssueError::new(ExternalIssueErrorKind::NotFound, resource.clone())
        }
        other => IssueError::unavailable(format!("G5 issues could not be read: {other}")),
    }
}

/// Wire time: RFC 3339 in UTC at the database's microsecond precision, so a
/// value read back compares exactly in conditional edits.
#[must_use]
pub fn wire_time(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn parse_wire_time(field: &str, value: &str) -> Result<DateTime<Utc>, G5Error> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| invalid(field, "must be the RFC 3339 time the editor read"))
}

// ---------------------------------------------------------------------------
// Site: where native issue pages live
// ---------------------------------------------------------------------------

/// The deployment's public address and the organization's slug: native
/// issue URLs are built from them when read, not stored.
#[derive(Debug, Clone)]
pub struct NativeSite {
    base_url: String,
    org_slug: String,
}

impl NativeSite {
    #[must_use]
    pub fn new(base_url: &str, org_slug: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            org_slug: org_slug.to_string(),
        }
    }

    /// `/<org>/issues/g5/<number>`, the issue page.
    #[must_use]
    pub fn issue_path(&self, number: u64) -> String {
        format!("/{}/issues/g5/{number}", self.org_slug)
    }

    #[must_use]
    pub fn issue_url(&self, number: u64) -> String {
        format!("{}{}", self.base_url, self.issue_path(number))
    }

    fn comment_url(&self, number: u64, comment_id: i64) -> String {
        format!("{}#comment-{comment_id}", self.issue_url(number))
    }

    /// The issue number a URL or path of this site names, if it names one.
    #[must_use]
    pub fn parse_issue_url(&self, reference: &str) -> Option<u64> {
        let reference = reference.trim().trim_end_matches('/');
        let path = reference
            .strip_prefix(&self.base_url)
            .unwrap_or(reference)
            .split(['#', '?'])
            .next()?;
        let number = path.strip_prefix(&format!("/{}/issues/g5/", self.org_slug))?;
        parse_number(number)
    }
}

/// The site for an organization: its slug from the system pool (the tenant
/// role cannot read `orgs`).
pub async fn native_site(
    system_pool: &PgPool,
    base_url: &str,
    org_id: Uuid,
) -> Result<NativeSite, G5Error> {
    let slug: String = sqlx::query_scalar("SELECT slug FROM orgs WHERE id = $1")
        .bind(org_id)
        .fetch_optional(system_pool)
        .await
        .map_err(|error| db_error("organization lookup", &error))?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("organization {org_id}"),
        })?;
    Ok(NativeSite::new(base_url, &slug))
}

fn parse_number(value: &str) -> Option<u64> {
    (!value.is_empty() && !value.starts_with('0') && value.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.parse::<u64>().ok())
        .flatten()
        .filter(|number| *number <= i64::MAX as u64)
}

/// The number a native reference names in the domain keyed `key`:
/// `KEY-12` (case-insensitive) or the site's issue URL. Bare numbers are
/// the caller's to qualify ([`crate::services::issue_creations::target_reference_for`]).
#[must_use]
pub fn parse_reference(reference: &str, key: &str, site: &NativeSite) -> Option<u64> {
    let reference = reference.trim();
    if let Some(number) = site.parse_issue_url(reference) {
        return Some(number);
    }
    let (prefix, number) = reference.rsplit_once('-')?;
    prefix
        .eq_ignore_ascii_case(key)
        .then(|| parse_number(number))
        .flatten()
}

// ---------------------------------------------------------------------------
// Domain
// ---------------------------------------------------------------------------

/// The issue key of a native domain: 2–10 uppercase letters and digits,
/// starting with a letter (`ACME` in `ACME-12`).
pub fn validate_issue_key(raw: &str) -> Result<String, G5Error> {
    let key = raw.trim().to_ascii_uppercase();
    let valid = (2..=10).contains(&key.len())
        && key.starts_with(|c: char| c.is_ascii_uppercase())
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if valid {
        Ok(key)
    } else {
        Err(invalid(
            "key",
            "the issue key must be 2-10 letters and digits, starting with a letter",
        ))
    }
}

/// A default issue key from the organization's slug.
#[must_use]
pub fn default_issue_key(slug: &str) -> String {
    let mut key: String = slug
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .skip_while(char::is_ascii_digit)
        .take(10)
        .collect();
    if key.len() < 2 {
        key = "G5".to_string();
    }
    key
}

/// Get or create the organization's native tracker.
async fn get_or_create_native_tracker(db: &mut ScopedDb, org_id: Uuid) -> Result<Uuid, G5Error> {
    sqlx::query(
        r#"
        INSERT INTO issue_trackers
            (org_id, provider, instance_key, display_name, web_base_url, api_base_url)
        VALUES ($1, $2, $3, $4, '', '')
        ON CONFLICT ON CONSTRAINT issue_trackers_instance_key DO NOTHING
        "#,
    )
    .bind(org_id)
    .bind(PROVIDER_G5)
    .bind(NATIVE_INSTANCE_KEY)
    .bind(NATIVE_DISPLAY_NAME)
    .execute(db.conn())
    .await
    .map_err(|error| db_error("tracker insert", &error))?;
    sqlx::query_scalar(
        "SELECT id FROM issue_trackers WHERE org_id = $1 AND provider = $2 AND instance_key = $3",
    )
    .bind(org_id)
    .bind(PROVIDER_G5)
    .bind(NATIVE_INSTANCE_KEY)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("tracker lookup", &error))
}

/// Add the organization's native domain (`g5.issue.native.domain`). The
/// caller holds the organization's domain lock. When it is the first domain
/// it becomes the default creation domain.
pub async fn create_native_domain(
    db: &mut ScopedDb,
    org_id: Uuid,
    actor: Uuid,
    key: &str,
    content_trusted: bool,
) -> Result<IssueDomainRecord, G5Error> {
    let key = validate_issue_key(key)?;
    let tracker_id = get_or_create_native_tracker(db, org_id).await?;
    let is_default: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM issue_domains WHERE org_id = $1 AND is_default_create_target)",
    )
    .bind(org_id)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("default domain check", &error))?;
    sqlx::query_as(&format!(
        r#"
        INSERT INTO issue_domains
            (org_id, tracker_id, provider, container_kind, external_container_id, display_key,
             locator, is_default_create_target, content_trusted, configured_by)
        VALUES ($1, $2, $3, $4, $5, $6, '{{}}'::jsonb, $7, $8, $9)
        RETURNING {DOMAIN_COLUMNS}
        "#
    ))
    .bind(org_id)
    .bind(tracker_id)
    .bind(PROVIDER_G5)
    .bind(CONTAINER_KIND_G5_PROJECT)
    .bind(NATIVE_CONTAINER_ID)
    .bind(&key)
    .bind(is_default)
    .bind(content_trusted)
    .bind(actor)
    .fetch_one(db.conn())
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            conflict("domain_exists", "the organization already uses G5 issues")
        }
        _ => db_error("domain insert", &error),
    })
}

/// Whether the native domain still has issues: it cannot be removed then,
/// since nothing else could reach them.
pub async fn domain_has_issues(db: &mut ScopedDb, domain_id: Uuid) -> Result<bool, G5Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM native_issues WHERE domain_id = $1)")
        .bind(domain_id)
        .fetch_one(db.conn())
        .await
        .map_err(|error| db_error("domain issue check", &error))
}

// ---------------------------------------------------------------------------
// Members: profiles, assignees
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ProfileRow {
    id: Uuid,
    email: String,
    display_name: String,
    avatar_url: Option<String>,
}

fn profile_dto(row: ProfileRow) -> ExternalIssueUserDTO {
    ExternalIssueUserDTO {
        login: row.email,
        avatar_url: row.avatar_url,
        user_id: Some(row.id),
        display_name: Some(row.display_name),
    }
}

/// Profiles of `ids`, for authors, assignees and actors.
pub async fn load_profiles(
    system_pool: &PgPool,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, ExternalIssueUserDTO>, G5Error> {
    let ids: Vec<Uuid> = ids
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<ProfileRow> =
        sqlx::query_as("SELECT id, email, display_name, avatar_url FROM users WHERE id = ANY($1)")
            .bind(&ids)
            .fetch_all(system_pool)
            .await
            .map_err(|error| db_error("profile lookup", &error))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.id, profile_dto(row)))
        .collect())
}

fn profile(profiles: &HashMap<Uuid, ExternalIssueUserDTO>, id: Uuid) -> ExternalIssueUserDTO {
    profiles
        .get(&id)
        .cloned()
        .unwrap_or_else(|| ExternalIssueUserDTO {
            login: id.to_string(),
            avatar_url: None,
            user_id: Some(id),
            display_name: None,
        })
}

/// Active organization members who can write: who native issues can be
/// assigned to.
const ASSIGNABLE_MEMBERS: &str = "SELECT u.id, u.email, u.display_name, u.avatar_url \
     FROM org_memberships m JOIN users u ON u.id = m.user_id \
     WHERE m.org_id = $1 AND m.role IN ('admin', 'member') \
       AND m.deactivated_at IS NULL";

/// Members a native issue can be assigned to, by name.
pub async fn assignable_users(
    system_pool: &PgPool,
    org_id: Uuid,
) -> Result<Vec<ExternalIssueUserDTO>, G5Error> {
    let rows: Vec<ProfileRow> = sqlx::query_as(&format!(
        "{ASSIGNABLE_MEMBERS} ORDER BY lower(u.display_name), lower(u.email)"
    ))
    .bind(org_id)
    .fetch_all(system_pool)
    .await
    .map_err(|error| db_error("assignable members", &error))?;
    Ok(rows.into_iter().map(profile_dto).collect())
}

/// Resolve assignees as people and agents send them: `@me` is `caller`,
/// anything else a member's email or user id. Each must be an active member
/// who can write; an unknown one is refused, never dropped.
pub async fn resolve_assignees(
    system_pool: &PgPool,
    org_id: Uuid,
    caller: Uuid,
    requested: &[String],
) -> Result<Vec<Uuid>, G5Error> {
    let members: Vec<ProfileRow> = sqlx::query_as(ASSIGNABLE_MEMBERS)
        .bind(org_id)
        .fetch_all(system_pool)
        .await
        .map_err(|error| db_error("assignable members", &error))?;
    let mut resolved: Vec<Uuid> = Vec::new();
    for raw in requested {
        let wanted = raw.trim();
        if wanted.is_empty() {
            continue;
        }
        let id = if wanted.eq_ignore_ascii_case(ASSIGN_SELF) {
            members.iter().find(|member| member.id == caller)
        } else {
            let as_id = Uuid::parse_str(wanted).ok();
            members.iter().find(|member| {
                Some(member.id) == as_id || member.email.eq_ignore_ascii_case(wanted)
            })
        }
        .map(|member| member.id)
        .ok_or_else(|| {
            invalid(
                "assignees",
                format!("{wanted} is not an organization member who can be assigned issues"),
            )
        })?;
        if !resolved.contains(&id) {
            resolved.push(id);
        }
    }
    if resolved.len() > MAX_ASSIGNEES {
        return Err(invalid(
            "assignees",
            format!("an issue can have at most {MAX_ASSIGNEES} assignees"),
        ));
    }
    Ok(resolved)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A native issue's title: one non-empty line within the limit.
pub fn validate_title(title: &str) -> Result<String, G5Error> {
    let title = title.trim();
    if title.is_empty() {
        return Err(invalid("title", "the title is empty"));
    }
    if title.contains(['\r', '\n']) {
        return Err(invalid("title", "the title must be one line"));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(invalid(
            "title",
            format!("the title is longer than {MAX_TITLE_CHARS} characters"),
        ));
    }
    Ok(title.to_string())
}

/// A description or comment within the limit, trailing whitespace trimmed.
pub fn validate_text(field: &str, text: &str, required: bool) -> Result<String, G5Error> {
    let text = text.trim_end();
    if required && text.trim().is_empty() {
        return Err(invalid(field, "the comment is empty"));
    }
    if text.chars().count() > MAX_COMMENT_CHARS {
        return Err(invalid(
            field,
            format!("the text is longer than {MAX_COMMENT_CHARS} characters"),
        ));
    }
    Ok(text.to_string())
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
struct IssueRow {
    id: i64,
    number: i64,
    title: String,
    body: String,
    state: String,
    state_reason: Option<String>,
    parent_id: Option<i64>,
    author_user_id: Uuid,
    origin: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    closed_by: Option<Uuid>,
    comment_count: i64,
    assignees: Vec<Uuid>,
    /// The catalog's current name, colour and description of each label.
    labels: sqlx::types::Json<Vec<labels::LabelRow>>,
}

const ISSUE_SELECT: &str = "SELECT i.id, i.number, i.title, i.body, i.state, i.state_reason, \
     i.parent_id, i.author_user_id, i.origin, i.created_at, i.updated_at, i.closed_at, \
     i.closed_by, \
     (SELECT count(*) FROM native_issue_comments c WHERE c.issue_id = i.id) AS comment_count, \
     COALESCE((SELECT array_agg(a.user_id ORDER BY a.created_at, a.user_id) \
               FROM native_issue_assignees a WHERE a.issue_id = i.id), '{}') AS assignees, \
     COALESCE((SELECT json_agg(json_build_object('id', l.id, 'name', l.name, \
                                                 'color', l.color, \
                                                 'description', l.description) \
                               ORDER BY lower(l.name), l.id) \
               FROM native_issue_label_assignments la \
               JOIN native_issue_labels l ON l.id = la.label_id \
               WHERE la.issue_id = i.id), '[]'::json) AS labels \
     FROM native_issues i";

fn number_of(row: &IssueRow) -> u64 {
    u64::try_from(row.number).unwrap_or_default()
}

/// The native read context: the domain read and how URLs and names render.
pub struct NativeReader<'a> {
    pub domain: &'a IssueDomainRecord,
    pub site: &'a NativeSite,
    pub system_pool: &'a PgPool,
}

impl NativeReader<'_> {
    fn display_key(&self, number: u64) -> String {
        format!("{}-{number}", self.domain.display_key)
    }

    fn summary(
        &self,
        row: &IssueRow,
        profiles: &HashMap<Uuid, ExternalIssueUserDTO>,
    ) -> ExternalIssueSummaryDTO {
        let number = number_of(row);
        ExternalIssueSummaryDTO {
            provider: PROVIDER_G5,
            tracker_id: self.domain.tracker_id,
            domain_id: self.domain.id,
            external_issue_id: row.id.to_string(),
            locator: ExternalIssueLocatorDTO {
                repository_id: NATIVE_LOCATOR_REPOSITORY.to_string(),
                number,
            },
            display_key: self.display_key(number),
            canonical_url: self.site.issue_url(number),
            title: row.title.clone(),
            state: row.state.clone(),
            state_reason: row.state_reason.clone(),
            assignees: row
                .assignees
                .iter()
                .map(|id| profile(profiles, *id))
                .collect(),
            labels: row.labels.iter().map(labels::LabelRow::to_dto).collect(),
            updated_at: wire_time(row.updated_at),
            author: Some(profile(profiles, row.author_user_id)),
            created_at: wire_time(row.created_at),
            closed_at: row.closed_at.map(wire_time),
            comment_count: u64::try_from(row.comment_count).unwrap_or_default(),
            content_trusted: self.domain.content_trusted,
            primary_workspace: None,
        }
    }

    async fn summaries(&self, rows: &[IssueRow]) -> Result<Vec<ExternalIssueSummaryDTO>, G5Error> {
        let ids: Vec<Uuid> = rows
            .iter()
            .flat_map(|row| row.assignees.iter().copied().chain([row.author_user_id]))
            .collect();
        let profiles = load_profiles(self.system_pool, ids).await?;
        Ok(rows
            .iter()
            .map(|row| self.summary(row, &profiles))
            .collect())
    }

    async fn one_summary(&self, row: &IssueRow) -> Result<ExternalIssueSummaryDTO, G5Error> {
        Ok(self
            .summaries(std::slice::from_ref(row))
            .await?
            .pop()
            .unwrap_or_else(|| unreachable!("one row gives one summary")))
    }
}

async fn row_by_number(
    db: &mut ScopedDb,
    domain_id: Uuid,
    number: u64,
) -> Result<Option<IssueRow>, G5Error> {
    let Ok(number) = i64::try_from(number) else {
        return Ok(None);
    };
    sqlx::query_as(&format!(
        "{ISSUE_SELECT} WHERE i.domain_id = $1 AND i.number = $2"
    ))
    .bind(domain_id)
    .bind(number)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("issue lookup", &error))
}

async fn row_by_id(
    db: &mut ScopedDb,
    domain_id: Uuid,
    id: i64,
) -> Result<Option<IssueRow>, G5Error> {
    sqlx::query_as(&format!(
        "{ISSUE_SELECT} WHERE i.domain_id = $1 AND i.id = $2"
    ))
    .bind(domain_id)
    .bind(id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("issue lookup", &error))
}

/// The issue id a link records, as stored.
pub fn parse_issue_id(external_issue_id: &str) -> Result<i64, IssueError> {
    external_issue_id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::NotFound,
                "the link does not name a G5 issue",
            )
        })
}

/// Who a native search keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeAssignee {
    Any,
    User(Uuid),
    Unassigned,
}

/// The people a search may name: the domain's assignees, and its authors
/// when an `author:` qualifier names someone. Read from current profiles,
/// so a renamed member is found under the new name.
#[derive(Default)]
struct SearchPeople {
    assignees: Vec<Uuid>,
    authors: Vec<Uuid>,
    profiles: HashMap<Uuid, ExternalIssueUserDTO>,
}

impl SearchPeople {
    async fn load(
        db: &mut ScopedDb,
        reader: &NativeReader<'_>,
        query: &NativeQuery,
    ) -> Result<Self, G5Error> {
        let named = |person: &Person| matches!(person, Person::Named(_));
        let wants_assignees = query.terms().any(|term| match term {
            Term::Words { .. } => true,
            Term::Assignee(person) => named(person),
            _ => false,
        });
        let wants_authors = query
            .terms()
            .any(|term| matches!(term, Term::Author(person) if named(person)));
        let mut people = Self::default();
        if wants_assignees {
            people.assignees = sqlx::query_scalar(
                "SELECT DISTINCT a.user_id FROM native_issue_assignees a \
                 JOIN native_issues i ON i.id = a.issue_id WHERE i.domain_id = $1",
            )
            .bind(reader.domain.id)
            .fetch_all(db.conn())
            .await
            .map_err(|error| db_error("search people", &error))?;
        }
        if wants_authors {
            people.authors = sqlx::query_scalar(
                "SELECT DISTINCT author_user_id FROM native_issues WHERE domain_id = $1",
            )
            .bind(reader.domain.id)
            .fetch_all(db.conn())
            .await
            .map_err(|error| db_error("search people", &error))?;
        }
        let ids = people
            .assignees
            .iter()
            .chain(&people.authors)
            .copied()
            .collect();
        people.profiles = load_profiles(reader.system_pool, ids).await?;
        Ok(people)
    }

    fn matching(&self, among: &[Uuid], needle: &str) -> Vec<Uuid> {
        let needle = needle.to_lowercase();
        among
            .iter()
            .copied()
            .filter(|id| {
                self.profiles.get(id).is_some_and(|profile| {
                    person_matches(
                        &needle,
                        &profile.login,
                        profile.display_name.as_deref().unwrap_or_default(),
                    )
                })
            })
            .collect()
    }
}

enum SearchArg {
    Text(String),
    Texts(Vec<String>),
    Ids(Vec<Uuid>),
    Number(i64),
}

/// The SQL of a parsed query; arguments follow the search's own five.
struct SearchSql<'a> {
    people: &'a SearchPeople,
    viewer: Option<Uuid>,
    args: Vec<SearchArg>,
}

const ASSIGNED_TO: &str =
    "EXISTS (SELECT 1 FROM native_issue_assignees a WHERE a.issue_id = i.id AND a.user_id = ANY(";

impl SearchSql<'_> {
    fn bind(&mut self, arg: SearchArg) -> String {
        self.args.push(arg);
        format!("${}", 5 + self.args.len())
    }

    fn person(&self, person: &Person, among: &[Uuid]) -> Result<Vec<Uuid>, IssueError> {
        match person {
            Person::Me => self.viewer.map(|id| vec![id]).ok_or_else(|| {
                IssueError::new(ExternalIssueErrorKind::InvalidQuery, "@me needs a person")
            }),
            Person::Named(needle) => Ok(self.people.matching(among, needle)),
        }
    }

    fn ids(&mut self, ids: Vec<Uuid>, matched: impl FnOnce(&str) -> String) -> String {
        if ids.is_empty() {
            return "false".to_string();
        }
        let ids = self.bind(SearchArg::Ids(ids));
        matched(&ids)
    }

    fn clause(&mut self, clause: &Clause) -> Result<String, IssueError> {
        let sql = match &clause.term {
            Term::Words { text, phrase } => {
                let parse = if *phrase {
                    "phraseto_tsquery"
                } else {
                    "plainto_tsquery"
                };
                let text_arg = self.bind(SearchArg::Text(text.clone()));
                let query = format!("{parse}('english'::regconfig, {text_arg})");
                let mut matched = vec![format!("(numnode({query}) > 0 AND i.search @@ {query})")];
                if let Some(state) = state_word(text).filter(|_| !*phrase) {
                    matched.push(format!("i.state = '{state}'"));
                }
                let assignees = self.people.matching(&self.people.assignees, text);
                if !assignees.is_empty() {
                    let ids = self.bind(SearchArg::Ids(assignees));
                    matched.push(format!("{ASSIGNED_TO}{ids}))"));
                }
                let matched = matched.join(" OR ");
                // A word with nothing to search for (a stopword) is ignored,
                // as websearch_to_tsquery ignores it.
                return Ok(if clause.negated {
                    format!("NOT ({matched})")
                } else {
                    format!("(numnode({query}) = 0 OR {matched})")
                });
            }
            Term::Assignee(person) => {
                let ids = self.person(person, &self.people.assignees)?;
                self.ids(ids, |ids| format!("{ASSIGNED_TO}{ids}))"))
            }
            Term::Author(person) => {
                let ids = self.person(person, &self.people.authors)?;
                self.ids(ids, |ids| format!("i.author_user_id = ANY({ids})"))
            }
            Term::Unassigned => {
                "NOT EXISTS (SELECT 1 FROM native_issue_assignees a WHERE a.issue_id = i.id)"
                    .to_string()
            }
            Term::Label(names) => {
                let names = self.bind(SearchArg::Texts(names.clone()));
                format!(
                    "EXISTS (SELECT 1 FROM native_issue_label_assignments la \
                     JOIN native_issue_labels l ON l.id = la.label_id \
                     WHERE la.issue_id = i.id AND lower(l.name) = ANY({names}))"
                )
            }
            Term::Unlabeled => "NOT EXISTS (SELECT 1 FROM native_issue_label_assignments la \
                 WHERE la.issue_id = i.id)"
                .to_string(),
        };
        Ok(if clause.negated {
            format!("NOT ({sql})")
        } else {
            sql
        })
    }

    fn group(&mut self, group: &[Clause]) -> Result<String, IssueError> {
        let clauses = group
            .iter()
            .map(|clause| self.clause(clause))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("({})", clauses.join(" OR ")))
    }

    /// `AND`-prefixed conditions. Positive word groups may instead be met by
    /// a title fragment or the issue's key/number; qualifier groups and groups
    /// with a negated clause always filter, so no alternative bypasses `-`.
    fn conditions(
        &mut self,
        query: &NativeQuery,
        free_text: &str,
        number: Option<i64>,
    ) -> Result<String, IssueError> {
        let mut filters = Vec::new();
        let mut words = Vec::new();
        for group in &query.groups {
            let sql = self.group(group)?;
            if NativeQuery::is_text_group(group) && group.iter().all(|clause| !clause.negated) {
                words.push(sql);
            } else {
                filters.push(sql);
            }
        }
        if !words.is_empty() {
            let mut alternatives = vec![format!("({})", words.join(" AND "))];
            if !free_text.is_empty() {
                let fragment = format!(
                    "%{}%",
                    free_text
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                );
                let fragment = self.bind(SearchArg::Text(fragment));
                alternatives.push(format!("i.title ILIKE {fragment}"));
            }
            if let Some(number) = number {
                let number = self.bind(SearchArg::Number(number));
                alternatives.push(format!("i.number = {number}"));
            }
            filters.push(format!("({})", alternatives.join(" OR ")));
        }
        Ok(filters
            .iter()
            .map(|filter| format!(" AND {filter}"))
            .collect())
    }
}

/// One domain's matches, most recently updated first, and whether more
/// exist than a page (`g5.issue.native.search`). Words match the stemmed
/// title and description, the issue's state or an assignee; a title
/// fragment or the issue's key/number also matches the words.
pub async fn search(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    query: &str,
    state: IssueStateFilter,
    assignee: NativeAssignee,
    viewer: Option<Uuid>,
) -> Result<(Vec<ExternalIssueSummaryDTO>, bool), IssueError> {
    let parsed = NativeQuery::parse(query)?;
    let free_text = parsed.free_text();
    let number = {
        let bare = free_text.strip_prefix('#').unwrap_or(&free_text);
        parse_number(bare)
            .or_else(|| parse_reference(&free_text, &reference_key(reader), reader.site))
    }
    .and_then(|number| i64::try_from(number).ok());
    let people = SearchPeople::load(db, reader, &parsed)
        .await
        .map_err(|error| read_error(&error))?;
    let mut sql = SearchSql {
        people: &people,
        viewer,
        args: Vec::new(),
    };
    let conditions = sql.conditions(&parsed, &free_text, number)?;
    let state = match state {
        IssueStateFilter::Open => Some("open"),
        IssueStateFilter::Closed => Some("closed"),
        IssueStateFilter::All => None,
    };
    let (assignee_mode, assignee_id) = match assignee {
        NativeAssignee::Any => ("any", None),
        NativeAssignee::User(id) => ("user", Some(id)),
        NativeAssignee::Unassigned => ("none", None),
    };
    let limit = i64::try_from(SEARCH_PAGE_SIZE + 1).unwrap_or(i64::MAX);
    let statement = format!(
        r#"
        {ISSUE_SELECT}
        WHERE i.domain_id = $1
          AND ($2::text IS NULL OR i.state = $2)
          AND CASE $3
                WHEN 'user' THEN EXISTS (SELECT 1 FROM native_issue_assignees a
                                         WHERE a.issue_id = i.id AND a.user_id = $4)
                WHEN 'none' THEN NOT EXISTS (SELECT 1 FROM native_issue_assignees a
                                             WHERE a.issue_id = i.id)
                ELSE true END
          {conditions}
        ORDER BY i.updated_at DESC, i.id DESC
        LIMIT $5
        "#
    );
    let mut rows = sqlx::query_as::<_, IssueRow>(&statement)
        .bind(reader.domain.id)
        .bind(state)
        .bind(assignee_mode)
        .bind(assignee_id)
        .bind(limit);
    for arg in sql.args {
        rows = match arg {
            SearchArg::Text(text) => rows.bind(text),
            SearchArg::Texts(texts) => rows.bind(texts),
            SearchArg::Ids(ids) => rows.bind(ids),
            SearchArg::Number(number) => rows.bind(number),
        };
    }
    let rows: Vec<IssueRow> = rows
        .fetch_all(db.conn())
        .await
        .map_err(|error| read_error(&db_error("search", &error)))?;
    let has_more = rows.len() > SEARCH_PAGE_SIZE;
    let rows = &rows[..rows.len().min(SEARCH_PAGE_SIZE)];
    let found = reader
        .summaries(rows)
        .await
        .map_err(|error| read_error(&error))?;
    Ok((found, has_more))
}

fn reference_key(reader: &NativeReader<'_>) -> String {
    reader.domain.display_key.clone()
}

/// The issue numbered `number`, or `not_found`.
pub async fn summary_by_number(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    number: u64,
) -> Result<ExternalIssueSummaryDTO, IssueError> {
    let row = row_by_number(db, reader.domain.id, number)
        .await
        .map_err(|error| read_error(&error))?
        .ok_or_else(|| issue_not_found(number))?;
    reader
        .one_summary(&row)
        .await
        .map_err(|error| read_error(&error))
}

/// The issue a link records, by its stable id.
pub async fn summary_by_id(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    id: i64,
) -> Result<ExternalIssueSummaryDTO, IssueError> {
    let row = row_by_id(db, reader.domain.id, id)
        .await
        .map_err(|error| read_error(&error))?
        .ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::NotFound,
                "the linked issue does not exist",
            )
        })?;
    reader
        .one_summary(&row)
        .await
        .map_err(|error| read_error(&error))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CommentRow {
    id: i64,
    issue_id: i64,
    author_user_id: Uuid,
    origin: String,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const COMMENT_COLUMNS: &str = "id, issue_id, author_user_id, origin, body, created_at, updated_at";

#[derive(Debug, Clone, sqlx::FromRow)]
struct EventRow {
    id: i64,
    actor_user_id: Uuid,
    kind: String,
    subject_user_id: Option<Uuid>,
    label_name: Option<String>,
    created_at: DateTime<Utc>,
}

/// A native comment as Activity and the issue page show it.
#[derive(Debug, Clone)]
pub struct NativeComment {
    pub id: i64,
    pub author: ExternalIssueUserDTO,
    pub author_user_id: Uuid,
    pub origin: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub url: String,
}

impl NativeComment {
    #[must_use]
    pub fn to_dto(&self) -> ExternalIssueCommentDTO {
        ExternalIssueCommentDTO {
            id: self.id.to_string(),
            author: Some(self.author.clone()),
            body: self.body.clone(),
            created_at: wire_time(self.created_at),
            updated_at: wire_time(self.updated_at),
            url: self.url.clone(),
        }
    }
}

/// A native state or assignment change.
#[derive(Debug, Clone)]
pub struct NativeEvent {
    pub id: i64,
    pub actor: ExternalIssueUserDTO,
    /// `closed`, `reopened`, `assigned`, `unassigned`, `labeled` or
    /// `unlabeled`, as GitHub names them.
    pub event: String,
    /// The assignee's email for assignment events; the label's name at the
    /// time for label events.
    pub detail: Option<String>,
    pub state_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// The issue's discussion: description, comments and events, oldest first.
#[derive(Debug, Clone)]
pub struct NativeTimeline {
    pub summary: ExternalIssueSummaryDTO,
    pub body: Option<String>,
    pub comments: Vec<NativeComment>,
    pub events: Vec<NativeEvent>,
    /// More comments or events exist than one read returns.
    pub truncated: bool,
}

async fn comments_of(
    db: &mut ScopedDb,
    issue: &IssueRow,
) -> Result<(Vec<CommentRow>, bool), G5Error> {
    let mut rows: Vec<CommentRow> = sqlx::query_as(&format!(
        "SELECT {COMMENT_COLUMNS} FROM native_issue_comments WHERE issue_id = $1 \
         ORDER BY created_at, id LIMIT $2"
    ))
    .bind(issue.id)
    .bind(NATIVE_TIMELINE_LIMIT + 1)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("comment list", &error))?;
    let more = rows.len() > NATIVE_TIMELINE_LIMIT as usize;
    rows.truncate(NATIVE_TIMELINE_LIMIT as usize);
    Ok((rows, more))
}

fn comment_of(
    reader: &NativeReader<'_>,
    number: u64,
    row: CommentRow,
    profiles: &HashMap<Uuid, ExternalIssueUserDTO>,
) -> NativeComment {
    NativeComment {
        id: row.id,
        author: profile(profiles, row.author_user_id),
        author_user_id: row.author_user_id,
        origin: row.origin,
        url: reader.site.comment_url(number, row.id),
        body: row.body,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// The issue numbered `number` with its description and, when
/// `with_comments`, its first comments, oldest first.
pub async fn detail_by_number(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    number: u64,
    with_comments: bool,
) -> Result<ExternalIssueDetailDTO, IssueError> {
    let row = row_by_number(db, reader.domain.id, number)
        .await
        .map_err(|error| read_error(&error))?
        .ok_or_else(|| issue_not_found(number))?;
    detail_of(db, reader, &row, with_comments)
        .await
        .map_err(|error| read_error(&error))
}

/// The issue a link records, with its description and optional comments.
pub async fn detail_by_id(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    id: i64,
    with_comments: bool,
) -> Result<ExternalIssueDetailDTO, IssueError> {
    let row = row_by_id(db, reader.domain.id, id)
        .await
        .map_err(|error| read_error(&error))?
        .ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::NotFound,
                "the linked issue does not exist",
            )
        })?;
    detail_of(db, reader, &row, with_comments)
        .await
        .map_err(|error| read_error(&error))
}

async fn detail_of(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    row: &IssueRow,
    with_comments: bool,
) -> Result<ExternalIssueDetailDTO, G5Error> {
    let (comments, more) = if with_comments {
        comments_of(db, row).await?
    } else {
        (Vec::new(), false)
    };
    let ids: Vec<Uuid> = row
        .assignees
        .iter()
        .copied()
        .chain([row.author_user_id])
        .chain(comments.iter().map(|comment| comment.author_user_id))
        .collect();
    let profiles = load_profiles(reader.system_pool, ids).await?;
    let number = number_of(row);
    Ok(ExternalIssueDetailDTO {
        summary: reader.summary(row, &profiles),
        body: Some(row.body.clone()).filter(|body| !body.trim().is_empty()),
        comments: comments
            .into_iter()
            .map(|comment| comment_of(reader, number, comment, &profiles).to_dto())
            .collect(),
        comments_truncated: more,
        links: Vec::new(),
    })
}

/// The linked issue's discussion for Activity.
pub async fn timeline_by_id(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    id: i64,
) -> Result<NativeTimeline, IssueError> {
    let row = row_by_id(db, reader.domain.id, id)
        .await
        .map_err(|error| read_error(&error))?
        .ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::NotFound,
                "the linked issue does not exist",
            )
        })?;
    timeline_of(db, reader, &row)
        .await
        .map_err(|error| read_error(&error))
}

async fn timeline_of(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    row: &IssueRow,
) -> Result<NativeTimeline, G5Error> {
    let (comments, more_comments) = comments_of(db, row).await?;
    let mut events: Vec<EventRow> = sqlx::query_as(
        "SELECT id, actor_user_id, kind, subject_user_id, label_name, created_at \
         FROM native_issue_events WHERE issue_id = $1 ORDER BY created_at, id LIMIT $2",
    )
    .bind(row.id)
    .bind(NATIVE_TIMELINE_LIMIT + 1)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("event list", &error))?;
    let more_events = events.len() > NATIVE_TIMELINE_LIMIT as usize;
    events.truncate(NATIVE_TIMELINE_LIMIT as usize);
    let ids: Vec<Uuid> = row
        .assignees
        .iter()
        .copied()
        .chain([row.author_user_id])
        .chain(comments.iter().map(|comment| comment.author_user_id))
        .chain(
            events
                .iter()
                .flat_map(|event| [Some(event.actor_user_id), event.subject_user_id])
                .flatten(),
        )
        .collect();
    let profiles = load_profiles(reader.system_pool, ids).await?;
    let number = number_of(row);
    Ok(NativeTimeline {
        summary: reader.summary(row, &profiles),
        body: Some(row.body.clone()).filter(|body| !body.trim().is_empty()),
        comments: comments
            .into_iter()
            .map(|comment| comment_of(reader, number, comment, &profiles))
            .collect(),
        events: events
            .into_iter()
            .map(|event| NativeEvent {
                id: event.id,
                actor: profile(&profiles, event.actor_user_id),
                state_reason: match event.kind.as_str() {
                    "closed" => Some("completed".to_string()),
                    "reopened" => Some("reopened".to_string()),
                    _ => None,
                },
                detail: event
                    .subject_user_id
                    .map(|subject| profile(&profiles, subject).login)
                    .or(event.label_name),
                event: event.kind,
                created_at: event.created_at,
            })
            .collect(),
        truncated: more_comments || more_events,
    })
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/// A new native issue, from a person or a workspace's agent as its owner.
#[derive(Debug, Clone)]
pub struct NewNativeIssue<'a> {
    pub org_id: Uuid,
    pub author: Uuid,
    /// `person` or `agent`.
    pub origin: &'static str,
    pub idempotency_key: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub assignees: &'a [Uuid],
    /// Catalog label names (validated, each once); unknown ones are refused.
    pub labels: &'a [String],
    /// The parent's number in the same domain.
    pub parent: Option<u64>,
}

/// What a creation established: the issue, its parent and whether this
/// request created it (`false` for a replay of the key).
#[derive(Debug, Clone)]
pub struct NativeCreation {
    pub issue: ExternalIssueSummaryDTO,
    pub parent: Option<ExternalIssueSummaryDTO>,
    pub created: bool,
}

/// Create an issue in the native domain, or return the one an earlier
/// request with the same key created. The parent must be an existing issue
/// of the same domain and is set in the same transaction, so a creation
/// never half-succeeds. The domain row's lock serializes numbering.
pub async fn create_issue(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    new: &NewNativeIssue<'_>,
) -> Result<NativeCreation, G5Error> {
    let title = validate_title(new.title)?;
    let body = validate_text("body", new.body, false)?;
    sqlx::query("SELECT 1 FROM issue_domains WHERE org_id = $1 AND id = $2 FOR UPDATE")
        .bind(new.org_id)
        .bind(reader.domain.id)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| db_error("domain lock", &error))?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("issue domain {}", reader.domain.id),
        })?;

    let parent = match new.parent {
        Some(number) => Some(
            row_by_number(db, reader.domain.id, number)
                .await?
                .ok_or_else(|| G5Error::ValidationError {
                    details: json!({
                        "field": "parent",
                        "code": "parent_not_found",
                        "error": format!("{} does not exist", reader.display_key(number)),
                    }),
                })?,
        ),
        None => None,
    };

    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM native_issues WHERE org_id = $1 AND author_user_id = $2 \
         AND idempotency_key = $3",
    )
    .bind(new.org_id)
    .bind(new.author)
    .bind(new.idempotency_key)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("key lookup", &error))?;
    if let Some(id) = existing {
        let row = row_by_id(db, reader.domain.id, id).await?.ok_or_else(|| {
            invalid(
                "idempotencyKey",
                "already used for an issue in another domain",
            )
        })?;
        let lowered = |names: &mut dyn Iterator<Item = &str>| {
            names.map(str::to_lowercase).collect::<HashSet<_>>()
        };
        let same = row.title == title
            && row.body == body
            && row.parent_id == parent.as_ref().map(|parent| parent.id)
            && row.assignees.iter().collect::<HashSet<_>>()
                == new.assignees.iter().collect::<HashSet<_>>()
            && lowered(&mut row.labels.iter().map(|label| label.name.as_str()))
                == lowered(&mut new.labels.iter().map(String::as_str));
        if !same {
            return Err(invalid(
                "idempotencyKey",
                "already used for a different issue",
            ));
        }
        return Ok(NativeCreation {
            issue: reader.one_summary(&row).await?,
            parent: match parent {
                Some(parent) => Some(reader.one_summary(&parent).await?),
                None => None,
            },
            created: false,
        });
    }

    let selected = labels::lock_named(db, reader.domain, "labels", new.labels).await?;
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO native_issues
            (org_id, domain_id, number, title, body, parent_id, author_user_id, origin,
             idempotency_key)
        VALUES ($1, $2,
                (SELECT COALESCE(max(number), 0) + 1 FROM native_issues WHERE domain_id = $2),
                $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
    )
    .bind(new.org_id)
    .bind(reader.domain.id)
    .bind(&title)
    .bind(&body)
    .bind(parent.as_ref().map(|parent| parent.id))
    .bind(new.author)
    .bind(new.origin)
    .bind(new.idempotency_key)
    .fetch_one(db.conn())
    .await
    .map_err(|error| db_error("issue insert", &error))?;
    for assignee in new.assignees {
        sqlx::query(
            "INSERT INTO native_issue_assignees (org_id, issue_id, user_id) VALUES ($1, $2, $3)",
        )
        .bind(new.org_id)
        .bind(id)
        .bind(assignee)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("assignee insert", &error))?;
    }
    labels::attach_new(db, new.org_id, reader.domain.id, id, &selected).await?;
    let row = row_by_id(db, reader.domain.id, id)
        .await?
        .ok_or_else(|| G5Error::Internal {
            message: "the created issue was not readable".to_string(),
        })?;
    Ok(NativeCreation {
        issue: reader.one_summary(&row).await?,
        parent: match parent {
            Some(parent) => Some(reader.one_summary(&parent).await?),
            None => None,
        },
        created: true,
    })
}

/// The issue `author` created with `idempotency_key`, if any, with its
/// parent: what a creation status check reports. Only reads.
pub async fn find_created(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    org_id: Uuid,
    author: Uuid,
    idempotency_key: &str,
) -> Result<Option<NativeCreation>, G5Error> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM native_issues WHERE org_id = $1 AND author_user_id = $2 \
         AND idempotency_key = $3",
    )
    .bind(org_id)
    .bind(author)
    .bind(idempotency_key)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("key lookup", &error))?;
    let Some(id) = id else {
        return Ok(None);
    };
    let Some(row) = row_by_id(db, reader.domain.id, id).await? else {
        return Ok(None);
    };
    let parent = match row.parent_id {
        Some(parent) => row_by_id(db, reader.domain.id, parent).await?,
        None => None,
    };
    Ok(Some(NativeCreation {
        issue: reader.one_summary(&row).await?,
        parent: match parent {
            Some(parent) => Some(reader.one_summary(&parent).await?),
            None => None,
        },
        created: false,
    }))
}

/// A new comment on a native issue.
#[derive(Debug, Clone)]
pub struct NewNativeComment<'a> {
    pub org_id: Uuid,
    pub issue_id: i64,
    pub author: Uuid,
    pub origin: &'static str,
    pub idempotency_key: &'a str,
    pub body: &'a str,
}

/// Add a comment, or return the one an earlier request with the same key
/// added (`false`). The same key with another issue or text is refused.
pub async fn add_comment(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    new: &NewNativeComment<'_>,
) -> Result<(NativeComment, bool), G5Error> {
    let body = validate_text("body", new.body, true)?;
    let issue = row_by_id(db, reader.domain.id, new.issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    // A retry returns the first result. A concurrent retry that loses the
    // insert race finds no row returned and reads the winner's instead.
    let (row, fresh) = match comment_by_key(db, new).await? {
        Some(row) => (replayed_comment(row, new, &body)?, false),
        None => {
            let inserted: Option<CommentRow> = sqlx::query_as(&format!(
                "INSERT INTO native_issue_comments \
                     (org_id, issue_id, author_user_id, origin, body, idempotency_key) \
                 VALUES ($1, $2, $3, $4, $5, $6) \
                 ON CONFLICT ON CONSTRAINT native_issue_comments_idempotency_key DO NOTHING \
                 RETURNING {COMMENT_COLUMNS}"
            ))
            .bind(new.org_id)
            .bind(new.issue_id)
            .bind(new.author)
            .bind(new.origin)
            .bind(&body)
            .bind(new.idempotency_key)
            .fetch_optional(db.conn())
            .await
            .map_err(|error| db_error("comment insert", &error))?;
            match inserted {
                Some(row) => {
                    touch(db, new.issue_id).await?;
                    (row, true)
                }
                None => {
                    let row = comment_by_key(db, new)
                        .await?
                        .ok_or_else(|| G5Error::Internal {
                            message: "the conflicting comment was not readable".to_string(),
                        })?;
                    (replayed_comment(row, new, &body)?, false)
                }
            }
        }
    };
    let profiles = load_profiles(reader.system_pool, vec![row.author_user_id]).await?;
    Ok((comment_of(reader, number_of(&issue), row, &profiles), fresh))
}

async fn comment_by_key(
    db: &mut ScopedDb,
    new: &NewNativeComment<'_>,
) -> Result<Option<CommentRow>, G5Error> {
    sqlx::query_as(&format!(
        "SELECT {COMMENT_COLUMNS} FROM native_issue_comments \
         WHERE org_id = $1 AND author_user_id = $2 AND idempotency_key = $3"
    ))
    .bind(new.org_id)
    .bind(new.author)
    .bind(new.idempotency_key)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("key lookup", &error))
}

/// The comment an earlier request stored under this key, if it is the same
/// comment; reusing the key for different content is refused.
fn replayed_comment(
    row: CommentRow,
    new: &NewNativeComment<'_>,
    body: &str,
) -> Result<CommentRow, G5Error> {
    if row.issue_id == new.issue_id && row.body == body {
        Ok(row)
    } else {
        Err(invalid(
            "idempotencyKey",
            "already used for a different comment",
        ))
    }
}

async fn touch(db: &mut ScopedDb, issue_id: i64) -> Result<(), G5Error> {
    sqlx::query("UPDATE native_issues SET updated_at = now() WHERE id = $1")
        .bind(issue_id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("issue touch", &error))?;
    Ok(())
}

/// Replace the text of `author`'s own comment on the issue, only if it is
/// unchanged since the editor read it (`expected_updated_at`), checked in
/// the update itself. Saving text the comment already has succeeds without
/// a change, so repeating an edit is harmless.
pub async fn edit_comment(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    issue_id: i64,
    comment_id: i64,
    author: Uuid,
    body: &str,
    expected_updated_at: &str,
) -> Result<NativeComment, G5Error> {
    let body = validate_text("body", body, true)?;
    let expected = parse_wire_time("expectedUpdatedAt", expected_updated_at)?;
    let issue = row_by_id(db, reader.domain.id, issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    let current: CommentRow = sqlx::query_as(&format!(
        "SELECT {COMMENT_COLUMNS} FROM native_issue_comments WHERE id = $1 AND issue_id = $2 \
         FOR UPDATE"
    ))
    .bind(comment_id)
    .bind(issue_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("comment lookup", &error))?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!(
            "comment {comment_id} on {}",
            reader.display_key(number_of(&issue))
        ),
    })?;
    if current.author_user_id != author {
        return Err(conflict(
            "not_your_comment",
            "only your own comments can be edited",
        ));
    }
    let row = if current.body == body {
        current
    } else if current.updated_at != expected {
        return Err(conflict(
            "comment_changed",
            "the comment changed since you started editing",
        ));
    } else {
        let row: CommentRow = sqlx::query_as(&format!(
            "UPDATE native_issue_comments SET body = $2, updated_at = now() \
             WHERE id = $1 RETURNING {COMMENT_COLUMNS}"
        ))
        .bind(comment_id)
        .bind(&body)
        .fetch_one(db.conn())
        .await
        .map_err(|error| db_error("comment update", &error))?;
        touch(db, issue_id).await?;
        row
    };
    let profiles = load_profiles(reader.system_pool, vec![row.author_user_id]).await?;
    Ok(comment_of(reader, number_of(&issue), row, &profiles))
}

/// Close (as completed) or reopen an issue. The requested state is applied
/// as given; an event is recorded only when the state actually changed, so
/// repeating a request changes nothing.
pub async fn set_state(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    org_id: Uuid,
    issue_id: i64,
    actor: Uuid,
    closed: bool,
) -> Result<ExternalIssueSummaryDTO, G5Error> {
    let (state, reason, event) = if closed {
        ("closed", "completed", "closed")
    } else {
        ("open", "reopened", "reopened")
    };
    let changed: Option<i64> = sqlx::query_scalar(
        r#"
        UPDATE native_issues
        SET state = $3, state_reason = $4,
            closed_at = CASE WHEN $3 = 'closed' THEN now() END,
            closed_by = CASE WHEN $3 = 'closed' THEN $5 END,
            updated_at = now()
        WHERE org_id = $1 AND id = $2 AND domain_id = $6 AND state <> $3
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(issue_id)
    .bind(state)
    .bind(reason)
    .bind(actor)
    .bind(reader.domain.id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("state update", &error))?;
    if changed.is_some() {
        record_event(db, org_id, issue_id, actor, event, None).await?;
    }
    let row = row_by_id(db, reader.domain.id, issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    reader.one_summary(&row).await
}

async fn record_event(
    db: &mut ScopedDb,
    org_id: Uuid,
    issue_id: i64,
    actor: Uuid,
    kind: &str,
    subject: Option<Uuid>,
) -> Result<(), G5Error> {
    sqlx::query(
        "INSERT INTO native_issue_events (org_id, issue_id, actor_user_id, kind, subject_user_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(org_id)
    .bind(issue_id)
    .bind(actor)
    .bind(kind)
    .bind(subject)
    .execute(db.conn())
    .await
    .map_err(|error| db_error("event insert", &error))?;
    Ok(())
}

/// A change to an issue's title, description or assignees; `None` keeps a
/// field.
#[derive(Debug, Clone, Default)]
pub struct NativeIssueEdit<'a> {
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub assignees: Option<&'a [Uuid]>,
}

/// Edit an issue (`g5.issue.native.page`) only if it is unchanged since the
/// editor read it (`expected_updated_at`), checked in the update itself.
/// Assignment changes append events.
pub async fn edit_issue(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    org_id: Uuid,
    number: u64,
    actor: Uuid,
    edit: &NativeIssueEdit<'_>,
    expected_updated_at: &str,
) -> Result<ExternalIssueSummaryDTO, G5Error> {
    let title = edit.title.map(validate_title).transpose()?;
    let body = edit
        .body
        .map(|body| validate_text("body", body, false))
        .transpose()?;
    if title.is_none() && body.is_none() && edit.assignees.is_none() {
        return Err(invalid(
            "title",
            "provide a title, description or assignees",
        ));
    }
    let expected = parse_wire_time("expectedUpdatedAt", expected_updated_at)?;
    let current = row_by_number(db, reader.domain.id, number)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: reader.display_key(number),
        })?;
    let updated: Option<i64> = sqlx::query_scalar(
        r#"
        UPDATE native_issues
        SET title = COALESCE($3, title), body = COALESCE($4, body), updated_at = now()
        WHERE org_id = $1 AND id = $2 AND updated_at = $5
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(current.id)
    .bind(&title)
    .bind(&body)
    .bind(expected)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("issue update", &error))?;
    if updated.is_none() {
        return Err(conflict(
            "issue_changed",
            "the issue changed since you started editing; reload it and try again",
        ));
    }
    if let Some(assignees) = edit.assignees {
        let before: HashSet<Uuid> = current.assignees.iter().copied().collect();
        let after: HashSet<Uuid> = assignees.iter().copied().collect();
        for removed in before.difference(&after) {
            sqlx::query("DELETE FROM native_issue_assignees WHERE issue_id = $1 AND user_id = $2")
                .bind(current.id)
                .bind(removed)
                .execute(db.conn())
                .await
                .map_err(|error| db_error("assignee removal", &error))?;
            record_event(db, org_id, current.id, actor, "unassigned", Some(*removed)).await?;
        }
        for added in assignees.iter().filter(|id| !before.contains(id)) {
            sqlx::query(
                "INSERT INTO native_issue_assignees (org_id, issue_id, user_id) VALUES ($1, $2, $3)",
            )
            .bind(org_id)
            .bind(current.id)
            .bind(added)
            .execute(db.conn())
            .await
            .map_err(|error| db_error("assignee insert", &error))?;
            record_event(db, org_id, current.id, actor, "assigned", Some(*added)).await?;
        }
    }
    let row = row_by_id(db, reader.domain.id, current.id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: reader.display_key(number),
        })?;
    reader.one_summary(&row).await
}

/// An edit as `g5d issue edit` names it: a new title or description, and
/// people to add to or remove from the current assignees (`@me`, an email
/// or a user id).
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeIssueChange<'a> {
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub add: &'a [String],
    pub remove: &'a [String],
    /// The description the change was written against: refused
    /// (`issue_changed`) when the issue no longer has it.
    pub expected_body: Option<&'a str>,
}

/// Apply `change` to the issue as read in this transaction
/// (`g5.external-issues.edit`): the new assignee set is the current one with
/// `remove` taken out and `add` put in, and the write is conditional on that
/// read, so a change committed in between is refused (`issue_changed`).
/// Adding someone already assigned or removing someone who is not changes
/// nothing; a change that leaves the issue as it is writes nothing.
pub async fn change_issue(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    org_id: Uuid,
    issue_id: i64,
    actor: Uuid,
    change: &NativeIssueChange<'_>,
) -> Result<ExternalIssueSummaryDTO, G5Error> {
    let title = change.title.map(validate_title).transpose()?;
    let body = change
        .body
        .map(|body| validate_text("body", body, false))
        .transpose()?;
    if title.is_none() && body.is_none() && change.add.is_empty() && change.remove.is_empty() {
        return Err(invalid(
            "title",
            "provide a title, description or assignee change",
        ));
    }
    let current = row_by_id(db, reader.domain.id, issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    if change
        .expected_body
        .is_some_and(|expected| expected != current.body)
    {
        return Err(conflict(
            "issue_changed",
            "the issue's description changed since it was read; read it again",
        ));
    }
    let added = resolve_assignees(reader.system_pool, org_id, actor, change.add).await?;
    let removed =
        resolve_unassignees(reader.system_pool, org_id, actor, &current, change.remove).await?;
    if let Some(both) = added.iter().find(|id| removed.contains(id)) {
        let named = load_profiles(reader.system_pool, vec![*both]).await?;
        return Err(invalid(
            "assignees",
            format!("{} is both added and removed", profile(&named, *both).login),
        ));
    }
    let mut assignees: Vec<Uuid> = current
        .assignees
        .iter()
        .copied()
        .filter(|id| !removed.contains(id))
        .collect();
    for id in added {
        if !assignees.contains(&id) {
            assignees.push(id);
        }
    }
    if assignees.len() > MAX_ASSIGNEES {
        return Err(invalid(
            "assignees",
            format!("an issue can have at most {MAX_ASSIGNEES} assignees"),
        ));
    }
    let title = title.filter(|title| *title != current.title);
    let body = body.filter(|body| *body != current.body);
    let same_people = assignees.len() == current.assignees.len()
        && assignees.iter().all(|id| current.assignees.contains(id));
    if title.is_none() && body.is_none() && same_people {
        return reader.one_summary(&current).await;
    }
    edit_issue(
        db,
        reader,
        org_id,
        number_of(&current),
        actor,
        &NativeIssueEdit {
            title: title.as_deref(),
            body: body.as_deref(),
            assignees: (!same_people).then_some(assignees.as_slice()),
        },
        &wire_time(current.updated_at),
    )
    .await
}

/// The people `requested` names for removal: `@me`, a user id, or the email
/// of a member or of someone the issue is assigned to (a former member stays
/// removable). Someone G5 cannot name is refused, never skipped.
async fn resolve_unassignees(
    system_pool: &PgPool,
    org_id: Uuid,
    caller: Uuid,
    issue: &IssueRow,
    requested: &[String],
) -> Result<Vec<Uuid>, G5Error> {
    if requested.is_empty() {
        return Ok(Vec::new());
    }
    let members: Vec<ProfileRow> = sqlx::query_as(ASSIGNABLE_MEMBERS)
        .bind(org_id)
        .fetch_all(system_pool)
        .await
        .map_err(|error| db_error("assignable members", &error))?;
    let assigned = load_profiles(system_pool, issue.assignees.clone()).await?;
    let mut resolved = Vec::new();
    for raw in requested {
        let wanted = raw.trim();
        if wanted.is_empty() {
            continue;
        }
        let id = if wanted.eq_ignore_ascii_case(ASSIGN_SELF) {
            Some(caller)
        } else if let Ok(id) = Uuid::parse_str(wanted) {
            Some(id)
        } else {
            members
                .iter()
                .find(|member| member.email.eq_ignore_ascii_case(wanted))
                .map(|member| member.id)
                .or_else(|| {
                    assigned
                        .values()
                        .find(|person| person.login.eq_ignore_ascii_case(wanted))
                        .and_then(|person| person.user_id)
                })
        }
        .ok_or_else(|| {
            invalid(
                "assignees",
                format!("{wanted} is neither an organization member nor assigned to the issue"),
            )
        })?;
        if !resolved.contains(&id) {
            resolved.push(id);
        }
    }
    Ok(resolved)
}

/// The issue numbered `number`, for writes addressed by number.
pub async fn issue_id_by_number(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    number: u64,
) -> Result<i64, G5Error> {
    row_by_number(db, reader.domain.id, number)
        .await?
        .map(|row| row.id)
        .ok_or_else(|| G5Error::NotFound {
            resource: reader.display_key(number),
        })
}

// ---------------------------------------------------------------------------
// Export (g5.issue.native.migration)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportedUser {
    email: String,
    display_name: Option<String>,
}

/// Every issue of the native domain with its labels, comments, events and
/// attachments, oldest first, and the domain's label catalog, people named by
/// email: what an administrator takes elsewhere. Attachment bytes are read as
/// [`NativeExport::write`] reaches them.
pub async fn export(
    db: &mut ScopedDb,
    system_pool: &PgPool,
    domain: &IssueDomainRecord,
) -> Result<NativeExport, G5Error> {
    let issues: Vec<IssueRow> = sqlx::query_as(&format!(
        "{ISSUE_SELECT} WHERE i.domain_id = $1 ORDER BY i.number"
    ))
    .bind(domain.id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("export issues", &error))?;
    let comments: Vec<CommentRow> = sqlx::query_as(
        "SELECT c.id, c.issue_id, c.author_user_id, c.origin, c.body, c.created_at, c.updated_at \
         FROM native_issue_comments c JOIN native_issues i ON i.id = c.issue_id \
         WHERE i.domain_id = $1 ORDER BY c.created_at, c.id",
    )
    .bind(domain.id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("export comments", &error))?;
    #[derive(sqlx::FromRow)]
    struct ExportEventRow {
        issue_id: i64,
        actor_user_id: Uuid,
        kind: String,
        subject_user_id: Option<Uuid>,
        label_name: Option<String>,
        created_at: DateTime<Utc>,
    }
    let events: Vec<ExportEventRow> = sqlx::query_as(
        "SELECT e.issue_id, e.actor_user_id, e.kind, e.subject_user_id, e.label_name, \
         e.created_at \
         FROM native_issue_events e JOIN native_issues i ON i.id = e.issue_id \
         WHERE i.domain_id = $1 ORDER BY e.created_at, e.id",
    )
    .bind(domain.id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("export events", &error))?;
    let catalog: Vec<labels::LabelRow> = sqlx::query_as(
        "SELECT id, name, color, description FROM native_issue_labels WHERE domain_id = $1 \
         ORDER BY lower(name), id",
    )
    .bind(domain.id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("export labels", &error))?;
    let ids: Vec<Uuid> = issues
        .iter()
        .flat_map(|issue| {
            issue.assignees.iter().copied().chain(
                [Some(issue.author_user_id), issue.closed_by]
                    .into_iter()
                    .flatten(),
            )
        })
        .chain(comments.iter().map(|comment| comment.author_user_id))
        .chain(
            events
                .iter()
                .flat_map(|event| [Some(event.actor_user_id), event.subject_user_id])
                .flatten(),
        )
        .collect();
    let profiles = load_profiles(system_pool, ids).await?;
    let person = |id: Uuid| {
        let user = profile(&profiles, id);
        ExportedUser {
            email: user.login,
            display_name: user.display_name,
        }
    };
    let mut comments_by_issue: HashMap<i64, Vec<&CommentRow>> = HashMap::new();
    for comment in &comments {
        comments_by_issue
            .entry(comment.issue_id)
            .or_default()
            .push(comment);
    }
    let mut events_by_issue: HashMap<i64, Vec<&ExportEventRow>> = HashMap::new();
    for event in &events {
        events_by_issue
            .entry(event.issue_id)
            .or_default()
            .push(event);
    }
    let attachments = attachments::export_rows(db, domain.id).await?;
    let mut attachments_by_issue: HashMap<i64, Vec<&attachments::ExportedAttachment>> =
        HashMap::new();
    for attachment in &attachments {
        attachments_by_issue
            .entry(attachment.issue_id)
            .or_default()
            .push(attachment);
    }
    let uploaders = load_profiles(
        system_pool,
        attachments.iter().map(|a| a.uploader_user_id).collect(),
    )
    .await?;
    let numbers: HashMap<i64, i64> = issues
        .iter()
        .map(|issue| (issue.id, issue.number))
        .collect();
    let exported: Vec<ExportedIssue> = issues
        .iter()
        .map(|issue| ExportedIssue {
            fields: json!({
                "number": issue.number,
                "key": format!("{}-{}", domain.display_key, issue.number),
                "title": issue.title,
                "body": issue.body,
                "state": issue.state,
                "stateReason": issue.state_reason,
                "parent": issue.parent_id.and_then(|id| numbers.get(&id)),
                "author": person(issue.author_user_id),
                "origin": issue.origin,
                "assignees": issue.assignees.iter().map(|id| person(*id)).collect::<Vec<_>>(),
                "labels": issue.labels.iter().map(|label| label.name.as_str()).collect::<Vec<_>>(),
                "createdAt": wire_time(issue.created_at),
                "updatedAt": wire_time(issue.updated_at),
                "closedAt": issue.closed_at.map(wire_time),
                "closedBy": issue.closed_by.map(person),
                "comments": comments_by_issue
                    .get(&issue.id)
                    .into_iter()
                    .flatten()
                    .map(|comment| json!({
                        "id": comment.id,
                        "author": person(comment.author_user_id),
                        "origin": comment.origin,
                        "body": comment.body,
                        "createdAt": wire_time(comment.created_at),
                        "updatedAt": wire_time(comment.updated_at),
                    }))
                    .collect::<Vec<_>>(),
                "events": events_by_issue
                    .get(&issue.id)
                    .into_iter()
                    .flatten()
                    .map(|event| json!({
                        "event": event.kind,
                        "actor": person(event.actor_user_id),
                        "subject": event.subject_user_id.map(person),
                        "label": event.label_name,
                        "createdAt": wire_time(event.created_at),
                    }))
                    .collect::<Vec<_>>(),
            }),
            attachments: attachments_by_issue
                .get(&issue.id)
                .into_iter()
                .flatten()
                .map(|attachment| {
                    let uploader = profile(&uploaders, attachment.uploader_user_id);
                    let fields = json!({
                        "id": attachment.id,
                        "fileName": attachment.file_name,
                        "contentType": attachment.content_type,
                        "sizeBytes": attachment.size_bytes,
                        "sha256": attachment.sha256,
                        "uploader": ExportedUser {
                            email: uploader.login,
                            display_name: uploader.display_name,
                        },
                        "origin": attachment.origin,
                        "createdAt": wire_time(attachment.created_at),
                    });
                    (attachment.bytes(), fields)
                })
                .collect(),
        })
        .collect();
    Ok(NativeExport {
        head: json!({
            "format": "g5-native-issues/1",
            "key": domain.display_key,
            "exportedAt": wire_time(Utc::now()),
            "labels": catalog
                .iter()
                .map(|label| json!({
                    "name": label.name,
                    "color": label.color,
                    "description": label.description,
                }))
                .collect::<Vec<_>>(),
        }),
        issues: exported,
    })
}

/// A native domain's export, before its attachments' bytes are read.
pub struct NativeExport {
    head: Value,
    issues: Vec<ExportedIssue>,
}

struct ExportedIssue {
    fields: Value,
    /// Where each attachment's bytes are, and its other fields.
    attachments: Vec<(attachments::StoredBytes, Value)>,
}

/// Raw bytes encoded per piece: a multiple of 3, so the pieces' base64
/// concatenates into the whole attachment's.
const EXPORT_PIECE_BYTES: usize = 3 * 256 * 1024;

impl NativeExport {
    /// Whether any exported issue carries an attachment, whose bytes need
    /// the attachment store.
    #[must_use]
    pub fn has_attachments(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| !issue.attachments.is_empty())
    }

    /// Write the export's JSON document in pieces to `out`, reading one
    /// attachment's bytes at a time, so that a domain's attachments are never
    /// held together. Never sends an empty piece; stops when `out` is closed.
    pub async fn write(
        self,
        db: &mut ScopedDb,
        store: &IssueAttachmentStore,
        out: &tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<(), G5Error> {
        let send = |piece: Vec<u8>| async move {
            out.send(piece).await.map_err(|_| G5Error::Internal {
                message: "the export's reader went away".into(),
            })
        };
        let mut piece = open_object(&self.head);
        piece.extend_from_slice(b",\"issues\":[");
        for (index, issue) in self.issues.into_iter().enumerate() {
            if index > 0 {
                piece.push(b',');
            }
            piece.extend(open_object(&issue.fields));
            piece.extend_from_slice(b",\"attachments\":[");
            for (position, (bytes, fields)) in issue.attachments.into_iter().enumerate() {
                if position > 0 {
                    piece.push(b',');
                }
                piece.extend(open_object(&fields));
                let content = match attachments::export_content(db, store, bytes).await {
                    Ok(content) => content,
                    // Removed after the export read its record, or its bytes
                    // were lost: the record is exported without them rather
                    // than ending the whole export.
                    Err(G5Error::NotFound { .. }) => {
                        piece.extend_from_slice(b",\"contentBase64\":null}");
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                piece.extend_from_slice(b",\"contentBase64\":\"");
                send(std::mem::take(&mut piece)).await?;
                for raw in content.chunks(EXPORT_PIECE_BYTES) {
                    send(
                        base64::engine::general_purpose::STANDARD
                            .encode(raw)
                            .into_bytes(),
                    )
                    .await?;
                }
                piece.extend_from_slice(b"\"}");
            }
            piece.extend_from_slice(b"]}");
        }
        piece.extend_from_slice(b"]}");
        send(piece).await
    }
}

/// A non-empty JSON object's text without its closing brace, so that more
/// members can follow.
fn open_object(object: &Value) -> Vec<u8> {
    debug_assert!(object
        .as_object()
        .is_some_and(|members| !members.is_empty()));
    let mut text = serde_json::to_vec(object).expect("a JSON value serializes");
    text.pop();
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_keys_are_short_uppercase_words() {
        assert_eq!(validate_issue_key(" acme ").unwrap(), "ACME");
        assert_eq!(validate_issue_key("G5").unwrap(), "G5");
        assert!(validate_issue_key("A").is_err());
        assert!(validate_issue_key("5ACME").is_err());
        assert!(validate_issue_key("AC-ME").is_err());
        assert!(validate_issue_key("ABCDEFGHIJK").is_err());
        assert_eq!(default_issue_key("acme-corp"), "ACMECORP");
        assert_eq!(default_issue_key("42-labs-and-friends"), "LABSANDFRI");
        assert_eq!(default_issue_key("x"), "G5");
    }

    #[test]
    fn references_name_a_number_by_key_or_page_url() {
        let site = NativeSite::new("https://g5.example/", "acme");
        assert_eq!(site.issue_url(12), "https://g5.example/acme/issues/g5/12");
        assert_eq!(parse_reference("ACME-12", "ACME", &site), Some(12));
        assert_eq!(parse_reference("acme-12", "ACME", &site), Some(12));
        assert_eq!(
            parse_reference(
                "https://g5.example/acme/issues/g5/12#comment-3",
                "ACME",
                &site
            ),
            Some(12)
        );
        assert_eq!(
            parse_reference("/acme/issues/g5/7/", "ACME", &site),
            Some(7)
        );
        assert_eq!(parse_reference("OTHER-12", "ACME", &site), None);
        assert_eq!(parse_reference("ACME-012", "ACME", &site), None);
        assert_eq!(parse_reference("acme/web#12", "ACME", &site), None);
        assert_eq!(
            parse_reference("https://g5.example/other/issues/g5/12", "ACME", &site),
            None
        );
    }

    #[test]
    fn wire_times_round_trip_at_database_precision() {
        let time = DateTime::parse_from_rfc3339("2026-10-22T10:11:12.345678Z")
            .unwrap()
            .with_timezone(&Utc);
        let wire = wire_time(time);
        assert_eq!(wire, "2026-10-22T10:11:12.345678Z");
        assert_eq!(parse_wire_time("t", &wire).unwrap(), time);
    }
}
