//! Repository PR listing as `gh pr list` does it (`g5.git.pr-list`): the
//! plain GraphQL listing for state and branch filters, GitHub search when an
//! author, label or free-text filter is given (`g5.git.pr-list.search`).
use super::*;
use serde_json::{json, Value};

/// Most PRs one listing returns: one GraphQL page.
pub const PULL_REQUEST_LIST_MAX: u32 = 100;

/// Which PRs a listing keeps, by GitHub's lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullRequestListState {
    Open,
    /// Closed without merging, and merged, as `gh pr list --state closed`.
    Closed,
    Merged,
    All,
}

impl PullRequestListState {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "open" => Self::Open,
            "closed" => Self::Closed,
            "merged" => Self::Merged,
            "all" => Self::All,
            _ => return None,
        })
    }
}

/// The filters of one repository listing.
#[derive(Clone, Copy, Debug)]
pub struct PullRequestListFilter<'a> {
    pub state: PullRequestListState,
    pub head: Option<&'a str>,
    pub base: Option<&'a str>,
    pub author: Option<&'a str>,
    pub labels: &'a [String],
    pub search: Option<&'a str>,
    /// 1..=[`PULL_REQUEST_LIST_MAX`].
    pub limit: u32,
    /// Also read each PR's body, as `gh pr list --json body` does.
    pub with_body: bool,
}

impl PullRequestListFilter<'_> {
    /// Whether the listing needs GitHub search rather than the plain listing.
    #[must_use]
    pub fn needs_search(&self) -> bool {
        self.author.is_some()
            || !self.labels.is_empty()
            || self.search.is_some_and(|text| !text.trim().is_empty())
    }

    /// Refuse values the query cannot carry unambiguously, before any request.
    pub fn validate(&self) -> Result<(), ProviderError> {
        const OP: &str = "list pull requests";
        if !(1..=PULL_REQUEST_LIST_MAX).contains(&self.limit) {
            return Err(ProviderError::invalid(
                OP,
                "A repository listing returns at most 100 PRs; use a limit of 1-100 and narrow the filters",
            ));
        }
        let plain = |value: Option<&str>| {
            value.is_none_or(|value| {
                !value.is_empty()
                    && !value
                        .chars()
                        .any(|c| c.is_whitespace() || c.is_control() || c == '"')
            })
        };
        if !plain(self.head) || !plain(self.base) || !plain(self.author) {
            return Err(ProviderError::invalid(
                OP,
                "--head, --base and --author each take one name without spaces or quotes",
            ));
        }
        if self.labels.iter().any(|label| {
            label.trim().is_empty()
                || label.contains(['"', '\\'])
                || label.chars().any(char::is_control)
        }) {
            return Err(ProviderError::invalid(
                OP,
                "--label must be a label name without quotes or backslashes",
            ));
        }
        if let Some(text) = self.search {
            if text.chars().count() > MAX_SEARCH_CHARS {
                return Err(ProviderError::invalid(
                    OP,
                    "--search is limited to 200 characters",
                ));
            }
            if text
                .split_whitespace()
                .any(|term| issues::reserved_qualifier(term).is_some())
            {
                return Err(ProviderError::invalid(
                    OP,
                    "G5 sets the repo:, org:, user:, owner:, is:, type: and state: qualifiers itself; use --repo and --state instead",
                ));
            }
            // The listing is newest created first: an open quote or a sort:
            // term would swallow or override the trailing sort:created-desc.
            if text.matches('"').count() % 2 == 1
                || text.split_whitespace().any(|term| {
                    term.trim_start_matches(['+', '-'])
                        .to_ascii_lowercase()
                        .starts_with("sort:")
                })
            {
                return Err(ProviderError::invalid(
                    OP,
                    "--search must balance its quotes and cannot set sort:; the listing is newest first",
                ));
            }
        }
        Ok(())
    }
}

const MAX_SEARCH_CHARS: usize = 200;

/// One listing's PRs, newest created first, in `g5d pr view`'s field names.
#[derive(Clone, Debug)]
pub struct PullRequestListPage {
    pub pulls: Vec<Value>,
    /// GitHub has more matching PRs than the page returned.
    pub has_more: bool,
}

const PULL_FIELDS: &str = "id number title url state isDraft headRefName headRefOid \
     baseRefName createdAt updatedAt mergedAt closedAt author{login} \
     labels(first:20){nodes{name}} repository{nameWithOwner}";

