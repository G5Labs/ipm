//! Remote Git integration policy and observations for a workspace repository.
//!
//! PostgreSQL owns policy and operation receipts. Git owns local, fetched and
//! synchronization refs. Cheap ls-remote hints expire in a shared table and
//! must never authorize a push, import or conflict resolution.

use crate::services::git::repo_store::{RepoKey, ScopedRepoStorage, TenantRepoStorage};
use chrono::{DateTime, Utc};
use g5_contracts::{error::G5Error, types::WorkspaceRepoKey};
use sqlx::PgConnection;
use std::{collections::BTreeMap, time::Duration};

mod binding;
pub(crate) use binding::{prepare_binding_publication, PreparedMakeRemoteApplication};
pub(crate) mod convergence;
pub(crate) mod merge;

#[cfg(feature = "test-helpers")]
pub use merge::open_remote_tracking_merge_run as open_remote_tracking_merge_run_for_tests;
mod observations;
mod policy;
pub(crate) use policy::enqueue_background_inbound_rows;
mod repositories;
pub(crate) use repositories::provider_candidate;
mod resolution;
pub(crate) use repositories::{
    capture_repositories, eligible_pr_source, repository_summaries, CoordinatedWorkspaceRepoRow,
    EligiblePrSourceRow, WorkspaceRepositoryRow,
};
mod publication;
pub(crate) use policy::{bind_policy, enqueue_after_landing, has_bindings, remote_branches};
pub use policy::{bind_published, enqueue_after_push, retry, set_auto_pull, RetryOutcome};
pub use publication::PreparedRemotePublication;
mod worker;
pub use observations::{observe_tip, observed_tip, observed_tip_or_none, RemoteTipObservation};
pub(crate) use resolution::{enqueue_resolution, pin_adoption_inputs, ResolutionInputs};
use uuid::Uuid;
pub use worker::{run_branch_integration_worker, BranchIntegrationWorkerDeps};

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct RemoteIntegration {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub local_branch: String,
    pub remote_branch: String,
    pub auto_pull_enabled: bool,
    pub sync_paused_reason: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub git_url: String,
    pub provider: Option<String>,
    pub acting_user_id: Option<Uuid>,
    pub lifecycle_generation: i64,
    pub transfer_pending: bool,
    pub push_mode: String,
    pub is_base: bool,
}

impl RemoteIntegration {
    pub fn key(&self) -> WorkspaceRepoKey {
        WorkspaceRepoKey {
            org_id: self.org_id,
            workspace_id: self.workspace_id,
            repo_id: self.repo_id,
        }
    }
}

const POLICY_QUERY: &str = "
    SELECT i.org_id, i.workspace_id, i.repo_id,
           CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END AS local_branch,
           i.remote_branch, i.auto_pull_enabled, i.sync_paused_reason, i.last_sync_at,
           r.git_url, r.provider, w.owner_user_id AS acting_user_id, w.push_mode, w.is_base,
           w.lifecycle_generation,
           EXISTS (SELECT 1 FROM workspace_ownership_transfers t WHERE t.workspace_id=w.id AND t.status='pending') AS transfer_pending
    FROM remote_integrations i
    JOIN workspace_repos m ON (m.org_id, m.workspace_id, m.repo_id) = (i.org_id, i.workspace_id, i.repo_id)
    JOIN workspaces w ON w.id = i.workspace_id AND w.org_id = i.org_id
    JOIN repos r ON r.id = i.repo_id AND r.org_id = i.org_id
    WHERE i.org_id = ANY(authz.current_org_ids()) AND w.deleted_at IS NULL";

/// Load policy through the canonical membership and organization boundary.
/// A caller's current org scope never substitutes for workspace authorization.
pub async fn load(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    for_update: bool,
) -> Result<Option<RemoteIntegration>, G5Error> {
    let lock = if for_update {
        "FOR UPDATE OF w, m, r, i"
    } else {
        ""
    };
    let query = format!(
        "{POLICY_QUERY} AND i.org_id = $1 AND i.workspace_id = $2 AND i.repo_id = $3 {lock}"
    );
    Ok(sqlx::query_as(&query)
        .bind(key.org_id)
        .bind(key.workspace_id)
        .bind(key.repo_id)
        .fetch_optional(conn)
        .await?)
}

