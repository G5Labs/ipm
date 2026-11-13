//! Attachments in an issue's description and comments, for the workspace
//! agent (`g5.attachments.github`).
//!
//! GitHub keeps no list of an item's attachments: they are the links its
//! texts carry. Its `full+json` representation adds each text's rendering,
//! in which an image or video uploaded to a private repository points at a
//! link signed for a few minutes. The links stay inside
//! [`RenderedAttachment`]; content is opened without a credential, only from
//! GitHub's own hosts, recognized by kind and bounded by GitHub's upload
//! limits. The same operations serve any GitHub item whose texts are read
//! with an admitted grant.

use std::time::Duration;

use g5_contracts::attachments::{image_type, video_type, AttachmentKind, RECOGNITION_PREFIX};
use g5_contracts::error::ExternalIssueErrorKind;
use sha2::{Digest, Sha256};
use url::Url;

use super::{
    normalize_issue, owned_comment, validate_issue_urls, GithubIssue, GithubIssueComment,
    IssueError, RawComment, RawIssue, COMMENT_PAGE_SIZE, INTERACTIVE_TIMEOUT, READ, READERS,
};
use crate::providers::transport::HostedBody;
use crate::providers::{Payload, ProviderEndpoints, RepositoryClient};

/// Grants whose texts' attachments can be opened: issue and pull-request reads.
const OPENERS: &[super::RepositoryCapability] = &[
    super::RepositoryCapability::IssuesRead,
    super::RepositoryCapability::IssueCommentWrite,
    super::RepositoryCapability::PullRequestRead,
];

/// Most attachments one listing names; the rest are reported as truncated.
pub const MAX_LISTED_ATTACHMENTS: usize = 50;
pub(in crate::providers) const FULL_JSON: &str = "application/vnd.github.full+json";

/// GitHub's own upload limit for each kind: images 10 MB, videos 100 MB
/// (on paid plans), other files 25 MB.
#[must_use]
pub const fn attachment_limit(kind: AttachmentKind) -> usize {
    match kind {
        AttachmentKind::Image => 10 * 1024 * 1024,
        AttachmentKind::Video => 100 * 1024 * 1024,
        AttachmentKind::File => 25 * 1024 * 1024,
    }
}

/// Budget for reading one attachment, redirects included.
const fn attachment_timeout(kind: AttachmentKind) -> Duration {
    Duration::from_secs(match kind {
        AttachmentKind::Image => 20,
        AttachmentKind::File => 60,
        AttachmentKind::Video => 180,
    })
}

/// One attachment in GitHub's rendering of an issue text. Its source, which
/// may be a signed link, never leaves this crate.
#[derive(Clone, PartialEq, Eq)]
pub struct RenderedAttachment {
    /// 1-based, in the order the text shows its attachments (emoji excluded).
    pub position: u32,
    pub kind: AttachmentKind,
    /// An image's alt text or a file's name.
    pub name: Option<String>,
    /// Why G5 does not fetch the attachment; `None` when it does.
    pub unavailable: Option<&'static str>,
    /// The address of an image hosted outside GitHub, which is public.
    pub external_url: Option<String>,
    source: Option<Url>,
}

impl std::fmt::Debug for RenderedAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderedAttachment")
            .field("position", &self.position)
            .field("kind", &self.kind)
            .field("name", &self.name)
            .field("unavailable", &self.unavailable)
            .field("external_url", &self.external_url)
            .finish_non_exhaustive()
    }
}

/// The attachments of one issue text and the version they were read at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedText {
    /// Changes whenever the text's Markdown does.
    pub version: String,
    pub attachments: Vec<RenderedAttachment>,
}

#[derive(Debug, Clone)]
pub struct RenderedIssue {
    pub issue: GithubIssue,
    pub description: RenderedText,
}

#[derive(Debug, Clone)]
pub struct RenderedComment {
    pub comment: GithubIssueComment,
    pub text: RenderedText,
}

/// An attachment being read: its recognized or declared type, its declared
/// length, and its bytes through [`AttachmentContent::chunk`].
pub struct AttachmentContent {
    pub kind: AttachmentKind,
    pub content_type: String,
    pub length: Option<u64>,
    pub limit: usize,
    prefix: Option<bytes::Bytes>,
    body: HostedBody,
}

