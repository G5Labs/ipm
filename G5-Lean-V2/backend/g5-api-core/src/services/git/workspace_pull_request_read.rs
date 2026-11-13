//! Read-only provider lookup. Callers authorize the workspace before entering;
//! selectors can only choose one of the repositories that workspace reads PRs
//! from (`g5.git.pr-read-repositories`).
#[path = "workspace_pull_request_attachments.rs"]
mod attachments;
#[path = "workspace_pull_request_checks.rs"]
mod checks;
#[path = "workspace_pull_request_list.rs"]
mod list;
pub use attachments::{
    AttachedPullRequestDTO, PullRequestAttachmentsDTO, LOCAL_ATTACHMENTS_REFUSAL,
};
pub use checks::{external_checks_verdict, read_checks, ExternalChecksVerdict};
pub(super) use list::select_repository;
pub use list::{PullRequestListQuery, ReadablePullRequestListQuery};

use super::{token_from_auth, WorkspacePullRequestProviderLifecycleService};
use crate::auth::RepositoryOperation;
use crate::services::git::workspace_audience::{pr_audience, readable};
use g5_authz::ScopedDb;
use g5_contracts::G5Error;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PullRequestReadQuery {
    pub selector: Option<String>,
    pub repo: Option<String>,
    #[serde(default)]
    pub comments: bool,
    #[serde(default)]
    pub reviews: bool,
    #[serde(default)]
    pub checks: bool,
    #[serde(default)]
    pub details: bool,
    pub head: Option<String>,
    /// `g5d pr list --all`: every readable G5 PR (`g5.git.pr-reads`).
    #[serde(default)]
    pub all: bool,
    /// `g5d pr list --workspace WS`: that workspace's G5 PRs, if readable.
    pub workspace: Option<String>,
    /// Filters of the `--all`/`--workspace` listing.
    pub state: Option<String>,
    pub author: Option<String>,
    pub search: Option<String>,
    pub limit: Option<u32>,
    /// Read the base branch's required checks from GitHub rather than the
    /// cache. Set only by the merge gate; never taken from a request.
    #[serde(skip)]
    pub fresh_requirements: bool,
}

pub(super) fn invalid(message: &str) -> G5Error {
    G5Error::ValidationError {
        details: json!({"message": message}),
    }
}

pub(super) fn selection(
    endpoints: &g5_scm::providers::ProviderEndpoints,
    selector: &str,
    repo: Option<&str>,
) -> Result<(i64, Option<String>), G5Error> {
    endpoints
        .select_pull_request(selector, repo)
        .map_err(|error| invalid(&error.to_string()))
}

/// Who reads (`g5.git.pr-reads`): an organization member, as themselves or
/// through their workspace's agent, and whether they read every workspace.
#[derive(Clone, Copy, Debug)]
pub struct PullRequestReader {
    pub org_id: Uuid,
    pub actor: Uuid,
    pub org_wide: bool,
}

/// Which readable G5 PRs to return (`g5.git.pr-reads`). Every filter is
/// optional; `limit` caps the rows.
#[derive(Debug, Default)]
pub struct ReadablePullRequestFilter<'a> {
    pub ids: Option<&'a [Uuid]>,
    /// PRs into or out of this workspace.
    pub workspace: Option<Uuid>,
    pub statuses: Option<Vec<&'a str>>,
    pub authors: Option<Vec<Uuid>>,
    /// Every word must appear in the title or saved description.
    pub search: Option<&'a str>,
    pub limit: Option<i64>,
}

impl WorkspacePullRequestProviderLifecycleService {
    /// Organization administrators read every live workspace
    /// (`g5.workspace.audience`).
    pub(super) async fn org_wide_reader(&self, org_id: Uuid, actor: Uuid) -> Result<bool, G5Error> {
        crate::services::git::workspace_audience::org_wide_reader(&self.system_pool, org_id, actor)
            .await
    }

    /// The reader of the scoped connection's organization.
    pub async fn pull_request_reader(
        &self,
        db: &ScopedDb,
        actor: Uuid,
    ) -> Result<PullRequestReader, G5Error> {
        let org_id = db
            .primary_org_id()
            .map_err(|e| G5Error::Internal {
                message: format!("PR reader organization: {e}"),
            })?
            .into_uuid();
        Ok(PullRequestReader {
            org_id,
            actor,
            org_wide: self.org_wide_reader(org_id, actor).await?,
        })
    }

