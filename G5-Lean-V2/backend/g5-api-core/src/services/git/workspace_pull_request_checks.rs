//! Live, revision-bound GitHub check observations behind the authorized PR reader.
use super::{
    attached_repository, invalid, selection, unavailable, PullRequestReadQuery, RepositoryUse,
};
use crate::auth::RepositoryOperation;
use crate::errors::AppError;
use crate::services::{coordinated_repo_publication::WorkspacePullRequestRole, AppState};
use g5_authz::ScopedDb;
use g5_contracts::G5Error;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

/// What `checks` says for a PR that expects no GitHub (`g5.git.pr-no-github`).
pub const NO_EXTERNAL_CHECKS: &str =
    "No external checks apply: this PR's repositories are not on GitHub.";

/// What `checks` says for one local repository pull request
/// (`g5.git.pr-no-github`): ready, with no checks, so the check policy reads
/// it as passing, as merge already treats a local-only member.
const LOCAL_REPOSITORY_NO_CHECKS: &str =
    "No external checks apply: this repository is not on GitHub.";

fn local_repository_checks(repository: &str, number: i64) -> Value {
    json!({
        "repository": repository,
        "tracker": "g5",
        "number": null,
        "g5Number": crate::services::git::pull_request_adapters::g5_pull_request_name(number),
        "availability": "ready",
        "noExternalChecks": true,
        "message": LOCAL_REPOSITORY_NO_CHECKS,
        "checks": [],
    })
}

/// Observe checks without changing publication, workspace, or CI state.
/// Both user and incarnation routes must first supply an authorized workspace scope.
pub async fn read_checks(
    state: &AppState,
    db: ScopedDb,
    workspace: Uuid,
    actor: Uuid,
    query: &PullRequestReadQuery,
) -> Result<Value, AppError> {
    tokio::time::timeout(
        Duration::from_secs(120),
        read_checks_inner(state, db, workspace, actor, query),
    )
    .await
    .map_err(|_| unavailable("GitHub check read timed out; retry the read"))?
}

