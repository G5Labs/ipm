//! The one UI/CLI create-or-update PR operation. Entry adapters supply typed
//! caller authority and input; all preparation and publication ordering lives here.
use std::sync::Arc;

use g5_auth::WorkspaceIncarnationClaims;
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{
    ensure_user_workspace_access, scoped_db_for_user_actor_in_org,
    scoped_db_for_user_actor_in_org_with_lock_timeout, scoped_db_for_workspace,
    user_can_access_workspace,
};
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::services::workspace_pull_request_provider_lifecycle::{
    provider_pull_request_completed, AutomaticRefreshAdmission,
};
use crate::services::{coordinated_repo_publication, AppState};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePullRequestRequest {
    pub target_workspace_id: Option<Uuid>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// `create` (the default) reuses an open G5 PR or opens one; `update`
    /// requires an open one, never creates a G5 PR and never replaces a
    /// provider PR merged or closed outside G5.
    #[serde(default)]
    pub mode: PullRequestWriteMode,
    /// `create` only: open a replacement for a provider PR merged or closed
    /// outside G5.
    #[serde(default)]
    pub replace: bool,
    /// Save the source workspace (commit and push its pending work) first, as
    /// the UI's Create PR and Update PR do (`g5.git.pr-save`). `g5d pr`
    /// sends `false` and publishes committed work only
    /// (`g5.git.pr-cli.create-save`).
    #[serde(default = "save_by_default")]
    pub save: bool,
}