    /// The readable G5 PRs (`g5.git.pr-reads`), newest created first: those
    /// whose source or target workspace the reader can open, or could open
    /// before it was deleted. The one read
    /// check every read-only PR route uses.
    pub async fn readable_pull_request_ids(
        &self,
        db: &mut ScopedDb,
        reader: PullRequestReader,
        filter: &ReadablePullRequestFilter<'_>,
    ) -> Result<Vec<Uuid>, G5Error> {
        let words: Vec<String> = filter
            .search
            .map(|text| {
                text.split_whitespace()
                    .map(|word| format!("%{}%", escape_like(word)))
                    .collect()
            })
            .unwrap_or_default();
        let ids: Option<Vec<Uuid>> = filter.ids.map(<[Uuid]>::to_vec);
        Ok(sqlx::query_scalar(&format!(
            "SELECT pr.id FROM workspace_pull_requests pr \
             JOIN workspaces sw ON sw.org_id = pr.org_id AND sw.id = pr.source_workspace_id \
             JOIN workspaces tw ON tw.org_id = pr.org_id AND tw.id = pr.target_workspace_id \
             WHERE pr.org_id = $1 AND ({sw} OR {tw}) \
               AND ($4::uuid[] IS NULL OR pr.id = ANY($4)) \
               AND ($5::uuid IS NULL OR pr.source_workspace_id = $5 OR pr.target_workspace_id = $5) \
               AND ($6::text[] IS NULL OR pr.status = ANY($6)) \
               AND ($7::uuid[] IS NULL OR pr.created_by = ANY($7)) \
               AND NOT EXISTS (SELECT 1 FROM unnest($8::text[]) AS word \
                               WHERE NOT (pr.title ILIKE word \
                                          OR COALESCE(pr.description_override, '') ILIKE word)) \
             ORDER BY pr.created_at DESC, pr.id DESC \
             LIMIT $9",
            sw = pr_audience("sw", 2, 3),
            tw = pr_audience("tw", 2, 3),
        ))
        .bind(reader.org_id)
        .bind(reader.org_wide)
        .bind(reader.actor)
        .bind(ids)
        .bind(filter.workspace)
        .bind(filter.statuses.clone())
        .bind(filter.authors.clone())
        .bind(&words)
        .bind(filter.limit)
        .fetch_all(db.conn())
        .await?)
    }

    /// Fails as not found unless the reader may read the PR (`g5.git.pr-reads`).
    pub async fn ensure_readable_pull_request(
        &self,
        db: &mut ScopedDb,
        reader: PullRequestReader,
        pull_request_id: Uuid,
    ) -> Result<(), G5Error> {
        let ids = [pull_request_id];
        let filter = ReadablePullRequestFilter {
            ids: Some(&ids),
            ..Default::default()
        };
        if self
            .readable_pull_request_ids(db, reader, &filter)
            .await?
            .is_empty()
        {
            return Err(G5Error::NotFound {
                resource: format!("workspace pull request {pull_request_id}"),
            });
        }
        Ok(())
    }

    pub async fn read_github_pull_request(
        &self,
        db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        query: &PullRequestReadQuery,
    ) -> Result<Value, G5Error> {
        let (number, selected_repo) = selection(
            &self.provider_endpoints()?,
            query
                .selector
                .as_deref()
                .ok_or_else(|| invalid("A GitHub PR selector is required"))?,
            query.repo.as_deref(),
        )?;
        let GithubPullRequestAccess {
            client,
            org,
            repo_id,
            scope,
            ..
        } = self
            .github_pull_request_access(db, workspace_id, actor, selected_repo.as_deref())
            .await?;
        let pull = client
            .read_pull_request(
                number,
                g5_scm::providers::PullRequestReadScope {
                    comments: query.comments,
                    reviews: query.reviews,
                },
            )
            .await
            .map_err(|error| {
                // Client errors are logged by code only; keep the provider's reason.
                tracing::warn!(
                    workspace_id = %workspace_id,
                    repo_id = %repo_id,
                    pull_request = number,
                    error = %error,
                    "GitHub PR read failed"
                );
                G5Error::from(error)
            })?;
        let mut pulls = [pull];
        self.attach_g5_pull_requests(&scope, org, repo_id, actor, &mut pulls)
            .await?;
        let [pull] = pulls;
        Ok(pull)
    }
}

