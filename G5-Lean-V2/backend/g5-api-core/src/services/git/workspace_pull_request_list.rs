//! Repository PR listing (`g5.git.pr-list`): GitHub's PRs for a repository the
//! workspace reads, named with their G5 PRs (`g5.git.pr-g5-identity`), or G5's
//! stored PRs when the repository has no GitHub integration
//! (`g5.git.pr-list.local`).
use super::super::{token_from_auth, WorkspacePullRequestProviderLifecycleService};
use super::{
    escape_like, invalid, pr_audience, readable_repositories, ReadableRepository, RepositoryUse,
};
use crate::auth::RepositoryOperation;
use crate::services::git::pull_request_adapters::g5_pull_request_name;
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::G5Error;
use g5_scm::providers::{
    PullRequestListFilter, PullRequestListState, RepositoryCapability, PULL_REQUEST_LIST_MAX,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// `g5d pr list` in repository mode. Every field is optional; `state`
/// defaults to `open` and `limit` to 30, as `gh pr list`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PullRequestListQuery {
    /// `OWNER/NAME` for a GitHub repository, else G5's repository name or slug.
    pub repo: Option<String>,
    /// `open`, `closed` (closed and merged), `merged` or `all`.
    pub state: Option<String>,
    pub head: Option<String>,
    pub base: Option<String>,
    pub author: Option<String>,
    /// Comma-separated label names, as `gh pr list --label a,b`.
    pub label: Option<String>,
    pub search: Option<String>,
    pub limit: Option<u32>,
    /// Read each PR's body (`--json body`).
    #[serde(default)]
    pub body: bool,
}

const DEFAULT_LIMIT: u32 = 30;

/// `g5d pr list --all` / `--workspace WS` (`g5.git.pr-reads`).
#[derive(Debug, Default, Deserialize)]
pub struct ReadablePullRequestListQuery {
    pub workspace: Option<String>,
    pub state: Option<String>,
    pub author: Option<String>,
    pub search: Option<String>,
    pub limit: Option<u32>,
}

/// Credential refusals that mean the repository has no GitHub integration,
/// rather than one that failed (`g5.git.pr-list.local`).
const NO_INTEGRATION: &[&str] = &[
    "no_scm_connection",
    "no_scm_token_manager",
    "github_app_unavailable",
];