fn save_by_default() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PullRequestWriteMode {
    #[default]
    Create,
    Update,
    /// `g5d pr edit` (`g5.git.pr-cli.edit`): the open PR's title and saved
    /// description only, applied to its provider PRs; nothing is saved or
    /// pushed.
    Edit,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePullRequestOutcome {
    pub pull_request: coordinated_repo_publication::WorkspacePullRequestSummary,
    pub already_existed: bool,
    pub dirty_drafts: Vec<coordinated_repo_publication::WorkspaceDirtyDraftSummary>,
    /// What this request did to each provider repository.
    pub publication: Publication,
}

/// Provider outcomes of one create/update, computed from this request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    /// Every provider PR this request synchronized is open on the provider
    /// with the saved head, and explicit text was applied.
    pub complete: bool,
    pub repositories: Vec<RepositoryPublication>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryPublication {
    pub repository: String,
    pub number: Option<i64>,
    pub url: Option<String>,
    /// `created`, `reused`, `replaced`, `action_required`, `failed`,
    /// `merged`, or `unchanged` when this request did not synchronize it.
    pub action: String,
    /// `published` when the provider PR's head is the saved head, else
    /// `pending`; `None` without an open provider PR.
    pub head: Option<&'static str>,
    /// For explicit text: `applied` or `not_applied`.
    pub title: Option<&'static str>,
    pub body: Option<&'static str>,
    pub message: Option<String>,
}

/// Classify each provider row against its PR number before this request.
/// Rows this request wrote carry the transaction's `now()` as `last_synced_at`.
fn publication(
    repos: &[coordinated_repo_publication::WorkspacePullRequestProviderRepoSummary],
    before: &[(Uuid, Option<i64>)],
    synced_at: chrono::DateTime<chrono::Utc>,
    title: bool,
    body: bool,
) -> Publication {
    let repositories: Vec<_> = repos
        .iter()
        .map(|repo| {
            let touched = repo.last_synced_at == Some(synced_at);
            let prior = before
                .iter()
                .find(|(id, _)| *id == repo.repo_id)
                .and_then(|(_, number)| *number);
            let open = repo.provider_status == "open";
            let action = match (touched, repo.provider_status.as_str()) {
                (false, _) => "unchanged".to_owned(),
                (true, "open") => match (prior, repo.provider_pr_number) {
                    (None, _) => "created".to_owned(),
                    (Some(a), Some(b)) if a != b => "replaced".to_owned(),
                    _ => "reused".to_owned(),
                },
                (true, status) => status.to_owned(),
            };
            let text = |requested: bool| {
                (requested && touched).then_some(if open { "applied" } else { "not_applied" })
            };
            RepositoryPublication {
                repository: repo.provider_repo_full_name.clone(),
                number: repo.provider_pr_number,
                url: repo.provider_pr_url.clone(),
                head: open.then_some(
                    if repo.provider_head_sha.is_some()
                        && repo.provider_head_sha == repo.source_head_sha
                    {
                        "published"
                    } else {
                        "pending"
                    },
                ),
                title: text(title),
                body: text(body),
                message: repo.last_error.clone().filter(|_| touched && !open),
                action,
            }
        })
        .collect();
    let complete = repositories
        .iter()
        .filter(|repo| repo.action != "unchanged")
        .all(|repo| repo.head == Some("published"));
    Publication {
        complete,
        repositories,
    }
}

/// A failure while saving or pushing the source, before any PR was recorded.
/// The error keeps its own status, code and details; `details.kind` tells
/// callers such as `g5d pr create` that nothing was published.
fn source_save_failed(error: G5Error) -> AppError {
    AppError::from(error).with_kind(crate::errors::FailureKind::PullRequestSourceSaveFailed)
}

fn no_open_pull_request(reason: String) -> AppError {
    AppError::from(G5Error::ConflictDetails {
        reason,
        details: serde_json::json!({"kind": "no_open_pull_request"}),
    })
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PullRequestTarget {
    pub id: Uuid,
    pub name: String,
}

/// The UI and omitted-target writes use this same ordered, authorized list.
pub async fn targets(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    source: Uuid,
    actor: Uuid,
) -> Result<Vec<PullRequestTarget>, AppError> {
    let org = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    ensure_user_workspace_access(&state.system_pool, db, actor, org, source).await?;
    let candidates = sqlx::query_as::<_, PullRequestTarget>(
        "SELECT t.id, t.name FROM workspaces s JOIN workspaces t ON t.org_id=s.org_id
         WHERE s.id=$1 AND s.org_id=$2 AND s.deleted_at IS NULL
           AND t.id<>s.id AND t.deleted_at IS NULL
           AND (t.id=s.cloned_from_id OR t.is_base)
           AND s.personal_user_id IS NULL AND t.personal_user_id IS NULL
         ORDER BY (t.id IS NOT DISTINCT FROM s.cloned_from_id) DESC, t.name, t.id",
    )
    .bind(source)
    .bind(org)
    .fetch_all(db.conn())
    .await?;
    let mut readable = Vec::new();
    for target in candidates {
        if user_can_access_workspace(&state.system_pool, db, actor, org, target.id).await? {
            readable.push(target);
        }
    }
    Ok(readable)
}

/// `g5d pr create -B NAME` from the VM: the targets the UI offers the owner.
pub async fn targets_for_incarnation(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
) -> Result<Vec<PullRequestTarget>, AppError> {
    let mut db = PullRequestCaller::Workspace(claims)
        .source_db(state, claims.workspace_id)
        .await?;
    let found = targets(state, &mut db, claims.workspace_id, claims.owner_user_id).await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(found)
}

/// `g5d pr create --dry-run` from the VM: what create would publish into
/// `target`, admitted as create is. Writes nothing.
pub async fn preview_for_incarnation(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    target: Uuid,
) -> Result<coordinated_repo_publication::WorkspacePullRequestTextPreview, AppError> {
    let db = PullRequestCaller::Workspace(claims)
        .authorized_db(state, claims.workspace_id, target)
        .await?;
    state
        .coordinated_repo_publication_service
        .preview_workspace_pull_request_text(
            db,
            &state.workspace_merge_service,
            state.repo_storage.clone(),
            claims.workspace_id,
            target,
        )
        .await
        .map_err(AppError::from)
}

/// VM delegation is confined to this operation. Never mint a human JWT or
/// synthesize an AuthContext::User from the workspace token.
pub enum PullRequestCaller<'a> {
    User(&'a AuthContext),
    Workspace(&'a WorkspaceIncarnationClaims),
    /// The agent of the token's workspace, publishing one of its direct
    /// children into it (`g5.story.agent-merge-child`). It acts for the
    /// owner as the owner's browser would: the token's own incarnation is
    /// checked, the child's VM is not pinned.
    WorkspaceChild(&'a WorkspaceIncarnationClaims),
    /// Server continuation re-admits the current owner, never a VM or old pusher.
    Continuation {
        org_id: Uuid,
        actor_id: Uuid,
    },
}

impl PullRequestCaller<'_> {
    fn actor_id(&self) -> Result<Uuid, AppError> {
        match self {
            Self::User(auth) => auth
                .user_id()
                .and_then(|id| id.parse().ok())
                .ok_or_else(|| AppError::from(G5Error::Unauthorized)),
            Self::Workspace(claims) | Self::WorkspaceChild(claims) => Ok(claims.owner_user_id),
            Self::Continuation { actor_id, .. } => Ok(*actor_id),
        }
    }

    fn expected_incarnation(&self) -> Option<(Uuid, u64)> {
        match self {
            Self::Workspace(claims) => Some((claims.vm_id, claims.epoch)),
            Self::User(_) | Self::WorkspaceChild(_) | Self::Continuation { .. } => None,
        }
    }

    async fn authorized_db(
        &self,
        state: &Arc<AppState>,
        source: Uuid,
        target: Uuid,
    ) -> Result<ScopedDb, AppError> {
        let mut db = self
            .source_db_for_target(state, source, Some(target))
            .await?;
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        crate::services::workspace_mutation_policy::ensure_not_review_workspace(
            &mut db,
            org_id,
            source,
            "Create pull request",
        )
        .await?;
        // This also checks and locks current ownership, for user and VM callers.
        if matches!(self, Self::Continuation { .. }) {
            use crate::services::workspace_mutation_policy::{
                ensure_workspace_allows_mutation, ensure_workspace_owner, WorkspaceMutationKind,
            };
            ensure_workspace_owner(&mut db, source).await?;
            ensure_workspace_allows_mutation(
                &mut db,
                source,
                WorkspaceMutationKind::GitOperation,
                "workspaces.pr_follow_push",
            )
            .await?;
        } else {
            crate::services::workspace_mutation_policy::ensure_workspace_mutable(
                &mut db,
                source,
                "workspaces.pr_create",
            )
            .await?;
            // No new PR into a workspace whose deletion closed its PRs
            // (`g5.workspace.delete.pull-requests`).
            crate::services::workspace_mutation_policy::ensure_workspace_not_deleting(
                db.conn(),
                target,
            )
            .await?;
        }
        // Membership and lifecycle can change while native save/push or
        // evidence preparation is in flight. Re-admit both sides in each phase.
        ensure_user_workspace_access(
            &state.system_pool,
            &mut db,
            self.actor_id()?,
            org_id,
            target,
        )
        .await?;
        Ok(db)
    }

    async fn source_db(&self, state: &Arc<AppState>, source: Uuid) -> Result<ScopedDb, AppError> {
        self.source_db_for_target(state, source, None).await
    }

    async fn source_db_for_target(
        &self,
        state: &Arc<AppState>,
        source: Uuid,
        target: Option<Uuid>,
    ) -> Result<ScopedDb, AppError> {
        let mut db = match self {
            Self::Continuation { org_id, actor_id } => {
                scoped_db_for_user_actor_in_org_with_lock_timeout(
                    state,
                    *actor_id,
                    *org_id,
                    &["workspaces:write"],
                    Some(std::time::Duration::from_secs(5)),
                )
                .await?
            }
            Self::User(auth) => {
                scoped_db_for_workspace(state, auth, source, &["workspaces:write"]).await?
            }
            Self::Workspace(claims) => {
                if source != claims.workspace_id {
                    return Err(AppError::from(G5Error::Forbidden {
                        missing_scopes: vec![],
                    }));
                }
                scoped_db_for_user_actor_in_org(
                    state,
                    claims.owner_user_id,
                    claims.org_id,
                    &["workspaces:write"],
                )
                .await?
            }
            Self::WorkspaceChild(claims) => {
                use crate::db::workspace_agent_scope::{
                    agent_owner_db, agent_target, AgentTargetNeed,
                };
                if target.is_some_and(|target| target != claims.workspace_id) {
                    return Err(AppError::from(G5Error::Forbidden {
                        missing_scopes: vec![],
                    }));
                }
                let mut db = agent_owner_db(state, claims, &["workspaces:write"]).await?;
                agent_target(&mut db, claims, source, AgentTargetNeed::OtherOnly).await?;
                db
            }
        };
        // Runtime lookup uses a separate system-pool SHARE lock. Capture it
        // before taking the tenant source UPDATE lock, then revalidate below.
        let runtime_handle = if let Self::Workspace(claims) = self {
            let handle = state
                .workspace_runtime
                .owner_handle(source, claims.owner_user_id)
                .await?;
            if handle.org_id != claims.org_id
                || handle.vm_id != claims.vm_id
                || handle.epoch != claims.epoch
            {
                return Err(AppError::from(G5Error::WorkspaceNotRunning));
            }
            Some(handle)
        } else {
            None
        };
        if let Some(target) = target {
            let mut ids = vec![source, target];
            ids.sort_unstable();
            ids.dedup();
            for id in ids {
                // VM admission takes an exclusive source lock below. Acquire
                // that strength in workspace order to avoid cross-PR upgrades.
                let query = if id == source && matches!(self, Self::Workspace(_)) {
                    "SELECT id FROM workspaces WHERE id=$1 AND deleted_at IS NULL FOR UPDATE"
                } else {
                    "SELECT id FROM workspaces WHERE id=$1 AND deleted_at IS NULL FOR SHARE"
                };
                sqlx::query(query)
                    .bind(id)
                    .fetch_optional(db.conn())
                    .await
                    .map_err(lock_wait)?;
            }
        }
        if let Some(handle) = runtime_handle {
            // Reuse live mutation admission, including lease, stop/transfer
            // and owner fencing; read/callback admission is deliberately broader.
            state
                .workspace_runtime
                .lock_owner_handle(&mut db, &handle)
                .await?;
        }
        Ok(db)
    }
}

/// Call after PR/evidence locks, before any external effect or commit. Workspace
/// identity and actor membership are already locked; recheck target authority.
async fn lock_target_access(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    actor: Uuid,
    target: Uuid,
) -> Result<(), AppError> {
    let org = db.primary_org_id()?.into_uuid();
    ensure_user_workspace_access(&state.system_pool, db, actor, org, target).await
}

pub async fn create_or_update(
    state: &Arc<AppState>,
    caller: PullRequestCaller<'_>,
    source_workspace_id: Uuid,
    request: CreatePullRequestRequest,
) -> Result<CreatePullRequestOutcome, AppError> {
    let actor_id = caller.actor_id()?;
    let mode = request.mode;
    let title = request.title.map(|title| title.trim().to_owned());
    if title.as_deref() == Some("") {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"message": "PR title must not be blank"}),
        }));
    }
    if request.replace && mode != PullRequestWriteMode::Create {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"message": "A replacement provider PR is opened by create, not update or edit"}),
        }));
    }
    if mode == PullRequestWriteMode::Edit && title.is_none() && request.description.is_none() {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"message": "Nothing to edit; pass --title, --body or --body-file"}),
        }));
    }
    // Refuse a description GitHub cannot take before saving anything.
    let limit = coordinated_repo_publication::PR_DESCRIPTION_MAX_CHARS;
    if let Some(count) = request
        .description
        .as_deref()
        .map(|text| text.trim().chars().count())
        .filter(|count| *count > limit)
    {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({
                "message": format!("PR description is too long ({count} characters; limit {limit})"),
            }),
        }));
    }
    let target_workspace_id = match request.target_workspace_id {
        Some(target) => target,
        // Update and edit continue this workspace's one open outgoing G5 PR.
        None if mode != PullRequestWriteMode::Create => {
            let mut db = caller.source_db(state, source_workspace_id).await?;
            let targets: Vec<Uuid> = sqlx::query_scalar(
                "SELECT target_workspace_id FROM workspace_pull_requests
                 WHERE source_workspace_id=$1 AND status='open' ORDER BY created_at",
            )
            .bind(source_workspace_id)
            .fetch_all(db.conn())
            .await?;
            db.rollback().await.map_err(AppError::from)?;
            match targets.as_slice() {
                [target] => *target,
                [] => return Err(no_open_pull_request(
                    "This workspace has no open G5 pull request to update; run g5d pr create (or choose Create PR) to open one".into(),
                )),
                _ => return Err(AppError::from(G5Error::ValidationError {
                    details: serde_json::json!({"message": "This workspace has several open G5 pull requests; choose one with --target-workspace-id (or --target)"}),
                })),
            }
        }
        None => {
            let mut db = caller.source_db(state, source_workspace_id).await?;
            let target = targets(state, &mut db, source_workspace_id, actor_id)
                .await?
                .first()
                .map(|target| target.id)
                .ok_or_else(|| AppError::from(G5Error::ValidationError {
                    details: serde_json::json!({"message": "No readable parent or Base workspace is available. Choose a destination with --target-workspace-id (or --target)."}),
                }))?;
            db.rollback().await.map_err(AppError::from)?;
            target
        }
    };
    let explicit_body = request.description.is_some();
    let description = request.description;
    let mut db = caller
        .authorized_db(state, source_workspace_id, target_workspace_id)
        .await?;

    tracing::debug!(
        source_workspace_id = %source_workspace_id,
        target_workspace_id = %target_workspace_id,
        "Workspace endpoint called: create_workspace_pull_request"
    );

    let is_base: bool = sqlx::query_scalar("SELECT is_base FROM workspaces WHERE id=$1")
        .bind(source_workspace_id)
        .fetch_one(db.conn())
        .await?;
    if is_base {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Base workspace cannot create a source pull request",
                "source_workspace_id": source_workspace_id,
            }),
        }));
    }
    if source_workspace_id == target_workspace_id {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"message": "Source and target workspaces must differ"}),
        }));
    }
    if mode == PullRequestWriteMode::Edit {
        return edit_text(
            state,
            db,
            EditText {
                source: source_workspace_id,
                target: target_workspace_id,
                actor: actor_id,
                title: title.as_deref(),
                description,
            },
        )
        .await;
    }
    if mode == PullRequestWriteMode::Update {
        let open: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_pull_requests
             WHERE source_workspace_id=$1 AND target_workspace_id=$2 AND status='open')",
        )
        .bind(source_workspace_id)
        .bind(target_workspace_id)
        .fetch_one(db.conn())
        .await?;
        if !open {
            return Err(no_open_pull_request(
                "No open G5 pull request from this workspace to that target; run g5d pr create (or choose Create PR) to open one".into(),
            ));
        }
    }
    // Native push calls back into the API to advance durable Git. Release the
    // scoped transaction before VM I/O, then capture fresh PR evidence only
    // after every member was saved successfully.
    db.rollback().await.map_err(AppError::from)?;
    if request.save {
        state
            .workspace_runtime
            .prepare_pull_request_source(
                source_workspace_id,
                actor_id,
                caller.expected_incarnation(),
            )
            .await
            .map_err(source_save_failed)?;
    }
    let db = caller
        .authorized_db(state, source_workspace_id, target_workspace_id)
        .await?;

    let prepared = state
        .coordinated_repo_publication_service
        .prepare_workspace_pull_request_create(
            db,
            &state.workspace_merge_service,
            state.repo_storage.clone(),
            source_workspace_id,
            target_workspace_id,
            actor_id,
            description,
            title.clone(),
        )
        .await?;
    let mut db = caller
        .authorized_db(state, source_workspace_id, target_workspace_id)
        .await?;
    let result = state
        .coordinated_repo_publication_service
        .apply_prepared_workspace_pull_request_create(
            &mut db,
            &state.workspace_merge_service,
            prepared,
        )
        .await?;
    lock_target_access(state, &mut db, actor_id, target_workspace_id).await?;
    if mode == PullRequestWriteMode::Update && !result.already_existed {
        // Closed while this request saved: update never opens one. Dropping
        // the transaction discards the new row.
        return Err(no_open_pull_request(
            "The G5 pull request closed while saving; run g5d pr create (or choose Create PR) to open one".into(),
        ));
    }

    let pull_request_id = result.pull_request.id;
    let already_existed = result.already_existed;
    let dirty_drafts = result.dirty_drafts;

    // #1198 / external_git_integration D20, workspace_service D44: before the PR
    // row is committed, push tracked sources and publish local-only sources so the
    // opened PR reflects the latest committed work. Abort-on-failure — an eligible
    // repo that fails to push/publish returns Err, `db` drops without commit, and
    // no PR (local or provider) is created. A non-eligible repo (no remote target)
    // is skipped and Create PR still succeeds for the rest.
    state
        .workspace_service
        .prepare_pr_source_publication(&mut db, pull_request_id, actor_id)
        .await
        .map_err(source_save_failed)?;
    // Whether a later incomplete step concerns GitHub (`g5.git.pr-no-github`).
    // A failed read aborts like the steps above: swallowing it could leave an
    // aborted transaction whose commit silently rolls back.
    let github = expects_github(state, &mut db, pull_request_id).await?;
    db.commit().await.map_err(AppError::from)?;
    // Only now is the source published and the G5 PR durable; provider
    // publication follows and reports its own outcome.
    tracing::info!(
        source_workspace_id = %source_workspace_id,
        target_workspace_id = %target_workspace_id,
        pull_request_id = %pull_request_id,
        already_existed,
        "create_workspace_pull_request: G5 pull request saved after source publication"
    );

    // A local PR is durable now. Any outer provider failure must be reported
    // as partial completion, never as success with the pre-provider summary.
    let mut provider_db = caller
        .authorized_db(state, source_workspace_id, target_workspace_id)
        .await
        .map_err(|err| {
            incomplete_provider_sync(pull_request_id, github, "admission", "not_attempted", err)
        })?;
    let provider_admission = async {
        sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
            .bind(pull_request_id)
            .fetch_optional(provider_db.conn())
            .await?;
        lock_target_access(state, &mut provider_db, actor_id, target_workspace_id).await
    }
    .await;
    provider_admission.map_err(|err| {
        incomplete_provider_sync(pull_request_id, github, "admission", "not_attempted", err)
    })?;
    let before = async {
        let synced_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(provider_db.conn())
            .await?;
        let numbers: Vec<(Uuid, Option<i64>)> = sqlx::query_as(
            "SELECT repo_id, provider_pr_number FROM workspace_pull_request_provider_repos
             WHERE pull_request_id=$1",
        )
        .bind(pull_request_id)
        .fetch_all(provider_db.conn())
        .await?;
        Ok::<_, sqlx::Error>((synced_at, numbers))
    }
    .await;
    let (synced_at, before) = before.map_err(|err| {
        incomplete_provider_sync(
            pull_request_id,
            github,
            "admission",
            "not_attempted",
            err.into(),
        )
    })?;
    state
        .workspace_pull_request_provider_lifecycle_service
        .publish_provider_pull_requests(
            &mut provider_db,
            pull_request_id,
            actor_id,
            crate::services::workspace_pull_request_provider_lifecycle::ProviderPullRequestWrite {
                title: title.as_deref(),
                body: explicit_body,
                replace: request.replace,
            },
        )
        .await
        .map_err(|err| {
            incomplete_provider_sync(
                pull_request_id,
                github,
                "synchronization",
                "unknown",
                err.into(),
            )
        })?;
    record_completed_sync(&mut provider_db, pull_request_id, actor_id)
        .await
        .map_err(|err| {
            incomplete_provider_sync(pull_request_id, github, "completion", "unknown", err)
        })?;
    let pull_request = state
        .coordinated_repo_publication_service
        .load_workspace_pull_request_summary(&mut provider_db, pull_request_id)
        .await
        .map_err(|err| {
            incomplete_provider_sync(pull_request_id, github, "summary", "unknown", err.into())
        })?;
    provider_db.commit().await.map_err(|err| {
        incomplete_provider_sync(pull_request_id, github, "commit", "unknown", err.into())
    })?;

    let publication = publication(
        &pull_request.provider_repos,
        &before,
        synced_at,
        title.is_some(),
        explicit_body,
    );
    Ok(CreatePullRequestOutcome {
        pull_request,
        already_existed,
        dirty_drafts,
        publication,
    })
}