/// Maintenance callers already have an organization-scoped connection.
pub async fn passive_candidates(
    conn: &mut PgConnection,
    limit: i64,
) -> Result<Vec<RemoteIntegration>, G5Error> {
    let query = format!(
        "{POLICY_QUERY} AND i.auto_pull_enabled
                         AND i.sync_paused_reason IS NULL
                         ORDER BY i.last_sync_at NULLS FIRST, i.workspace_id, i.repo_id LIMIT $1"
    );
    Ok(sqlx::query_as(&query).bind(limit).fetch_all(conn).await?)
}

/// Exact synchronization evidence accompanying one publication. This is an
/// operation input, never a stored copy of the repository's current state.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SyncWatermarkUpdate {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub expected: Option<String>,
    pub new: Option<String>,
}

/// Policy fields whose change invalidates transport prepared by a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PublicationPolicy {
    local_branch: String,
    remote_branch: String,
    git_url: String,
    owner_user_id: Option<Uuid>,
    lifecycle_generation: i64,
    transfer_pending: bool,
    push_mode: String,
    auto_pull_enabled: bool,
    sync_paused_reason: Option<String>,
    is_base: bool,
}

impl RemoteIntegration {
    fn publication_policy(&self) -> PublicationPolicy {
        PublicationPolicy {
            local_branch: self.local_branch.clone(),
            remote_branch: self.remote_branch.clone(),
            git_url: self.git_url.clone(),
            owner_user_id: self.acting_user_id,
            lifecycle_generation: self.lifecycle_generation,
            transfer_pending: self.transfer_pending,
            push_mode: self.push_mode.clone(),
            auto_pull_enabled: self.auto_pull_enabled,
            sync_paused_reason: self.sync_paused_reason.clone(),
            is_base: self.is_base,
        }
    }
}

/// Exact admission proof for a worker publication after its preparation
/// transaction has closed. A delayed worker must still own both live leases.
#[derive(Debug, Clone)]
struct PublicationClaim {
    key: WorkspaceRepoKey,
    policy: PublicationPolicy,
    owner_token: Uuid,
    lease_generation: i64,
    requested_at: DateTime<Utc>,
    queue_advance_id: Option<Uuid>,
    trigger_kind: String,
    push_authority: Option<crate::models::branch_integration_queue::PushAuthority>,
    /// Whether this member's import, if any, rests on auto-pull rather than
    /// an explicit request, so the merged-PR hold applies at landing.
    automatic_import: bool,
    /// Other members whose due imports land in the same cut under the same
    /// owner token (`g5.governance.remote.pull`).
    co_members: Vec<CoMemberClaim>,
}

/// Lease proof for a member co-landed with a worker cycle's import.
#[derive(Debug, Clone)]
struct CoMemberClaim {
    key: WorkspaceRepoKey,
    policy: PublicationPolicy,
    lease_generation: i64,
    automatic_import: bool,
}

/// SQL over a pull request `pr` and the repository bound as `$2`: both of its
/// workspaces track the repository remotely, the two rows
/// `repositories::provider_candidate` joins (`remote_branch` is never null).
pub(crate) const PULL_REQUEST_TRACKS_REPOSITORY_SQL: &str = "\
    (EXISTS (SELECT 1 FROM remote_integrations ri WHERE ri.org_id = pr.org_id \
             AND ri.repo_id = $2 AND ri.workspace_id = pr.source_workspace_id) \
     AND EXISTS (SELECT 1 FROM remote_integrations ri WHERE ri.org_id = pr.org_id \
             AND ri.repo_id = $2 AND ri.workspace_id = pr.target_workspace_id))";

/// The workspace PR whose partial merge holds this member's automatic import
/// (`g5.governance.remote.pull`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ImportHold {
    pub pull_request_id: Uuid,
    pub title: String,
}

