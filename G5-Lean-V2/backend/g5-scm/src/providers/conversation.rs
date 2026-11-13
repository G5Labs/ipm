//! Writes to a pull request's conversation: discussion comments, review-thread
//! replies and thread resolution. Callers record and reconcile submissions;
//! a failure's `effect` says whether GitHub may hold the write.
use super::*;
use serde_json::{json, Value};

/// A comment GitHub holds, as G5 links to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostedPullComment {
    /// The REST (database) ID.
    pub id: u64,
    pub url: String,
}

/// A review thread's current state, read before a write that names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewThreadState {
    pub id: String,
    pub is_resolved: bool,
    pub pull_number: i64,
    /// The thread's comment carrying a caller's marker, when asked for.
    pub marked: Option<PostedPullComment>,
}

/// Comments searched when reconciling a discussion comment.
const MARKER_PAGES: u32 = 10;
/// Thread replies searched when reconciling a reply; GitHub threads are short.
const THREAD_COMMENTS: usize = 100;

impl RepositoryClient {
    fn posted(&self, id: &Value, url: &Value) -> Option<PostedPullComment> {
        let url = url
            .as_str()
            .filter(|url| self.grant.endpoints.web_path(url).is_some())?;
        Some(PostedPullComment {
            id: id.as_u64().filter(|id| *id > 0)?,
            url: url.to_owned(),
        })
    }

    /// `POST repos/{owner}/{name}/issues/{number}/comments`: a pull request's
    /// discussion is its issue thread.
    pub async fn post_pull_request_comment(
        &self,
        number: i64,
        body: &str,
    ) -> Result<PostedPullComment, ProviderError> {
        const OP: &str = "post pull request comment";
        self.require(OP, &[RepositoryCapability::PullRequestWrite])?;
        let response = self
            .request(
                OP,
                reqwest::Method::POST,
                self.repo_url(&["issues", &number.to_string(), "comments"])?,
                Payload::Json(json!({ "body": body })),
            )
            .await?;
        let comment: Value = response.json()?;
        self.posted(&comment["id"], &comment["html_url"])
            .ok_or_else(|| {
                response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The accepted comment response was not understood",
                )
            })
    }

    /// The discussion comment created since `since` (RFC 3339) whose body
    /// ends with `marker`, searching a bounded number of pages.
    pub async fn find_marked_pull_request_comment(
        &self,
        number: i64,
        since: &str,
        marker: &str,
    ) -> Result<Option<PostedPullComment>, ProviderError> {
        const OP: &str = "find pull request comment";
        self.require(
            OP,
            &[
                RepositoryCapability::PullRequestRead,
                RepositoryCapability::PullRequestWrite,
            ],
        )?;
        let number =
            u64::try_from(number).map_err(|_| ProviderError::invalid(OP, "Invalid PR number"))?;
        for page in 1..=MARKER_PAGES {
            let (comments, more) = issues::comment_page(
                self,
                OP,
                number,
                &[
                    ("since", since.to_owned()),
                    ("per_page", "100".into()),
                    ("page", page.to_string()),
                ],
                REPOSITORY_TIMEOUT,
            )
            .await?;
            if let Some(found) = comments.iter().find_map(|c| {
                c.body
                    .as_deref()
                    .is_some_and(|body| body.trim_end().ends_with(marker))
                    .then(|| self.posted(&json!(c.id), &json!(c.html_url)))
                    .flatten()
            }) {
                return Ok(Some(found));
            }
            if !more {
                break;
            }
        }
        Ok(None)
    }

    /// Read a review thread of this repository. A thread elsewhere, or a node
    /// that is not a review thread, is refused as not found. With `marker`,
    /// the thread's latest comments are searched for it.
    pub async fn review_thread(
        &self,
        thread_id: &str,
        marker: Option<&str>,
    ) -> Result<ReviewThreadState, ProviderError> {
        const OP: &str = "read review thread";
        self.require(
            OP,
            &[
                RepositoryCapability::PullRequestRead,
                RepositoryCapability::PullRequestWrite,
            ],
        )?;
        let query = format!(
            "query($id:ID!){{node(id:$id){{...on PullRequestReviewThread{{id isResolved \
             pullRequest{{number repository{{nameWithOwner}}}} \
             comments(last:{THREAD_COMMENTS}){{nodes{{databaseId body url}}}}}}}}}}"
        );
        let (response, data) = self
            .graphql_request(OP, &query, json!({"id": thread_id}), false)
            .await?;
        let thread = &data["node"];
        let not_found = || {
            let mut error = response.failure(
                ProviderErrorKind::Rejected,
                "The review thread was not found in the selected repository",
            );
            error.status = Some(404);
            error
        };
        if thread["id"].as_str() != Some(thread_id) {
            return Err(not_found());
        }
        let in_repository = thread["pullRequest"]["repository"]["nameWithOwner"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(self.repository_name().unwrap_or("")));
        if !in_repository {
            return Err(not_found());
        }
        let (Some(is_resolved), Some(pull_number), Some(comments)) = (
            thread["isResolved"].as_bool(),
            thread["pullRequest"]["number"].as_i64(),
            thread["comments"]["nodes"].as_array(),
        ) else {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid review thread",
            ));
        };
        let marked = marker.and_then(|marker| {
            comments.iter().find_map(|c| {
                c["body"]
                    .as_str()
                    .is_some_and(|body| body.trim_end().ends_with(marker))
                    .then(|| self.posted(&c["databaseId"], &c["url"]))
                    .flatten()
            })
        });
        Ok(ReviewThreadState {
            id: thread_id.to_owned(),
            is_resolved,
            pull_number,
            marked,
        })
    }

    /// Reply in a review thread (`addPullRequestReviewThreadReply`).
    pub async fn reply_to_review_thread(
        &self,
        thread_id: &str,
        body: &str,
    ) -> Result<PostedPullComment, ProviderError> {
        const OP: &str = "reply to review thread";
        self.require(OP, &[RepositoryCapability::PullRequestWrite])?;
        let (response, data) = self
            .graphql_request(
                OP,
                "mutation($id:ID!,$body:String!){addPullRequestReviewThreadReply(input:\
                 {pullRequestReviewThreadId:$id,body:$body}){comment{databaseId url}}}",
                json!({"id": thread_id, "body": body}),
                true,
            )
            .await?;
        let comment = &data["addPullRequestReviewThreadReply"]["comment"];
        self.posted(&comment["databaseId"], &comment["url"])
            .ok_or_else(|| {
                response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The accepted reply response was not understood",
                )
            })
    }

    /// Resolve or unresolve a review thread; returns the state GitHub reports.
    /// Repeating it is harmless.
    pub async fn set_review_thread_resolution(
        &self,
        thread_id: &str,
        resolved: bool,
    ) -> Result<bool, ProviderError> {
        const OP: &str = "resolve review thread";
        self.require(OP, &[RepositoryCapability::PullRequestWrite])?;
        let field = if resolved {
            "resolveReviewThread"
        } else {
            "unresolveReviewThread"
        };
        let (response, data) = self
            .graphql_request(
                OP,
                &format!(
                    "mutation($id:ID!){{{field}(input:{{threadId:$id}}){{thread{{id isResolved}}}}}}"
                ),
                json!({"id": thread_id}),
                true,
            )
            .await?;
        data[field]["thread"]["isResolved"]
            .as_bool()
            .ok_or_else(|| {
                response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The accepted resolution response was not understood",
                )
            })
    }
}
