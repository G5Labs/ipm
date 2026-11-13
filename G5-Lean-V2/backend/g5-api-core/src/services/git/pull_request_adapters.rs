//! Repository pull requests and the adapters that serve them
//! (`g5.git.pr-repository`, `g5.git.pr-adapter`).
//!
//! A G5 PR has one repository pull request for each member it changes. Each
//! is served by the adapter its member binds: GitHub when the member has a
//! GitHub link or publication would publish it to GitHub, otherwise the local
//! adapter. A local repository pull request is a projection of its G5 PR (the
//! G5 PR's number and lifecycle, the recorded heads and the canonical
//! description) and holds no state of its own, so a member that becomes
//! published simply stops having one. This module is the one owner of that
//! binding: the PR service asks it, and no surface decides it for itself.

use std::collections::{HashMap, HashSet};

use g5_authz::ScopedDb;
use g5_contracts::G5Error;
use g5_scm::providers::{ProviderEndpoints, ProviderError};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use super::coordinated_repo_publication::{
    WorkspacePullRequestProviderRepoSummary, WorkspacePullRequestSummary,
};
use super::workspace_merge_service::{MergePreflightRepoOutcome, MergePreflightRepoResult};

/// Which adapter serves a repository pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PullRequestAdapter {
    #[serde(rename = "github")]
    Github,
    /// G5's own: a projection of the G5 PR.
    #[serde(rename = "g5")]
    Local,
}

/// One repository pull request of a G5 PR, as its adapter reports it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryPullRequestSummary {
    pub repo_id: Uuid,
    /// GitHub's `owner/name`, or G5's repository name for a local one.
    pub repository: String,
    pub adapter: PullRequestAdapter,
    /// GitHub's number; `None` for a GitHub one not yet published and for a
    /// local one, which is named by its G5 PR (`PR######`,
    /// `g5.git.pr-local-number`).
    pub number: Option<i64>,
    pub url: Option<String>,
    /// `OPEN`, `MERGED` or `CLOSED`; `None` for a GitHub one not yet
    /// published. A GitHub one's state is the last recorded observation.
    pub state: Option<String>,
    /// The GitHub adapter's recorded publication status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_status: Option<String>,
    /// GitHub's head and base branches, or the recorded G5 branches for a
    /// local one.
    pub head_ref_name: Option<String>,
    pub base_ref_name: Option<String>,
    pub head_ref_oid: String,
    pub base_ref_oid: String,
    /// The GitHub adapter's publication record (published heads, last error,
    /// merge evidence), for a GitHub one that has been recorded. Surfaces read
    /// GitHub publication facts here rather than from the G5 PR's raw records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<WorkspacePullRequestProviderRepoSummary>,
}

/// A G5 PR's name, `PR######`: its number as six zero-padded lowercase hex
/// digits (`PR3fa9c1`), which never reads as a GitHub number
/// (`g5.git.pr-local-number`).
pub fn g5_pull_request_name(number: i64) -> String {
    format!("PR{number:06x}")
}

/// The number in a G5 PR's name, `PR` and exactly six hex digits, in any
/// case; `None` for anything else, such as a bare GitHub number.
pub fn parse_g5_pull_request_name(selector: &str) -> Option<i64> {
    let digits = selector
        .get(..2)
        .filter(|prefix| prefix.eq_ignore_ascii_case("PR"))
        .map(|_| &selector[2..])?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    i64::from_str_radix(digits, 16)
        .ok()
        .filter(|number| *number > 0)
}

/// Serializes a G5 PR's number as its name, `PR######`.
pub fn serialize_g5_pull_request_name<S: serde::Serializer>(
    number: &i64,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&g5_pull_request_name(*number))
}

/// Whether GitHub is expected to report on a repository pull request
/// (`g5.git.pr-no-github`): a recorded GitHub one, or an open PR's GitHub one
/// that publication has yet to publish.
pub fn expects_github(repository: &RepositoryPullRequestSummary, pull_status: &str) -> bool {
    repository.adapter == PullRequestAdapter::Github
        && (repository.github.is_some() || pull_status == "open")
}