/// The workspace PR, if any, that holds background auto-pull for this member:
/// the workspace is the target of an open workspace PR whose provider PR for
/// this repository is merged, or being merged, while a sibling provider PR is
/// not. Importing the member alone would half-import that PR, so automatic
/// cycles leave it to the merged-PR import; an explicit pull still imports it
/// and names the PR. Closing the workspace PR lifts the hold. An in-flight
/// provider merge holds the PR row for update, which `SKIP LOCKED` detects
/// without waiting.
pub async fn import_hold(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<Option<ImportHold>, sqlx::Error> {
    // The GitHub adapter's records decide it (`g5.git.pr-adapter`).
    super::pull_request_adapters::github_import_hold(conn, workspace_id, repo_id).await
}

/// Why a remote-tracked source member's captured head may not be what a fork's
/// requester expects (`g5.fork.source.advisory`). Advisory only: forking is
/// never refused for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkSourceAdvisory {
    pub repo_id: Uuid,
    pub repo_name: String,
    /// `import_held`, `paused` or `behind`.
    pub kind: &'static str,
    /// For `import_held`: the half-merged workspace PR holding auto-pull.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<ImportHold>,
    /// For `paused`: the pause reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<String>,
    /// For `behind`: the provider tip last observed for the member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_tip: Option<String>,
}

/// Each remote-tracked member of `workspace_id` that is held by the merged-PR
/// hold, paused, or behind its latest provider observation. A member is behind
/// when the observed tip differs from its head and its head is not that tip's
/// descendant (an unknown tip counts as behind).
pub async fn fork_source_advisories(
    conn: &mut PgConnection,
    storage: &ScopedRepoStorage,
    workspace_id: Uuid,
) -> Result<Vec<ForkSourceAdvisory>, G5Error> {
    let members = crate::services::workspace_repositories::load_workspace_repositories(
        conn,
        workspace_id,
        false,
    )
    .await?;
    let versions =
        crate::services::workspace_repositories::read_repository_versions(storage, members).await?;
    let mut advisories = Vec::new();
    for version in versions {
        let member = &version.member;
        let key = WorkspaceRepoKey {
            org_id: member.org_id,
            workspace_id,
            repo_id: member.repo_id,
        };
        let Some(policy) = load(conn, key, false).await? else {
            continue;
        };
        let advisory = |kind| ForkSourceAdvisory {
            repo_id: member.repo_id,
            repo_name: member.repo_name.clone(),
            kind,
            pull_request: None,
            pause_reason: None,
            provider_tip: None,
        };
        if let Some(hold) = import_hold(conn, workspace_id, member.repo_id).await? {
            advisories.push(ForkSourceAdvisory {
                pull_request: Some(hold),
                ..advisory("import_held")
            });
        }
        if let Some(reason) = policy.sync_paused_reason.clone() {
            advisories.push(ForkSourceAdvisory {
                pause_reason: Some(reason),
                ..advisory("paused")
            });
            continue;
        }
        // A hint, never authority: a failed read leaves the member without a
        // "behind" advisory and keeps the held and paused advisories above.
        let Some(observation) = observed_tip_or_none(conn, &policy).await else {
            continue;
        };
        if observation.oid == version.head_commit {
            continue;
        }
        let ahead = storage
            .repo_path(member.org_id, member.repo_id)
            .ok()
            .and_then(|path| g5_scm::GitRepo::open(&path).ok())
            .and_then(|repo| {
                repo.is_ancestor_of(&observation.oid, &version.head_commit)
                    .ok()
            })
            .unwrap_or(false);
        if !ahead {
            advisories.push(ForkSourceAdvisory {
                provider_tip: Some(observation.oid),
                ..advisory("behind")
            });
        }
    }
    Ok(advisories)
}

async fn automatic_import_held(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(import_hold(conn, workspace_id, repo_id).await?.is_some())
}