async fn read_checks_inner(
    state: &AppState,
    mut db: ScopedDb,
    workspace: Uuid,
    actor: Uuid,
    query: &PullRequestReadQuery,
) -> Result<Value, AppError> {
    if query.comments || query.reviews || query.head.as_deref().is_some_and(|h| !valid_sha(h)) {
        return Err(invalid(
            "Checks accept a full 40-character head SHA and no comments or reviews option",
        )
        .into());
    }
    let service = &state.workspace_pull_request_provider_lifecycle_service;
    let reader = service.pull_request_reader(&db, actor).await?;
    let mut selected = Vec::new();
    let mut results = Vec::new();
    // Local repository pull requests: no external checks apply to them.
    let mut local = Vec::new();
    // A G5 PR ID or name selects any readable PR (`g5.git.pr-reads`).
    let g5_id = match query.selector.as_deref() {
        Some(selector) => service.g5_selection(&mut db, actor, selector).await?,
        None => None,
    };
    if let (Some(selector), None) = (query.selector.as_deref(), g5_id) {
        service
            .ensure_github_number(&mut db, workspace, actor, selector, query.repo.as_deref())
            .await?;
        let (number, repo) = selection(
            &service.provider_endpoints().map_err(G5Error::from)?,
            selector,
            query.repo.as_deref(),
        )?;
        let attached = attached_repository(
            &mut db,
            workspace,
            repo.as_deref(),
            &service.provider_endpoints().map_err(G5Error::from)?,
            RepositoryUse::Read,
            Some(reader),
        )
        .await?;
        selected.push((attached, number));
    } else {
        // None selects this workspace's outgoing PR.
        let mut pulls = match g5_id {
            Some(id) => {
                service
                    .ensure_readable_pull_request(&mut db, reader, id)
                    .await?;
                state
                    .coordinated_repo_publication_service
                    .list_workspace_pull_requests_by_ids(&mut db, &[id])
                    .await?
            }
            _ => {
                state
                    .coordinated_repo_publication_service
                    .list_workspace_pull_requests(
                        &mut db,
                        workspace,
                        WorkspacePullRequestRole::All,
                        None,
                    )
                    .await?
            }
        };
        pulls.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        let pull = match g5_id {
            Some(id) => pulls.iter().find(|p| p.id == id),
            None => pulls
                .iter()
                .find(|p| p.source_workspace_id == workspace && p.status == "open")
                .or_else(|| pulls.iter().find(|p| p.source_workspace_id == workspace)),
        }
        .ok_or_else(|| {
            invalid("No matching G5 PR; use g5d pr list --state all or specify a GitHub PR")
        })?;
        // Each repository pull request through its adapter
        // (`g5.git.pr-adapter`): a recorded GitHub one is read from GitHub, an
        // open PR's unpublished GitHub one is incomplete publication, and a
        // local one has no external checks.
        use crate::services::git::pull_request_adapters::{expects_github, PullRequestAdapter};
        let any_github = pull
            .repository_pull_requests
            .iter()
            .any(|r| expects_github(r, &pull.status));
        for repository in pull.repository_pull_requests.iter().filter(|repository| {
            query
                .repo
                .as_ref()
                .is_none_or(|s| s.eq_ignore_ascii_case(&repository.repository))
        }) {
            if repository.adapter == PullRequestAdapter::Local {
                local.push(local_repository_checks(&repository.repository, pull.number));
                continue;
            }
            if !expects_github(repository, &pull.status) {
                continue;
            }
            let Some(record) = &repository.github else {
                results.push(json!({"repository":repository.repository,"availability":"unavailable","message":"Provider publication is incomplete. Run g5d pr update and retry.","checks":[]}));
                continue;
            };
            let identity = attached_repository(
                &mut db,
                workspace,
                Some(&record.provider_repo_full_name),
                &service.provider_endpoints().map_err(G5Error::from)?,
                RepositoryUse::Read,
                Some(reader),
            )
            .await?;
            if identity.1 != repository.repo_id {
                return Err(invalid(
                    "Published PR repository identity changed; run g5d pr update and retry",
                )
                .into());
            }
            match repository.number {
                Some(number) => selected.push((identity, number)),
                None => results.push(json!({"repository":identity.3,"availability":"unavailable","message":"No GitHub PR is published for this member. Run g5d pr update and retry.","checks":[]})),
            }
        }
        // No provider record and nothing publication would publish to GitHub:
        // GitHub is not expected, so no external checks apply
        // (`g5.git.pr-no-github`), as merge's check policy already treats it.
        if selected.is_empty() && results.is_empty() && !any_github {
            return Ok(json!({
                "source": "g5",
                "repositories": local,
                "noExternalChecks": true,
                "message": NO_EXTERNAL_CHECKS,
            }));
        }
        if selected.is_empty() && results.is_empty() && local.is_empty() {
            return Err(invalid(
                "No matching published GitHub PR. Run g5d pr update or select a provider PR.",
            )
            .into());
        }
        if selected.is_empty() && results.is_empty() {
            // Only local repository pull requests were selected.
            return Ok(json!({
                "source": "g5",
                "repositories": local,
                "noExternalChecks": true,
                "message": NO_EXTERNAL_CHECKS,
            }));
        }
    }
    if query.head.is_some() && (selected.len() != 1 || !results.is_empty()) {
        return Err(invalid(
            "--head requires one published repository; select it with --repo OWNER/REPO",
        )
        .into());
    }
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
            .bind(workspace)
            .fetch_one(db.conn())
            .await?;
    let org = db.primary_org_id()?.into_uuid();
    let config = state
        .config_service
        .resolve_effective(&mut db, org, Some(workspace), owner)
        .await?;
    let mut secrets: Vec<String> = config
        .secrets
        .into_values()
        .filter(|s| !s.is_empty())
        .collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    db.rollback().await?;
    let mut rate_limited = false;
    let mut retry_at = Value::Null;
    for ((org, repo, url, name), number) in selected {
        if rate_limited {
            results.push(json!({"repository":name,"number":number,"availability":"unavailable","message":"Read stopped after a GitHub rate limit; retry after the indicated delay.","rateLimited":true,"retryAt":retry_at,"checks":[]}));
            continue;
        }
        let result = async {
            let auth = service.credentials.resolve(org, repo, actor, &url, RepositoryOperation::ReadPullRequestChecks).await
                .map_err(|_| unavailable("GitHub check access is unavailable. Grant the connected repository Pull requests, Contents, Checks and Commit statuses read access; verify its GitHub App installation or Account connection."))?;
            let token = super::token_from_auth(&auth).ok_or_else(|| unavailable("GitHub check credentials unavailable"))?;
            service.repository_protocol(&token,&name,g5_scm::providers::RepositoryCapability::ChecksRead)?
                .read_pull_request_checks(number,&g5_scm::providers::PullRequestChecksQuery {head:query.head.clone(),details:query.details,required:Some(service.required_checks.clone()),refresh_required:query.fresh_requirements},&secrets).await.map_err(ReadFailure::from)
        }.await;
        results.push(match result {
            Ok(value) => {
                rate_limited = value["rateLimited"] == true;
                retry_at = value["retryAt"].clone();
                value
            },
            Err(error) => {
                rate_limited = error.rate_limited;
                retry_at = json!(error.retry_at);
                json!({"repository":name,"number":number,"availability":"unavailable","message":error.message,"rateLimited":rate_limited,"retryAt":retry_at,"checks":[]})
            },
        });
    }
    // Each local repository pull request is reported beside GitHub's
    // instead of being left out (`g5.git.pr-no-github`).
    results.extend(local);
    Ok(json!({"source":"github","repositories":results}))
}

