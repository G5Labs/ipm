//! `IntentLogService` — read-only projection of human-authored/edited intent
//! records into the workspace **Intent Log** audit table.
//!
//! Spec: `g5/g5-api/services/intent_log_service`. Product/test contract:
//! `views/pm_views/intent_log.pm_view.md`, `views/test_views/intent_log.test_view.md`.
//!
//! This service computes normalized rows at read time from the same
//! overlay→committed JSONL the grounding/trace route reads (no new store). It
//! reuses the grounding route's overlay-aware read + block-extraction helpers
//! so live workspace file-overlay semantics are not duplicated. Git blame fallback and the
//! durable provenance backfill are layered on in later passes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::warn;
use uuid::Uuid;

use crate::services::git::repo_store::TenantRepoStorage;
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::provenance::{
    classify_author_type, classify_blame_author_type, parse_iso, record_actor,
};
use g5_contracts::types::grounding::{FileType, GroundingEdge, IntentRecord};
use g5_contracts::types::{HistoricalGitVersion, HistoricalReadReason};
use g5_scm::{prepare_files_commit_from_exact_base, ExactBaseFilesCommit};
use g5_scm::{BlameAttribution, GitAuthor, GitRepo};
use g5_semantic::block_extraction;
use g5_semantic::parsing::block_line_range_by_label;

use crate::errors::AppError;
use crate::grounding_projection::{
    format_chat_content, list_grounding_jsonl_files_from_view, read_jsonl_files_from_view,
};
use crate::services::workspace_reader::{
    CommittedRepoReadView, HistoricalBlameRequest, RepoFileRead, RepoReadView,
};
use crate::services::WorkspaceRepoContext;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
const PREVIEW_CHARS: usize = 200;
const MAX_BLAME_CACHE_ENTRIES: usize = 4096;

// ─── Query / Response DTOs ───────────────────────────────────────────────────

/// Parsed `GET /api/v1/intent-log` query parameters. Multi-value filters are
/// comma-separated. Owned by the route as `Query<IntentLogQuery>`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentLogQuery {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub q: Option<String>,
    pub source_kind: Option<String>,
    pub intent_kind: Option<String>,
    pub author_type: Option<String>,
    pub actor: Option<String>,
    pub time_range: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub trace_status: Option<String>,
    pub artifact_type: Option<String>,
    pub repo: Option<String>,
    pub sort: Option<String>,
    pub group: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

/// One normalized Intent Log row (mirrors PM view [I14]).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentLogEntry {
    pub id: String,
    pub occurred_at: Option<String>,
    pub occurred_at_source: String, // record | git_blame | unknown
    pub actor: Option<String>,
    pub actor_source: String, // record | git_blame | unknown
    pub source_kind: String,  // intent_record | git_attribution
    pub intent_kind: String,  // external_doc | chat | view | ontology
    pub author_type: String,  // human | agent | supervisor_agent | system | unknown
    pub artifact_type: Option<String>,
    pub artifact_path: Option<String>,
    pub block_id: Option<String>,
    pub content_preview: String,
    pub content: Option<String>,
    /// Block title from canonical extraction (non-chat rows; null when the
    /// source is unresolvable).
    pub title: Option<String>,
    /// Normalized block label from canonical extraction.
    pub block_label: Option<String>,
    pub is_human: bool,
    pub trace_key: Option<TraceKey>,
    pub trace_status: String, // traceable | unlinked | unregistered | not_applicable | unknown
    pub raw_source: RawSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceKey {
    pub file_type: String,
    pub file_name: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Extra source paths kept when duplicates collapse (dedup provenance).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub duplicate_paths: Vec<String>,
}

/// A single malformed/unreadable source diagnostic ([D17]).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDiagnostic {
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub reason: String,
}

/// `{ value, count }` facet bucket.
#[derive(Debug, Clone, Serialize)]
pub struct FacetBucket {
    pub value: String,
    pub count: usize,
}

/// Response meta carried inside `data` (the standard `ResponseEnvelope.meta`
/// is a fixed pagination shape, so the rich Intent Log meta lives here).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentLogMeta {
    pub facets: BTreeMap<String, Vec<FacetBucket>>,
    pub cursor: Option<String>,
    pub has_more: bool,
    pub diagnostics: Vec<SourceDiagnostic>,
}

/// `data` payload: `{ rows, meta }`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentLogResponse {
    pub rows: Vec<IntentLogEntry>,
    pub meta: IntentLogMeta,
}

/// A record whose who/when metadata was filled from git blame at read time and
/// is therefore a candidate for the durable provenance backfill ([D21]).
#[derive(Debug, Clone)]
pub struct ProvenanceBackfill {
    pub repo_id: Uuid,
    pub file_type: FileType,
    pub record_path: String,
    pub file_name: String,
    pub intent_record_number: Option<String>,
    pub created_at: Option<String>,
    pub created_by: Option<String>,
}

/// Output of [`IntentLogService::project`]: the response plus any backfill
/// candidates discovered during git fallback.
#[derive(Debug, Clone)]
pub struct IntentLogProjection {
    pub response: IntentLogResponse,
    pub backfill: Vec<ProvenanceBackfill>,
}

// ─── Service ─────────────────────────────────────────────────────────────────

/// Read-only Intent Log projection service.
pub struct IntentLogService {
    repo_storage: TenantRepoStorage,
    blame_cache: Mutex<HashMap<BlameCacheKey, Option<BlameAttribution>>>,
}

/// Internal working row: the normalized entry, a parsed sort key, and an
/// optional deferred git-blame target.
struct WorkingRow {
    entry: IntentLogEntry,
    sort_time: Option<DateTime<Utc>>,
    blame: Option<BlameTarget>,
}

/// A document/view/ontology record missing who/when metadata that may be filled
/// from `git blame` over its I/D-block line range.
#[derive(Clone)]
struct BlameTarget {
    repo_id: Uuid,
    commit: String,
    file_type: FileType,
    record_path: String,
    file_name: String,
    block_label: String,
    needs_time: bool,
    needs_actor: bool,
}

/// Immutable git identity for an expensive blame lookup. Including the source
/// file prevents line-identical blocks in different artifacts from sharing
/// attribution, and including the commit makes cache invalidation automatic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BlameCacheKey {
    repo_id: Uuid,
    commit: String,
    file_name: String,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraceLinkKey {
    repo_id: Uuid,
    file_type: FileType,
    file_name: String,
    block_id: String,
}

impl IntentLogService {
    pub fn new(repo_storage: TenantRepoStorage) -> Self {
        Self {
            repo_storage,
            blame_cache: Mutex::new(HashMap::new()),
        }
    }

    fn cached_blame_result(&self, key: &BlameCacheKey) -> Option<Option<BlameAttribution>> {
        self.blame_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
    }

    fn cache_blame_result(&self, key: BlameCacheKey, value: Option<BlameAttribution>) {
        if let Ok(mut cache) = self.blame_cache.lock() {
            if cache.len() >= MAX_BLAME_CACHE_ENTRIES && !cache.contains_key(&key) {
                cache.clear();
            }
            cache.insert(key, value);
        }
    }

