//! GitHub Issues REST adapter for the external issue integration.
//!
//! Reads use an installation token narrowed to one domain repository and
//! Issues read (selected by the integration owner).
//! Every GitHub call has a five-second interactive budget and maps failures to
//! a typed [`ExternalIssueErrorKind`], so a timeout or 5xx is reported as
//! `unavailable` rather than an empty result.
//!
//! Issue identity is GitHub's numeric REST `id`, normalized losslessly to a
//! base-10 string; the repository-local `number` is routing metadata only.
//! GitHub's Issues endpoints also return pull requests; those are rejected
//! (`not_an_issue`) or excluded from search results.

use std::time::Duration;

use g5_contracts::error::{ExternalIssueErrorKind, G5Error};
use serde::{de::DeserializeOwned, Deserialize};

mod attachments;
pub use attachments::*;
pub mod labels;

use super::{
    valid_repository_parts, GithubAuthenticatedUser, Payload, ProviderEndpoints, ProviderError,
    RepositoryCapability, RepositoryClient, RepositoryIdentity, Response,
};

/// Interactive request budget for each GitHub call.
pub const INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(5);
/// Search page size; also the cap on federated search results.
pub const SEARCH_PAGE_SIZE: usize = 30;
/// Comments returned with an issue detail (oldest first).
pub const COMMENT_PAGE_SIZE: usize = 100;

const MAX_USER_QUERY_CHARS: usize = 200;
const MAX_ISSUE_NUMBER: u64 = u32::MAX as u64;
const READ: &str = "read repository issues";
const READERS: &[RepositoryCapability] = &[
    RepositoryCapability::IssuesRead,
    RepositoryCapability::IssueCommentWrite,
];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A typed external issue failure. The message never contains credentials or
/// provider URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueError {
    pub kind: ExternalIssueErrorKind,
    pub message: String,
}

impl IssueError {
    #[must_use]
    pub fn new(kind: ExternalIssueErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ExternalIssueErrorKind::Unavailable, message)
    }

    #[must_use]
    pub fn invalid_reference(message: impl Into<String>) -> Self {
        Self::new(ExternalIssueErrorKind::InvalidReference, message)
    }

    #[must_use]
    pub fn not_an_issue() -> Self {
        Self::new(
            ExternalIssueErrorKind::NotAnIssue,
            "the reference names a GitHub pull request, not an issue",
        )
    }
}

impl std::fmt::Display for IssueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

impl From<IssueError> for G5Error {
    fn from(error: IssueError) -> Self {
        G5Error::ExternalIssue {
            kind: error.kind,
            message: error.message,
        }
    }
}

/// The external-issue meaning of a transport failure. Rate-limit evidence is
/// the transport's; a 401/403 without it is an access refusal.
impl From<ProviderError> for IssueError {
    fn from(error: ProviderError) -> Self {
        use ExternalIssueErrorKind as Kind;
        let kind = if error.rate_limited {
            Kind::RateLimited
        } else {
            match error.status {
                Some(401 | 403) => Kind::AccessDenied,
                // The integration does not follow redirects: a moved or
                // transferred issue must be resolved again from its new reference.
                Some(300..=399 | 404 | 410) => Kind::NotFound,
                _ => Kind::Unavailable,
            }
        };
        Self::new(kind, error.to_string())
    }
}

/// One interactive GET under the selected grant.
async fn interactive_get(
    client: &RepositoryClient,
    segments: &[&str],
    params: &[(&str, String)],
) -> Result<Response, ProviderError> {
    client
        .send(
            READ,
            reqwest::Method::GET,
            super::query(client.grant.endpoints.api(segments), params),
            Payload::Empty,
            &[],
            INTERACTIVE_TIMEOUT,
        )
        .await
}

async fn github_get<T: DeserializeOwned>(
    client: &RepositoryClient,
    segments: &[&str],
    params: &[(&str, String)],
) -> Result<T, IssueError> {
    client.require(READ, READERS)?;
    Ok(interactive_get(client, segments, params).await?.json()?)
}

/// The one issue-comment page read (`.../issues/{number}/comments`) for either
/// repository identity, and whether GitHub reports another page. Callers
/// select the capability, page parameters and budget.
pub(super) async fn comment_page(
    client: &RepositoryClient,
    operation: &'static str,
    number: u64,
    params: &[(&str, String)],
    timeout: Duration,
) -> Result<(Vec<RawComment>, bool), ProviderError> {
    let number = number.to_string();
    let url = match &client.grant.repository {
        RepositoryIdentity::Id(id) => client.grant.endpoints.api(&[
            "repositories",
            &id.to_string(),
            "issues",
            &number,
            "comments",
        ]),
        RepositoryIdentity::Name(_) => client.repo_url(&["issues", &number, "comments"])?,
    };
    let response = client
        .send(
            operation,
            reqwest::Method::GET,
            super::query(url, params),
            Payload::Empty,
            &[],
            timeout,
        )
        .await?;
    Ok((response.json()?, response.has_next_page()))
}

// ---------------------------------------------------------------------------
// Normalized provider records
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRepositoryInfo {
    pub id: u64,
    pub full_name: String,
    pub audience: RepositoryAudience,
}

/// Who can see a repository, as GitHub currently reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryAudience {
    /// `private` or `internal`.
    NonPublic,
    Public,
    /// GitHub reported neither a visibility nor a private flag.
    Unknown,
}

