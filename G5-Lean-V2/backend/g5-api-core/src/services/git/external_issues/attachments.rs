//! Attachments of an issue, listed and fetched through the adapter that
//! serves its domain (`g5.attachments`, `g5.issue.native.adapter`).
//!
//! The caller chooses and admits the issue as for its other reads; the
//! adapter finds and fetches. A listing names each attachment by place and
//! gives an opaque handle; a fetch takes the handle back. GitHub's adapter
//! reads the issue's rendered texts again for each fetch, so the signed
//! links its rendering carries are used within that request and never
//! stored or returned. G5's adapter reads its own store.

use g5_contracts::attachments::AttachmentKind;
use g5_contracts::error::{ExternalIssueErrorKind, G5Error};
use serde::Serialize;
use serde_json::json;

use super::{
    domain_token, ensure_linked_issue, link_domain, summary, user_dto, ExternalIssueSummaryDTO,
    ExternalIssueUserDTO, IssueBackends, IssueDomainAccess, LinkedLocation,
    WorkspaceIssueLinkRecord,
};
use crate::services::git::github_issues::{
    fetch_rendered_comment, fetch_rendered_comments, fetch_rendered_issue, open_attachment,
    AttachmentContent, IssueError, RenderedAttachment, RenderedText, MAX_LISTED_ATTACHMENTS,
};
use crate::services::git::native_issues::{self, NativeAttachmentPlace};

/// The issue whose attachments are read: a workspace link, or an issue in a
/// domain.
#[derive(Clone, Copy)]
pub enum IssueAttachmentTarget<'a> {
    Linked(&'a WorkspaceIssueLinkRecord),
    Domain {
        access: &'a IssueDomainAccess,
        number: u64,
    },
}

/// One attachment as a listing names it (`g5.attachments`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueAttachmentDTO {
    /// Opaque: a fetch takes it back unchanged.
    pub handle: String,
    /// `issue`, `description` or `comment`.
    pub place: &'static str,
    pub comment_id: Option<String>,
    /// Who wrote the place and when; for an attachment at the issue, who
    /// uploaded it.
    pub author: Option<ExternalIssueUserDTO>,
    pub created_at: String,
    /// 1-based within its text, when the adapter knows it.
    pub position: Option<u32>,
    pub kind: AttachmentKind,
    /// An image's alt text or a file's name.
    pub name: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub retrievable: bool,
    /// Why the attachment cannot be retrieved.
    pub reason: Option<String>,
    /// The public address of an image hosted outside the provider.
    pub external_url: Option<String>,
    /// The file a pull request's review comment is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueAttachmentsDTO {
    pub issue: ExternalIssueSummaryDTO,
    pub attachments: Vec<IssueAttachmentDTO>,
    /// More attachments than one listing names.
    pub truncated: bool,
    /// Comments were read and the tracker has more than one page.
    pub comments_truncated: bool,
}

/// An attachment being read: its kind, type, name, declared length and
/// bytes through [`IssueAttachmentStream::chunk`].
pub struct IssueAttachmentStream {
    pub kind: AttachmentKind,
    pub content_type: String,
    pub name: Option<String>,
    pub length: Option<u64>,
    body: AttachmentBody,
}

enum AttachmentBody {
    Github(Box<AttachmentContent>),
    Stored(g5_storage::ByteStream),
}

impl IssueAttachmentStream {
    /// A GitHub attachment opened by the shared mechanism, for the
    /// pull-request service.
    pub(crate) fn github(content: AttachmentContent, name: Option<String>) -> Self {
        Self {
            kind: content.kind,
            content_type: content.content_type.clone(),
            name,
            length: content.length,
            body: AttachmentBody::Github(Box::new(content)),
        }
    }

    /// The next part of the attachment, `None` at its end.
    pub async fn chunk(&mut self) -> Result<Option<bytes::Bytes>, IssueError> {
        match &mut self.body {
            AttachmentBody::Github(content) => content.chunk().await,
            AttachmentBody::Stored(body) => {
                use futures_util::TryStreamExt as _;
                body.try_next().await.map_err(|error| {
                    IssueError::unavailable(format!("the attachment could not be read: {error}"))
                })
            }
        }
    }
}

/// Where a GitHub attachment's handle points: a text and a position in it,
/// at the text's version.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GithubHandle {
    Description {
        position: u32,
        version: String,
    },
    Comment {
        id: String,
        position: u32,
        version: String,
    },
}