    /// Build the read-only projection from one complete captured authority cut.
    /// Every repository view carries its own validated live overlay and one
    /// request-stable dirty set.
    pub async fn project(
        &self,
        repo_contexts: &[WorkspaceRepoContext],
        repo_views: &BTreeMap<Uuid, RepoReadView>,
        query: &IntentLogQuery,
    ) -> Result<IntentLogProjection, AppError> {
        // 1. Build the grounding link set across all visible repos (for trace status).
        let linked = self.build_link_set(repo_contexts, repo_views).await;

        // 2. Enumerate + read intent records across repos, collecting diagnostics.
        let mut rows: Vec<WorkingRow> = Vec::new();
        let mut diagnostics: Vec<SourceDiagnostic> = Vec::new();
        let mut backfill: Vec<ProvenanceBackfill> = Vec::new();
        let mut seen: HashMap<(String, String, String, String, String), usize> = HashMap::new();

        for repo in repo_contexts {
            let Some(repo_view) = repo_views.get(&repo.repo_id) else {
                continue;
            };
            let files = self.list_intent_record_files(repo_view).await;
            let paths = files
                .iter()
                .map(|(_, path)| path.clone())
                .collect::<Vec<_>>();
            let mut record_files = repo_view
                .read_files(&self.repo_storage, &paths)
                .await
                .map_err(AppError::from_error)?;
            let mut parsed_records = Vec::new();

            for (_, path) in files {
                let (records, mut file_diags) =
                    Self::parse_record_file(&path, record_files.remove(&path));
                diagnostics.append(&mut file_diags);
                parsed_records.extend(records.into_iter().map(|record| (path.clone(), record)));
            }

            let source_paths = parsed_records
                .iter()
                .filter_map(|(_, record)| {
                    (record.file_type != FileType::Chat)
                        .then(|| record.file_name.clone())
                        .flatten()
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let source_texts = repo_view
                .read_files(&self.repo_storage, &source_paths)
                .await
                .map_err(AppError::from_error)?
                .into_iter()
                .filter_map(|(path, read)| match read {
                    RepoFileRead::Content { bytes, .. } => {
                        String::from_utf8(bytes).ok().map(|content| (path, content))
                    }
                    RepoFileRead::Deleted | RepoFileRead::Missing => None,
                })
                .collect::<BTreeMap<_, _>>();

            for (path, record) in parsed_records {
                let Some(built) =
                    self.normalize(&record, repo, &source_texts, &path, &linked, repo_contexts)
                else {
                    continue;
                };
                // Dedup by natural key AND content, preserving duplicate
                // source paths. Same key with different content remains a
                // separate audit row (PM [D9]).
                let key = dedupe_key(&built.entry);
                if let Some(existing_idx) = seen.get(&key).copied() {
                    let duplicate_path = duplicate_source_path(&built.entry);
                    if let Some(p) = duplicate_path {
                        let existing = &mut rows[existing_idx].entry;
                        if !existing.raw_source.duplicate_paths.contains(&p) {
                            existing.raw_source.duplicate_paths.push(p);
                        }
                    }
                    continue;
                }
                seen.insert(key, rows.len());
                rows.push(built);
            }
        }

        // 3. Resolve git fallback before semantic filters/facets/sort/paging so
        // git-derived who/when participates in the same audit universe as record
        // metadata. The blame cache and durable backfill amortize the cost.
        self.apply_blame(&mut rows, &mut backfill, repo_views).await;

        // 4. Filter.
        rows.retain(|r| Self::matches_filters(r, query));

        // 5. Facets over the filtered universe (before pagination).
        let facets = Self::compute_facets(&rows);

        // 6. Sort (default: by time desc, unknown last).
        Self::sort_rows(&mut rows, query.sort.as_deref());

        // 7. Cursor pagination.
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = query.cursor.as_deref().and_then(decode_cursor).unwrap_or(0);
        let total = rows.len();
        let page_rows: Vec<WorkingRow> = rows.into_iter().skip(offset).take(limit).collect();
        let page: Vec<IntentLogEntry> = page_rows.into_iter().map(|r| r.entry).collect();
        let next_offset = offset + page.len();
        let has_more = next_offset < total;
        let cursor = if has_more {
            Some(encode_cursor(next_offset))
        } else {
            None
        };

        Ok(IntentLogProjection {
            response: IntentLogResponse {
                rows: page,
                meta: IntentLogMeta {
                    facets,
                    cursor,
                    has_more,
                    diagnostics,
                },
            },
            backfill,
        })
    }

    /// Select repo branches that can safely receive provenance-only commits.
    /// Branches with in-flight work or an attached workspace runtime are skipped.
    pub async fn backfill_writable_repo_ids(
        &self,
        db: &mut ScopedDb,
        repo_contexts: &[WorkspaceRepoContext],
    ) -> HashSet<Uuid> {
        let mut writable = HashSet::new();
        for repo in repo_contexts {
            match self.has_in_flight_work(db, repo.workspace_id).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(error = %error, workspace_id = %repo.workspace_id, "intent-log backfill: in-flight work check failed");
                    continue;
                }
            }

            match self.has_attached_runtime(db, repo.workspace_id).await {
                Ok(false) => {
                    writable.insert(repo.repo_id);
                }
                Ok(true) => {}
                Err(error) => {
                    tracing::warn!(error = %error, workspace_id = %repo.workspace_id, "intent-log backfill: runtime check failed");
                }
            }
        }
        writable
    }

    /// Durably backfill git-derived provenance into the intent-record JSONL of
    /// **every owning repo** and commit it on each repo's branch ([D21]).
    /// Best-effort and idempotent (skips when nothing is missing), bounded to
    /// view/ontology records. Changed rows retain unknown historical fields;
    /// untouched and malformed lines are preserved verbatim.
    ///
    /// `writable_repo_ids` restricts preparation to branches already selected
    /// by [`Self::backfill_writable_repo_ids`]. Returns the prepared commit per
    /// changed repo as `(repo_id, repo_name, new_head)`; no canonical
    /// ref is moved. The caller submits the complete cut to the authority
    /// finalizer, whose exact-head CAS handles concurrent advancement.
    pub async fn prepare_provenance_backfill(
        &self,
        repo_contexts: &[WorkspaceRepoContext],
        repo_views: &BTreeMap<Uuid, CommittedRepoReadView>,
        candidates: &[ProvenanceBackfill],
        writable_repo_ids: &HashSet<Uuid>,
    ) -> Vec<(Uuid, String, String)> {
        // Group candidates by owning repo. External-doc records may live under
        // legacy `external_design_doc` paths, so each candidate carries the exact
        // source JSONL path that produced the row.
        let mut by_repo: HashMap<Uuid, Vec<ProvenanceBackfill>> = HashMap::new();
        for c in candidates {
            if matches!(
                c.file_type,
                FileType::ExternalDoc | FileType::View | FileType::Ontology
            ) {
                by_repo.entry(c.repo_id).or_default().push(c.clone());
            }
        }

        let mut prepared: Vec<(Uuid, String, String)> = Vec::new();
        for repo in repo_contexts {
            if !writable_repo_ids.contains(&repo.repo_id) {
                continue;
            }
            let Some(cands) = by_repo.get(&repo.repo_id).cloned() else {
                continue;
            };
            if cands.is_empty() {
                continue;
            }
            let Some(repo_view) = repo_views.get(&repo.repo_id) else {
                continue;
            };
            let Ok(repo_path) = self.repo_storage.path(repo_view.key()) else {
                continue;
            };
            if repo_view.head_commit() != repo.commit {
                tracing::warn!(
                    repo_id = %repo.repo_id,
                    captured_head = %repo_view.head_commit(),
                    requested_head = %repo.commit,
                    "intent-log backfill: captured authority member no longer matches candidate cut"
                );
                continue;
            }
            let record_paths = cands
                .iter()
                .map(|candidate| candidate.record_path.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let base_files = match repo_view
                .read_files(&self.repo_storage, &record_paths)
                .await
            {
                Ok(files) => files,
                Err(error) => {
                    tracing::warn!(error = %error, repo_id = %repo.repo_id, "intent-log backfill: committed batch read failed");
                    continue;
                }
            };
            let base_commit = repo.commit.clone();
            let repo_id = repo.repo_id;
            let repo_name = repo.repo_name.clone();

            let joined = tokio::task::spawn_blocking(move || {
                prepare_repo_provenance(&repo_path, &base_commit, base_files, cands)
            })
            .await;

            match joined {
                Ok(Ok(Some(head))) => prepared.push((repo_id, repo_name, head)),
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    tracing::warn!(error = %error, repo_id = %repo.repo_id, "intent-log backfill: commit failed for repo");
                }
                Err(error) => {
                    tracing::warn!(error = %error, repo_id = %repo.repo_id, "intent-log backfill: blocking task join failed");
                }
            }
        }
        prepared
    }

    async fn has_attached_runtime(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<bool, AppError> {
        sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workspace_vms vm
                WHERE vm.workspace_id = $1
                  AND vm.status IN ('starting', 'running', 'stopping')
            )
            "#,
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(AppError::from)
    }

