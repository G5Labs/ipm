//! Public user-facing HTTP endpoints for querying intent traceability.
//!
//! These endpoints are called by the frontend UI and use standard user
//! authentication (not worker JWT). They read grounding data from JSONL
//! files from the active live workspace file overlay first, then from committed git fallback.
//!
//! Routes:
//! - `GET /api/v1/grounding/trace-parents`
//! - `GET /api/v1/grounding/trace-children`

use g5_api_core::services::git::repo_store::TenantRepoStorage;
use std::collections::{hash_map::DefaultHasher, BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace_repo;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
#[cfg(test)]
use crate::services::workspace_reader::read_test_file_at_commit;
use crate::services::workspace_reader::{
    resolve_connected_binding_view, RepoFileRead, RepoReadView,
};
use crate::services::{AppState, WorkspaceRepoContext};
use g5_auth::Scope;
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::grounding::{FileType, GroundingEdge, IntentRecord};
#[cfg(test)]
use g5_scm::GitRepo;
use g5_scm::TreeEntryType;
use g5_semantic::block_extraction;
use g5_semantic::local_grounding::jsonl_store::encode_file_name;

// ─── Request / Response DTOs ────────────────────────────────────────────────

/// Frontend trace request — requires natural key fields.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceRequest {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub file_type: String,
    pub file_name: String,
    pub intent_record_number: String,
}

/// A single trace result row (metadata + content from JSONL).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceResult {
    pub intent_record_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub file_type: String,
    pub file_name: Option<String>,
    pub intent_record_number: Option<String>,
    pub code_block_name: Option<String>,
    pub content: Option<String>,
    /// Block title from canonical extraction (markdown records only).
    pub title: Option<String>,
    /// Normalized block label / code symbol from canonical extraction.
    pub block_label: Option<String>,
    /// Stored chat session identity; null for non-chat records.
    pub chat_session_id: Option<String>,
    /// Immutable stored chat context; null for non-chat records.
    pub chat_context: Option<String>,
    /// Verbatim stored user message; null for non-chat records.
    pub user_content: Option<String>,
    /// Original intent-record creator when stored.
    pub created_by: Option<String>,
    /// Original intent-record creation timestamp when stored.
    pub created_at: Option<String>,
    /// Existing physical code path accepted by the branch file API.
    pub source_file_path: Option<String>,
    /// True when a non-chat source or block cannot be resolved.
    pub stale: bool,
    pub is_human: bool,
    pub priority: Option<i16>,
    pub certainty: Option<i16>,
    pub relationship: Option<String>,
    pub relevance: Option<u8>,
}

/// Trace response envelope.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<String>,
    pub total_count: usize,
    pub results: Vec<TraceResult>,
}

struct CurrentGroundingRepo<'a> {
    context: &'a WorkspaceRepoContext,
    view: &'a RepoReadView,
}

enum TraceContentFallback {
    View(Box<RepoReadView>),
    #[cfg(test)]
    ExplicitCommit {
        git_repo: Arc<GitRepo>,
        commit: String,
    },
}

pub(crate) struct CapturedGroundingReadSet {
    pub current_repo: WorkspaceRepoContext,
    pub repo_contexts: Vec<WorkspaceRepoContext>,
    pub repo_views: BTreeMap<Uuid, RepoReadView>,
}

// ─── Router ─────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/trace-parents",
            get(trace_parents),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/trace-children",
            get(trace_children),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

/// Capture one complete exact-cut view for grounding and intent readers.
/// Repository slugs are auxiliary path metadata only; branch identity, commit
/// SHAs, and live overlays all come from the typed authority reader.
pub(crate) async fn capture_grounding_read_set(
    state: &AppState,
    db: &mut ScopedDb,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<CapturedGroundingReadSet, AppError> {
    let target = g5_contracts::types::AuthorityTarget::Workspace { workspace_id };
    let resolved = resolve_connected_binding_view(db, &state.repo_storage, target)
        .await
        .map_err(AppError::from_error)?;
    let repo_views = resolved
        .repos()
        .map(|(repo_id, view)| (*repo_id, view.clone()))
        .collect();
    let snapshot = resolved.view.snapshot;

    let repo_contexts = snapshot
        .repos
        .iter()
        .map(|repo| WorkspaceRepoContext {
            workspace_id: repo.workspace_id,
            repo_id: repo.repo_id,
            repo_name: repo.repo_name.clone(),
            repo_slug: repo.repo_slug.clone(),
            branch_name: repo.branch_name.clone(),
            commit: repo.head_commit.clone(),
        })
        .collect::<Vec<_>>();
    let current_repo = repo_contexts
        .iter()
        .find(|repo| repo.repo_id == repo_id && repo.workspace_id == workspace_id)
        .cloned()
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Requested branch is not a member of its typed authority cut",
                    "workspace_id": workspace_id,
                    "repo_id": repo_id,
                    "target": snapshot.target,
                }),
            })
        })?;

    Ok(CapturedGroundingReadSet {
        current_repo,
        repo_contexts,
        repo_views,
    })
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// Current-format tracing reports only explicit durable records. Every read/parse
/// failure is an error, never a successful empty relationship set.
struct TraceReadBudget {
    bytes: usize,
    files: usize,
}
impl Default for TraceReadBudget {
    fn default() -> Self {
        Self {
            bytes: crate::services::workspace_reader::MAX_FILE_READ_BYTES,
            files: 10_000,
        }
    }
}
async fn strict_trace_bytes(
    state: &AppState,
    view: &RepoReadView,
    path: &str,
    budget: &mut TraceReadBudget,
) -> Result<Option<Vec<u8>>, AppError> {
    if budget.files == 0 || budget.bytes == 0 {
        return Err(G5Error::Internal {
            message: "Grounding read limit exceeded".into(),
        }
        .into());
    }
    budget.files -= 1;
    match view
        .committed_view()
        .read_file_bounded(&state.repo_storage, path, budget.bytes)
        .await
    {
        Ok(bytes) => {
            budget.bytes = budget.bytes.saturating_sub(bytes.len());
            Ok(Some(bytes))
        }
        Err(G5Error::NotFound { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
async fn strict_trace_records<T: serde::de::DeserializeOwned>(
    state: &AppState,
    view: &RepoReadView,
    path: &str,
    budget: &mut TraceReadBudget,
) -> Result<Vec<T>, AppError> {
    let Some(bytes) = strict_trace_bytes(state, view, path, budget).await? else {
        return Ok(vec![]);
    };
    let text = String::from_utf8(bytes).map_err(|_| {
        AppError::from_error(G5Error::Internal {
            message: format!("Invalid UTF-8 in {path}"),
        })
    })?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).map_err(|_| {
                AppError::from_error(G5Error::Internal {
                    message: format!("Invalid grounding record in {path}"),
                })
            })
        })
        .collect()
}

async fn current_trace_result(
    state: &AppState,
    view: &RepoReadView,
    owner: &WorkspaceRepoContext,
    record: &IntentRecord,
    budget: &mut TraceReadBudget,
) -> Result<TraceResult, AppError> {
    let mut content =
        format_chat_content(record.chat_context.as_deref(), record.content.as_deref());
    let mut title = None;
    let mut block_label = None;
    let mut source_file_path = None;
    let mut stale = false;
    if record.file_type != FileType::Chat {
        let name = record.file_name.as_deref().unwrap_or("");
        let id = record
            .intent_record_number
            .as_deref()
            .or(record.code_block_name.as_deref())
            .unwrap_or("");
        let mut sources = vec![];
        for path in if record.file_type == FileType::Code && !name.starts_with("generation/") {
            vec![name.to_string(), format!("generation/{name}")]
        } else {
            vec![name.to_string()]
        } {
            if let Some(bytes) = strict_trace_bytes(state, view, &path, budget).await? {
                sources.push((path, bytes));
            }
        }
        if sources.len() == 1 {
            let (path, bytes) = sources.remove(0);
            if record.file_type == FileType::Code {
                source_file_path = Some(path);
            }
            let text = String::from_utf8(bytes).map_err(|_| {
                AppError::from_error(G5Error::Internal {
                    message: "Grounding source is not UTF-8".into(),
                })
            })?;
            match block_extraction::extract_block_from_text(record.file_type, name, id, &text) {
                Ok(block) => {
                    content = Some(block.content);
                    title = block.title;
                    block_label = block.label;
                }
                Err(_) => {
                    content = None;
                    stale = true;
                }
            }
        } else {
            content = None;
            stale = true;
        }
    }
    Ok(trace_result_fields(
        record,
        owner,
        content,
        title,
        block_label,
        source_file_path,
        stale,
    ))
}

async fn current_format_trace(
    state: &AppState,
    captured: &CapturedGroundingReadSet,
    query: &TraceRequest,
    parents: bool,
) -> Result<TraceResponse, AppError> {
    let mut budget = TraceReadBudget::default();
    let repo = &captured.current_repo;
    let view = captured.repo_views.get(&repo.repo_id).ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "Grounding repository is unavailable".into(),
        })
    })?;
    let records: Vec<IntentRecord> = strict_trace_records(
        state,
        view,
        &format!(
            "intent_record/ontology/{}.jsonl",
            encode_file_name(&query.file_name)
        ),
        &mut budget,
    )
    .await?;
    let matches: Vec<_> = records
        .iter()
        .filter(|r| {
            r.file_type == FileType::Ontology
                && r.file_name.as_deref() == Some(query.file_name.as_str())
                && r.intent_record_number.as_deref() == Some(&query.intent_record_number)
        })
        .collect();
    if matches.is_empty() {
        return Ok(TraceResponse {
            availability: Some("not_recorded".into()),
            total_count: 0,
            results: vec![],
        });
    }
    if matches.len() != 1 {
        return Err(AppError::from_error(G5Error::Internal {
            message: "Ambiguous explicit grounding record".into(),
        }));
    }
    let mut results = Vec::new();
    for edge_repo in &captured.repo_contexts {
        if parents && edge_repo.repo_id != repo.repo_id {
            continue;
        }
        let edge_view = captured.repo_views.get(&edge_repo.repo_id).ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: "Grounding repository is unavailable".into(),
            })
        })?;
        let paths = if parents {
            vec![format!(
                "intent_grounding/ontology/{}.jsonl",
                encode_file_name(&query.file_name)
            )]
        } else {
            match edge_view
                .list_tree_bounded(
                    &state.repo_storage,
                    "intent_grounding",
                    u32::MAX,
                    crate::services::workspace_reader::FILE_TREE_LIMITS,
                )
                .await
            {
                Ok(listing) if listing.truncated => {
                    return Err(AppError::from_error(G5Error::Internal {
                        message: "Grounding discovery exceeded the listing limit".into(),
                    }))
                }
                Ok(listing) => listing
                    .entries
                    .into_iter()
                    .filter(|e| {
                        matches!(e.entry_type, TreeEntryType::File) && e.path.ends_with(".jsonl")
                    })
                    .map(|e| e.path)
                    .collect(),
                Err(G5Error::NotFound { .. }) => vec![],
                Err(e) => return Err(e.into()),
            }
        };
        for path in paths {
            let edges: Vec<GroundingEdge> =
                strict_trace_records(state, edge_view, &path, &mut budget).await?;
            for edge in edges {
                let relevant = if parents {
                    edge.target_id == query.intent_record_number
                        && edge.target_file == query.file_name
                        && edge.target_file_type == FileType::Ontology
                } else {
                    edge.source_id == query.intent_record_number
                        && edge.source_file_type == FileType::Ontology
                        && repo_scoped_file_candidates(repo, &query.file_name)
                            .contains(&edge.source_file)
                };
                if !relevant {
                    continue;
                }
                // A bare source belongs to the ledger's repository, never an equal path in a sibling.
                if !parents
                    && edge.source_file == query.file_name
                    && edge_repo.repo_id != repo.repo_id
                {
                    continue;
                }
                let (kind, name, id) = if parents {
                    (edge.source_file_type, &edge.source_file, &edge.source_id)
                } else {
                    (edge.target_file_type, &edge.target_file, &edge.target_id)
                };
                let (owner, name) = if let Some((prefix, rest)) = name.split_once('/') {
                    let owners: Vec<_> = captured
                        .repo_contexts
                        .iter()
                        .filter(|r| r.repo_name == prefix || r.repo_slug == prefix)
                        .collect();
                    if owners.len() > 1 {
                        return Err(G5Error::Internal {
                            message: "Ambiguous grounding repository target".into(),
                        }
                        .into());
                    }
                    owners
                        .first()
                        .map(|r| (*r, rest))
                        .unwrap_or((edge_repo, name.as_str()))
                } else {
                    (edge_repo, name.as_str())
                };
                let owner_view = captured.repo_views.get(&owner.repo_id).ok_or_else(|| {
                    AppError::from_error(G5Error::Internal {
                        message: "Grounding target repository unavailable".into(),
                    })
                })?;
                let records: Vec<IntentRecord> = strict_trace_records(
                    state,
                    owner_view,
                    &format!(
                        "intent_record/{}/{}.jsonl",
                        kind.subfolder(),
                        encode_file_name(name)
                    ),
                    &mut budget,
                )
                .await?;
                let matches: Vec<_> = records
                    .iter()
                    .filter(|r| {
                        r.file_type == kind
                            && r.file_name.as_deref() == Some(name)
                            && (r.intent_record_number.as_deref() == Some(id)
                                || r.code_block_name.as_deref() == Some(id))
                    })
                    .collect();
                if matches.len() != 1 {
                    return Err(AppError::from_error(G5Error::Internal {
                        message: format!(
                            "Missing or ambiguous explicit grounding endpoint: {name}#{id}"
                        ),
                    }));
                }
                let mut result =
                    current_trace_result(state, owner_view, owner, matches[0], &mut budget).await?;
                result.relationship = edge.relationship;
                result.relevance = edge.relevance;
                results.push(result);
            }
        }
    }
    Ok(TraceResponse {
        availability: Some("recorded".into()),
        total_count: results.len(),
        results,
    })
}