impl GithubHandle {
    fn encode(&self) -> String {
        match self {
            Self::Description { position, version } => format!("description:{position}:{version}"),
            Self::Comment {
                id,
                position,
                version,
            } => format!("comment:{id}:{position}:{version}"),
        }
    }

    fn parse(handle: &str) -> Option<Self> {
        let parts: Vec<&str> = handle.split(':').collect();
        let position = |value: &str| value.parse::<u32>().ok().filter(|p| *p > 0);
        let version = |value: &str| {
            (value.len() == 16 && value.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| value.to_string())
        };
        match parts.as_slice() {
            ["description", p, v] => Some(Self::Description {
                position: position(p)?,
                version: version(v)?,
            }),
            ["comment", id, p, v] if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) => {
                Some(Self::Comment {
                    id: (*id).to_string(),
                    position: position(p)?,
                    version: version(v)?,
                })
            }
            _ => None,
        }
    }
}

/// A G5 attachment's handle: its id.
fn native_handle(id: i64) -> String {
    format!("g5:{id}")
}

fn parse_native_handle(handle: &str) -> Option<i64> {
    handle
        .strip_prefix("g5:")
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id > 0)
}

fn invalid_handle() -> G5Error {
    G5Error::ValidationError {
        details: json!({"field": "handle", "error": "not a handle this issue's listing gave"}),
    }
}

struct GithubTarget<'a> {
    access: &'a IssueDomainAccess,
    repository_id: u64,
    number: u64,
    link: Option<&'a WorkspaceIssueLinkRecord>,
}

enum Adapter<'a> {
    Github(GithubTarget<'a>),
    Native {
        access: &'a IssueDomainAccess,
        issue: NativeIssue,
    },
}

enum NativeIssue {
    Id(i64),
    Number(u64),
}

/// The adapter serving the issue a target names.
fn adapter_for<'a>(
    domains: &'a [IssueDomainAccess],
    target: IssueAttachmentTarget<'a>,
) -> Result<Adapter<'a>, IssueError> {
    match target {
        IssueAttachmentTarget::Linked(link) => {
            let linked = link_domain(domains, link)?;
            Ok(match linked.location {
                LinkedLocation::Native { issue_id } => Adapter::Native {
                    access: linked.access,
                    issue: NativeIssue::Id(issue_id),
                },
                LinkedLocation::Github { repository_id } => Adapter::Github(GithubTarget {
                    access: linked.access,
                    repository_id,
                    number: linked.number,
                    link: Some(link),
                }),
            })
        }
        IssueAttachmentTarget::Domain { access, number } if access.is_native() => {
            Ok(Adapter::Native {
                access,
                issue: NativeIssue::Number(number),
            })
        }
        IssueAttachmentTarget::Domain { access, number } => Ok(Adapter::Github(GithubTarget {
            access,
            repository_id: access.github_repository_id()?,
            number,
            link: None,
        })),
    }
}

/// The issue's attachments: GitHub's from its description and, when
/// `with_comments`, its first comment page; G5's from its store, with their
/// places derived from all its texts.
pub async fn list_issue_attachments(
    backends: &IssueBackends<'_>,
    domains: &[IssueDomainAccess],
    target: IssueAttachmentTarget<'_>,
    with_comments: bool,
) -> Result<IssueAttachmentsDTO, IssueError> {
    let mut listing = match adapter_for(domains, target)? {
        Adapter::Github(github) => list_github(backends, &github, with_comments).await?,
        Adapter::Native { access, issue } => list_native(backends, access, issue).await?,
    };
    listing.truncated = listing.attachments.len() > MAX_LISTED_ATTACHMENTS;
    listing.attachments.truncate(MAX_LISTED_ATTACHMENTS);
    Ok(listing)
}

/// Open one attachment of the issue by the handle its listing gave.
pub async fn open_issue_attachment(
    backends: &IssueBackends<'_>,
    domains: &[IssueDomainAccess],
    target: IssueAttachmentTarget<'_>,
    handle: &str,
) -> Result<IssueAttachmentStream, G5Error> {
    match adapter_for(domains, target)? {
        Adapter::Github(github) => {
            let handle = GithubHandle::parse(handle).ok_or_else(invalid_handle)?;
            open_github(backends, &github, &handle).await
        }
        Adapter::Native { access, issue } => {
            let id = parse_native_handle(handle).ok_or_else(invalid_handle)?;
            open_native(backends, access, issue, id).await
        }
    }
}