    async fn has_in_flight_work(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM tasks t
                WHERE t.workspace_id = $1 AND t.kind <> 'preview'
                  AND t.status IN ('queued', 'running', 'waiting')
            )
            "#,
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
    }

    /// Build the set of repo-owner-aware keys that participate in any grounding
    /// edge (either endpoint), used to classify trace status.
    async fn build_link_set(
        &self,
        repo_contexts: &[WorkspaceRepoContext],
        repo_views: &BTreeMap<Uuid, RepoReadView>,
    ) -> HashSet<TraceLinkKey> {
        let mut linked: HashSet<TraceLinkKey> = HashSet::new();
        for repo in repo_contexts {
            let Some(repo_view) = repo_views.get(&repo.repo_id) else {
                continue;
            };
            let files = list_grounding_jsonl_files_from_view(&self.repo_storage, repo_view).await;
            let corpus: BTreeMap<String, Vec<GroundingEdge>> =
                read_jsonl_files_from_view(&self.repo_storage, repo_view, &files)
                    .await
                    .unwrap_or_default();
            for edges in corpus.into_values() {
                for edge in edges {
                    linked.insert(trace_link_key_for_endpoint(
                        repo_contexts,
                        repo,
                        edge.source_file_type,
                        &edge.source_file,
                        &edge.source_id,
                    ));
                    linked.insert(trace_link_key_for_endpoint(
                        repo_contexts,
                        repo,
                        edge.target_file_type,
                        &edge.target_file,
                        &edge.target_id,
                    ));
                }
            }
        }
        linked
    }

    /// Enumerate committed `intent_record/<sub>/*.jsonl` paths. Excludes `code`.
    async fn list_intent_record_files(&self, repo_view: &RepoReadView) -> Vec<(FileType, String)> {
        let mut out: Vec<(FileType, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        if let Ok(entries) = repo_view
            .list_committed_tree(&self.repo_storage, "intent_record", 32)
            .await
        {
            for entry in entries {
                if !entry.path.ends_with(".jsonl") || !entry.path.starts_with("intent_record/") {
                    continue;
                }
                let subfolder = entry
                    .path
                    .strip_prefix("intent_record/")
                    .and_then(|rest| rest.split('/').next())
                    .unwrap_or_default();
                if let Some(file_type) = subfolder_to_file_type(subfolder) {
                    if seen.insert(entry.path.clone()) {
                        out.push((file_type, entry.path));
                    }
                }
            }
        }

        out
    }

    /// Read intent records from one JSONL file (overlay→committed) while
    /// recording malformed lines as diagnostics ([D17]).
    fn parse_record_file(
        path: &str,
        read: Option<RepoFileRead>,
    ) -> (Vec<IntentRecord>, Vec<SourceDiagnostic>) {
        let mut records = Vec::new();
        let mut diags = Vec::new();
        let Some(RepoFileRead::Content { bytes, .. }) = read else {
            return (records, diags);
        };
        let Ok(text) = String::from_utf8(bytes) else {
            diags.push(SourceDiagnostic {
                source_path: path.to_string(),
                line: None,
                reason: "intent-record JSONL is not UTF-8".to_string(),
            });
            return (records, diags);
        };
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            match serde_json::from_str::<IntentRecord>(trimmed) {
                Ok(record) => records.push(record),
                Err(e) => diags.push(SourceDiagnostic {
                    source_path: path.to_string(),
                    line: Some(idx + 1),
                    reason: format!("malformed JSONL: {e}"),
                }),
            }
        }
        (records, diags)
    }

    /// Normalize one record into an `IntentLogEntry`. Returns `None` for records
    /// that cannot be represented (no identity).
    #[allow(clippy::too_many_arguments)]
    fn normalize(
        &self,
        record: &IntentRecord,
        repo: &WorkspaceRepoContext,
        source_texts: &BTreeMap<String, String>,
        record_path: &str,
        linked: &HashSet<TraceLinkKey>,
        repo_contexts: &[WorkspaceRepoContext],
    ) -> Option<WorkingRow> {
        let block_id = record.intent_record_number.clone();
        let (intent_kind, artifact_type) = classify(record);
        let author_type = classify_author_type(record);

        // Path / session identity.
        let trace_file_name = trace_file_name(record).map(str::to_string);
        let artifact_path = record.file_name.clone().or_else(|| {
            if record.file_type == FileType::Chat {
                trace_file_name.clone()
            } else {
                None
            }
        });
        let session_id = if record.file_type == FileType::Chat {
            trace_file_name.clone()
        } else {
            None
        };

        // Content + preview. Non-chat content is served exclusively by the
        // canonical extraction service over the exact captured read view;
        // stored non-chat JSONL bodies never become display authority.
        let mut title = None;
        let mut block_label = None;
        let content = if record.file_type == FileType::Chat {
            format_chat_content(record.chat_context.as_deref(), record.content.as_deref())
        } else if let (Some(file_name), Some(identifier)) = (
            record.file_name.as_deref(),
            record
                .intent_record_number
                .as_deref()
                .or(record.code_block_name.as_deref()),
        ) {
            source_texts.get(file_name).and_then(|text| {
                match block_extraction::extract_block_from_text(
                    record.file_type,
                    file_name,
                    identifier,
                    text,
                ) {
                    Ok(block) => Some(block),
                    Err(error) => {
                        warn!(file_name, identifier, error = %error, "Unable to extract Intent Log content");
                        None
                    }
                }
            })
            .map(|block| {
                title = block.title;
                block_label = block.label;
                block.content
            })
        } else {
            None
        };
        let content_preview =
            build_content_preview(record, block_id.as_deref(), content.as_deref());

        // Time + actor (record metadata only; git blame fills gaps in a later pass).
        let (occurred_at, occurred_at_source, sort_time) = match record
            .created_at
            .as_deref()
            .or(record.last_updated_at.as_deref())
        {
            Some(raw) => {
                let parsed = parse_iso(raw);
                let display = parsed
                    .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                    .unwrap_or_else(|| raw.to_string());
                (Some(display), "record".to_string(), parsed)
            }
            None => (None, "unknown".to_string(), None),
        };
        let (actor, actor_source) = record_actor(record);

        // Deferred git-blame target: external-doc/view/ontology blocks missing who/when.
        let blame = match record.file_type {
            FileType::ExternalDoc | FileType::View | FileType::Ontology => {
                match (record.file_name.clone(), block_id.clone()) {
                    (Some(file_name), Some(block_label))
                        if occurred_at.is_none() || actor.is_none() =>
                    {
                        Some(BlameTarget {
                            repo_id: repo.repo_id,
                            commit: repo.commit.clone(),
                            file_type: record.file_type,
                            record_path: record_path.to_string(),
                            file_name,
                            block_label,
                            needs_time: occurred_at.is_none(),
                            needs_actor: actor.is_none(),
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        };

        // Trace status from the link set.
        let trace_status = self.trace_status(record, linked, repo, repo_contexts);

        let trace_key = match (trace_file_name, block_id.clone()) {
            (Some(file_name), Some(id)) => Some(TraceKey {
                file_type: record.file_type.to_string(),
                file_name,
                id,
            }),
            _ => None,
        };

        let source_kind = if blame.is_some() {
            "git_attribution"
        } else {
            "intent_record"
        };

        let entry = IntentLogEntry {
            id: intent_log_row_uuid(repo.repo_id, record, content.as_deref()).to_string(),
            occurred_at,
            occurred_at_source,
            actor,
            actor_source,
            source_kind: source_kind.to_string(),
            intent_kind,
            author_type,
            artifact_type,
            artifact_path,
            block_id: block_id.clone(),
            content_preview,
            content,
            title,
            block_label,
            is_human: record.is_human,
            trace_key,
            trace_status,
            raw_source: RawSource {
                source_type: record.file_type.to_string(),
                id: block_id.unwrap_or_default(),
                repo_id: Some(repo.repo_id.to_string()),
                repo_name: Some(repo.repo_name.clone()),
                repo_slug: Some(repo.repo_slug.clone()),
                session_id,
                commit_sha: Some(repo.commit.clone()),
                duplicate_paths: Vec::new(),
            },
        };

        Some(WorkingRow {
            entry,
            sort_time,
            blame,
        })
    }

    fn trace_status(
        &self,
        record: &IntentRecord,
        linked: &HashSet<TraceLinkKey>,
        repo: &WorkspaceRepoContext,
        repo_contexts: &[WorkspaceRepoContext],
    ) -> String {
        let Some(block) = record.intent_record_number.as_deref() else {
            return "not_applicable".to_string();
        };
        let Some(file_name) = trace_file_name(record) else {
            return "unknown".to_string();
        };
        let key =
            trace_link_key_for_endpoint(repo_contexts, repo, record.file_type, file_name, block);
        if linked.contains(&key) {
            "traceable".to_string()
        } else {
            "unlinked".to_string()
        }
    }

    fn matches_filters(row: &WorkingRow, query: &IntentLogQuery) -> bool {
        let entry = &row.entry;
        if let Some(q) = query.q.as_deref() {
            if !q.trim().is_empty() && !entry_matches_search(entry, q) {
                return false;
            }
        }
        if let Some(kinds) = csv(query.source_kind.as_deref()) {
            if !kinds.contains(&entry.source_kind) {
                return false;
            }
        }
        if let Some(kinds) = csv(query.intent_kind.as_deref()) {
            if !kinds.contains(&entry.intent_kind) {
                return false;
            }
        }
        let author_types =
            csv(query.author_type.as_deref()).unwrap_or_else(|| vec!["human".to_string()]);
        if !author_types.contains(&entry.author_type) {
            return false;
        }
        if let Some(actors) = csv(query.actor.as_deref()) {
            match &entry.actor {
                Some(a) if actors.contains(a) => {}
                _ => return false,
            }
        }
        if let Some(statuses) = csv(query.trace_status.as_deref()) {
            if !statuses.contains(&entry.trace_status) {
                return false;
            }
        }
        if let Some(types) = csv(query.artifact_type.as_deref()) {
            match &entry.artifact_type {
                Some(t) if types.contains(t) => {}
                _ => return false,
            }
        }
        if !matches_time_filter(row, query) {
            return false;
        }
        if !matches_repo_filter(entry, query.repo.as_deref()) {
            return false;
        }
        true
    }

    fn compute_facets(rows: &[WorkingRow]) -> BTreeMap<String, Vec<FacetBucket>> {
        let mut facets: BTreeMap<String, HashMap<String, usize>> = BTreeMap::new();
        for row in rows {
            let e = &row.entry;
            *facets
                .entry("intentKind".to_string())
                .or_default()
                .entry(e.intent_kind.clone())
                .or_default() += 1;
            *facets
                .entry("authorType".to_string())
                .or_default()
                .entry(e.author_type.clone())
                .or_default() += 1;
            *facets
                .entry("sourceKind".to_string())
                .or_default()
                .entry(e.source_kind.clone())
                .or_default() += 1;
            *facets
                .entry("traceStatus".to_string())
                .or_default()
                .entry(e.trace_status.clone())
                .or_default() += 1;
            if let Some(actor) = &e.actor {
                *facets
                    .entry("actor".to_string())
                    .or_default()
                    .entry(actor.clone())
                    .or_default() += 1;
            }
            if let Some(t) = &e.artifact_type {
                *facets
                    .entry("artifactType".to_string())
                    .or_default()
                    .entry(t.clone())
                    .or_default() += 1;
            }
            let repo_value = e
                .raw_source
                .repo_slug
                .as_ref()
                .or(e.raw_source.repo_name.as_ref())
                .or(e.raw_source.repo_id.as_ref());
            if let Some(repo) = repo_value {
                *facets
                    .entry("repo".to_string())
                    .or_default()
                    .entry(repo.clone())
                    .or_default() += 1;
            }
            let day = row
                .sort_time
                .map(|dt| dt.date_naive().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            *facets
                .entry("day".to_string())
                .or_default()
                .entry(day)
                .or_default() += 1;
        }
        facets
            .into_iter()
            .map(|(k, counts)| {
                let mut buckets: Vec<FacetBucket> = counts
                    .into_iter()
                    .map(|(value, count)| FacetBucket { value, count })
                    .collect();
                buckets.sort_by(|a, b| b.count.cmp(&a.count).then(a.value.cmp(&b.value)));
                (k, buckets)
            })
            .collect()
    }

    fn sort_rows(rows: &mut [WorkingRow], sort: Option<&str>) {
        match sort.unwrap_or("when") {
            "actor" => rows.sort_by(|a, b| a.entry.actor.cmp(&b.entry.actor)),
            "kind" => rows.sort_by(|a, b| a.entry.intent_kind.cmp(&b.entry.intent_kind)),
            "file" => rows.sort_by(|a, b| a.entry.artifact_path.cmp(&b.entry.artifact_path)),
            "trace" => rows.sort_by(|a, b| a.entry.trace_status.cmp(&b.entry.trace_status)),
            // "when" (default): newest first, unknown (None) sorts last.
            _ => rows.sort_by(|a, b| match (a.sort_time, b.sort_time) {
                (Some(x), Some(y)) => y.cmp(&x),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }),
        }
    }

    /// Git blame fallback ([D18]). For each external-doc/view/ontology row
    /// missing who/when metadata, locate its I/D-block line range in the
    /// committed file and blame it; fill `git_blame` values and record a
    /// provenance-backfill candidate. Ambiguous/unlocatable → unknown.
    async fn apply_blame(
        &self,
        rows: &mut [WorkingRow],
        backfill: &mut Vec<ProvenanceBackfill>,
        repo_views: &BTreeMap<Uuid, RepoReadView>,
    ) {
        let mut memo: HashMap<BlameCacheKey, Option<BlameAttribution>> = HashMap::new();
        let mut requested_files: BTreeMap<(Uuid, String), BTreeSet<String>> = BTreeMap::new();
        for row in rows.iter() {
            if let Some(target) = row.blame.as_ref() {
                requested_files
                    .entry((target.repo_id, target.commit.clone()))
                    .or_default()
                    .insert(target.file_name.clone());
            }
        }

        let mut committed_texts: BTreeMap<(Uuid, String, String), String> = BTreeMap::new();
        let mut authorized_versions: BTreeMap<(Uuid, String), CommittedRepoReadView> =
            BTreeMap::new();
        for ((repo_id, commit), paths) in requested_files {
            // Blame reads only repositories of the workspace's own cut, so a
            // stored target never names another repository's storage.
            let Some(view) = repo_views.get(&repo_id) else {
                continue;
            };
            let authorized = if view.committed_view().head_commit() == commit {
                view.committed_view().clone()
            } else {
                match view
                    .committed_view()
                    .historical_sibling(&HistoricalGitVersion {
                        repo_id,
                        head_commit: commit.clone(),
                        reason: HistoricalReadReason::BlameAttribution,
                    }) {
                    Ok(view) => view,
                    Err(_) => continue,
                }
            };
            let paths = paths.into_iter().collect::<Vec<_>>();
            let files = match authorized.read_files(&self.repo_storage, &paths).await {
                Ok(files) => files,
                Err(error) => {
                    tracing::warn!(error = %error, repo_id = %repo_id, commit = %commit, "intent-log blame: committed batch read failed");
                    continue;
                }
            };
            authorized_versions.insert((repo_id, commit.clone()), authorized);
            for (path, bytes) in files {
                if let Some(text) = bytes.and_then(|bytes| String::from_utf8(bytes).ok()) {
                    committed_texts.insert((repo_id, commit.clone(), path), text);
                }
            }
        }

        let mut row_blame_keys = vec![None; rows.len()];
        let mut pending_keys = HashSet::new();
        let mut pending_by_version: BTreeMap<(Uuid, String), Vec<BlameCacheKey>> = BTreeMap::new();
        for (index, row) in rows.iter().enumerate() {
            let Some(target) = row.blame.as_ref() else {
                continue;
            };
            // Blame is over committed history; the block must exist in the
            // committed file (overlay-only blocks cannot be attributed).
            let Some(text) = committed_texts.get(&(
                target.repo_id,
                target.commit.clone(),
                target.file_name.clone(),
            )) else {
                continue;
            };
            let Some((start, end)) = block_line_range_by_label(&target.block_label, text) else {
                continue;
            };
            let key = BlameCacheKey {
                repo_id: target.repo_id,
                commit: target.commit.clone(),
                file_name: target.file_name.clone(),
                start_line: start,
                end_line: end,
            };
            row_blame_keys[index] = Some(key.clone());
            if memo.contains_key(&key) {
                continue;
            }
            if let Some(cached) = self.cached_blame_result(&key) {
                memo.insert(key, cached);
                continue;
            }
            let version = (key.repo_id, key.commit.clone());
            if authorized_versions.contains_key(&version) && pending_keys.insert(key.clone()) {
                pending_by_version.entry(version).or_default().push(key);
            }
        }

        for (version, keys) in pending_by_version {
            let Some(authorized) = authorized_versions.get(&version) else {
                continue;
            };
            let requests = keys
                .iter()
                .map(|key| HistoricalBlameRequest {
                    path: key.file_name.clone(),
                    start_line: key.start_line,
                    end_line: key.end_line,
                })
                .collect::<Vec<_>>();
            let results = match authorized.blame_ranges(&self.repo_storage, &requests).await {
                Ok(results) => results,
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        repo_id = %version.0,
                        commit = %version.1,
                        "intent-log blame: exact-version batch failed"
                    );
                    continue;
                }
            };
            for (key, result) in keys.into_iter().zip(results) {
                let resolved = match result {
                    Ok(attribution) => attribution,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            repo_id = %key.repo_id,
                            commit = %key.commit,
                            path = %key.file_name,
                            "intent-log blame: range attribution failed"
                        );
                        None
                    }
                };
                self.cache_blame_result(key.clone(), resolved.clone());
                memo.insert(key, resolved);
            }
        }

        for (row, blame_key) in rows.iter_mut().zip(row_blame_keys) {
            let Some(target) = row.blame.clone() else {
                continue;
            };
            let Some(key) = blame_key else {
                continue;
            };
            let Some(attr) = memo.get(&key).and_then(Option::as_ref) else {
                continue;
            };

            let mut filled_time = None;
            let mut filled_actor = None;
            if target.needs_time && row.entry.occurred_at.is_none() {
                row.entry.occurred_at = Some(attr.committed_at.clone());
                row.entry.occurred_at_source = "git_blame".to_string();
                row.sort_time = parse_iso(&attr.committed_at);
                filled_time = Some(attr.committed_at.clone());
            }
            if target.needs_actor && row.entry.actor.is_none() {
                row.entry.actor = Some(attr.author.clone());
                row.entry.actor_source = "git_blame".to_string();
                row.entry.author_type = classify_blame_author_type(&attr.author);
                filled_actor = Some(attr.author.clone());
            }
            if filled_time.is_some() || filled_actor.is_some() {
                backfill.push(ProvenanceBackfill {
                    repo_id: target.repo_id,
                    file_type: target.file_type,
                    record_path: target.record_path.clone(),
                    file_name: target.file_name.clone(),
                    intent_record_number: Some(target.block_label.clone()),
                    created_at: filled_time,
                    created_by: filled_actor,
                });
            }
        }
    }
}

// ─── Free helpers ────────────────────────────────────────────────────────────

fn intent_log_row_uuid(repo_id: Uuid, record: &IntentRecord, content: Option<&str>) -> Uuid {
    let mut hasher = Sha256::new();
    hash_required_field(&mut hasher, "repo_id", &repo_id.to_string());
    hash_required_field(&mut hasher, "file_type", &record.file_type.to_string());
    hash_optional_field(&mut hasher, "file_name", record.file_name.as_deref());
    hash_optional_field(
        &mut hasher,
        "intent_record_number",
        record.intent_record_number.as_deref(),
    );
    hash_optional_field(
        &mut hasher,
        "code_block_name",
        record.code_block_name.as_deref(),
    );
    hash_optional_field(
        &mut hasher,
        "chat_session_id",
        record.chat_session_id.as_deref(),
    );
    // Non-chat identity is stable across source edits and registration
    // refreshes. Chat keeps content in the hash because the stored message is
    // its identity source.
    hash_optional_field(
        &mut hasher,
        "content",
        (record.file_type == FileType::Chat)
            .then_some(content)
            .flatten(),
    );
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn hash_required_field(hasher: &mut Sha256, name: &str, value: &str) {
    hash_field_bytes(hasher, name, b"1", value.as_bytes());
}

fn hash_optional_field(hasher: &mut Sha256, name: &str, value: Option<&str>) {
    match value {
        Some(value) => hash_field_bytes(hasher, name, b"1", value.as_bytes()),
        None => hash_field_bytes(hasher, name, b"0", b""),
    }
}

fn hash_field_bytes(hasher: &mut Sha256, name: &str, marker: &[u8], value: &[u8]) {
    hasher.update(name.as_bytes());
    hasher.update([0]);
    hasher.update(marker);
    hasher.update([0]);
    hasher.update(value.len().to_le_bytes());
    hasher.update(value);
    hasher.update([0xff]);
}

fn dedupe_key(entry: &IntentLogEntry) -> (String, String, String, String, String) {
    let content_component = if entry.intent_kind == "chat" {
        entry
            .content
            .as_deref()
            .unwrap_or(entry.content_preview.as_str())
            .to_string()
    } else {
        String::new()
    };
    (
        entry.raw_source.repo_id.clone().unwrap_or_default(),
        entry.intent_kind.clone(),
        entry
            .artifact_path
            .clone()
            .or_else(|| entry.raw_source.session_id.clone())
            .unwrap_or_default(),
        entry.block_id.clone().unwrap_or_default(),
        content_component,
    )
}

fn duplicate_source_path(entry: &IntentLogEntry) -> Option<String> {
    entry
        .artifact_path
        .clone()
        .or_else(|| entry.raw_source.session_id.clone())
}

fn subfolder_to_file_type(name: &str) -> Option<FileType> {
    match name {
        "views" => Some(FileType::View),
        "ontology" => Some(FileType::Ontology),
        "chat" => Some(FileType::Chat),
        "external" | "external_design_doc" => Some(FileType::ExternalDoc),
        _ => None, // "code" and unknown dirs excluded for MVP
    }
}

/// `(intent_kind, artifact_type)` for a record (PM [D19]/[D4]).
fn classify(record: &IntentRecord) -> (String, Option<String>) {
    match record.file_type {
        FileType::View => ("view".to_string(), Some("view".to_string())),
        FileType::Ontology => ("ontology".to_string(), Some("ontology".to_string())),
        FileType::Code => ("code".to_string(), None),
        FileType::Chat => ("chat".to_string(), Some("chat_session".to_string())),
        FileType::ExternalDoc => {
            let artifact = match record.file_name.as_deref() {
                Some(p) if p.starts_with("designs/") => "design_doc",
                _ => "project_doc",
            };
            ("external_doc".to_string(), Some(artifact.to_string()))
        }
    }
}

fn trace_file_name(record: &IntentRecord) -> Option<&str> {
    match record.file_type {
        FileType::Chat => record
            .chat_session_id
            .as_deref()
            .or(record.file_name.as_deref()),
        _ => record.file_name.as_deref(),
    }
}

fn trace_link_key_for_endpoint(
    repo_contexts: &[WorkspaceRepoContext],
    default_repo: &WorkspaceRepoContext,
    file_type: FileType,
    file_name: &str,
    block_id: &str,
) -> TraceLinkKey {
    let (repo_id, file_name) =
        resolve_trace_file_owner(repo_contexts, default_repo, file_type, file_name);
    TraceLinkKey {
        repo_id,
        file_type,
        file_name,
        block_id: block_id.to_string(),
    }
}

fn resolve_trace_file_owner(
    repo_contexts: &[WorkspaceRepoContext],
    default_repo: &WorkspaceRepoContext,
    file_type: FileType,
    file_name: &str,
) -> (Uuid, String) {
    if let Some((prefix, rest)) = file_name.split_once('/') {
        if let Some(repo) = repo_contexts
            .iter()
            .find(|r| r.repo_name == prefix || r.repo_slug == prefix)
        {
            return (repo.repo_id, rest.to_string());
        }
    }

    if matches!(
        file_type,
        FileType::View | FileType::Chat | FileType::ExternalDoc
    ) {
        if let Some(docs_repo) = repo_contexts
            .iter()
            .find(|repo| repo.repo_name == "docs" || repo.repo_slug == "docs")
        {
            return (docs_repo.repo_id, file_name.to_string());
        }
    }

    (default_repo.repo_id, file_name.to_string())
}

fn entry_matches_search(entry: &IntentLogEntry, q: &str) -> bool {
    let needle = q.to_lowercase();
    let haystacks = [
        entry.content_preview.as_str(),
        entry.content.as_deref().unwrap_or(""),
        entry.actor.as_deref().unwrap_or(""),
        entry.artifact_path.as_deref().unwrap_or(""),
        entry.source_kind.as_str(),
        entry.intent_kind.as_str(),
        entry.author_type.as_str(),
        entry.block_id.as_deref().unwrap_or(""),
        entry.raw_source.repo_id.as_deref().unwrap_or(""),
        entry.raw_source.repo_name.as_deref().unwrap_or(""),
        entry.raw_source.repo_slug.as_deref().unwrap_or(""),
        entry.raw_source.commit_sha.as_deref().unwrap_or(""),
        entry.raw_source.session_id.as_deref().unwrap_or(""),
    ];
    haystacks.iter().any(|h| h.to_lowercase().contains(&needle))
}

fn matches_time_filter(row: &WorkingRow, query: &IntentLogQuery) -> bool {
    let mut from = None;
    let mut to = None;

    if let Some(raw) = query.time_range.as_deref() {
        match raw.trim().to_lowercase().as_str() {
            "day" => from = Some(Utc::now() - Duration::days(1)),
            "week" => from = Some(Utc::now() - Duration::days(7)),
            "month" => from = Some(Utc::now() - Duration::days(30)),
            "all" | "custom" | "" => {}
            _ => {}
        }
    }

    if let Some(raw) = query.from.as_deref() {
        from = parse_time_bound(raw, false).or(from);
    }
    if let Some(raw) = query.to.as_deref() {
        to = parse_time_bound(raw, true).or(to);
    }

    if from.is_none() && to.is_none() {
        return true;
    }

    let Some(time) = row.sort_time else {
        return false;
    };
    if let Some(from) = from {
        if time < from {
            return false;
        }
    }
    if let Some(to) = to {
        if time > to {
            return false;
        }
    }
    true
}

fn parse_time_bound(raw: &str, end_of_day: bool) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    parse_iso(trimmed).or_else(|| {
        let date = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").ok()?;
        let time = if end_of_day {
            NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999)?
        } else {
            NaiveTime::from_hms_opt(0, 0, 0)?
        };
        Some(DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_time(time),
            Utc,
        ))
    })
}