impl WorkspacePullRequestProviderLifecycleService {
    /// List the selected repository's PRs. `agent` spends the agent search
    /// budget when GitHub search is used (`g5.git.pr-list.search`).
    pub async fn list_repository_pull_requests(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        query: &PullRequestListQuery,
        agent: bool,
    ) -> Result<Value, G5Error> {
        let state = PullRequestListState::parse(query.state.as_deref().unwrap_or("open"))
            .ok_or_else(|| invalid("--state must be open, closed, merged or all"))?;
        let labels = query
            .label
            .as_deref()
            .map(|labels| {
                labels
                    .split(',')
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let filter = PullRequestListFilter {
            state,
            head: non_empty(&query.head),
            base: non_empty(&query.base),
            author: non_empty(&query.author),
            labels: &labels,
            search: non_empty(&query.search),
            limit: query.limit.unwrap_or(DEFAULT_LIMIT),
            with_body: query.body,
        };
        filter.validate().map_err(G5Error::from)?;

        // The endpoints default to GitHub's, so an error is a misconfiguration,
        // as in the other PR reads; it is never read as "not on GitHub".
        let endpoints = self.provider_endpoints()?;
        let repos = readable_repositories(&mut db, workspace_id, RepositoryUse::Read).await?;
        let github = |url: &str| endpoints.repository_name_from_clone_url(url);
        let own_has = |repos: &[ReadableRepository]| {
            repos.iter().any(|repo| {
                select_repository(vec![repo.clone()], query.repo.as_deref(), github).is_ok()
            })
        };
        // A named repository this workspace lacks is looked up among every
        // readable workspace's members (`g5.git.pr-reads`).
        let repos = if query.repo.is_some() && !own_has(&repos) {
            let reader = self.pull_request_reader(&db, actor).await?;
            super::audience_repositories(&mut db, reader).await?
        } else {
            repos
        };
        let (repo, github_name) = select_repository(repos, query.repo.as_deref(), github)?;
        let Some(full_name) = github_name else {
            return self
                .list_stored(
                    db,
                    &repo,
                    repo.name.clone(),
                    actor,
                    &filter,
                    StoredFallback::NotOnGithub,
                )
                .await;
        };
        let search_domains = if agent && filter.needs_search() {
            Some(
                crate::services::external_issues::count_external_domains(&mut db, repo.org_id)
                    .await?,
            )
        } else {
            None
        };
        let scope = db.reopener();
        // Release the read transaction before token minting or provider I/O.
        db.rollback().await.map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?;
        let auth = match self
            .credentials
            .resolve(
                repo.org_id,
                repo.repo_id,
                actor,
                &repo.git_url,
                RepositoryOperation::ReadPullRequests,
            )
            .await
        {
            Ok(auth) => token_from_auth(&auth),
            Err(G5Error::Conflict { reason })
                if NO_INTEGRATION
                    .iter()
                    .any(|code| reason == format!("Repository credentials: {code}")) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        let Some(token) = auth else {
            let db = open(&scope).await?;
            return self
                .list_stored(
                    db,
                    &repo,
                    full_name,
                    actor,
                    &filter,
                    StoredFallback::NotConnected,
                )
                .await;
        };
        if let Some(domains) = search_domains {
            let store = crate::middleware::rate_limit::PostgresRateLimitStore::new(
                self.system_pool.clone(),
            );
            crate::services::external_issues::reserve_agent_search(
                &store,
                repo.org_id,
                workspace_id,
                domains,
            )
            .await?;
        }
        let page = self
            .repository_protocol(&token, &full_name, RepositoryCapability::PullRequestRead)?
            .list_pull_requests(&filter)
            .await
            .map_err(|error| {
                tracing::warn!(
                    workspace_id = %workspace_id,
                    repo_id = %repo.repo_id,
                    error = %error,
                    "GitHub PR listing failed"
                );
                G5Error::from(error)
            })?;
        let mut pulls = page.pulls;
        self.attach_g5_pull_requests(&scope, repo.org_id, repo.repo_id, actor, &mut pulls)
            .await?;
        for pull in &mut pulls {
            pull["tracker"] = json!("github");
        }
        // The repository's local repository pull requests are listed beside
        // GitHub's (`g5.git.pr-list`).
        let (local, local_more) = self
            .stored_rows(open(&scope).await?, &repo, &full_name, actor, &filter, true)
            .await?;
        let (pulls, merged_more) = newest_first(pulls, local, filter.limit as usize);
        Ok(json!({
            "source": "github",
            "repository": full_name,
            "pullRequests": pulls,
            "hasMore": page.has_more || local_more || merged_more,
        }))
    }

    /// Name each GitHub PR's G5 PRs (`g5.git.pr-g5-identity`), newest first,
    /// keeping only those whose source or target workspace `actor` may read.
    pub(super) async fn attach_g5_pull_requests(
        &self,
        scope: &ScopedDbReopener,
        org_id: Uuid,
        repo_id: Uuid,
        actor: Uuid,
        pulls: &mut [Value],
    ) -> Result<(), G5Error> {
        let numbers = pulls
            .iter()
            .filter_map(|pull| pull["number"].as_i64())
            .collect::<Vec<_>>();
        if numbers.is_empty() {
            return Ok(());
        }
        let org_wide = self.org_wide_reader(org_id, actor).await?;
        let mut db = open(scope).await?;
        type LinkRow = (
            i64,
            Uuid,
            String,
            Uuid,
            String,
            Uuid,
            String,
            i64,
            bool,
            bool,
        );
        let rows: Vec<LinkRow> = sqlx::query_as(&format!(
            "SELECT p.provider_pr_number, pr.id, pr.status, \
                    pr.source_workspace_id, sw.name, pr.target_workspace_id, tw.name, \
                    pr.number, sw.deleted_at IS NOT NULL, tw.deleted_at IS NOT NULL \
             FROM workspace_pull_request_provider_repos p \
             JOIN workspace_pull_requests pr ON pr.org_id = p.org_id AND pr.id = p.pull_request_id \
             JOIN workspaces sw ON sw.org_id = pr.org_id AND sw.id = pr.source_workspace_id \
             JOIN workspaces tw ON tw.org_id = pr.org_id AND tw.id = pr.target_workspace_id \
             WHERE p.org_id = $1 AND p.repo_id = $2 AND p.provider_pr_number = ANY($3) \
               AND ({sw} OR {tw}) \
             ORDER BY pr.created_at DESC, pr.id DESC",
            sw = pr_audience("sw", 4, 5),
            tw = pr_audience("tw", 4, 5),
        ))
        .bind(org_id)
        .bind(repo_id)
        .bind(&numbers)
        .bind(org_wide)
        .bind(actor)
        .fetch_all(db.conn())
        .await?;
        db.rollback().await.map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?;
        let mut linked: HashMap<i64, Vec<Value>> = HashMap::new();
        for (
            number,
            id,
            status,
            source_id,
            source_name,
            target_id,
            target_name,
            g5_number,
            source_deleted,
            target_deleted,
        ) in rows
        {
            linked.entry(number).or_default().push(json!({
                "id": id,
                "status": status,
                "number": g5_pull_request_name(g5_number),
                "sourceWorkspaceId": source_id,
                "sourceWorkspaceName": source_name,
                "sourceWorkspaceDeleted": source_deleted,
                "targetWorkspaceId": target_id,
                "targetWorkspaceName": target_name,
                "targetWorkspaceDeleted": target_deleted,
            }));
        }
        for pull in pulls {
            let number = pull["number"].as_i64();
            pull["g5PullRequests"] = json!(number
                .and_then(|number| linked.remove(&number))
                .unwrap_or_default());
        }
        Ok(())
    }

    /// G5's stored PRs that change `repo`, where GitHub cannot list them
    /// (`g5.git.pr-list.local`): every one for a repository not on GitHub,
    /// whose repository pull requests are all local, and, for a GitHub
    /// repository without an integration, its GitHub ones from their stored
    /// records as well.
    async fn list_stored(
        &self,
        db: ScopedDb,
        repo: &ReadableRepository,
        repository: String,
        actor: Uuid,
        filter: &PullRequestListFilter<'_>,
        fallback: StoredFallback,
    ) -> Result<Value, G5Error> {
        let github_expected = fallback == StoredFallback::NotConnected;
        if !filter.labels.is_empty() {
            return Err(invalid(if github_expected {
                "--label needs GitHub; G5's stored PRs have no labels"
            } else {
                "--label is not available: this repository's PRs are G5 PRs, which have no labels"
            }));
        }
        let (pulls, has_more) = self
            .stored_rows(db, repo, &repository, actor, filter, false)
            .await?;
        Ok(json!({
            "source": "g5",
            "repository": repository,
            "fallback": fallback.reason(),
            "githubExpected": github_expected,
            "pullRequests": pulls,
            "hasMore": has_more,
        }))
    }

    /// The rows of G5's stored PRs that change `repo`, newest created first,
    /// and whether more exist. With `local_only`, only its local repository
    /// pull requests: those with no GitHub record whose workspaces do not both
    /// track the repository remotely (`g5.git.pr-repository`), which a GitHub
    /// listing shows beside GitHub's. Labels match none of them.
    async fn stored_rows(
        &self,
        mut db: ScopedDb,
        repo: &ReadableRepository,
        repository: &str,
        actor: Uuid,
        filter: &PullRequestListFilter<'_>,
        local_only: bool,
    ) -> Result<(Vec<Value>, bool), G5Error> {
        if !filter.labels.is_empty() {
            return Ok((Vec::new(), false));
        }
        let authors = match filter.author {
            None => None,
            Some(author) => Some(self.matching_members(repo.org_id, actor, author).await?),
        };
        let org_wide = self.org_wide_reader(repo.org_id, actor).await?;
        let statuses: Option<Vec<&str>> = match filter.state {
            PullRequestListState::Open => Some(vec!["open"]),
            PullRequestListState::Closed => Some(vec!["closed", "merged"]),
            PullRequestListState::Merged => Some(vec!["merged"]),
            PullRequestListState::All => None,
        };
        let words = filter
            .search
            .map(|text| {
                text.split_whitespace()
                    .map(|word| format!("%{}%", escape_like(word)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let fetch = i64::from(filter.limit.min(PULL_REQUEST_LIST_MAX)) + 1;
        // Only PRs that change this repository, as publication decides
        // (`repo_needs_provider_pr`): its recorded versions differ and its
        // preflight is neither identical nor already merged. A G5 PR records
        // every member, changed or not.
        let rows: Vec<StoredRow> = sqlx::query_as(&format!(
            "SELECT pr.id, pr.title, pr.status, pr.created_by, pr.created_at, pr.updated_at, \
                    pr.merged_at, s.head ->> 'branchName' AS head_branch, \
                    t.head ->> 'branchName' AS base_branch, \
                    s.head ->> 'headCommit' AS head_oid, \
                    pr.source_workspace_id, sw.name AS source_name, \
                    pr.target_workspace_id, tw.name AS target_name, \
                    p.provider_pr_number, p.provider_pr_url, \
                    (p.id IS NOT NULL) AS github_linked, pr.number AS g5_number, \
                    ($12 AND {publishes}) AS publishes_to_github, \
                    (sw.deleted_at IS NOT NULL) AS source_deleted, \
                    (tw.deleted_at IS NOT NULL) AS target_deleted \
             FROM workspace_pull_requests pr \
             JOIN workspaces sw ON sw.org_id = pr.org_id AND sw.id = pr.source_workspace_id \
             JOIN workspaces tw ON tw.org_id = pr.org_id AND tw.id = pr.target_workspace_id \
             {joins} \
             WHERE pr.org_id = $1 AND ({sw} OR {tw}) AND {changes} \
               AND ($5::text[] IS NULL OR pr.status = ANY($5)) \
               AND ($6::text IS NULL OR s.head ->> 'branchName' = $6) \
               AND ($7::text IS NULL OR t.head ->> 'branchName' = $7) \
               AND ($8::uuid[] IS NULL OR pr.created_by = ANY($8)) \
               AND NOT EXISTS (SELECT 1 FROM unnest($9::text[]) AS word \
                               WHERE NOT (pr.title ILIKE word \
                                          OR COALESCE(pr.description_override, '') ILIKE word)) \
               AND (NOT $11 OR (p.id IS NULL AND NOT ($12 AND {publishes}))) \
             ORDER BY pr.created_at DESC, pr.id DESC \
             LIMIT $10",
            sw = pr_audience("sw", 3, 4),
            tw = pr_audience("tw", 3, 4),
            joins = CHANGED_REPOSITORY_JOINS,
            changes = CHANGES_REPOSITORY,
            publishes = crate::services::git::pull_request_adapters::PUBLISHES_TO_GITHUB_SQL,
        ))
        .bind(repo.org_id)
        .bind(repo.repo_id)
        .bind(org_wide)
        .bind(actor)
        .bind(statuses)
        .bind(filter.head)
        .bind(filter.base)
        .bind(authors)
        .bind(&words)
        .bind(fetch)
        .bind(local_only)
        .bind(
            self.adapters
                .is_github_repository(repo.provider.as_deref(), &repo.git_url),
        )
        .fetch_all(db.conn())
        .await?;
        db.rollback().await.map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?;
        let has_more = rows.len() > filter.limit as usize;
        let rows = &rows[..rows.len().min(filter.limit as usize)];
        let creators = self
            .creator_handles(rows.iter().filter_map(|row| row.created_by).collect())
            .await?;
        let github = |row: &StoredRow| row.github_linked || row.publishes_to_github;
        let pulls = rows
            .iter()
            .map(|row| {
                json!({
                    "source": "g5",
                    // GitHub's repository pull request, or the local one,
                    // which is named by its G5 PR, never by a GitHub-like
                    // number (`g5.git.pr-local-number`). The binding
                    // (`g5.git.pr-repository`): GitHub's when recorded there
                    // or publication would publish it there.
                    "tracker": if github(row) { "github" } else { "g5" },
                    "id": row.id,
                    "number": if github(row) { row.provider_pr_number } else { None },
                    "g5Number": (!github(row)).then(|| g5_pull_request_name(row.g5_number)),
                    "repository": repository,
                    "url": row.provider_pr_url,
                    "title": row.title,
                    "state": row.status.to_ascii_uppercase(),
                    "author": row.created_by.and_then(|id| creators.get(&id)).map_or(
                        json!({"login": null}),
                        |(login, name)| json!({"login": login, "name": name}),
                    ),
                    "isDraft": null,
                    "headRefName": row.head_branch,
                    "headRefOid": row.head_oid,
                    "baseRefName": row.base_branch,
                    "createdAt": row.created_at,
                    "updatedAt": row.updated_at,
                    "mergedAt": row.merged_at,
                    "closedAt": null,
                    "labels": [],
                    "g5PullRequests": [{
                        "id": row.id,
                        "status": row.status,
                        "sourceWorkspaceId": row.source_workspace_id,
                        "sourceWorkspaceName": row.source_name,
                        "targetWorkspaceId": row.target_workspace_id,
                        "targetWorkspaceName": row.target_name,
                        "number": g5_pull_request_name(row.g5_number),
                        "sourceWorkspaceDeleted": row.source_deleted,
                        "targetWorkspaceDeleted": row.target_deleted,
                    }],
                })
            })
            .collect::<Vec<_>>();
        Ok((pulls, has_more))
    }

    /// `g5d pr list --all` or `--workspace WS` (`g5.git.pr-reads`): the IDs of
    /// readable G5 PRs, newest first, and whether more exist. `workspace` is a
    /// workspace ID or case-insensitive name the reader can open.
    pub async fn list_readable_pull_requests(
        &self,
        db: &mut ScopedDb,
        actor: Uuid,
        query: &ReadablePullRequestListQuery,
    ) -> Result<(Vec<Uuid>, bool), G5Error> {
        let reader = self.pull_request_reader(db, actor).await?;
        let state = PullRequestListState::parse(query.state.as_deref().unwrap_or("open"))
            .ok_or_else(|| invalid("--state must be open, closed, merged or all"))?;
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=PULL_REQUEST_LIST_MAX).contains(&limit) {
            return Err(invalid(&format!(
                "--limit must be 1-{PULL_REQUEST_LIST_MAX}"
            )));
        }
        // The same cap as a repository listing's search.
        if query
            .search
            .as_deref()
            .is_some_and(|s| s.chars().count() > 200)
        {
            return Err(invalid("--search is limited to 200 characters"));
        }
        let workspace = match non_empty(&query.workspace) {
            None => None,
            Some(name) => Some(self.readable_workspace(db, reader, name).await?),
        };
        let authors = match non_empty(&query.author) {
            None => None,
            Some(author) => Some(self.matching_members(reader.org_id, actor, author).await?),
        };
        let statuses = match state {
            PullRequestListState::Open => Some(vec!["open"]),
            PullRequestListState::Closed => Some(vec!["closed", "merged"]),
            PullRequestListState::Merged => Some(vec!["merged"]),
            PullRequestListState::All => None,
        };
        let mut ids = self
            .readable_pull_request_ids(
                db,
                reader,
                &super::ReadablePullRequestFilter {
                    ids: None,
                    workspace,
                    statuses,
                    authors,
                    search: non_empty(&query.search),
                    limit: Some(i64::from(limit) + 1),
                },
            )
            .await?;
        let has_more = ids.len() > limit as usize;
        ids.truncate(limit as usize);
        Ok((ids, has_more))
    }

    /// A workspace whose PRs the reader may read, by ID or case-insensitive
    /// name: a live one first, then the most recently deleted one under that
    /// name (`g5.git.pr-reads`).
    async fn readable_workspace(
        &self,
        db: &mut ScopedDb,
        reader: super::PullRequestReader,
        name: &str,
    ) -> Result<Uuid, G5Error> {
        let id: Option<Uuid> = sqlx::query_scalar(&format!(
            "SELECT w.id FROM workspaces w WHERE w.org_id = $1 \
               AND (w.id::text = $4 OR lower(w.name) = lower($4)) AND {} \
             ORDER BY (w.id::text = $4) DESC, (w.deleted_at IS NULL) DESC, \
                      w.deleted_at DESC NULLS FIRST, w.created_at DESC, w.id LIMIT 1",
            pr_audience("w", 2, 3)
        ))
        .bind(reader.org_id)
        .bind(reader.org_wide)
        .bind(reader.actor)
        .bind(name)
        .fetch_optional(db.conn())
        .await?;
        id.ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {name} you can read"),
        })
    }

    /// Organization members a stored `--author` names: `@me`, an email, or
    /// an email's local part, case-insensitively.
    pub(super) async fn matching_members(
        &self,
        org_id: Uuid,
        actor: Uuid,
        author: &str,
    ) -> Result<Vec<Uuid>, G5Error> {
        if author == "@me" {
            return Ok(vec![actor]);
        }
        Ok(sqlx::query_scalar(
            "SELECT u.id FROM users u \
             JOIN org_memberships m ON m.user_id = u.id AND m.org_id = $1 \
             WHERE lower(u.email) = lower($2) OR lower(split_part(u.email, '@', 1)) = lower($2)",
        )
        .bind(org_id)
        .bind(author)
        .fetch_all(&self.system_pool)
        .await?)
    }

    /// Each creator's email local part and display name.
    async fn creator_handles(
        &self,
        ids: Vec<Uuid>,
    ) -> Result<HashMap<Uuid, (String, String)>, G5Error> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT id, split_part(email, '@', 1), display_name FROM users WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.system_pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, login, name)| (id, (login, name)))
            .collect())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct StoredRow {
    id: Uuid,
    title: String,
    status: String,
    created_by: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    merged_at: Option<chrono::DateTime<chrono::Utc>>,
    head_branch: Option<String>,
    base_branch: Option<String>,
    head_oid: Option<String>,
    source_workspace_id: Uuid,
    source_name: String,
    target_workspace_id: Uuid,
    target_name: String,
    provider_pr_number: Option<i64>,
    provider_pr_url: Option<String>,
    github_linked: bool,
    publishes_to_github: bool,
    g5_number: i64,
    source_deleted: bool,
    target_deleted: bool,
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn open(scope: &ScopedDbReopener) -> Result<ScopedDb, G5Error> {
    scope.open().await.map_err(|error| G5Error::Internal {
        message: format!("Failed to open PR listing transaction: {error}"),
    })
}

/// Choose the listed repository (`g5.git.pr-read-repositories`): `--repo`
/// names a GitHub repository by `OWNER/NAME` and another by its G5 name or
/// slug; without it, the single GitHub repository, else the single
/// repository. Returns its GitHub name when it has one.
pub(crate) fn select_repository(
    repos: Vec<ReadableRepository>,
    selected: Option<&str>,
    github_name: impl Fn(&str) -> Option<String>,
) -> Result<(ReadableRepository, Option<String>), G5Error> {
    let named = repos
        .into_iter()
        .map(|repo| {
            let name = github_name(&repo.git_url);
            (repo, name)
        })
        .collect::<Vec<_>>();
    let mut candidates = match selected.map(str::trim) {
        Some(selected) => named
            .into_iter()
            .filter(|(repo, name)| match name {
                Some(name) => name.eq_ignore_ascii_case(selected),
                None => {
                    repo.name.eq_ignore_ascii_case(selected)
                        || repo.slug.eq_ignore_ascii_case(selected)
                }
            })
            .collect::<Vec<_>>(),
        None if named.iter().any(|(_, name)| name.is_some()) => named
            .into_iter()
            .filter(|(_, name)| name.is_some())
            .collect(),
        None => named,
    };
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err(invalid(
            "No matching repository is attached to this workspace",
        )),
        _ => Err(invalid(
            "Multiple repositories match; specify --repo OWNER/REPO",
        )),
    }
}

/// Joins `s`/`t` (the PR's recorded source and target heads for the
/// repository bound as `$2`) and `p` (its GitHub record there).
pub(super) const CHANGED_REPOSITORY_JOINS: &str = "\
    JOIN LATERAL (SELECT h AS head FROM jsonb_array_elements(pr.source_heads) h \
                  WHERE h ->> 'repoId' = $2::text LIMIT 1) s ON TRUE \
    LEFT JOIN LATERAL (SELECT h AS head FROM jsonb_array_elements(pr.target_heads) h \
                       WHERE h ->> 'repoId' = $2::text LIMIT 1) t ON TRUE \
    LEFT JOIN workspace_pull_request_provider_repos p \
      ON p.org_id = pr.org_id AND p.pull_request_id = pr.id AND p.repo_id = $2";

/// Whether the PR changes that repository, as publication decides
/// (`pull_request_adapters::changes_member`): its recorded versions differ and
/// its preflight is neither identical nor already merged.
pub(super) const CHANGES_REPOSITORY: &str = "\
    (t.head IS NULL OR s.head ->> 'headCommit' IS DISTINCT FROM t.head ->> 'headCommit') \
    AND NOT jsonb_path_exists(COALESCE(pr.preflight, '{}'::jsonb), \
        '$.repoResults[*] ? (@.repoId == $repo && (@.outcome == \"identical\" || @.outcome == \"sourceAlreadyMerged\"))', \
        jsonb_build_object('repo', $2::text))";

/// Two newest-created-first listings merged into one, capped at `limit`,
/// and whether the cap dropped any.
fn newest_first(github: Vec<Value>, local: Vec<Value>, limit: usize) -> (Vec<Value>, bool) {
    let created = |pull: &Value| {
        pull["createdAt"]
            .as_str()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
    };
    let mut merged = Vec::with_capacity(github.len() + local.len());
    let (mut github, mut local) = (github.into_iter().peekable(), local.into_iter().peekable());
    loop {
        let take_local = match (github.peek(), local.peek()) {
            (None, None) => break,
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (Some(g), Some(l)) => created(l) > created(g),
        };
        merged.extend(if take_local {
            local.next()
        } else {
            github.next()
        });
    }
    let more = merged.len() > limit;
    merged.truncate(limit);
    (merged, more)
}

/// Why a repository listing answers from G5's stored PRs
/// (`g5.git.pr-list.local`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoredFallback {
    /// Never expected on GitHub, so no notice is due (`g5.git.pr-no-github`).
    NotOnGithub,
    /// On GitHub, but without a usable integration.
    NotConnected,
}

impl StoredFallback {
    fn reason(self) -> &'static str {
        match self {
            Self::NotOnGithub => "This repository is not on GitHub",
            Self::NotConnected => "GitHub is not connected for this repository",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str, url: &str) -> ReadableRepository {
        ReadableRepository {
            org_id: Uuid::nil(),
            repo_id: Uuid::now_v7(),
            git_url: url.into(),
            name: name.into(),
            slug: name.to_ascii_lowercase(),
            provider: None,
        }
    }

