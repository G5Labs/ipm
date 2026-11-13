//! Comments G5 posts to external trackers under a person's own identity.
//!
//! A comment is written in G5 and posted to GitHub as the person's account
//! (docs/external-issue-integration-design.md, "Comments from a workspace
//! reviewer" and "Publishing and destination changes"). Its payload ends
//! with a hidden marker. An issue comment's marker is derived from the
//! organization, author and idempotency key (`g5.issue.comment-recovery`),
//! so a retry finds an earlier send on the issue instead of posting again,
//! and nothing is recorded in G5. Pull request comments still record each
//! submission, naming the record in the marker (G5-Lean#950).

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::services::git::external_issues::IssueErrorDTO;
use crate::services::git::github_issues::IssueError;

/// Longest comment body accepted (GitHub's limit is 65,536 characters,
/// including the marker).
pub const MAX_COMMENT_CHARS: usize = 60_000;

fn db_error(context: &str, error: &sqlx::Error) -> G5Error {
    G5Error::Internal {
        message: format!("issue comment publication {context}: {error}"),
    }
}

// ---------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------

/// The hidden line G5 appends to every comment it posts.
#[must_use]
pub fn marker(publication_id: Uuid) -> String {
    format!("<!-- g5-comment {publication_id} -->")
}

/// How far back an issue comment's marker is looked for: as long as the
/// comment composer keeps a draft and its key (`DRAFT_RETENTION_MS` in
/// `ActivityCommentComposer.tsx`), so a retry G5 offers as safe finds an
/// earlier send.
pub const COMMENT_MARKER_WINDOW_DAYS: i64 = 14;

/// An issue comment's marker id: equal for every retry of one submission
/// (same organization, author, origin and key) and different across
/// submissions, in the UUID shape every G5 comment marker has. The origin
/// keeps a person's key from naming their agent's comment.
#[must_use]
pub fn keyed_marker_id(
    org_id: Uuid,
    author_user_id: Uuid,
    origin: &str,
    idempotency_key: &str,
) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"g5-comment");
    digest.update(org_id.as_bytes());
    digest.update(author_user_id.as_bytes());
    digest.update(origin.as_bytes());
    digest.update([0]);
    digest.update(idempotency_key.as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest.finalize()[..16]);
    Uuid::from_bytes(id)
}

/// The exact text sent to GitHub: the person's comment and the marker.
#[must_use]
pub fn render_payload(body: &str, publication_id: Uuid) -> String {
    format!("{}\n\n{}", body.trim_end(), marker(publication_id))
}

static MARKER_AT_END: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*(<!-- g5-comment [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12} -->)\s*\z")
        .expect("marker pattern compiles")
});

/// The G5 reference line ending a comment G5 posted, if any.
#[must_use]
pub fn existing_marker(body: &str) -> Option<String> {
    MARKER_AT_END
        .captures(body)
        .and_then(|captures| captures.get(1))
        .map(|marker| marker.as_str().to_string())
}

/// The replacement text for an edit: the new body, keeping the reference
/// line of a comment G5 posted (an edit replaces the whole text).
#[must_use]
pub fn render_edit_payload(body: &str, marker: Option<&str>) -> String {
    match marker {
        Some(marker) => format!("{}\n\n{marker}", body.trim_end()),
        None => body.trim_end().to_string(),
    }
}

/// The person's comment within a payload, without the marker.
#[must_use]
pub fn body_of(payload: &str, publication_id: Uuid) -> String {
    payload
        .strip_suffix(&marker(publication_id))
        .map_or(payload, str::trim_end)
        .to_string()
}

struct SecretPattern {
    kind: &'static str,
    regex: Regex,
}