struct EditText<'a> {
    source: Uuid,
    target: Uuid,
    actor: Uuid,
    title: Option<&'a str>,
    description: Option<String>,
}

/// `g5d pr edit` (`g5.git.pr-cli.edit`): new text on the open G5 PR and its
/// provider PRs in one transaction, with no save, push or new provider PR.
async fn edit_text(
    state: &Arc<AppState>,
    mut db: ScopedDb,
    edit: EditText<'_>,
) -> Result<CreatePullRequestOutcome, AppError> {
    let explicit_body = edit.description.is_some();
    let pull_request_id = state
        .coordinated_repo_publication_service
        .edit_open_pull_request_text(&mut db, edit.source, edit.target, edit.title, edit.description)
        .await
        .map_err(|error| match error {
            G5Error::NotFound { .. } => no_open_pull_request(
                "No open G5 pull request from this workspace to that target; run g5d pr create (or choose Create PR) to open one".into(),
            ),
            other => AppError::from(other),
        })?;
    lock_target_access(state, &mut db, edit.actor, edit.target).await?;
    let synced_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(db.conn())
        .await?;
    let before: Vec<(Uuid, Option<i64>)> = sqlx::query_as(
        "SELECT repo_id, provider_pr_number FROM workspace_pull_request_provider_repos
         WHERE pull_request_id=$1",
    )
    .bind(pull_request_id)
    .fetch_all(db.conn())
    .await?;
    state
        .workspace_pull_request_provider_lifecycle_service
        .edit_provider_pull_requests(
            &mut db,
            pull_request_id,
            edit.actor,
            edit.title,
            explicit_body,
        )
        .await?;
    let pull_request = state
        .coordinated_repo_publication_service
        .load_workspace_pull_request_summary(&mut db, pull_request_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    publish_pr_updated(state, edit.source, edit.target, pull_request_id);
    let publication = publication(
        &pull_request.provider_repos,
        &before,
        synced_at,
        edit.title.is_some(),
        explicit_body,
    );
    Ok(CreatePullRequestOutcome {
        pull_request,
        already_existed: true,
        dirty_drafts: Vec::new(),
        publication,
    })
}

fn incomplete_provider_sync(
    pull_request_id: Uuid,
    github: bool,
    phase: &str,
    outcome: &str,
    error: AppError,
) -> AppError {
    let cause = error.inner.error_code();
    tracing::warn!(%pull_request_id, phase, outcome, error = ?error,
        "Provider synchronization incomplete after saving G5 PR");
    // A PR that expects no GitHub has no provider to synchronize; say what
    // was not confirmed without naming one (`g5.git.pr-no-github`).
    let (unconfirmed, rollback) = if github {
        (
            "provider synchronization was not confirmed",
            " Completed publication has not been rolled back.",
        )
    } else {
        ("completing it was not confirmed", "")
    };
    AppError::from(G5Error::ConflictDetails {
        reason: format!(
            "G5 pull request {pull_request_id} was saved, but {unconfirmed} ({phase}: {cause}). Inspect the saved PR, correct the cause, then choose Update PR or run g5d pr update. If you supplied --title, include it again to retry that title.{rollback}"
        ),
        details: serde_json::json!({
            "kind": "pull_request_provider_sync_incomplete",
            "pullRequestId": pull_request_id,
            "providerSyncPhase": phase,
            "providerSyncOutcome": outcome,
            "causeCode": cause,
        }),
    })
}

/// The PR-history message when a push to an Auto-push-off workspace stays in
/// G5 (`g5.git.pr-cli.manual-forwarding`).
pub const NOT_FORWARDED: &str = "Auto-push is off for this workspace, so pushes stay in G5 and GitHub still shows the last published commit. Run g5d pr update (Update PR in the UI) to publish them, or turn Auto-push on.";

pub const PUSH_REFRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const FAILURE_RECORD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// How long automatic refresh may keep meeting a settling state (see
/// [`is_settling`]) before the PR's history reports it as a failure.
pub const SETTLING_REFRESH_BUDGET: std::time::Duration = std::time::Duration::from_secs(300);

const SETTLING_KIND: &str = "pull_request_refresh_settling";

/// A refresh refusal that a later attempt resolves without anyone acting,
/// such as a pushed commit that has not reached GitHub yet.
pub(crate) fn settling(reason: impl Into<String>) -> G5Error {
    G5Error::ConflictDetails {
        reason: reason.into(),
        details: serde_json::json!({"kind": SETTLING_KIND}),
    }
}

/// The provider branch does not hold the pushed source commit yet: remote
/// integration is still pushing it, or, with Auto-push off, nobody has.
pub(crate) fn provider_branch_behind() -> G5Error {
    G5Error::ConflictDetails {
        reason: "Waiting for the pushed source commit to reach the remote branch".into(),
        details: serde_json::json!({"kind": SETTLING_KIND, "providerBranchBehind": true}),
    }
}

fn is_provider_branch_behind(error: &G5Error) -> bool {
    matches!(error, G5Error::ConflictDetails { details, .. }
        if details["kind"] == SETTLING_KIND && details["providerBranchBehind"] == true)
}

/// Whether an automatic refresh failure is a settling state: the push or a
/// concurrent landing/PR write is still in flight, so retrying converges.
pub fn is_settling(error: &G5Error) -> bool {
    match error {
        G5Error::ConflictDetails { details, .. } => details["kind"] == SETTLING_KIND,
        G5Error::DatabaseRetryable { .. } => true,
        G5Error::ValidationError { details } => matches!(
            details["code"].as_str(),
            Some("WORKSPACE_COMMIT_FENCE_BUSY" | "WORKSPACE_CUT_CHANGED")
        ),
        _ => false,
    }
}

/// Automatic refresh admits with a bounded lock wait. Its expiry (SQLSTATE
/// `55P03`) means a landing or PR write still holds the row: settling, not
/// an internal error.
fn lock_wait(error: sqlx::Error) -> AppError {
    let code = error
        .as_database_error()
        .and_then(|db| db.code().map(|code| code.into_owned()));
    if code.as_deref() == Some("55P03") {
        settling("A concurrent workspace or PR update is still in progress").into()
    } else {
        error.into()
    }
}

/// When each PR's unbroken run of settling refresh failures began. The scan
/// leader keeps it across passes; a new leader or restart only restarts the
/// budget. A zero budget reports settling states like any failure.
pub struct SettlingRefreshes {
    budget: std::time::Duration,
    since: std::collections::HashMap<Uuid, std::time::Instant>,
}

impl SettlingRefreshes {
    #[must_use]
    pub fn new(budget: std::time::Duration) -> Self {
        Self {
            budget,
            since: std::collections::HashMap::new(),
        }
    }

    /// Whether `id` has now been settling for at least the budget.
    fn overdue(&mut self, id: Uuid) -> bool {
        let began = self.since.entry(id).or_insert_with(std::time::Instant::now);
        began.elapsed() >= self.budget
    }

    fn settled(&mut self, id: Uuid) {
        self.since.remove(&id);
    }

    /// Forget PRs that are no longer open.
    pub fn retain(&mut self, open: &std::collections::HashSet<Uuid>) {
        self.since.retain(|id, _| open.contains(id));
    }
}

/// Reconcile one durable open PR against current pushed refs. Notifications are
/// hints only; the same operation is used by startup/periodic recovery.
pub async fn refresh_after_push(
    state: &Arc<AppState>,
    org_id: Uuid,
    pull_request_id: Uuid,
) -> Result<bool, AppError> {
    refresh_after_push_with_timeout(state, org_id, pull_request_id, PUSH_REFRESH_TIMEOUT).await
}

/// Bound publication work while allowing its failure record to commit afterward.
/// The explicit budget also lets integration checks exercise deadline recovery.
/// One attempt has no retry left, so it reports a settling state as a failure.
pub async fn refresh_after_push_with_timeout(
    state: &Arc<AppState>,
    org_id: Uuid,
    pull_request_id: Uuid,
    timeout: std::time::Duration,
) -> Result<bool, AppError> {
    let mut settling = SettlingRefreshes::new(std::time::Duration::ZERO);
    refresh_after_push_tracked(state, org_id, pull_request_id, timeout, &mut settling).await
}

/// One attempt of a retrying caller: settling states stay out of the PR's
/// history until they outlast `settling`'s budget.
pub async fn refresh_after_push_tracked(
    state: &Arc<AppState>,
    org_id: Uuid,
    pull_request_id: Uuid,
    timeout: std::time::Duration,
    settling: &mut SettlingRefreshes,
) -> Result<bool, AppError> {
    let mut context = None;
    let mut auto_push = None;
    // Until discovery says otherwise, a failure keeps its GitHub wording.
    let mut github = true;
    let mut stage = "finding the PR";
    let attempted = tokio::time::timeout(timeout, async {
        let deps = crate::db::maintenance::TenantMaintenanceDeps {
            system_pool: state.system_pool.clone(),
            tenant_pool: state.tenant_pool.clone(),
            authz: state.authz.clone(),
            repo_storage: state.repo_storage.clone(),
        };
        let mut discovery =
            crate::db::maintenance::scoped_db_for_maintenance_org(&deps, org_id).await?;
        let pair: Option<(Uuid, Uuid, Uuid)> = sqlx::query_as(
            "SELECT pr.source_workspace_id, pr.target_workspace_id, sw.owner_user_id
         FROM workspace_pull_requests pr
         JOIN workspaces sw ON sw.id=pr.source_workspace_id AND sw.org_id=pr.org_id
         JOIN workspaces tw ON tw.id=pr.target_workspace_id AND tw.org_id=pr.org_id
         WHERE pr.id=$1 AND pr.org_id=$2 AND pr.status='open'
           AND sw.deleted_at IS NULL AND tw.deleted_at IS NULL AND sw.owner_user_id IS NOT NULL",
        )
        .bind(pull_request_id)
        .bind(org_id)
        .fetch_optional(discovery.conn())
        .await?;
        let Some((source, target, actor)) = pair else {
            return Ok(false);
        };
        context = Some((source, target, actor));
        // Discovery has no effect authority. Inaccessible unchanged PRs are no-ops,
        // and mutable local preflight evidence alone is not provider completion.
        stage = "reading published heads";
        if !needs_push_refresh(state, &mut discovery, source, pull_request_id).await? {
            return Ok(false);
        }
        if state
            .workspace_pull_request_provider_lifecycle_service
            .automatic_refresh_paused(&mut discovery, pull_request_id)
            .await?
        {
            // The stage matches the first observation's, so its report is not repeated.
            stage = "checking the GitHub PR";
            return Err(provider_pull_request_completed().into());
        }
        // With Auto-push off a push reaches the provider only when someone
        // publishes it (g5.git.pr-cli.manual-forwarding).
        let push_mode: String = sqlx::query_scalar("SELECT push_mode FROM workspaces WHERE id=$1")
            .bind(source)
            .fetch_one(discovery.conn())
            .await?;
        auto_push = Some(push_mode == "auto");
        github = expects_github(state, &mut discovery, pull_request_id).await?;
        discovery.rollback().await?;
        let caller = PullRequestCaller::Continuation {
            org_id,
            actor_id: actor,
        };
        refresh_existing_after_push(
            state,
            &caller,
            source,
            target,
            pull_request_id,
            github,
            &mut stage,
        )
        .await
    })
    .await;
    let timed_out = attempted.is_err();
    let result = attempted.unwrap_or_else(|_| {
        Err(G5Error::ServiceUnavailable {
            message: "Automatic PR refresh deadline elapsed; push again, or retry with g5d pr update (Update PR in the UI).".into(),
        }
        .into())
    });
    let error = match &result {
        Ok(_) => {
            settling.settled(pull_request_id);
            return result;
        }
        Err(error) => error,
    };
    // With Auto-push off, a provider branch that lacks the push stays that way
    // until someone publishes it: say so once rather than wait.
    if auto_push == Some(false) && is_provider_branch_behind(&error.inner) {
        if let Some((source, target, actor)) = context {
            let caller = PullRequestCaller::Continuation {
                org_id,
                actor_id: actor,
            };
            match tokio::time::timeout(
                FAILURE_RECORD_TIMEOUT,
                record_refresh_failure(
                    state,
                    &caller,
                    source,
                    target,
                    pull_request_id,
                    NOT_FORWARDED,
                ),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(record_error)) => tracing::warn!(%pull_request_id, error = %record_error,
                    "Could not record that a push was not forwarded; durable inventory will retry"),
                Err(_) => tracing::warn!(%pull_request_id,
                    "Recording that a push was not forwarded timed out; durable inventory will retry"),
            }
        }
        settling.settled(pull_request_id);
        return Ok(false);
    }
    let waiting = !timed_out && is_settling(&error.inner);
    if waiting && !settling.overdue(pull_request_id) {
        tracing::debug!(%pull_request_id, stage, error = %error,
            "Automatic PR refresh is waiting for a settling state");
        return result;
    }
    if matches!(
        error.inner,
        G5Error::Internal { .. } | G5Error::GitError { .. }
    ) {
        tracing::warn!(%pull_request_id, stage, error = ?error,
            "Automatic PR refresh failed with an internal error");
    }
    if let Some((source, target, actor)) = context {
        let caller = PullRequestCaller::Continuation {
            org_id,
            actor_id: actor,
        };
        let message = refresh_failure_message(
            &error.inner,
            stage,
            timed_out,
            waiting.then_some(settling.budget),
            github,
        );
        // Admission and the completion recheck can themselves block. Keep this
        // separate from the publication budget so later PRs always get a turn.
        match tokio::time::timeout(
            FAILURE_RECORD_TIMEOUT,
            record_refresh_failure(state, &caller, source, target, pull_request_id, &message),
        )
        .await
        {
            // A reported settling run starts over, so a stage or reason that
            // changes while it persists is not reported again on every pass.
            Ok(Ok(())) => {
                if waiting {
                    settling.settled(pull_request_id);
                }
            }
            Ok(Err(record_error)) => tracing::warn!(%pull_request_id, error = %record_error,
                "Could not record PR refresh failure; durable inventory will retry"),
            Err(_) => tracing::warn!(%pull_request_id,
                "PR refresh failure recording timed out; durable inventory will retry"),
        }
    }
    result
}

