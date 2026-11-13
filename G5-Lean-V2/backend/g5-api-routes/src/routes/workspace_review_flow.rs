//! The review surface selects named, authorized published cuts. It never accepts
//! arbitrary Git refs and never starts a workspace to serve a durable read.
use crate::services::git::{
    workspace_reader::{CommittedRepoReadView, MAX_FILE_READ_BYTES},
    workspace_repositories::{load_workspace_repositories, readable_parent_workspace},
    workspace_revision_history as history,
};
use crate::services::workspace::workspace_review_queries as queries;
use crate::{
    db::scoped::scoped_db_for_workspace,
    errors::AppError,
    middleware::AuthContext,
    routes::{audited::AuditedRouter, ResponseEnvelope},
    services::AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use g5_api_core::services::git::repo_store::ScopedRepoStorage;
use g5_auth::Scope;
use g5_contracts::{
    error::G5Error,
    types::{HistoricalGitVersion, HistoricalReadReason, MergeSourceHead},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(overview), &[Scope::WorkspacesRead])
        .audited_route("/history", get(revisions), &[Scope::WorkspacesRead])
        .audited_route("/commits", get(commits), &[Scope::WorkspacesRead])
        .audited_route("/changes", get(changes), &[Scope::WorkspacesRead])
        .into_router()
}
#[derive(Deserialize)]
pub struct ReviewPath {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewQuery {
    pub mode: Option<String>,
    pub repo_id: Option<Uuid>,
    pub path: Option<String>,
    pub definition_id: Option<Uuid>,
    pub phase: Option<String>,
    pub revision_id: Option<Uuid>,
    pub before: Option<String>,
    pub comment_id: Option<Uuid>,
    pub pull_request_id: Option<Uuid>,
}
fn invalid(message: &str) -> AppError {
    G5Error::ValidationError {
        details: json!(message),
    }
    .into()
}
/// A recorded head's committed view; its string repository id is parsed here,
/// and the view is keyed for the workspace's organization within `storage`'s
/// scope (`g5.repo.storage-tenancy`).
fn view(
    storage: &ScopedRepoStorage,
    org_id: Uuid,
    head: &MergeSourceHead,
) -> Result<CommittedRepoReadView, AppError> {
    Ok(CommittedRepoReadView::from_historical(
        storage,
        org_id,
        &HistoricalGitVersion {
            repo_id: Uuid::parse_str(&head.repo_id)
                .map_err(|_| invalid("Invalid recorded repository"))?,
            head_commit: head.head_commit.clone(),
            reason: HistoricalReadReason::Diff,
        },
    )?)
}
// Review selections use the same committed-file ceiling as the Files reader.
// Inspect the immutable object's size before materializing either comparison side.
async fn review_file(
    view: Option<&CommittedRepoReadView>,
    state: &AppState,
    path: &str,
) -> Result<Option<Vec<u8>>, AppError> {
    let Some(view) = view else { return Ok(None) };
    let paths = [path.to_string()];
    let metadata = view.file_metadata(&state.repo_storage, &paths).await?;
    let Some((_hash, size)) = metadata.get(path).and_then(Option::as_ref) else {
        return Ok(None);
    };
    if *size > MAX_FILE_READ_BYTES {
        return Err(G5Error::FileTooLarge {
            path: path.to_string(),
            size: *size as u64,
            max_bytes: MAX_FILE_READ_BYTES as u64,
        }
        .into());
    }
    Ok(view
        .read_files(&state.repo_storage, &paths)
        .await?
        .remove(path)
        .flatten())
}
async fn lock_cut(
    db: &mut g5_authz::ScopedDb,
    workspace_id: Uuid,
    actor_id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    let parent = match readable_parent_workspace(db.conn(), workspace_id, actor_id).await {
        Ok(id) => Some(id),
        Err(G5Error::DiffGraphUnavailable { .. }) => None,
        Err(e) => return Err(e.into()),
    };
    let mut ids = vec![workspace_id];
    if let Some(id) = parent {
        ids.push(id);
    }
    ids.sort();
    ids.dedup();
    for id in ids {
        crate::services::branch_commit::acquire_workspace_read_fence(db.conn(), id).await?;
    }
    Ok(parent)
}
async fn overview(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(p): Path<ReviewPath>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, p.workspace_id, &["workspaces:read"]).await?;
    let parent_id = lock_cut(&mut db, p.workspace_id, auth.require_user_id()?).await?;
    let heads = history::published_heads(&mut db, &state.repo_storage, p.workspace_id).await?;
    let base = history::saved_base(db.conn(), p.workspace_id).await?;
    let members = load_workspace_repositories(db.conn(), p.workspace_id, false).await?;
    let queries::ReviewRecords {
        workspace,
        pr,
        requests,
        decisions,
        latest,
    } = queries::overview(db.conn(), p.workspace_id).await?;
    let pushed_at = latest
        .filter(|r| {
            serde_json::from_value::<Vec<MergeSourceHead>>(r.heads.clone())
                .is_ok_and(|h| history::same_heads(&h, &heads))
        })
        .map(|r| r.pushed_at)
        .unwrap_or(json!({}));
    let parent = if let Some(id) = parent_id {
        Some(history::published_heads(&mut db, &state.repo_storage, id).await?)
    } else {
        None
    };
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(
        json!({"workspace":workspace,"pr":pr,"heads":heads,"pushedAt":pushed_at,"baseHeads":base,"parentHeads":parent,
        "repos":members.iter().map(|m|json!({"id":m.repo_id,"name":m.repo_name,"role":m.role,"rootPath":m.root_path})).collect::<Vec<_>>(),"requests":requests,"decisions":decisions}),
    )))
}

