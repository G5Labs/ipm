//! The workspace agent's email to the workspace's current owner
//! (`g5.workspace.owner-email`), requested through the shared email module.
//!
//! The recipient is never the caller's choice: it is the current owner's login
//! email, read in the same system transaction that proves the caller is the
//! workspace's live incarnation and records the request. A queued message stays wanted
//! only while that person still owns the workspace
//! (`g5.workspace.owner-email-validity`). A live workspace's owner is always
//! an eligible member of its organization (`workspaces_owner_same_org_fk`), so
//! membership needs no check of its own.

use g5_auth::WorkspaceIncarnationClaims;
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub use crate::services::email::BodyFormat;
use crate::services::email::{
    link_path, DeliveryGate, Email, EmailRequest, Message, RequestOutcome, Sender, Validity,
    WorkspaceOwnerEmail,
};
use crate::services::workspace_incarnation::{current_incarnation_on_system, IncarnationLock};

/// `G5 Agent <agent@{sending domain}>` (`g5.email.sender`).
const AGENT_SENDER: Sender = Sender::new("agent").with_display_name("G5 Agent");

/// Longest caller idempotency key.
pub const MAX_IDEMPOTENCY_KEY_CHARS: usize = 128;

/// One agent request, with the credentials it arrived with.
pub struct OwnerEmailRequest<'a> {
    pub claims: &'a WorkspaceIncarnationClaims,
    pub subject: &'a str,
    pub body: &'a str,
    pub format: BodyFormat,
    pub idempotency_key: &'a str,
}

/// What a request did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerEmailOutcome {
    /// A new message was queued for the owner.
    Queued(Uuid),
    /// The idempotency key was already used; nothing new was queued.
    Existing(Uuid),
}

/// Queue one message from the workspace's agent to its current owner, on
/// `db`, a system-pool transaction scoped to the claims' organization that the
/// caller commits. Refused unless the claims are the workspace's live
/// incarnation (checked here, with the workspace and VM rows held `FOR SHARE`
/// until commit), when email is disabled or a volume limit is reached
/// (`g5.workspace.owner-email-limit`), and when the email module refuses a
/// Markdown body (`g5.workspace.owner-email-format`).
pub async fn request_owner_email(
    email: &Email,
    db: &mut ScopedDb,
    request: OwnerEmailRequest<'_>,
) -> Result<OwnerEmailOutcome, G5Error> {
    let subject = validate_subject(request.subject)?;
    let body = validate_body(request.body)?;
    validate_idempotency_key(request.idempotency_key)?;
    let claims = request.claims;
    let live = current_incarnation_on_system(db.conn(), claims, IncarnationLock::Share)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check the workspace incarnation: {e}"),
        })?;
    if live.is_none() {
        return Err(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        });
    }
    if !email.is_enabled() {
        return Err(email_unavailable());
    }

    let context: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT w.name, o.name, o.slug, u.email
           FROM workspaces w
           JOIN orgs o ON o.id = w.org_id
           JOIN users u ON u.id = w.owner_user_id
          WHERE w.id = $1 AND w.org_id = $2 AND w.owner_user_id = $3
            AND w.deleted_at IS NULL",
    )
    .bind(claims.workspace_id)
    .bind(claims.org_id)
    .bind(claims.owner_user_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to load the workspace owner: {e}"),
    })?;
    let Some((workspace_name, org_name, org_slug, owner_email)) = context else {
        return Err(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        });
    };

    let link_scopes = match request.format {
        BodyFormat::Text => Vec::new(),
        BodyFormat::Markdown => organization_repository_scopes(db.conn(), claims.org_id).await?,
    };

    let outcome = email
        .request(
            db.conn(),
            EmailRequest {
                org_id: claims.org_id,
                sender: AGENT_SENDER,
                recipient: owner_email,
                message: Message::WorkspaceOwnerMessage(WorkspaceOwnerEmail {
                    workspace_id: claims.workspace_id,
                    request_key: request.idempotency_key.to_string(),
                    link_path: link_path(&[&org_slug, &workspace_name]),
                    workspace_name,
                    org_name,
                    subject: subject.to_string(),
                    body: body.to_string(),
                    format: request.format,
                    link_scopes,
                }),
            },
        )
        .await?;
    match outcome {
        RequestOutcome::Queued(id) => Ok(OwnerEmailOutcome::Queued(id)),
        RequestOutcome::Coalesced(id) | RequestOutcome::Existing(id) => {
            Ok(OwnerEmailOutcome::Existing(id))
        }
        RequestOutcome::Disabled => Err(email_unavailable()),
    }
}