/// The PR-history message of a reported refresh failure: where it stopped and
/// why. Internal details stay in the server log, never in reader-visible history.
///
/// `github` is false for a PR that expects no GitHub (`g5.git.pr-no-github`):
/// its message then names no provider and gives no provider advice.
fn refresh_failure_message(
    error: &G5Error,
    stage: &str,
    timed_out: bool,
    settling_budget: Option<std::time::Duration>,
    github: bool,
) -> String {
    if timed_out {
        let advice = if github {
            " Check provider access and synchronization."
        } else {
            ""
        };
        return format!("Automatic PR refresh deadline elapsed while {stage}; push again, or retry with g5d pr update (Update PR in the UI).{advice}");
    }
    let reason = match error {
        G5Error::Internal { .. } | G5Error::GitError { .. } => {
            "an internal error occurred; details are in the server log".to_owned()
        }
        G5Error::Conflict { reason } | G5Error::ConflictDetails { reason, .. } => reason.clone(),
        other => other.to_string(),
    };
    let reason = crate::services::identity::github_app_auth::redact_message(&reason);
    let (settling_advice, failure_advice) = if github {
        (
            "Check branch synchronization and provider access",
            "Check provider access, PR status and branch synchronization",
        )
    } else {
        (
            "Check branch synchronization",
            "Check PR status and branch synchronization",
        )
    };
    match settling_budget.filter(|budget| !budget.is_zero()) {
        Some(budget) => format!(
            "Automatic PR refresh has not completed for over {} min while {stage}: {reason}. {settling_advice}, then push again or retry with g5d pr update (Update PR in the UI).",
            budget.as_secs().div_ceil(60)
        ),
        None => format!(
            "Automatic PR refresh failed while {stage} ({}): {reason}. {failure_advice}, then push again or retry with g5d pr update (Update PR in the UI).",
            error.error_code()
        ),
    }
}

