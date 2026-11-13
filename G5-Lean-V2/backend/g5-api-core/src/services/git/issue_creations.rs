//! Issues created from G5 in the organization's default creation target
//! (`g5.external-issues.creation`).
//!
//! A person creates under their own GitHub identity, a workspace's agent
//! under its owner's, through the same guards as comments. G5 stores nothing
//! for a creation: the description ends with a hidden marker derived from
//! the organization, the author and the submission's idempotency key. Before
//! sending, and when a person checks an unclear outcome, the author's recent
//! issues are searched for that marker, so a retry with the same key finds
//! the issue instead of creating another (`g5.external-issues.creation.recovery`).

use std::sync::LazyLock;

use g5_contracts::error::G5Error;
use regex::Regex;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::services::git::github_issues::GithubIssue;
use crate::services::git::issue_comments::{secret_kinds, MAX_COMMENT_CHARS};

/// Longest title accepted (GitHub's own limit is 256 characters).
pub const MAX_TITLE_CHARS: usize = 256;
/// Most assignees GitHub accepts on one issue.
pub const MAX_ASSIGNEES: usize = 10;
/// How far back the marker is looked for: retries come within this window.
pub const MARKER_WINDOW_HOURS: i64 = 24;
/// The assignee standing for the author's own GitHub account, as in `gh`.
pub const ASSIGN_SELF: &str = "@me";

/// The hidden line ending a description G5 sent. Equal for every retry of
/// one submission (same organization, author and key) and different across
/// submissions.
#[must_use]
pub fn creation_marker(org_id: Uuid, author_user_id: Uuid, idempotency_key: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(org_id.as_bytes());
    digest.update(author_user_id.as_bytes());
    digest.update(idempotency_key.as_bytes());
    let token = hex::encode(&digest.finalize()[..16]);
    format!("<!-- g5-issue {token} -->")
}

/// The exact description sent to GitHub: the person's text and the marker.
#[must_use]
pub fn render_payload(body: &str, marker: &str) -> String {
    let body = body.trim_end();
    if body.trim().is_empty() {
        marker.to_string()
    } else {
        format!("{body}\n\n{marker}")
    }
}

/// Whether `issue` is the one a submission with `marker` created.
#[must_use]
pub fn carries_marker(issue: &GithubIssue, marker: &str) -> bool {
    issue
        .body
        .as_deref()
        .is_some_and(|body| body.trim_end().ends_with(marker))
}

static LOGIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9][A-Za-z0-9-]{0,38}$").expect("login pattern compiles")
});

fn invalid(field: &str, error: &str) -> G5Error {
    G5Error::ValidationError {
        details: json!({"field": field, "error": error}),
    }
}

/// Refuse `text` when the comment secret screen matches it; matched values
/// never reach the error.
pub fn screen(field: &str, text: &str, action: &str) -> Result<(), G5Error> {
    let kinds = secret_kinds(text);
    if kinds.is_empty() {
        return Ok(());
    }
    Err(G5Error::ValidationError {
        details: json!({
            "field": field,
            "code": "secret_detected",
            "error": format!("the {field} appears to contain a secret; remove it before {action}"),
            "kinds": kinds,
        }),
    })
}

/// A submission's fields after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueDraft {
    pub title: String,
    pub body: String,
    /// GitHub logins, deduplicated; may include [`ASSIGN_SELF`].
    pub assignees: Vec<String>,
}

/// Validate a submission before anything is sent: a one-line title, a
/// bounded description, and assignee logins. Title and description pass the
/// comment secret screen; matched values never reach the error.
pub fn validate_draft(
    title: &str,
    body: &str,
    assignees: &[String],
) -> Result<IssueDraft, G5Error> {
    const ACTION: &str = "creating the issue";
    let title = checked_title(title, ACTION)?;
    let body = checked_body(body, ACTION)?;
    let assignees = checked_logins(assignees)?;
    if assignees.len() > MAX_ASSIGNEES {
        return Err(invalid(
            "assignees",
            &format!("an issue can have at most {MAX_ASSIGNEES} assignees"),
        ));
    }
    Ok(IssueDraft {
        title,
        body,
        assignees,
    })
}

/// An edit of a GitHub issue after validation (`g5.external-issues.edit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueEditDraft {
    pub title: Option<String>,
    pub body: Option<String>,
    /// GitHub logins to add and to remove, deduplicated; may include
    /// [`ASSIGN_SELF`].
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

