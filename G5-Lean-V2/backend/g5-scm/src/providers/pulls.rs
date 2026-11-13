//! Repository-scoped pull request protocol; publication and recovery stay with callers.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct GithubPull {
    pub number: i64,
    pub node_id: Option<String>,
    pub html_url: String,
    pub state: String,
    pub merged: Option<bool>,
    pub merge_commit_sha: Option<String>,
    /// Whether GitHub can rebase the head onto the base. `None` while GitHub
    /// is still computing it, or in minimal provider fixtures.
    #[serde(default)]
    pub rebaseable: Option<bool>,
    /// GitHub's `mergeable_state`: `blocked` while branch protection or
    /// rulesets are unmet, `unknown` while GitHub is still computing it.
    /// `None` in minimal provider fixtures.
    #[serde(default)]
    pub mergeable_state: Option<String>,
    pub head: GithubPullRef,
    pub base: GithubPullRef,
}
#[derive(Debug, Deserialize)]
pub struct GithubPullRef {
    pub sha: String,
    /// Branch name; absent only in minimal provider fixtures.
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct GithubMergeResponse {
    pub merged: bool,
    pub sha: Option<String>,
    pub message: Option<String>,
}
/// Optional live PR detail beyond the pull request itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct PullRequestReadScope {
    /// Discussion comments, inline review threads and all reviews.
    pub comments: bool,
    /// `reviewDecision` and each reviewer's latest review.
    pub reviews: bool,
}
pub struct CreatePullRequest<'a> {
    pub title: &'a str,
    pub head: &'a str,
    pub base: &'a str,
    pub body: &'a str,
}
impl ProviderEndpoints {
    pub fn select_pull_request(
        &self,
        selector: &str,
        repo: Option<&str>,
    ) -> Result<(i64, Option<String>), ProviderError> {
        const OP: &str = "select pull request";
        self.github(OP)?;
        let explicit = repo
            .map(|name| repository_name(name).map(|_| name.to_owned()))
            .transpose()?;
        let (number, from_url) = if selector.contains("://") {
            let path = self.exact_web_path(selector);
            let Some([owner, repo, kind, number]) = path.as_deref() else {
                return Err(ProviderError::invalid(
                    OP,
                    "Use the selected provider instance's OWNER/REPO/pull/NUMBER URL",
                ));
            };
            if kind != "pull" {
                return Err(ProviderError::invalid(
                    OP,
                    "Use the selected provider instance's OWNER/REPO/pull/NUMBER URL",
                ));
            }
            let name = format!("{owner}/{repo}");
            repository_name(&name)?;
            (number.clone(), Some(name))
        } else {
            (
                selector.strip_prefix('#').unwrap_or(selector).to_owned(),
                None,
            )
        };
        let number = number
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| {
                ProviderError::invalid(
                    OP,
                    "PR selector must be a positive number or provider PR URL",
                )
            })?;
        if explicit
            .as_ref()
            .zip(from_url.as_ref())
            .is_some_and(|(a, b)| !a.eq_ignore_ascii_case(b))
        {
            return Err(ProviderError::invalid(
                OP,
                "--repo does not match the PR URL",
            ));
        }
        Ok((number, from_url.or(explicit)))
    }
}
impl RepositoryClient {
    fn pull(&self, response: &Response, number: Option<i64>) -> Result<GithubPull, ProviderError> {
        let pull: GithubPull = response.json()?;
        // A same-origin redirect means the provider moved the repository
        // (renamed or transferred): its PR URL names the new owner/repository.
        // It must still be a pull request on this instance with this number.
        let expected_repository = if response.redirected {
            None
        } else {
            Some(self.repository_name()?)
        };
        let valid_url = self
            .grant
            .endpoints
            .select_pull_request(&pull.html_url, expected_repository)
            .is_ok_and(|(n, _)| n == pull.number);
        if pull.number <= 0
            || number.is_some_and(|n| n != pull.number)
            || !matches!(pull.state.as_str(), "open" | "closed")
            || !valid_url
        {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid pull request identity or URL",
            ));
        }
        Ok(pull)
    }
    pub async fn find_open_pull_request(
        &self,
        source: &str,
        target: &str,
    ) -> Result<Option<GithubPull>, ProviderError> {
        const OP: &str = "find open pull request";
        self.require(
            OP,
            &[
                RepositoryCapability::PullRequestRead,
                RepositoryCapability::PullRequestWrite,
                RepositoryCapability::PullRequestMerge,
            ],
        )?;
        let (owner, _) = repository_name(self.repository_name()?)?;
        let response = self
            .request(
                OP,
                reqwest::Method::GET,
                query(
                    self.repo_url(&["pulls"])?,
                    &[
                        ("state", "open".into()),
                        ("base", target.into()),
                        ("head", format!("{owner}:{source}")),
                        ("per_page", "10".into()),
                    ],
                ),
                Payload::Empty,
            )
            .await?;
        let pulls: Vec<Value> = response.json()?;
        // The provider selection query preserves the existing first-match policy.
        pulls
            .into_iter()
            .next()
            .map(|value| {
                let pull: GithubPull = serde_json::from_value(value).map_err(|_| {
                    response.failure(
                        ProviderErrorKind::InvalidResponse,
                        "Invalid pull request response",
                    )
                })?;
                if pull.number <= 0
                    || !self
                        .grant
                        .endpoints
                        .select_pull_request(&pull.html_url, Some(self.repository_name()?))
                        .is_ok_and(|(n, _)| n == pull.number)
                {
                    return Err(response.failure(
                        ProviderErrorKind::InvalidResponse,
                        "Invalid pull request identity or URL",
                    ));
                }
                Ok(pull)
            })
            .transpose()
    }
    pub async fn pull_request(&self, number: i64) -> Result<GithubPull, ProviderError> {
        const OP: &str = "read pull request";
        self.require(
            OP,
            &[
                RepositoryCapability::PullRequestRead,
                RepositoryCapability::PullRequestWrite,
                RepositoryCapability::PullRequestMerge,
            ],
        )?;
        let response = self
            .request(
                OP,
                reqwest::Method::GET,
                self.repo_url(&["pulls", &number.to_string()])?,
                Payload::Empty,
            )
            .await?;
        self.pull(&response, Some(number))
    }
    pub async fn create_pull_request(
        &self,
        input: CreatePullRequest<'_>,
    ) -> Result<GithubPull, ProviderError> {
        const OP: &str = "create pull request";
        self.require(OP, &[RepositoryCapability::PullRequestWrite])?;
        let response = self.request(OP,reqwest::Method::POST,self.repo_url(&["pulls"])? ,Payload::Json(json!({"title":input.title,"head":input.head,"base":input.base,"body":input.body,"maintainer_can_modify":true}))).await?;
        self.pull(&response, None)
    }
    /// Replace an existing PR's title and/or body; omitted fields are kept.
    pub async fn update_pull_request(
        &self,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<GithubPull, ProviderError> {
        const OP: &str = "update pull request";
        self.require(OP, &[RepositoryCapability::PullRequestWrite])?;
        let mut fields = serde_json::Map::new();
        if let Some(title) = title {
            fields.insert("title".into(), json!(title));
        }
        if let Some(body) = body {
            fields.insert("body".into(), json!(body));
        }
        if fields.is_empty() {
            return Err(ProviderError::invalid(OP, "Nothing to update"));
        }
        let response = self
            .request(
                OP,
                reqwest::Method::PATCH,
                self.repo_url(&["pulls", &number.to_string()])?,
                Payload::Json(Value::Object(fields)),
            )
            .await?;
        self.pull(&response, Some(number))
    }
    pub async fn merge_pull_request(
        &self,
        number: i64,
        expected_head: &str,
    ) -> Result<GithubMergeResponse, ProviderError> {
        const OP: &str = "merge pull request";
        self.require(OP, &[RepositoryCapability::PullRequestMerge])?;
        if number <= 0
            || expected_head.len() != 40
            || !expected_head.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ProviderError::invalid(
                OP,
                "Merge requires a positive PR number and full expected head SHA",
            ));
        }
        let response = self
            .request(
                OP,
                reqwest::Method::PUT,
                self.repo_url(&["pulls", &number.to_string(), "merge"])?,
                Payload::Json(json!({"sha":expected_head,"merge_method":"rebase"})),
            )
            .await?;
        let mut merge: GithubMergeResponse = response.json()?;
        if !merge.merged {
            let mut error = response.failure(
                ProviderErrorKind::Rejected,
                merge
                    .message
                    .as_deref()
                    .unwrap_or("The provider refused the merge"),
            );
            // The protocol explicitly confirmed that the merge did not happen.
            error.effect = crate::FailureEffect::Refused;
            return Err(error);
        }
        merge.message = merge
            .message
            .map(|message| transport::redact(&message, std::slice::from_ref(&self.grant.token.0)));
        Ok(merge)
    }
    pub async fn published_branch_head(&self, branch: &str) -> Result<String, ProviderError> {
        const OP: &str = "read published branch head";
        self.require(OP, &[RepositoryCapability::ContentsRead])?;
        #[derive(Deserialize)]
        struct Ref {
            object: Object,
        }
        #[derive(Deserialize)]
        struct Object {
            sha: String,
        }
        let response = self
            .request(
                OP,
                reqwest::Method::GET,
                self.repo_url(&["git", "ref", "heads", branch])?,
                Payload::Empty,
            )
            .await?;
        let reference: Ref = response.json()?;
        Ok(reference.object.sha)
    }
    pub async fn read_pull_request(
        &self,
        number: i64,
        scope: PullRequestReadScope,
    ) -> Result<Value, ProviderError> {
        const OP: &str = "read pull request detail";
        self.require(OP, &[RepositoryCapability::PullRequestRead])?;
        // GitHub answers `mergeable: null` while it computes mergeability in the
        // background; re-read an open PR briefly before reporting it unknown
        // (g5.governance.pr.read.freshness). A missing key is not "computing".
        let mut attempt = 0;
        let (response, pull) = loop {
            let response = self
                .request(
                    OP,
                    reqwest::Method::GET,
                    self.repo_url(&["pulls", &number.to_string()])?,
                    Payload::Empty,
                )
                .await?;
            let pull: Value = response.json()?;
            let computing =
                pull["state"] == "open" && pull.get("mergeable").is_some_and(Value::is_null);
            attempt += 1;
            if !computing || attempt > MERGEABLE_REREADS {
                break (response, pull);
            }
            tokio::time::sleep(MERGEABLE_REREAD_DELAY).await;
        };
        if pull["number"].as_i64() != Some(number)
            || !pull["title"].is_string()
            || !matches!(pull["state"].as_str(), Some("open" | "closed"))
        {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid PR response",
            ));
        }
        let (owner, name) = repository_name(self.repository_name()?)?;
        let mut result = json!({
            "id": pull["node_id"].as_str(), "number": number, "repository": self.repository_name()?,
            "url": self.grant.endpoints.web(&[owner,name,"pull",&number.to_string()]).as_str(),
            "title": pull["title"], "body": pull["body"],
            "state": if pull["merged"].as_bool() == Some(true) { "MERGED" } else if pull["state"] == "closed" { "CLOSED" } else { "OPEN" },
            "author": {"login": pull["user"]["login"]},
            "isDraft": pull["draft"], "headRefName": pull["head"]["ref"], "headRefOid": pull["head"]["sha"],
            "baseRefName": pull["base"]["ref"], "baseRefOid": pull["base"]["sha"],
            "mergeable": pull["mergeable"], "mergeStateStatus": pull["mergeable_state"],
            "additions": pull["additions"], "deletions": pull["deletions"], "changedFiles": pull["changed_files"],
            "createdAt": pull["created_at"], "updatedAt": pull["updated_at"], "mergedAt": pull["merged_at"],
            "commentCount": pull["comments"], "reviewCommentCount": pull["review_comments"]
        });
        let node_id = || {
            pull["node_id"].as_str().ok_or_else(|| {
                response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider returned a PR without a node ID",
                )
            })
        };
        if scope.comments {
            let number = u64::try_from(number)
                .map_err(|_| ProviderError::invalid(OP, "Invalid pull request number"))?;
            let (comments, more) = issues::comment_page(
                self,
                OP,
                number,
                &[("per_page", "100".into()), ("page", "1".into())],
                REPOSITORY_TIMEOUT,
            )
            .await?;
            result["comments"] = json!(comments
                .into_iter()
                .map(|c| {
                    let url = c
                        .html_url
                        .filter(|url| self.grant.endpoints.web_path(url).is_some());
                    json!({"id":c.id,"author":{"login":c.user.map(|u| u.login)},"body":c.body,"createdAt":c.created_at,"url":url})
                })
                .collect::<Vec<_>>());
            result["commentsHasMore"] = json!(more);
            let id = node_id()?;
            let (threads, more) = self.review_connection(OP, id, REVIEW_THREADS).await?;
            result["reviewThreads"] = json!(threads
                .iter()
                .map(|t| review_thread(t, |url| self.web_url(url)))
                .collect::<Vec<_>>());
            result["reviewThreadsHasMore"] = json!(more);
            let (reviews, more) = self.review_connection(OP, id, REVIEWS).await?;
            result["reviews"] = json!(reviews
                .iter()
                .map(|r| review(r, |url| self.web_url(url)))
                .collect::<Vec<_>>());
            result["reviewsHasMore"] = json!(more);
        }
        if scope.reviews {
            let query = format!(
                "query($id:ID!){{node(id:$id){{...on PullRequest{{reviewDecision \
                 latestReviews(first:{LATEST_REVIEWS}){{pageInfo{{hasNextPage}} nodes{{{}}}}}}}}}}}",
                REVIEWS.nodes
            );
            let (response, pull) = self.graphql(OP, &query, json!({"id": node_id()?})).await?;
            let (Some(latest), Some(more)) = (
                pull["latestReviews"]["nodes"].as_array(),
                pull["latestReviews"]["pageInfo"]["hasNextPage"].as_bool(),
            ) else {
                return Err(response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider returned an invalid PR review summary",
                ));
            };
            result["reviewDecision"] = pull["reviewDecision"].clone();
            result["latestReviews"] = json!(latest
                .iter()
                .map(|r| review(r, |url| self.web_url(url)))
                .collect::<Vec<_>>());
            result["latestReviewsHasMore"] = json!(more);
        }
        Ok(result)
    }
    /// One GraphQL read of the selected pull request node.
    async fn graphql(
        &self,
        operation: &'static str,
        query: &str,
        variables: Value,
    ) -> Result<(Response, Value), ProviderError> {
        let (response, mut data) = self
            .graphql_request(operation, query, variables, false)
            .await?;
        let node = data["node"].take();
        Ok((response, node))
    }
    /// One GraphQL request, returning `data`. GraphQL reports failures inside
    /// a successful HTTP response, so they are mapped here. A mutation
    /// (`write`) is classified as a write by the transport, so a lost or
    /// unreadable answer stays unknown; GraphQL errors of a refusal type
    /// mean GitHub did not apply it.
    pub(super) async fn graphql_request(
        &self,
        operation: &'static str,
        query: &str,
        variables: Value,
        write: bool,
    ) -> Result<(Response, Value), ProviderError> {
        let body = json!({"query": query, "variables": variables});
        let response = self
            .request(
                operation,
                reqwest::Method::POST,
                self.grant.endpoints.graphql(),
                if write {
                    Payload::Json(body)
                } else {
                    Payload::Query(body)
                },
            )
            .await?;
        let mut body: Value = response.json()?;
        if let Some(errors) = body["errors"].as_array().filter(|e| !e.is_empty()) {
            let mut message = errors[0]["message"]
                .as_str()
                .unwrap_or("GraphQL query failed")
                .to_owned();
            // The field path names what was refused, e.g. a grant that
            // reaches the PR but not a review's commit.
            if let Some(path) = errors[0]["path"].as_array().filter(|p| !p.is_empty()) {
                let path = path
                    .iter()
                    .map(|segment| match segment {
                        Value::String(field) => field.clone(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(".");
                message = format!("{message} (at {path})");
            }
            let kind = errors[0]["type"].as_str();
            let mut error = response.failure(
                if response.rate_limited() || kind == Some("RATE_LIMITED") {
                    ProviderErrorKind::RateLimited
                } else {
                    ProviderErrorKind::Rejected
                },
                &message,
            );
            if write && kind.is_some_and(|kind| GRAPHQL_REFUSALS.contains(&kind)) {
                error.effect = crate::FailureEffect::Refused;
            }
            return Err(error);
        }
        let data = body["data"].take();
        Ok((response, data))
    }
    /// A provider link, only when it belongs to the configured web origin.
    fn web_url(&self, url: &Value) -> Option<String> {
        url.as_str()
            .filter(|url| self.grant.endpoints.web_path(url).is_some())
            .map(str::to_owned)
    }
    /// Pages of one PR review connection (`reviewThreads` or `reviews`), read
    /// through GraphQL because REST does not report thread resolution. Pages
    /// stay small so long automated review bodies remain within the response
    /// limit; the bool reports nodes beyond [`REVIEW_PAGES`].
    async fn review_connection(
        &self,
        operation: &'static str,
        pull_id: &str,
        connection: ReviewConnection,
    ) -> Result<(Vec<Value>, bool), ProviderError> {
        let query = format!(
            "query($id:ID!,$after:String){{node(id:$id){{...on PullRequest{{{}(first:{REVIEW_PAGE},after:$after){{pageInfo{{hasNextPage endCursor}} nodes{{{}}}}}}}}}}}",
            connection.field, connection.nodes
        );
        let mut nodes = Vec::new();
        let mut after = Value::Null;
        for _ in 0..REVIEW_PAGES {
            let (response, pull) = self
                .graphql(operation, &query, json!({"id": pull_id, "after": after}))
                .await?;
            let page = &pull[connection.field];
            let (Some(page_nodes), Some(next)) = (
                page["nodes"].as_array(),
                page["pageInfo"]["hasNextPage"].as_bool(),
            ) else {
                return Err(response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider returned an invalid PR review response",
                ));
            };
            nodes.extend(page_nodes.iter().cloned());
            if !next {
                return Ok((nodes, false));
            }
            after = page["pageInfo"]["endCursor"].clone();
            if !after.is_string() {
                return Err(response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider returned an invalid PR review page cursor",
                ));
            }
        }
        Ok((nodes, true))
    }
}