async fn record_refresh_failure(
    state: &Arc<AppState>,
    caller: &PullRequestCaller<'_>,
    source: Uuid,
    target: Uuid,
    pull_request_id: Uuid,
    message: &str,
) -> Result<(), AppError> {
    // Recording failure is not new publication authority. Never write with
    // a stale owner's identity, and never turn a completed push into failure.
    let mut db = caller.source_db(state, source).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, source).await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
            .bind(pull_request_id)
            .fetch_optional(db.conn())
            .await?;
    if status.as_deref() != Some("open")
        || !needs_push_refresh(state, &mut db, source, pull_request_id)
            .await
            .unwrap_or(true)
    {
        return Ok(());
    }
    let updated = sqlx::query("INSERT INTO workspace_pull_request_events
        (org_id,pull_request_id,event_type,actor_id,source_heads,target_heads,preflight,message,created_at)
        SELECT pr.org_id,pr.id,'provider_failed',$4,pr.source_heads,pr.target_heads,pr.preflight,$2,clock_timestamp()
        FROM workspace_pull_requests pr
        WHERE pr.id=$1 AND pr.status='open' AND pr.source_workspace_id=$3
          AND EXISTS(SELECT 1 FROM workspaces WHERE id=$3 AND owner_user_id=$4 AND deleted_at IS NULL)
          AND NOT EXISTS (SELECT 1 FROM (
              SELECT event_type,message FROM workspace_pull_request_events
              WHERE pull_request_id=$1 AND source_heads IS NOT NULL
                AND event_type IN ('provider_failed','provider_updated')
              ORDER BY created_at DESC,id DESC LIMIT 1
          ) latest WHERE latest.event_type='provider_failed' AND latest.message=$2)")
        .bind(pull_request_id).bind(message).bind(source).bind(caller.actor_id()?)
        .execute(db.conn()).await?;
    db.commit().await?;
    if updated.rows_affected() > 0 {
        publish_pr_updated(state, source, target, pull_request_id);
    }
    Ok(())
}