/// Validate an edit before anything is sent, under the creation rules for
/// each named field. An edit naming no change, or adding and removing the
/// same login, is refused.
pub fn validate_edit(
    title: Option<&str>,
    body: Option<&str>,
    add: &[String],
    remove: &[String],
) -> Result<IssueEditDraft, G5Error> {
    const ACTION: &str = "editing the issue";
    let draft = IssueEditDraft {
        title: title
            .map(|title| checked_title(title, ACTION))
            .transpose()?,
        body: body.map(|body| checked_body(body, ACTION)).transpose()?,
        add: checked_logins(add)?,
        remove: checked_logins(remove)?,
    };
    if draft.title.is_none()
        && draft.body.is_none()
        && draft.add.is_empty()
        && draft.remove.is_empty()
    {
        return Err(invalid(
            "title",
            "provide a title, description or assignee change",
        ));
    }
    if let Some(both) = draft.add.iter().find(|login| {
        draft
            .remove
            .iter()
            .any(|other| other.eq_ignore_ascii_case(login))
    }) {
        return Err(invalid(
            "assignees",
            &format!("{both} is both added and removed"),
        ));
    }
    Ok(draft)
}

fn checked_title(title: &str, action: &str) -> Result<String, G5Error> {
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
            &format!("the title is longer than {MAX_TITLE_CHARS} characters"),
        ));
    }
    screen("title", title, action)?;
    Ok(title.to_string())
}

fn checked_body(body: &str, action: &str) -> Result<String, G5Error> {
    if body.chars().count() > MAX_COMMENT_CHARS {
        return Err(invalid(
            "body",
            &format!("the description is longer than {MAX_COMMENT_CHARS} characters"),
        ));
    }
    screen("body", body, action)?;
    Ok(body.trim_end().to_string())
}

/// Assignee logins, trimmed and deduplicated without regard to case;
/// [`ASSIGN_SELF`] stands for the acting account.
fn checked_logins(assignees: &[String]) -> Result<Vec<String>, G5Error> {
    let mut unique: Vec<String> = Vec::new();
    for assignee in assignees {
        let assignee = assignee.trim();
        if assignee.is_empty() {
            continue;
        }
        let valid = assignee.eq_ignore_ascii_case(ASSIGN_SELF) || LOGIN.is_match(assignee);
        if !valid {
            return Err(invalid(
                "assignees",
                &format!("{assignee:?} is not a GitHub login"),
            ));
        }
        if !unique
            .iter()
            .any(|seen| seen.eq_ignore_ascii_case(assignee))
        {
            unique.push(if assignee.eq_ignore_ascii_case(ASSIGN_SELF) {
                ASSIGN_SELF.to_string()
            } else {
                assignee.to_string()
            });
        }
    }
    Ok(unique)
}

/// The G5 creation marker line ending `description`, if any.
fn creation_marker_line(description: &str) -> Option<&str> {
    let line = description.trim_end().lines().last()?.trim();
    (line.starts_with("<!-- g5-issue ") && line.ends_with(" -->")).then_some(line)
}

/// `description` as its author wrote it: without a G5 creation marker.
#[must_use]
pub fn description_without_marker(description: &str) -> &str {
    let trimmed = description.trim_end();
    match creation_marker_line(trimmed) {
        Some(marker) => trimmed.strip_suffix(marker).map_or(trimmed, str::trim_end),
        None => trimmed,
    }
}

/// A new description for an issue whose current description is `current`:
/// a G5 creation marker ending `current` is kept, so a retried creation
/// still finds the issue (`g5.external-issues.creation.recovery`).
#[must_use]
pub fn edited_description(current: Option<&str>, body: &str) -> String {
    let marker = current.and_then(creation_marker_line);
    match marker {
        Some(marker) if !body.trim_end().ends_with(marker) => render_payload(body, marker),
        _ => body.trim_end().to_string(),
    }
}

/// The logins sent to GitHub: [`ASSIGN_SELF`] becomes the author's login.
#[must_use]
pub fn resolve_assignees(assignees: &[String], author_login: &str) -> Vec<String> {
    let mut resolved: Vec<String> = Vec::new();
    for assignee in assignees {
        let login = if assignee == ASSIGN_SELF {
            author_login
        } else {
            assignee.as_str()
        };
        if !resolved.iter().any(|seen| seen.eq_ignore_ascii_case(login)) {
            resolved.push(login.to_string());
        }
    }
    resolved
}