/// A GitHub repository's pull requests as the owner reads them: the
/// selected repository and a client holding the owner's read grant.
pub(super) struct GithubPullRequestAccess {
    pub client: g5_scm::providers::RepositoryClient,
    /// `owner/name`.
    pub name: String,
    pub org: Uuid,
    pub repo_id: Uuid,
    pub scope: g5_authz::ScopedDbReopener,
}

impl WorkspacePullRequestProviderLifecycleService {
    /// Select the repository (`g5.git.pr-read-repositories`) and mint the
    /// owner's pull-request read grant for it. The read transaction ends
    /// before token minting or provider I/O.
    pub(super) async fn github_pull_request_access(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        selected_repo: Option<&str>,
    ) -> Result<GithubPullRequestAccess, G5Error> {
        let reader = self.pull_request_reader(&db, actor).await?;
        let (org, repo_id, git_url, name) = attached_repository(
            &mut db,
            workspace_id,
            selected_repo,
            &self.provider_endpoints()?,
            RepositoryUse::Read,
            Some(reader),
        )
        .await?;
        let scope = db.reopener();
        // Release the read transaction before token minting or provider I/O.
        db.rollback().await.map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?;
        let auth = self
            .credentials
            .resolve(
                org,
                repo_id,
                actor,
                &git_url,
                RepositoryOperation::ReadPullRequests,
            )
            .await?;
        let token = token_from_auth(&auth)
            .ok_or_else(|| invalid("GitHub repository credentials are unavailable"))?;
        let client = self.repository_protocol(
            &token,
            &name,
            g5_scm::providers::RepositoryCapability::PullRequestRead,
        )?;
        Ok(GithubPullRequestAccess {
            client,
            name,
            org,
            repo_id,
            scope,
        })
    }

    /// The client and repository name of [`Self::github_pull_request_access`].
    pub(super) async fn github_pull_request_client(
        &self,
        db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        selected_repo: Option<&str>,
    ) -> Result<(g5_scm::providers::RepositoryClient, String), G5Error> {
        let access = self
            .github_pull_request_access(db, workspace_id, actor, selected_repo)
            .await?;
        Ok((access.client, access.name))
    }
}

impl WorkspacePullRequestProviderLifecycleService {
    /// Read GitHub's PR by number or URL (`g5.git.pr-adapter`). A bare number
    /// is always GitHub's and prefers the workspace's GitHub repository; in a
    /// repository not on GitHub it names nothing, so it is refused with the
    /// G5 PR's name form (`g5.git.pr-local-number`).
    pub async fn read_repository_pull_request(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        query: &PullRequestReadQuery,
    ) -> Result<Value, G5Error> {
        let selector = query
            .selector
            .as_deref()
            .ok_or_else(|| invalid("A PR selector is required"))?;
        self.ensure_github_number(
            &mut db,
            workspace_id,
            actor,
            selector,
            query.repo.as_deref(),
        )
        .await?;
        self.read_github_pull_request(db, workspace_id, actor, query)
            .await
    }

    /// The G5 PR a selector names directly: its ID, or its name `PR######`
    /// among the PRs the reader may read (`g5.git.pr-local-number`,
    /// `g5.git.pr-reads`); `None` for a GitHub number, URL or branch. An ID
    /// is returned unchecked; callers check that it is readable.
    pub async fn g5_selection(
        &self,
        db: &mut ScopedDb,
        actor: Uuid,
        selector: &str,
    ) -> Result<Option<Uuid>, G5Error> {
        if let Ok(id) = Uuid::parse_str(selector) {
            return Ok(Some(id));
        }
        let Some(number) =
            crate::services::git::pull_request_adapters::parse_g5_pull_request_name(selector)
        else {
            return Ok(None);
        };
        let reader = self.pull_request_reader(db, actor).await?;
        let id: Option<Uuid> = sqlx::query_scalar(&format!(
            "SELECT pr.id FROM workspace_pull_requests pr \
             JOIN workspaces sw ON sw.org_id = pr.org_id AND sw.id = pr.source_workspace_id \
             JOIN workspaces tw ON tw.org_id = pr.org_id AND tw.id = pr.target_workspace_id \
             WHERE pr.org_id = $1 AND pr.number = $4 AND ({sw} OR {tw})",
            sw = pr_audience("sw", 2, 3),
            tw = pr_audience("tw", 2, 3),
        ))
        .bind(reader.org_id)
        .bind(reader.org_wide)
        .bind(reader.actor)
        .bind(number)
        .fetch_optional(db.conn())
        .await?;
        id.map(Some).ok_or_else(|| G5Error::NotFound {
            resource: format!(
                "G5 PR {} that you can read",
                crate::services::git::pull_request_adapters::g5_pull_request_name(number)
            ),
        })
    }