/// `stage` names the step in progress, for a failure's history message.
async fn refresh_existing_after_push(
    state: &Arc<AppState>,
    caller: &PullRequestCaller<'_>,
    source: Uuid,
    target: Uuid,
    id: Uuid,
    github: bool,
    stage: &mut &'static str,
) -> Result<bool, AppError> {
    let actor = caller.actor_id()?;
    *stage = "checking access";
    let mut db = caller.authorized_db(state, source, target).await?;
    *stage = "reading published heads";
    if !needs_push_refresh(state, &mut db, source, id).await? {
        return Ok(false);
    }
    *stage = "preparing PR evidence";
    let prepared = state
        .coordinated_repo_publication_service
        .prepare_published_workspace_pull_request_refresh(
            db,
            &state.workspace_merge_service,
            source,
            id,
            actor,
        )
        .await?;
    *stage = "checking access";
    let mut db = caller.authorized_db(state, source, target).await?;
    // Provider merge takes the same lock. Recheck closed/merged state after
    // waiting; never advance evidence belonging to an accepted merge.
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(db.conn())
            .await
            .map_err(lock_wait)?;
    if status.as_deref() != Some("open") {
        return Ok(false);
    }
    // Target identity and ownership are locked above. Recheck the fixed audience.
    // The locked source's owner FK preserves organization membership. Recheck
    // current access below; credential resolution also rechecks membership at
    // each provider admission without retaining a second pooled transaction.
    lock_target_access(state, &mut db, actor, target).await?;
    *stage = "reading published heads";
    if !needs_push_refresh(state, &mut db, source, id).await? {
        return Ok(false);
    }
    *stage = if github {
        "checking the GitHub PR"
    } else {
        "checking the PR"
    };
    let admission = state
        .workspace_pull_request_provider_lifecycle_service
        .ensure_automatic_refresh_allowed(&mut db, id, actor)
        .await?;
    if admission == AutomaticRefreshAdmission::Paused {
        // Keep the observation so later passes refuse without a provider read;
        // the refusal itself is still reported (g5.git.pr-refresh.completed-provider).
        db.commit().await?;
        return Err(provider_pull_request_completed().into());
    }
    *stage = "saving PR evidence";
    let summary = state
        .coordinated_repo_publication_service
        .apply_prepared_workspace_pull_request_refresh(
            &mut db,
            &state.workspace_merge_service,
            prepared,
        )
        .await
        .map_err(|error| match error {
            // A landing moved the cut or PR since preparation; the next attempt
            // prepares from the new one.
            G5Error::Conflict { reason } => settling(reason),
            other => other,
        })?;
    *stage = if github {
        "synchronizing with GitHub"
    } else {
        "updating the PR"
    };
    state
        .workspace_pull_request_provider_lifecycle_service
        .synchronize_after_push(&mut db, id, actor)
        .await?;
    *stage = "confirming published heads";
    if published_heads(state, &mut db, source).await? != head_vector(&summary.source_heads) {
        return Err(
            settling("Source advanced during automatic PR refresh; retry current heads").into(),
        );
    }
    *stage = "recording completion";
    record_completed_sync(&mut db, id, actor).await?;
    db.commit().await?;
    publish_pr_updated(state, source, target, id);
    Ok(true)
}