/// `GET /api/v1/grounding/trace-parents`
///
/// Returns all parent intent records for the specified block by reading
/// the target's committed grounding JSONL file (edges are target-keyed).
async fn trace_parents(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<TraceRequest>,
) -> Result<Json<crate::routes::ResponseEnvelope<TraceResponse>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        query.workspace_id,
        query.repo_id,
        &["workspaces:read"],
    )
    .await?;

    let captured =
        capture_grounding_read_set(&state, &mut db, query.workspace_id, query.repo_id).await?;
    if query.file_type == "ontology" && query.file_name.starts_with(".g5/ontology/") {
        let response = current_format_trace(&state, &captured, &query, true).await?;
        return Ok(Json(crate::routes::ResponseEnvelope::ok(response)));
    }
    let current_repo = captured.current_repo;
    let repo_contexts = captured.repo_contexts;
    let repo_views = captured.repo_views;
    let content_fallbacks = repo_views
        .values()
        .cloned()
        .map(|view| TraceContentFallback::View(Box::new(view)))
        .collect::<Vec<_>>();
    let results = if let Some(trace_repo) =
        select_trace_repo(&query, &current_repo, &repo_contexts).cloned()
    {
        trace_parents_on_repo(
            &state,
            &trace_repo,
            &repo_contexts,
            &repo_views,
            &query,
            &content_fallbacks,
        )
        .await
    } else {
        Vec::new()
    };
    Ok(Json(crate::routes::ResponseEnvelope::ok(TraceResponse {
        availability: None,
        total_count: results.len(),
        results,
    })))
}

/// `GET /api/v1/grounding/trace-children`
///
/// Returns all child intent records for the specified block by scanning
/// all committed grounding JSONL files (edges are target-keyed, so
/// children are spread across multiple files).
async fn trace_children(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<TraceRequest>,
) -> Result<Json<crate::routes::ResponseEnvelope<TraceResponse>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        query.workspace_id,
        query.repo_id,
        &["workspaces:read"],
    )
    .await?;

    let captured =
        capture_grounding_read_set(&state, &mut db, query.workspace_id, query.repo_id).await?;
    if query.file_type == "ontology" && query.file_name.starts_with(".g5/ontology/") {
        let response = current_format_trace(&state, &captured, &query, false).await?;
        return Ok(Json(crate::routes::ResponseEnvelope::ok(response)));
    }
    let current_repo = captured.current_repo;
    let repo_contexts = captured.repo_contexts;
    let repo_views = captured.repo_views;
    let content_fallbacks = repo_views
        .values()
        .cloned()
        .map(|view| TraceContentFallback::View(Box::new(view)))
        .collect::<Vec<_>>();
    let results = if let Some(trace_repo) =
        select_trace_repo(&query, &current_repo, &repo_contexts).cloned()
    {
        trace_children_on_repo(
            &state,
            &trace_repo,
            &repo_contexts,
            &repo_views,
            &query,
            &content_fallbacks,
        )
        .await
    } else {
        Vec::new()
    };
    Ok(Json(crate::routes::ResponseEnvelope::ok(TraceResponse {
        availability: None,
        total_count: results.len(),
        results,
    })))
}

async fn trace_parents_on_repo(
    state: &AppState,
    repo: &WorkspaceRepoContext,
    repo_contexts: &[WorkspaceRepoContext],
    repo_views: &BTreeMap<Uuid, RepoReadView>,
    body: &TraceRequest,
    content_fallbacks: &[TraceContentFallback],
) -> Vec<TraceResult> {
    let Some(repo_view) = repo_views.get(&repo.repo_id) else {
        return Vec::new();
    };

    let file_type = FileType::from_str_loose(&body.file_type).unwrap_or(FileType::Code);
    let encoded = encode_file_name(&body.file_name);
    let jsonl_path = format!("intent_grounding/{}/{encoded}.jsonl", file_type.subfolder());

    let edges: Vec<GroundingEdge> =
        read_jsonl_from_view(&state.repo_storage, repo_view, &jsonl_path).await;
    let mut results = Vec::new();

    for edge in edges {
        if edge.target_id == body.intent_record_number {
            if let Some((record, record_repo)) = find_intent_record_resolving_scoped(
                state,
                CurrentGroundingRepo {
                    context: repo,
                    view: repo_view,
                },
                repo_contexts,
                repo_views,
                edge.source_file_type,
                &edge.source_file,
                &edge.source_id,
            )
            .await
            {
                let Some(record_view) = repo_views.get(&record_repo.repo_id) else {
                    continue;
                };
                let mut result = intent_record_to_trace_result_with_overlay_and_fallbacks(
                    &record,
                    &state.repo_storage,
                    record_view,
                    &record_repo,
                    content_fallbacks,
                )
                .await;
                result.relationship = edge.relationship.clone();
                result.relevance = edge.relevance;
                results.push(result);
            }
        }
    }
    results
}