impl std::fmt::Debug for AttachmentContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachmentContent")
            .field("kind", &self.kind)
            .field("content_type", &self.content_type)
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

impl AttachmentContent {
    /// The next part of the content, `None` at its end. A body beyond the
    /// kind's limit or deadline is an error, never a truncated file.
    pub async fn chunk(&mut self) -> Result<Option<bytes::Bytes>, IssueError> {
        if let Some(prefix) = self.prefix.take() {
            return Ok(Some(prefix));
        }
        Ok(self.body.chunk().await?)
    }
}

/// The version of an issue text: a short hash of its Markdown.
#[must_use]
pub fn text_version(markdown: &str) -> String {
    let digest = Sha256::digest(markdown.as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
/// `GET /repositories/{id}/issues/{number}` with GitHub's rendering.
pub async fn fetch_rendered_issue(
    client: &RepositoryClient,
    number: u64,
) -> Result<RenderedIssue, IssueError> {
    client.require(READ, READERS)?;
    let repository_id = client.repository_id()?.to_string();
    let url = client.grant.endpoints.api(&[
        "repositories",
        &repository_id,
        "issues",
        &number.to_string(),
    ]);
    let mut raw: RawIssue = rendered_get(client, url).await?;
    validate_issue_urls(client, &raw)?;
    let html = raw.body_html.take().unwrap_or_default();
    let issue = normalize_issue(raw)?;
    if issue.number != number {
        return Err(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the GitHub issue has moved",
        ));
    }
    let description = rendered_text(
        &client.grant.endpoints,
        issue.body.as_deref().unwrap_or_default(),
        &html,
    );
    Ok(RenderedIssue { issue, description })
}

/// The first comment page with GitHub's rendering, oldest first.
pub async fn fetch_rendered_comments(
    client: &RepositoryClient,
    number: u64,
) -> Result<Vec<RenderedComment>, IssueError> {
    client.require(READ, READERS)?;
    let repository_id = client.repository_id()?.to_string();
    let url = super::super::query(
        client.grant.endpoints.api(&[
            "repositories",
            &repository_id,
            "issues",
            &number.to_string(),
            "comments",
        ]),
        &[("per_page", COMMENT_PAGE_SIZE.to_string())],
    );
    let raw: Vec<RawComment> = rendered_get(client, url).await?;
    let mut comments = raw
        .into_iter()
        .map(|raw| rendered_comment(client, raw))
        .collect::<Result<Vec<_>, _>>()?;
    comments.sort_by(|left, right| left.comment.created_at.cmp(&right.comment.created_at));
    Ok(comments)
}

/// One comment with GitHub's rendering; it must be on issue `number`.
pub async fn fetch_rendered_comment(
    client: &RepositoryClient,
    number: u64,
    comment_id: &str,
) -> Result<RenderedComment, IssueError> {
    if super::parse_canonical_github_id(comment_id).is_none() {
        return Err(IssueError::invalid_reference("Invalid comment identity"));
    }
    client.require(READ, READERS)?;
    let repository_id = client.repository_id()?.to_string();
    let url = client.grant.endpoints.api(&[
        "repositories",
        &repository_id,
        "issues",
        "comments",
        comment_id,
    ]);
    let raw: RawComment = rendered_get(client, url).await?;
    let rendered = rendered_comment(client, raw)?;
    if rendered.comment.issue_number != Some(number) {
        return Err(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the comment is not on this issue",
        ));
    }
    Ok(rendered)
}