    /// A bare number (not a URL) is GitHub's (`g5.git.pr-local-number`):
    /// refuse one that selects a repository not on GitHub, pointing to the
    /// G5 PR's name instead. A bare number prefers the workspace's GitHub
    /// repository, and a named repository the workspace lacks is looked up
    /// among every readable workspace's members
    /// (`g5.git.pr-read-repositories`).
    pub(super) async fn ensure_github_number(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        selector: &str,
        named: Option<&str>,
    ) -> Result<(), G5Error> {
        match self
            .repository_not_on_github(db, workspace_id, actor, selector, named)
            .await?
        {
            Some(repository) => Err(not_github_number(&repository, selector)),
            None => Ok(()),
        }
    }

    /// The repository a bare number selects when that repository is not on
    /// GitHub, so the number names no GitHub pull request.
    pub(super) async fn repository_not_on_github(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor: Uuid,
        selector: &str,
        named: Option<&str>,
    ) -> Result<Option<String>, G5Error> {
        if selector.contains("://") {
            return Ok(None);
        }
        let endpoints = self.provider_endpoints()?;
        let github = |url: &str| endpoints.repository_name_from_clone_url(url);
        let mut repos = readable_repositories(db, workspace_id, RepositoryUse::Read).await?;
        if named.is_some() && list::select_repository(repos.clone(), named, github).is_err() {
            let reader = self.pull_request_reader(db, actor).await?;
            repos = audience_repositories(db, reader).await?;
        }
        Ok(match list::select_repository(repos, named, github) {
            Ok((repo, None)) => Some(repo.name),
            _ => None,
        })
    }
}

/// G5 PRs and their local repository pull requests carry no conversation
/// (`g5.git.pr-local-conversation`).
pub const LOCAL_CONVERSATION_REFUSAL: &str =
    "G5 PRs (PR######) and repositories not on GitHub have \
    no PR comments or review threads; discuss the work in the workspace's comments and reviews";

/// A bare number in a repository not on GitHub: G5 PRs are named `PR######`.
pub(super) fn not_github_number(repository: &str, selector: &str) -> G5Error {
    let number = selector.trim_start_matches('#');
    invalid(&format!(
        "{repository} is not on GitHub, so #{number} names no PR there; G5 PRs are named \
         PR and six hex digits: select one by its name, such as PR3fa9c1, or by its ID"
    ))
}

fn unavailable(message: &str) -> G5Error {
    G5Error::ServiceUnavailable {
        message: message.to_owned(),
    }
}

/// What a selected repository is used for (`g5.git.pr-read-repositories`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RepositoryUse {
    /// View, checks and listing: members, and a personal workspace's
    /// reference repositories (`g5.personal.pr-reads`).
    Read,
    /// Comments and thread resolution: members only.
    Write,
}

/// A repository a workspace reads PRs from.
#[derive(Clone, Debug)]
pub(super) struct ReadableRepository {
    pub org_id: Uuid,
    pub repo_id: Uuid,
    pub git_url: String,
    /// G5's repository name and slug, which name a repository not on GitHub.
    pub name: String,
    pub slug: String,
    /// The recorded provider, which must agree with the trusted origin.
    pub provider: Option<String>,
}