    fn github(url: &str) -> Option<String> {
        url.strip_prefix("https://github.com/").map(str::to_owned)
    }

    #[test]
    fn github_and_local_rows_merge_newest_first_under_the_cap() {
        let row = |at: &str, n: i64| json!({"createdAt": at, "number": n});
        let github = vec![
            row("2026-10-03T00:00:00Z", 3),
            row("2026-10-01T00:00:00Z", 1),
        ];
        let local = vec![row("2026-10-02T00:00:00.5+00:00", 2)];
        let (merged, more) = newest_first(github.clone(), local.clone(), 10);
        let numbers: Vec<i64> = merged
            .iter()
            .map(|p| p["number"].as_i64().unwrap())
            .collect();
        assert_eq!(numbers, vec![3, 2, 1]);
        assert!(!more);
        let (merged, more) = newest_first(github, local, 2);
        assert_eq!(merged.len(), 2);
        assert!(more);
    }

    #[test]
    fn selection_prefers_github_and_names_local_repositories_by_g5_name() {
        let repos = || {
            vec![
                repo("G5-Lean", "https://github.com/acme/api"),
                repo("Docs", "http://g5/git/docs"),
            ]
        };
        let (chosen, name) = select_repository(repos(), None, github).unwrap();
        assert_eq!(
            (chosen.name.as_str(), name.as_deref()),
            ("G5-Lean", Some("acme/api"))
        );
        let (chosen, name) = select_repository(repos(), Some("ACME/API"), github).unwrap();
        assert_eq!(
            (chosen.name.as_str(), name.as_deref()),
            ("G5-Lean", Some("acme/api"))
        );
        let (chosen, name) = select_repository(repos(), Some("docs"), github).unwrap();
        assert_eq!((chosen.name.as_str(), name), ("Docs", None));
        // A GitHub repository is named by OWNER/NAME only.
        assert!(select_repository(repos(), Some("G5-Lean"), github).is_err());
        assert!(select_repository(repos(), Some("other/repo"), github).is_err());
        // Without GitHub, the single repository is used.
        let (chosen, _) =
            select_repository(vec![repo("Docs", "http://g5/git/docs")], None, github).unwrap();
        assert_eq!(chosen.name, "Docs");
        // Several GitHub repositories need --repo.
        let several = vec![
            repo("A", "https://github.com/acme/a"),
            repo("B", "https://github.com/acme/b"),
        ];
        assert!(select_repository(several, None, github).is_err());
    }

    #[test]
    fn like_patterns_match_words_literally() {
        assert_eq!(escape_like("50%_a\\b"), "50\\%\\_a\\\\b");
    }
}