async fn needs_push_refresh(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    source: Uuid,
    id: Uuid,
) -> Result<bool, AppError> {
    let Some((heads, provider_mismatch)) = sqlx::query_as::<_, (serde_json::Value, bool)>(
        "SELECT pr.source_heads, EXISTS (
             SELECT 1 FROM workspace_pull_request_provider_repos p
             LEFT JOIN LATERAL jsonb_array_elements(pr.source_heads) h ON h->>'repoId'=p.repo_id::text
             WHERE p.pull_request_id=pr.id AND p.source_head_sha IS DISTINCT FROM h->>'headCommit'
         ) FROM workspace_pull_requests pr WHERE pr.id=$1 AND pr.status='open'",
    ).bind(id).fetch_optional(db.conn()).await? else { return Ok(false) };
    Ok(provider_mismatch
        || published_heads(state, db, source).await? != head_vector(&heads)
        || state
            .workspace_pull_request_provider_lifecycle_service
            .has_missing_provider_pull_request(db, id)
            .await?)
}

/// Whether the PR expects GitHub (`g5.git.pr-no-github`): it has a provider
/// record, or publication would publish one of its members there.
async fn expects_github(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    id: Uuid,
) -> Result<bool, AppError> {
    let recorded: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1)",
    )
    .bind(id)
    .fetch_one(db.conn())
    .await?;
    Ok(recorded
        || state
            .workspace_pull_request_provider_lifecycle_service
            .has_missing_provider_pull_request(db, id)
            .await?)
}