/// Trace children on a single repo at a given commit. Returns empty vec on any
/// failure (missing repo path, git error, no grounding data).
async fn trace_children_on_repo(
    state: &AppState,
    repo: &WorkspaceRepoContext,
    repo_contexts: &[WorkspaceRepoContext],
    repo_views: &BTreeMap<Uuid, RepoReadView>,
    body: &TraceRequest,
    content_fallbacks: &[TraceContentFallback],
) -> Vec<TraceResult> {
    let source_names = repo_scoped_file_candidates(repo, &body.file_name);
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let mut source_owner_cache: HashMap<(Uuid, Uuid, String, String, String), Option<Uuid>> =
        HashMap::new();

    for candidate_repo in repo_contexts {
        let Some(candidate_view) = repo_views.get(&candidate_repo.repo_id) else {
            continue;
        };
        let grounding_files =
            list_grounding_jsonl_files_from_view(&state.repo_storage, candidate_view).await;
        let grounding_corpus: BTreeMap<String, Vec<GroundingEdge>> =
            read_jsonl_files_from_view(&state.repo_storage, candidate_view, &grounding_files)
                .await
                .unwrap_or_default();

        for edges in grounding_corpus.into_values() {
            for edge in edges {
                if !source_names.contains(&edge.source_file)
                    || edge.source_id != body.intent_record_number
                {
                    continue;
                }

                // Grounding is target-keyed, so this corpus may live in a
                // sibling repository. Resolve the edge's SOURCE record before
                // accepting it and require that its typed owner is the repo
                // selected by the trace request. A bare duplicate path in a
                // sibling repo is not the same source identity.
                let source_cache_key = (
                    candidate_repo.repo_id,
                    candidate_repo.workspace_id,
                    edge.source_file_type.to_string(),
                    edge.source_file.clone(),
                    edge.source_id.clone(),
                );
                let source_owner_repo_id =
                    if let Some(cached) = source_owner_cache.get(&source_cache_key) {
                        *cached
                    } else {
                        let resolved = find_intent_record_resolving_scoped(
                            state,
                            CurrentGroundingRepo {
                                context: candidate_repo,
                                view: candidate_view,
                            },
                            repo_contexts,
                            repo_views,
                            edge.source_file_type,
                            &edge.source_file,
                            &edge.source_id,
                        )
                        .await
                        .map(|(_source_record, source_repo)| source_repo.repo_id);
                        source_owner_cache.insert(source_cache_key, resolved);
                        resolved
                    };
                if source_owner_repo_id != Some(repo.repo_id) {
                    continue;
                }

                if let Some((record, record_repo)) = find_intent_record_resolving_scoped(
                    state,
                    CurrentGroundingRepo {
                        context: candidate_repo,
                        view: candidate_view,
                    },
                    repo_contexts,
                    repo_views,
                    edge.target_file_type,
                    &edge.target_file,
                    &edge.target_id,
                )
                .await
                {
                    let key = (
                        record_repo.repo_id,
                        record_repo.workspace_id,
                        record.file_type.to_string(),
                        record.file_name.clone(),
                        record.intent_record_number.clone(),
                        record.code_block_name.clone(),
                    );
                    if !seen.insert(key) {
                        continue;
                    }
                    let Some(record_view) = repo_views.get(&record_repo.repo_id) else {
                        continue;
                    };
                    let mut result = intent_record_to_trace_result_with_overlay_and_fallbacks(
                        &record,
                        &state.repo_storage,
                        record_view,
                        &record_repo,
                        content_fallbacks,
                    )
                    .await;
                    result.relationship = edge.relationship.clone();
                    result.relevance = edge.relevance;
                    results.push(result);
                }
            }
        }
    }
    results
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// List the effective grounding JSONL corpus from one captured repository
/// view. The committed tree and dirty membership belong to the same request
/// snapshot, so multi-file scans cannot mix authority cuts or persistence
/// generations.
pub(crate) async fn list_grounding_jsonl_files_from_view(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
) -> Vec<String> {
    let mut paths = match repo_view
        .list_committed_tree(repo_storage, "intent_grounding", 32)
        .await
    {
        Ok(entries) => entries
            .into_iter()
            .filter(|entry| {
                matches!(entry.entry_type, TreeEntryType::File)
                    && entry.path.starts_with("intent_grounding/")
                    && entry.path.ends_with(".jsonl")
            })
            .map(|entry| entry.path)
            .collect::<HashSet<_>>(),
        Err(G5Error::NotFound { .. }) => HashSet::new(),
        Err(error) => {
            warn!(
                repo_id = %repo_view.repo.repo_id,
                commit = %repo_view.repo.head_commit,
                error = %error,
                "Unable to list committed grounding corpus"
            );
            HashSet::new()
        }
    };

    let dirty_paths = repo_view
        .dirty_files()
        .iter()
        .filter(|path| path.starts_with("intent_grounding/") && path.ends_with(".jsonl"))
        .cloned()
        .collect::<Vec<_>>();
    match repo_view.read_files(repo_storage, &dirty_paths).await {
        Ok(effective) => {
            for path in dirty_paths {
                match effective.get(&path) {
                    Some(RepoFileRead::Content { .. }) => {
                        paths.insert(path);
                    }
                    Some(RepoFileRead::Deleted | RepoFileRead::Missing) => {
                        paths.remove(&path);
                    }
                    None => {}
                }
            }
        }
        Err(error) => {
            warn!(
                repo_id = %repo_view.repo.repo_id,
                commit = %repo_view.repo.head_commit,
                error = %error,
                "Unable to read captured grounding overlay corpus"
            );
        }
    }

    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Read and parse a JSONL corpus through one captured repository view.
///
/// All requested files share one exact committed Git member and one captured
/// live workspace file dirty-path set. Missing, deleted, or non-UTF-8 files contribute an
/// empty record list; an unreadable repository or commit fails the batch.
pub(crate) async fn read_jsonl_files_from_view<T: serde::de::DeserializeOwned>(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
    paths: &[String],
) -> Result<BTreeMap<String, Vec<T>>, G5Error> {
    let mut reads = repo_view.read_files(repo_storage, paths).await?;
    Ok(paths
        .iter()
        .cloned()
        .map(|path| {
            let records = match reads.remove(&path) {
                Some(RepoFileRead::Content { bytes, .. }) => String::from_utf8(bytes)
                    .ok()
                    .map(|content| parse_jsonl(&content, &path))
                    .unwrap_or_default(),
                Some(RepoFileRead::Deleted | RepoFileRead::Missing) | None => Vec::new(),
            };
            (path, records)
        })
        .collect())
}

pub(crate) async fn read_jsonl_from_view<T: serde::de::DeserializeOwned>(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
    path: &str,
) -> Vec<T> {
    read_jsonl_files_from_view(repo_storage, repo_view, &[path.to_string()])
        .await
        .ok()
        .and_then(|mut files| files.remove(path))
        .unwrap_or_default()
}

/// List all JSONL files under `intent_grounding/` at a specific commit.
///
/// Walks the `intent_grounding/` subfolders (views, ontology, code, external)
/// and collects all `.jsonl` file paths. Returns empty vec if the directory
/// doesn't exist at the commit (no grounding data yet).
#[cfg(test)]
pub(crate) async fn list_grounding_jsonl_files(
    git_repo: Arc<GitRepo>,
    commit: String,
) -> Vec<String> {
    tokio::task::spawn_blocking(move || {
        let subfolders = match git_repo.list_tree(&commit, "intent_grounding") {
            Ok(entries) => entries,
            Err(_) => return Vec::new(), // No grounding directory yet
        };

        let mut paths = Vec::new();
        for entry in &subfolders {
            if matches!(entry.entry_type, TreeEntryType::Directory) {
                if let Ok(files) = git_repo.list_tree(&commit, &entry.path) {
                    for file in files {
                        if matches!(file.entry_type, TreeEntryType::File)
                            && file.name.ends_with(".jsonl")
                        {
                            paths.push(file.path);
                        }
                    }
                }
            }
        }
        paths
    })
    .await
    .unwrap_or_default()
}

#[cfg(test)]
async fn read_text_at_commit(
    git_repo: Arc<GitRepo>,
    commit: String,
    path: String,
) -> Option<String> {
    let bytes = read_test_file_at_commit(git_repo, commit, path)
        .await
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn select_trace_repo<'a>(
    query: &TraceRequest,
    _current_repo: &'a WorkspaceRepoContext,
    repo_contexts: &'a [WorkspaceRepoContext],
) -> Option<&'a WorkspaceRepoContext> {
    repo_contexts
        .iter()
        .find(|repo| repo.repo_id == query.repo_id)
}

pub(crate) fn repo_scoped_file_candidates(
    repo: &WorkspaceRepoContext,
    file_name: &str,
) -> HashSet<String> {
    let mut candidates = HashSet::from([file_name.to_string()]);
    if !file_name.starts_with(&format!("{}/", repo.repo_name)) {
        candidates.insert(format!("{}/{}", repo.repo_name, file_name));
    }
    if repo.repo_slug != repo.repo_name && !file_name.starts_with(&format!("{}/", repo.repo_slug)) {
        candidates.insert(format!("{}/{}", repo.repo_slug, file_name));
    }
    candidates
}

fn split_repo_scoped_file<'a>(
    repo_contexts: &'a [WorkspaceRepoContext],
    file_name: &'a str,
) -> Option<(&'a WorkspaceRepoContext, &'a str)> {
    let (prefix, rest) = file_name.split_once('/')?;
    if rest.is_empty() {
        return None;
    }
    repo_contexts
        .iter()
        .find(|repo| repo.repo_name == prefix || repo.repo_slug == prefix)
        .map(|repo| (repo, rest))
}

/// Read and parse a JSONL file at a specific commit. Skips malformed lines.
/// Returns an empty vec if the file doesn't exist.
#[cfg(test)]
async fn read_jsonl_at_commit<T: serde::de::DeserializeOwned>(
    git_repo: Arc<GitRepo>,
    commit: &str,
    path: &str,
) -> Vec<T> {
    let Some(content) = read_text_at_commit(git_repo, commit.to_string(), path.to_string()).await
    else {
        return Vec::new();
    };

    parse_jsonl(&content, path)
}