/// A provider PR's lifecycle as GitHub reports it: merge evidence wins over
/// `closed`, and Draft is GitHub's own draft flag on an open PR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PullRequestLifecycle {
    Open,
    Draft,
    Closed,
    Merged,
}

/// One provider PR's lifecycle and description: the raw Markdown body and
/// GitHub's own sanitized rendering of it.
#[derive(Clone, Debug)]
pub struct PullRequestDescription {
    pub lifecycle: PullRequestLifecycle,
    pub body: Option<String>,
    pub body_html: Option<String>,
}

impl RepositoryClient {
    /// One read of the PR in GitHub's `full+json` representation, without the
    /// mergeability re-reads `read_pull_request` waits for.
    pub async fn read_pull_request_description(
        &self,
        number: i64,
    ) -> Result<PullRequestDescription, ProviderError> {
        const OP: &str = "read pull request description";
        self.require(OP, &[RepositoryCapability::PullRequestRead])?;
        let response = self
            .send_media(
                OP,
                reqwest::Method::GET,
                self.repo_url(&["pulls", &number.to_string()])?,
                Payload::Empty,
                &[],
                REPOSITORY_TIMEOUT,
                Some("application/vnd.github.full+json"),
            )
            .await?;
        let pull: Value = response.json()?;
        let open = match pull["state"].as_str() {
            Some("open") => true,
            Some("closed") => false,
            _ => {
                return Err(response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider returned an invalid PR response",
                ))
            }
        };
        if pull["number"].as_i64() != Some(number) {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid PR response",
            ));
        }
        let lifecycle = if pull["merged"].as_bool() == Some(true) || !pull["merged_at"].is_null() {
            PullRequestLifecycle::Merged
        } else if !open {
            PullRequestLifecycle::Closed
        } else if pull["draft"].as_bool() == Some(true) {
            PullRequestLifecycle::Draft
        } else {
            PullRequestLifecycle::Open
        };
        Ok(PullRequestDescription {
            lifecycle,
            body: pull["body"].as_str().map(str::to_owned),
            body_html: pull["body_html"].as_str().map(str::to_owned),
        })
    }

    /// GitHub's rendering of Markdown (`POST /markdown`, `gfm` mode). With a
    /// repository context, bare references such as `#12` link into it. The
    /// request only reads: it changes no provider state.
    pub async fn render_markdown(
        &self,
        text: &str,
        context: Option<&str>,
    ) -> Result<String, ProviderError> {
        const OP: &str = "render markdown";
        self.require(OP, &[RepositoryCapability::PullRequestRead])?;
        let mut body = json!({"text": text, "mode": "gfm"});
        if let Some(context) = context {
            body["context"] = json!(context);
        }
        let response = self
            .request(
                OP,
                reqwest::Method::POST,
                self.grant.endpoints.api(&["markdown"]),
                Payload::Query(body),
            )
            .await?;
        response.require_success()?;
        String::from_utf8(response.body.clone()).map_err(|_| {
            response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned rendered Markdown that is not UTF-8",
            )
        })
    }
}