/// The workspace's members and, for reads in a personal workspace, the
/// current members of its organization's Base: the set its reference
/// checkouts come from, read per request as Git reads are
/// (`workspace_incarnation::personal_reference_readable`).
pub(super) async fn readable_repositories(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    purpose: RepositoryUse,
) -> Result<Vec<ReadableRepository>, G5Error> {
    let rows: Vec<(Uuid, Uuid, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT r.org_id, r.id, r.git_url, r.name, r.slug, r.provider FROM workspace_repos m \
         JOIN repos r ON (r.org_id,r.id)=(m.org_id,m.repo_id) \
         JOIN workspaces w ON (w.org_id,w.id)=(m.org_id,m.workspace_id) \
         WHERE m.workspace_id=$1 AND w.deleted_at IS NULL \
         UNION \
         SELECT r.org_id, r.id, r.git_url, r.name, r.slug, r.provider FROM workspaces p \
         JOIN workspaces base ON base.org_id=p.org_id AND base.is_base AND base.deleted_at IS NULL \
         JOIN workspace_repos m ON (m.org_id,m.workspace_id)=(base.org_id,base.id) \
         JOIN repos r ON (r.org_id,r.id)=(m.org_id,m.repo_id) \
         WHERE $2 AND p.id=$1 AND p.personal_user_id IS NOT NULL AND p.deleted_at IS NULL",
    )
    .bind(workspace_id)
    .bind(purpose == RepositoryUse::Read)
    .fetch_all(db.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(org_id, repo_id, git_url, name, slug, provider)| ReadableRepository {
                org_id,
                repo_id,
                git_url,
                name,
                slug,
                provider,
            },
        )
        .collect())
}

pub(super) fn escape_like(word: &str) -> String {
    word.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// The members of every workspace the reader can open (`g5.git.pr-reads`):
/// where a named repository is looked up once the workspace's own set has
/// none.
pub(super) async fn audience_repositories(
    db: &mut ScopedDb,
    reader: PullRequestReader,
) -> Result<Vec<ReadableRepository>, G5Error> {
    let rows: Vec<(Uuid, Uuid, String, String, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT DISTINCT r.org_id, r.id, r.git_url, r.name, r.slug, r.provider FROM workspace_repos m \
         JOIN repos r ON (r.org_id,r.id)=(m.org_id,m.repo_id) \
         JOIN workspaces w ON (w.org_id,w.id)=(m.org_id,m.workspace_id) \
         WHERE w.org_id = $1 AND {}",
        readable("w", 2, 3)
    ))
    .bind(reader.org_id)
    .bind(reader.org_wide)
    .bind(reader.actor)
    .fetch_all(db.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(org_id, repo_id, git_url, name, slug, provider)| ReadableRepository {
                org_id,
                repo_id,
                git_url,
                name,
                slug,
                provider,
            },
        )
        .collect())
}

/// The GitHub repository a PR read or write selects. Without a name, only the
/// workspace's own set is considered, so a bare number keeps meaning this
/// workspace's repository. A named repository the workspace lacks is, for a
/// read with a `reader`, looked up among the members of every workspace the
/// reader can open (`g5.git.pr-reads`).
pub(super) async fn attached_repository(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    selected_repo: Option<&str>,
    endpoints: &g5_scm::providers::ProviderEndpoints,
    purpose: RepositoryUse,
    reader: Option<PullRequestReader>,
) -> Result<(Uuid, Uuid, String, String), G5Error> {
    let github = |rows: Vec<ReadableRepository>| {
        let mut seen = std::collections::HashSet::new();
        rows.into_iter()
            .filter_map(|repo| {
                let name = endpoints.repository_name_from_clone_url(&repo.git_url)?;
                if selected_repo.is_some_and(|selected| !selected.eq_ignore_ascii_case(&name)) {
                    return None;
                }
                seen.insert(repo.repo_id)
                    .then_some((repo.org_id, repo.repo_id, repo.git_url, name))
            })
            .collect::<Vec<_>>()
    };
    let mut candidates = github(readable_repositories(db, workspace_id, purpose).await?);
    if candidates.is_empty() && selected_repo.is_some() && purpose == RepositoryUse::Read {
        if let Some(reader) = reader {
            candidates = github(audience_repositories(db, reader).await?);
        }
    }
    match candidates.len() {
        0 => Err(invalid(if reader.is_some() && selected_repo.is_some() {
            "No matching GitHub repository is in a workspace you can read"
        } else {
            "No matching GitHub repository is attached to this workspace"
        })),
        1 => Ok(candidates.remove(0)),
        _ => Err(invalid(
            "Multiple GitHub repositories match; specify --repo OWNER/REPO or a PR URL",
        )),
    }
}
