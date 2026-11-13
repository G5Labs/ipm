//! The texts of a GitHub pull request in GitHub's rendering, for its
//! attachments (`g5.attachments.github`).
//!
//! The same parser and credential-free fetch as an issue's
//! ([`super::issues::attachments`]) serve a pull request: only the texts
//! read and the grant differ. A pull request's texts are its description,
//! its conversation comments, its reviews and its review comments, each in
//! the `full+json` representation, whose rendering carries the signed links
//! of private images and videos.

use g5_contracts::error::ExternalIssueErrorKind;
use serde::Deserialize;
use url::Url;

use super::issues::{
    owned_comment, rendered_text, user, GithubIssueUser, IssueError, RawComment, RawUser,
    RenderedText, FULL_JSON,
};
use super::*;

/// Texts of one kind read with a listing (GitHub's largest page).
const TEXT_PAGE: usize = 100;
const READ_TEXTS: &str = "read pull request texts";

/// Which text of a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullTextKind {
    Description,
    /// A conversation comment.
    Comment,
    /// A review's own text.
    Review,
    /// A comment on a line of the diff.
    ReviewComment,
}

/// One text of a pull request and the attachments its rendering shows.
#[derive(Debug, Clone)]
pub struct RenderedPullText {
    pub kind: PullTextKind,
    /// GitHub's id of the comment or review; `None` for the description.
    pub id: Option<String>,
    pub author: Option<GithubIssueUser>,
    pub created_at: Option<String>,
    /// The file a review comment is on.
    pub path: Option<String>,
    pub text: RenderedText,
}

/// A pull request's texts, for a listing.
#[derive(Debug, Clone)]
pub struct RenderedPull {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub description: RenderedPullText,
    /// Conversation comments, reviews with a text and review comments,
    /// oldest first; empty unless asked for.
    pub discussion: Vec<RenderedPullText>,
    /// Some kind had more than one page.
    pub discussion_truncated: bool,
}

#[derive(Debug, Deserialize)]
struct RawPullText {
    number: u64,
    title: String,
    html_url: String,
    #[serde(default)]
    user: Option<RawUser>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    body_html: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

/// A review, or a review comment: both name their pull request.
#[derive(Debug, Deserialize)]
struct RawReviewText {
    id: u64,
    #[serde(default)]
    user: Option<RawUser>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    body_html: Option<String>,
    /// Reviews: when submitted; review comments use `created_at`.
    #[serde(default)]
    submitted_at: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    pull_request_url: Option<String>,
}

impl RepositoryClient {
    /// The pull request's description and, with `discussion`, the first
    /// page of each of its other texts.
    pub async fn read_rendered_pull(
        &self,
        number: u64,
        discussion: bool,
    ) -> Result<RenderedPull, IssueError> {
        self.require(READ_TEXTS, &[RepositoryCapability::PullRequestRead])?;
        let (pull, description) = self.rendered_description(number).await?;
        let mut texts = Vec::new();
        let mut truncated = false;
        if discussion {
            let n = number.to_string();
            let page = [("per_page", TEXT_PAGE.to_string())];
            let (comments, more): (Vec<RawComment>, bool) = self
                .rendered_page(self.repo_url(&["issues", &n, "comments"])?, &page)
                .await?;
            truncated |= more;
            for raw in comments {
                texts.push(self.rendered_comment(number, raw)?);
            }
            let (reviews, more): (Vec<RawReviewText>, bool) = self
                .rendered_page(self.repo_url(&["pulls", &n, "reviews"])?, &page)
                .await?;
            truncated |= more;
            for raw in reviews {
                // A review without a text (an approval alone) shows nothing.
                if raw
                    .body
                    .as_deref()
                    .is_some_and(|body| !body.trim().is_empty())
                {
                    texts.push(self.rendered_review(number, PullTextKind::Review, raw)?);
                }
            }
            let (comments, more): (Vec<RawReviewText>, bool) = self
                .rendered_page(self.repo_url(&["pulls", &n, "comments"])?, &page)
                .await?;
            truncated |= more;
            for raw in comments {
                texts.push(self.rendered_review(number, PullTextKind::ReviewComment, raw)?);
            }
            texts.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        }
        Ok(RenderedPull {
            number,
            title: pull.title,
            url: pull.html_url,
            description,
            discussion: texts,
            discussion_truncated: truncated,
        })
    }