fn parse_jsonl<T: serde::de::DeserializeOwned>(content: &str, path: &str) -> Vec<T> {
    let mut records = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<T>(trimmed) {
            Ok(record) => records.push(record),
            Err(e) => {
                warn!(path, error = %e, "Skipping malformed JSONL line in grounding file");
            }
        }
    }
    records
}

#[cfg(test)]
async fn find_intent_record(
    git_repo: Arc<GitRepo>,
    commit: &str,
    file_type: FileType,
    file_name: &str,
    identifier: &str,
) -> Option<IntentRecord> {
    let encoded = encode_file_name(file_name);
    let path = format!("intent_record/{}/{encoded}.jsonl", file_type.subfolder());

    let records: Vec<IntentRecord> = read_jsonl_at_commit(git_repo, commit, &path).await;
    records.into_iter().find(|r| {
        // Match by intent_record_number or code_block_name
        r.intent_record_number.as_deref() == Some(identifier)
            || r.code_block_name.as_deref() == Some(identifier)
    })
}

async fn find_intent_record_in_view(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
    file_type: FileType,
    file_name: &str,
    identifier: &str,
) -> Option<IntentRecord> {
    let encoded = encode_file_name(file_name);
    let path = format!("intent_record/{}/{encoded}.jsonl", file_type.subfolder());
    let records: Vec<IntentRecord> = read_jsonl_from_view(repo_storage, repo_view, &path).await;
    records.into_iter().find(|record| {
        record.intent_record_number.as_deref() == Some(identifier)
            || record.code_block_name.as_deref() == Some(identifier)
    })
}

async fn find_intent_record_resolving_scoped(
    state: &AppState,
    current: CurrentGroundingRepo<'_>,
    repo_contexts: &[WorkspaceRepoContext],
    repo_views: &BTreeMap<Uuid, RepoReadView>,
    file_type: FileType,
    file_name: &str,
    identifier: &str,
) -> Option<(IntentRecord, WorkspaceRepoContext)> {
    if let Some(record) = find_intent_record_in_view(
        &state.repo_storage,
        current.view,
        file_type,
        file_name,
        identifier,
    )
    .await
    {
        return Some((record, current.context.clone()));
    }

    if let Some((target_repo, repo_local_file)) = split_repo_scoped_file(repo_contexts, file_name) {
        if let Some(record) = find_intent_record_in_context(
            state,
            repo_views,
            target_repo,
            file_type,
            repo_local_file,
            identifier,
        )
        .await
        {
            return Some((record, target_repo.clone()));
        }
    }

    for candidate_repo in repo_contexts {
        if candidate_repo.repo_id == current.context.repo_id {
            continue;
        }

        if let Some(record) = find_intent_record_in_context(
            state,
            repo_views,
            candidate_repo,
            file_type,
            file_name,
            identifier,
        )
        .await
        {
            return Some((record, candidate_repo.clone()));
        }
    }

    None
}

async fn find_intent_record_in_context(
    state: &AppState,
    repo_views: &BTreeMap<Uuid, RepoReadView>,
    target_repo: &WorkspaceRepoContext,
    file_type: FileType,
    file_name: &str,
    identifier: &str,
) -> Option<IntentRecord> {
    let target_view = repo_views.get(&target_repo.repo_id)?;
    find_intent_record_in_view(
        &state.repo_storage,
        target_view,
        file_type,
        file_name,
        identifier,
    )
    .await
}

/// Map a JSONL `IntentRecord` to the response DTO.
///
/// When the record has no inline `content`, we extract the block text from the
/// actual file at the given commit. Canonical intent / decision blocks start
/// with a bare `[I##. …]`, `[D##x. …]`, or `[Q##x. …]` label line and continue
/// until the next block boundary.
#[cfg(test)]
async fn intent_record_to_trace_result(
    record: &IntentRecord,
    git_repo: Arc<GitRepo>,
    commit: &str,
    content_fallbacks: &[TraceContentFallback],
) -> TraceResult {
    let owner = WorkspaceRepoContext {
        workspace_id: Uuid::nil(),
        repo_id: Uuid::nil(),
        repo_name: String::new(),
        repo_slug: String::new(),
        branch_name: String::new(),
        commit: commit.to_string(),
    };
    let content = if record.file_type == FileType::Chat {
        format_chat_content(record.chat_context.as_deref(), record.content.as_deref())
    } else {
        match record.content.clone() {
            Some(content) => Some(content),
            None => match extract_block_content(git_repo, commit, record).await {
                Some(content) => Some(content),
                None => {
                    extract_block_content_from_explicit_fallbacks(content_fallbacks, record).await
                }
            },
        }
    };

    TraceResult {
        intent_record_id: synthetic_uuid(record),
        workspace_id: owner.workspace_id,
        repo_id: owner.repo_id,
        file_type: record.file_type.to_string(),
        file_name: record.file_name.clone(),
        intent_record_number: record.intent_record_number.clone(),
        code_block_name: record.code_block_name.clone(),
        content: content.clone(),
        title: None,
        block_label: None,
        chat_session_id: (record.file_type == FileType::Chat)
            .then(|| record.chat_session_id.clone())
            .flatten(),
        chat_context: (record.file_type == FileType::Chat)
            .then(|| record.chat_context.clone())
            .flatten(),
        user_content: (record.file_type == FileType::Chat)
            .then(|| record.content.clone())
            .flatten(),
        created_by: record.created_by.clone(),
        created_at: record.created_at.clone(),
        source_file_path: None,
        stale: record.file_type != FileType::Chat && content.is_none(),
        is_human: record.is_human,
        priority: record.priority.map(|v| v as i16),
        certainty: record.certainty.map(|v| v as i16),
        relationship: None,
        relevance: None,
    }
}

#[cfg(test)]
async fn extract_block_content_from_explicit_fallbacks(
    fallbacks: &[TraceContentFallback],
    record: &IntentRecord,
) -> Option<String> {
    for source in fallbacks {
        if let TraceContentFallback::ExplicitCommit { git_repo, commit } = source {
            if let Some(content) = extract_block_content(git_repo.clone(), commit, record).await {
                return Some(content);
            }
        }
    }
    None
}

async fn intent_record_to_trace_result_with_overlay_and_fallbacks(
    record: &IntentRecord,
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
    owner: &WorkspaceRepoContext,
    content_fallbacks: &[TraceContentFallback],
) -> TraceResult {
    let source_file_path =
        resolve_source_file_path_from_view(record, repo_storage, repo_view).await;
    let mut title = None;
    let mut block_label = None;
    let (content, stale) = if record.file_type == FileType::Chat {
        // Chat records: format chat_context + content per pm_view §3.8.2.
        // Content is stored inline in the JSONL record, not in a file.
        (
            format_chat_content(record.chat_context.as_deref(), record.content.as_deref()),
            false,
        )
    } else {
        match extract_record_block_from_view(repo_storage, repo_view, record).await {
            BlockRead::Found(block) => {
                title = block.title;
                block_label = block.label;
                (Some(block.content), false)
            }
            BlockRead::TerminalMissing => (None, true),
            BlockRead::Absent => {
                match extract_record_block_from_fallbacks(repo_storage, content_fallbacks, record)
                    .await
                {
                    Some(block) => {
                        title = block.title;
                        block_label = block.label;
                        (Some(block.content), false)
                    }
                    None => (None, true),
                }
            }
        }
    };

    trace_result_fields(
        record,
        owner,
        content,
        title,
        block_label,
        source_file_path,
        stale,
    )
}

fn trace_result_fields(
    record: &IntentRecord,
    owner: &WorkspaceRepoContext,
    content: Option<String>,
    title: Option<String>,
    block_label: Option<String>,
    source_file_path: Option<String>,
    stale: bool,
) -> TraceResult {
    TraceResult {
        intent_record_id: synthetic_uuid(record),
        workspace_id: owner.workspace_id,
        repo_id: owner.repo_id,
        file_type: record.file_type.to_string(),
        file_name: record.file_name.clone(),
        intent_record_number: record.intent_record_number.clone(),
        code_block_name: record.code_block_name.clone(),
        content: content.clone(),
        title,
        block_label,
        chat_session_id: (record.file_type == FileType::Chat)
            .then(|| record.chat_session_id.clone())
            .flatten(),
        chat_context: (record.file_type == FileType::Chat)
            .then(|| record.chat_context.clone())
            .flatten(),
        user_content: (record.file_type == FileType::Chat)
            .then(|| record.content.clone())
            .flatten(),
        created_by: record.created_by.clone(),
        created_at: record.created_at.clone(),
        source_file_path,
        stale,
        is_human: record.is_human,
        priority: record.priority.map(|v| v as i16),
        certainty: record.certainty.map(|v| v as i16),
        relationship: None,
        relevance: None,
    }
}

enum BlockRead {
    Found(g5_semantic::block_extraction::ExtractedBlock),
    /// The source was present (or explicitly deleted) but the requested block
    /// could not be served. Sibling fallbacks must not resurrect it.
    TerminalMissing,
    /// This repository does not own the source path; sibling content may.
    Absent,
}