/// What Activity names a G5 PR's GitHub publication by
/// (`g5.git.pr-adapter`): how many GitHub repository pull requests it has,
/// and the one when there is exactly one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GithubPublicationLabel {
    pub repository_count: i64,
    pub repository: Option<String>,
    pub number: Option<i64>,
    pub url: Option<String>,
}

/// The per-PR half of the GitHub publication rule as SQL over `pr` and the
/// repository bound as `$2`: both workspaces track the repository remotely.
/// With the repository-level half ([`PullRequestAdapters::is_github_repository`])
/// it is exactly [`PullRequestAdapters::github_publication_target`]'s rule,
/// whose row is `remote_integrations::provider_candidate`'s join of the same
/// two rows (`remote_branch` is never null).
pub(crate) const PUBLISHES_TO_GITHUB_SQL: &str =
    super::remote_integrations::PULL_REQUEST_TRACKS_REPOSITORY_SQL;

/// Name each G5 PR's GitHub publication for Activity, from the GitHub
/// adapter's records.
pub async fn github_publication_labels(
    conn: &mut PgConnection,
    pull_request_ids: &[Uuid],
) -> Result<HashMap<Uuid, GithubPublicationLabel>, G5Error> {
    if pull_request_ids.is_empty() {
        return Ok(HashMap::new());
    }
    type LabelRow = (Uuid, i64, Option<String>, Option<i64>, Option<String>);
    let rows: Vec<LabelRow> = sqlx::query_as(
        "SELECT pull_request_id, COUNT(*), \
                CASE WHEN COUNT(*) = 1 THEN MIN(provider_repo_full_name) END, \
                CASE WHEN COUNT(*) = 1 THEN MIN(provider_pr_number) END, \
                CASE WHEN COUNT(*) = 1 THEN MIN(provider_pr_url) END \
         FROM workspace_pull_request_provider_repos \
         WHERE pull_request_id = ANY($1) GROUP BY pull_request_id",
    )
    .bind(pull_request_ids)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, repository_count, repository, number, url)| {
            (
                id,
                GithubPublicationLabel {
                    repository_count,
                    repository,
                    number,
                    url,
                },
            )
        })
        .collect())
}