    /// One text of the pull request, read again for a fetch: it must still
    /// be on pull request `number`.
    pub async fn read_rendered_pull_text(
        &self,
        number: u64,
        kind: PullTextKind,
        id: Option<&str>,
    ) -> Result<RenderedPullText, IssueError> {
        self.require(READ_TEXTS, &[RepositoryCapability::PullRequestRead])?;
        let id = match (kind, id) {
            (PullTextKind::Description, _) => {
                return Ok(self.rendered_description(number).await?.1);
            }
            (_, Some(id)) if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) => id,
            _ => {
                return Err(IssueError::invalid_reference(
                    "Invalid pull request text identity",
                ))
            }
        };
        let n = number.to_string();
        match kind {
            PullTextKind::Comment => {
                let raw: RawComment = self
                    .rendered_one(self.repo_url(&["issues", "comments", id])?)
                    .await?;
                self.rendered_comment(number, raw)
            }
            PullTextKind::Review => {
                let raw: RawReviewText = self
                    .rendered_one(self.repo_url(&["pulls", &n, "reviews", id])?)
                    .await?;
                self.rendered_review(number, kind, raw)
            }
            PullTextKind::ReviewComment => {
                let raw: RawReviewText = self
                    .rendered_one(self.repo_url(&["pulls", "comments", id])?)
                    .await?;
                self.rendered_review(number, kind, raw)
            }
            PullTextKind::Description => unreachable!("handled above"),
        }
    }

    async fn rendered_description(
        &self,
        number: u64,
    ) -> Result<(RawPullText, RenderedPullText), IssueError> {
        let mut raw: RawPullText = self
            .rendered_one(self.repo_url(&["pulls", &number.to_string()])?)
            .await?;
        let (owner, name) = repository_name(self.repository_name()?)?;
        let expected = self
            .grant
            .endpoints
            .web(&[owner, name, "pull", &number.to_string()]);
        if raw.number != number || self.grant.endpoints.web_path(&raw.html_url).is_none() {
            return Err(IssueError::unavailable(
                "GitHub returned an invalid pull request identity or URL",
            ));
        }
        raw.html_url = expected.to_string();
        let html = raw.body_html.take().unwrap_or_default();
        let description = RenderedPullText {
            kind: PullTextKind::Description,
            id: None,
            author: raw.user.take().map(user),
            created_at: raw.created_at.clone(),
            path: None,
            text: rendered_text(
                &self.grant.endpoints,
                raw.body.as_deref().unwrap_or_default(),
                &html,
            ),
        };
        Ok((raw, description))
    }

    fn rendered_comment(
        &self,
        number: u64,
        mut raw: RawComment,
    ) -> Result<RenderedPullText, IssueError> {
        let html = raw.body_html.take().unwrap_or_default();
        let comment = owned_comment(self, raw)?;
        if comment.issue_number != Some(number) {
            return Err(not_on_pull());
        }
        Ok(RenderedPullText {
            kind: PullTextKind::Comment,
            text: rendered_text(&self.grant.endpoints, &comment.body, &html),
            id: Some(comment.id),
            author: comment.author,
            created_at: Some(comment.created_at),
            path: None,
        })
    }

    fn rendered_review(
        &self,
        number: u64,
        kind: PullTextKind,
        raw: RawReviewText,
    ) -> Result<RenderedPullText, IssueError> {
        let endpoints = &self.grant.endpoints;
        if raw
            .html_url
            .as_deref()
            .is_some_and(|url| endpoints.web_path(url).is_none())
        {
            return Err(IssueError::unavailable(
                "GitHub returned a review URL outside the selected tracker",
            ));
        }
        // Both kinds name their pull request: `…/pulls/{number}`.
        let on_pull = raw
            .pull_request_url
            .as_deref()
            .and_then(|url| endpoints.api_path(url))
            .is_some_and(|path| {
                let tail: Vec<&str> = path.iter().rev().take(2).map(String::as_str).collect();
                tail == [number.to_string().as_str(), "pulls"]
            });
        if !on_pull {
            return Err(not_on_pull());
        }
        let body = raw.body.unwrap_or_default();
        Ok(RenderedPullText {
            kind,
            id: Some(raw.id.to_string()),
            author: raw.user.map(user),
            created_at: raw.submitted_at.or(raw.created_at),
            path: raw.path.filter(|_| kind == PullTextKind::ReviewComment),
            text: rendered_text(
                endpoints,
                &body,
                raw.body_html.as_deref().unwrap_or_default(),
            ),
        })
    }

    async fn rendered_one<T: serde::de::DeserializeOwned>(
        &self,
        url: Url,
    ) -> Result<T, IssueError> {
        Ok(self
            .send_media(
                READ_TEXTS,
                reqwest::Method::GET,
                url,
                Payload::Empty,
                &[],
                REPOSITORY_TIMEOUT,
                Some(FULL_JSON),
            )
            .await?
            .json()?)
    }

    async fn rendered_page<T: serde::de::DeserializeOwned>(
        &self,
        url: Url,
        params: &[(&str, String)],
    ) -> Result<(Vec<T>, bool), IssueError> {
        let response = self
            .send_media(
                READ_TEXTS,
                reqwest::Method::GET,
                query(url, params),
                Payload::Empty,
                &[],
                REPOSITORY_TIMEOUT,
                Some(FULL_JSON),
            )
            .await?;
        Ok((response.json()?, response.has_next_page()))
    }
}

fn not_on_pull() -> IssueError {
    IssueError::new(
        ExternalIssueErrorKind::NotFound,
        "the text is not on this pull request",
    )
}