impl RepositoryAudience {
    fn classify(visibility: Option<&str>, private: Option<bool>) -> Self {
        match visibility.map(str::to_ascii_lowercase).as_deref() {
            Some("private" | "internal") => Self::NonPublic,
            Some("public") => Self::Public,
            Some(_) => Self::Unknown,
            None => match private {
                Some(true) => Self::NonPublic,
                Some(false) => Self::Public,
                None => Self::Unknown,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssueUser {
    /// GitHub's account id. Logins can be renamed and reclaimed; the id
    /// cannot.
    pub id: Option<u64>,
    pub login: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssueLabel {
    /// GitHub's label id; absent when GitHub sent only a name.
    pub id: Option<u64>,
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// A GitHub issue (never a pull request).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssue {
    /// REST issue id as a canonical decimal string.
    pub id: String,
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    /// `open` or `closed`.
    pub state: String,
    pub state_reason: Option<String>,
    pub html_url: String,
    /// `owner/name` derived from the provider's repository URL, when present.
    pub repository_full_name: Option<String>,
    pub author: Option<GithubIssueUser>,
    pub assignees: Vec<GithubIssueUser>,
    pub labels: Vec<GithubIssueLabel>,
    pub comment_count: u64,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssueComment {
    pub id: String,
    pub author: Option<GithubIssueUser>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub html_url: String,
    /// The issue the comment is on, from its `issue_url`, when present.
    pub issue_number: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawRepository {
    id: u64,
    full_name: String,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    private: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawUser {
    #[serde(default)]
    id: Option<u64>,
    pub(super) login: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawLabel {
    Object(labels::RawCatalogLabel),
    Name(String),
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawIssue {
    id: u64,
    number: u64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    /// GitHub's rendering of `body`, sent only for the `full+json` media type.
    #[serde(default)]
    body_html: Option<String>,
    state: String,
    #[serde(default)]
    state_reason: Option<String>,
    html_url: String,
    #[serde(default)]
    repository_url: Option<String>,
    #[serde(default)]
    user: Option<RawUser>,
    #[serde(default)]
    assignees: Option<Vec<RawUser>>,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    comments: u64,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

impl RawIssue {
    fn is_pull_request(&self) -> bool {
        self.pull_request
            .as_ref()
            .is_some_and(|value| !value.is_null())
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct RawComment {
    pub(super) id: u64,
    #[serde(default)]
    pub(super) user: Option<RawUser>,
    #[serde(default)]
    pub(super) body: Option<String>,
    /// GitHub's rendering of `body`, sent only for the `full+json` media type.
    #[serde(default)]
    pub(super) body_html: Option<String>,
    pub(super) created_at: String,
    // Optional here so the PR-detail view tolerates partial records; the
    // issue mapping still requires both.
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    pub(super) html_url: Option<String>,
    #[serde(default)]
    issue_url: Option<String>,
}

/// The issue number at the end of an `issue_url`.
fn issue_number_from_url(issue_url: &str) -> Option<u64> {
    issue_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|number| number.parse().ok())
}

#[derive(Debug, Deserialize)]
struct RawSearchPage {
    // Required: without them a partial page would look complete.
    total_count: u64,
    incomplete_results: bool,
    #[serde(default)]
    items: Vec<RawIssue>,
}

pub(super) fn user(raw: RawUser) -> GithubIssueUser {
    GithubIssueUser {
        id: raw.id,
        login: raw.login,
        avatar_url: raw.avatar_url.filter(|url| !url.trim().is_empty()),
    }
}

/// Canonical decimal form of a GitHub numeric id (no sign, no leading
/// zeros). Zero is not a valid GitHub id.
fn canonical_id(id: u64) -> Result<String, IssueError> {
    if id == 0 {
        return Err(IssueError::unavailable("GitHub returned an invalid id"));
    }
    Ok(id.to_string())
}

/// `owner/name` from `https://api.github.com/repos/{owner}/{name}`.
fn full_name_from_repository_url(repository_url: &str) -> Option<String> {
    let url = url::Url::parse(repository_url).ok()?;
    let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        [.., "repos", owner, name] => Some(format!("{owner}/{name}")),
        _ => None,
    }
}

/// Response links must belong to the independently selected API or web owner.
/// Renamed repositories and issue fragments remain valid on that owner.
fn validate_issue_urls(client: &RepositoryClient, raw: &RawIssue) -> Result<(), IssueError> {
    if raw.is_pull_request() {
        return Err(IssueError::not_an_issue());
    }
    if parse_issue_url(&raw.html_url, &client.grant.endpoints).is_err() {
        return Err(IssueError::unavailable(
            "GitHub returned an issue URL outside the selected tracker",
        ));
    }
    if let Some(value) = &raw.repository_url {
        let path = client.grant.endpoints.api_path(value);
        if !path.as_ref().is_some_and(|path| matches!(path.as_slice(), [kind, owner, name] if kind == "repos" && valid_repository_parts(owner, name))) {
            return Err(IssueError::unavailable("GitHub returned a repository URL outside the selected tracker"));
        }
    }
    Ok(())
}
pub(super) fn owned_comment(
    client: &RepositoryClient,
    raw: RawComment,
) -> Result<GithubIssueComment, IssueError> {
    let endpoints = &client.grant.endpoints;
    if raw
        .html_url
        .as_deref()
        .is_some_and(|url| endpoints.web_path(url).is_none())
        || raw
            .issue_url
            .as_ref()
            .is_some_and(|value| endpoints.api_path(value).is_none())
    {
        return Err(IssueError::unavailable(
            "GitHub returned a comment URL outside the selected tracker",
        ));
    }
    normalize_comment(raw)
}

/// Normalize a provider issue. Pull requests are rejected.
pub(crate) fn normalize_issue(raw: RawIssue) -> Result<GithubIssue, IssueError> {
    if raw.is_pull_request() {
        return Err(IssueError::not_an_issue());
    }
    let state = raw.state.to_ascii_lowercase();
    if state != "open" && state != "closed" {
        return Err(IssueError::unavailable(
            "GitHub returned an unknown issue state",
        ));
    }
    if raw.number == 0 {
        return Err(IssueError::unavailable(
            "GitHub returned an invalid issue number",
        ));
    }
    Ok(GithubIssue {
        id: canonical_id(raw.id)?,
        number: raw.number,
        title: raw.title,
        body: raw.body,
        state,
        state_reason: raw.state_reason.filter(|reason| !reason.trim().is_empty()),
        html_url: raw.html_url,
        repository_full_name: raw
            .repository_url
            .as_deref()
            .and_then(full_name_from_repository_url),
        author: raw.user.map(user),
        assignees: raw
            .assignees
            .unwrap_or_default()
            .into_iter()
            .map(user)
            .collect(),
        labels: raw
            .labels
            .into_iter()
            .map(|label| match label {
                RawLabel::Object(raw) => labels::catalog_label(raw),
                RawLabel::Name(name) => GithubIssueLabel {
                    id: None,
                    name,
                    color: None,
                    description: None,
                },
            })
            .collect(),
        comment_count: raw.comments,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
        closed_at: raw.closed_at,
    })
}

fn normalize_comment(raw: RawComment) -> Result<GithubIssueComment, IssueError> {
    let (Some(updated_at), Some(html_url)) = (raw.updated_at, raw.html_url) else {
        return Err(IssueError::unavailable(
            "GitHub returned a comment without its timestamp or URL",
        ));
    };
    Ok(GithubIssueComment {
        id: canonical_id(raw.id)?,
        author: raw.user.map(user),
        body: raw.body.unwrap_or_default(),
        created_at: raw.created_at,
        updated_at,
        html_url,
        issue_number: raw.issue_url.as_deref().and_then(issue_number_from_url),
    })
}

// ---------------------------------------------------------------------------
// REST operations
// ---------------------------------------------------------------------------

/// `GET /repositories/{id}`: the repository's current `owner/name`.
pub async fn fetch_repository(
    client: &RepositoryClient,
) -> Result<GithubRepositoryInfo, IssueError> {
    client.require(
        "read repository metadata",
        &[
            RepositoryCapability::MetadataRead,
            RepositoryCapability::IssuesRead,
            RepositoryCapability::IssueCommentWrite,
        ],
    )?;
    let repository_id = client.repository_id()?;
    let raw: RawRepository =
        interactive_get(client, &["repositories", &repository_id.to_string()], &[])
            .await?
            .json()?;
    if raw.id != repository_id || raw.full_name.trim().is_empty() {
        return Err(IssueError::unavailable(
            "GitHub repository metadata did not match the requested repository",
        ));
    }
    Ok(GithubRepositoryInfo {
        id: raw.id,
        audience: RepositoryAudience::classify(raw.visibility.as_deref(), raw.private),
        full_name: raw.full_name,
    })
}

/// `GET /repositories/{id}/issues/{number}`. A pull request is rejected with
/// `not_an_issue`.
pub async fn fetch_issue(
    client: &RepositoryClient,
    number: u64,
) -> Result<GithubIssue, IssueError> {
    let repository_id = client.repository_id()?.to_string();
    let raw: RawIssue = github_get(
        client,
        &[
            "repositories",
            &repository_id,
            "issues",
            &number.to_string(),
        ],
        &[],
    )
    .await?;
    validate_issue_urls(client, &raw)?;
    let issue = normalize_issue(raw)?;
    if issue.number != number {
        return Err(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the GitHub issue has moved",
        ));
    }
    Ok(issue)
}

/// The first page of comments, oldest first.
pub async fn fetch_issue_comments(
    client: &RepositoryClient,
    number: u64,
) -> Result<Vec<GithubIssueComment>, IssueError> {
    client.require(READ, READERS)?;
    client.repository_id()?;
    let (raw, _) = comment_page(
        client,
        READ,
        number,
        &[("per_page", COMMENT_PAGE_SIZE.to_string())],
        INTERACTIVE_TIMEOUT,
    )
    .await?;
    let mut comments = raw
        .into_iter()
        .map(|raw| owned_comment(client, raw))
        .collect::<Result<Vec<_>, _>>()?;
    comments.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(comments)
}

/// Timeline events Activity shows; other kinds (commits, project cards,
/// subscriptions, ...) are skipped.
const TIMELINE_EVENTS: &[&str] = &[
    "labeled",
    "unlabeled",
    "assigned",
    "unassigned",
    "closed",
    "reopened",
    "renamed",
    "milestoned",
    "demilestoned",
    "cross-referenced",
    "locked",
    "unlocked",
];

/// A non-comment issue timeline event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssueEvent {
    /// Stable identity: the event id, or for events without one (such as
    /// cross-references) the event, time and referencing issue.
    pub key: String,
    pub event: String,
    pub actor: Option<GithubIssueUser>,
    pub created_at: String,
    /// Event subject: a label, assignee, milestone, new title or the
    /// referencing issue.
    pub detail: Option<String>,
    pub state_reason: Option<String>,
}

/// One entry of an issue's timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubTimelineItem {
    Comment(GithubIssueComment),
    Event(GithubIssueEvent),
}

#[derive(Debug, Deserialize)]
struct RawNamed {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawRename {
    #[serde(default)]
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSourceIssue {
    number: u64,
    #[serde(default)]
    repository: Option<RawSourceRepository>,
}

#[derive(Debug, Deserialize)]
struct RawSourceRepository {
    full_name: String,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    #[serde(default)]
    issue: Option<RawSourceIssue>,
}

#[derive(Debug, Deserialize)]
struct RawTimelineItem {
    event: String,
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    actor: Option<RawUser>,
    #[serde(default)]
    user: Option<RawUser>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    label: Option<RawNamed>,
    #[serde(default)]
    assignee: Option<RawUser>,
    #[serde(default)]
    milestone: Option<RawNamed>,
    #[serde(default)]
    rename: Option<RawRename>,
    #[serde(default)]
    source: Option<RawSource>,
    #[serde(default)]
    state_reason: Option<String>,
}

/// Normalize the timeline entry at `position` in the fetched page; `None` for
/// kinds Activity does not show or entries missing the fields it needs.
fn timeline_item(position: usize, raw: RawTimelineItem) -> Option<GithubTimelineItem> {
    let created_at = raw.created_at?;
    if raw.event == "commented" {
        return Some(GithubTimelineItem::Comment(GithubIssueComment {
            id: canonical_id(raw.id?).ok()?,
            author: raw.user.or(raw.actor).map(user),
            body: raw.body.unwrap_or_default(),
            updated_at: raw.updated_at.unwrap_or_else(|| created_at.clone()),
            created_at,
            html_url: raw.html_url?,
            issue_number: None,
        }));
    }
    if !TIMELINE_EVENTS.contains(&raw.event.as_str()) {
        return None;
    }
    let detail = match raw.event.as_str() {
        "labeled" | "unlabeled" => raw.label.and_then(|label| label.name),
        "assigned" | "unassigned" => raw.assignee.map(|assignee| assignee.login),
        "milestoned" | "demilestoned" => raw.milestone.and_then(|milestone| milestone.title),
        "renamed" => raw.rename.and_then(|rename| rename.to),
        "cross-referenced" => {
            raw.source
                .and_then(|source| source.issue)
                .map(|issue| match issue.repository {
                    Some(repository) => format!("{}#{}", repository.full_name, issue.number),
                    None => format!("#{}", issue.number),
                })
        }
        _ => None,
    };
    let actor = raw.actor.map(user);
    // Events without an id (cross-references) can share their event, time and
    // subject; the actor and the position in GitHub's append-only, oldest-first
    // timeline keep them distinct.
    let key = match raw.id {
        Some(id) => id.to_string(),
        None => format!(
            "{}:{}:{}:{}:{position}",
            raw.event,
            created_at,
            detail.as_deref().unwrap_or_default(),
            actor.as_ref().map_or("", |actor| actor.login.as_str()),
        ),
    };
    Some(GithubTimelineItem::Event(GithubIssueEvent {
        key,
        event: raw.event,
        actor,
        created_at,
        detail,
        state_reason: raw.state_reason,
    }))
}

/// The first page of an issue's timeline (comments and events, oldest
/// first) and whether GitHub has more.
pub async fn fetch_issue_timeline(
    client: &RepositoryClient,
    number: u64,
) -> Result<(Vec<GithubTimelineItem>, bool), IssueError> {
    let repository_id = client.repository_id()?.to_string();
    let raw: Vec<serde_json::Value> = github_get(
        client,
        &[
            "repositories",
            &repository_id,
            "issues",
            &number.to_string(),
            "timeline",
        ],
        &[("per_page", COMMENT_PAGE_SIZE.to_string())],
    )
    .await?;
    let truncated = raw.len() >= COMMENT_PAGE_SIZE;
    let items = raw
        .into_iter()
        .enumerate()
        .filter_map(|(position, value)| {
            serde_json::from_value::<RawTimelineItem>(value)
                .ok()
                .filter(|raw| {
                    raw.html_url
                        .as_ref()
                        .is_none_or(|value| client.grant.endpoints.web_path(value).is_some())
                })
                .and_then(|raw| timeline_item(position, raw))
        })
        .collect();
    Ok((items, truncated))
}

/// What happened to a comment G5 tried to post.
#[derive(Debug, Clone)]
pub enum CommentPostOutcome {
    Created(GithubIssueComment),
    /// GitHub answered and refused; nothing was posted.
    Rejected(IssueError),
    /// No definite answer (timeout, network failure, 5xx): the comment may
    /// or may not exist. Resolve by looking for it, never by posting again.
    Unknown(String),
}

/// `POST /repositories/{id}/issues/{number}/comments` with the person's
/// user access token.
pub async fn post_issue_comment(
    client: &RepositoryClient,
    number: u64,
    body: &str,
) -> CommentPostOutcome {
    send_comment(client, None, number, body).await
}
pub async fn update_issue_comment(
    client: &RepositoryClient,
    comment_id: &str,
    body: &str,
) -> CommentPostOutcome {
    if parse_canonical_github_id(comment_id).is_none() {
        return CommentPostOutcome::Rejected(IssueError::invalid_reference(
            "Invalid comment identity",
        ));
    }
    send_comment(client, Some(comment_id), 0, body).await
}

/// `GET /repositories/{id}/issues/comments/{comment_id}`.
pub async fn fetch_issue_comment(
    client: &RepositoryClient,
    comment_id: &str,
) -> Result<GithubIssueComment, IssueError> {
    if parse_canonical_github_id(comment_id).is_none() {
        return Err(IssueError::invalid_reference("Invalid comment identity"));
    }
    let repository_id = client.repository_id()?.to_string();
    let raw: RawComment = github_get(
        client,
        &[
            "repositories",
            &repository_id,
            "issues",
            "comments",
            comment_id,
        ],
        &[],
    )
    .await?;
    owned_comment(client, raw)
}

async fn send_comment(
    client: &RepositoryClient,
    comment_id: Option<&str>,
    number: u64,
    body: &str,
) -> CommentPostOutcome {
    let operation = if comment_id.is_some() {
        "update issue comment"
    } else {
        "post issue comment"
    };
    if let Err(error) = client.require(operation, &[RepositoryCapability::IssueCommentWrite]) {
        return CommentPostOutcome::Rejected(error.into());
    }
    let repository_id = match client.repository_id() {
        Ok(id) => id,
        Err(error) => return CommentPostOutcome::Rejected(error.into()),
    };
    let repository_id = repository_id.to_string();
    let number = number.to_string();
    let (method, path) = if let Some(id) = comment_id {
        (
            reqwest::Method::PATCH,
            ["repositories", &repository_id, "issues", "comments", id],
        )
    } else {
        (
            reqwest::Method::POST,
            [
                "repositories",
                &repository_id,
                "issues",
                &number,
                "comments",
            ],
        )
    };
    let response = match client
        .send(
            operation,
            method,
            client.grant.endpoints.api(&path),
            Payload::Json(serde_json::json!({ "body": body })),
            &[],
            INTERACTIVE_TIMEOUT,
        )
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return if error.effect == crate::FailureEffect::Refused {
                CommentPostOutcome::Rejected(error.into())
            } else {
                CommentPostOutcome::Unknown(error.to_string())
            }
        }
    };
    if let Err(error) = response.require_success() {
        return if error.effect == crate::FailureEffect::Refused {
            let mut mapped = IssueError::from(error);
            if response.status == 422 && !response.rate_limited() {
                mapped.kind = ExternalIssueErrorKind::InvalidReference;
            }
            CommentPostOutcome::Rejected(mapped)
        } else {
            CommentPostOutcome::Unknown(error.to_string())
        };
    }
    match response
        .json::<RawComment>()
        .map_err(IssueError::from)
        .and_then(|raw| owned_comment(client, raw))
    {
        Ok(comment) => CommentPostOutcome::Created(comment),
        Err(_) => CommentPostOutcome::Unknown(
            response
                .failure(
                    super::ProviderErrorKind::InvalidResponse,
                    "The accepted comment response was not understood",
                )
                .to_string(),
        ),
    }
}

/// One page (1-based) of comments updated since `since` (RFC 3339), oldest
/// first, `COMMENT_PAGE_SIZE` per page.
pub async fn fetch_issue_comments_since(
    client: &RepositoryClient,
    number: u64,
    since: &str,
    page: u32,
) -> Result<Vec<GithubIssueComment>, IssueError> {
    client.require(READ, READERS)?;
    client.repository_id()?;
    let (raw, _) = comment_page(
        client,
        READ,
        number,
        &[
            ("since", since.to_string()),
            ("per_page", COMMENT_PAGE_SIZE.to_string()),
            ("page", page.to_string()),
        ],
        INTERACTIVE_TIMEOUT,
    )
    .await?;
    raw.into_iter()
        .map(|raw| owned_comment(client, raw))
        .collect()
}

/// What happened to an issue G5 tried to create.
#[derive(Debug, Clone)]
pub enum IssueCreateOutcome {
    Created(Box<GithubIssue>),
    /// GitHub answered and refused; nothing was created.
    Rejected(IssueError),
    /// No definite answer (timeout, network failure, 5xx): the issue may or
    /// may not exist. Resolve by looking for it, never by creating again.
    Unknown(String),
}

/// Why an issue write did not succeed.
enum WriteFailure {
    /// GitHub answered and refused; nothing changed.
    Rejected(IssueError),
    /// No definite answer: the write may have applied.
    Unknown(String),
}

/// Send one issue write with the person's user access token. A refusal is
/// final; a timeout, network failure or 5xx may have applied. A 422 is the
/// request's fault (`invalid_reference`), not GitHub's.
async fn send_issue_write(
    client: &RepositoryClient,
    op: &'static str,
    method: reqwest::Method,
    segments: &[&str],
    payload: serde_json::Value,
) -> Result<Response, WriteFailure> {
    match send_write(client, op, method, segments, Payload::Json(payload), false).await? {
        Some(response) => Ok(response),
        None => Err(WriteFailure::Unknown(format!("{op}: no response"))),
    }
}

/// [`send_issue_write`] with any payload; with `absent_ok`, GitHub's 404 for
/// a label it does not have ("Label does not exist") is `None` rather than a
/// refusal. Any other 404 (a deleted or moved issue answers a bare "Not
/// Found") and a redirect are still refused.
async fn send_write(
    client: &RepositoryClient,
    op: &'static str,
    method: reqwest::Method,
    segments: &[&str],
    payload: Payload<'_>,
    absent_ok: bool,
) -> Result<Option<Response>, WriteFailure> {
    client
        .require(op, &[RepositoryCapability::IssueWrite])
        .map_err(|error| WriteFailure::Rejected(error.into()))?;
    let repository_id = client
        .repository_id()
        .map_err(|error| WriteFailure::Rejected(error.into()))?
        .to_string();
    let mut path = vec!["repositories", repository_id.as_str()];
    path.extend_from_slice(segments);
    let classify = |error: ProviderError| {
        if error.effect == crate::FailureEffect::Refused {
            WriteFailure::Rejected(error.into())
        } else {
            WriteFailure::Unknown(error.to_string())
        }
    };
    let response = client
        .send(
            op,
            method,
            client.grant.endpoints.api(&path),
            payload,
            &[],
            INTERACTIVE_TIMEOUT,
        )
        .await
        .map_err(classify)?;
    if absent_ok
        && response.status == 404
        && !response.rate_limited()
        && String::from_utf8_lossy(&response.body).contains("Label does not exist")
    {
        return Ok(None);
    }
    if let Err(error) = response.require_success() {
        return Err(match classify(error) {
            WriteFailure::Rejected(mut mapped) => {
                if response.status == 422 && !response.rate_limited() {
                    mapped.kind = ExternalIssueErrorKind::InvalidReference;
                }
                WriteFailure::Rejected(mapped)
            }
            unknown => unknown,
        });
    }
    Ok(Some(response))
}

/// The issue in an accepted write's response, or why it is not trusted.
fn accepted_issue(client: &RepositoryClient, response: &Response) -> Result<GithubIssue, String> {
    response
        .json::<RawIssue>()
        .map_err(IssueError::from)
        .and_then(|raw| {
            validate_issue_urls(client, &raw)?;
            normalize_issue(raw)
        })
        .map_err(|_| {
            response
                .failure(
                    super::ProviderErrorKind::InvalidResponse,
                    "The accepted issue response was not understood",
                )
                .to_string()
        })
}

/// `POST /repositories/{id}/issues` with the person's user access token.
/// GitHub drops assignees the author may not set and refuses unknown ones;
/// it also silently drops labels the author may not set, so callers compare
/// the answer with the labels they asked for.
pub async fn create_issue(
    client: &RepositoryClient,
    title: &str,
    body: &str,
    assignees: &[String],
    labels: &[String],
) -> IssueCreateOutcome {
    let mut payload = serde_json::json!({
        "title": title,
        "body": body,
        "assignees": assignees,
    });
    if !labels.is_empty() {
        payload["labels"] = serde_json::json!(labels);
    }
    match send_issue_write(
        client,
        "create issue",
        reqwest::Method::POST,
        &["issues"],
        payload,
    )
    .await
    {
        Ok(response) => match accepted_issue(client, &response) {
            Ok(issue) => IssueCreateOutcome::Created(Box::new(issue)),
            Err(message) => IssueCreateOutcome::Unknown(message),
        },
        Err(WriteFailure::Rejected(error)) => IssueCreateOutcome::Rejected(error),
        Err(WriteFailure::Unknown(message)) => IssueCreateOutcome::Unknown(message),
    }
}

/// A person's request to close or reopen an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueStateChange {
    /// Close the issue as completed.
    Close,
    /// Reopen the issue.
    Reopen,
}

impl IssueStateChange {
    /// The GitHub `state` the change asks for.
    #[must_use]
    pub fn state(self) -> &'static str {
        match self {
            Self::Close => "closed",
            Self::Reopen => "open",
        }
    }
}

/// What happened when G5 tried to close or reopen an issue.
#[derive(Debug, Clone)]
pub enum IssueUpdateOutcome {
    /// GitHub applied the change; the issue as it answered.
    Updated(Box<GithubIssue>),
    /// GitHub answered and refused; nothing changed.
    Rejected(IssueError),
    /// No definite answer (timeout, network failure, 5xx): the change may
    /// or may not have applied. The write is idempotent, so resolve by
    /// reading the issue or sending the same state again.
    Unknown(String),
}

/// `PATCH /repositories/{id}/issues/{number}` with the person's user access
/// token: close the issue as completed, or reopen it. Sending a state the
/// issue already has is harmless.
pub async fn update_issue_state(
    client: &RepositoryClient,
    number: u64,
    change: IssueStateChange,
) -> IssueUpdateOutcome {
    let number = number.to_string();
    let payload = match change {
        IssueStateChange::Close => {
            serde_json::json!({"state": "closed", "state_reason": "completed"})
        }
        IssueStateChange::Reopen => serde_json::json!({"state": "open"}),
    };
    match send_issue_write(
        client,
        "update issue state",
        reqwest::Method::PATCH,
        &["issues", &number],
        payload,
    )
    .await
    {
        Ok(response) => match accepted_issue(client, &response) {
            Ok(issue) => IssueUpdateOutcome::Updated(Box::new(issue)),
            Err(message) => IssueUpdateOutcome::Unknown(message),
        },
        Err(WriteFailure::Rejected(error)) => IssueUpdateOutcome::Rejected(error),
        Err(WriteFailure::Unknown(message)) => IssueUpdateOutcome::Unknown(message),
    }
}

/// A change to an issue's title, description or assignees; `None` keeps a
/// field. `assignees` is the complete new set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueEdit {
    pub title: Option<String>,
    pub body: Option<String>,
    pub assignees: Option<Vec<String>>,
}

/// `PATCH /repositories/{id}/issues/{number}` with the person's user access
/// token: set the edited fields in one write. GitHub refuses unknown logins
/// and drops assignees the person may not set, so callers compare the
/// answer with the requested set. Sending the same values again is harmless.
pub async fn update_issue(
    client: &RepositoryClient,
    number: u64,
    edit: &IssueEdit,
) -> IssueUpdateOutcome {
    let number = number.to_string();
    let mut payload = serde_json::Map::new();
    if let Some(title) = &edit.title {
        payload.insert("title".into(), serde_json::json!(title));
    }
    if let Some(body) = &edit.body {
        payload.insert("body".into(), serde_json::json!(body));
    }
    if let Some(assignees) = &edit.assignees {
        payload.insert("assignees".into(), serde_json::json!(assignees));
    }
    match send_issue_write(
        client,
        "edit issue",
        reqwest::Method::PATCH,
        &["issues", &number],
        serde_json::Value::Object(payload),
    )
    .await
    {
        Ok(response) => match accepted_issue(client, &response) {
            Ok(issue) => IssueUpdateOutcome::Updated(Box::new(issue)),
            Err(message) => IssueUpdateOutcome::Unknown(message),
        },
        Err(WriteFailure::Rejected(error)) => IssueUpdateOutcome::Rejected(error),
        Err(WriteFailure::Unknown(message)) => IssueUpdateOutcome::Unknown(message),
    }
}

/// `GET /repositories/{id}/assignees/{login}`: whether `login` can be
/// assigned to the repository's issues (204) or not (404).
pub async fn is_assignable(client: &RepositoryClient, login: &str) -> Result<bool, IssueError> {
    client.require(READ, READERS)?;
    let repository_id = client.repository_id()?.to_string();
    let response = interactive_get(
        client,
        &["repositories", &repository_id, "assignees", login],
        &[],
    )
    .await?;
    match response.status {
        204 => Ok(true),
        404 => Ok(false),
        _ => {
            response.require_success()?;
            Err(response
                .failure(
                    super::ProviderErrorKind::InvalidResponse,
                    "GitHub gave an unexpected answer about the assignee",
                )
                .into())
        }
    }
}

/// What happened when G5 tried to make an issue a sub-issue.
#[derive(Debug, Clone)]
pub enum SubIssueOutcome {
    Attached,
    /// GitHub answered and refused (for example, the issue already has a
    /// parent or the parent is full); nothing changed.
    Rejected(IssueError),
    /// No definite answer: read the issue's parent before trying again.
    Unknown(String),
}

/// `POST /repositories/{id}/issues/{parent}/sub_issues` with the person's
/// user access token: make the issue with REST id `sub_issue_id` a sub-issue
/// of `parent`. An existing parent is never replaced.
pub async fn add_sub_issue(
    client: &RepositoryClient,
    parent: u64,
    sub_issue_id: u64,
) -> SubIssueOutcome {
    let number = parent.to_string();
    let payload = serde_json::json!({"sub_issue_id": sub_issue_id});
    match send_issue_write(
        client,
        "add sub-issue",
        reqwest::Method::POST,
        &["issues", &number, "sub_issues"],
        payload,
    )
    .await
    {
        Ok(_) => SubIssueOutcome::Attached,
        Err(WriteFailure::Rejected(error)) => SubIssueOutcome::Rejected(error),
        Err(WriteFailure::Unknown(message)) => SubIssueOutcome::Unknown(message),
    }
}

/// `GET /repositories/{id}/issues/{number}/parent`: the issue's parent, or
/// `None` when it has none.
pub async fn fetch_parent_issue(
    client: &RepositoryClient,
    number: u64,
) -> Result<Option<GithubIssue>, IssueError> {
    client.require(READ, READERS)?;
    let repository_id = client.repository_id()?.to_string();
    let response = interactive_get(
        client,
        &[
            "repositories",
            &repository_id,
            "issues",
            &number.to_string(),
            "parent",
        ],
        &[],
    )
    .await?;
    if response.status == 404 {
        return Ok(None);
    }
    let raw: RawIssue = response.json()?;
    validate_issue_urls(client, &raw)?;
    normalize_issue(raw).map(Some)
}

/// One page (1-based) of issues `creator` opened that changed since `since`
/// (RFC 3339), newest first, and whether GitHub reports another page. Pull
/// requests are skipped.
pub async fn fetch_issues_created_by(
    client: &RepositoryClient,
    creator: &str,
    since: &str,
    page: u32,
) -> Result<(Vec<GithubIssue>, bool), IssueError> {
    client.require(READ, READERS)?;
    let repository_id = client.repository_id()?.to_string();
    let response = interactive_get(
        client,
        &["repositories", &repository_id, "issues"],
        &[
            ("creator", creator.to_string()),
            ("state", "all".to_string()),
            ("since", since.to_string()),
            ("sort", "created".to_string()),
            ("direction", "desc".to_string()),
            ("per_page", COMMENT_PAGE_SIZE.to_string()),
            ("page", page.to_string()),
        ],
    )
    .await?;
    let more = response.has_next_page();
    let raw: Vec<RawIssue> = response.json()?;
    let mut issues = Vec::with_capacity(raw.len());
    for raw in raw {
        if raw.is_pull_request() {
            continue;
        }
        validate_issue_urls(client, &raw)?;
        issues.push(normalize_issue(raw)?);
    }
    Ok((issues, more))
}

/// `GET /repositories/{id}/assignees`: the first page of accounts that can
/// be assigned to the repository's issues.
pub async fn fetch_assignable_users(
    client: &RepositoryClient,
) -> Result<Vec<GithubIssueUser>, IssueError> {
    let repository_id = client.repository_id()?.to_string();
    let raw: Vec<RawUser> = github_get(
        client,
        &["repositories", &repository_id, "assignees"],
        &[("per_page", COMMENT_PAGE_SIZE.to_string())],
    )
    .await?;
    Ok(raw.into_iter().map(user).collect())
}

/// Issue state filter for search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueStateFilter {
    Open,
    Closed,
    All,
}

impl IssueStateFilter {
    #[must_use]
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("open") => Some(Self::Open),
            Some("closed") => Some(Self::Closed),
            Some("all") => Some(Self::All),
            Some(_) => None,
        }
    }
}

/// Qualifiers the integration owns. A user term with one of these prefixes
/// could widen the search beyond the domain repository (GitHub ORs repeated
/// `repo:` qualifiers) or contradict the issue/state filter, so it is dropped.
const RESERVED_QUALIFIERS: &[&str] =
    &["repo:", "org:", "user:", "owner:", "is:", "type:", "state:"];

/// The reserved qualifier `term` starts with (`repo:`, `is:` …), if any:
/// [`build_search_query`] drops such terms.
#[must_use]
pub fn reserved_qualifier(term: &str) -> Option<&'static str> {
    let bare = term
        .trim_start_matches(['-', '"', '('])
        .to_ascii_lowercase();
    RESERVED_QUALIFIERS
        .iter()
        .copied()
        .find(|qualifier| bare.starts_with(qualifier))
}

/// What a search is limited to besides the domain repository and the
/// user's terms.
#[derive(Debug, Clone, Copy)]
pub struct IssueSearchFilter<'a> {
    pub state: IssueStateFilter,
    pub assignee: IssueAssigneeFilter<'a>,
}