// ─── GitHub ──────────────────────────────────────────────────────────────────

fn github_listed<'a>(
    text: &'a RenderedText,
    place: &'static str,
    comment_id: Option<&'a str>,
    author: Option<ExternalIssueUserDTO>,
    created_at: &'a str,
) -> impl Iterator<Item = IssueAttachmentDTO> + 'a {
    text.attachments
        .iter()
        .map(move |attachment: &RenderedAttachment| {
            let handle = match comment_id {
                None => GithubHandle::Description {
                    position: attachment.position,
                    version: text.version.clone(),
                },
                Some(id) => GithubHandle::Comment {
                    id: id.to_string(),
                    position: attachment.position,
                    version: text.version.clone(),
                },
            };
            IssueAttachmentDTO {
                handle: handle.encode(),
                place,
                comment_id: comment_id.map(str::to_owned),
                author: author.clone(),
                created_at: created_at.to_owned(),
                position: Some(attachment.position),
                kind: attachment.kind,
                name: attachment.name.clone(),
                content_type: None,
                size_bytes: None,
                retrievable: attachment.unavailable.is_none(),
                reason: attachment.unavailable.map(str::to_owned),
                external_url: attachment.external_url.clone(),
                path: None,
            }
        })
}

async fn list_github(
    backends: &IssueBackends<'_>,
    github: &GithubTarget<'_>,
    with_comments: bool,
) -> Result<IssueAttachmentsDTO, IssueError> {
    let client = backends.github()?;
    let token = domain_token(client, github.access).await?;
    let (rendered, comments) = tokio::join!(
        fetch_rendered_issue(client, &token, github.repository_id, github.number),
        async {
            if with_comments {
                fetch_rendered_comments(client, &token, github.repository_id, github.number)
                    .await
                    .map(Some)
            } else {
                Ok(None)
            }
        }
    );
    let rendered = rendered?;
    if let Some(link) = github.link {
        ensure_linked_issue(link, &rendered.issue)?;
    }
    let comments = comments?;
    let issue = &rendered.issue;
    let mut attachments: Vec<IssueAttachmentDTO> = github_listed(
        &rendered.description,
        "description",
        None,
        issue.author.clone().map(user_dto),
        &issue.created_at,
    )
    .collect();
    for comment in comments.iter().flatten() {
        attachments.extend(github_listed(
            &comment.text,
            "comment",
            Some(&comment.comment.id),
            comment.comment.author.clone().map(user_dto),
            &comment.comment.created_at,
        ));
    }
    let comments_truncated = comments.as_ref().is_some_and(|comments| {
        issue.comment_count > u64::try_from(comments.len()).unwrap_or(u64::MAX)
    });
    Ok(IssueAttachmentsDTO {
        issue: summary(github.access, github.repository_id, issue),
        attachments,
        truncated: false,
        comments_truncated,
    })
}

/// Read the handle's text again, check it is still the listed version, and
/// open the attachment at its position. A changed text never returns
/// another attachment under the listed place.
async fn open_github(
    backends: &IssueBackends<'_>,
    github: &GithubTarget<'_>,
    handle: &GithubHandle,
) -> Result<IssueAttachmentStream, G5Error> {
    let client = backends.github()?;
    let token = domain_token(client, github.access).await?;
    let (text, position, version) = match handle {
        GithubHandle::Description { position, version } => {
            let rendered =
                fetch_rendered_issue(client, &token, github.repository_id, github.number).await?;
            if let Some(link) = github.link {
                ensure_linked_issue(link, &rendered.issue)?;
            }
            (rendered.description, *position, version)
        }
        GithubHandle::Comment {
            id,
            position,
            version,
        } => (
            fetch_rendered_comment(client, &token, github.repository_id, github.number, id)
                .await?
                .text,
            *position,
            version,
        ),
    };
    if &text.version != version {
        return Err(G5Error::ConflictDetails {
            reason: "the issue text changed since its attachments were listed; list them again"
                .to_string(),
            details: json!({ "code": "issue_text_changed" }),
        });
    }
    let attachment = text
        .attachments
        .iter()
        .find(|attachment| attachment.position == position)
        .ok_or_else(|| {
            IssueError::new(
                ExternalIssueErrorKind::NotFound,
                format!("the text has no attachment {position}"),
            )
        })?;
    let content = open_attachment(client, &token, github.repository_id, attachment).await?;
    Ok(IssueAttachmentStream {
        kind: content.kind,
        content_type: content.content_type.clone(),
        name: attachment.name.clone(),
        length: content.length,
        body: AttachmentBody::Github(Box::new(content)),
    })
}