/// The open G5 PR into `workspace_id` whose partial GitHub merge holds this
/// member's automatic import (`g5.governance.remote.pull`): its GitHub
/// repository pull request for `repo_id` is merged, or being merged, while a
/// sibling's is not. An in-flight GitHub merge holds the PR row for update,
/// which `SKIP LOCKED` detects without waiting.
pub(crate) async fn github_import_hold(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<Option<super::remote_integrations::ImportHold>, sqlx::Error> {
    sqlx::query_as(
        "SELECT pr.id AS pull_request_id, pr.title FROM workspace_pull_requests pr
         JOIN workspace_pull_request_provider_repos mine
           ON mine.pull_request_id = pr.id AND mine.org_id = pr.org_id AND mine.repo_id = $2
         WHERE pr.target_workspace_id = $1 AND pr.status = 'open'
           AND mine.provider_pr_number IS NOT NULL
           AND EXISTS (
               SELECT 1 FROM workspace_pull_request_provider_repos sibling
               WHERE sibling.pull_request_id = pr.id AND sibling.repo_id <> $2
                 AND (sibling.provider_status <> 'merged' OR mine.merge_pending))
           AND (mine.provider_status = 'merged' OR mine.merge_pending OR NOT EXISTS (
               SELECT 1 FROM workspace_pull_requests unlocked
               WHERE unlocked.id = pr.id FOR SHARE SKIP LOCKED))
         ORDER BY pr.created_at, pr.id
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(repo_id)
    .fetch_optional(conn)
    .await
}

/// Where publication would publish a member on GitHub: both workspaces track
/// a remote branch of a repository whose trusted origin is GitHub.
#[derive(Clone, Debug)]
pub(crate) struct GithubPublicationTarget {
    pub(crate) repo_id: Uuid,
    pub(crate) provider_full_name: String,
    pub(crate) git_url: String,
    pub(crate) source_remote_branch: String,
    pub(crate) target_remote_branch: String,
}

/// Whether a G5 PR changes a member, as publication decides: the recorded
/// versions differ and the preflight finds it neither identical nor already
/// merged. An unchanged member has no repository pull request.
pub(crate) fn changes_member(repo: &MergePreflightRepoResult) -> bool {
    repo.source_head != repo.target_head
        && !matches!(
            repo.outcome,
            MergePreflightRepoOutcome::Identical | MergePreflightRepoOutcome::SourceAlreadyMerged
        )
}

/// The adapter binding and the GitHub publication rule, shared by the PR
/// service's reads and publication.
pub struct PullRequestAdapters {
    origin_policy: g5_scm::ScmOriginPolicy,
    endpoints: Result<ProviderEndpoints, ProviderError>,
}

impl PullRequestAdapters {
    pub fn new(
        origin_policy: g5_scm::ScmOriginPolicy,
        github_api_base_url: &str,
        github_web_base_url: &str,
    ) -> Self {
        Self {
            origin_policy,
            endpoints: ProviderEndpoints::new(
                g5_scm::ScmProvider::GitHub,
                github_api_base_url,
                github_web_base_url,
            ),
        }
    }

    /// The repository-level half of the GitHub publication rule: the
    /// repository's trusted origin is GitHub and names a GitHub repository.
    pub(crate) fn is_github_repository(&self, provider: Option<&str>, git_url: &str) -> bool {
        self.origin_policy
            .effective_provider(provider, git_url)
            .is_some_and(|p| p.is_github())
            && self
                .endpoints
                .as_ref()
                .is_ok_and(|endpoints| endpoints.repository_name_from_clone_url(git_url).is_some())
    }

    /// Where publication would publish `repo` on GitHub, if anywhere.
    pub(crate) async fn github_publication_target(
        &self,
        conn: &mut PgConnection,
        org_id: Uuid,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        repo_id: Uuid,
    ) -> Result<Option<GithubPublicationTarget>, G5Error> {
        let Some(row) = super::remote_integrations::provider_candidate(
            conn,
            org_id,
            source_workspace_id,
            target_workspace_id,
            repo_id,
        )
        .await?
        else {
            return Ok(None);
        };
        // Endpoints are checked only below, so a misconfiguration fails
        // loudly instead of reading as "not on GitHub".
        if !self
            .origin_policy
            .effective_provider(row.provider.as_deref(), &row.git_url)
            .is_some_and(|p| p.is_github())
            || row.source_tracking_mode != "remote_tracking"
            || row.target_tracking_mode != "remote_tracking"
        {
            return Ok(None);
        }
        let (Some(source_remote_branch), Some(target_remote_branch)) =
            (row.source_remote_branch, row.target_remote_branch)
        else {
            return Ok(None);
        };
        // User grants are broader than installation tokens. The API target
        // must share the Git URL identity checked by RepositoryCredentials;
        // imported/cached provider_full_name is not an authorization boundary.
        let endpoints = self.endpoints.as_ref().map_err(|error| G5Error::Internal {
            message: format!("GitHub endpoints are misconfigured: {error}"),
        })?;
        let Some(provider_full_name) = endpoints.repository_name_from_clone_url(&row.git_url)
        else {
            return Ok(None);
        };
        Ok(Some(GithubPublicationTarget {
            repo_id: row.repo_id,
            provider_full_name,
            git_url: row.git_url,
            source_remote_branch,
            target_remote_branch,
        }))
    }

    /// Give each summary its repository pull requests, one per member it
    /// changes (plus any recorded GitHub one), in recorded member order.
    pub async fn attach(
        &self,
        db: &mut ScopedDb,
        summaries: &mut [WorkspacePullRequestSummary],
    ) -> Result<(), G5Error> {
        if summaries.is_empty() {
            return Ok(());
        }
        let members: Vec<Vec<RecordedMember>> = summaries.iter().map(recorded_members).collect();
        let repo_ids = members
            .iter()
            .flatten()
            .map(|member| member.repo_id)
            .collect::<HashSet<_>>();
        let names = repository_names(db, repo_ids.into_iter().collect()).await?;
        let org_id = summary_org(db)?;
        for (summary, members) in summaries.iter_mut().zip(members) {
            // A changed member without a GitHub record is GitHub's when
            // publication would publish it there, whatever the PR's status:
            // open, it is not yet published; ended, it never was.
            let mut pending = HashSet::new();
            {
                for member in members.iter().filter(|member| member.changed) {
                    let recorded = summary
                        .provider_repos
                        .iter()
                        .any(|row| row.repo_id == member.repo_id);
                    if !recorded
                        && self
                            .github_publication_target(
                                db.conn(),
                                org_id,
                                summary.source_workspace_id,
                                summary.target_workspace_id,
                                member.repo_id,
                            )
                            .await?
                            .is_some()
                    {
                        pending.insert(member.repo_id);
                    }
                }
            }
            summary.repository_pull_requests = bind(summary, &members, &names, &pending);
        }
        Ok(())
    }
}

/// One member a PR recorded: its recorded source and target versions, and
/// whether the PR changes it as publication decides.
struct RecordedMember {
    repo_id: Uuid,
    source_head: String,
    target_head: String,
    source_branch: Option<String>,
    target_branch: Option<String>,
    changed: bool,
}

/// The members a PR recorded, from its recorded heads: a member changes when
/// its versions differ and its preflight finds it neither identical nor
/// already merged (the same rule as [`changes_member`] and the stored
/// listing's). Malformed evidence yields none rather than inventing members.
fn recorded_members(summary: &WorkspacePullRequestSummary) -> Vec<RecordedMember> {
    let heads = |value: &serde_json::Value| -> Vec<(Uuid, (String, Option<String>))> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|head| {
                Some((
                    Uuid::parse_str(head.get("repoId")?.as_str()?).ok()?,
                    (
                        head.get("headCommit")?.as_str()?.to_owned(),
                        head.get("branchName")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned),
                    ),
                ))
            })
            .collect()
    };
    let targets: HashMap<Uuid, (String, Option<String>)> =
        heads(&summary.target_heads).into_iter().collect();
    let outcomes: HashMap<Uuid, MergePreflightRepoOutcome> = summary
        .preflight
        .get("repoResults")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|result| {
            Some((
                Uuid::parse_str(result.get("repoId")?.as_str()?).ok()?,
                serde_json::from_value(result.get("outcome")?.clone()).ok()?,
            ))
        })
        .collect();
    // Members the preflight evidence lists without recorded heads still count.
    let mut sources = heads(&summary.source_heads);
    let mut targets = targets;
    for result in summary
        .preflight
        .get("repoResults")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(repo_id), Some(source), Some(target)) = (
            result
                .get("repoId")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok()),
            result.get("sourceHead").and_then(serde_json::Value::as_str),
            result.get("targetHead").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        if sources.iter().any(|(id, _)| *id == repo_id) {
            continue;
        }
        let branch = |key: &str| {
            result
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        sources.push((repo_id, (source.to_owned(), branch("sourceBranchName"))));
        targets
            .entry(repo_id)
            .or_insert_with(|| (target.to_owned(), branch("targetBranchName")));
    }
    sources
        .into_iter()
        .map(|(repo_id, (source_head, source_branch))| {
            let (target_head, target_branch) = targets.get(&repo_id).cloned().unwrap_or_default();
            let changed = source_head != target_head
                && !matches!(
                    outcomes.get(&repo_id),
                    Some(
                        MergePreflightRepoOutcome::Identical
                            | MergePreflightRepoOutcome::SourceAlreadyMerged
                    )
                );
            RecordedMember {
                repo_id,
                source_head,
                target_head,
                source_branch,
                target_branch,
                changed,
            }
        })
        .collect()
}