async fn validate_publication_claim(
    conn: &mut PgConnection,
    claim: &PublicationClaim,
) -> Result<(), G5Error> {
    let owned: Option<Uuid> = sqlx::query_scalar(
        "SELECT q.repo_id FROM remote_integration_queue q
         JOIN remote_ref_leases lease ON lease.repo_id = q.repo_id AND lease.org_id = q.org_id
         WHERE q.org_id = $1 AND q.workspace_id = $2 AND q.repo_id = $3
           AND q.locked_by = $4 AND q.locked_until > now()
           AND q.requested_at = $5 AND q.advance_id IS NOT DISTINCT FROM $6 AND q.trigger_kind = $7
           AND lease.owner_token = $4 AND lease.lease_generation = $8
           AND lease.remote_branch = $9 AND lease.lease_expires_at > now()
           AND q.requested_by_user_id IS NOT DISTINCT FROM $10
           AND q.lifecycle_generation IS NOT DISTINCT FROM $11
         FOR UPDATE OF q, lease",
    )
    .bind(claim.key.org_id)
    .bind(claim.key.workspace_id)
    .bind(claim.key.repo_id)
    .bind(claim.owner_token)
    .bind(claim.requested_at)
    .bind(claim.queue_advance_id)
    .bind(&claim.trigger_kind)
    .bind(claim.lease_generation)
    .bind(&claim.policy.remote_branch)
    .bind(claim.push_authority.map(|a| a.requested_by_user_id))
    .bind(claim.push_authority.map(|a| a.lifecycle_generation))
    .fetch_optional(&mut *conn)
    .await?;
    let current = load(conn, claim.key, true).await?;
    if owned.is_none() || current.is_none_or(|current| current.publication_policy() != claim.policy)
    {
        return Err(G5Error::Conflict {
            reason: "Remote integration claim expired or changed before publication".into(),
        });
    }
    for co in &claim.co_members {
        let leased: Option<Uuid> = sqlx::query_scalar(
            "SELECT repo_id FROM remote_ref_leases
             WHERE org_id = $1 AND repo_id = $2 AND remote_branch = $3
               AND owner_token = $4 AND lease_generation = $5 AND lease_expires_at > now()
             FOR UPDATE",
        )
        .bind(co.key.org_id)
        .bind(co.key.repo_id)
        .bind(&co.policy.remote_branch)
        .bind(claim.owner_token)
        .bind(co.lease_generation)
        .fetch_optional(&mut *conn)
        .await?;
        let current = load(conn, co.key, true).await?;
        if leased.is_none()
            || current.is_none_or(|current| current.publication_policy() != co.policy)
        {
            return Err(G5Error::Conflict {
                reason: "A co-landed member's claim expired or changed before publication".into(),
            });
        }
    }
    let automatic = std::iter::once((claim.key, claim.automatic_import)).chain(
        claim
            .co_members
            .iter()
            .map(|co| (co.key, co.automatic_import)),
    );
    for (key, automatic_import) in automatic {
        if automatic_import && automatic_import_held(conn, key.workspace_id, key.repo_id).await? {
            return Err(G5Error::Conflict {
                reason: "A multi-repository pull request is partly merged on the provider; its import waits for the merged pull request".into(),
            });
        }
    }
    Ok(())
}

/// Exact input shared by member pulls and remote binding. Database
/// state supplies identity and policy; all version observations belong to Git.
#[derive(Debug, Clone)]
pub struct PullInput {
    /// The repository, keyed for its organization by the caller's scope.
    pub key: RepoKey,
    pub git_url: String,
    pub remote_branch: String,
    pub local_head: String,
    pub watermark: Option<String>,
    pub observed_remote_tip: Option<String>,
}

pub struct PreparedPull {
    pub remote_tip: String,
    pub relation: crate::services::git::branch_integration_service::TipRelation,
}

/// Why pull preparation failed. `credential_refused` is the native runner's
/// typed refusal of the selected credential, so callers can ask the person to
/// reconnect without matching Git's diagnostic wording.
#[derive(Debug)]
pub struct PullFailure {
    pub error: G5Error,
    pub credential_refused: bool,
}

impl From<G5Error> for PullFailure {
    fn from(error: G5Error) -> Self {
        Self {
            error,
            credential_refused: false,
        }
    }
}

impl From<g5_scm::native::NativeGitError> for PullFailure {
    fn from(error: g5_scm::native::NativeGitError) -> Self {
        Self {
            credential_refused: error.kind == g5_scm::native::FailureKind::Unauthorized,
            error: error.into(),
        }
    }
}

/// Fetch only missing objects, preserving every public ref during preparation.
/// A racing remote must still supply the exact advertised commit or preparation
/// fails. Both Base and working members use this path and the same classifier.
pub async fn prepare_pull(
    storage: &TenantRepoStorage,
    input: PullInput,
    auth: g5_scm::GitAuth,
) -> Result<PreparedPull, G5Error> {
    prepare_pull_classified(storage, input, auth)
        .await
        .map_err(|failure| failure.error)
}