/// Open one listed attachment, sending no credential, and recognize its
/// content: an image or video must be one, a file is data.
pub async fn open_attachment(
    client: &RepositoryClient,
    attachment: &RenderedAttachment,
) -> Result<AttachmentContent, IssueError> {
    const OP: &str = "fetch attachment";
    // No credential goes with the fetch: any grant that read the texts will do.
    client.require(OP, OPENERS)?;
    let (Some(source), None) = (&attachment.source, attachment.unavailable) else {
        return Err(IssueError::new(
            ExternalIssueErrorKind::InvalidReference,
            attachment
                .unavailable
                .unwrap_or("the attachment has no source"),
        ));
    };
    let kind = attachment.kind;
    let limit = attachment_limit(kind);
    let endpoints = &client.grant.endpoints;
    let mut body = client
        .http
        .open_hosted(
            OP,
            source.clone(),
            &|url| hosted_by_tracker(endpoints, url),
            attachment_timeout(kind),
            limit,
        )
        .await
        .map_err(|error| match (kind, error.status) {
            (AttachmentKind::File, Some(401 | 403 | 404)) => IssueError::new(
                ExternalIssueErrorKind::AccessDenied,
                "GitHub serves this file in a private repository only on its site, to signed-in people; open it there",
            ),
            _ => IssueError::from(error),
        })?;
    let mut prefix = Vec::new();
    while prefix.len() < RECOGNITION_PREFIX {
        match body.chunk().await? {
            Some(chunk) => prefix.extend_from_slice(&chunk),
            None => break,
        }
    }
    let unrecognized = |what: &str| IssueError::new(ExternalIssueErrorKind::InvalidReference, what);
    let content_type = match kind {
        AttachmentKind::Image => image_type(&prefix)
            .ok_or_else(|| unrecognized("the content is not a PNG, JPEG, GIF or WebP image"))?
            .to_string(),
        AttachmentKind::Video => video_type(&prefix)
            .ok_or_else(|| unrecognized("the content is not an MP4, QuickTime or WebM video"))?
            .to_string(),
        AttachmentKind::File => file_type(body.content_type.as_deref(), attachment.name.as_deref())
            .ok_or_else(|| unrecognized("GitHub answered with a web page instead of the file"))?,
    };
    Ok(AttachmentContent {
        kind,
        content_type,
        length: body.length,
        limit,
        prefix: (!prefix.is_empty()).then(|| bytes::Bytes::from(prefix)),
        body,
    })
}

/// A file's type as GitHub declares it; `None` for a web page standing in
/// for a file that is not one (GitHub's sign-in page).
fn file_type(declared: Option<&str>, name: Option<&str>) -> Option<String> {
    let essence = declared
        .and_then(|value| value.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| {
            value.len() <= 100
                && value.split_once('/').is_some_and(|(kind, sub)| {
                    !kind.is_empty()
                        && !sub.is_empty()
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"/.+-".contains(&b))
                })
        });
    let html_name = name.is_some_and(|name| {
        let name = name.to_ascii_lowercase();
        name.ends_with(".html") || name.ends_with(".htm")
    });
    match essence {
        Some(value) if value == "text/html" && !html_name => None,
        Some(value) => Some(value),
        None => Some("application/octet-stream".to_string()),
    }
}

pub(in crate::providers) fn rendered_text(
    endpoints: &ProviderEndpoints,
    markdown: &str,
    html: &str,
) -> RenderedText {
    RenderedText {
        version: text_version(markdown),
        attachments: rendered_attachments(endpoints, html),
    }
}

/// The attachments GitHub's sanitized rendering shows, in order: images
/// (emoji excluded, which are images but not the text's own), videos, and
/// links to files uploaded to the tracker.
fn rendered_attachments(endpoints: &ProviderEndpoints, html: &str) -> Vec<RenderedAttachment> {
    let mut position = 0;
    let mut found = Vec::new();
    for (tag, attributes) in tags(html, &["img", "video", "a"]) {
        let (kind, address, name) = match tag {
            "img" => {
                if attribute(&attributes, "class")
                    .is_some_and(|class| class.split_ascii_whitespace().any(|c| c == "emoji"))
                {
                    continue;
                }
                let alt = attribute(&attributes, "alt")
                    .map(str::trim)
                    .filter(|alt| !alt.is_empty())
                    .map(str::to_owned);
                (AttachmentKind::Image, attribute(&attributes, "src"), alt)
            }
            "video" => (AttachmentKind::Video, attribute(&attributes, "src"), None),
            _ => {
                let Some(name) = attribute(&attributes, "href")
                    .and_then(|href| uploaded_file_name(endpoints, href))
                else {
                    continue;
                };
                (
                    AttachmentKind::File,
                    attribute(&attributes, "href"),
                    Some(name),
                )
            }
        };
        position += 1;
        let source = address.and_then(|src| Url::parse(src).ok());
        let (unavailable, external_url, source) = match source {
            Some(url) if hosted_by_tracker(endpoints, &url) => (None, None, Some(url)),
            // An address that may carry a signature (a query, or a host
            // under the tracker's own, as Enterprise Server's media host is)
            // never leaves G5 (`g5.attachments.github`).
            Some(url)
                if matches!(url.scheme(), "http" | "https")
                    && !may_carry_signature(endpoints, &url) =>
            {
                (
                    Some("the attachment is hosted outside GitHub; fetch it from its address"),
                    Some(url.to_string()),
                    None,
                )
            }
            Some(url) if matches!(url.scheme(), "http" | "https") => (
                Some("the attachment is hosted where G5 does not fetch from"),
                None,
                None,
            ),
            _ => (Some("the attachment has no usable address"), None, None),
        };
        found.push(RenderedAttachment {
            position,
            kind,
            name,
            unavailable,
            external_url,
            source,
        });
    }
    found
}