/// An issue reference after trimming, or `None` when it is empty. A bare
/// number (`12` or `#12`, as `gh` takes) names an issue in the creation
/// target `target_key` (`owner/name`).
#[must_use]
pub fn target_reference(raw: Option<&str>, target_key: &str) -> Option<String> {
    let raw = raw.map(str::trim).filter(|raw| !raw.is_empty())?;
    let bare = raw.strip_prefix('#').unwrap_or(raw);
    Some(
        if !bare.is_empty() && bare.bytes().all(|b| b.is_ascii_digit()) {
            format!("{target_key}#{bare}")
        } else {
            raw.to_string()
        },
    )
}

/// [`target_reference`] for a creation domain: a bare number names
/// `KEY-12` in a native domain, `owner/name#12` in a GitHub one.
#[must_use]
pub fn target_reference_for(
    raw: Option<&str>,
    domain: &crate::services::git::external_issues::IssueDomainRecord,
) -> Option<String> {
    if !domain.is_native() {
        return target_reference(raw, &domain.display_key);
    }
    let raw = raw.map(str::trim).filter(|raw| !raw.is_empty())?;
    let bare = raw.strip_prefix('#').unwrap_or(raw);
    Some(
        if !bare.is_empty() && bare.bytes().all(|b| b.is_ascii_digit()) {
            format!("{}-{bare}", domain.display_key)
        } else {
            raw.to_string()
        },
    )
}

/// The reference an agent names an issue to link with (`g5d issue link`,
/// `g5d workspace fork --issue`): a bare number names an issue in the
/// organization's creation domain, as `gh` does in the current repository;
/// anything else is passed on as given.
#[must_use]
pub fn creation_reference(
    domains: &[crate::services::git::external_issues::IssueDomainAccess],
    raw: &str,
) -> String {
    domains
        .iter()
        .find(|access| access.domain.is_default_create_target)
        .and_then(|target| target_reference_for(Some(raw), &target.domain))
        .unwrap_or_else(|| raw.to_string())
}