static SECRET_PATTERNS: LazyLock<Vec<SecretPattern>> = LazyLock::new(|| {
    [
        (
            "GitHub token",
            r"\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,})",
        ),
        ("AWS access key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        ("private key", r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----"),
        ("Slack token", r"\bxox[abposr]-[A-Za-z0-9-]{10,}"),
        ("Google API key", r"\bAIza[0-9A-Za-z_\-]{35}"),
        ("API secret key", r"\bsk-(?:ant-|proj-)?[A-Za-z0-9_\-]{24,}"),
    ]
    .into_iter()
    .map(|(kind, pattern)| SecretPattern {
        kind,
        regex: Regex::new(pattern).expect("secret pattern compiles"),
    })
    .collect()
});

/// Kinds of known secret patterns found in `text`. The matched values are
/// never returned, so they cannot reach errors or logs.
#[must_use]
pub fn secret_kinds(text: &str) -> Vec<&'static str> {
    SECRET_PATTERNS
        .iter()
        .filter(|pattern| pattern.regex.is_match(text))
        .map(|pattern| pattern.kind)
        .collect()
}

/// Validate a comment body before anything is recorded or sent.
pub fn validate_body(body: &str) -> Result<(), G5Error> {
    if body.trim().is_empty() {
        return Err(G5Error::ValidationError {
            details: json!({"field": "body", "error": "the comment is empty"}),
        });
    }
    if body.chars().count() > MAX_COMMENT_CHARS {
        return Err(G5Error::ValidationError {
            details: json!({
                "field": "body",
                "error": format!("the comment is longer than {MAX_COMMENT_CHARS} characters"),
            }),
        });
    }
    let kinds = secret_kinds(body);
    if !kinds.is_empty() {
        return Err(G5Error::ValidationError {
            details: json!({
                "field": "body",
                "code": "secret_detected",
                "error": "the comment appears to contain a secret; remove it before posting",
                "kinds": kinds,
            }),
        });
    }
    Ok(())
}

/// A 409 whose `details.code` names why a comment cannot be posted.
#[must_use]
pub fn comment_conflict(code: &str, reason: &str) -> G5Error {
    G5Error::ConflictDetails {
        reason: reason.to_string(),
        details: json!({ "code": code }),
    }
}