/// [`prepare_pull`] with the typed credential-refusal outcome.
pub async fn prepare_pull_classified(
    storage: &TenantRepoStorage,
    input: PullInput,
    auth: g5_scm::GitAuth,
) -> Result<PreparedPull, PullFailure> {
    let path = storage.path(&input.key)?;
    let deadline = Duration::from_secs(120);
    let prepare = async move {
        let git = crate::git_runtime::network(deadline)?;
        let remote_tip = match input.observed_remote_tip {
            Some(tip) => tip,
            None => git
                .remote_branch_tip(&input.git_url, &input.remote_branch, &auth)
                .await?
                .ok_or_else(|| G5Error::NotFound {
                    resource: format!("remote branch '{}'", input.remote_branch),
                })?,
        };
        let has_tip = {
            let (path, tip) = (path.clone(), remote_tip.clone());
            tokio::task::spawn_blocking(move || g5_scm::GitRepo::open(&path)?.has_commit(&tip))
                .await
                .map_err(|error| G5Error::Internal {
                    message: format!("Remote pull preparation failed: {error}"),
                })??
        };
        if !has_tip {
            git.fetch_branch_objects(&path, &input.git_url, &input.remote_branch, &auth)
                .await?;
        }
        Ok(tokio::task::spawn_blocking(move || {
            let repo = g5_scm::GitRepo::open(&path)?;
            if !repo.has_commit(&remote_tip)? {
                return Err(G5Error::Conflict {
                    reason: "Remote moved during pull; retry with its current tip".into(),
                });
            }
            let relation = classify_history(
                &repo,
                &input.local_head,
                &remote_tip,
                input.watermark.as_deref(),
            )?;
            Ok(PreparedPull {
                remote_tip,
                relation,
            })
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Remote pull preparation failed: {error}"),
        })??)
    };
    tokio::time::timeout(deadline, prepare)
        .await
        .map_err(|_| G5Error::ServiceUnavailable {
            message: "Remote pull timed out".into(),
        })?
}

/// Shared history policy (`g5.governance.remote.workspace-rewrite`). A
/// watermark is synchronization evidence, not a fork point: it tells which
/// side rewrote. Equal tips are converged whatever the watermark. A provider
/// that lost the watermark rewrote; a G5 branch that lost it is a workspace
/// rewrite, publishable only while the provider still equals the watermark.
/// A local rewind is classified before tip ancestry, so the unchanged provider
/// is never imported back over it. An absent watermark cannot override the
/// actual relationship of tips.
pub fn classify_history(
    repo: &g5_scm::GitRepo,
    local: &str,
    remote: &str,
    watermark: Option<&str>,
) -> Result<crate::services::git::branch_integration_service::TipRelation, G5Error> {
    use crate::services::git::branch_integration_service::TipRelation;
    if local == remote {
        return Ok(TipRelation::Equal);
    }
    if let Some(watermark) = watermark.filter(|sha| sha.bytes().any(|byte| byte != b'0')) {
        let (_, remote_behind) = repo.graph_ahead_behind(remote, watermark)?;
        if remote_behind > 0 {
            return Ok(TipRelation::WatermarkViolation);
        }
        let (_, local_behind) = repo.graph_ahead_behind(local, watermark)?;
        if local_behind > 0 {
            return Ok(if remote == watermark {
                TipRelation::WorkspaceRewritten
            } else {
                TipRelation::Diverged
            });
        }
    }
    let (ahead, behind) = repo.graph_ahead_behind(local, remote)?;
    Ok(match (ahead, behind) {
        (0, 0) => TipRelation::Equal,
        (0, _) => TipRelation::MirrorAhead,
        (_, 0) => TipRelation::WorkspaceAhead,
        _ => TipRelation::Diverged,
    })
}