struct ReviewConnection {
    field: &'static str,
    nodes: &'static str,
}
const REVIEW_THREADS: ReviewConnection = ReviewConnection {
    field: "reviewThreads",
    nodes: "id isResolved isOutdated path line startLine originalLine originalStartLine diffSide \
        subjectType resolvedBy{login} comments(first:50){totalCount nodes{databaseId \
        author{login} body createdAt url diffHunk commit{oid} originalCommit{oid}}}",
};
const REVIEWS: ReviewConnection = ReviewConnection {
    field: "reviews",
    nodes: "id author{login} state body submittedAt url commit{oid}",
};
/// Re-reads of a PR whose mergeability GitHub is still computing: about 3 s.
const MERGEABLE_REREADS: usize = 3;
const MERGEABLE_REREAD_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
const REVIEW_PAGE: usize = 25;
const REVIEW_PAGES: usize = 8;
/// Latest reviews hold one review per reviewer.
const LATEST_REVIEWS: usize = 100;
/// GraphQL error types with which GitHub refuses a mutation without applying it.
const GRAPHQL_REFUSALS: &[&str] = &[
    "FORBIDDEN",
    "NOT_FOUND",
    "UNPROCESSABLE",
    "INSUFFICIENT_SCOPES",
    "RATE_LIMITED",
    "BAD_REQUEST",
];