// ---------------------------------------------------------------------------
// Publication records
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PublicationRecord {
    pub id: Uuid,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    /// The issue domain and tracker; `None` for a pull request destination.
    pub domain_id: Option<Uuid>,
    pub tracker_id: Option<Uuid>,
    /// The issue, or for a pull request destination the provider PR's ID.
    pub external_issue_id: String,
    pub display_key: String,
    pub locator: Value,
    pub author_user_id: Uuid,
    pub github_login: String,
    pub payload: String,
    pub state: String,
    /// `comment` or `edit`.
    pub kind: String,
    pub target_comment_id: Option<String>,
    /// `person`, or `agent` for the workspace's agent posting as its owner.
    pub origin: String,
    /// `issue`, `pull_request` (a PR's discussion) or `review_thread`.
    pub destination: String,
    /// The review thread (node ID) a reply is posted in.
    pub target_thread_id: Option<String>,
    pub external_comment_id: Option<String>,
    pub external_comment_url: Option<String>,
    pub error: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

const PUBLICATION_COLUMNS: &str = "id, org_id, workspace_id, domain_id, tracker_id, \
     external_issue_id, display_key, locator, author_user_id, github_login, payload, state, \
     kind, target_comment_id, origin, destination, target_thread_id, external_comment_id, \
     external_comment_url, error, created_at, published_at";

/// A submitted comment as its author sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentPublicationDTO {
    pub id: Uuid,
    /// `pending`, `published`, `failed`, `outcome_unknown` or `discarded`.
    pub state: String,
    /// `comment` (posted) or `edit` (replaced an existing comment's text).
    pub kind: String,
    /// `person`, or `agent` when the workspace's agent posted it as its owner.
    pub origin: String,
    /// `issue`, `pull_request` or `review_thread`.
    pub destination: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub display_key: String,
    pub github_login: String,
    /// The comment as written, without the G5 marker.
    pub body: String,
    /// The exact text sent to GitHub.
    pub payload: String,
    pub external_comment_url: Option<String>,
    pub error: Option<IssueErrorDTO>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

impl PublicationRecord {
    #[must_use]
    pub fn to_dto(&self) -> CommentPublicationDTO {
        let error = self.error.as_ref().map(|error| IssueErrorDTO {
            issue_error: match error.get("issueError").and_then(Value::as_str) {
                Some("access_denied") => "access_denied",
                Some("not_found") => "not_found",
                Some("invalid_reference") => "invalid_reference",
                Some("rate_limited") => "rate_limited",
                _ => "unavailable",
            },
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
        CommentPublicationDTO {
            id: self.id,
            state: self.state.clone(),
            kind: self.kind.clone(),
            origin: self.origin.clone(),
            destination: self.destination.clone(),
            thread_id: self.target_thread_id.clone(),
            display_key: self.display_key.clone(),
            github_login: self.github_login.clone(),
            body: match existing_marker(&self.payload) {
                Some(marker) => self
                    .payload
                    .strip_suffix(&marker)
                    .map_or(self.payload.as_str(), str::trim_end)
                    .to_string(),
                None => body_of(&self.payload, self.id),
            },
            payload: self.payload.clone(),
            external_comment_url: self.external_comment_url.clone(),
            error,
            created_at: self.created_at,
            published_at: self.published_at,
        }
    }

    /// Whether this record is the submission of `body`: a replay of an
    /// idempotency key must carry the same text.
    #[must_use]
    pub fn is_same_comment(&self, body: &str) -> bool {
        self.to_dto().body == body.trim_end()
    }

    /// `(repositoryId, number)` from the frozen locator.
    #[must_use]
    pub fn github_locator(&self) -> Option<(u64, u64)> {
        let repository_id = self
            .locator
            .get("repositoryId")
            .and_then(Value::as_str)
            .and_then(crate::services::git::github_issues::parse_canonical_github_id)?;
        let number = self.locator.get("number").and_then(Value::as_u64)?;
        Some((repository_id, number))
    }
}

/// A submission to record before sending.
#[derive(Debug, Clone)]
pub struct NewPublication<'a> {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// `issue` (with its domain and tracker), `pull_request` or `review_thread`.
    pub destination: &'static str,
    pub domain_id: Option<Uuid>,
    pub tracker_id: Option<Uuid>,
    pub external_issue_id: &'a str,
    pub display_key: &'a str,
    pub locator: &'a Value,
    pub author_user_id: Uuid,
    pub github_login: &'a str,
    pub idempotency_key: &'a str,
    pub payload: &'a str,
    /// The comment an edit replaces; `None` for a new comment.
    pub target_comment_id: Option<&'a str>,
    /// `person`, or `agent` for the workspace's agent posting as its owner.
    pub origin: &'static str,
    /// The review thread a reply is posted in.
    pub target_thread_id: Option<&'a str>,
}

/// An earlier submission with the same author and idempotency key.
pub async fn find_by_key(
    db: &mut ScopedDb,
    org_id: Uuid,
    author_user_id: Uuid,
    idempotency_key: &str,
) -> Result<Option<PublicationRecord>, G5Error> {
    sqlx::query_as(&format!(
        "SELECT {PUBLICATION_COLUMNS} FROM issue_comment_publications \
         WHERE org_id = $1 AND author_user_id = $2 AND idempotency_key = $3"
    ))
    .bind(org_id)
    .bind(author_user_id)
    .bind(idempotency_key)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("key lookup", &error))
}

/// Record a submission as `pending`. A concurrent submission with the same
/// key wins; its record is returned instead (`false`).
pub async fn insert_pending(
    db: &mut ScopedDb,
    org_id: Uuid,
    publication: &NewPublication<'_>,
) -> Result<(PublicationRecord, bool), G5Error> {
    let inserted = sqlx::query_as::<_, PublicationRecord>(&format!(
        r#"
        INSERT INTO issue_comment_publications
            (id, workspace_id, domain_id, tracker_id, external_issue_id, display_key, locator,
             author_user_id, github_login, idempotency_key, payload, kind, target_comment_id,
             origin, destination, target_thread_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                CASE WHEN $12::text IS NULL THEN 'comment' ELSE 'edit' END, $12, $13, $14, $15)
        ON CONFLICT (org_id, author_user_id, idempotency_key) DO NOTHING
        RETURNING {PUBLICATION_COLUMNS}
        "#
    ))
    .bind(publication.id)
    .bind(publication.workspace_id)
    .bind(publication.domain_id)
    .bind(publication.tracker_id)
    .bind(publication.external_issue_id)
    .bind(publication.display_key)
    .bind(publication.locator)
    .bind(publication.author_user_id)
    .bind(publication.github_login)
    .bind(publication.idempotency_key)
    .bind(publication.payload)
    .bind(publication.target_comment_id)
    .bind(publication.origin)
    .bind(publication.destination)
    .bind(publication.target_thread_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("insert", &error))?;
    match inserted {
        Some(record) => Ok((record, true)),
        None => find_by_key(
            db,
            org_id,
            publication.author_user_id,
            publication.idempotency_key,
        )
        .await?
        .map(|record| (record, false))
        .ok_or_else(|| G5Error::Internal {
            message: "issue comment publication vanished after a key conflict".to_string(),
        }),
    }
}

/// One of the author's submissions on a workspace's issues.
pub async fn load_own(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    author_user_id: Uuid,
    publication_id: Uuid,
) -> Result<PublicationRecord, G5Error> {
    sqlx::query_as::<_, PublicationRecord>(&format!(
        "SELECT {PUBLICATION_COLUMNS} FROM issue_comment_publications \
         WHERE id = $1 AND workspace_id = $2 AND author_user_id = $3 \
           AND destination = 'issue'"
    ))
    .bind(publication_id)
    .bind(workspace_id)
    .bind(author_user_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("load", &error))?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("comment publication {publication_id}"),
    })
}