fn summary_org(db: &ScopedDb) -> Result<Uuid, G5Error> {
    db.primary_org_id()
        .map(|org| org.into_uuid())
        .map_err(|error| G5Error::Internal {
            message: format!("PR organization: {error}"),
        })
}

async fn repository_names(
    db: &mut ScopedDb,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, String>, G5Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM repos WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(db.conn())
        .await?;
    Ok(rows.into_iter().collect())
}

/// The binding (`g5.git.pr-repository`): a recorded GitHub link or a pending
/// GitHub publication is GitHub's; any other changed member is local.
fn bind(
    summary: &WorkspacePullRequestSummary,
    members: &[RecordedMember],
    names: &HashMap<Uuid, String>,
    pending_github: &HashSet<Uuid>,
) -> Vec<RepositoryPullRequestSummary> {
    let mut bound = Vec::new();
    let mut seen = HashSet::new();
    for repo in members {
        let recorded = summary
            .provider_repos
            .iter()
            .find(|row| row.repo_id == repo.repo_id);
        let open = summary.status == "open";
        if let Some(row) = recorded {
            bound.push(github_row(row, &repo.source_head, &repo.target_head));
        } else if pending_github.contains(&repo.repo_id) {
            // Not published: an open PR's is pending; an ended PR's never was
            // and shares the G5 PR's end.
            bound.push(RepositoryPullRequestSummary {
                repo_id: repo.repo_id,
                repository: names.get(&repo.repo_id).cloned().unwrap_or_default(),
                adapter: PullRequestAdapter::Github,
                number: None,
                url: None,
                state: (!open).then(|| summary.status.to_ascii_uppercase()),
                provider_status: open.then(|| "pending_create".to_string()),
                head_ref_name: None,
                base_ref_name: None,
                head_ref_oid: repo.source_head.clone(),
                base_ref_oid: repo.target_head.clone(),
                github: None,
            });
        } else if repo.changed {
            bound.push(RepositoryPullRequestSummary {
                repo_id: repo.repo_id,
                repository: names.get(&repo.repo_id).cloned().unwrap_or_default(),
                adapter: PullRequestAdapter::Local,
                number: None,
                url: None,
                state: Some(summary.status.to_ascii_uppercase()),
                provider_status: None,
                head_ref_name: repo.source_branch.clone(),
                base_ref_name: repo.target_branch.clone(),
                head_ref_oid: repo.source_head.clone(),
                base_ref_oid: repo.target_head.clone(),
                github: None,
            });
        } else {
            continue;
        }
        seen.insert(repo.repo_id);
    }
    // A recorded GitHub PR for a member the evidence no longer lists stays.
    for row in &summary.provider_repos {
        if seen.insert(row.repo_id) {
            bound.push(github_row(
                row,
                row.source_head_sha.as_deref().unwrap_or_default(),
                row.target_head_sha.as_deref().unwrap_or_default(),
            ));
        }
    }
    bound
}