async fn revisions(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(p): Path<ReviewPath>,
    Query(q): Query<ReviewQuery>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, p.workspace_id, &["workspaces:read"]).await?;
    let before = q
        .before
        .as_deref()
        .map(|s| {
            s.split_once('|')
                .ok_or_else(|| invalid("Invalid history cursor"))
        })
        .transpose()?;
    let before_at = before
        .map(|(at, _)| {
            chrono::DateTime::parse_from_rfc3339(at).map(|t| t.with_timezone(&chrono::Utc))
        })
        .transpose()
        .map_err(|_| invalid("Invalid history time"))?;
    let before_id = before
        .map(|(_, id)| Uuid::parse_str(id))
        .transpose()
        .map_err(|_| invalid("Invalid history identity"))?;
    // Parent history is admitted through current read access. Its rows stop at
    // the child's immutable fork capture; no future parent movement leaks in.
    let rows = queries::revisions(
        db.conn(),
        p.workspace_id,
        auth.require_user_id()?,
        before_at,
        before_id,
    )
    .await?;
    let more = rows.len() > 25;
    let rows = rows.into_iter().take(25).collect::<Vec<_>>();
    let next = if more {
        rows.last()
            .map(|r| format!("{}|{}", r.observed_at.to_rfc3339(), r.id))
    } else {
        None
    };
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(
        json!({"rows":rows,"nextCursor":next}),
    )))
}