/// Who a searched issue must be assigned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueAssigneeFilter<'a> {
    Any,
    /// Assigned to this GitHub account (among others).
    Account(&'a GithubAuthenticatedUser),
    /// Assigned to no one.
    Unassigned,
}

/// `repo:{fullName} is:issue <terms> [state:open|closed] [assignee:{login} | no:assignee]`.
#[must_use]
pub fn build_search_query(full_name: &str, user_query: &str, filter: IssueSearchFilter) -> String {
    let mut parts = vec![format!("repo:{full_name}"), "is:issue".to_string()];
    let bounded: String = user_query.chars().take(MAX_USER_QUERY_CHARS).collect();
    for term in bounded.split_whitespace() {
        if reserved_qualifier(term).is_none() {
            parts.push(term.to_string());
        }
    }
    match filter.state {
        IssueStateFilter::Open => parts.push("state:open".to_string()),
        IssueStateFilter::Closed => parts.push("state:closed".to_string()),
        IssueStateFilter::All => {}
    }
    match filter.assignee {
        IssueAssigneeFilter::Any => {}
        IssueAssigneeFilter::Account(account) => {
            parts.push(format!("assignee:{}", account.login));
        }
        IssueAssigneeFilter::Unassigned => parts.push("no:assignee".to_string()),
    }
    parts.join(" ")
}