/// The author's issue comments in a workspace that are not on GitHub yet.
pub async fn open_for_author(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    author_user_id: Uuid,
) -> Result<Vec<PublicationRecord>, G5Error> {
    sqlx::query_as(&format!(
        "SELECT {PUBLICATION_COLUMNS} FROM issue_comment_publications \
         WHERE workspace_id = $1 AND author_user_id = $2 AND kind = 'comment' \
           AND destination = 'issue' AND state IN ('pending', 'failed', 'outcome_unknown') \
         ORDER BY created_at, id"
    ))
    .bind(workspace_id)
    .bind(author_user_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("open list", &error))
}

/// Resolution of a send or a reconciliation.
#[derive(Debug, Clone)]
pub enum PublicationUpdate {
    Published {
        comment_id: String,
        comment_url: String,
    },
    Failed(IssueError),
    OutcomeUnknown(String),
    Discarded,
}

/// Apply an update unless the record is already published (a published
/// comment is final).
pub async fn update(
    db: &mut ScopedDb,
    publication_id: Uuid,
    update: &PublicationUpdate,
) -> Result<PublicationRecord, G5Error> {
    let (state, comment_id, comment_url, error) = match update {
        PublicationUpdate::Published {
            comment_id,
            comment_url,
        } => (
            "published",
            Some(comment_id.as_str()),
            Some(comment_url.as_str()),
            None,
        ),
        PublicationUpdate::Failed(error) => (
            "failed",
            None,
            None,
            Some(json!({"issueError": error.kind.as_str(), "message": error.message})),
        ),
        PublicationUpdate::OutcomeUnknown(message) => (
            "outcome_unknown",
            None,
            None,
            Some(json!({"issueError": "unavailable", "message": message})),
        ),
        PublicationUpdate::Discarded => ("discarded", None, None, None),
    };
    sqlx::query_as::<_, PublicationRecord>(&format!(
        r#"
        UPDATE issue_comment_publications
        SET state = $2, external_comment_id = $3, external_comment_url = $4, error = $5,
            published_at = CASE WHEN $2 = 'published' THEN now() ELSE NULL END,
            updated_at = now()
        WHERE id = $1 AND state <> 'published'
        RETURNING {PUBLICATION_COLUMNS}
        "#
    ))
    .bind(publication_id)
    .bind(state)
    .bind(comment_id)
    .bind(comment_url)
    .bind(error)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("update", &error))?
    .ok_or_else(|| comment_conflict("already_published", "the comment is already on GitHub"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_markers_name_one_submission_and_keep_the_marker_shape() {
        let (org, author) = (Uuid::nil(), Uuid::from_u128(1));
        let id = keyed_marker_id(org, author, "person", "k-1");
        assert_eq!(id, keyed_marker_id(org, author, "person", "k-1"));
        for other in [
            keyed_marker_id(org, author, "person", "k-2"),
            keyed_marker_id(org, author, "agent", "k-1"),
            keyed_marker_id(org, Uuid::from_u128(2), "person", "k-1"),
            keyed_marker_id(Uuid::from_u128(3), author, "person", "k-1"),
        ] {
            assert_ne!(id, other);
        }
        let payload = render_payload("Hello", id);
        assert_eq!(existing_marker(&payload), Some(marker(id)));
    }

    #[test]
    fn payload_carries_the_marker_and_round_trips_the_body() {
        let id = Uuid::nil();
        let payload = render_payload("Looks right.\n\n", id);
        assert_eq!(
            payload,
            "Looks right.\n\n<!-- g5-comment 00000000-0000-0000-0000-000000000000 -->"
        );
        assert_eq!(body_of(&payload, id), "Looks right.");
        assert!(payload.contains(&marker(id)));
    }

    #[test]
    fn edits_keep_the_reference_line_of_comments_g5_posted() {
        let id = Uuid::parse_str("cbc40edc-c5ed-4f83-ba43-85069252d6b6").unwrap();
        let posted = render_payload("Old text", id);
        let marker = existing_marker(&posted).expect("G5 comment");
        assert_eq!(marker, super::marker(id));
        assert_eq!(
            render_edit_payload("New text\n", Some(&marker)),
            format!("New text\n\n{marker}")
        );
        // A comment written on GitHub has no reference line to keep.
        assert_eq!(existing_marker("Written on GitHub"), None);
        assert_eq!(
            existing_marker("quote <!-- g5-comment x --> mid-text"),
            None
        );
        assert_eq!(render_edit_payload("New text", None), "New text");
    }

    #[test]
    fn secrets_are_detected_by_kind_only() {
        assert_eq!(secret_kinds("all clear, see #12"), Vec::<&str>::new());
        let token = format!("ghp_{}", "a".repeat(36));
        assert_eq!(secret_kinds(&format!("use {token}")), vec!["GitHub token"]);
        assert_eq!(
            secret_kinds("key AKIAABCDEFGHIJKLMNOP"),
            vec!["AWS access key"]
        );
        assert_eq!(
            secret_kinds("-----BEGIN RSA PRIVATE KEY-----\nabc"),
            vec!["private key"]
        );
        let error = validate_body(&format!("oops {token}")).unwrap_err();
        let G5Error::ValidationError { details } = error else {
            panic!("expected a validation error");
        };
        assert_eq!(details["code"], "secret_detected");
        assert!(!details.to_string().contains(&token), "{details}");
    }

    #[test]
    fn bodies_must_be_present_and_bounded() {
        assert!(validate_body("  \n ").is_err());
        assert!(validate_body(&"x".repeat(MAX_COMMENT_CHARS + 1)).is_err());
        assert!(validate_body("Looks right.").is_ok());
    }
}