/// Aggregate merge-policy verdict over an external PR's check snapshot
/// (governance g5.governance.merge.external-policy). The organization policy
/// requires `Passing` before merge; both `Failing` and `Pending` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChecksVerdict {
    /// Every provider repo is `ready`, nothing failed and every check its base
    /// branch requires has passed (a repo requiring none passes unless one fails).
    Passing,
    /// Not failing, but a required check is still pending or not yet reported,
    /// or a repo's status is not readable (`availability` other than `ready`).
    Pending,
    /// Some check run or commit status, required or not, is failing (`fail`/`cancel`).
    Failing,
}

/// Reduce the `read_checks` snapshot (`{repositories:[{availability,checks:[{bucket,required}]}]}`)
/// to one verdict, worst-wins across the repositories a G5 PR spans. An empty
/// repository set means no external PR applies and is treated as `Passing`.
/// Checks the base branch does not require are advisory until they fail.
pub fn external_checks_verdict(snapshot: &Value) -> ExternalChecksVerdict {
    let Some(repos) = snapshot.get("repositories").and_then(Value::as_array) else {
        // A malformed snapshot is not a pass; wait rather than admit.
        return ExternalChecksVerdict::Pending;
    };
    let mut pending = false;
    for repo in repos {
        if let Some(checks) = repo.get("checks").and_then(Value::as_array) {
            for check in checks {
                match check.get("bucket").and_then(Value::as_str) {
                    Some("fail") | Some("cancel") => return ExternalChecksVerdict::Failing,
                    Some("pending") if check["required"] == true => pending = true,
                    _ => {}
                }
            }
        }
        // `pending`/`unavailable` head (not yet readable) is not a pass.
        if repo.get("availability").and_then(Value::as_str) != Some("ready") {
            pending = true;
        }
    }
    if pending {
        ExternalChecksVerdict::Pending
    } else {
        ExternalChecksVerdict::Passing
    }
}

fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Debug)]
struct ReadFailure {
    message: String,
    rate_limited: bool,
    /// When a rate-limited reader may retry (g5.git.check-waits).
    retry_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl From<G5Error> for ReadFailure {
    fn from(error: G5Error) -> Self {
        Self {
            message: error.to_string(),
            rate_limited: false,
            retry_at: None,
        }
    }
}
impl From<g5_scm::providers::ProviderError> for ReadFailure {
    fn from(error: g5_scm::providers::ProviderError) -> Self {
        Self {
            message: error.to_string(),
            rate_limited: error.rate_limited,
            retry_at: error.retry_at(chrono::Utc::now()),
        }
    }
}

#[cfg(test)]
mod verdict_tests {
    use super::{external_checks_verdict, ExternalChecksVerdict};
    use serde_json::json;

    /// Rows are `(bucket, required)`.
    fn repo(availability: &str, rows: &[(&str, bool)]) -> serde_json::Value {
        json!({
            "repository": "acme/api",
            "availability": availability,
            "checks": rows
                .iter()
                .map(|(b, required)| json!({"kind": "check_run", "name": "ci", "bucket": b, "required": required}))
                .collect::<Vec<_>>(),
        })
    }

    fn snapshot(repos: Vec<serde_json::Value>) -> serde_json::Value {
        json!({ "source": "github", "repositories": repos })
    }

    fn verdict(repos: Vec<serde_json::Value>) -> ExternalChecksVerdict {
        external_checks_verdict(&snapshot(repos))
    }

    #[test]
    fn no_provider_repos_is_passing() {
        assert_eq!(verdict(vec![]), ExternalChecksVerdict::Passing);
    }

    #[test]
    fn ready_with_no_checks_is_passing() {
        assert_eq!(
            verdict(vec![repo("ready", &[])]),
            ExternalChecksVerdict::Passing
        );
    }

    #[test]
    fn all_pass_is_passing() {
        assert_eq!(
            verdict(vec![repo("ready", &[("pass", true), ("skipping", false)])]),
            ExternalChecksVerdict::Passing
        );
    }

    #[test]
    fn any_fail_is_failing_required_or_not() {
        assert_eq!(
            verdict(vec![repo("ready", &[("pending", true), ("fail", false)])]),
            ExternalChecksVerdict::Failing
        );
        assert_eq!(
            verdict(vec![repo("ready", &[("cancel", true)])]),
            ExternalChecksVerdict::Failing
        );
    }

    #[test]
    fn only_required_pending_blocks() {
        assert_eq!(
            verdict(vec![repo("ready", &[("pass", true), ("pending", true)])]),
            ExternalChecksVerdict::Pending
        );
        assert_eq!(
            verdict(vec![repo("ready", &[("pass", true), ("pending", false)])]),
            ExternalChecksVerdict::Passing
        );
    }

    #[test]
    fn not_ready_availability_blocks_even_without_checks() {
        // A freshly opened PR whose head is not yet readable must not pass.
        assert_eq!(
            verdict(vec![repo("pending", &[])]),
            ExternalChecksVerdict::Pending
        );
        assert_eq!(
            verdict(vec![repo("unavailable", &[])]),
            ExternalChecksVerdict::Pending
        );
    }

    #[test]
    fn worst_wins_across_repos() {
        assert_eq!(
            verdict(vec![
                repo("ready", &[("pass", true)]),
                repo("ready", &[("fail", false)]),
            ]),
            ExternalChecksVerdict::Failing
        );
        assert_eq!(
            verdict(vec![repo("ready", &[("pass", true)]), repo("pending", &[])]),
            ExternalChecksVerdict::Pending
        );
    }

    #[test]
    fn malformed_snapshot_is_pending_not_pass() {
        assert_eq!(
            external_checks_verdict(&json!({"source": "github"})),
            ExternalChecksVerdict::Pending
        );
    }
}