/// Keep only issues from `full_name`; pull requests and anything GitHub
/// returned for another repository are excluded. With an account, only
/// issues assigned to that account id are kept: the `assignee:` qualifier
/// names a login, which can be renamed and reclaimed by another account.
/// Unassigned keeps only issues without assignees.
pub(crate) fn filter_search_items(
    items: Vec<RawIssue>,
    full_name: &str,
    assignee: IssueAssigneeFilter<'_>,
) -> Vec<GithubIssue> {
    items
        .into_iter()
        .filter(|item| !item.is_pull_request())
        .filter_map(|item| normalize_issue(item).ok())
        .filter(|issue| {
            issue
                .repository_full_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(full_name))
        })
        .filter(|issue| match assignee {
            IssueAssigneeFilter::Any => true,
            IssueAssigneeFilter::Account(account) => issue
                .assignees
                .iter()
                .any(|user| user.id == Some(account.id)),
            IssueAssigneeFilter::Unassigned => issue.assignees.is_empty(),
        })
        .take(SEARCH_PAGE_SIZE)
        .collect()
}

/// One repository's search results, most recently updated first.
#[derive(Debug, Clone)]
pub struct GithubIssueSearchPage {
    pub issues: Vec<GithubIssue>,
    /// GitHub matched more issues than it returned, or timed out before
    /// finding them all.
    pub has_more: bool,
}

