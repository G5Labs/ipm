//! Provider steps of PR conversation writes (g5.governance.pr.conversation):
//! the attached repository and PR a write names, and a client acting as the
//! acting person's GitHub identity. Callers authorize the workspace, record
//! submissions and hold no transaction across provider requests.
use super::read::{
    attached_repository, invalid, readable_repositories, select_repository, selection,
    RepositoryUse, LOCAL_CONVERSATION_REFUSAL,
};
use super::{token_from_auth, WorkspacePullRequestProviderLifecycleService};
use crate::auth::RepositoryOperation;
use crate::errors::AppError;
use crate::services::github_issues::IssueError;
use crate::services::issue_comments::{
    find_by_key, insert_pending, marker, render_payload, update as update_publication,
    NewPublication, PublicationRecord, PublicationUpdate,
};
use crate::services::AppState;
use g5_authz::ScopedDb;
use g5_contracts::G5Error;
use g5_scm::providers::{ProviderError, RepositoryCapability, RepositoryClient};
use g5_scm::FailureEffect;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

/// The attached repository, and PR when one was selected, a write names.
#[derive(Debug, Clone)]
pub struct ConversationTarget {
    pub number: Option<i64>,
    pub org_id: Uuid,
    pub repo_id: Uuid,
    pub git_url: String,
    /// `owner/name`.
    pub repository: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewThreadResolutionRequest {
    /// The review thread's node ID, as `g5d pr view -c` reports it.
    pub thread: String,
    /// Optionally, the GitHub PR number or URL the thread must belong to.
    pub selector: Option<String>,
    pub repo: Option<String>,
    pub resolved: bool,
}

/// A GitHub review thread node ID.
pub fn valid_thread_id(id: &str) -> bool {
    (1..=200).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'=' | b'-'))
}

impl WorkspacePullRequestProviderLifecycleService {
    /// Resolve the read selectors to one of the workspace's attached GitHub
    /// repositories, and the selected PR number when a selector was given.
    pub async fn conversation_target(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        selector: Option<&str>,
        repo: Option<&str>,
    ) -> Result<ConversationTarget, G5Error> {
        let endpoints = self.provider_endpoints()?;
        // A G5 PR or local repository pull request carries no conversation
        // (`g5.git.pr-local-conversation`): refuse before any effect.
        let g5_name = |s: &str| {
            crate::services::git::pull_request_adapters::parse_g5_pull_request_name(s).is_some()
                || Uuid::parse_str(s).is_ok()
        };
        if selector.is_some_and(g5_name) {
            return Err(invalid(LOCAL_CONVERSATION_REFUSAL));
        }
        if !selector.is_some_and(|selector| selector.contains("://")) {
            let members = readable_repositories(db, workspace_id, RepositoryUse::Write).await?;
            if let Ok((_, None)) = select_repository(members, repo, |url| {
                endpoints.repository_name_from_clone_url(url)
            }) {
                return Err(invalid(LOCAL_CONVERSATION_REFUSAL));
            }
        }
        let (number, selected_repo) = match selector {
            Some(selector) => {
                let (number, selected) = selection(&endpoints, selector, repo)?;
                (Some(number), selected.or_else(|| repo.map(str::to_owned)))
            }
            None => (None, repo.map(str::to_owned)),
        };
        let (org_id, repo_id, git_url, repository) = attached_repository(
            db,
            workspace_id,
            selected_repo.as_deref(),
            &endpoints,
            RepositoryUse::Write,
            None,
        )
        .await?;
        Ok(ConversationTarget {
            number,
            org_id,
            repo_id,
            git_url,
            repository,
        })
    }

    /// A client writing as `actor`'s own GitHub identity: their GitHub App
    /// user grant when the organization has an App binding, otherwise their
    /// personal connection. Never the installation or another person.
    async fn conversation_client(
        &self,
        target: &ConversationTarget,
        actor: Uuid,
    ) -> Result<RepositoryClient, G5Error> {
        let auth = self
            .credentials
            .resolve(
                target.org_id,
                target.repo_id,
                actor,
                &target.git_url,
                RepositoryOperation::PullRequestConversation,
            )
            .await?;
        let token = token_from_auth(&auth)
            .ok_or_else(|| invalid("GitHub repository credentials are unavailable"))?;
        self.repository_protocol(
            &token,
            &target.repository,
            RepositoryCapability::PullRequestWrite,
        )
        .map_err(G5Error::from)
    }
}