/// `gh`'s review shape (`commit{oid}`, node `id`) plus an owned-origin `url`.
fn review(review: &Value, web_url: impl Fn(&Value) -> Option<String>) -> Value {
    json!({
        "id": review["id"], "author": {"login": review["author"]["login"]},
        "state": review["state"], "body": review["body"], "submittedAt": review["submittedAt"],
        "commit": commit(&review["commit"]), "url": web_url(&review["url"]),
    })
}
fn commit(value: &Value) -> Value {
    value["oid"]
        .as_str()
        .map_or(Value::Null, |oid| json!({"oid": oid}))
}

fn review_thread(thread: &Value, web_url: impl Fn(&Value) -> Option<String>) -> Value {
    let comments = thread["comments"]["nodes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    json!({
        // The node ID `g5d pr comment --reply-to` and `g5d pr resolve` take.
        "id": thread["id"],
        "path": thread["path"], "line": thread["line"], "startLine": thread["startLine"],
        "originalLine": thread["originalLine"], "originalStartLine": thread["originalStartLine"],
        "diffSide": thread["diffSide"], "subjectType": thread["subjectType"],
        "isResolved": thread["isResolved"], "isOutdated": thread["isOutdated"],
        "resolvedBy": thread["resolvedBy"]["login"].as_str().map(|login| json!({"login": login})),
        // The diff context the thread was started on.
        "diffHunk": comments.first().map_or(&Value::Null, |c| &c["diffHunk"]),
        "comments": comments.iter().map(|c| json!({
            "id": c["databaseId"], "author": {"login": c["author"]["login"]}, "body": c["body"],
            "createdAt": c["createdAt"], "url": web_url(&c["url"]),
            "commit": commit(&c["commit"]), "originalCommit": commit(&c["originalCommit"]),
        })).collect::<Vec<_>>(),
        "commentsHasMore": thread["comments"]["totalCount"].as_u64().is_some_and(|n| n > comments.len() as u64),
    })
}
