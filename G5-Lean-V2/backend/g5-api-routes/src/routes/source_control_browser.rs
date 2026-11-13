//! Navigation captures pushed comparison cuts; explicit Save also checks local
//! state. Ordinary branch events do not replace an applied selection.
use super::{audited::AuditedRouter, ResponseEnvelope};
use crate::services::git::{
    workspace_repositories::load_workspace_repositories, workspace_revision_history as history,
};
use crate::{
    db::scoped::scoped_db_for_workspace,
    errors::{AppError, FailureKind},
    middleware::AuthContext,
    services::AppState,
};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use g5_auth::Scope;
use g5_contracts::G5Error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use uuid::Uuid;

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/menu", get(menu), &[Scope::WorkspacesRead])
        .audited_route("/checkpoint", get(checkpoint), &[Scope::WorkspacesRead])
        .audited_route("/check", post(check), &[Scope::WorkspacesRead])
        .audited_route("/history", post(history_page), &[Scope::WorkspacesRead])
        .audited_route("/apply", post(apply), &[Scope::WorkspacesRead])
        .audited_route("/pushed", post(pushed_scope), &[Scope::WorkspacesRead])
        .audited_route("/save", post(save), &[Scope::WorkspacesWrite])
        .into_router()
}
fn conflict(message: impl Into<String>) -> AppError {
    G5Error::Conflict {
        reason: message.into(),
    }
    .into()
}
fn stale_list() -> AppError {
    conflict("The workspace changed. Refresh list, then click Show.")
        .with_kind(FailureKind::SourceControlStaleList)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repo {
    repo_id: Uuid,
    name: String,
    role: String,
    base: Option<String>,
    workspace_base: Option<String>,
    parent: Option<String>,
    parent_name: Option<String>,
    comparison: Option<g5_scm::PushedComparison>,
    /// Personal workspaces have no fork baseline or pushed-since-fork history.
    unpushed_only: bool,
    pushed: String,
    commits: Vec<g5_scm::SourceCommit>,
    truncated: bool,
    live: Option<Value>,
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history_commits: Vec<g5_scm::SourceHistoryCommit>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Menu {
    /// The store narrowed to the capturing scope, and the workspace's
    /// organization (`g5.repo.storage-tenancy`).
    #[serde(skip)]
    storage: g5_api_core::services::git::repo_store::ScopedRepoStorage,
    #[serde(skip)]
    org_id: Uuid,
    generation: i64,
    fingerprint: String,
    repos: Vec<Repo>,
    private: bool,
}

async fn capture(
    state: &Arc<AppState>,
    auth: &AuthContext,
    id: Uuid,
    full: bool,
) -> Result<Menu, AppError> {
    let actor = auth.require_user_id()?;
    let mut db = scoped_db_for_workspace(state, auth, id, &["workspaces:read"]).await?;
    let org = db.primary_org_id()?.into_uuid();
    let kind = crate::services::workspace_kind::workspace_kind(db.conn(), id).await?;
    let unpushed_only = matches!(
        kind,
        Some(
            g5_contracts::types::WorkspaceKind::Personal | g5_contracts::types::WorkspaceKind::Base
        )
    );
    let generation = state
        .workspace_service
        .source_control_generation(&mut db, id)
        .await?;
    let members = load_workspace_repositories(db.conn(), id, false).await?;
    let (heads, parents) = history::comparison_cuts(
        &mut db,
        &state.system_pool,
        &state.repo_storage,
        id,
        actor,
        None,
    )
    .await?;
    let bases = if unpushed_only {
        None
    } else {
        history::saved_base(db.conn(), id).await?
    };
    let handle = state
        .workspace_runtime
        .get(id)
        .await?
        .filter(|h| h.status == "running" && h.owner_user_id == actor);
    let storage = state.repo_storage.scoped(&db);
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    db.rollback().await?;
    let mut repos = Vec::new();
    let mut signatures = Vec::new();
    for member in members {
        let head = heads
            .iter()
            .find(|h| h.repo_id == member.repo_id.to_string());
        let missing_head = head.is_none();
        let fallback = g5_contracts::types::MergeSourceHead {
            repo_id: member.repo_id.to_string(),
            workspace_id: id.to_string(),
            branch_name: member.branch_name.clone(),
            head_commit: String::new(),
        };
        let head = head.unwrap_or(&fallback);
        let base = bases
            .as_ref()
            .and_then(|b| b.iter().find(|h| h.repo_id == head.repo_id))
            .map(|h| h.head_commit.clone());
        let mut repo = Repo {
            history_commits: Vec::new(),
            repo_id: member.repo_id,
            name: member.repo_name,
            role: member.role,
            workspace_base: base.clone(),
            base: None,
            parent: parents
                .as_ref()
                .and_then(|hs| hs.iter().find(|h| h.repo_id == head.repo_id))
                .map(|h| h.head_commit.clone()),
            parent_name: parents
                .as_ref()
                .and_then(|hs| hs.iter().find(|h| h.repo_id == head.repo_id))
                .map(|h| h.branch_name.clone()),
            comparison: None,
            unpushed_only,
            pushed: head.head_commit.clone(),
            commits: Vec::new(),
            truncated: false,
            live: None,
            error: if missing_head {
                Some("Pushed source head is unavailable".into())
            } else {
                None
            },
        };
        // The list describes pushed history. Local files are read only by Show.
        if handle.is_some() && !missing_head {
            repo.live = Some(json!({"selectable":true}));
        }
        if !unpushed_only && !missing_head {
            if let Some(parent) = &repo.parent {
                let path = storage.repo_path(org_id, repo.repo_id);
                let tip = repo.pushed.clone();
                let parent = parent.clone();
                let fork = repo.workspace_base.clone();
                match tokio::task::spawn_blocking(move || -> Result<_, G5Error> {
                    let path = path?;
                    if full {
                        let comparison =
                            capture_comparison(&path, &tip, &parent, fork.as_deref(), 250, None)?;
                        Ok((comparison.merge_base.clone(), Some(comparison)))
                    } else {
                        Ok((
                            g5_scm::GitRepo::open(&path)?.pushed_comparison_base(&tip, &parent)?,
                            None,
                        ))
                    }
                })
                .await
                .map_err(|e| conflict(e.to_string()))?
                {
                    Ok((base, comparison)) => {
                        repo.base = Some(base);
                        if let Some(comparison) = comparison {
                            repo.commits = comparison.commits.clone();
                            repo.truncated = comparison.truncated;
                            repo.comparison = Some(comparison);
                        }
                    }
                    Err(error) => repo.error = Some(error.to_string()),
                }
            } else {
                repo.error = Some("Parent comparison unavailable; parent read access and captured heads are required".into());
            }
        }
        signatures.push(json!([
            repo.repo_id,
            repo.role,
            repo.base,
            repo.workspace_base,
            repo.unpushed_only,
            repo.pushed,
            repo.live.is_some(),
            repo.error
        ]));
        repos.push(repo);
    }
    let mut db = scoped_db_for_workspace(state, auth, id, &["workspaces:read"]).await?;
    crate::db::scoped::ensure_user_workspace_access_until_commit(
        &state.system_pool,
        &mut db,
        actor,
        org,
        id,
    )
    .await?;
    db.rollback().await?;
    if let Some(original) = &handle {
        if !state.workspace_runtime.get(id).await?.is_some_and(|h| {
            h.status == "running"
                && h.owner_user_id == actor
                && h.vm_id == original.vm_id
                && h.epoch == original.epoch
        }) {
            return Err(conflict(
                "Live authority changed while building the checklist",
            ));
        }
    }
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&signatures).map_err(|e| conflict(e.to_string()))?)
    );
    Ok(Menu {
        storage,
        org_id,
        generation,
        fingerprint,
        repos,
        private: handle.is_some(),
    })
}
async fn menu(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ResponseEnvelope<Menu>>, AppError> {
    let menu = capture(&state, &auth, id, true).await?;
    Ok(Json(ResponseEnvelope::ok(menu)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    fingerprint: String,
    #[serde(default)]
    selected: BTreeMap<Uuid, Vec<String>>,
}
async fn check(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
    Json(input): Json<Check>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let state_now = capture(&state, &auth, id, false).await?;
    if state_now.fingerprint == input.fingerprint {
        return Ok(Json(ResponseEnvelope::ok(json!({"changed":false}))));
    }
    if input.selected.len() > 100 || input.selected.values().any(|ids| ids.len() > 250) {
        return Err(conflict("Invalid commit selection"));
    }
    let (menu, missing) = refreshed_menu(&state, &auth, id, input.selected).await?;
    Ok(Json(ResponseEnvelope::ok(
        json!({"changed":true,"menu":menu,"missingCommits":missing}),
    )))
}

async fn refreshed_menu(
    state: &Arc<AppState>,
    auth: &AuthContext,
    id: Uuid,
    selected: BTreeMap<Uuid, Vec<String>>,
) -> Result<(Menu, BTreeMap<Uuid, Vec<String>>), AppError> {
    let mut menu = capture(state, auth, id, true).await?;
    if !selected.is_empty() {
        if let Some(repo) = menu.repos.iter().find(|repo| repo.error.is_some()) {
            return Err(conflict(format!(
                "Repository changes in {} are unavailable; retry",
                repo.name
            )));
        }
    }
    let mut missing = BTreeMap::new();
    for (repo_id, selected) in selected {
        let repo = menu
            .repos
            .iter_mut()
            .find(|repo| repo.repo_id == repo_id)
            .ok_or_else(|| conflict("Selected repository is no longer authorized"))?;
        if repo.error.is_some() || selected.is_empty() {
            continue;
        }
        let path = menu
            .storage
            .repo_path(menu.org_id, repo_id)
            .map_err(|_| conflict("Repository unavailable"))?;
        let tip = repo.pushed.clone();
        let fork = repo.workspace_base.clone();
        let (commits, unavailable) = tokio::task::spawn_blocking(move || -> Result<_, G5Error> {
            let git = g5_scm::GitRepo::open(&path)?;
            let mut commits = Vec::new();
            let mut unavailable = Vec::new();
            for sha in selected {
                let cut = if fork.as_ref() == Some(&sha) {
                    &sha
                } else {
                    &tip
                };
                match git.source_control_available_commit(cut, &sha)? {
                    Some(commit) => commits.push(commit),
                    None => unavailable.push(sha),
                }
            }
            Ok((commits, unavailable))
        })
        .await
        .map_err(|error| conflict(error.to_string()))??;
        repo.history_commits = commits;
        if !unavailable.is_empty() {
            missing.insert(repo_id, unavailable);
        }
    }
    // Selected-history validation can outlast the initial capture's access check.
    let mut db = scoped_db_for_workspace(state, auth, id, &["workspaces:read"]).await?;
    let org = db.primary_org_id()?.into_uuid();
    crate::db::scoped::ensure_user_workspace_access_until_commit(
        &state.system_pool,
        &mut db,
        auth.require_user_id()?,
        org,
        id,
    )
    .await?;
    db.rollback().await?;
    Ok((menu, missing))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selection {
    repo_id: Uuid,
    base: Option<String>,
    pushed: String,
    commits: Vec<String>,
    include_unpushed: bool,
    // Older clients may still send this field; local state is captured at Show.
    #[serde(default, rename = "liveFingerprint")]
    _live_fingerprint: Option<String>,
    #[serde(default)]
    all_pushed: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    repos: Vec<Selection>,
    #[serde(default)]
    fingerprint: Option<String>,
}
async fn apply(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
    Json(input): Json<Apply>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    if input.repos.len() > 100
        || input.repos.iter().any(|repo| {
            repo.commits.len() > 250
                || repo.commits.iter().collect::<BTreeSet<_>>().len() != repo.commits.len()
        })
        || input
            .repos
            .iter()
            .map(|r| r.repo_id)
            .collect::<BTreeSet<_>>()
            .len()
            != input.repos.len()
    {
        return Err(conflict("Invalid repository selection"));
    }
    let current = capture(&state, &auth, id, false).await?;
    if input
        .fingerprint
        .as_ref()
        .is_some_and(|fingerprint| fingerprint != &current.fingerprint)
        || input.repos.iter().any(|selected| {
            current
                .repos
                .iter()
                .find(|repo| repo.repo_id == selected.repo_id)
                .is_some_and(|repo| {
                    repo.pushed != selected.pushed
                        || (!repo.unpushed_only && repo.base != selected.base)
                })
        })
    {
        let selected = input
            .repos
            .into_iter()
            .map(|repo| (repo.repo_id, repo.commits))
            .collect();
        return refresh_for_selection(&state, &auth, id, selected).await;
    }
    let private = input.repos.iter().any(|r| r.include_unpushed);
    let mut own_merge_selection = !input.repos.is_empty()
        && input.repos.iter().all(|selected| {
            !selected.include_unpushed && !selected.all_pushed && !selected.commits.is_empty()
        });
    let history_selections = input
        .repos
        .iter()
        .map(|r| (r.repo_id, r.pushed.clone(), r.commits.clone()))
        .collect::<Vec<_>>();
    let mut patches = Vec::new();
    for selected in input.repos {
        let repo = current
            .repos
            .iter()
            .find(|r| r.repo_id == selected.repo_id)
            .ok_or_else(|| conflict("Selected repository is no longer authorized"))?;
        if repo.base != selected.base {
            return Err(if repo.unpushed_only || repo.error.is_some() {
                conflict("Selected workspace base is unavailable")
            } else {
                stale_list()
            });
        }
        let planned = if repo.unpushed_only {
            if selected.base.is_some() || selected.all_pushed {
                return Err(conflict("No outgoing comparison is available"));
            }
            if selected.commits.len() > 250
                || selected.commits.iter().collect::<BTreeSet<_>>().len() != selected.commits.len()
            {
                return Err(conflict("Invalid history selection"));
            }
            let path = current
                .storage
                .repo_path(current.org_id, selected.repo_id)
                .map_err(|_| conflict("Repository unavailable"))?;
            let tip = selected.pushed.clone();
            let commits = selected.commits.clone();
            let current_tip = repo.pushed.clone();
            let patches=tokio::task::spawn_blocking(move || -> Result<Vec<Value>,G5Error> {
                let git=g5_scm::GitRepo::open(&path)?;
                if tip!=current_tip && git.source_control_available_commit(&current_tip,&tip)?.is_none() {return Err(G5Error::StaleHistory);}
                let mut patches=Vec::new();
                for sha in commits {
                    if git.source_control_available_commit(&tip,&sha)?.is_none() {return Err(G5Error::StaleHistory);}
                    let commit=git.source_control_commit(&tip,&sha)?;
                    for file in git.source_control_commit_diff(&tip,&sha)? {
                        patches.push(json!({"label":format!("{} · {}{}",&sha[..7],commit.summary,if commit.parents.len()>1 {" · merge: first parent"}else{""}),"before":commit.parents.first().map(|p|json!(p)).unwrap_or(json!({"kind":"emptyTree"})),"after":sha,"file":file}));
                    }
                }
                Ok(patches)
            }).await.map_err(|e| conflict(e.to_string()))?;
            patches.map(|patches| (false, patches, false))
        } else {
            let path = current
                .storage
                .repo_path(current.org_id, selected.repo_id)
                .map_err(|_| conflict("Repository unavailable"))?;
            let requested_tip = selected.pushed.clone();
            let current_tip = repo.pushed.clone();
            let base = selected
                .base
                .clone()
                .ok_or_else(|| conflict("Selected workspace base is unavailable"))?;
            let commits = selected.commits.clone();
            let all_pushed = selected.all_pushed;
            let parent = repo.parent.clone();
            let fork = repo.workspace_base.clone();
            if commits.len() > 250 || commits.iter().collect::<BTreeSet<_>>().len() != commits.len()
            {
                return Err(conflict("Invalid commit selection"));
            }
            tokio::task::spawn_blocking(move || -> Result<_, G5Error> {
            let git = g5_scm::GitRepo::open(&path)?;
            if requested_tip != current_tip && git.source_control_available_commit(&current_tip, &requested_tip)?.is_none() { return Err(G5Error::StaleHistory); }
            let comparison = git.pushed_comparison(&requested_tip, parent.as_deref().ok_or(G5Error::StaleHistory)?, fork.as_deref(), 250)?;
            if comparison.merge_base != base { return Err(G5Error::StaleHistory); }
            let available = comparison.commits; let truncated = comparison.truncated;
            let all = all_pushed || (!truncated && commits.len() == available.len() && available.iter().all(|c| commits.contains(&c.sha)));
            for sha in &commits {
                let cut = if fork.as_ref() == Some(sha) { sha.as_str() } else { requested_tip.as_str() };
                if git.source_control_available_commit(cut, sha)?.is_none() { return Err(G5Error::StaleHistory); }
            }
            let mut result = Vec::new();
            if all && (all_pushed || !commits.is_empty()) {
                for file in git.source_control_diff(&base, &requested_tip)? { result.push(json!({"label":"Pushed changes vs merge base","before":base,"after":requested_tip,"file":file})); }
            } else {
                for sha in &commits {
                    let own_merge = comparison.parent_merges.contains(sha) && available.iter().any(|c| &c.sha == sha);
                    let patch_tip = if fork.as_ref() == Some(sha) { sha.as_str() } else { requested_tip.as_str() };
                    let commit = git.source_control_commit(patch_tip, sha)?;
                    let label = if own_merge {"Merged from parent · own changes".to_owned()} else {format!("{}{}", sha, if commit.parents.len() > 1 {" · merge: first parent"} else {""})};
                    for file in if own_merge {git.source_control_remerge_diff(&requested_tip, sha)?} else {git.source_control_commit_diff(patch_tip, sha)?} {
                        result.push(json!({"label":label,"before":if own_merge {json!({"kind":"remerge"})}else{commit.parents.first().map(|p| json!(p)).unwrap_or(json!({"kind":"emptyTree"}))},"after":sha,"file":file}));
                    }
                }
            }
            let own_merge = !commits.is_empty() && commits.iter().all(|sha| comparison.parent_merges.contains(sha));
            Ok((all, result, own_merge))
        }).await.map_err(|e| conflict(e.to_string()))?
        };
        let (all, committed, own_merge) = match planned {
            Ok(planned) => planned,
            Err(G5Error::StaleHistory) => {
                let selected = history_selections
                    .iter()
                    .map(|(repo_id, _, commits)| (*repo_id, commits.clone()))
                    .collect();
                return refresh_for_selection(&state, &auth, id, selected).await;
            }
            Err(error) => return Err(error.into()),
        };
        own_merge_selection &= own_merge;
        let mut result = committed;
        if selected.include_unpushed {
            if repo.live.is_none() {
                return Err(conflict("Unpushed changes require the running owner"));
            }
            let before = if all {
                selected
                    .base
                    .as_deref()
                    .ok_or_else(|| conflict("Selected workspace base is unavailable"))?
            } else {
                selected.pushed.as_str()
            };
            let response = state
                .workspace_runtime
                .review_inspect(
                    id,
                    auth.require_user_id()?,
                    selected.repo_id,
                    "source_diff",
                    before,
                    None,
                )
                .await?;
            if response["status"] != "succeeded" {
                return Err(conflict("Live comparison unavailable"));
            }
            // A net diff may start at the merge base; local HEAD must still
            // contain the current pushed tip before its work can be stacked.
            let live = &response["result"];
            // Running guests retain their older runtime. Verify their read in
            // this request until they provide the native consistency receipt.
            if live["stateVerified"] != true {
                let after = state
                    .workspace_runtime
                    .review_inspect(
                        id,
                        auth.require_user_id()?,
                        selected.repo_id,
                        "source_state",
                        before,
                        None,
                    )
                    .await?;
                let fingerprint = live["fingerprint"]
                    .as_str()
                    .filter(|value| !value.is_empty());
                if after["status"] != "succeeded"
                    || fingerprint.is_none()
                    || after["result"]["fingerprint"].as_str().is_none()
                {
                    return Err(conflict("Live comparison unavailable"));
                }
                if after["result"]["fingerprint"].as_str() != fingerprint {
                    return Err(conflict(
                        "Files changed while reading this comparison; click Show again",
                    ));
                }
            }
            let descends_from_pushed = before == repo.pushed
                || live["headCommit"] == repo.pushed
                || live["localCommits"]
                    .as_array()
                    .is_some_and(|commits| commits.iter().any(|sha| sha == &repo.pushed));
            if live["selectable"] != true || !descends_from_pushed {
                return Err(conflict("Local HEAD does not descend from the pushed source; push or reconcile local history first"));
            }
            let text = response["result"]["patches"]
                .as_array()
                .ok_or_else(|| conflict("Live patch unavailable"))?
                .iter()
                .map(|p| p.as_str().ok_or_else(|| conflict("Invalid live patch")))
                .collect::<Result<Vec<_>, _>>()?
                .join("");
            let files = g5_scm::source_control_patch(text.as_bytes())?;
            if all {
                result.clear();
            }
            for file in files {
                result.push(json!({"label":if all {"All changes vs merge base"}else{"Not yet pushed"},"before":before,"after":null,"file":file}));
            }
        }
        for mut patch in result {
            patch["repoId"] = json!(repo.repo_id);
            patch["repoName"] = json!(repo.name);
            patches.push(patch);
        }
        if patches.len() > 2000 {
            return Err(conflict(
                "Selection exceeds 2,000 file patches; narrow the selection",
            ));
        }
    }
    let after = capture(&state, &auth, id, false).await?;
    if current.fingerprint != after.fingerprint {
        let selected = history_selections
            .into_iter()
            .map(|(repo_id, _, commits)| (repo_id, commits))
            .collect();
        return refresh_for_selection(&state, &auth, id, selected).await;
    }
    // Renames union file identities across selected parent patches, so an
    // earlier edit and a later rename remain one tree row/count.
    let mut identities = BTreeMap::<(String, String), String>::new();
    for patch in &patches {
        if patch["file"]["change"] == "renamed" {
            let repo = patch["repoId"].as_str().unwrap_or_default().to_owned();
            let path = patch["file"]["path"].as_str().unwrap_or_default();
            if let Some(old) = patch["file"]["oldPath"].as_str() {
                let from = canonical_path(&identities, &repo, old);
                let to = canonical_path(&identities, &repo, path);
                if from != to {
                    identities.insert((repo, from), to);
                }
            }
        }
    }
    let roles = current
        .repos
        .iter()
        .map(|repo| (repo.repo_id.to_string(), repo.role.as_str()))
        .collect::<BTreeMap<_, _>>();
    for patch in &mut patches {
        let repo = patch["repoId"].as_str().unwrap_or_default();
        let path = canonical_path(
            &identities,
            repo,
            patch["file"]["path"].as_str().unwrap_or_default(),
        );
        let kind = g5_contracts::g5_paths::change_file_kind(
            &path,
            roles.get(repo).copied().unwrap_or_default(),
        );
        patch["fileKey"] = json!(path);
        patch["kind"] = json!(kind);
    }
    let count = patches
        .iter()
        .map(|p| (p["repoId"].to_string(), p["fileKey"].to_string()))
        .collect::<BTreeSet<_>>()
        .len();
    let mut selected_commits = BTreeMap::new();
    for (repo_id, tip, commits) in history_selections {
        let repo = current
            .repos
            .iter()
            .find(|r| r.repo_id == repo_id)
            .ok_or_else(|| conflict("Repository unavailable"))?;
        let extra = commits
            .into_iter()
            .filter(|sha| !repo.commits.iter().any(|c| &c.sha == sha))
            .collect::<Vec<_>>();
        if extra.is_empty() {
            continue;
        }
        let path = current
            .storage
            .repo_path(current.org_id, repo_id)
            .map_err(|_| conflict("Repository unavailable"))?;
        let fork = repo.workspace_base.clone();
        let metadata =
            tokio::task::spawn_blocking(move || -> Result<Vec<g5_scm::SourceCommit>, G5Error> {
                let git = g5_scm::GitRepo::open(&path)?;
                extra
                    .into_iter()
                    .map(|sha| {
                        git.source_control_commit(
                            if fork.as_ref() == Some(&sha) {
                                &sha
                            } else {
                                &tip
                            },
                            &sha,
                        )
                    })
                    .collect()
            })
            .await
            .map_err(|e| conflict(e.to_string()))??;
        for commit in metadata {
            selected_commits.insert(format!("{repo_id}:{}", commit.sha), commit);
        }
    }
    let output = json!({"patches":patches,"fileCount":count,"private":private,"emptyOwnChanges":own_merge_selection && count==0,"selectedCommits":selected_commits});
    if serde_json::to_vec(&output)
        .map_err(|e| conflict(e.to_string()))?
        .len()
        > 16 * 1024 * 1024
    {
        return Err(conflict(
            "Selection exceeds the 16 MiB diff limit; narrow the selection",
        ));
    }
    Ok(Json(ResponseEnvelope::ok(output)))
}
async fn refresh_for_selection(
    state: &Arc<AppState>,
    auth: &AuthContext,
    id: Uuid,
    selected: BTreeMap<Uuid, Vec<String>>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let (menu, missing) = refreshed_menu(state, auth, id, selected).await?;
    Ok(Json(ResponseEnvelope::ok(
        json!({"refreshed":true,"menu":menu,"missingCommits":missing}),
    )))
}

fn canonical_path(
    identities: &BTreeMap<(String, String), String>,
    repo: &str,
    path: &str,
) -> String {
    let mut current = path.to_owned();
    for _ in 0..identities.len() {
        let Some(next) = identities.get(&(repo.to_owned(), current.clone())) else {
            break;
        };
        current = next.clone();
    }
    current
}

/// Shared completion marker. Only explicit Save callers invoke this; ordinary
/// Git publication continues without moving the checklist checkpoint.
pub(super) async fn finish_save(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    id: Uuid,
    actor: Uuid,
    incarnation: (Uuid, u64),
) -> Result<Value, AppError> {
    crate::services::workspace_mutation_policy::ensure_workspace_owner_is(&mut db, id, actor)
        .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::DirectAgentWrite,
        "source_control.save",
    )
    .await?;
    let org = db.primary_org_id()?.into_uuid();
    if crate::services::workspace_review_service::find_review_by_review_workspace(&mut db, org, id)
        .await?
        .is_some()
    {
        return Err(conflict("A review copy is saved by Push review"));
    }
    db.rollback().await?;
    state
        .workspace_runtime
        .save_source_control(id, actor, incarnation)
        .await?;
    Ok(json!({"saved":true}))
}
async fn save(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, id, &["workspaces:read", "workspaces:write"])
        .await?;
    let actor = auth.require_user_id()?;
    let handle = state
        .workspace_runtime
        .get(id)
        .await?
        .filter(|h| h.status == "running" && h.owner_user_id == actor)
        .ok_or(G5Error::WorkspaceNotRunning)?;
    finish_save(&state, db, id, actor, (handle.vm_id, handle.epoch)).await?;
    let mut db =
        crate::db::scoped::scoped_db_for_workspace_update(&state, &auth, id, &["workspaces:write"])
            .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner_is(&mut db, id, actor)
        .await?;
    let result = publish_checkpoint(&state, db, id).await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

pub(super) async fn publish_checkpoint(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    id: Uuid,
) -> Result<Value, AppError> {
    let generation = state
        .workspace_service
        .advance_source_control_generation(&mut db, id)
        .await?;
    db.commit().await?;
    state.event_bus.publish(
        &crate::ws::Channel::Workspace(id),
        &crate::ws::events::WsEvent::WorkspaceSourceControlSaved {
            workspace_id: id,
            generation,
        },
    );
    Ok(json!({"saved":true,"generation":generation}))
}
async fn checkpoint(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, id, &["workspaces:read"]).await?;
    let generation = state
        .workspace_service
        .source_control_generation(&mut db, id)
        .await?;
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(json!({"generation":generation}))))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PushedScope {
    #[serde(default)]
    heads: BTreeMap<Uuid, String>,
}
async fn pushed_scope(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
    Json(input): Json<PushedScope>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let current = capture(&state, &auth, id, true).await?;
    if !input.heads.is_empty()
        && (input.heads.len() != current.repos.len() || input.heads.len() > 100)
    {
        return Err(conflict(
            "The counted repository scope is no longer available",
        ));
    }
    let represented_menu = serde_json::to_value(&current).map_err(|e| conflict(e.to_string()))?;
    let mut repos = Vec::new();
    for repo in &current.repos {
        if repo.error.is_some() || repo.base.is_none() {
            continue;
        }
        let pushed = repo.pushed.clone();
        repos.push(Selection {
            repo_id: repo.repo_id,
            base: Some(
                repo.base
                    .clone()
                    .ok_or_else(|| conflict("Immutable workspace base unavailable"))?,
            ),
            pushed,
            commits: Vec::new(),
            include_unpushed: false,
            _live_fingerprint: None,
            all_pushed: true,
        });
    }
    if repos.is_empty() {
        return Err(conflict("No repository comparison is available"));
    }
    let Json(mut response) = apply(
        State(state),
        auth,
        Path(id),
        Json(Apply {
            repos,
            fingerprint: Some(current.fingerprint.clone()),
        }),
    )
    .await?;
    if response.data["refreshed"] != true {
        response.data["menu"] = represented_menu;
    }
    Ok(Json(response))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryRequest {
    repo_id: Uuid,
    pushed: String,
    parent: Option<String>,
    #[serde(default)]
    base: Option<String>,
    workspace_base: Option<String>,
    section: String,
    cursor: Option<String>,
}
async fn history_page(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
    Json(input): Json<HistoryRequest>,
) -> Result<Json<ResponseEnvelope<g5_scm::ComparisonHistoryPage>>, AppError> {
    let current = capture(&state, &auth, id, false).await?;
    let repo = current
        .repos
        .iter()
        .find(|r| r.repo_id == input.repo_id)
        .ok_or_else(|| conflict("Repository unavailable"))?;
    if repo.workspace_base != input.workspace_base {
        return Err(conflict("Workspace-base evidence changed"));
    }
    let mut db = scoped_db_for_workspace(&state, &auth, id, &["workspaces:read"]).await?;
    if repo.pushed != input.pushed
        && !history::recorded_head(db.conn(), id, input.repo_id, &input.pushed).await?
    {
        return Err(conflict("History source cut is unavailable"));
    }
    // Current history pages depend on the source and unique merge base, not
    // unrelated target advances. The supplied parent is only a legacy hint.
    let current_cut = input.pushed == repo.pushed && input.base == repo.base;
    let parent_cut = if current_cut {
        repo.base.clone()
    } else {
        input.parent
    };
    if !current_cut && repo.parent != parent_cut {
        let parent_id = history::readable_comparison_parent(
            &mut db,
            &state.system_pool,
            id,
            auth.require_user_id()?,
        )
        .await?
        .ok_or_else(|| conflict("Parent comparison is unavailable"))?;
        let parent = parent_cut
            .as_deref()
            .ok_or_else(|| conflict("Parent comparison cut unavailable"))?;
        if !history::recorded_head(db.conn(), parent_id, input.repo_id, parent).await? {
            return Err(conflict("History parent cut is unavailable"));
        }
    }
    db.rollback().await?;
    let path = current
        .storage
        .repo_path(current.org_id, input.repo_id)
        .map_err(|_| conflict("Repository unavailable"))?;
    let page = tokio::task::spawn_blocking(move || {
        g5_scm::GitRepo::open(&path)?.comparison_history(
            &input.pushed,
            parent_cut.as_deref(),
            input.workspace_base.as_deref(),
            &input.section,
            input.cursor.as_deref(),
            50,
        )
    })
    .await
    .map_err(|e| conflict(e.to_string()))??;
    Ok(Json(ResponseEnvelope::ok(page)))
}

fn capture_comparison(
    path: &std::path::Path,
    source: &str,
    parent: &str,
    fork: Option<&str>,
    limit: usize,
    previous: Option<&g5_scm::PushedComparison>,
) -> Result<g5_scm::PushedComparison, G5Error> {
    let git = g5_scm::GitRepo::open(path)?;
    if let Some(previous) = previous {
        if let Some(comparison) =
            git.reuse_pushed_comparison(source, parent, fork, limit, previous)?
        {
            return Ok(comparison);
        }
    }
    git.pushed_comparison(source, parent, fork, limit)
}
#[cfg(test)]
mod comparison_parity {
    use super::*;
    #[test]
    fn source_control_capture_matches_the_shared_git_matrix() {
        let fixture = g5_scm::comparison_fixture::comparison_fixture();
        for case in &fixture.scenarios {
            let source = &fixture.heads[case.source];
            let parent = &fixture.heads[case.parent];
            let fork = &fixture.heads[case.base];
            let captured = capture_comparison(
                fixture.directory.path(),
                source,
                parent,
                Some(fork),
                250,
                None,
            );
            let oracle = g5_scm::GitRepo::open(fixture.directory.path())
                .unwrap()
                .pushed_comparison(source, parent, Some(fork), 250);
            match (captured, oracle) {
                (Ok(actual), Ok(expected)) => {
                    let repeated = capture_comparison(
                        fixture.directory.path(),
                        source,
                        parent,
                        Some(fork),
                        250,
                        Some(&actual),
                    )
                    .unwrap();
                    let expected = serde_json::to_value(expected).unwrap();
                    assert_eq!(
                        serde_json::to_value(&actual).unwrap(),
                        expected,
                        "{}",
                        case.name
                    );
                    assert_eq!(
                        serde_json::to_value(repeated).unwrap(),
                        expected,
                        "{} reused",
                        case.name
                    );
                }
                (Err(_), Err(_)) => (),
                _ => panic!("{} comparison parity failed", case.name),
            }
        }
    }

    #[test]
    fn capture_rebuilds_comparison_when_the_captured_cuts_change() {
        let fixture = g5_scm::comparison_fixture::comparison_fixture();
        let old = capture_comparison(
            fixture.directory.path(),
            &fixture.heads["X2"],
            &fixture.heads["B"],
            Some(&fixture.heads["B"]),
            250,
            None,
        )
        .unwrap();
        let changed = capture_comparison(
            fixture.directory.path(),
            &fixture.heads["R2"],
            &fixture.heads["D"],
            Some(&fixture.heads["B"]),
            250,
            Some(&old),
        )
        .unwrap();
        assert_eq!(changed.merge_base, fixture.heads["D"]);
        assert_eq!(
            changed
                .commits
                .iter()
                .map(|c| c.summary.as_str())
                .collect::<Vec<_>>(),
            ["R2", "R1"]
        );
    }
}