// The GitHub adapter's conversation protocol (`g5.git.pr-adapter`): comment
// receipts, replay reconciliation and thread resolution. Routes reach GitHub
// only through these.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PullRequestCommentRequest {
    /// GitHub PR number or URL. A reply may omit it: its thread names the PR.
    pub selector: Option<String>,
    pub repo: Option<String>,
    pub body: String,
    /// Reply in this review thread (node ID) instead of the PR discussion.
    pub reply_to: Option<String>,
    /// Stable across retries of the same comment.
    pub idempotency_key: String,
}

pub fn validation(field: &str, error: &str) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: json!({"field": field, "error": error}),
    })
}

fn provider(error: ProviderError) -> AppError {
    AppError::from_error(G5Error::from(error))
}

/// A replayed key must name this comment: workspace, author kind,
/// destination, repository, PR when selected, thread and text.
fn ensure_same_submission(
    record: &PublicationRecord,
    workspace_id: Uuid,
    origin: &str,
    target: &ConversationTarget,
    request: &PullRequestCommentRequest,
) -> Result<(), AppError> {
    let destination = if request.reply_to.is_some() {
        "review_thread"
    } else {
        "pull_request"
    };
    let same_pr = record.locator["repository"]
        .as_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(&target.repository))
        && target
            .number
            .is_none_or(|number| record.locator["number"].as_i64() == Some(number));
    if record.workspace_id != workspace_id
        || record.origin != origin
        || record.destination != destination
        || record.target_thread_id != request.reply_to
        || !same_pr
    {
        Err(validation(
            "idempotencyKey",
            "already used for another comment",
        ))
    } else if !record.is_same_comment(&request.body) {
        Err(validation(
            "idempotencyKey",
            "already used for a different comment",
        ))
    } else {
        Ok(())
    }
}

/// Look for an unresolved submission's marker on GitHub. Never sends again.
async fn reconcile(
    client: &RepositoryClient,
    record: &PublicationRecord,
) -> Result<PublicationUpdate, AppError> {
    let found = match (&record.target_thread_id, record.locator["number"].as_i64()) {
        (Some(thread), _) => {
            client
                .review_thread(thread, Some(&marker(record.id)))
                .await
                .map_err(provider)?
                .marked
        }
        (None, Some(number)) => {
            let since = (record.created_at - chrono::Duration::minutes(1)).to_rfc3339();
            client
                .find_marked_pull_request_comment(number, &since, &marker(record.id))
                .await
                .map_err(provider)?
        }
        (None, None) => {
            return Err(AppError::from_error(G5Error::Internal {
                message: "pull request comment publication has no locator".into(),
            }))
        }
    };
    Ok(match found {
        Some(comment) => PublicationUpdate::Published {
            comment_id: comment.id.to_string(),
            comment_url: comment.url,
        },
        None => {
            PublicationUpdate::OutcomeUnknown("the comment was not found on GitHub yet".to_string())
        }
    })
}