/// Check an idempotency key's shape (`max` characters, no surrounding
/// whitespace).
pub fn validate_key(key: &str, max: usize) -> Result<(), G5Error> {
    if key.is_empty() || key.trim() != key || key.len() > max {
        return Err(invalid(
            "idempotencyKey",
            &format!("must be 1-{max} characters without surrounding whitespace"),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Where a new issue goes: the organization's default creation target.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationDestinationDTO {
    pub domain_id: Uuid,
    pub tracker_id: Uuid,
    /// `github` or `g5`.
    pub provider: String,
    /// GitHub numeric repository id, or `g5` for G5 issues.
    pub repository_id: String,
    /// `owner/name`, or the G5 issue key.
    pub display_key: String,
    pub content_trusted: bool,
}

/// The issue a creation produced or found.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedIssueDTO {
    pub external_issue_id: String,
    pub repository_id: String,
    pub number: u64,
    /// `owner/name#number`, the reference Start workspace accepts.
    pub display_key: String,
    pub title: String,
    pub url: String,
}

impl CreatedIssueDTO {
    #[must_use]
    pub fn new(destination: &CreationDestinationDTO, issue: &GithubIssue) -> Self {
        let repository = issue
            .repository_full_name
            .clone()
            .unwrap_or_else(|| destination.display_key.clone());
        Self {
            external_issue_id: issue.id.clone(),
            repository_id: destination.repository_id.clone(),
            number: issue.number,
            display_key: format!("{repository}#{}", issue.number),
            title: issue.title.clone(),
            url: issue.html_url.clone(),
        }
    }
}

impl CreatedIssueDTO {
    /// A native issue (`g5.issue.native.writes`).
    #[must_use]
    pub fn from_summary(
        summary: &crate::services::git::external_issues::ExternalIssueSummaryDTO,
    ) -> Self {
        Self {
            external_issue_id: summary.external_issue_id.clone(),
            repository_id: summary.locator.repository_id.clone(),
            number: summary.locator.number,
            display_key: summary.display_key.clone(),
            title: summary.title.clone(),
            url: summary.canonical_url.clone(),
        }
    }
}

/// The issue a submission asked to create its issue under: a GitHub
/// sub-issue relationship in the same repository.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationParentDTO {
    pub external_issue_id: String,
    pub number: u64,
    /// `owner/name#number`.
    pub display_key: String,
    pub title: String,
    pub url: String,
    /// `pending` (no issue yet), `attached`, `not_attached` (a check found
    /// it without a parent, or GitHub refused: see `message`),
    /// `outcome_unknown` (no definite answer; a retry with the key checks
    /// before attaching) or `other_parent` (already a sub-issue of another
    /// issue, which G5 never moves).
    pub state: &'static str,
    pub message: Option<String>,
}

impl CreationParentDTO {
    #[must_use]
    pub fn new(destination: &CreationDestinationDTO, issue: &GithubIssue) -> Self {
        let created = CreatedIssueDTO::new(destination, issue);
        Self {
            external_issue_id: created.external_issue_id,
            number: created.number,
            display_key: created.display_key,
            title: created.title,
            url: created.url,
            state: "pending",
            message: None,
        }
    }
}

impl CreationParentDTO {
    /// A native parent, set in the creating transaction.
    #[must_use]
    pub fn from_summary(
        summary: &crate::services::git::external_issues::ExternalIssueSummaryDTO,
        state: &'static str,
    ) -> Self {
        let parent = CreatedIssueDTO::from_summary(summary);
        Self {
            external_issue_id: parent.external_issue_id,
            number: parent.number,
            display_key: parent.display_key,
            title: parent.title,
            url: parent.url,
            state,
            message: None,
        }
    }
}

/// What a create or check request established.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCreationDTO {
    /// `created` (the issue exists), `outcome_unknown` (GitHub gave no
    /// definite answer; check before creating again) or `not_found` (a check
    /// found no issue with this submission's marker).
    pub state: &'static str,
    /// Whether this request sent the issue to GitHub; `false` when an earlier
    /// attempt's issue was found by its marker, or for a check.
    pub sent: bool,
    pub issue: Option<CreatedIssueDTO>,
    /// The key to retry or check with.
    pub idempotency_key: String,
    pub destination: CreationDestinationDTO,
    /// The GitHub account the issue is created as; `None` for G5 issues,
    /// which are created as the author's G5 account.
    pub github_login: Option<String>,
    pub title: String,
    /// The exact description sent, including the marker (G5 issues: the
    /// description).
    pub payload: String,
    /// The logins sent, with `@me` resolved (G5 issues: member emails).
    pub assignees: Vec<String>,
    /// The labels sent, named as the destination's catalog has them (G5
    /// issues: the labels the issue carries).
    pub labels: Vec<String>,
    /// Labels GitHub's answer lacks: it silently drops labels the author may
    /// not set. The issue stays; add them with an explicit change.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels_not_applied: Vec<String>,
    /// Why the outcome is unknown.
    pub message: Option<String>,
    /// The requested parent and whether the issue is under it; `None` when
    /// none was requested. The issue's `state` stays `created` when only
    /// the parent is missing.
    pub parent: Option<CreationParentDTO>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_identify_one_submission() {
        let (org, author) = (Uuid::nil(), Uuid::from_u128(1));
        let marker = creation_marker(org, author, "key-1");
        assert!(marker.starts_with("<!-- g5-issue ") && marker.ends_with(" -->"));
        assert_eq!(marker, creation_marker(org, author, "key-1"));
        assert_ne!(marker, creation_marker(org, author, "key-2"));
        assert_ne!(marker, creation_marker(org, Uuid::from_u128(2), "key-1"));
        assert_ne!(marker, creation_marker(Uuid::from_u128(3), author, "key-1"));
        assert_eq!(
            render_payload("Steps to reproduce\n\n", &marker),
            format!("Steps to reproduce\n\n{marker}")
        );
        assert_eq!(render_payload("  \n", &marker), marker);
    }

    #[test]
    fn drafts_are_bounded_one_line_and_screened() {
        let ok = validate_draft("  Fix login  ", "Body\n", &[]).unwrap();
        assert_eq!((ok.title.as_str(), ok.body.as_str()), ("Fix login", "Body"));
        assert!(validate_draft(" ", "", &[]).is_err());
        assert!(validate_draft("two\nlines", "", &[]).is_err());
        assert!(validate_draft(&"x".repeat(MAX_TITLE_CHARS + 1), "", &[]).is_err());
        assert!(validate_draft("t", &"x".repeat(MAX_COMMENT_CHARS + 1), &[]).is_err());
        assert!(
            validate_draft("t", "", &[]).is_ok(),
            "the description is optional"
        );
        let token = format!("ghp_{}", "a".repeat(36));
        for (title, body, field) in [
            (format!("leak {token}"), String::new(), "title"),
            ("t".to_string(), format!("leak {token}"), "body"),
        ] {
            let G5Error::ValidationError { details } =
                validate_draft(&title, &body, &[]).unwrap_err()
            else {
                panic!("expected a validation error");
            };
            assert_eq!(details["code"], "secret_detected");
            assert_eq!(details["field"], field);
            assert!(!details.to_string().contains(&token), "{details}");
        }
    }

    #[test]
    fn assignees_are_logins_or_me_deduplicated_and_resolved() {
        let draft = validate_draft(
            "t",
            "",
            &[
                "octo".into(),
                "@ME".into(),
                " Octo ".into(),
                String::new(),
                "hub-ot".into(),
            ],
        )
        .unwrap();
        assert_eq!(draft.assignees, vec!["octo", "@me", "hub-ot"]);
        assert_eq!(
            resolve_assignees(&draft.assignees, "Octo"),
            vec!["octo", "hub-ot"]
        );
        assert!(validate_draft("t", "", &["not a login".into()]).is_err());
        assert!(validate_draft("t", "", &["-dash".into()]).is_err());
        let many: Vec<String> = (0..=MAX_ASSIGNEES).map(|n| format!("u{n}")).collect();
        assert!(validate_draft("t", "", &many).is_err());
    }

    #[test]
    fn bare_numbers_name_the_creation_target() {
        for raw in ["12", "#12", " #12 "] {
            assert_eq!(
                target_reference(Some(raw), "acme/web").as_deref(),
                Some("acme/web#12")
            );
        }
        assert_eq!(
            target_reference(Some("other/repo#3"), "acme/web").as_deref(),
            Some("other/repo#3")
        );
        assert_eq!(
            target_reference(Some("#"), "acme/web").as_deref(),
            Some("#")
        );
        assert_eq!(target_reference(Some("  "), "acme/web"), None);
        assert_eq!(target_reference(None, "acme/web"), None);
    }

    #[test]
    fn keys_are_bounded_and_trimmed() {
        assert!(validate_key("k", 200).is_ok());
        assert!(validate_key("", 200).is_err());
        assert!(validate_key(" k", 200).is_err());
        assert!(validate_key(&"k".repeat(201), 200).is_err());
    }

    #[test]
    fn edits_name_a_change_and_keep_the_creation_marker() {
        let logins = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>()
        };
        let edit =
            validate_edit(Some(" New "), None, &logins(&["octo", "OCTO", "@ME"]), &[]).unwrap();
        assert_eq!(edit.title.as_deref(), Some("New"));
        assert_eq!(edit.add, logins(&["octo", "@me"]));
        assert!(validate_edit(None, None, &[], &[]).is_err(), "no change");
        assert!(validate_edit(None, None, &logins(&["octo"]), &logins(&["Octo"])).is_err());
        assert!(validate_edit(None, None, &logins(&["not a login"]), &[]).is_err());
        assert!(validate_edit(Some("two\nlines"), None, &[], &[]).is_err());
        let token = format!("ghp_{}", "a".repeat(36));
        let Err(G5Error::ValidationError { details }) =
            validate_edit(None, Some(&format!("leak {token}")), &[], &[])
        else {
            panic!("a secret in the description is refused");
        };
        assert_eq!(details["code"], "secret_detected");
        assert!(!details.to_string().contains(&token));

        let marker = creation_marker(Uuid::nil(), Uuid::nil(), "k");
        let current = render_payload("Old", &marker);
        assert_eq!(
            edited_description(Some(&current), "New\n"),
            format!("New\n\n{marker}")
        );
        assert_eq!(edited_description(Some(&current), &current), current);
        assert_eq!(edited_description(Some("Plain"), "New"), "New");
        assert_eq!(edited_description(None, "New"), "New");
        assert_eq!(description_without_marker(&current), "Old");
        assert_eq!(description_without_marker("Plain\n"), "Plain");
    }
}