/// The file name of a link to a file uploaded to the tracker:
/// `<web>/user-attachments/files/<id>/<name>`, or the older
/// `<web>/<owner>/<repo>/files/<id>/<name>`.
fn uploaded_file_name(endpoints: &ProviderEndpoints, href: &str) -> Option<String> {
    let path = endpoints.exact_web_path(href)?;
    let name = match path.as_slice() {
        [area, files, id, name] if area == "user-attachments" && files == "files" => {
            id.bytes().all(|b| b.is_ascii_digit()).then_some(name)
        }
        [_, _, files, id, name] if files == "files" => {
            id.bytes().all(|b| b.is_ascii_digit()).then_some(name)
        }
        _ => None,
    }?;
    let decoded = percent_decode(name)?;
    (!decoded.trim().is_empty() && !decoded.contains(['/', '\\'])).then_some(decoded)
}

/// A path segment's text, `%XX` escapes decoded; `None` unless it is UTF-8.
fn percent_decode(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&high), Some(&low)) if hex(high).is_some() && hex(low).is_some() => {
                out.push(u8::try_from(hex(high)? * 16 + hex(low)?).ok()?);
                i += 3;
            }
            (byte, _, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Whether an address outside GitHub's hosts might be a signed link: one
/// with a query or fragment, or on a host under the tracker's web host.
fn may_carry_signature(endpoints: &ProviderEndpoints, url: &Url) -> bool {
    url.query().is_some()
        || url.fragment().is_some()
        || matches!(
            (url.host_str(), endpoints.web.host_str()),
            (Some(host), Some(web)) if host.ends_with(&format!(".{web}"))
        )
}

/// GitHub's own hosts for issue attachments: the tracker's web origin and,
/// on github.com, GitHub's content hosts (uploads, and camo for images
/// linked from elsewhere). No userinfo, and HTTPS unless the web origin
/// itself is plain HTTP.
fn hosted_by_tracker(endpoints: &ProviderEndpoints, url: &Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let web = &endpoints.web;
    if url.origin() == web.origin() {
        return true;
    }
    url.scheme() == "https"
        && url.port().is_none()
        && web.host_str() == Some("github.com")
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".githubusercontent.com"))
}

/// Each opening tag named in `names`, with its attributes, in document
/// order. GitHub's rendering is sanitized HTML, so text that looks like a
/// tag is escaped and every `<name` opens one.
fn tags<'n>(html: &str, names: &[&'n str]) -> Vec<(&'n str, Attributes)> {
    let bytes = html.as_bytes();
    let mut tags = Vec::new();
    let mut at = 0;
    while let Some(found) = bytes[at..].iter().position(|b| *b == b'<') {
        let start = at + found + 1;
        let name_end = bytes[start..]
            .iter()
            .position(|b| !b.is_ascii_alphanumeric())
            .map_or(bytes.len(), |offset| start + offset);
        at = name_end.max(start);
        let Some(name) = names
            .iter()
            .find(|name| html[start..name_end].eq_ignore_ascii_case(name))
        else {
            continue;
        };
        if !bytes
            .get(name_end)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>')
        {
            continue;
        }
        let (attributes, end) = parse_attributes(html, name_end);
        tags.push((*name, attributes));
        at = end;
    }
    tags
}

