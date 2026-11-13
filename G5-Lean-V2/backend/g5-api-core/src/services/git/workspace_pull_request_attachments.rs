//! Attachments of a GitHub pull request (`g5.attachments`,
//! `g5.git.pr-adapter`): the pull-request service selects and admits it as
//! for `g5d pr view NUMBER -c`, and GitHub's adapter lists the attachments
//! its texts show and fetches one by the handle its listing gave. A fetch
//! reads that one text again, so the signed links GitHub's rendering
//! carries are used within the request and never stored or returned.
//!
//! G5 PRs and repositories not on GitHub are refused before any read, as
//! their conversation is (`g5.git.pr-local-conversation`).

use g5_contracts::G5Error;
use g5_scm::providers::issues::{open_attachment, IssueError, RenderedText};
use g5_scm::providers::{PullTextKind, RenderedPullText, RepositoryClient};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use super::{invalid, selection, PullRequestReadQuery};
use crate::services::git::external_issues::{
    user_dto, ExternalIssueUserDTO, IssueAttachmentDTO, IssueAttachmentStream,
};
use crate::services::git::workspace_pull_request_provider_lifecycle::WorkspacePullRequestProviderLifecycleService;
use g5_authz::ScopedDb;

/// Most attachments one listing names, as for issues.
const MAX_LISTED: usize = g5_scm::providers::issues::MAX_LISTED_ATTACHMENTS;

/// Refusal for a G5 PR, which has no attachments of its own to list.
pub const LOCAL_ATTACHMENTS_REFUSAL: &str =
    "G5 PRs (PR######) and repositories not on GitHub have no PR attachments: a G5 PR's \
     description is G5's own, so read its files in the workspace; `g5d pr view PR######` \
     names its GitHub PRs, whose attachments `g5d pr attachments NUMBER` lists";

/// The pull request a listing is of.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachedPullRequestDTO {
    /// `owner/name#N`.
    pub display_key: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestAttachmentsDTO {
    pub pull_request: AttachedPullRequestDTO,
    pub attachments: Vec<IssueAttachmentDTO>,
    /// More attachments than one listing names.
    pub truncated: bool,
    /// Some kind of text had more than one page.
    pub comments_truncated: bool,
}

/// Where a pull-request attachment's handle points: a text, the
/// attachment's position in it and the text's version.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PullHandle {
    kind: PullTextKind,
    id: Option<String>,
    position: u32,
    version: String,
}

fn kind_name(kind: PullTextKind) -> &'static str {
    match kind {
        PullTextKind::Description => "description",
        PullTextKind::Comment => "comment",
        PullTextKind::Review => "review",
        PullTextKind::ReviewComment => "review-comment",
    }
}

impl PullHandle {
    fn encode(&self) -> String {
        match &self.id {
            None => format!(
                "{}:{}:{}",
                kind_name(self.kind),
                self.position,
                self.version
            ),
            Some(id) => format!(
                "{}:{id}:{}:{}",
                kind_name(self.kind),
                self.position,
                self.version
            ),
        }
    }

    fn parse(handle: &str) -> Option<Self> {
        let parts: Vec<&str> = handle.split(':').collect();
        let position = |value: &str| value.parse::<u32>().ok().filter(|p| *p > 0);
        let version = |value: &str| {
            (value.len() == 16 && value.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| value.to_string())
        };
        let id = |value: &str| {
            (!value.is_empty() && value.len() <= 20 && value.bytes().all(|b| b.is_ascii_digit()))
                .then(|| value.to_string())
        };
        let (kind, id, p, v) = match parts.as_slice() {
            ["description", p, v] => (PullTextKind::Description, None, *p, *v),
            [kind, i, p, v] => {
                let kind = match *kind {
                    "comment" => PullTextKind::Comment,
                    "review" => PullTextKind::Review,
                    "review-comment" => PullTextKind::ReviewComment,
                    _ => return None,
                };
                (kind, Some(id(i)?), *p, *v)
            }
            _ => return None,
        };
        Some(Self {
            kind,
            id,
            position: position(p)?,
            version: version(v)?,
        })
    }
}

fn place(kind: PullTextKind) -> &'static str {
    match kind {
        PullTextKind::Description => "description",
        PullTextKind::Comment => "comment",
        PullTextKind::Review => "review",
        PullTextKind::ReviewComment => "review comment",
    }
}

fn listed(text: &RenderedPullText) -> impl Iterator<Item = IssueAttachmentDTO> + '_ {
    let author: Option<ExternalIssueUserDTO> = text.author.clone().map(user_dto);
    text.text.attachments.iter().map(move |attachment| {
        let handle = PullHandle {
            kind: text.kind,
            id: text.id.clone(),
            position: attachment.position,
            version: text.text.version.clone(),
        };
        IssueAttachmentDTO {
            handle: handle.encode(),
            place: place(text.kind),
            comment_id: text.id.clone(),
            author: author.clone(),
            created_at: text.created_at.clone().unwrap_or_default(),
            position: Some(attachment.position),
            kind: attachment.kind,
            name: attachment.name.clone(),
            content_type: None,
            size_bytes: None,
            retrievable: attachment.unavailable.is_none(),
            reason: attachment.unavailable.map(str::to_owned),
            external_url: attachment.external_url.clone(),
            path: text.path.clone(),
        }
    })
}