async fn extract_record_block_from_view(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
    record: &IntentRecord,
) -> BlockRead {
    let Some(file_name) = record.file_name.as_deref() else {
        return BlockRead::TerminalMissing;
    };
    let Some(identifier) = record
        .intent_record_number
        .as_deref()
        .or(record.code_block_name.as_deref())
    else {
        return BlockRead::TerminalMissing;
    };

    let read = match repo_view.read_file(repo_storage, file_name).await {
        Ok(read) => read,
        Err(error) => {
            warn!(
                repo_id = %repo_view.repo.repo_id,
                file_name,
                identifier,
                error = %error,
                "Unable to read grounding trace source"
            );
            return BlockRead::TerminalMissing;
        }
    };
    let bytes = match read {
        RepoFileRead::Content { bytes, .. } => bytes,
        RepoFileRead::Deleted => return BlockRead::TerminalMissing,
        RepoFileRead::Missing => return BlockRead::Absent,
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            warn!(file_name, identifier, error = %error, "Grounding trace source is not UTF-8");
            return BlockRead::TerminalMissing;
        }
    };
    match block_extraction::extract_block_from_text(record.file_type, file_name, identifier, &text)
    {
        Ok(block) => BlockRead::Found(block),
        Err(error) => {
            warn!(file_name, identifier, error = %error, "Unable to extract grounding trace block");
            BlockRead::TerminalMissing
        }
    }
}

async fn extract_record_block_from_fallbacks(
    repo_storage: &TenantRepoStorage,
    fallbacks: &[TraceContentFallback],
    record: &IntentRecord,
) -> Option<g5_semantic::block_extraction::ExtractedBlock> {
    for source in fallbacks {
        let block = match source {
            TraceContentFallback::View(view) => {
                match extract_record_block_from_view(repo_storage, view, record).await {
                    BlockRead::Found(block) => Some(block),
                    BlockRead::TerminalMissing | BlockRead::Absent => None,
                }
            }
            #[cfg(test)]
            TraceContentFallback::ExplicitCommit { git_repo, commit } => {
                let file_name = record.file_name.as_deref()?;
                let identifier = record
                    .intent_record_number
                    .as_deref()
                    .or(record.code_block_name.as_deref())?;
                let text =
                    read_text_at_commit(git_repo.clone(), commit.clone(), file_name.to_string())
                        .await?;
                block_extraction::extract_block_from_text(
                    record.file_type,
                    file_name,
                    identifier,
                    &text,
                )
                .ok()
            }
        };
        if block.is_some() {
            return block;
        }
    }
    None
}

async fn resolve_source_file_path_from_view(
    record: &IntentRecord,
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
) -> Option<String> {
    if record.file_type != FileType::Code {
        return None;
    }

    let canonical = record.file_name.as_deref()?;
    let sanitized = crate::routes::files::sanitize_path(canonical).ok()?;
    if sanitized != canonical {
        return None;
    }

    let mut candidates = vec![canonical.to_string()];
    if !canonical.starts_with("generation/") {
        candidates.push(format!("generation/{canonical}"));
    }
    candidates.sort();
    candidates.dedup();

    let reads = repo_view.read_files(repo_storage, &candidates).await.ok()?;
    let mut matches = candidates
        .into_iter()
        .filter(|candidate| matches!(reads.get(candidate), Some(RepoFileRead::Content { .. })))
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches.remove(0))
}

/// Format chat content per `chat_agent.pm_view.md` §3.8.2:
/// - If `chat_context` is present: "Chat context: <ctx>\n---------\nUser input: <content>"
/// - If `chat_context` is absent: "User input: <content>"
/// - If `content` is also absent: None
pub(crate) fn format_chat_content(
    chat_context: Option<&str>,
    content: Option<&str>,
) -> Option<String> {
    let c = content.unwrap_or_default();
    if c.is_empty() {
        return None;
    }
    match chat_context {
        Some(ctx) if !ctx.is_empty() => {
            Some(format!("Chat context: {ctx}\n---------\nUser input: {c}"))
        }
        _ => Some(format!("User input: {c}")),
    }
}

/// Extract the content of an intent/decision block from a committed file.
///
/// Delegates to the canonical block-extraction service. Retired Markdown
/// wrapper forms such as `## [I1. ...]` or `- [D2a. ...]` are invalid and do
/// not receive a compatibility extraction path.
#[cfg(test)]
async fn extract_block_content(
    git_repo: Arc<GitRepo>,
    commit: &str,
    record: &IntentRecord,
) -> Option<String> {
    let file_name = record.file_name.as_deref()?;
    let text = read_text_at_commit(git_repo, commit.to_string(), file_name.to_string()).await?;
    extract_block_content_from_text(record, &text)
}

#[cfg(test)]
pub(crate) fn extract_block_content_from_text(record: &IntentRecord, text: &str) -> Option<String> {
    let identifier = record
        .intent_record_number
        .as_deref()
        .or(record.code_block_name.as_deref())?;
    block_extraction::extract_block_from_text(
        record.file_type,
        record.file_name.as_deref()?,
        identifier,
        text,
    )
    .ok()
    .map(|block| block.content)
}