/// `GET /search/issues` limited to one repository.
pub async fn search_repository_issues(
    client: &RepositoryClient,
    user_query: &str,
    filter: IssueSearchFilter<'_>,
) -> Result<GithubIssueSearchPage, IssueError> {
    client.require("search repository issues", READERS)?;
    let full_name = client.repository_name()?;
    let response = interactive_get(
        client,
        &["search", "issues"],
        &[
            ("q", build_search_query(full_name, user_query, filter)),
            ("sort", "updated".into()),
            ("order", "desc".into()),
            ("per_page", SEARCH_PAGE_SIZE.to_string()),
        ],
    )
    .await?;
    let page: RawSearchPage = response.json().map_err(|error| {
        // A non-rate-limit validation refusal names the user's query.
        let refused_query = error.status == Some(422) && !error.rate_limited;
        let mut mapped = IssueError::from(error);
        if refused_query {
            mapped.kind = ExternalIssueErrorKind::InvalidQuery;
        }
        mapped
    })?;
    Ok(search_page(page, full_name, filter.assignee, |raw| {
        validate_issue_urls(client, raw).is_ok()
    }))
}

fn search_page(
    mut page: RawSearchPage,
    full_name: &str,
    assignee: IssueAssigneeFilter<'_>,
    valid: impl FnMut(&RawIssue) -> bool,
) -> GithubIssueSearchPage {
    // Completeness describes the provider's page, before local safety/domain
    // filtering. A discarded record does not imply another page exists.
    let returned = u64::try_from(page.items.len()).unwrap_or(u64::MAX);
    page.items.retain(valid);
    GithubIssueSearchPage {
        has_more: page.incomplete_results || page.total_count > returned,
        issues: filter_search_items(page.items, full_name, assignee),
    }
}

// ---------------------------------------------------------------------------
// Reference parsing
// ---------------------------------------------------------------------------

/// A parsed GitHub issue reference: `owner/name#number` or an issue URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssueReference {
    pub owner: String,
    pub name: String,
    pub number: u64,
}

impl GithubIssueReference {
    /// `owner/name` as written in the reference.
    #[must_use]
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Case-insensitive `owner/name` match, as GitHub treats them.
    #[must_use]
    pub fn names_repository(&self, full_name: &str) -> bool {
        self.full_name().eq_ignore_ascii_case(full_name)
    }
}

fn parse_number(value: &str) -> Result<u64, IssueError> {
    let invalid = || IssueError::invalid_reference("the issue number is not valid");
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    let number: u64 = value.parse().map_err(|_| invalid())?;
    if number == 0 || number > MAX_ISSUE_NUMBER {
        return Err(invalid());
    }
    Ok(number)
}

fn parse_issue_url(
    input: &str,
    endpoints: &ProviderEndpoints,
) -> Result<GithubIssueReference, IssueError> {
    let path = endpoints.web_path(input).ok_or_else(|| {
        IssueError::invalid_reference("the URL is not a GitHub issue URL for this tracker")
    })?;
    match path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [_, _, "pull" | "pulls", ..] => Err(IssueError::not_an_issue()),
        [owner, name, "issues", number, ..] => {
            if !valid_repository_parts(owner, name) {
                return Err(IssueError::invalid_reference(
                    "the repository in the URL is not valid",
                ));
            }
            Ok(GithubIssueReference {
                owner: (*owner).to_string(),
                name: (*name).to_string(),
                number: parse_number(number)?,
            })
        }
        _ => Err(IssueError::invalid_reference(
            "the URL does not name a GitHub issue",
        )),
    }
}