async fn rendered_get<T: serde::de::DeserializeOwned>(
    client: &RepositoryClient,
    url: Url,
) -> Result<T, IssueError> {
    Ok(client
        .send_media(
            READ,
            reqwest::Method::GET,
            url,
            Payload::Empty,
            &[],
            INTERACTIVE_TIMEOUT,
            Some(FULL_JSON),
        )
        .await?
        .json()?)
}

fn rendered_comment(
    client: &RepositoryClient,
    mut raw: RawComment,
) -> Result<RenderedComment, IssueError> {
    let html = raw.body_html.take().unwrap_or_default();
    let comment = owned_comment(client, raw)?;
    let text = rendered_text(&client.grant.endpoints, &comment.body, &html);
    Ok(RenderedComment { comment, text })
}

type Attributes = Vec<(String, String)>;

fn attribute<'a>(attributes: &'a Attributes, name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// Attributes from `start` to the tag's `>`, and the index after it.
fn parse_attributes(html: &str, start: usize) -> (Attributes, usize) {
    let bytes = html.as_bytes();
    let mut attributes = Vec::new();
    let mut i = start;
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() {
            return (attributes, i);
        }
        if bytes[i] == b'>' {
            return (attributes, i + 1);
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && !matches!(bytes[i], b'=' | b'>' | b'/')
        {
            i += 1;
        }
        let name = html[name_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if bytes.get(i) == Some(&b'=') {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let (raw, next) = match bytes.get(i) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let begin = i + 1;
                    let end = bytes[begin..]
                        .iter()
                        .position(|b| *b == quote)
                        .map_or(bytes.len(), |offset| begin + offset);
                    (&html[begin..end], (end + 1).min(bytes.len()))
                }
                _ => {
                    let begin = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    (&html[begin..i], i)
                }
            };
            value = decode_entities(raw);
            i = next;
        }
        if !name.is_empty() && attribute(&attributes, &name).is_none() {
            attributes.push((name, value));
        }
    }
}