fn issue_error(error: IssueError) -> G5Error {
    G5Error::from(error)
}

impl WorkspacePullRequestProviderLifecycleService {
    /// The selected GitHub pull request's attachments: its description's
    /// and, with `comments`, those of its first page of each other text.
    pub async fn list_pull_request_attachments(
        &self,
        db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        query: &PullRequestReadQuery,
    ) -> Result<PullRequestAttachmentsDTO, G5Error> {
        let (client, number, repository) = self
            .attachment_client(db, workspace_id, actor, query)
            .await?;
        let pull = client
            .read_rendered_pull(number, query.comments)
            .await
            .map_err(issue_error)?;
        let mut attachments: Vec<IssueAttachmentDTO> = listed(&pull.description).collect();
        for text in &pull.discussion {
            attachments.extend(listed(text));
        }
        let truncated = attachments.len() > MAX_LISTED;
        attachments.truncate(MAX_LISTED);
        Ok(PullRequestAttachmentsDTO {
            pull_request: AttachedPullRequestDTO {
                display_key: format!("{repository}#{number}"),
                repository,
                number,
                title: pull.title,
                url: pull.url,
            },
            attachments,
            truncated,
            comments_truncated: pull.discussion_truncated,
        })
    }

    /// One attachment by the handle the listing gave: its text is read
    /// again and must still be the listed version.
    pub async fn open_pull_request_attachment(
        &self,
        db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        query: &PullRequestReadQuery,
        handle: &str,
    ) -> Result<IssueAttachmentStream, G5Error> {
        let handle = PullHandle::parse(handle.trim()).ok_or_else(|| G5Error::ValidationError {
            details: json!({"field": "handle", "error": "not a handle this PR's listing gave"}),
        })?;
        let (client, number, _) = self
            .attachment_client(db, workspace_id, actor, query)
            .await?;
        let text: RenderedText = client
            .read_rendered_pull_text(number, handle.kind, handle.id.as_deref())
            .await
            .map_err(issue_error)?
            .text;
        if text.version != handle.version {
            return Err(G5Error::ConflictDetails {
                reason: "the PR text changed since its attachments were listed; list them again"
                    .to_string(),
                details: json!({ "code": "pull_request_text_changed" }),
            });
        }
        let attachment = text
            .attachments
            .iter()
            .find(|attachment| attachment.position == handle.position)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("attachment {} of the text", handle.position),
            })?;
        let content = open_attachment(&client, attachment)
            .await
            .map_err(issue_error)?;
        Ok(IssueAttachmentStream::github(
            content,
            attachment.name.clone(),
        ))
    }

    /// The GitHub pull request a query names, admitted as for
    /// `g5d pr view NUMBER -c`, and a client holding the owner's read grant.
    async fn attachment_client(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        query: &PullRequestReadQuery,
    ) -> Result<(RepositoryClient, u64, String), G5Error> {
        let selector = query
            .selector
            .as_deref()
            .map(str::trim)
            .ok_or_else(|| invalid("A GitHub PR number or URL is required"))?;
        if Uuid::parse_str(selector).is_ok()
            || crate::services::git::pull_request_adapters::parse_g5_pull_request_name(selector)
                .is_some()
        {
            return Err(invalid(LOCAL_ATTACHMENTS_REFUSAL));
        }
        // A number in a repository not on GitHub names no GitHub PR, and its
        // G5 PR has none either (`g5.attachments.cli`).
        if self
            .repository_not_on_github(
                &mut db,
                workspace_id,
                actor,
                selector,
                query.repo.as_deref(),
            )
            .await?
            .is_some()
        {
            return Err(invalid(LOCAL_ATTACHMENTS_REFUSAL));
        }
        let (number, selected_repo) =
            selection(&self.provider_endpoints()?, selector, query.repo.as_deref())?;
        let (client, name) = self
            .github_pull_request_client(db, workspace_id, actor, selected_repo.as_deref())
            .await?;
        let number = u64::try_from(number).map_err(|_| invalid("Invalid PR number"))?;
        Ok((client, number, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_name_each_kind_of_text_and_refuse_anything_else() {
        let version = "0123456789abcdef".to_string();
        for (kind, id) in [
            (PullTextKind::Description, None),
            (PullTextKind::Comment, Some("501")),
            (PullTextKind::Review, Some("601")),
            (PullTextKind::ReviewComment, Some("701")),
        ] {
            let handle = PullHandle {
                kind,
                id: id.map(str::to_owned),
                position: 2,
                version: version.clone(),
            };
            assert_eq!(PullHandle::parse(&handle.encode()), Some(handle));
        }
        for bad in [
            "",
            "description:0:0123456789abcdef",
            "comment:x:1:0123456789abcdef",
            "comment:1:0123456789abcdef",
            "thread:1:1:0123456789abcdef",
            "review:1:1:short",
            "g5:7",
        ] {
            assert_eq!(PullHandle::parse(bad), None, "{bad}");
        }
    }
}