/// `repo:{fullName} is:pr <state> [head:] [base:] [author:] [label:"…"]… <terms> sort:created-desc`.
#[must_use]
pub fn build_pull_request_search_query(full_name: &str, filter: &PullRequestListFilter) -> String {
    let mut parts = vec![format!("repo:{full_name}"), "is:pr".to_owned()];
    match filter.state {
        PullRequestListState::Open => parts.push("is:open".into()),
        PullRequestListState::Closed => parts.push("is:closed".into()),
        PullRequestListState::Merged => parts.push("is:merged".into()),
        PullRequestListState::All => {}
    }
    for (qualifier, value) in [
        ("head", filter.head),
        ("base", filter.base),
        ("author", filter.author),
    ] {
        if let Some(value) = value {
            parts.push(format!("{qualifier}:{value}"));
        }
    }
    for label in filter.labels {
        parts.push(format!("label:\"{label}\""));
    }
    if let Some(text) = filter.search {
        parts.extend(text.split_whitespace().map(str::to_owned));
    }
    parts.push("sort:created-desc".into());
    parts.join(" ")
}

impl RepositoryClient {
    /// List the repository's PRs (`g5.git.pr-list`). Nodes from another
    /// repository, or that are not PRs, are dropped.
    pub async fn list_pull_requests(
        &self,
        filter: &PullRequestListFilter<'_>,
    ) -> Result<PullRequestListPage, ProviderError> {
        const OP: &str = "list pull requests";
        self.require(OP, &[RepositoryCapability::PullRequestRead])?;
        filter.validate()?;
        let full_name = self.repository_name()?.to_owned();
        let (owner, name) = repository_name(&full_name)?;
        let fields = if filter.with_body {
            format!("{PULL_FIELDS} body")
        } else {
            PULL_FIELDS.to_owned()
        };
        let (nodes, has_more) = if filter.needs_search() {
            let query = format!(
                "query($q:String!,$first:Int!){{search(query:$q,type:ISSUE,first:$first){{issueCount nodes{{...on PullRequest{{{fields}}}}}}}}}"
            );
            let (response, data) = self
                .graphql_request(
                    OP,
                    &query,
                    json!({"q": build_pull_request_search_query(&full_name, filter), "first": filter.limit}),
                    false,
                )
                .await?;
            let search = &data["search"];
            let (Some(nodes), Some(count)) =
                (search["nodes"].as_array(), search["issueCount"].as_u64())
            else {
                return Err(response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider returned an invalid PR search response",
                ));
            };
            let returned = u64::try_from(nodes.len()).unwrap_or(u64::MAX);
            (nodes.clone(), count > returned)
        } else {
            let states = match filter.state {
                PullRequestListState::Open => json!(["OPEN"]),
                PullRequestListState::Closed => json!(["CLOSED", "MERGED"]),
                PullRequestListState::Merged => json!(["MERGED"]),
                PullRequestListState::All => Value::Null,
            };
            let query = format!(
                "query($owner:String!,$name:String!,$first:Int!,$states:[PullRequestState!],$head:String,$base:String)\
                 {{repository(owner:$owner,name:$name){{pullRequests(first:$first,states:$states,headRefName:$head,\
                 baseRefName:$base,orderBy:{{field:CREATED_AT,direction:DESC}}){{pageInfo{{hasNextPage}} nodes{{{fields}}}}}}}}}"
            );
            let (response, data) = self
                .graphql_request(
                    OP,
                    &query,
                    json!({"owner": owner, "name": name, "first": filter.limit,
                           "states": states, "head": filter.head, "base": filter.base}),
                    false,
                )
                .await?;
            let page = &data["repository"]["pullRequests"];
            let (Some(nodes), Some(next)) = (
                page["nodes"].as_array(),
                page["pageInfo"]["hasNextPage"].as_bool(),
            ) else {
                return Err(response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider returned an invalid PR listing",
                ));
            };
            (nodes.clone(), next)
        };
        let pulls = nodes
            .iter()
            .filter(|node| {
                node["repository"]["nameWithOwner"]
                    .as_str()
                    .is_some_and(|repo| repo.eq_ignore_ascii_case(&full_name))
                    && node["number"].as_i64().is_some_and(|n| n > 0)
            })
            .map(|node| {
                let number = node["number"].as_i64().unwrap_or_default();
                json!({
                    "source": "github",
                    "id": node["id"],
                    "number": number,
                    "repository": full_name,
                    "url": self.grant.endpoints.web(&[owner, name, "pull", &number.to_string()]).as_str(),
                    "title": node["title"],
                    "body": node["body"],
                    "state": match node["state"].as_str() {
                        Some(state @ ("OPEN" | "CLOSED" | "MERGED")) => json!(state),
                        _ => Value::Null,
                    },
                    "author": {"login": node["author"]["login"]},
                    "isDraft": node["isDraft"],
                    "headRefName": node["headRefName"],
                    "headRefOid": node["headRefOid"],
                    "baseRefName": node["baseRefName"],
                    "createdAt": node["createdAt"],
                    "updatedAt": node["updatedAt"],
                    "mergedAt": node["mergedAt"],
                    "closedAt": node["closedAt"],
                    "labels": node["labels"]["nodes"].as_array().map(|labels| {
                        labels.iter().map(|label| json!({"name": label["name"]})).collect::<Vec<_>>()
                    }).unwrap_or_default(),
                })
            })
            .collect();
        Ok(PullRequestListPage { pulls, has_more })
    }
}