fn matches_repo_filter(entry: &IntentLogEntry, repo_filter: Option<&str>) -> bool {
    let Some(repos) = csv(repo_filter) else {
        return true;
    };
    let needles: Vec<String> = repos.into_iter().map(|repo| repo.to_lowercase()).collect();
    let haystacks = [
        entry.raw_source.repo_id.as_deref().unwrap_or(""),
        entry.raw_source.repo_name.as_deref().unwrap_or(""),
        entry.raw_source.repo_slug.as_deref().unwrap_or(""),
    ];
    haystacks.iter().any(|value| {
        let normalized = value.to_lowercase();
        needles.contains(&normalized)
    })
}

fn csv(value: Option<&str>) -> Option<Vec<String>> {
    let raw = value?;
    let parts: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

/// Human-meaningful `content_preview` (PM [I18], spec [D13]). Pure projection of
/// existing fields — never mutates the record. Two canonical forms:
///   - content edits (view/ontology/code/external_doc with a block id):
///     `User edit <block> to "<summary>"`
///   - chat intents: `context: <ctx>, user instruction: "<message>"`
///
/// Anything else falls back to the plain first-line preview.
fn build_content_preview(
    record: &IntentRecord,
    block_id: Option<&str>,
    content: Option<&str>,
) -> String {
    match record.file_type {
        FileType::Chat => {
            let message = preview(record.content.as_deref().unwrap_or(""));
            match record.chat_context.as_deref() {
                Some(ctx) if !ctx.trim().is_empty() => {
                    format!(
                        "context: {}, user instruction: \"{}\"",
                        preview(ctx),
                        message
                    )
                }
                _ => format!("user instruction: \"{message}\""),
            }
        }
        FileType::View | FileType::Ontology | FileType::Code | FileType::ExternalDoc => {
            match (block_id, content) {
                (Some(block), Some(text)) if !text.trim().is_empty() => {
                    format!("User edit {block} to \"{}\"", preview(text))
                }
                _ => content.map(preview).unwrap_or_default(),
            }
        }
    }
}

fn preview(content: &str) -> String {
    let first_line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let trimmed = first_line.trim();
    if trimmed.chars().count() > PREVIEW_CHARS {
        let truncated: String = trimmed.chars().take(PREVIEW_CHARS).collect();
        format!("{truncated}…")
    } else {
        trimmed.to_string()
    }
}

/// Opaque cursor = base64-url of the row offset.
fn encode_cursor(offset: usize) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(offset.to_string())
}