async fn published_heads(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<std::collections::BTreeMap<Uuid, String>, AppError> {
    let snapshot = crate::services::workspace_reader::resolve_published_snapshot(
        db,
        &state.repo_storage,
        g5_contracts::types::AuthorityTarget::Workspace { workspace_id },
    )
    .await?;
    Ok(snapshot
        .repos
        .into_iter()
        .map(|repo| (repo.repo_id, repo.head_commit))
        .collect())
}

fn head_vector(value: &serde_json::Value) -> std::collections::BTreeMap<Uuid, String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|member| {
            Some((
                member["repoId"].as_str()?.parse().ok()?,
                member["headCommit"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// Full-flow outcomes reuse PR history. Local-only preflight events and individual
/// provider events cannot settle an earlier automatic failure. The PR row lock is
/// held across these writes; effect time avoids transaction-start ordering races.
async fn record_completed_sync(db: &mut ScopedDb, id: Uuid, actor: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO workspace_pull_request_events
        (org_id,pull_request_id,event_type,actor_id,source_heads,target_heads,preflight,created_at)
        SELECT org_id,id,'provider_updated',$2,source_heads,target_heads,preflight,clock_timestamp()
        FROM workspace_pull_requests
        WHERE id=$1 AND status='open' AND provider_status IN ('open','local_only')",
    )
    .bind(id)
    .bind(actor)
    .execute(db.conn())
    .await?;
    Ok(())
}

fn publish_pr_updated(state: &Arc<AppState>, source: Uuid, target: Uuid, id: Uuid) {
    for workspace_id in [source, target] {
        state.event_bus.publish(
            &crate::ws::Channel::Workspace(workspace_id),
            &crate::ws::events::WsEvent::WorkspacePullRequestUpdated {
                workspace_id,
                pull_request_id: id,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{is_settling, refresh_failure_message, settling};
    use g5_contracts::error::G5Error;
    use std::time::Duration;

    #[test]
    fn settling_states_are_typed_and_failures_are_not() {
        assert!(is_settling(&settling("waiting")));
        assert!(is_settling(&G5Error::DatabaseRetryable {
            message: "deadlock".into()
        }));
        for code in ["WORKSPACE_COMMIT_FENCE_BUSY", "WORKSPACE_CUT_CHANGED"] {
            assert!(is_settling(&G5Error::ValidationError {
                details: serde_json::json!({"code": code}),
            }));
        }
        assert!(!is_settling(&G5Error::Conflict {
            reason: "GitHub credential is required to refresh this PR".into()
        }));
        assert!(!is_settling(&G5Error::ConflictDetails {
            reason: "other".into(),
            details: serde_json::json!({"kind": "pull_request_provider_sync_incomplete"}),
        }));
        assert!(!is_settling(&G5Error::Internal {
            message: "boom".into()
        }));
    }

    #[test]
    fn failure_messages_name_the_stage_and_reason_without_internal_details() {
        let conflict = refresh_failure_message(
            &G5Error::Conflict {
                reason: "Could not verify published source: https://x-access-token:secret@github.com/a/b denied".into(),
            },
            "synchronizing with GitHub",
            false,
            None,
            true,
        );
        assert!(conflict.starts_with(
            "Automatic PR refresh failed while synchronizing with GitHub (CONFLICT): Could not verify published source: [REDACTED"
        ), "{conflict}");
        assert!(!conflict.contains("secret"), "{conflict}");
        assert!(conflict.ends_with("push again or retry with g5d pr update (Update PR in the UI)."));

        let internal = refresh_failure_message(
            &G5Error::Internal {
                message: "Database error: relation pr_secret_table".into(),
            },
            "preparing PR evidence",
            false,
            None,
            true,
        );
        assert!(internal.contains("failed while preparing PR evidence (INTERNAL_ERROR): an internal error occurred; details are in the server log."), "{internal}");
        assert!(!internal.contains("pr_secret_table"), "{internal}");

        let waited = refresh_failure_message(
            &settling("Waiting for the pushed source commit to reach the remote branch"),
            "synchronizing with GitHub",
            false,
            Some(Duration::from_secs(300)),
            true,
        );
        assert!(waited.starts_with("Automatic PR refresh has not completed for over 5 min while synchronizing with GitHub: Waiting for the pushed source commit to reach the remote branch."), "{waited}");

        // A single attempt reports a settling state as an ordinary failure.
        let once = refresh_failure_message(
            &settling("Waiting"),
            "synchronizing with GitHub",
            false,
            Some(Duration::ZERO),
            true,
        );
        assert!(
            once.contains("failed while synchronizing with GitHub (CONFLICT): Waiting."),
            "{once}"
        );

        let deadline = refresh_failure_message(
            &G5Error::ServiceUnavailable {
                message: "late".into(),
            },
            "synchronizing with GitHub",
            true,
            None,
            true,
        );
        assert!(
            deadline.starts_with(
                "Automatic PR refresh deadline elapsed while synchronizing with GitHub;"
            ),
            "{deadline}"
        );
        assert!(deadline.contains("provider access"), "{deadline}");

        // A PR that expects no GitHub gets no provider advice
        // (`g5.git.pr-no-github`).
        for local in [
            refresh_failure_message(&settling("Waiting"), "updating the PR", false, None, false),
            refresh_failure_message(
                &settling("Waiting"),
                "updating the PR",
                false,
                Some(Duration::from_secs(300)),
                false,
            ),
            refresh_failure_message(
                &G5Error::ServiceUnavailable {
                    message: "late".into(),
                },
                "updating the PR",
                true,
                None,
                false,
            ),
        ] {
            assert!(!local.contains("provider"), "{local}");
            assert!(!local.contains("GitHub"), "{local}");
            assert!(local.contains("g5d pr update"), "{local}");
        }
    }
}