/// The web roots of the organization's Git-provider repositories, which an
/// owner email's links may lead into (`g5.workspace.owner-email-format`).
/// Provider repositories are admitted with an `https` clone URL on the
/// provider's host, whose web root is that URL without `.git`; local
/// repositories have no web page.
/// The `https` web roots of the organization's provider repositories, the
/// scopes an author's Markdown links may lead into (`g5.email.link-policy`).
pub(crate) async fn organization_repository_scopes(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<Vec<String>, G5Error> {
    let urls: Vec<String> = sqlx::query_scalar(
        "SELECT git_url FROM repos WHERE org_id = $1 AND provider IS NOT NULL ORDER BY git_url",
    )
    .bind(org_id)
    .fetch_all(conn)
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to load the organization's repositories: {e}"),
    })?;
    Ok(urls.iter().filter_map(|url| web_root(url)).collect())
}

fn web_root(git_url: &str) -> Option<String> {
    let trimmed = git_url.trim().trim_end_matches('/');
    let root = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    root.starts_with("https://").then(|| root.to_string())
}

/// A definite refusal (409), so clients do not retry it as an outage.
fn email_unavailable() -> G5Error {
    G5Error::Conflict {
        reason: "Email is not enabled in this deployment".to_string(),
    }
}

fn invalid(field: &str, error: &str) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({ "field": field, "error": error }),
    }
}

fn validate_subject(subject: &str) -> Result<&str, G5Error> {
    let subject = subject.trim();
    if subject.is_empty() {
        return Err(invalid("subject", "must not be empty"));
    }
    if subject.contains(['\r', '\n']) {
        return Err(invalid("subject", "must be one line"));
    }
    if subject.chars().count() > WorkspaceOwnerEmail::MAX_SUBJECT_CHARS {
        return Err(invalid("subject", "must be at most 120 characters"));
    }
    Ok(subject)
}

fn validate_body(body: &str) -> Result<&str, G5Error> {
    if body.trim().is_empty() {
        return Err(invalid("body", "must not be empty"));
    }
    if body.chars().count() > WorkspaceOwnerEmail::MAX_BODY_CHARS {
        return Err(invalid("body", "must be at most 20000 characters"));
    }
    Ok(body)
}

fn validate_idempotency_key(key: &str) -> Result<(), G5Error> {
    let valid = !key.is_empty()
        && key.chars().count() <= MAX_IDEMPOTENCY_KEY_CHARS
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "idempotencyKey",
            "must be 1-128 letters, digits, '-', '_', '.' or ':'",
        ))
    }
}

/// A queued owner message is wanted only while its workspace exists and its
/// current owner's login email is the recipient
/// (`g5.workspace.owner-email-validity`).
pub struct WorkspaceOwnerEmailGate;

#[async_trait::async_trait]
impl DeliveryGate for WorkspaceOwnerEmailGate {
    async fn still_wanted(
        &self,
        pool: &PgPool,
        purpose_key: &str,
        recipient: &str,
    ) -> Result<Validity, sqlx::Error> {
        let Some(workspace_id) = purpose_key
            .split_once(':')
            .and_then(|(workspace, _)| Uuid::parse_str(workspace).ok())
        else {
            return Ok(Validity::Cancelled("invalid_purpose"));
        };
        // The worker's pool has no organization context, so the workspace is
        // read through the system authority lookup rather than the table.
        let row: Option<(bool, Option<String>)> = sqlx::query_as(
            "SELECT w.deleted_at IS NOT NULL, u.email
               FROM authz.lookup_workspace_runtime_authority($1) AS w
               LEFT JOIN users u ON u.id = w.owner_user_id",
        )
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?;
        Ok(match row {
            None | Some((true, _)) => Validity::Cancelled("workspace_deleted"),
            Some((_, owner_email)) if owner_email.as_deref().map(str::trim) != Some(recipient) => {
                Validity::Cancelled("owner_changed")
            }
            Some(_) => Validity::Wanted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_web_roots_drop_the_git_suffix_and_skip_other_schemes() {
        assert_eq!(
            web_root("https://github.com/acme/api.git").as_deref(),
            Some("https://github.com/acme/api")
        );
        assert_eq!(
            web_root("https://git.acme.test/ghe/acme/web/").as_deref(),
            Some("https://git.acme.test/ghe/acme/web")
        );
        for local in [
            "g5-local://repo",
            "/tmp/r.git",
            "git@github.com:acme/api.git",
        ] {
            assert_eq!(web_root(local), None, "{local}");
        }
    }

    #[test]
    fn subjects_bodies_and_keys_are_bounded() {
        assert_eq!(validate_subject("  Done  ").unwrap(), "Done");
        assert!(validate_subject(" ").is_err());
        assert!(validate_subject("a\nb").is_err());
        assert!(validate_subject(&"x".repeat(121)).is_err());
        assert!(validate_body("\n\t ").is_err());
        assert!(validate_body(&"x".repeat(20_000)).is_ok());
        assert!(validate_body(&"x".repeat(20_001)).is_err());
        assert!(validate_idempotency_key("g5d-0193:a.b_c").is_ok());
        for bad in ["", "a b", "a/b", &"k".repeat(129)] {
            assert!(validate_idempotency_key(bad).is_err(), "{bad:?}");
        }
    }
}