fn github_row(
    row: &WorkspacePullRequestProviderRepoSummary,
    head: &str,
    base: &str,
) -> RepositoryPullRequestSummary {
    let state = match (row.provider_status.as_str(), row.provider_pr_number) {
        ("merged", _) => Some("MERGED".to_string()),
        (_, Some(_)) => Some("OPEN".to_string()),
        (_, None) => None,
    };
    RepositoryPullRequestSummary {
        repo_id: row.repo_id,
        repository: row.provider_repo_full_name.clone(),
        adapter: PullRequestAdapter::Github,
        number: row.provider_pr_number,
        url: row.provider_pr_url.clone(),
        state,
        provider_status: Some(row.provider_status.clone()),
        head_ref_name: Some(row.source_remote_branch.clone()),
        base_ref_name: Some(row.target_remote_branch.clone()),
        head_ref_oid: row
            .source_head_sha
            .clone()
            .unwrap_or_else(|| head.to_string()),
        base_ref_oid: base.to_string(),
        github: Some(row.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(repo_id: Uuid, source: &str, target: &str, outcome: &str) -> serde_json::Value {
        serde_json::json!({
            "repoId": repo_id,
            "sourceWorkspaceId": Uuid::nil(),
            "targetWorkspaceId": Uuid::nil(),
            "sourceBranchName": "child",
            "targetBranchName": "main",
            "sourceHead": source,
            "targetHead": target,
            "outcome": outcome,
            "conflictPaths": [],
            "message": null,
        })
    }

    #[test]
    fn changed_members_bind_github_or_local_and_unchanged_ones_have_none() {
        let (github, local, unchanged, merged) = (
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let members: Vec<MergePreflightRepoResult> = serde_json::from_value(serde_json::json!([
            member(github, "a1", "b1", "fastForward"),
            member(local, "a2", "b2", "fastForward"),
            member(unchanged, "same", "same", "identical"),
            member(merged, "a4", "b4", "sourceAlreadyMerged"),
        ]))
        .expect("members");
        assert!(changes_member(&members[0]) && changes_member(&members[1]));
        assert!(!changes_member(&members[2]) && !changes_member(&members[3]));
    }

    #[test]
    fn g5_pull_request_names_never_read_as_github_numbers() {
        assert_eq!(g5_pull_request_name(7), "PR000007");
        assert_eq!(g5_pull_request_name(0x3f_a9c1), "PR3fa9c1");
        assert_eq!(g5_pull_request_name(0xff_ffff), "PRffffff");
        assert_eq!(parse_g5_pull_request_name("PR3fa9c1"), Some(0x3f_a9c1));
        assert_eq!(parse_g5_pull_request_name("pr3FA9C1"), Some(0x3f_a9c1));
        assert_eq!(parse_g5_pull_request_name("PR000012"), Some(0x12));
        for other in [
            "7",
            "#7",
            "PR",
            "PR0",
            "PR12",
            "PR3fa9c",
            "PR000000",
            "PR3fa9c10",
            "PR-3fa9c",
            "PR3fa9cg",
            "PRR7",
            "pull/7",
            "P",
            "prefab",
        ] {
            assert_eq!(parse_g5_pull_request_name(other), None, "{other}");
        }
    }

    #[test]
    fn github_is_expected_for_a_recorded_one_or_an_open_prs_unpublished_one() {
        let repository = |adapter, github| RepositoryPullRequestSummary {
            repo_id: Uuid::nil(),
            repository: "acme/app".to_owned(),
            adapter,
            number: None,
            url: None,
            state: None,
            provider_status: None,
            head_ref_name: None,
            base_ref_name: None,
            head_ref_oid: String::new(),
            base_ref_oid: String::new(),
            github,
        };
        let record = WorkspacePullRequestProviderRepoSummary {
            id: Uuid::nil(),
            repo_id: Uuid::nil(),
            provider: "github".to_owned(),
            provider_repo_full_name: "acme/app".to_owned(),
            source_remote_branch: "child".to_owned(),
            target_remote_branch: "main".to_owned(),
            provider_pr_number: Some(7),
            provider_pr_url: None,
            provider_status: "closed".to_owned(),
            source_head_sha: None,
            target_head_sha: None,
            merge_target_head_sha: None,
            merge_pending: false,
            provider_head_sha: None,
            provider_base_sha: None,
            last_error: None,
            last_synced_at: None,
            updated_at: chrono::Utc::now(),
            merged_outside_g5: false,
        };
        let unpublished = repository(PullRequestAdapter::Github, None);
        assert!(expects_github(&unpublished, "open"));
        assert!(!expects_github(&unpublished, "closed"));
        let recorded = repository(PullRequestAdapter::Github, Some(record));
        assert!(expects_github(&recorded, "closed"));
        assert!(!expects_github(
            &repository(PullRequestAdapter::Local, None),
            "open"
        ));
    }
}