/// Generate a deterministic UUID from an intent record's natural key.
///
/// The frontend uses `intentRecordId` as a React key. We derive a stable
/// UUID from the natural key fields so the same record always produces
/// the same ID across requests.
pub(crate) fn synthetic_uuid(record: &IntentRecord) -> Uuid {
    let mut hasher = DefaultHasher::new();
    record.file_type.to_string().hash(&mut hasher);
    record.file_name.hash(&mut hasher);
    record.intent_record_number.hash(&mut hasher);
    record.code_block_name.hash(&mut hasher);
    record.chat_session_id.hash(&mut hasher);
    let hash = hasher.finish();

    // Pack the 8-byte hash into a 16-byte UUID (repeat to fill).
    let hi = hash.to_le_bytes();
    let lo = hash.wrapping_mul(0x9E3779B97F4A7C15).to_le_bytes();
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hi);
    bytes[8..].copy_from_slice(&lo);
    // Set version 4 and variant bits for RFC 4122 compliance.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use g5_scm::auth::GitAuthor;

    fn test_author() -> GitAuthor {
        GitAuthor {
            name: "Test".into(),
            email: "test@test.com".into(),
        }
    }

    /// Create a temp repo + dir. Returns (Arc<GitRepo>, TempDir).
    fn init_temp() -> (Arc<GitRepo>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .expect("git init failed");
        let repo = Arc::new(GitRepo::open(dir.path()).unwrap());
        (repo, dir)
    }

    /// Helper: create a temp git repo with committed grounding files.
    struct TestRepo {
        repo: Arc<GitRepo>,
        _dir: tempfile::TempDir,
        commit: String,
    }

    impl TestRepo {
        /// Build a repo with a realistic grounding edge graph.
        ///
        /// Graph:
        ///   pm_view.md I1 (view)  ──▶  spec.semlift.md I1 (ontology)
        ///   pm_view.md I1 (view)  ──▶  spec.semlift.md I2 (ontology)
        ///   spec.semlift.md I1    ──▶  src/lib.rs fn_init (code)
        ///   spec.semlift.md I2    ──▶  src/lib.rs fn_validate (code)
        #[allow(clippy::useless_vec)]
        fn with_grounding() -> Self {
            let (repo, dir) = init_temp();
            let author = test_author();
            let base = dir.path();

            // --- Grounding edges (target-keyed JSONL) ---

            // Edge files under intent_grounding/ontology/ (target = ontology)
            let ont_dir = base.join("intent_grounding/ontology");
            std::fs::create_dir_all(&ont_dir).unwrap();
            let ont_edges = vec![
                GroundingEdge {
                    source_file_type: FileType::View,
                    target_file_type: FileType::Ontology,
                    source_file: "views/pm_views/feature.pm_view.md".into(),
                    target_file: ".semlift/specs/feature.semlift.md".into(),
                    source_id: "I1".into(),
                    target_id: "I1".into(),
                    relationship: Some("grounds".into()),
                    relevance: Some(90),
                    created_by: None,
                    created_at: None,
                    created_actor: None,
                    executed_by: None,
                },
                GroundingEdge {
                    source_file_type: FileType::View,
                    target_file_type: FileType::Ontology,
                    source_file: "views/pm_views/feature.pm_view.md".into(),
                    target_file: ".semlift/specs/feature.semlift.md".into(),
                    source_id: "I1".into(),
                    target_id: "I2".into(),
                    relationship: Some("grounds".into()),
                    relevance: Some(85),
                    created_by: None,
                    created_at: None,
                    created_actor: None,
                    executed_by: None,
                },
            ];
            let encoded_ont = encode_file_name(".semlift/specs/feature.semlift.md");
            let ont_path = ont_dir.join(format!("{encoded_ont}.jsonl"));
            let ont_content: Vec<String> = ont_edges
                .iter()
                .map(|e| serde_json::to_string(e).unwrap())
                .collect();
            std::fs::write(&ont_path, ont_content.join("\n")).unwrap();

            // Edge files under intent_grounding/code/ (target = code)
            let code_dir = base.join("intent_grounding/code");
            std::fs::create_dir_all(&code_dir).unwrap();
            let code_edges = vec![
                GroundingEdge {
                    source_file_type: FileType::Ontology,
                    target_file_type: FileType::Code,
                    source_file: ".semlift/specs/feature.semlift.md".into(),
                    target_file: "src/lib.rs".into(),
                    source_id: "I1".into(),
                    target_id: "fn_init".into(),
                    relationship: Some("implements".into()),
                    relevance: Some(95),
                    created_by: None,
                    created_at: None,
                    created_actor: None,
                    executed_by: None,
                },
                GroundingEdge {
                    source_file_type: FileType::Ontology,
                    target_file_type: FileType::Code,
                    source_file: ".semlift/specs/feature.semlift.md".into(),
                    target_file: "src/lib.rs".into(),
                    source_id: "I2".into(),
                    target_id: "fn_validate".into(),
                    relationship: Some("implements".into()),
                    relevance: Some(90),
                    created_by: None,
                    created_at: None,
                    created_actor: None,
                    executed_by: None,
                },
            ];
            let encoded_code = encode_file_name("src/lib.rs");
            let code_path = code_dir.join(format!("{encoded_code}.jsonl"));
            let code_content: Vec<String> = code_edges
                .iter()
                .map(|e| serde_json::to_string(e).unwrap())
                .collect();
            std::fs::write(&code_path, code_content.join("\n")).unwrap();

            // Edge file under intent_grounding/views/ (target = view) — for reverse trace
            let view_dir = base.join("intent_grounding/views");
            std::fs::create_dir_all(&view_dir).unwrap();
            // No edges targeting views in this graph → empty subdir

            // --- Intent records ---

            // View intent records
            let ir_view_dir = base.join("intent_record/views");
            std::fs::create_dir_all(&ir_view_dir).unwrap();
            let view_record = IntentRecord {
                file_type: FileType::View,
                file_name: Some("views/pm_views/feature.pm_view.md".into()),
                intent_record_number: Some("I1".into()),
                code_block_name: None,
                chat_session_id: None,
                chat_context: None,
                content: Some("The system shall validate all input fields.".into()),
                content_fingerprint: None,
                is_human: true,
                is_uncertain: false,
                priority: Some(1),
                certainty: Some(90),
                created_by: None,
                last_updated_by: None,
                created_at: None,
                last_updated_at: None,
                created_actor: None,
                updated_actor: None,
                executed_by: None,
            };
            let encoded_view = encode_file_name("views/pm_views/feature.pm_view.md");
            let ir_view_path = ir_view_dir.join(format!("{encoded_view}.jsonl"));
            std::fs::write(&ir_view_path, serde_json::to_string(&view_record).unwrap()).unwrap();

            // Ontology intent records
            let ir_ont_dir = base.join("intent_record/ontology");
            std::fs::create_dir_all(&ir_ont_dir).unwrap();
            let ont_records = vec![
                IntentRecord {
                    file_type: FileType::Ontology,
                    file_name: Some(".semlift/specs/feature.semlift.md".into()),
                    intent_record_number: Some("I1".into()),
                    code_block_name: None,
                    chat_session_id: None,
                    chat_context: None,
                    content: Some("Initialize the feature module.".into()),
                    content_fingerprint: None,
                    is_human: false,
                    is_uncertain: false,
                    priority: Some(2),
                    certainty: Some(85),
                    created_by: None,
                    last_updated_by: None,
                    created_at: None,
                    last_updated_at: None,
                    created_actor: None,
                    updated_actor: None,
                    executed_by: None,
                },
                IntentRecord {
                    file_type: FileType::Ontology,
                    file_name: Some(".semlift/specs/feature.semlift.md".into()),
                    intent_record_number: Some("I2".into()),
                    code_block_name: None,
                    chat_session_id: None,
                    chat_context: None,
                    content: Some("Validate input according to field definitions.".into()),
                    content_fingerprint: None,
                    is_human: false,
                    is_uncertain: false,
                    priority: Some(2),
                    certainty: Some(80),
                    created_by: None,
                    last_updated_by: None,
                    created_at: None,
                    last_updated_at: None,
                    created_actor: None,
                    updated_actor: None,
                    executed_by: None,
                },
            ];
            let encoded_ont_ir = encode_file_name(".semlift/specs/feature.semlift.md");
            let ir_ont_path = ir_ont_dir.join(format!("{encoded_ont_ir}.jsonl"));
            let ir_ont_content: Vec<String> = ont_records
                .iter()
                .map(|r| serde_json::to_string(r).unwrap())
                .collect();
            std::fs::write(&ir_ont_path, ir_ont_content.join("\n")).unwrap();

            // Code intent records
            let ir_code_dir = base.join("intent_record/code");
            std::fs::create_dir_all(&ir_code_dir).unwrap();
            let code_records = vec![
                IntentRecord {
                    file_type: FileType::Code,
                    file_name: Some("src/lib.rs".into()),
                    intent_record_number: None,
                    code_block_name: Some("fn_init".into()),
                    chat_session_id: None,
                    chat_context: None,
                    content: Some("pub fn init() { ... }".into()),
                    content_fingerprint: None,
                    is_human: false,
                    is_uncertain: false,
                    priority: None,
                    certainty: None,
                    created_by: None,
                    last_updated_by: None,
                    created_at: None,
                    last_updated_at: None,
                    created_actor: None,
                    updated_actor: None,
                    executed_by: None,
                },
                IntentRecord {
                    file_type: FileType::Code,
                    file_name: Some("src/lib.rs".into()),
                    intent_record_number: None,
                    code_block_name: Some("fn_validate".into()),
                    chat_session_id: None,
                    chat_context: None,
                    content: Some("pub fn validate() { ... }".into()),
                    content_fingerprint: None,
                    is_human: false,
                    is_uncertain: false,
                    priority: None,
                    certainty: None,
                    created_by: None,
                    last_updated_by: None,
                    created_at: None,
                    last_updated_at: None,
                    created_actor: None,
                    updated_actor: None,
                    executed_by: None,
                },
            ];
            let encoded_code_ir = encode_file_name("src/lib.rs");
            let ir_code_path = ir_code_dir.join(format!("{encoded_code_ir}.jsonl"));
            let ir_code_content: Vec<String> = code_records
                .iter()
                .map(|r| serde_json::to_string(r).unwrap())
                .collect();
            std::fs::write(&ir_code_path, ir_code_content.join("\n")).unwrap();

            // --- Commit everything ---
            repo.add_all().unwrap();
            let commit = repo.commit("Add grounding data", &author).unwrap();

            TestRepo {
                repo,
                _dir: dir,
                commit,
            }
        }

        /// Build a repo with NO grounding data (simulates the code-only repo).
        fn empty() -> Self {
            let (repo, dir) = init_temp();
            let author = test_author();

            std::fs::write(dir.path().join("README.md"), "# Code repo").unwrap();
            repo.add_all().unwrap();
            let commit = repo.commit("Initial", &author).unwrap();

            TestRepo {
                repo,
                _dir: dir,
                commit,
            }
        }
    }

    // ── read_jsonl_at_commit ────────────────────────────────────────────

    #[tokio::test]
    async fn read_jsonl_at_commit_returns_edges() {
        let t = TestRepo::with_grounding();
        let encoded = encode_file_name(".semlift/specs/feature.semlift.md");
        let path = format!("intent_grounding/ontology/{encoded}.jsonl");

        let edges: Vec<GroundingEdge> =
            read_jsonl_at_commit(t.repo.clone(), &t.commit, &path).await;
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].source_id, "I1");
        assert_eq!(edges[0].target_id, "I1");
        assert_eq!(edges[1].source_id, "I1");
        assert_eq!(edges[1].target_id, "I2");
    }

    #[tokio::test]
    async fn read_jsonl_at_commit_missing_file_returns_empty() {
        let t = TestRepo::with_grounding();
        let edges: Vec<GroundingEdge> =
            read_jsonl_at_commit(t.repo.clone(), &t.commit, "nonexistent.jsonl").await;
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn read_jsonl_at_commit_skips_malformed_lines() {
        let (repo, dir) = init_temp();
        let author = test_author();

        let grounding_dir = dir.path().join("intent_grounding/ontology");
        std::fs::create_dir_all(&grounding_dir).unwrap();
        std::fs::write(
            grounding_dir.join("test.jsonl"),
            "not valid json\n{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"a.md\",\"target_file\":\"b.md\",\"source_id\":\"I1\",\"target_id\":\"I2\"}\n",
        ).unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("Mixed", &author).unwrap();

        let edges: Vec<GroundingEdge> = read_jsonl_at_commit(
            repo.clone(),
            &commit,
            "intent_grounding/ontology/test.jsonl",
        )
        .await;
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source_id, "I1");
    }

    #[tokio::test]
    async fn read_jsonl_at_commit_skips_blank_and_comment_lines() {
        let (repo, dir) = init_temp();
        let author = test_author();

        let grounding_dir = dir.path().join("intent_grounding/ontology");
        std::fs::create_dir_all(&grounding_dir).unwrap();
        std::fs::write(
            grounding_dir.join("test.jsonl"),
            "\n# This is a comment\n  \n{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"a.md\",\"target_file\":\"b.md\",\"source_id\":\"I1\",\"target_id\":\"I2\"}\n",
        ).unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("With blanks", &author).unwrap();

        let edges: Vec<GroundingEdge> = read_jsonl_at_commit(
            repo.clone(),
            &commit,
            "intent_grounding/ontology/test.jsonl",
        )
        .await;
        assert_eq!(edges.len(), 1);
    }

    // ── list_grounding_jsonl_files ──────────────────────────────────────

    #[tokio::test]
    async fn list_grounding_jsonl_files_discovers_all_subfolder_files() {
        let t = TestRepo::with_grounding();
        let mut files = list_grounding_jsonl_files(t.repo.clone(), t.commit.clone()).await;
        files.sort();

        // We created files in ontology/ and code/ subfolders
        assert_eq!(files.len(), 2, "Expected 2 JSONL files, got: {files:?}");
        assert!(files.iter().any(|f| f.contains("ontology/")));
        assert!(files.iter().any(|f| f.contains("code/")));
    }

    #[tokio::test]
    async fn list_grounding_jsonl_files_returns_empty_when_no_grounding_dir() {
        let t = TestRepo::empty();
        let files = list_grounding_jsonl_files(t.repo.clone(), t.commit.clone()).await;
        assert!(files.is_empty());
    }

    // ── find_intent_record ──────────────────────────────────────────────

    #[tokio::test]
    async fn find_intent_record_by_intent_record_number() {
        let t = TestRepo::with_grounding();
        let record = find_intent_record(
            t.repo.clone(),
            &t.commit,
            FileType::Ontology,
            ".semlift/specs/feature.semlift.md",
            "I2",
        )
        .await;
        assert!(record.is_some());
        let r = record.unwrap();
        assert_eq!(r.intent_record_number.as_deref(), Some("I2"));
        assert_eq!(
            r.content.as_deref(),
            Some("Validate input according to field definitions.")
        );
    }

    #[tokio::test]
    async fn find_intent_record_by_code_block_name() {
        let t = TestRepo::with_grounding();
        let record = find_intent_record(
            t.repo.clone(),
            &t.commit,
            FileType::Code,
            "src/lib.rs",
            "fn_init",
        )
        .await;
        assert!(record.is_some());
        let r = record.unwrap();
        assert_eq!(r.code_block_name.as_deref(), Some("fn_init"));
    }

    #[tokio::test]
    async fn find_intent_record_missing_returns_none() {
        let t = TestRepo::with_grounding();
        let record = find_intent_record(
            t.repo.clone(),
            &t.commit,
            FileType::Ontology,
            ".semlift/specs/feature.semlift.md",
            "I99",
        )
        .await;
        assert!(record.is_none());
    }

    #[tokio::test]
    async fn find_intent_record_missing_file_returns_none() {
        let t = TestRepo::with_grounding();
        let record = find_intent_record(
            t.repo.clone(),
            &t.commit,
            FileType::View,
            "nonexistent.md",
            "I1",
        )
        .await;
        assert!(record.is_none());
    }

    // ── extract_block_content ───────────────────────────────────────────

    #[tokio::test]
    async fn extract_block_content_parses_intent_block() {
        let (repo, dir) = init_temp();
        let author = test_author();

        let content = "\
# Feature Spec

[I1. Data validation]
All inputs must be validated before processing.
Validation includes type checking and range checks.

[I2. Error handling]
Errors should be reported to the user.
";
        std::fs::write(dir.path().join("spec.md"), content).unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("Add spec", &author).unwrap();

        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None, // No inline content — will be extracted
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let extracted = extract_block_content(repo.clone(), &commit, &record).await;
        assert!(extracted.is_some());
        let text = extracted.unwrap();
        assert!(!text.contains("[I1. Data validation]"));
        assert!(text.contains("All inputs must be validated"));
        assert!(text.contains("range checks."));
        // Should NOT include the I2 block
        assert!(!text.contains("[I2."));
    }

    #[tokio::test]
    async fn extract_block_content_rejects_heading_prefixed_intent_block() {
        let (repo, dir) = init_temp();
        let author = test_author();

        let content = "\
# Feature Spec

## [I1. Data validation]
All inputs must be validated before processing.
Validation includes type checking and range checks.

## [I2. Error handling]
Errors should be reported to the user.
";
        std::fs::write(dir.path().join("spec.md"), content).unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("Add spec", &author).unwrap();

        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        assert!(extract_block_content(repo.clone(), &commit, &record)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn extract_block_content_rejects_list_prefixed_intent_block() {
        let (repo, dir) = init_temp();
        let author = test_author();

        let content = "\
- [D2a. Audit decision]
Retain the original source context.

1. [I3. Next intent]
Unrelated content.
";
        std::fs::write(dir.path().join("spec.md"), content).unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("Add spec", &author).unwrap();

        let record = IntentRecord {
            file_type: FileType::View,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("D2a".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        assert!(extract_block_content(repo.clone(), &commit, &record)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn extract_block_content_stops_at_heading() {
        let (repo, dir) = init_temp();
        let author = test_author();

        let content = "\
[I1. Only block]
Some content here.

## Next Section
Unrelated content.
";
        std::fs::write(dir.path().join("spec.md"), content).unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("Add spec", &author).unwrap();

        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let text = extract_block_content(repo.clone(), &commit, &record)
            .await
            .unwrap();
        assert!(text.contains("Some content here."));
        assert!(!text.contains("Unrelated content."));
    }

    #[tokio::test]
    async fn extract_block_content_skips_ontology_tags() {
        let (repo, dir) = init_temp();
        let author = test_author();

        let content = "\
[I1. Tagged block] (Tags: human)
Important content.
More content.
";
        std::fs::write(dir.path().join("spec.md"), content).unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("Add spec", &author).unwrap();

        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let text = extract_block_content(repo.clone(), &commit, &record)
            .await
            .unwrap();
        assert!(text.contains("Important content."));
        assert!(text.contains("More content."));
        // Canonical label metadata should be stripped with the label line.
        assert!(!text.contains("Tags: human"));
    }

    #[tokio::test]
    async fn extract_block_content_missing_label_returns_none() {
        let (repo, dir) = init_temp();
        let author = test_author();

        std::fs::write(dir.path().join("spec.md"), "No intent blocks here.").unwrap();
        repo.add_all().unwrap();
        let commit = repo.commit("Add spec", &author).unwrap();

        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        assert!(extract_block_content(repo.clone(), &commit, &record)
            .await
            .is_none());
    }

    // ── synthetic_uuid ──────────────────────────────────────────────────

    #[test]
    fn synthetic_uuid_is_deterministic() {
        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: false,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let uuid1 = synthetic_uuid(&record);
        let uuid2 = synthetic_uuid(&record);
        assert_eq!(uuid1, uuid2);
    }

    #[test]
    fn synthetic_uuid_differs_for_different_records() {
        let mut r1 = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: false,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };
        let r2 = IntentRecord {
            intent_record_number: Some("I2".into()),
            ..r1.clone()
        };

        assert_ne!(synthetic_uuid(&r1), synthetic_uuid(&r2));

        // Different file_type also produces different UUID
        r1.file_type = FileType::View;
        assert_ne!(synthetic_uuid(&r1), synthetic_uuid(&r2));
    }

    #[test]
    fn synthetic_uuid_differs_for_same_chat_hash_in_different_sessions() {
        let r1 = IntentRecord {
            file_type: FileType::Chat,
            file_name: None,
            intent_record_number: Some("a7f3b2c1".into()),
            code_block_name: None,
            chat_session_id: Some("session-a".into()),
            chat_context: None,
            content: Some("same content".into()),
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };
        let r2 = IntentRecord {
            chat_session_id: Some("session-b".into()),
            ..r1.clone()
        };

        assert_ne!(synthetic_uuid(&r1), synthetic_uuid(&r2));
    }

    #[test]
    fn synthetic_uuid_is_valid_v4() {
        let record = IntentRecord {
            file_type: FileType::Code,
            file_name: Some("src/main.rs".into()),
            intent_record_number: None,
            code_block_name: Some("fn_main".into()),
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: false,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let uuid = synthetic_uuid(&record);
        // Version should be 4
        assert_eq!(uuid.get_version_num(), 4);
    }

    #[test]
    fn trace_result_serializes_edge_metadata() {
        let result = TraceResult {
            intent_record_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            file_type: "ontology".to_string(),
            file_name: Some(".semlift/specs/feature.semlift.md".to_string()),
            intent_record_number: Some("D1".to_string()),
            code_block_name: None,
            content: Some("[D1. Referenced block]".to_string()),
            title: Some("Referenced block".to_string()),
            block_label: Some("D1".to_string()),
            chat_session_id: None,
            chat_context: None,
            user_content: None,
            created_by: None,
            created_at: None,
            source_file_path: None,
            stale: false,
            is_human: false,
            priority: None,
            certainty: None,
            relationship: Some("ref".to_string()),
            relevance: Some(80),
        };

        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["relationship"], "ref");
        assert_eq!(value["relevance"], 80);
        assert_eq!(value["title"], "Referenced block");
        assert_eq!(value["blockLabel"], "D1");
    }

    // ── intent_record_to_trace_result ───────────────────────────────────

    #[tokio::test]
    async fn intent_record_to_trace_result_uses_inline_content() {
        let t = TestRepo::with_grounding();
        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some(".semlift/specs/feature.semlift.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: Some("Inline content here.".into()),
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: Some(1),
            certainty: Some(90),
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let result = intent_record_to_trace_result(&record, t.repo.clone(), &t.commit, &[]).await;
        assert_eq!(result.file_type, "ontology");
        assert_eq!(
            result.file_name.as_deref(),
            Some(".semlift/specs/feature.semlift.md")
        );
        assert_eq!(result.intent_record_number.as_deref(), Some("I1"));
        assert_eq!(result.content.as_deref(), Some("Inline content here."));
        assert!(result.is_human);
        assert_eq!(result.priority, Some(1));
        assert_eq!(result.certainty, Some(90));
    }

    #[tokio::test]
    async fn intent_record_to_trace_result_extracts_content_from_fallback_repo() {
        let (primary_repo, primary_dir) = init_temp();
        let (fallback_repo, fallback_dir) = init_temp();
        let author = test_author();

        std::fs::write(primary_dir.path().join("README.md"), "primary repo").unwrap();
        primary_repo.add_all().unwrap();
        let primary_commit = primary_repo.commit("Add primary marker", &author).unwrap();

        let content = "\
[I1. Fallback content]
This content lives in the sibling repo.

[I2. Other content]
Do not include this block.
";
        std::fs::write(fallback_dir.path().join("spec.md"), content).unwrap();
        fallback_repo.add_all().unwrap();
        let fallback_commit = fallback_repo.commit("Add fallback spec", &author).unwrap();

        let record = IntentRecord {
            file_type: FileType::Ontology,
            file_name: Some("spec.md".into()),
            intent_record_number: Some("I1".into()),
            code_block_name: None,
            chat_session_id: None,
            chat_context: None,
            content: None,
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };
        let fallbacks = [TraceContentFallback::ExplicitCommit {
            git_repo: fallback_repo,
            commit: fallback_commit,
        }];

        let result = intent_record_to_trace_result(
            &record,
            primary_repo.clone(),
            &primary_commit,
            &fallbacks,
        )
        .await;
        assert!(result.content.as_deref().is_some_and(|text| {
            text.contains("This content lives in the sibling repo.")
                && !text.contains("Do not include this block.")
        }));
    }

    #[tokio::test]
    async fn intent_record_to_trace_result_formats_chat_with_context() {
        let t = TestRepo::with_grounding();
        let record = IntentRecord {
            file_type: FileType::Chat,
            file_name: None,
            intent_record_number: Some("abcd1234".into()),
            code_block_name: None,
            chat_session_id: Some("task-abc-123".into()),
            chat_context: Some("Discussing retry configuration".into()),
            content: Some("make the retry count 5".into()),
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let result = intent_record_to_trace_result(&record, t.repo.clone(), &t.commit, &[]).await;
        assert_eq!(result.file_type, "chat");
        assert_eq!(
            result.content.as_deref(),
            Some("Chat context: Discussing retry configuration\n---------\nUser input: make the retry count 5")
        );
        assert!(result.is_human);
        assert!(result.file_name.is_none());
    }

    #[tokio::test]
    async fn intent_record_to_trace_result_formats_chat_without_context() {
        let t = TestRepo::with_grounding();
        let record = IntentRecord {
            file_type: FileType::Chat,
            file_name: None,
            intent_record_number: Some("ef012345".into()),
            code_block_name: None,
            chat_session_id: Some("task-xyz-789".into()),
            chat_context: None,
            content: Some("add pagination to the task list".into()),
            content_fingerprint: None,
            is_human: true,
            is_uncertain: false,
            priority: None,
            certainty: None,
            created_by: None,
            last_updated_by: None,
            created_at: None,
            last_updated_at: None,
            created_actor: None,
            updated_actor: None,
            executed_by: None,
        };

        let result = intent_record_to_trace_result(&record, t.repo.clone(), &t.commit, &[]).await;
        assert_eq!(result.file_type, "chat");
        assert_eq!(
            result.content.as_deref(),
            Some("User input: add pagination to the task list")
        );
    }

    // ── End-to-end trace via helpers ────────────────────────────────────

    /// Test the full trace_parents path: given ontology I1, verify
    /// we find its view parent (pm_view.md I1).
    #[tokio::test]
    async fn trace_parents_finds_view_source_for_ontology_target() {
        let t = TestRepo::with_grounding();

        // Read the ontology target file's grounding edges
        let encoded = encode_file_name(".semlift/specs/feature.semlift.md");
        let jsonl_path = format!("intent_grounding/ontology/{encoded}.jsonl");
        let edges: Vec<GroundingEdge> =
            read_jsonl_at_commit(t.repo.clone(), &t.commit, &jsonl_path).await;

        // Filter for target_id == "I1" (same as trace_parents does)
        let parent_edges: Vec<&GroundingEdge> =
            edges.iter().filter(|e| e.target_id == "I1").collect();
        assert_eq!(parent_edges.len(), 1);
        assert_eq!(
            parent_edges[0].source_file,
            "views/pm_views/feature.pm_view.md"
        );
        assert_eq!(parent_edges[0].source_id, "I1");

        // Resolve the parent intent record
        let record = find_intent_record(
            t.repo.clone(),
            &t.commit,
            parent_edges[0].source_file_type,
            &parent_edges[0].source_file,
            &parent_edges[0].source_id,
        )
        .await;
        assert!(record.is_some());
        let r = record.unwrap();
        assert_eq!(r.file_type, FileType::View);
        assert_eq!(
            r.content.as_deref(),
            Some("The system shall validate all input fields.")
        );
    }

    /// Test the full trace_children scan: given pm_view.md I1, verify
    /// we find both ontology children (I1 and I2).
    #[tokio::test]
    async fn trace_children_finds_ontology_targets_for_view_source() {
        let t = TestRepo::with_grounding();

        let grounding_files = list_grounding_jsonl_files(t.repo.clone(), t.commit.clone()).await;
        let mut children = Vec::new();

        for jsonl_path in &grounding_files {
            let edges: Vec<GroundingEdge> =
                read_jsonl_at_commit(t.repo.clone(), &t.commit, jsonl_path).await;
            for edge in edges {
                if edge.source_file == "views/pm_views/feature.pm_view.md" && edge.source_id == "I1"
                {
                    if let Some(record) = find_intent_record(
                        t.repo.clone(),
                        &t.commit,
                        edge.target_file_type,
                        &edge.target_file,
                        &edge.target_id,
                    )
                    .await
                    {
                        children.push(record);
                    }
                }
            }
        }

        assert_eq!(
            children.len(),
            2,
            "Expected 2 children, got: {}",
            children.len()
        );
        let irns: Vec<_> = children
            .iter()
            .map(|c| c.intent_record_number.as_deref().unwrap_or(""))
            .collect();
        assert!(irns.contains(&"I1"));
        assert!(irns.contains(&"I2"));
    }

    /// Verify that an empty repo (no intent_grounding/) returns no results.
    #[tokio::test]
    async fn trace_children_empty_repo_returns_nothing() {
        let t = TestRepo::empty();
        let files = list_grounding_jsonl_files(t.repo.clone(), t.commit.clone()).await;
        assert!(files.is_empty());
    }

    #[test]
    fn repo_scoped_file_candidates_include_local_and_route_prefixed_names() {
        let repo = WorkspaceRepoContext {
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            repo_name: "docs".to_string(),
            repo_slug: "docs".to_string(),
            branch_name: "main".to_string(),
            commit: "abc123".to_string(),
        };

        let candidates = repo_scoped_file_candidates(&repo, "views/window-functions.md");

        assert!(candidates.contains("views/window-functions.md"));
        assert!(candidates.contains("docs/views/window-functions.md"));
    }

    #[test]
    fn select_trace_repo_requires_an_exact_member() {
        let current = WorkspaceRepoContext {
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            repo_name: "code".to_string(),
            repo_slug: "code".to_string(),
            branch_name: "main".to_string(),
            commit: "abc123".to_string(),
        };
        let docs = WorkspaceRepoContext {
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            repo_name: "docs".to_string(),
            repo_slug: "docs".to_string(),
            branch_name: "main".to_string(),
            commit: "def456".to_string(),
        };
        let repos = vec![current.clone(), docs.clone()];
        let mut query = TraceRequest {
            workspace_id: current.workspace_id,
            repo_id: current.repo_id,
            file_type: "view".to_string(),
            file_name: "views/x.md".to_string(),
            intent_record_number: "I1".to_string(),
        };

        assert_eq!(
            select_trace_repo(&query, &current, &repos).map(|repo| repo.repo_id),
            Some(current.repo_id)
        );

        query.repo_id = docs.repo_id;
        assert_eq!(
            select_trace_repo(&query, &current, &repos).map(|repo| repo.repo_id),
            Some(docs.repo_id)
        );

        query.repo_id = Uuid::new_v4();
        assert!(select_trace_repo(&query, &current, &repos).is_none());
    }

    #[test]
    fn split_repo_scoped_file_strips_known_repo_prefix() {
        let docs = WorkspaceRepoContext {
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            repo_name: "docs".to_string(),
            repo_slug: "docs".to_string(),
            branch_name: "main".to_string(),
            commit: "abc123".to_string(),
        };
        let tinydb = WorkspaceRepoContext {
            workspace_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            repo_name: "tinydb".to_string(),
            repo_slug: "tinydb".to_string(),
            branch_name: "main".to_string(),
            commit: "def456".to_string(),
        };
        let repos = vec![docs, tinydb];

        let (repo, file_name) =
            split_repo_scoped_file(&repos, "docs/views/window-functions.md").unwrap();

        assert_eq!(repo.repo_name, "docs");
        assert_eq!(file_name, "views/window-functions.md");
        assert!(split_repo_scoped_file(&repos, "views/window-functions.md").is_none());
    }

    /// Multi-hop: pm_view.md I1 → spec I1 → code fn_init
    #[tokio::test]
    async fn multi_hop_trace_view_to_ontology_to_code() {
        let t = TestRepo::with_grounding();

        // Hop 1: pm_view.md I1 → spec children
        let grounding_files = list_grounding_jsonl_files(t.repo.clone(), t.commit.clone()).await;
        let mut hop1_children = Vec::new();
        for jsonl_path in &grounding_files {
            let edges: Vec<GroundingEdge> =
                read_jsonl_at_commit(t.repo.clone(), &t.commit, jsonl_path).await;
            for edge in edges {
                if edge.source_file == "views/pm_views/feature.pm_view.md" && edge.source_id == "I1"
                {
                    hop1_children.push(edge);
                }
            }
        }
        assert_eq!(hop1_children.len(), 2);

        // Hop 2: spec I1 → code children
        let spec_i1_edge = hop1_children.iter().find(|e| e.target_id == "I1").unwrap();
        assert_eq!(
            spec_i1_edge.target_file,
            ".semlift/specs/feature.semlift.md"
        );

        let mut hop2_children = Vec::new();
        for jsonl_path in &grounding_files {
            let edges: Vec<GroundingEdge> =
                read_jsonl_at_commit(t.repo.clone(), &t.commit, jsonl_path).await;
            for edge in edges {
                if edge.source_file == spec_i1_edge.target_file
                    && edge.source_id == spec_i1_edge.target_id
                {
                    hop2_children.push(edge);
                }
            }
        }
        assert_eq!(hop2_children.len(), 1);
        assert_eq!(hop2_children[0].target_file, "src/lib.rs");
        assert_eq!(hop2_children[0].target_id, "fn_init");

        // Resolve the code record
        let code_record = find_intent_record(
            t.repo.clone(),
            &t.commit,
            FileType::Code,
            "src/lib.rs",
            "fn_init",
        )
        .await;
        assert!(code_record.is_some());
        assert_eq!(
            code_record.unwrap().code_block_name.as_deref(),
            Some("fn_init")
        );
    }
}