/// Record and post a PR comment or review-thread reply as `author`.
///
/// `open_db` admits the caller (re-checked before recording); no transaction
/// is held across a GitHub request. A replay of the key returns the recorded
/// submission (`false`), reconciling it first when its outcome is unclear.
pub async fn publish_pull_request_comment<F, Fut>(
    state: &AppState,
    open_db: F,
    workspace_id: Uuid,
    author: Uuid,
    origin: &'static str,
    request: &PullRequestCommentRequest,
) -> Result<(PublicationRecord, bool), AppError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<g5_authz::ScopedDb, AppError>>,
{
    let service = &state.workspace_pull_request_provider_lifecycle_service;
    let mut db = open_db().await?;
    let target = service
        .conversation_target(
            &mut db,
            workspace_id,
            request.selector.as_deref(),
            request.repo.as_deref(),
        )
        .await?;
    let existing = find_by_key(&mut db, target.org_id, author, &request.idempotency_key).await?;
    let completion = db.reopener();
    db.commit().await.map_err(AppError::from)?;
    if let Some(record) = existing {
        ensure_same_submission(&record, workspace_id, origin, &target, request)?;
        if !matches!(record.state.as_str(), "pending" | "outcome_unknown") {
            return Ok((record, false));
        }
        let client = service.conversation_client(&target, author).await?;
        let update_to = reconcile(&client, &record).await?;
        let mut db = completion.open().await.map_err(AppError::from)?;
        let record = update_publication(&mut db, record.id, &update_to).await?;
        db.commit().await.map_err(AppError::from)?;
        return Ok((record, false));
    }

    let client = service.conversation_client(&target, author).await?;
    // Bind the destination before recording: a reply's thread is in this
    // repository (and the selected PR), and a comment's number is a PR.
    let number = match &request.reply_to {
        Some(thread) => {
            let thread = client.review_thread(thread, None).await.map_err(provider)?;
            if target.number.is_some_and(|n| n != thread.pull_number) {
                return Err(validation(
                    "replyTo",
                    "the review thread belongs to another pull request",
                ));
            }
            thread.pull_number
        }
        None => {
            // check_request requires a selector, which resolves to a number.
            let Some(number) = target.number else {
                return Err(validation(
                    "selector",
                    "a GitHub PR number or URL is required unless replying in a review thread",
                ));
            };
            client.pull_request(number).await.map_err(provider)?;
            number
        }
    };
    let login = super::super::github_user_grants::display_login(&state.system_pool, author)
        .await
        .map_err(AppError::from_error)?;
    let publication_id = Uuid::new_v4();
    let payload = render_payload(&request.body, publication_id);
    let locator = json!({"repository": target.repository, "number": number});
    let display_key = format!("{}#{number}", target.repository);
    let mut db = open_db().await?;
    let (record, fresh) = insert_pending(
        &mut db,
        target.org_id,
        &NewPublication {
            id: publication_id,
            workspace_id,
            destination: if request.reply_to.is_some() {
                "review_thread"
            } else {
                "pull_request"
            },
            domain_id: None,
            tracker_id: None,
            external_issue_id: &number.to_string(),
            display_key: &display_key,
            locator: &locator,
            author_user_id: author,
            github_login: &login,
            idempotency_key: &request.idempotency_key,
            payload: &payload,
            target_comment_id: None,
            origin,
            target_thread_id: request.reply_to.as_deref(),
        },
    )
    .await?;
    let completion = db.reopener();
    db.commit().await.map_err(AppError::from)?;
    if !fresh {
        // A concurrent submission with the key won; it must be this comment.
        ensure_same_submission(&record, workspace_id, origin, &target, request)?;
        return Ok((record, false));
    }

    let outcome = match &request.reply_to {
        Some(thread) => client.reply_to_review_thread(thread, &payload).await,
        None => client.post_pull_request_comment(number, &payload).await,
    };
    let update_to = match outcome {
        Ok(comment) => PublicationUpdate::Published {
            comment_id: comment.id.to_string(),
            comment_url: comment.url,
        },
        Err(error) if error.effect == FailureEffect::Refused => {
            PublicationUpdate::Failed(IssueError::from(error))
        }
        Err(error) => PublicationUpdate::OutcomeUnknown(error.to_string()),
    };
    let mut db = completion.open().await.map_err(AppError::from)?;
    let record = update_publication(&mut db, record.id, &update_to).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok((record, true))
}

/// Resolve or unresolve a review thread as `actor`. Already in the requested
/// state is success without a write; an unclear write is re-read.
pub async fn set_review_thread_resolution(
    state: &AppState,
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    actor: Uuid,
    request: &ReviewThreadResolutionRequest,
) -> Result<Value, AppError> {
    if !valid_thread_id(&request.thread) {
        return Err(validation("thread", "must be a review thread ID"));
    }
    let service = &state.workspace_pull_request_provider_lifecycle_service;
    let target = service
        .conversation_target(
            &mut db,
            workspace_id,
            request.selector.as_deref(),
            request.repo.as_deref(),
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let client = service.conversation_client(&target, actor).await?;
    let thread = client
        .review_thread(&request.thread, None)
        .await
        .map_err(provider)?;
    if target.number.is_some_and(|n| n != thread.pull_number) {
        return Err(validation(
            "thread",
            "the review thread belongs to another pull request",
        ));
    }
    let changed = thread.is_resolved != request.resolved;
    if changed {
        match client
            .set_review_thread_resolution(&request.thread, request.resolved)
            .await
        {
            Ok(resolved) if resolved == request.resolved => {}
            Ok(_) => {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: "GitHub did not change the review thread's resolution".into(),
                }))
            }
            Err(error) if error.effect == FailureEffect::Refused => return Err(provider(error)),
            Err(error) => {
                let reread = client.review_thread(&request.thread, None).await;
                if !reread.is_ok_and(|now| now.is_resolved == request.resolved) {
                    return Err(provider(error));
                }
            }
        }
    }
    Ok(json!({
        "threadId": request.thread,
        "repository": target.repository,
        "number": thread.pull_number,
        "isResolved": request.resolved,
        "changed": changed,
    }))
}