/// Parse `https://github.com/{owner}/{name}/issues/{n}` (or the same path on
/// the configured GitHub web base URL) and `owner/name#n`. Pull request URLs
/// fail with `not_an_issue`; anything else unparseable with
/// `invalid_reference`.
pub fn parse_issue_reference(
    input: &str,
    endpoints: &ProviderEndpoints,
) -> Result<GithubIssueReference, IssueError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(IssueError::invalid_reference(
            "an issue reference is required",
        ));
    }
    if input.len() > 2048 {
        return Err(IssueError::invalid_reference(
            "the issue reference is too long",
        ));
    }
    if input.contains("://") {
        return parse_issue_url(input, endpoints);
    }
    let (repository, number) = input
        .split_once('#')
        .ok_or_else(|| IssueError::invalid_reference("use an issue URL or owner/name#number"))?;
    let (owner, name) = repository
        .split_once('/')
        .ok_or_else(|| IssueError::invalid_reference("use an issue URL or owner/name#number"))?;
    if !valid_repository_parts(owner, name) {
        return Err(IssueError::invalid_reference(
            "the repository in the reference is not valid",
        ));
    }
    Ok(GithubIssueReference {
        owner: owner.to_string(),
        name: name.to_string(),
        number: parse_number(number)?,
    })
}