async fn changes(
    State(state): State<Arc<AppState>>,
    user: crate::middleware::AuthenticatedUser,
    auth: AuthContext,
    Path(p): Path<ReviewPath>,
    Query(q): Query<ReviewQuery>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, p.workspace_id, &["workspaces:read"]).await?;
    let storage = state.repo_storage.scoped(&db);
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let mode = q.mode.as_deref().unwrap_or("base");
    let parent_id = if mode == "base" {
        None
    } else {
        lock_cut(&mut db, p.workspace_id, auth.require_user_id()?).await?
    };
    let base_cuts = if mode == "base" {
        Some(
            history::comparison_cuts(
                &mut db,
                &state.system_pool,
                &state.repo_storage,
                p.workspace_id,
                auth.require_user_id()?,
                q.pull_request_id,
            )
            .await?,
        )
    } else {
        None
    };
    let mut after = if let Some((source, _)) = &base_cuts {
        source.clone()
    } else {
        history::published_heads(&mut db, &state.repo_storage, p.workspace_id).await?
    };
    if q.path.is_some() != q.repo_id.is_some() {
        return Err(invalid("Select both a repository and a path"));
    }
    let comment_anchor: Option<(Uuid, String, String)> = if let Some(id) = q.comment_id {
        let c = queries::line_anchor(db.conn(), p.workspace_id, id).await?;
        let (repo, path, sha) =
            c.ok_or_else(|| invalid("Comment anchor is unavailable in this workspace"))?;
        if q.repo_id != Some(repo) || q.path.as_deref() != Some(path.as_str()) {
            return Err(invalid("Comment does not match the selected file"));
        }
        let sha = sha.ok_or_else(|| {
            invalid("This comment was left on unpushed work and has no historical pushed anchor")
        })?;
        let captured = queries::anchor_heads(db.conn(), p.workspace_id, repo, &sha).await?;
        if let Some(v) = captured {
            after = serde_json::from_value(v).map_err(|_| invalid("Invalid recorded vector"))?;
        } else if !after
            .iter()
            .any(|h| h.repo_id == repo.to_string() && h.head_commit == sha)
        {
            return Err(invalid(
                "The comment's pushed revision is no longer available",
            ));
        }
        Some((repo, path, sha))
    } else {
        None
    };
    let running_owner = state
        .workspace_runtime
        .get(p.workspace_id)
        .await?
        .is_some_and(|h| {
            h.status == "running" && h.owner_user_id == auth.require_user_id().unwrap_or_default()
        });
    if matches!(mode, "uncommitted" | "unpushed") && !running_owner {
        return Err(G5Error::Forbidden {
            missing_scopes: vec!["running_workspace_owner".into()],
        }
        .into());
    }
    if comment_anchor.is_none()
        && running_owner
        && matches!(mode, "all" | "uncommitted" | "unpushed")
    {
        let members = load_workspace_repositories(db.conn(), p.workspace_id, false).await?;
        db.rollback().await?;
        let mut files = Vec::new();
        let mut selected_file = None;
        let mut before_heads = Vec::new();
        for head in &after {
            let repo_id =
                Uuid::parse_str(&head.repo_id).map_err(|_| invalid("Invalid repository"))?;
            if q.repo_id.is_some_and(|id| id != repo_id) {
                continue;
            }
            let safe_path = q
                .path
                .as_deref()
                .map(crate::routes::files::sanitize_path)
                .transpose()?;
            let result = state
                .workspace_runtime
                .review_inspect(
                    p.workspace_id,
                    auth.require_user_id()?,
                    repo_id,
                    mode,
                    &head.head_commit,
                    safe_path.as_deref(),
                )
                .await?;
            if result["status"] != "succeeded" {
                return Err(G5Error::Conflict {
                    reason:
                        "Current workspace comparison is unavailable; check its branch association"
                            .into(),
                }
                .into());
            }
            let result = &result["result"];
            let mut before_head = head.clone();
            before_head.head_commit = result["beforeCommit"]
                .as_str()
                .ok_or_else(|| invalid("Workspace did not return its comparison revision"))?
                .to_owned();
            before_heads.push(before_head);
            if let Some(path) = safe_path {
                let current = crate::routes::workspaces::read_workspace_file_for_actor(
                    &state,
                    &user,
                    &auth,
                    p.workspace_id,
                    repo_id,
                    &path,
                )
                .await;
                let mut binary = result["binary"].as_bool().unwrap_or(false);
                let content = match current {
                    Ok(file) if file.source == "live" && file.encoding == "utf-8" => {
                        binary |= file.content.contains('\0');
                        Some(file.content)
                    }
                    Ok(file) if file.source == "live" => {
                        binary = true;
                        None
                    }
                    Ok(_) => {
                        return Err(G5Error::Conflict {
                            reason: "Workspace changed while reading; reload the comparison".into(),
                        }
                        .into())
                    }
                    Err(e) if matches!(e.inner, G5Error::NotFound { .. }) => None,
                    Err(e) => return Err(e),
                };
                selected_file = Some(
                    json!({"repoId":repo_id,"path":path,"before":if mode=="all" {Value::Null}else{result["before"].clone()},"after":content,"beforeCommit":result["beforeCommit"],"afterCommit":null,"binary":binary}),
                );
            } else {
                let member = members.iter().find(|m| m.repo_id == repo_id);
                for path in result["paths"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    files.push(json!({"repoId":repo_id,"path":path,"status":if mode=="all" {"unchanged"} else {"modified"},"additions":0,"deletions":0,"kind":g5_contracts::g5_paths::change_file_kind(path, member.map(|m|m.role.as_str()).unwrap_or_default())}));
                }
            }
        }
        return Ok(Json(ResponseEnvelope::ok(
            json!({"beforeHeads":before_heads,"afterHeads":after,"files":files,"file":selected_file,"baselineAvailable":true}),
        )));
    }
    let mut unavailable = Vec::<Value>::new();
    let before = match mode {
        "all" => None,
        "base" => {
            let parent = base_cuts.as_ref().and_then(|(_, parent)| parent.as_ref());
            let mut bases = Vec::new();
            for source in &after {
                let parent = parent
                    .as_ref()
                    .and_then(|hs| hs.iter().find(|h| h.repo_id == source.repo_id));
                let Some(parent) = parent else {
                    unavailable.push(
                        json!({"repoId":source.repo_id,"reason":"Parent comparison unavailable"}),
                    );
                    continue;
                };
                let path = Uuid::parse_str(&source.repo_id)
                    .map_err(|_| invalid("Invalid recorded repository"))
                    .and_then(|repo_id| {
                        storage
                            .repo_path(org_id, repo_id)
                            .map_err(AppError::from_error)
                    });
                let source_sha = source.head_commit.clone();
                let parent_sha = parent.head_commit.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let path = path.map_err(|_| G5Error::Conflict {
                        reason: "Repository storage unavailable".into(),
                    })?;
                    g5_scm::GitRepo::open(&path)?.pushed_comparison_base(&source_sha, &parent_sha)
                })
                .await
                .map_err(|e| invalid(&e.to_string()))?;
                match result {
                    Ok(merge_base) => {
                        let mut base = source.clone();
                        base.head_commit = merge_base;
                        bases.push(base);
                    }
                    Err(e) => {
                        unavailable.push(json!({"repoId":source.repo_id,"reason":e.to_string()}))
                    }
                }
            }
            if bases.is_empty() {
                None
            } else {
                Some(bases)
            }
        }
        "parent" => {
            let id = parent_id.ok_or_else(|| invalid("Parent comparison unavailable"))?;
            Some(history::published_heads(&mut db, &state.repo_storage, id).await?)
        }
        "last_review" => {
            let v = queries::checkpoint(
                db.conn(),
                p.workspace_id,
                auth.require_user_id()?,
                q.definition_id,
                q.phase.as_deref(),
            )
            .await?;
            v.map(serde_json::from_value)
                .transpose()
                .map_err(|_| invalid("Invalid review checkpoint"))?
        }
        "history" => {
            let selected = queries::revision(
                db.conn(),
                p.workspace_id,
                auth.require_user_id()?,
                q.revision_id,
            )
            .await?
            .ok_or_else(|| invalid("Revision is not in this workspace history"))?;
            let prior = queries::preceding(
                db.conn(),
                p.workspace_id,
                auth.require_user_id()?,
                &selected,
            )
            .await?;
            after =
                serde_json::from_value(selected.heads).map_err(|_| invalid("Invalid revision"))?;
            prior
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| invalid("Invalid preceding revision"))?
        }
        "uncommitted" | "unpushed" => {
            return Err(invalid(
                "Current comparisons require the owner's running workspace",
            ));
        }
        _ => return Err(invalid("Unknown file view")),
    };
    let members = load_workspace_repositories(db.conn(), p.workspace_id, false).await?;
    if mode == "base" {
        for member in &members {
            if !after
                .iter()
                .any(|h| h.repo_id == member.repo_id.to_string())
            {
                unavailable.push(json!({"repoId":member.repo_id,"repoName":member.repo_name,"reason":"Pushed source head is unavailable"}));
            }
        }
    }
    db.rollback().await?;
    let bases = before
        .as_ref()
        .map(|h| {
            h.iter()
                .map(|h| (h.repo_id.clone(), h))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut available_count = bases.len();
    let mut files = Vec::<Value>::new();
    let mut selected_file = None;
    let repo_ids = after
        .iter()
        .map(|h| h.repo_id.clone())
        .chain(bases.keys().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    for repo_id in repo_ids {
        if mode == "base" && !bases.contains_key(&repo_id) {
            continue;
        }
        let current = after.iter().find(|h| h.repo_id == repo_id);
        let h = current
            .or_else(|| bases.get(&repo_id).copied())
            .ok_or_else(|| invalid("Repository is not a member of the comparison"))?;
        let rid = Uuid::parse_str(&h.repo_id).map_err(|_| invalid("Invalid repository"))?;
        if q.repo_id.is_some_and(|id| id != rid) {
            continue;
        }
        let head_view = current
            .map(|head| view(&storage, org_id, head))
            .transpose()?;
        let base_view = bases
            .get(&h.repo_id)
            .map(|b| view(&storage, org_id, b))
            .transpose()?;
        if let Some(path) = &q.path {
            let path = crate::routes::files::sanitize_path(path)?;
            let read = review_file(head_view.as_ref(), &state, &path).await?;
            let old = review_file(base_view.as_ref(), &state, &path).await?;
            let binary = [old.as_ref(), read.as_ref()]
                .into_iter()
                .flatten()
                .any(|b| b.contains(&0) || std::str::from_utf8(b).is_err());
            let decode = |bytes: Option<&Vec<u8>>| -> Option<String> {
                bytes.and_then(|b| String::from_utf8(b.clone()).ok())
            };
            selected_file = Some(
                json!({"repoId":rid,"path":path,"before":decode(old.as_ref()),"after":decode(read.as_ref()),"binary":binary,"beforeCommit":bases.get(&h.repo_id).map(|b|&b.head_commit),"afterCommit":current.map(|h|&h.head_commit)}),
            );
        } else if mode == "all" {
            let Some(head_view) = head_view else {
                continue;
            };
            for entry in head_view
                .list_tree(&state.repo_storage, "", u32::MAX)
                .await?
            {
                let path = entry.path;
                let member = members.iter().find(|m| m.repo_id == rid);
                files.push(json!({"repoId":rid,"path":path,"status":"unchanged","additions":0,"deletions":0,"kind":g5_contracts::g5_paths::change_file_kind(&path, member.map(|m|m.role.as_str()).unwrap_or_default())}));
            }
        } else if let (Some(head_view), Some(v)) = (&head_view, &base_view) {
            let summary = if mode == "base" {
                match head_view
                    .source_control_summary_from(&state.repo_storage, v)
                    .await
                {
                    Ok(summary) => summary,
                    Err(e) => {
                        unavailable.push(json!({"repoId":h.repo_id,"reason":e.to_string()}));
                        available_count = available_count.saturating_sub(1);
                        continue;
                    }
                }
            } else {
                head_view.diff_summary_from(&state.repo_storage, v).await?
            };
            for f in summary.files {
                let member = members.iter().find(|m| m.repo_id == rid);
                files.push(json!({"repoId":rid,"path":f.path,"status":f.status,"additions":f.additions,"deletions":f.deletions,"kind":g5_contracts::g5_paths::change_file_kind(&f.path, member.map(|m|m.role.as_str()).unwrap_or_default())}));
            }
        } else if before.is_some() {
            let added = head_view.is_some();
            if let Some(v) = head_view.as_ref().or(base_view.as_ref()) {
                let entries = v.list_tree(&state.repo_storage, "", u32::MAX).await?;
                // Repository membership changes need only a path summary.
                // Load bytes for the selected file, never every blob at once.
                for entry in entries {
                    let path = entry.path;
                    let member = members.iter().find(|m| m.repo_id == rid);
                    files.push(json!({"repoId":rid,"path":path,"status":if added {"added"} else {"deleted"},"additions":null,"deletions":null,"kind":g5_contracts::g5_paths::change_file_kind(&path, member.map(|m|m.role.as_str()).unwrap_or_default())}));
                }
            }
        }
    }
    for error in &mut unavailable {
        if let Some(member) = members
            .iter()
            .find(|m| error["repoId"].as_str() == Some(m.repo_id.to_string().as_str()))
        {
            error["repoName"] = json!(member.repo_name);
        }
    }
    Ok(Json(ResponseEnvelope::ok(
        json!({"beforeHeads":before,"afterHeads":after,"files":files,"file":selected_file,"baselineAvailable":if mode=="base" {available_count>0} else {mode=="all" || before.is_some()},"unavailableRepositories":unavailable}),
    )))
}

/// Full Git history within a named workspace revision. The cursor must be on
/// that revision's first-parent ancestry; it cannot select another ref.
async fn commits(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(p): Path<ReviewPath>,
    Query(q): Query<ReviewQuery>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, p.workspace_id, &["workspaces:read"]).await?;
    let repo_id = q
        .repo_id
        .ok_or_else(|| invalid("Select a repository column"))?;
    let heads = if let Some(id) = q.revision_id {
        let revision =
            queries::revision(db.conn(), p.workspace_id, auth.require_user_id()?, Some(id))
                .await?
                .ok_or_else(|| invalid("Revision is not in this workspace history"))?;
        let value = revision.heads;
        serde_json::from_value::<Vec<MergeSourceHead>>(value)
            .map_err(|_| invalid("Invalid revision"))?
    } else {
        lock_cut(&mut db, p.workspace_id, auth.require_user_id()?).await?;
        history::published_heads(&mut db, &state.repo_storage, p.workspace_id).await?
    };
    let head = heads
        .iter()
        .find(|h| h.repo_id == repo_id.to_string())
        .ok_or_else(|| invalid("Repository is not a member of this revision"))?;
    let storage = state.repo_storage.scoped(&db);
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let head_view = view(&storage, org_id, head)?;
    let cursor = q
        .before
        .map(|sha| {
            head_view.historical_sibling(&HistoricalGitVersion {
                repo_id,
                head_commit: sha,
                reason: HistoricalReadReason::CommitHistory,
            })
        })
        .transpose()?;
    db.rollback().await?;
    let page = head_view
        .list_first_parent_history(&state.repo_storage, cursor.as_ref(), 25)
        .await?;
    Ok(Json(ResponseEnvelope::ok(
        json!({"headCommit":head.head_commit,"commits":page.commits,"nextCursor":page.next_cursor}),
    )))
}