fn decode_cursor(cursor: &str) -> Option<usize> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .ok()?;
    String::from_utf8(bytes).ok()?.parse::<usize>().ok()
}

/// Blocking: prepare the provenance backfill for one repo from an exact base
/// as a detached commit. Fills only absent `created_at`/`created_by` on
/// matching records, rewrites only changed lines (untouched + malformed lines
/// preserved verbatim), and returns the detached commit object if anything
/// changed (else `None`). No canonical ref is moved.
fn prepare_repo_provenance(
    repo_path: &std::path::Path,
    base_commit: &str,
    base_files: BTreeMap<String, Option<Vec<u8>>>,
    candidates: Vec<ProvenanceBackfill>,
) -> Result<Option<String>, G5Error> {
    let git_repo = GitRepo::open(repo_path)?;
    let author = GitAuthor {
        name: "G5 Intent Log Backfill".to_string(),
        email: "g5@system.local".to_string(),
    };

    // Group this repo's candidates by their exact source JSONL file path.
    let mut by_file: BTreeMap<String, Vec<ProvenanceBackfill>> = BTreeMap::new();
    for c in candidates {
        by_file.entry(c.record_path.clone()).or_default().push(c);
    }

    let mut changed_files: Vec<(String, Vec<u8>)> = Vec::new();
    for (path, cands) in by_file {
        let Some(text) = base_files
            .get(&path)
            .and_then(|content| content.as_ref())
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::to_string)
        else {
            continue;
        };

        let mut out_lines: Vec<String> = Vec::new();
        let mut changed = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                out_lines.push(line.to_string());
                continue;
            }
            match serde_json::from_str::<IntentRecord>(trimmed) {
                Ok(mut rec) => {
                    let mut this_changed = false;
                    if let Some(c) = cands.iter().find(|c| {
                        rec.intent_record_number.as_deref() == c.intent_record_number.as_deref()
                            && rec.file_name.as_deref() == Some(c.file_name.as_str())
                    }) {
                        // Fill only absent fields; never overwrite.
                        if rec.created_at.is_none() {
                            if let Some(t) = &c.created_at {
                                rec.created_at = Some(t.clone());
                                this_changed = true;
                            }
                        }
                        if rec.created_by.is_none() {
                            if let Some(a) = &c.created_by {
                                rec.created_by = Some(a.clone());
                                this_changed = true;
                            }
                        }
                    }
                    if this_changed {
                        // Backfill owns provenance only. Preserve unknown historical
                        // fields, including retired assessments, in the stored row.
                        let mut value: serde_json::Value = serde_json::from_str(trimmed)
                            .expect("a parsed intent record is valid JSON");
                        if let Some(fields) = value.as_object_mut() {
                            if let Some(created_at) = &rec.created_at {
                                fields.insert(
                                    "created_at".to_string(),
                                    serde_json::json!(created_at),
                                );
                            }
                            if let Some(created_by) = &rec.created_by {
                                fields.insert(
                                    "created_by".to_string(),
                                    serde_json::json!(created_by),
                                );
                            }
                        } else {
                            // Serde also accepts positional records. Keep their
                            // existing backfill/normalization behavior; they cannot
                            // contain unknown fields that need preserving.
                            value = serde_json::to_value(&rec).map_err(|e| G5Error::Internal {
                                message: format!("serialize intent record: {e}"),
                            })?;
                        }
                        out_lines.push(serde_json::to_string(&value).map_err(|e| {
                            G5Error::Internal {
                                message: format!("serialize intent record: {e}"),
                            }
                        })?);
                        changed = true;
                    } else {
                        out_lines.push(line.to_string());
                    }
                }
                // Preserve malformed lines verbatim (never drop audit rows).
                Err(_) => out_lines.push(line.to_string()),
            }
        }
        if !changed {
            continue;
        }
        let mut content = out_lines.join("\n");
        content.push('\n');
        changed_files.push((path, content.into_bytes()));
    }
    if changed_files.is_empty() {
        return Ok(None);
    }

    let head = prepare_files_commit_from_exact_base(
        &git_repo,
        ExactBaseFilesCommit {
            base_commit,
            files: &changed_files,
            message: "chore(intent-log): backfill git-derived provenance",
            author: &author,
        },
    )?;
    Ok(Some(head))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn rec(file_type: FileType, file_name: &str, num: &str) -> IntentRecord {
        IntentRecord {
            file_type,
            file_name: Some(file_name.into()),
            intent_record_number: Some(num.into()),
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
        }
    }

    fn repo_with_id(name: &str, repo_id: Uuid) -> WorkspaceRepoContext {
        WorkspaceRepoContext {
            workspace_id: Uuid::nil(),
            repo_id,
            repo_name: name.into(),
            repo_slug: name.into(),
            branch_name: "main".into(),
            commit: "deadbeef".into(),
        }
    }

    fn test_service() -> (IntentLogService, tempfile::TempDir, tempfile::TempDir) {
        let base = tempfile::tempdir().expect("base tempdir");
        let worktree = tempfile::tempdir().expect("worktree tempdir");
        let storage = TenantRepoStorage::local_for_test(base.path().to_path_buf());
        (IntentLogService::new(storage), base, worktree)
    }

    fn author(name: &str) -> GitAuthor {
        GitAuthor {
            name: name.to_string(),
            email: format!("{}@example.test", name.to_lowercase()),
        }
    }

    fn commit_all(repo: &GitRepo, message: &str, author: &GitAuthor) -> String {
        repo.add_all().expect("stage files");
        repo.commit(message, author).expect("commit files")
    }

    fn init_git_repo() -> (GitRepo, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("git tempdir");
        let repo = GitRepo::init(dir.path(), false).expect("init git repo");
        (repo, dir)
    }

    fn row(occurred_at: Option<&str>, repo_slug: &str) -> WorkingRow {
        WorkingRow {
            entry: IntentLogEntry {
                id: "id".into(),
                occurred_at: occurred_at.map(String::from),
                occurred_at_source: "record".into(),
                actor: Some("Ziniu".into()),
                actor_source: "record".into(),
                source_kind: "intent_record".into(),
                intent_kind: "ontology".into(),
                author_type: "human".into(),
                artifact_type: Some("ontology".into()),
                artifact_path: Some(".semlift/specs/x.semlift.md".into()),
                block_id: Some("I1".into()),
                content_preview: "Initialize".into(),
                content: Some("Initialize".into()),
                title: None,
                block_label: Some("I1".into()),
                is_human: true,
                trace_key: None,
                trace_status: "unlinked".into(),
                raw_source: RawSource {
                    source_type: "ontology".into(),
                    id: "I1".into(),
                    repo_id: Some("repo-id".into()),
                    repo_name: Some(repo_slug.into()),
                    repo_slug: Some(repo_slug.into()),
                    session_id: None,
                    commit_sha: Some("deadbeef".into()),
                    duplicate_paths: Vec::new(),
                },
            },
            sort_time: occurred_at.and_then(parse_iso),
            blame: None,
        }
    }

    #[test]
    fn classify_external_doc_by_path() {
        let (kind, art) = classify(&rec(FileType::ExternalDoc, "designs/foo.md", "x"));
        assert_eq!(kind, "external_doc");
        assert_eq!(art.as_deref(), Some("design_doc"));
        let (kind, art) = classify(&rec(FileType::ExternalDoc, "aux_files/bar.md", "x"));
        assert_eq!(kind, "external_doc");
        assert_eq!(art.as_deref(), Some("project_doc"));
    }

    #[test]
    fn classify_view_ontology_chat() {
        assert_eq!(classify(&rec(FileType::View, "views/x.md", "I1")).0, "view");
        assert_eq!(
            classify(&rec(FileType::Ontology, ".semlift/specs/x.md", "I1")).0,
            "ontology"
        );
        assert_eq!(classify(&rec(FileType::Chat, "sess", "C1")).0, "chat");
    }

    #[test]
    fn classify_author_type_from_record_provenance() {
        let mut agent = rec(FileType::View, "views/x.md", "I1");
        agent.created_by = Some("Codex".into());
        assert_eq!(classify_author_type(&agent), "agent");

        let mut generated = rec(FileType::Ontology, ".semlift/specs/x.md", "I1");
        generated.is_human = false;
        assert_eq!(classify_author_type(&generated), "agent");

        let mut r = rec(FileType::Chat, "sess", "C1");
        r.created_by = Some("human_like_agent:supervisor".into());
        assert_eq!(classify(&r).0, "chat");
        assert_eq!(classify_author_type(&r), "supervisor_agent");
    }

    /// RED-4 (issue #1752): a legacy `system@g5.dev` + `is_human=true` row
    /// must NOT classify as `human`.
    #[test]
    fn red4_legacy_system_email_row_is_not_human() {
        let mut r = rec(FileType::Ontology, ".semlift/specs/x.md", "I1");
        r.created_by = Some("system@g5.dev".into());
        r.is_human = true;
        assert_ne!(classify_author_type(&r), "human");
        assert_eq!(classify_author_type(&r), "unknown");
    }

    /// Typed-row matrix (`grounding_identity_and_routing` D16): persisted
    /// `actor_kind` is authoritative and string markers never override it.
    #[test]
    fn classify_author_type_prefers_persisted_actor_kind() {
        use g5_contracts::types::grounding::ProvenanceActor;
        let typed = |kind: &str| {
            let mut r = rec(FileType::Ontology, ".semlift/specs/x.md", "I1");
            // Contradictory legacy string marker: must not override the type.
            r.created_by = Some("claude".into());
            r.is_human = false;
            r.created_actor = Some(ProvenanceActor {
                actor_kind: kind.to_string(),
                actor_id: Some("id".to_string()),
                display: Some("claude".to_string()),
                source: "platform_auth".to_string(),
            });
            r
        };
        assert_eq!(classify_author_type(&typed("user")), "human");
        assert_eq!(classify_author_type(&typed("agent")), "agent");
        assert_eq!(
            classify_author_type(&typed("supervisor_agent")),
            "supervisor_agent"
        );
        assert_eq!(classify_author_type(&typed("system")), "system");
        assert_eq!(classify_author_type(&typed("unknown")), "unknown");
    }

    /// Legacy rows without typed fields keep the heuristic unchanged —
    /// except honest `unknown` for unattributable rows.
    #[test]
    fn classify_author_type_legacy_rows_keep_heuristic_without_regression() {
        // Real human email + is_human stays human.
        let mut human = rec(FileType::View, "views/x.md", "I1");
        human.created_by = Some("engineer@example.com".into());
        assert_eq!(classify_author_type(&human), "human");

        // Agent markers stay agent; supervisor marker stays supervisor.
        let mut agent = rec(FileType::View, "views/x.md", "I1");
        agent.created_by = Some("codex".into());
        assert_eq!(classify_author_type(&agent), "agent");
        let mut supervisor = rec(FileType::Chat, "sess", "C1");
        supervisor.created_by = Some("human_like_agent:supervisor".into());
        assert_eq!(classify_author_type(&supervisor), "supervisor_agent");

        // No resolvable provenance at all renders unknown, never human.
        let empty = rec(FileType::View, "views/x.md", "I1");
        assert_eq!(classify_author_type(&empty), "unknown");
    }

    #[test]
    fn chat_trace_identity_prefers_chat_session_id() {
        let mut r = rec(FileType::Chat, "legacy-file-name", "C1");
        r.chat_session_id = Some("session-key".into());
        assert_eq!(trace_file_name(&r), Some("session-key"));

        r.chat_session_id = None;
        assert_eq!(trace_file_name(&r), Some("legacy-file-name"));
    }

    #[test]
    fn record_actor_prefers_created_by_then_last_updated_by() {
        let mut r = rec(FileType::Ontology, ".semlift/specs/foo.semlift.md", "I1");
        assert_eq!(record_actor(&r), (None, "unknown".to_string()));

        r.last_updated_by = Some("Editor".into());
        assert_eq!(
            record_actor(&r),
            (Some("Editor".to_string()), "record".to_string())
        );

        r.created_by = Some("Author".into());
        assert_eq!(
            record_actor(&r),
            (Some("Author".to_string()), "record".to_string())
        );
    }

    #[test]
    fn subfolder_mapping_excludes_code() {
        assert_eq!(subfolder_to_file_type("views"), Some(FileType::View));
        assert_eq!(subfolder_to_file_type("ontology"), Some(FileType::Ontology));
        assert_eq!(subfolder_to_file_type("chat"), Some(FileType::Chat));
        assert_eq!(
            subfolder_to_file_type("external_design_doc"),
            Some(FileType::ExternalDoc)
        );
        assert_eq!(
            subfolder_to_file_type("external"),
            Some(FileType::ExternalDoc)
        );
        assert_eq!(subfolder_to_file_type("code"), None);
    }

    #[test]
    fn trace_file_owner_resolves_repo_prefix_and_docs_owned_defaults() {
        let docs_id = Uuid::new_v4();
        let code_id = Uuid::new_v4();
        let docs = repo_with_id("docs", docs_id);
        let code = repo_with_id("code", code_id);
        let repos = vec![code.clone(), docs.clone()];

        assert_eq!(
            resolve_trace_file_owner(
                &repos,
                &code,
                FileType::Ontology,
                "docs/.semlift/specs/x.semlift.md"
            ),
            (docs_id, ".semlift/specs/x.semlift.md".to_string())
        );
        assert_eq!(
            resolve_trace_file_owner(&repos, &code, FileType::View, "views/x.md"),
            (docs_id, "views/x.md".to_string())
        );
        assert_eq!(
            resolve_trace_file_owner(&repos, &code, FileType::Ontology, ".semlift/specs/x.md"),
            (code_id, ".semlift/specs/x.md".to_string())
        );
    }

    #[test]
    fn trace_status_is_repo_scoped_for_duplicate_paths() {
        let (service, _base, _worktree) = test_service();
        let linked_repo_id = Uuid::new_v4();
        let other_repo_id = Uuid::new_v4();
        let linked_repo = repo_with_id("code", linked_repo_id);
        let other_repo = repo_with_id("other-code", other_repo_id);
        let repos = vec![linked_repo.clone(), other_repo.clone()];
        let record = rec(FileType::Ontology, ".semlift/specs/x.semlift.md", "I1");
        let linked = HashSet::from([trace_link_key_for_endpoint(
            &repos,
            &linked_repo,
            FileType::Ontology,
            ".semlift/specs/x.semlift.md",
            "I1",
        )]);

        assert_eq!(
            service.trace_status(&record, &linked, &linked_repo, &repos),
            "traceable"
        );
        assert_eq!(
            service.trace_status(&record, &linked, &other_repo, &repos),
            "unlinked"
        );
    }

    #[test]
    fn csv_splits_and_trims() {
        assert_eq!(
            csv(Some("a, b ,c")),
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
        assert_eq!(csv(Some("   ")), None);
        assert_eq!(csv(None), None);
    }

    #[test]
    fn preview_first_nonblank_line_and_truncation() {
        assert_eq!(preview("\n\nhello\nworld"), "hello");
        let long = "x".repeat(PREVIEW_CHARS + 50);
        assert!(preview(&long).ends_with('…'));
    }

    #[test]
    fn content_preview_formats_edit_and_chat() {
        // Edit record → `User edit <block> to "<summary>"`.
        let mut edit = rec(FileType::Ontology, ".semlift/specs/x.semlift.md", "D-3");
        edit.content = Some("PII handling: never log PII in plaintext".into());
        assert_eq!(
            build_content_preview(
                &edit,
                edit.intent_record_number.as_deref(),
                edit.content.as_deref()
            ),
            "User edit D-3 to \"PII handling: never log PII in plaintext\"",
        );

        // Chat record with context → `context: <ctx>, user instruction: "<msg>"`.
        let mut chat = rec(FileType::Chat, "cs-1", "C1");
        chat.chat_context = Some("agent proposed storing SSN".into());
        chat.content = Some("never store SSN in plaintext".into());
        assert_eq!(
            build_content_preview(&chat, chat.intent_record_number.as_deref(), chat.content.as_deref()),
            "context: agent proposed storing SSN, user instruction: \"never store SSN in plaintext\"",
        );

        // Chat record without context → `user instruction: "<msg>"`.
        let mut chat2 = rec(FileType::Chat, "cs-2", "C2");
        chat2.content = Some("add retries with backoff".into());
        assert_eq!(
            build_content_preview(
                &chat2,
                chat2.intent_record_number.as_deref(),
                chat2.content.as_deref()
            ),
            "user instruction: \"add retries with backoff\"",
        );

        // Record without a block id falls back to the plain preview.
        let mut noblock = rec(FileType::View, "views/x.pm_view.md", "");
        noblock.intent_record_number = None;
        noblock.content = Some("first line\nsecond".into());
        assert_eq!(
            build_content_preview(&noblock, None, noblock.content.as_deref()),
            "first line",
        );
    }

    #[test]
    fn intent_log_row_uuid_is_repo_scoped_and_non_chat_content_stable() {
        let repo_a = Uuid::new_v4();
        let repo_b = Uuid::new_v4();
        let record = rec(FileType::Ontology, ".semlift/specs/x.semlift.md", "I1");

        let first = intent_log_row_uuid(repo_a, &record, Some("same content"));
        let second = intent_log_row_uuid(repo_a, &record, Some("same content"));
        let other_repo = intent_log_row_uuid(repo_b, &record, Some("same content"));
        let other_content = intent_log_row_uuid(repo_a, &record, Some("different content"));

        assert_eq!(first, second);
        assert_ne!(first, other_repo);
        assert_eq!(first, other_content);

        let chat = rec(FileType::Chat, "chat/session.jsonl", "I1");
        let chat_first = intent_log_row_uuid(repo_a, &chat, Some("first message"));
        let chat_second = intent_log_row_uuid(repo_a, &chat, Some("second message"));
        assert_ne!(chat_first, chat_second);
    }

    #[test]
    fn parse_iso_handles_both_spellings() {
        assert!(parse_iso("2026-06-01T12:00:00Z").is_some());
        assert!(parse_iso("2026-05-26T14:54:07.628594+00:00").is_some());
        assert!(parse_iso("not a date").is_none());
    }

    #[test]
    fn matches_filters_supports_time_range_and_repo() {
        let in_range = row(Some("2026-06-01T12:00:00Z"), "g5");
        let out_of_range = row(Some("2026-04-01T12:00:00Z"), "g5");
        let unknown_time = row(None, "g5");

        let query = IntentLogQuery {
            time_range: Some("custom".into()),
            author_type: Some("human".into()),
            from: Some("2026-05-01".into()),
            to: Some("2026-06-02".into()),
            repo: Some("g5".into()),
            ..Default::default()
        };

        assert!(IntentLogService::matches_filters(&in_range, &query));
        assert!(!IntentLogService::matches_filters(&out_of_range, &query));
        assert!(!IntentLogService::matches_filters(&unknown_time, &query));

        let wrong_repo = row(Some("2026-06-01T12:00:00Z"), "other");
        assert!(!IntentLogService::matches_filters(&wrong_repo, &query));

        let partial_repo = IntentLogQuery {
            repo: Some("g".into()),
            ..query
        };
        assert!(
            !IntentLogService::matches_filters(&in_range, &partial_repo),
            "repo filter must not substring-match another repo id/name/slug"
        );
    }

    #[test]
    fn matches_filters_defaults_to_human_author_type() {
        let human = row(Some("2026-06-01T12:00:00Z"), "g5");
        let mut agent = row(Some("2026-06-01T12:00:00Z"), "g5");
        agent.entry.author_type = "agent".into();
        let default_query = IntentLogQuery {
            workspace_id: Uuid::nil(),
            ..Default::default()
        };
        assert!(IntentLogService::matches_filters(&human, &default_query));
        assert!(!IntentLogService::matches_filters(&agent, &default_query));

        let agent_query = IntentLogQuery {
            workspace_id: Uuid::nil(),
            author_type: Some("agent".into()),
            ..Default::default()
        };
        assert!(!IntentLogService::matches_filters(&human, &agent_query));
        assert!(IntentLogService::matches_filters(&agent, &agent_query));
    }

    #[test]
    fn block_line_range_spans_block() {
        let text = "[I1. First]\nbody line\nmore body\n[I2. Second]\nother\n";
        assert_eq!(block_line_range_by_label("I1", text), Some((1, 3)));
        assert_eq!(block_line_range_by_label("I9", text), None);
    }

    #[test]
    fn blame_cache_key_distinguishes_file_names_across_same_head() {
        let (service, _base, _worktree) = test_service();
        let repo_id = Uuid::new_v4();
        let head = "a".repeat(40);
        let a_key = BlameCacheKey {
            repo_id,
            commit: head.clone(),
            file_name: "a.md".to_string(),
            start_line: 1,
            end_line: 2,
        };
        let b_key = BlameCacheKey {
            repo_id,
            commit: head,
            file_name: "b.md".to_string(),
            start_line: 1,
            end_line: 2,
        };
        service.cache_blame_result(
            a_key.clone(),
            Some(BlameAttribution {
                author: "Alice".to_string(),
                committed_at: "2026-06-01T12:00:00Z".to_string(),
            }),
        );
        service.cache_blame_result(
            b_key.clone(),
            Some(BlameAttribution {
                author: "Bob".to_string(),
                committed_at: "2026-06-02T12:00:00Z".to_string(),
            }),
        );
        let a = service
            .cached_blame_result(&a_key)
            .flatten()
            .expect("a.md attribution");
        let b = service
            .cached_blame_result(&b_key)
            .flatten()
            .expect("b.md attribution");

        assert_eq!(a.author, "Alice");
        assert_eq!(b.author, "Bob");
    }

    #[test]
    fn blame_cache_is_bounded_and_commit_keyed() {
        let (service, _base, _worktree) = test_service();

        for idx in 0..MAX_BLAME_CACHE_ENTRIES {
            service.blame_cache.lock().expect("cache lock").insert(
                BlameCacheKey {
                    repo_id: Uuid::nil(),
                    commit: format!("commit-{idx}"),
                    file_name: "spec.md".to_string(),
                    start_line: 1,
                    end_line: 1,
                },
                None,
            );
        }

        let key = BlameCacheKey {
            repo_id: Uuid::nil(),
            commit: "b".repeat(40),
            file_name: "spec.md".to_string(),
            start_line: 1,
            end_line: 2,
        };
        service.cache_blame_result(
            key.clone(),
            Some(BlameAttribution {
                author: "Alice".to_string(),
                committed_at: "2026-06-01T12:00:00Z".to_string(),
            }),
        );
        let attr = service
            .cached_blame_result(&key)
            .flatten()
            .expect("spec attribution");

        assert_eq!(attr.author, "Alice");
        assert_eq!(service.blame_cache.lock().expect("cache lock").len(), 1);
    }

    #[test]
    fn cursor_roundtrips() {
        let c = encode_cursor(42);
        assert_eq!(decode_cursor(&c), Some(42));
        assert_eq!(decode_cursor("!!!not-base64!!!"), None);
    }

    #[test]
    fn prepare_repo_provenance_uses_exact_base_without_advancing_branch() {
        let (git_repo, dir) = init_git_repo();
        let writer = author("Alice");
        let spec = ".semlift/specs/provenance.semlift.md";
        fs::create_dir_all(dir.path().join(".semlift/specs")).expect("spec dir");
        fs::create_dir_all(dir.path().join("intent_record/ontology")).expect("record dir");
        fs::write(
            dir.path().join(spec),
            "[I1. Missing provenance]\nBackfill this block from git blame.\n",
        )
        .expect("write spec");
        let record_path = format!(
            "intent_record/ontology/{}.jsonl",
            g5_semantic::local_grounding::jsonl_store::encode_file_name(spec)
        );
        // Serde also accepts positional records. Their existing provenance
        // backfill must still run before a later edit changes git blame.
        let unusual_row = serde_json::json!([
            "ontology", spec, "I1", null, null, null, null, null, true, false, null, null, null,
            null, null, null, null, null, null
        ])
        .to_string();
        assert!(serde_json::from_str::<IntentRecord>(&unusual_row).is_ok());
        fs::write(
            dir.path().join(&record_path),
            serde_json::json!({
                "file_type": "ontology",
                "file_name": spec,
                "intent_record_number": "I1",
                "is_human": true,
                "review_metadata": {"retired": "unrecognized schema"},
                "review_action_state": ["obsolete"]
            })
            .to_string()
                + "\n"
                + &unusual_row
                + "\nmalformed history row\n",
        )
        .expect("write record");
        let base = commit_all(&git_repo, "seed provenance record", &writer);
        let base_files = BTreeMap::from([(
            record_path.clone(),
            Some(fs::read(dir.path().join(&record_path)).expect("read base record")),
        )]);

        fs::write(dir.path().join("README.md"), "branch advanced\n").expect("advance branch");
        let advanced = commit_all(&git_repo, "advance branch", &writer);
        let branch_name = g5_scm::workspace::current_branch(&git_repo).expect("current branch");
        let prepared = prepare_repo_provenance(
            dir.path(),
            &base,
            base_files,
            vec![ProvenanceBackfill {
                repo_id: Uuid::new_v4(),
                file_type: FileType::Ontology,
                record_path: record_path.clone(),
                file_name: spec.to_string(),
                intent_record_number: Some("I1".to_string()),
                created_at: Some("2026-06-01T12:00:00Z".to_string()),
                created_by: Some("Alice".to_string()),
            }],
        )
        .expect("prepare provenance commit")
        .expect("changed provenance commit");

        assert_eq!(
            git_repo.resolve_branch_head(&branch_name).unwrap(),
            advanced
        );
        assert!(git_repo.commit_exists(&prepared));
        assert!(git_repo.is_ancestor_of(&base, &prepared).unwrap());
        assert!(!git_repo.is_ancestor_of(&advanced, &prepared).unwrap());
        let stored = git_repo.show_file(&prepared, &record_path).unwrap();
        let stored = String::from_utf8(stored).unwrap();
        let mut lines = stored.lines();
        let row: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(row["created_by"], "Alice");
        assert_eq!(row["created_at"], "2026-06-01T12:00:00Z");
        assert_eq!(
            row["review_metadata"],
            serde_json::json!({"retired": "unrecognized schema"})
        );
        assert_eq!(row["review_action_state"], serde_json::json!(["obsolete"]));
        let positional: IntentRecord = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(positional.file_name.as_deref(), Some(spec));
        assert_eq!(positional.intent_record_number.as_deref(), Some("I1"));
        assert!(positional.is_human);
        assert_eq!(positional.created_by.as_deref(), Some("Alice"));
        assert_eq!(
            positional.created_at.as_deref(),
            Some("2026-06-01T12:00:00Z")
        );
        assert_eq!(lines.collect::<Vec<_>>(), ["malformed history row"]);
    }
}