/// Prepare Base's adoption of its provider history for one repository
/// (`g5.workspace.remote.base-provider-rewrite`). The cut resets the branch to
/// the reviewed provider tip; the convergence consumer moves the provider
/// observation and watermark with it and clears the pause, and the replaced
/// head is pinned so the discarded commits stay recoverable. Nothing is pushed.
pub(crate) async fn prepare_provider_history_acceptance(
    scope: &g5_authz::ScopedDbReopener,
    storage: &TenantRepoStorage,
    target: g5_contracts::types::AuthorityTarget,
    captured_heads: &BTreeMap<Uuid, String>,
    row: &CoordinatedWorkspaceRepoRow,
    provider_tip: &str,
    actor_user_id: Uuid,
) -> Result<PreparedRemotePublication, sqlx::Error> {
    let remote_branch = row.remote_branch.clone().ok_or_else(|| {
        sqlx::Error::Protocol("Accepting provider history requires a remote binding".into())
    })?;
    let convergence = vec![convergence::PreparedRemoteTrackingConvergence {
        workspace_id: row.workspace_id,
        repo_id: row.repo_id,
        remote_branch,
        expected_tracking_mode: row.tracking_mode.clone(),
        expected_remote_branch: row.remote_branch.clone(),
        expected_old_head: row.head_commit.clone(),
        proposed_new_head: provider_tip.to_string(),
        expected_baseline_commit: row.baseline_commit.clone(),
        expected_git_url: row.git_url.clone(),
        expected_sync_paused_reason: row.sync_paused_reason.clone(),
        acting_user_id: actor_user_id,
        trigger_kind: crate::models::branch_sync_run::trigger_kind::MANUAL.to_string(),
        status: convergence::RemoteTrackingConvergenceStatus::Imported,
    }];
    let proposed_heads = BTreeMap::from([(row.repo_id, provider_tip.to_string())]);
    let advance = prepare_pull_cut(
        scope,
        storage,
        target,
        captured_heads,
        &proposed_heads,
        &convergence,
    )
    .await?
    .into_unadmitted_advance();
    let path = storage
        .repo_path(scope, row.org_id, row.repo_id)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let pin = g5_scm::RefCommit {
        repo_path: path,
        ref_name: format!("refs/g5/base-accepted/{}", advance.advance_id()),
        expected_old: None,
        new_oid: Some(row.head_commit.clone()),
    };
    Ok(
        PreparedRemotePublication::new(advance.with_additional_refs(vec![pin]))
            .for_base_acceptance(publication::BaseAcceptanceAdmission {
                org_id: row.org_id,
                workspace_id: row.workspace_id,
                repo_id: row.repo_id,
                actor_id: actor_user_id,
            }),
    )
}

async fn prepare_pull_cut(
    scope: &g5_authz::ScopedDbReopener,
    storage: &TenantRepoStorage,
    target: g5_contracts::types::AuthorityTarget,
    captured_heads: &BTreeMap<Uuid, String>,
    proposed_heads: &BTreeMap<Uuid, String>,
    convergence: &[crate::services::git::advance_consumer::PreparedRemoteTrackingConvergence],
) -> Result<PreparedRemotePublication, sqlx::Error> {
    if convergence.is_empty() {
        return Err(sqlx::Error::Protocol(
            "Remote publication requires synchronization effects".into(),
        ));
    }
    if proposed_heads
        .keys()
        .any(|id| !captured_heads.contains_key(id))
    {
        return Err(sqlx::Error::Protocol(
            "Pull includes a repository outside its captured workspace".into(),
        ));
    }
    let repos = captured_heads
        .iter()
        .map(
            |(repo_id, head)| g5_contracts::types::WorkspaceHeadRepoInput {
                repo_id: *repo_id,
                expected_old_head: head.clone(),
                proposed_new_head: proposed_heads.get(repo_id).unwrap_or(head).clone(),
            },
        )
        .collect();
    let advance = crate::services::git::branch_commit::prepare_branch_commit(
        scope,
        storage,
        target,
        repos,
        "remote_pull",
    )
    .await?;
    let advance = stage_merge_convergence(scope, storage, advance, convergence).await?;
    Ok(PreparedRemotePublication::new(advance))
}