/// Decode the character references an attribute value may carry.
fn decode_entities(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
            let entity = &rest[1..end];
            let character = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                    .and_then(char::from_u32),
            };
            character.map(|character| (character, end))
        });
        match decoded {
            Some((character, end)) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScmProvider;

    fn github() -> ProviderEndpoints {
        ProviderEndpoints::new(
            ScmProvider::GitHub,
            "https://api.github.com",
            "https://github.com",
        )
        .unwrap()
    }

    #[test]
    fn attachments_are_listed_in_order_by_kind_without_emoji() {
        let html = r#"<p><a href="https://private-user-images.githubusercontent.com/1/2-a.png?jwt=x&amp;y=1"><img src="https://private-user-images.githubusercontent.com/1/2-a.png?jwt=x&amp;y=1" alt="Request approvals help" style="max-width: 100%;"></a>
<g-emoji class="g-emoji"><img class="emoji" alt=":+1:" src="https://github.githubassets.com/images/icons/emoji/unicode/1f44d.png"></g-emoji>
<code>&lt;img src="https://github.com/nope.png"&gt;</code>
<details><summary>demo.mp4</summary><video src="https://private-user-images.githubusercontent.com/1/3-b.mp4?jwt=v" controls="controls"></video></details>
<a href="https://github.com/user-attachments/files/1234/build%20log.txt">build log.txt</a>
<a href="https://github.com/acme/web/files/99/old.pdf">old.pdf</a>
<a href="https://github.com/acme/web/issues/3">not a file</a><abbr title="x">y</abbr>
<IMG SRC='https://camo.githubusercontent.com/abc/def' ALT=mode>
<img src="https://example.com/outside.png">
<img alt="no source"></p>"#;
        let found = rendered_attachments(&github(), html);
        let kinds: Vec<_> = found.iter().map(|a| a.kind).collect();
        use AttachmentKind::{File, Image, Video};
        assert_eq!(kinds, [Image, Video, File, File, Image, Image, Image]);
        assert_eq!(
            found.iter().map(|a| a.position).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(found[0].name.as_deref(), Some("Request approvals help"));
        assert_eq!(
            found[0].source.as_ref().map(Url::as_str),
            Some("https://private-user-images.githubusercontent.com/1/2-a.png?jwt=x&y=1")
        );
        assert_eq!(found[1].unavailable, None);
        assert_eq!(found[2].name.as_deref(), Some("build log.txt"));
        assert_eq!(found[3].name.as_deref(), Some("old.pdf"));
        assert_eq!(found[4].name.as_deref(), Some("mode"));
        assert!(found[5].unavailable.is_some());
        assert_eq!(
            found[5].external_url.as_deref(),
            Some("https://example.com/outside.png")
        );
        assert!(found[5].source.is_none());
        assert!(found[6].unavailable.is_some());
        // The signed link never appears in a debug rendering.
        assert!(!format!("{found:?}").contains("jwt"));
    }

    #[test]
    fn an_outside_address_that_may_be_signed_never_leaves_g5() {
        let enterprise = ProviderEndpoints::new(
            ScmProvider::GitHub,
            "https://ghe.example.com/api/v3",
            "https://ghe.example.com",
        )
        .unwrap();
        let html = r#"<img src="https://cdn.example.org/a.png?sig=secret">
<img src="https://media.ghe.example.com/user/1/files/x.png">
<img src="https://cdn.example.org/b.png">"#;
        let found = rendered_attachments(&enterprise, html);
        assert_eq!(found.len(), 3);
        for withheld in &found[..2] {
            assert!(withheld.unavailable.is_some());
            assert_eq!(withheld.external_url, None);
            assert!(withheld.source.is_none());
        }
        assert_eq!(
            found[2].external_url.as_deref(),
            Some("https://cdn.example.org/b.png")
        );
    }

    #[test]
    fn only_github_s_own_hosts_serve_attachments() {
        let hosted = |url: &str| hosted_by_tracker(&github(), &Url::parse(url).unwrap());
        assert!(hosted("https://github.com/user-attachments/assets/1234"));
        assert!(hosted(
            "https://private-user-images.githubusercontent.com/1/2.png?jwt=x"
        ));
        assert!(hosted("https://camo.githubusercontent.com/a/b"));
        assert!(!hosted(
            "http://private-user-images.githubusercontent.com/1/2.png"
        ));
        assert!(!hosted("https://githubusercontent.com.evil.example/1.png"));
        assert!(!hosted("https://evilgithubusercontent.com/1.png"));
        assert!(!hosted(
            "https://user:pw@github.com/user-attachments/assets/1"
        ));
        assert!(!hosted("https://api.github.com/user-attachments/assets/1"));
        assert!(!hosted(
            "https://private-user-images.githubusercontent.com:8443/1.png"
        ));
        // Enterprise Server: only its own web origin.
        let enterprise = ProviderEndpoints::new(
            ScmProvider::GitHub,
            "https://ghe.example/api/v3",
            "https://ghe.example",
        )
        .unwrap();
        let url = |value: &str| Url::parse(value).unwrap();
        assert!(hosted_by_tracker(
            &enterprise,
            &url("https://ghe.example/user-attachments/assets/1")
        ));
        assert!(!hosted_by_tracker(
            &enterprise,
            &url("https://private-user-images.githubusercontent.com/1.png")
        ));
    }

    #[test]
    fn a_file_keeps_github_s_type_unless_it_is_a_stand_in_page() {
        assert_eq!(
            file_type(Some("application/pdf"), Some("a.pdf")).as_deref(),
            Some("application/pdf")
        );
        assert_eq!(
            file_type(Some("text/plain; charset=utf-8"), Some("log.txt")).as_deref(),
            Some("text/plain")
        );
        assert_eq!(
            file_type(Some("text/html; charset=utf-8"), Some("log.txt")),
            None
        );
        assert_eq!(
            file_type(Some("text/html"), Some("report.HTML")).as_deref(),
            Some("text/html")
        );
        assert_eq!(
            file_type(Some("bogus value"), None).as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(
            file_type(None, None).as_deref(),
            Some("application/octet-stream")
        );
    }

    #[test]
    fn character_references_in_attributes_are_decoded() {
        assert_eq!(decode_entities("a&amp;b&#39;c&#x27;d&quot;"), "a&b'c'd\"");
        assert_eq!(decode_entities("a & b &unknown; &"), "a & b &unknown; &");
    }

    #[test]
    fn the_version_follows_the_markdown() {
        assert_eq!(text_version("same"), text_version("same"));
        assert_ne!(text_version("same"), text_version("changed"));
        assert_eq!(text_version("").len(), 16);
    }
}