// ─── G5 issues ───────────────────────────────────────────────────────────────

async fn native_issue_id(
    db: &mut g5_authz::ScopedDb,
    reader: &native_issues::NativeReader<'_>,
    issue: &NativeIssue,
) -> Result<i64, IssueError> {
    match issue {
        NativeIssue::Id(id) => Ok(*id),
        NativeIssue::Number(number) => native_issues::issue_id_by_number(db, reader, *number)
            .await
            .map_err(|error| native_issues::read_error(&error)),
    }
}

async fn list_native(
    backends: &IssueBackends<'_>,
    access: &IssueDomainAccess,
    issue: NativeIssue,
) -> Result<IssueAttachmentsDTO, IssueError> {
    let native = backends.native()?;
    let reader = native.reader(access);
    let mut db = native.open().await?;
    let id = native_issue_id(&mut db, &reader, &issue).await?;
    let summary = native_issues::summary_by_id(&mut db, &reader, id).await?;
    let read = |error: G5Error| native_issues::read_error(&error);
    let stored = native_issues::list_attachments(&mut db, &reader, id)
        .await
        .map_err(read)?;
    let places = native_issues::attachment_places(&mut db, &reader, id, &stored)
        .await
        .map_err(read)?;
    super::finish(db).await?;
    let attachments = stored
        .into_iter()
        .zip(places)
        .map(|(attachment, place)| {
            let (place, comment_id, author, created_at) = match place {
                NativeAttachmentPlace::Description => (
                    "description",
                    None,
                    summary.author.clone(),
                    summary.created_at.clone(),
                ),
                NativeAttachmentPlace::Comment {
                    id,
                    author,
                    created_at,
                } => (
                    "comment",
                    Some(id.to_string()),
                    Some(author),
                    native_issues::wire_time(created_at),
                ),
                NativeAttachmentPlace::Issue => (
                    "issue",
                    None,
                    Some(attachment.uploader.clone()),
                    native_issues::wire_time(attachment.created_at),
                ),
            };
            IssueAttachmentDTO {
                handle: native_handle(attachment.id),
                place,
                comment_id,
                author,
                created_at,
                position: None,
                kind: AttachmentKind::Image,
                name: Some(attachment.file_name),
                content_type: Some(attachment.content_type),
                size_bytes: Some(attachment.size_bytes),
                retrievable: true,
                reason: None,
                external_url: None,
                path: None,
            }
        })
        .collect();
    Ok(IssueAttachmentsDTO {
        issue: summary,
        attachments,
        truncated: false,
        comments_truncated: false,
    })
}

async fn open_native(
    backends: &IssueBackends<'_>,
    access: &IssueDomainAccess,
    issue: NativeIssue,
    attachment_id: i64,
) -> Result<IssueAttachmentStream, G5Error> {
    let native = backends.native()?;
    let reader = native.reader(access);
    let mut db = native.open().await?;
    let id = native_issue_id(&mut db, &reader, &issue).await?;
    let (attachment, content) =
        native_issues::attachment_content(&mut db, &reader, native.attachments, id, attachment_id)
            .await?;
    super::finish(db).await?;
    Ok(IssueAttachmentStream {
        kind: AttachmentKind::Image,
        content_type: attachment.content_type,
        name: Some(attachment.file_name),
        length: Some(content.length),
        body: AttachmentBody::Stored(content.body),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_round_trip_and_refuse_anything_else() {
        let description = GithubHandle::Description {
            position: 2,
            version: "0123456789abcdef".to_string(),
        };
        let comment = GithubHandle::Comment {
            id: "9501".to_string(),
            position: 1,
            version: "fedcba9876543210".to_string(),
        };
        for handle in [&description, &comment] {
            assert_eq!(GithubHandle::parse(&handle.encode()).as_ref(), Some(handle));
        }
        for bad in [
            "description:0:0123456789abcdef",
            "description:1:short",
            "comment:x:1:0123456789abcdef",
            "comment::1:0123456789abcdef",
            "g5:7",
            "",
        ] {
            assert_eq!(GithubHandle::parse(bad), None, "{bad}");
        }
        assert_eq!(parse_native_handle(&native_handle(7)), Some(7));
        assert_eq!(parse_native_handle("g5:0"), None);
        assert_eq!(parse_native_handle("description:1:0123456789abcdef"), None);
    }
}