/// Attach remote synchronization to a merge publication. The merge service
/// owns admission and the workspace fences; this module owns watermark and
/// provider observation effects, including their landing-time policy checks.
pub(crate) async fn stage_merge_convergence(
    scope: &g5_authz::ScopedDbReopener,
    storage: &TenantRepoStorage,
    advance: crate::services::git::branch_commit::PreparedBranchCommit,
    convergence: &[convergence::PreparedRemoteTrackingConvergence],
) -> Result<crate::services::git::branch_commit::PreparedBranchCommit, sqlx::Error> {
    // Deliver the same accepted provider tip as the branch/watermark cut.
    // This is shared by merges and worker fast-forwards; no VM files or
    // flush receipts participate in the publication.
    let provider_refs = convergence
        .iter()
        .map(|input| {
            let path = storage
                .repo_path(scope, advance.org_id(), input.repo_id)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            g5_scm::GitRepo::provider_observation_update(
                path,
                input.workspace_id,
                &input.remote_branch,
                &input.proposed_new_head,
            )
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let advance = advance.with_additional_refs(provider_refs);
    let mut db = scope
        .open()
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    crate::services::git::advance_consumer::register_remote_tracking_convergence_consumer(
        db.conn(),
        &advance,
        convergence,
    )
    .await?;
    db.commit()
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    Ok(advance)
}

/// Convert operation evidence into opaque generic ref effects. Only this
/// boundary knows how a workspace synchronization mark maps to a Git ref.
fn watermark_effects(
    storage: &ScopedRepoStorage,
    org_id: Uuid,
    members: &[crate::services::git::branch_commit::BranchCommitMember],
    watermarks: &[SyncWatermarkUpdate],
) -> Result<Vec<g5_scm::RefCommit>, G5Error> {
    watermarks
        .iter()
        .map(|watermark| {
            let member = members
                .iter()
                .find(|member| {
                    member.workspace_id == watermark.workspace_id
                        && member.repo_id == watermark.repo_id
                })
                .ok_or_else(|| G5Error::Conflict {
                    reason: "Synchronization watermark is outside the landing cut".into(),
                })?;
            let path = storage.repo_path(org_id, member.repo_id)?;
            Ok(g5_scm::GitRepo::sync_watermark_update(
                path,
                &member.branch_name,
                watermark.expected.clone(),
                watermark.new.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod pull_tests {
    use super::*;
    use crate::services::git::branch_integration_service::TipRelation;
    use g5_scm::{GitAuth, GitRepo};

    #[test]
    fn pull_failure_keeps_the_runners_credential_refusal() {
        let refusal = |kind| g5_scm::native::NativeGitError {
            operation: "list remote branches",
            target: None,
            kind,
            effect: g5_scm::FailureEffect::Refused,
            detail: "fatal: could not read Username".into(),
        };
        assert!(
            PullFailure::from(refusal(g5_scm::native::FailureKind::Unauthorized))
                .credential_refused
        );
        assert!(!PullFailure::from(refusal(g5_scm::native::FailureKind::Exit)).credential_refused);
        assert!(
            !PullFailure::from(G5Error::Internal {
                message: "x".into()
            })
            .credential_refused
        );
    }

    fn commit(repo: &git2::Repository, parent: Option<&str>, message: &str) -> String {
        let signature = git2::Signature::now("Pull Test", "pull@example.test").unwrap();
        let oid = repo.treebuilder(None).unwrap().write().unwrap();
        let tree = repo.find_tree(oid).unwrap();
        let parent = parent.map(|sha| repo.find_commit(git2::Oid::from_str(sha).unwrap()).unwrap());
        let parents = parent.iter().collect::<Vec<_>>();
        repo.commit(None, &signature, &signature, message, &tree, &parents)
            .unwrap()
            .to_string()
    }

    #[test]
    fn shared_history_policy_handles_equal_ff_local_changes_and_remote_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let raw = git2::Repository::init_bare(dir.path()).unwrap();
        let a = commit(&raw, None, "base");
        let b = commit(&raw, Some(&a), "remote advance");
        let c = commit(&raw, Some(&a), "local change");
        let repo = GitRepo::open(dir.path()).unwrap();
        assert_eq!(
            classify_history(&repo, &a, &a, None).unwrap(),
            TipRelation::Equal
        );
        assert_eq!(
            classify_history(&repo, &a, &b, None).unwrap(),
            TipRelation::MirrorAhead
        );
        assert_eq!(
            classify_history(&repo, &b, &a, None).unwrap(),
            TipRelation::WorkspaceAhead
        );
        assert_eq!(
            classify_history(&repo, &c, &b, Some(&a)).unwrap(),
            TipRelation::Diverged
        );
        // The provider lost the watermark: a provider rewrite.
        assert_eq!(
            classify_history(&repo, &b, &a, Some(&b)).unwrap(),
            TipRelation::WatermarkViolation
        );
        assert_eq!(
            classify_history(&repo, &c, &a, Some(&b)).unwrap(),
            TipRelation::WatermarkViolation
        );
        // G5 rewrote against the unchanged provider: a workspace rewrite,
        // including a rewind the provider still contains.
        assert_eq!(
            classify_history(&repo, &c, &b, Some(&b)).unwrap(),
            TipRelation::WorkspaceRewritten
        );
        assert_eq!(
            classify_history(&repo, &a, &b, Some(&b)).unwrap(),
            TipRelation::WorkspaceRewritten
        );
        // Equal tips settle whatever either side did since the watermark.
        assert_eq!(
            classify_history(&repo, &c, &c, Some(&b)).unwrap(),
            TipRelation::Equal
        );
        // A workspace rewrite against a provider that advanced normally
        // must be reconciled, never published over the advance.
        let d = commit(&raw, Some(&b), "provider advance after sync");
        assert_eq!(
            classify_history(&repo, &c, &d, Some(&b)).unwrap(),
            TipRelation::Diverged
        );
        assert_eq!(
            classify_history(&repo, &a, &d, Some(&b)).unwrap(),
            TipRelation::Diverged
        );
        // Ordinary history keeps its ancestry classification.
        assert_eq!(
            classify_history(&repo, &b, &d, Some(&b)).unwrap(),
            TipRelation::MirrorAhead
        );
        let e = commit(&raw, Some(&b), "workspace advance after sync");
        assert_eq!(
            classify_history(&repo, &e, &b, Some(&b)).unwrap(),
            TipRelation::WorkspaceAhead
        );
    }

    #[tokio::test]
    async fn pull_fetches_exact_objects_without_mutating_base_working_or_watermark_refs() {
        let root = tempfile::tempdir().unwrap();
        let upstream = tempfile::tempdir().unwrap();
        let storage = TenantRepoStorage::local_for_test(root.path().into());
        let remote = git2::Repository::init_bare(upstream.path()).unwrap();
        let a = commit(&remote, None, "base");
        remote
            .reference(
                "refs/heads/trunk",
                git2::Oid::from_str(&a).unwrap(),
                true,
                "initial",
            )
            .unwrap();
        let key = TenantRepoStorage::test_key(Uuid::new_v4(), Uuid::new_v4());
        let local_path = storage.path(&key).unwrap();
        let local = git2::build::RepoBuilder::new()
            .bare(true)
            .clone(upstream.path().to_str().unwrap(), &local_path)
            .unwrap();
        for name in [
            "refs/heads/trunk",
            "refs/heads/work-item",
            "refs/g5-tracking/trunk/baseline",
        ] {
            local
                .reference(name, git2::Oid::from_str(&a).unwrap(), true, "initial")
                .unwrap();
        }
        let b = commit(&remote, Some(&a), "provider advance");
        remote
            .reference(
                "refs/heads/trunk",
                git2::Oid::from_str(&b).unwrap(),
                true,
                "advance",
            )
            .unwrap();
        let before = local
            .references()
            .unwrap()
            .map(|r| {
                let r = r.unwrap();
                (r.name().unwrap().to_string(), r.target())
            })
            .collect::<BTreeMap<_, _>>();
        let fetch_head_before = std::fs::read(local_path.join("FETCH_HEAD")).ok();
        let input = PullInput {
            key,
            git_url: upstream.path().to_str().unwrap().into(),
            remote_branch: "trunk".into(),
            local_head: a.clone(),
            watermark: Some(a.clone()),
            observed_remote_tip: None,
        };
        let prepared = prepare_pull(&storage, input.clone(), GitAuth::None)
            .await
            .unwrap();
        assert_eq!(prepared.remote_tip, b);
        assert_eq!(prepared.relation, TipRelation::MirrorAhead);
        assert!(GitRepo::open(&local_path).unwrap().has_commit(&b).unwrap());
        let after = local
            .references()
            .unwrap()
            .map(|r| {
                let r = r.unwrap();
                (r.name().unwrap().to_string(), r.target())
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            before, after,
            "preparation must leave all public and synchronization refs unchanged"
        );
        assert_eq!(
            std::fs::read(local_path.join("FETCH_HEAD")).ok(),
            fetch_head_before
        );
        assert_eq!(
            local.find_remote("origin").unwrap().url().unwrap(),
            input.git_url.as_str()
        );

        // An advertised version absent from the fetched history must not be replaced with the current tip.
        let raced = PullInput {
            observed_remote_tip: Some("f".repeat(40)),
            ..input
        };
        assert!(matches!(
            prepare_pull(&storage, raced, GitAuth::None).await,
            Err(G5Error::Conflict { .. })
        ));
    }
}