/// Parse a canonical positive decimal GitHub id (no sign or leading zeros).
#[must_use]
pub fn parse_canonical_github_id(value: &str) -> Option<u64> {
    if value.is_empty() || value.starts_with('0') || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && i64::try_from(*id).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_client(
        base: &str,
        identity: super::super::RepositoryIdentity,
        capability: RepositoryCapability,
        token: &str,
    ) -> RepositoryClient {
        use super::super::{ProviderClients, ProviderEndpoints, RepositoryGrant};
        ProviderClients::new().unwrap().repository(
            RepositoryGrant::new(
                ProviderEndpoints::new(crate::ScmProvider::GitHub, base, "https://github.com")
                    .unwrap(),
                identity,
                capability,
                token.to_owned(),
            )
            .unwrap(),
        )
    }
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const WEB: &str = "https://ghe.example.test";

    /// The parser bound to a configured web base (API base is irrelevant).
    fn parse_issue_reference(input: &str, web: &str) -> Result<GithubIssueReference, IssueError> {
        super::parse_issue_reference(
            input,
            &ProviderEndpoints::new(crate::ScmProvider::GitHub, web, web).unwrap(),
        )
    }

    fn reference(owner: &str, name: &str, number: u64) -> GithubIssueReference {
        GithubIssueReference {
            owner: owner.to_string(),
            name: name.to_string(),
            number,
        }
    }

    fn kind(result: Result<GithubIssueReference, IssueError>) -> ExternalIssueErrorKind {
        result.expect_err("reference must be rejected").kind
    }

    #[test]
    fn parses_issue_urls_on_github_and_configured_host() {
        assert_eq!(
            parse_issue_reference(
                "https://github.com/Acme/web/issues/12",
                "https://github.com"
            ),
            Ok(reference("Acme", "web", 12))
        );
        assert_eq!(
            parse_issue_reference(
                "  https://github.com/acme/web.site/issues/7#issuecomment-1?x=1 ",
                "https://github.com"
            ),
            Ok(reference("acme", "web.site", 7))
        );
        assert_eq!(
            parse_issue_reference("https://ghe.example.test/acme/web/issues/3/", WEB),
            Ok(reference("acme", "web", 3))
        );
        assert_eq!(
            parse_issue_reference(
                "http://127.0.0.1:9999/acme/web/issues/3",
                "http://127.0.0.1:9999"
            ),
            Ok(reference("acme", "web", 3))
        );
    }

    #[test]
    fn refuses_a_different_instance_even_when_repository_names_match() {
        assert!(parse_issue_reference("https://github.com/acme/web/issues/12", WEB).is_err());
        assert!(parse_issue_reference(
            "https://ghe.example.test/acme/web/issues/12",
            "https://github.com"
        )
        .is_err());
        // An Enterprise prefix and port bound the tracker.
        let prefixed = "https://ghe.example.test:8443/git";
        assert_eq!(
            parse_issue_reference(
                "https://ghe.example.test:8443/git/acme/web/issues/12",
                prefixed
            ),
            Ok(reference("acme", "web", 12))
        );
        for other in [
            "https://ghe.example.test/git/acme/web/issues/12",
            "https://ghe.example.test:8443/acme/web/issues/12",
            "https://ghe.example.test:8443/git-other/acme/web/issues/12",
        ] {
            assert!(parse_issue_reference(other, prefixed).is_err(), "{other}");
        }
    }

    #[test]
    fn parses_short_references() {
        assert_eq!(
            parse_issue_reference("acme/web#42", WEB),
            Ok(reference("acme", "web", 42))
        );
        assert_eq!(
            parse_issue_reference("my-org/repo_name.js#1", WEB),
            Ok(reference("my-org", "repo_name.js", 1))
        );
        assert!(reference("Acme", "Web", 1).names_repository("acme/web"));
        assert!(!reference("acme", "web", 1).names_repository("acme/web2"));
    }

    #[test]
    fn rejects_pull_requests_as_not_an_issue() {
        for input in [
            "https://ghe.example.test/acme/web/pull/12",
            "https://ghe.example.test/acme/web/pull/12/files",
            "https://ghe.example.test/acme/web/pulls/12",
        ] {
            assert_eq!(
                kind(parse_issue_reference(input, WEB)),
                ExternalIssueErrorKind::NotAnIssue,
                "{input}"
            );
        }
    }

    #[test]
    fn rejects_garbage_as_invalid_reference() {
        for input in [
            "",
            "   ",
            "acme/web",
            "#12",
            "acme#12",
            "acme/web#",
            "acme/web#0",
            "acme/web#-1",
            "acme/web#12abc",
            "acme/web#99999999999",
            "ac me/web#1",
            "acme/../web#1",
            "acme/web/extra#1",
            "https://gitlab.com/acme/web/issues/1",
            "https://github.com/acme/web",
            "https://github.com/acme/web/issues/",
            "https://github.com/acme/web/issues/abc",
            "https://user:pw@github.com/acme/web/issues/1",
            "ftp://github.com/acme/web/issues/1",
            "https://github.com/acme/web/discussions/1",
            "not a url://",
        ] {
            assert_eq!(
                kind(parse_issue_reference(input, WEB)),
                ExternalIssueErrorKind::InvalidReference,
                "{input:?}"
            );
        }
    }

    #[test]
    fn canonical_ids_are_positive_decimals_without_leading_zeros() {
        assert_eq!(parse_canonical_github_id("1001"), Some(1001));
        assert_eq!(
            parse_canonical_github_id("9007199254740993"),
            Some(9_007_199_254_740_993)
        );
        for value in [
            "",
            "0",
            "0123",
            "-1",
            "+1",
            "1e3",
            "12a",
            "18446744073709551615",
        ] {
            assert_eq!(parse_canonical_github_id(value), None, "{value}");
        }
    }

    fn raw_issue(value: serde_json::Value) -> RawIssue {
        serde_json::from_value(value).expect("raw issue")
    }

    fn issue_json(id: u64, number: u64) -> serde_json::Value {
        json!({
            "id": id,
            "number": number,
            "title": "Crash on save",
            "body": "It crashes",
            "state": "open",
            "state_reason": null,
            "html_url": format!("https://github.com/acme/issues/issues/{number}"),
            "repository_url": "https://api.github.com/repos/acme/issues",
            "user": {"login": "octo", "avatar_url": "https://avatars.example/octo"},
            "assignees": [{"login": "hubot", "avatar_url": ""}],
            "labels": [{"id": 208045946, "name": "bug", "color": "D73A4A", "description": "Broken"}, "triage"],
            "comments": 3,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z",
            "closed_at": null
        })
    }

    #[test]
    fn issue_ids_are_normalized_losslessly() {
        // 2^53 + 1 is not representable as an f64; it must survive intact.
        let issue = normalize_issue(raw_issue(issue_json(9_007_199_254_740_993, 12))).unwrap();
        assert_eq!(issue.id, "9007199254740993");
        assert_eq!(issue.number, 12);
        assert_eq!(issue.repository_full_name.as_deref(), Some("acme/issues"));
        assert_eq!(issue.author.as_ref().unwrap().login, "octo");
        assert_eq!(issue.assignees[0].avatar_url, None);
        assert_eq!(
            issue.labels,
            vec![
                GithubIssueLabel {
                    id: Some(208_045_946),
                    name: "bug".to_string(),
                    color: Some("d73a4a".to_string()),
                    description: Some("Broken".to_string()),
                },
                GithubIssueLabel {
                    id: None,
                    name: "triage".to_string(),
                    color: None,
                    description: None,
                },
            ]
        );
        assert_eq!(issue.comment_count, 3);
        assert!(normalize_issue(raw_issue(issue_json(0, 12))).is_err());
    }

    #[test]
    fn pull_requests_are_rejected_and_excluded_from_search() {
        let mut pull = issue_json(2, 13);
        pull["pull_request"] = json!({"url": "https://api.github.com/repos/acme/issues/pulls/13"});
        assert_eq!(
            normalize_issue(raw_issue(pull.clone())).unwrap_err().kind,
            ExternalIssueErrorKind::NotAnIssue
        );
        // An explicit null is not a pull request.
        let mut null_pull = issue_json(3, 14);
        null_pull["pull_request"] = serde_json::Value::Null;
        assert!(normalize_issue(raw_issue(null_pull)).is_ok());

        let mut other_repo = issue_json(4, 15);
        other_repo["repository_url"] = json!("https://api.github.com/repos/other/public");
        let items = vec![
            raw_issue(issue_json(1, 12)),
            raw_issue(pull),
            raw_issue(other_repo),
        ];
        let results = filter_search_items(items, "ACME/Issues", IssueAssigneeFilter::Any);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn assignee_filters_name_the_login_or_no_assignee_and_check_results() {
        let ada = GithubAuthenticatedUser {
            id: 501,
            login: "ada".to_string(),
        };
        let query = |text: &str, assignee| {
            build_search_query(
                "acme/issues",
                text,
                IssueSearchFilter {
                    state: IssueStateFilter::Open,
                    assignee,
                },
            )
        };
        assert_eq!(
            query("", IssueAssigneeFilter::Account(&ada)),
            "repo:acme/issues is:issue state:open assignee:ada"
        );
        assert_eq!(
            query("crash", IssueAssigneeFilter::Unassigned),
            "repo:acme/issues is:issue crash state:open no:assignee"
        );

        let assigned = |id: u64, number: u64, assignees: serde_json::Value| {
            let mut issue = issue_json(id, number);
            issue["assignees"] = assignees;
            raw_issue(issue)
        };
        let ids = |assignee| {
            let items = vec![
                assigned(
                    1,
                    12,
                    json!([{"id": 7, "login": "hubot"}, {"id": 501, "login": "ada"}]),
                ),
                // The login was reclaimed by another account.
                assigned(2, 13, json!([{"id": 999, "login": "ada"}])),
                assigned(3, 14, json!([])),
            ];
            filter_search_items(items, "acme/issues", assignee)
                .into_iter()
                .map(|issue| issue.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(IssueAssigneeFilter::Account(&ada)), ["1"]);
        assert_eq!(ids(IssueAssigneeFilter::Unassigned), ["3"]);
        assert_eq!(ids(IssueAssigneeFilter::Any), ["1", "2", "3"]);
    }

    #[test]
    fn timeline_keeps_comments_and_shown_events() {
        let values = vec![
            json!({"event": "commented", "id": 7001, "user": {"login": "ada"},
                   "body": "First", "created_at": "2026-01-02T00:00:00Z",
                   "updated_at": "2026-01-02T00:00:00Z",
                   "html_url": "https://github.com/acme/issues/issues/12#issuecomment-7001"}),
            json!({"event": "labeled", "id": 42, "actor": {"login": "ada"},
                   "label": {"name": "bug", "color": "d73a4a"},
                   "created_at": "2026-01-03T00:00:00Z"}),
            json!({"event": "closed", "id": 43, "actor": {"login": "ada"},
                   "state_reason": "completed", "created_at": "2026-01-04T00:00:00Z"}),
            json!({"event": "cross-referenced", "actor": {"login": "bob"},
                   "created_at": "2026-01-05T00:00:00Z",
                   "source": {"type": "issue", "issue": {"number": 99,
                       "repository": {"full_name": "acme/web"}}}}),
            // Not shown, or malformed.
            json!({"event": "committed", "sha": "abc"}),
            json!({"event": "subscribed", "id": 44, "created_at": "2026-01-06T00:00:00Z"}),
            json!({"event": "commented", "id": 7002}),
        ];
        let items: Vec<GithubTimelineItem> = values
            .into_iter()
            .enumerate()
            .filter_map(|(position, value)| {
                serde_json::from_value::<RawTimelineItem>(value)
                    .ok()
                    .and_then(|raw| timeline_item(position, raw))
            })
            .collect();
        assert_eq!(items.len(), 4, "{items:?}");
        let GithubTimelineItem::Comment(comment) = &items[0] else {
            panic!("expected a comment");
        };
        assert_eq!(
            (comment.id.as_str(), comment.body.as_str()),
            ("7001", "First")
        );
        assert_eq!(comment.author.as_ref().unwrap().login, "ada");
        let events: Vec<(String, String, Option<String>)> = items[1..]
            .iter()
            .map(|item| match item {
                GithubTimelineItem::Event(event) => {
                    (event.key.clone(), event.event.clone(), event.detail.clone())
                }
                GithubTimelineItem::Comment(_) => panic!("expected an event"),
            })
            .collect();
        assert_eq!(
            events,
            vec![
                (
                    "42".to_string(),
                    "labeled".to_string(),
                    Some("bug".to_string())
                ),
                ("43".to_string(), "closed".to_string(), None),
                (
                    "cross-referenced:2026-01-05T00:00:00Z:acme/web#99:bob:3".to_string(),
                    "cross-referenced".to_string(),
                    Some("acme/web#99".to_string())
                ),
            ]
        );
    }

    #[test]
    fn keyless_timeline_events_stay_distinct() {
        let reference = json!({"event": "cross-referenced", "actor": {"login": "bob"},
            "created_at": "2026-01-05T00:00:00Z", "source": {"type": "issue"}});
        let keys: Vec<String> = [reference.clone(), reference]
            .into_iter()
            .enumerate()
            .filter_map(|(position, value)| {
                serde_json::from_value::<RawTimelineItem>(value)
                    .ok()
                    .and_then(|raw| timeline_item(position, raw))
            })
            .map(|item| match item {
                GithubTimelineItem::Event(event) => event.key,
                GithubTimelineItem::Comment(_) => panic!("expected an event"),
            })
            .collect();
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1], "{keys:?}");
    }

    #[test]
    fn repository_audience_follows_visibility_then_the_private_flag() {
        use RepositoryAudience::{NonPublic, Public, Unknown};
        assert_eq!(
            RepositoryAudience::classify(Some("private"), Some(false)),
            NonPublic
        );
        assert_eq!(
            RepositoryAudience::classify(Some("Internal"), None),
            NonPublic
        );
        assert_eq!(
            RepositoryAudience::classify(Some("public"), Some(true)),
            Public
        );
        assert_eq!(RepositoryAudience::classify(Some("secret"), None), Unknown);
        assert_eq!(RepositoryAudience::classify(None, Some(true)), NonPublic);
        assert_eq!(RepositoryAudience::classify(None, Some(false)), Public);
        assert_eq!(RepositoryAudience::classify(None, None), Unknown);
    }

    #[test]
    fn search_page_reports_matches_github_did_not_return() {
        let page = |total_count, incomplete_results| RawSearchPage {
            total_count,
            incomplete_results,
            items: vec![raw_issue(issue_json(1, 12)), raw_issue(issue_json(2, 13))],
        };
        for (total, incomplete, expected_more) in
            [(2, false, false), (41, false, true), (2, true, true)]
        {
            let result = search_page(
                page(total, incomplete),
                "acme/issues",
                IssueAssigneeFilter::Any,
                |_| true,
            );
            assert_eq!(result.has_more, expected_more);
            assert_eq!(result.issues.len(), 2);
        }
        let filtered = search_page(
            page(2, false),
            "acme/issues",
            IssueAssigneeFilter::Any,
            |_| false,
        );
        assert!(filtered.issues.is_empty());
        assert!(
            !filtered.has_more,
            "local filtering must not invent a missing provider page"
        );

        // A response that omits either completeness field is rejected rather
        // than read as complete.
        for body in [
            json!({"incomplete_results": false, "items": []}),
            json!({"total_count": 0, "items": []}),
        ] {
            assert!(serde_json::from_value::<RawSearchPage>(body).is_err());
        }
    }

    fn state_only(state: IssueStateFilter) -> IssueSearchFilter<'static> {
        IssueSearchFilter {
            state,
            assignee: IssueAssigneeFilter::Any,
        }
    }

    #[test]
    fn search_query_is_confined_to_the_domain_repository() {
        assert_eq!(
            build_search_query(
                "acme/issues",
                "crash on save",
                state_only(IssueStateFilter::Open)
            ),
            "repo:acme/issues is:issue crash on save state:open"
        );
        assert_eq!(
            build_search_query(
                "acme/issues",
                "crash repo:other/secret -org:x is:pr State:closed label:bug",
                state_only(IssueStateFilter::All)
            ),
            "repo:acme/issues is:issue crash label:bug"
        );
        assert_eq!(
            build_search_query("acme/issues", "", state_only(IssueStateFilter::Closed)),
            "repo:acme/issues is:issue state:closed"
        );
        assert_eq!(IssueStateFilter::parse(None), Some(IssueStateFilter::Open));
        assert_eq!(
            IssueStateFilter::parse(Some("all")),
            Some(IssueStateFilter::All)
        );
        assert_eq!(IssueStateFilter::parse(Some("bogus")), None);
    }

    #[test]
    fn github_statuses_map_to_typed_errors() {
        use ExternalIssueErrorKind as Kind;
        // Transport evidence (status and rate-limit detection) mapped to the
        // external-issue kind; there is no second status table.
        let kind = |status: Option<u16>, limited| {
            let mut error = ProviderError::new(
                READ,
                super::super::ProviderErrorKind::Rejected,
                crate::FailureEffect::Refused,
                "refused",
            );
            error.status = status;
            error.rate_limited = limited;
            IssueError::from(error).kind
        };
        assert_eq!(kind(Some(200), false), Kind::Unavailable);
        assert_eq!(kind(None, false), Kind::Unavailable);
        assert_eq!(kind(Some(401), false), Kind::AccessDenied);
        assert_eq!(kind(Some(403), false), Kind::AccessDenied);
        assert_eq!(kind(Some(403), true), Kind::RateLimited);
        assert_eq!(kind(Some(422), true), Kind::RateLimited);
        assert_eq!(kind(Some(429), true), Kind::RateLimited);
        assert_eq!(kind(Some(404), false), Kind::NotFound);
        assert_eq!(kind(Some(410), false), Kind::NotFound);
        assert_eq!(kind(Some(301), false), Kind::NotFound);
        assert_eq!(kind(Some(500), false), Kind::Unavailable);
        assert_eq!(kind(Some(502), false), Kind::Unavailable);
        assert_eq!(kind(Some(422), false), Kind::Unavailable);

        let error: G5Error = IssueError::unavailable("down").into();
        assert_eq!(
            error.status_code(),
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        );
        let response = error.to_error_response(String::new());
        assert_eq!(response.code, "SERVICE_UNAVAILABLE");
        assert_eq!(response.details, json!({"issueError": "unavailable"}));
        assert_eq!(response.message, "down");
        for (kind, status, code) in [
            (Kind::AccessDenied, 403, "FORBIDDEN"),
            (Kind::NotFound, 404, "NOT_FOUND"),
            (Kind::OutsideDomain, 422, "VALIDATION_ERROR"),
            (Kind::NotAnIssue, 422, "VALIDATION_ERROR"),
            (Kind::InvalidReference, 422, "VALIDATION_ERROR"),
            (Kind::InvalidQuery, 422, "VALIDATION_ERROR"),
            (Kind::RateLimited, 503, "SERVICE_UNAVAILABLE"),
            (Kind::BindingNeedsAttention, 409, "CONFLICT"),
        ] {
            let error: G5Error = IssueError::new(kind, "x").into();
            assert_eq!(error.status_code().as_u16(), status);
            let response = error.to_error_response(String::new());
            assert_eq!(response.code, code);
            assert_eq!(response.details["issueError"], kind.as_str());
        }
    }

    #[tokio::test]
    async fn search_refusals_and_secondary_rate_limits_are_typed() {
        let server = MockServer::start().await;
        let respond = |q: &str, response: ResponseTemplate| {
            Mock::given(method("GET"))
                .and(path("/search/issues"))
                .and(query_param(
                    "q",
                    format!("repo:acme/issues is:issue {q} state:open"),
                ))
                .respond_with(response)
        };
        respond(
            "author:nobody",
            ResponseTemplate::new(422).set_body_json(json!({
                "message": "Validation Failed",
                "errors": [{"message": "The listed users cannot be searched either because the users do not exist or you do not have permission to view the users.", "resource": "Search", "field": "q", "code": "invalid"}]
            })),
        )
        .mount(&server)
        .await;
        respond(
            "burst",
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "25")
                .set_body_json(json!({"message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."})),
        )
        .mount(&server)
        .await;
        respond(
            "wait",
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "25")
                .insert_header("retry-after", "60")
                .set_body_string(""),
        )
        .mount(&server)
        .await;
        respond(
            "spam",
            ResponseTemplate::new(422)
                .insert_header("x-ratelimit-remaining", "25")
                .set_body_json(json!({"message": "You have exceeded a secondary rate limit and have been temporarily blocked from content creation."})),
        )
        .mount(&server)
        .await;
        respond(
            "denied",
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "25")
                .set_body_json(json!({"message": "Resource not accessible by integration"})),
        )
        .mount(&server)
        .await;

        let client = test_client(
            &server.uri(),
            super::super::RepositoryIdentity::Name("acme/issues".into()),
            RepositoryCapability::IssuesRead,
            "ghs_read",
        );
        async fn search(client: &RepositoryClient, q: &str) -> IssueError {
            search_repository_issues(
                client,
                q,
                IssueSearchFilter {
                    state: IssueStateFilter::Open,
                    assignee: IssueAssigneeFilter::Any,
                },
            )
            .await
            .unwrap_err()
        }
        let refused = search(&client, "author:nobody").await;
        assert_eq!(refused.kind, ExternalIssueErrorKind::InvalidQuery);
        assert!(
            refused
                .message
                .contains("The listed users cannot be searched"),
            "{refused:?}"
        );
        for q in ["burst", "wait", "spam"] {
            let limited = search(&client, q).await;
            assert_eq!(limited.kind, ExternalIssueErrorKind::RateLimited, "{q}");
            assert!(limited.message.contains("rate limit"), "{limited:?}");
        }
        assert_eq!(
            search(&client, "denied").await.kind,
            ExternalIssueErrorKind::AccessDenied
        );

        let writer = test_client(
            &server.uri(),
            super::super::RepositoryIdentity::Id(1001),
            RepositoryCapability::IssueCommentWrite,
            "ghu_person",
        );
        // Comment posting: a 422 is a refused comment unless it reports a
        // rate limit.
        for (number, message, kind) in [
            (
                1,
                "Validation Failed",
                ExternalIssueErrorKind::InvalidReference,
            ),
            (
                2,
                "You have exceeded a secondary rate limit",
                ExternalIssueErrorKind::RateLimited,
            ),
        ] {
            Mock::given(method("POST"))
                .and(path(format!("/repositories/1001/issues/{number}/comments")))
                .respond_with(ResponseTemplate::new(422).set_body_json(json!({"message": message})))
                .mount(&server)
                .await;
            match post_issue_comment(&writer, number, "hi").await {
                CommentPostOutcome::Rejected(error) => assert_eq!(error.kind, kind, "{message}"),
                other => panic!("expected a refusal for {message}: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn rest_reads_use_the_token_and_map_failures() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repositories/1001"))
            .and(header("authorization", "Bearer ghs_read"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id": 1001, "full_name": "acme/issues"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repositories/1001/issues/12"))
            .respond_with(ResponseTemplate::new(200).set_body_json({
                let mut issue = issue_json(501, 12);
                issue["repository_url"] = json!(format!("{}/repos/acme/issues", server.uri()));
                issue
            }))
            .mount(&server)
            .await;
        let mut pull = issue_json(502, 13);
        pull["pull_request"] = json!({"url": "x"});
        Mock::given(method("GET"))
            .and(path("/repositories/1001/issues/13"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pull))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repositories/1001/issues/14"))
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;
        // A 422 outside search is not a refused query.
        Mock::given(method("GET"))
            .and(path("/repositories/1001/issues/16"))
            .respond_with(
                ResponseTemplate::new(422).set_body_json(json!({"message": "Validation Failed"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repositories/1001/issues/15"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(6)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repositories/1001/issues/12/comments"))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 2, "user": null, "body": "second", "created_at": "2026-01-03T00:00:00Z",
                 "updated_at": "2026-01-03T00:00:00Z", "html_url": "https://github.com/c/2"},
                {"id": 1, "user": {"login": "octo"}, "body": "first", "created_at": "2026-01-02T00:00:00Z",
                 "updated_at": "2026-01-02T00:00:00Z", "html_url": "https://github.com/c/1"}
            ])))
            .mount(&server)
            .await;

        let client = test_client(
            &server.uri(),
            super::super::RepositoryIdentity::Id(1001),
            RepositoryCapability::IssuesRead,
            "ghs_read",
        );
        let repository = fetch_repository(&client).await.unwrap();
        assert_eq!(repository.full_name, "acme/issues");
        let issue = fetch_issue(&client, 12).await.unwrap();
        assert_eq!(issue.id, "501");
        assert_eq!(
            fetch_issue(&client, 13).await.unwrap_err().kind,
            ExternalIssueErrorKind::NotAnIssue
        );
        assert_eq!(
            fetch_issue(&client, 14).await.unwrap_err().kind,
            ExternalIssueErrorKind::Unavailable
        );
        assert_eq!(
            fetch_issue(&client, 99).await.unwrap_err().kind,
            ExternalIssueErrorKind::NotFound
        );
        assert_eq!(
            fetch_issue(&client, 16).await.unwrap_err().kind,
            ExternalIssueErrorKind::Unavailable
        );
        let timed_out = fetch_issue(&client, 15).await.unwrap_err();
        assert_eq!(timed_out.kind, ExternalIssueErrorKind::Unavailable);
        assert!(timed_out.message.contains("5 seconds"), "{timed_out:?}");
        let comments = fetch_issue_comments(&client, 12).await.unwrap();
        let bodies: Vec<&str> = comments.iter().map(|c| c.body.as_str()).collect();
        assert_eq!(bodies, vec!["first", "second"]);
    }
}
