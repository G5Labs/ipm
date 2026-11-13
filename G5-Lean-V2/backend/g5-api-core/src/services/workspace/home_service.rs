//! Organization Home read projection. This service assembles compact,
//! read-only attention items from existing approval and question authority.

use std::cmp::Ordering;
use std::collections::HashMap;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::approval_config_service::ApprovalConfigService;
use crate::services::approval_definition_service::ApprovalDefinitionService;
use crate::services::doc_service::DocService;
use crate::services::workspace_service::{CapturedHomeQuestionCandidates, WorkspaceService};

pub const DEFAULT_HOME_ATTENTION_LIMIT: usize = 20;
pub const MAX_HOME_ATTENTION_LIMIT: usize = 50;

#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeCandidateObservation {
    pub org_id: Uuid,
    pub candidate_limit: usize,
    pub approval_count: usize,
    pub project_doc_count: usize,
    pub merged_count: usize,
}

#[cfg(debug_assertions)]
static HOME_CANDIDATE_OBSERVATIONS: std::sync::LazyLock<
    std::sync::Mutex<Vec<HomeCandidateObservation>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

#[cfg(debug_assertions)]
fn observe_home_candidates(observation: HomeCandidateObservation) {
    HOME_CANDIDATE_OBSERVATIONS
        .lock()
        .expect("Home candidate observer lock")
        .push(observation);
}

/// Drain one organization's observations, leaving those of reads running
/// beside it (parallel tests share this record).
#[cfg(debug_assertions)]
pub fn take_home_candidate_observations(org_id: Uuid) -> Vec<HomeCandidateObservation> {
    let mut observations = HOME_CANDIDATE_OBSERVATIONS
        .lock()
        .expect("Home candidate observer lock");
    let (taken, kept) = std::mem::take(&mut *observations)
        .into_iter()
        .partition(|observation| observation.org_id == org_id);
    *observations = kept;
    taken
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeAttentionKind {
    Approval,
    OpenQuestion,
}

impl HomeAttentionKind {
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Approval => 0,
            Self::OpenQuestion => 1,
        }
    }
}

/// Page order of the Home attention list over its one total key
/// `(actionable_at, kind_rank, workspace_id, id)` (g5.ui.home-attention-order):
/// Oldest walks it ascending, Newest is exactly its reverse
/// (g5.ui.home-attention-order-reverse). A read that names no order keeps the
/// oldest-first order the API always had (g5.ui.home-attention-order-api-default);
/// the Home UI always names one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeAttentionOrder {
    #[default]
    Oldest,
    Newest,
}

impl HomeAttentionOrder {
    /// Compare two keys by their place in this order: `Less` lists `left`
    /// first. Every source and the merge order through this one rule.
    pub(crate) fn compare<K: Ord + ?Sized>(self, left: &K, right: &K) -> Ordering {
        match self {
            Self::Oldest => left.cmp(right),
            Self::Newest => right.cmp(left),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HomeAttentionItem {
    pub id: String,
    pub kind: HomeAttentionKind,
    pub workspace_id: Uuid,
    pub workspace_name: String,
    /// The owning workspace's owner, for the same avatar the Workspaces tile
    /// shows; `None` only when the workspace carries no resolvable owner.
    pub owner_user_id: Option<Uuid>,
    pub source_id: String,
    pub display_name: Option<String>,
    pub open_question_count: Option<i64>,
    pub actionable_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct OrderedHomeAttentionItem {
    item: HomeAttentionItem,
}

impl OrderedHomeAttentionItem {
    fn key(&self) -> (DateTime<Utc>, u8, Uuid, &str) {
        (
            self.item.actionable_at,
            self.item.kind.rank(),
            self.item.workspace_id,
            self.item.id.as_str(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HomeAttentionCursor {
    pub(crate) version: u8,
    pub(crate) actionable_at: DateTime<Utc>,
    pub(crate) kind_rank: u8,
    pub(crate) workspace_id: Uuid,
    pub(crate) id: String,
    /// The order this position belongs to; a cursor issued before orders
    /// existed carries none and continues oldest first.
    #[serde(default)]
    pub(crate) order: HomeAttentionOrder,
}

impl HomeAttentionCursor {
    fn key(&self) -> (DateTime<Utc>, u8, Uuid, &str) {
        (
            self.actionable_at,
            self.kind_rank,
            self.workspace_id,
            self.id.as_str(),
        )
    }

    /// Whether the key lists after this position in the cursor's order.
    pub(crate) fn is_after(
        &self,
        actionable_at: DateTime<Utc>,
        kind_rank: u8,
        workspace_id: Uuid,
        id: &str,
    ) -> bool {
        self.order
            .compare(&(actionable_at, kind_rank, workspace_id, id), &self.key())
            == Ordering::Greater
    }

    /// Whether every key with this `(actionable_at, kind_rank, workspace_id)`
    /// prefix lists before this position, so a whole source can be skipped.
    pub(crate) fn passes_prefix(
        &self,
        actionable_at: DateTime<Utc>,
        kind_rank: u8,
        workspace_id: Uuid,
    ) -> bool {
        self.order.compare(
            &(actionable_at, kind_rank, workspace_id),
            &(self.actionable_at, self.kind_rank, self.workspace_id),
        ) == Ordering::Less
    }
}

pub struct CapturedHomeAttention {
    #[cfg(debug_assertions)]
    org_id: Uuid,
    approval_items: Vec<OrderedHomeAttentionItem>,
    question_inputs: Option<CapturedHomeQuestionCandidates>,
    owned_workspace_names: HashMap<Uuid, String>,
    owner_by_workspace: HashMap<Uuid, Uuid>,
    cursor: Option<HomeAttentionCursor>,
    kind: Option<HomeAttentionKind>,
    order: HomeAttentionOrder,
    limit: usize,
}

pub struct HomeAttentionPage {
    pub items: Vec<HomeAttentionItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct VisibleWorkspaceRow {
    id: Uuid,
    name: String,
    owner_user_id: Uuid,
}

#[derive(Debug, Clone, Default)]
pub struct HomeService;

impl HomeService {
    pub fn new() -> Self {
        Self
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn capture_attention(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        org_id: Uuid,
        caller_user_id: Uuid,
        cursor: Option<&str>,
        limit: Option<usize>,
        kind: Option<HomeAttentionKind>,
        order: HomeAttentionOrder,
        approval_config_service: &ApprovalConfigService,
        approval_definition_service: &ApprovalDefinitionService,
        workspace_service: &WorkspaceService,
        doc_service: &DocService,
    ) -> Result<CapturedHomeAttention, AppError> {
        let limit = validate_limit(limit)?;
        let cursor = cursor.map(decode_cursor).transpose()?;
        if cursor
            .as_ref()
            .zip(kind)
            .is_some_and(|(cursor, kind)| cursor.kind_rank != kind.rank())
        {
            return Err(invalid_cursor());
        }
        // A position from one order never continues the other.
        if cursor.as_ref().is_some_and(|cursor| cursor.order != order) {
            return Err(invalid_cursor());
        }

        let org_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(caller_user_id)
        .fetch_optional(system_pool)
        .await?;
        let org_wide_access = matches!(org_role.as_deref(), Some("admin"));

        let visible_workspaces: Vec<VisibleWorkspaceRow> = sqlx::query_as(
            r#"
            SELECT w.id, w.name, w.owner_user_id
            FROM workspaces w
            WHERE w.org_id = $1 AND w.deleted_at IS NULL
              AND authz.workspace_audience_allows(w.personal_user_id, w.owner_user_id,
                  w.agent_approval_review_id, $2, $3)
            ORDER BY w.id
            "#,
        )
        .bind(org_id)
        .bind(caller_user_id)
        .bind(org_wide_access)
        .fetch_all(db.conn())
        .await?;

        // Every visible workspace's owner, so each attention row can show the
        // owning workspace's owner avatar as the Workspaces tile does; shared
        // by approval items here and question items in materialization.
        let owner_by_workspace = visible_workspaces
            .iter()
            .map(|workspace| (workspace.id, workspace.owner_user_id))
            .collect::<HashMap<Uuid, Uuid>>();

        let approval_items = if kind.is_none() || kind == Some(HomeAttentionKind::Approval) {
            let approval_candidates = visible_workspaces
                .iter()
                .map(|workspace| (workspace.id, workspace.name.clone()))
                .collect::<Vec<_>>();
            approval_config_service
                .actionable_for_org(
                    db,
                    system_pool,
                    approval_definition_service,
                    org_id,
                    caller_user_id,
                    &approval_candidates,
                    cursor.as_ref(),
                    order,
                    limit + 1,
                )
                .await
                .map_err(AppError::from_error)?
                .into_iter()
                .map(|approval| OrderedHomeAttentionItem {
                    item: HomeAttentionItem {
                        id: approval.id,
                        kind: HomeAttentionKind::Approval,
                        workspace_id: approval.source_workspace_id,
                        workspace_name: approval.source_workspace_name,
                        owner_user_id: owner_by_workspace
                            .get(&approval.source_workspace_id)
                            .copied(),
                        source_id: approval.definition_id.to_string(),
                        display_name: Some(approval.definition_name),
                        open_question_count: None,
                        actionable_at: approval.actionable_at,
                    },
                })
                .collect()
        } else {
            Vec::new()
        };

        let owned_workspace_names = visible_workspaces
            .into_iter()
            .filter(|workspace| workspace.owner_user_id == caller_user_id)
            .map(|workspace| (workspace.id, workspace.name))
            .collect::<HashMap<_, _>>();
        let owned_workspace_ids = owned_workspace_names.keys().copied().collect::<Vec<_>>();
        let question_inputs = if kind == Some(HomeAttentionKind::Approval) {
            None
        } else {
            Some(
                workspace_service
                    .capture_home_question_candidates(
                        db,
                        org_id,
                        caller_user_id,
                        &owned_workspace_ids,
                        cursor.as_ref(),
                        order,
                        limit + 1,
                        kind,
                        doc_service,
                    )
                    .await?,
            )
        };

        Ok(CapturedHomeAttention {
            #[cfg(debug_assertions)]
            org_id,
            approval_items,
            question_inputs,
            owned_workspace_names,
            owner_by_workspace,
            cursor,
            kind,
            order,
            limit,
        })
    }

    pub async fn materialize_attention(
        &self,
        captured: CapturedHomeAttention,
        workspace_service: &WorkspaceService,
        doc_service: &DocService,
        runtime: &crate::services::WorkspaceRuntime,
    ) -> Result<HomeAttentionPage, AppError> {
        let mut ordered = captured.approval_items;
        let question_candidates = if let Some(question_inputs) = captured.question_inputs {
            workspace_service
                .materialize_home_question_candidates(question_inputs, doc_service, runtime)
                .await?
        } else {
            crate::services::workspace_service::HomeQuestionCandidateSets {
                project_docs: Vec::new(),
                question_sets: Vec::new(),
            }
        };
        let candidate_limit = captured.limit + 1;
        let approval_count = ordered.len();
        // Project Doc and question-set sources share the open-question
        // stream budget.
        let project_doc_count =
            question_candidates.project_docs.len() + question_candidates.question_sets.len();
        validate_candidate_bounds(
            candidate_limit,
            captured.kind,
            approval_count,
            project_doc_count,
        )?;

        for candidate in question_candidates.project_docs {
            let Some(workspace_name) = captured.owned_workspace_names.get(&candidate.workspace_id)
            else {
                continue;
            };
            let source = candidate.source;
            ordered.push(OrderedHomeAttentionItem {
                item: HomeAttentionItem {
                    id: format!("open_question:{}:{}", candidate.workspace_id, source.doc_id),
                    kind: HomeAttentionKind::OpenQuestion,
                    workspace_id: candidate.workspace_id,
                    workspace_name: workspace_name.clone(),
                    owner_user_id: captured
                        .owner_by_workspace
                        .get(&candidate.workspace_id)
                        .copied(),
                    source_id: source.doc_id,
                    display_name: Some(source.display_name),
                    open_question_count: Some(source.open_question_count),
                    actionable_at: source.actionable_at,
                },
            });
        }
        for candidate in question_candidates.question_sets {
            let Some(workspace_name) = captured.owned_workspace_names.get(&candidate.workspace_id)
            else {
                continue;
            };
            // A set item opens its first referencing Project Doc; its own id
            // namespace keeps it distinct from that document's embedded item.
            ordered.push(OrderedHomeAttentionItem {
                item: HomeAttentionItem {
                    id: format!(
                        "open_question_set:{}:{}",
                        candidate.workspace_id, candidate.source.path
                    ),
                    kind: HomeAttentionKind::OpenQuestion,
                    workspace_id: candidate.workspace_id,
                    workspace_name: workspace_name.clone(),
                    owner_user_id: captured
                        .owner_by_workspace
                        .get(&candidate.workspace_id)
                        .copied(),
                    source_id: candidate.referencing_doc_id,
                    display_name: Some(candidate.source.title),
                    open_question_count: Some(candidate.source.open_question_count),
                    actionable_at: candidate.actionable_at,
                },
            });
        }
        #[cfg(debug_assertions)]
        observe_home_candidates(HomeCandidateObservation {
            org_id: captured.org_id,
            candidate_limit,
            approval_count,
            project_doc_count,
            merged_count: ordered.len(),
        });

        ordered.sort_by(|left, right| captured.order.compare(&left.key(), &right.key()));
        if let Some(cursor) = &captured.cursor {
            ordered.retain(|item| {
                let (actionable_at, kind_rank, workspace_id, id) = item.key();
                cursor.is_after(actionable_at, kind_rank, workspace_id, id)
            });
        }

        let has_more = ordered.len() > captured.limit;
        ordered.truncate(captured.limit);
        let next_cursor = if has_more {
            ordered
                .last()
                .map(|item| encode_cursor(item, captured.order))
                .transpose()?
        } else {
            None
        };
        Ok(HomeAttentionPage {
            items: ordered.into_iter().map(|ordered| ordered.item).collect(),
            next_cursor,
        })
    }
}

fn validate_candidate_bounds(
    candidate_limit: usize,
    kind: Option<HomeAttentionKind>,
    approval_count: usize,
    project_doc_count: usize,
) -> Result<(), AppError> {
    let merged_count = approval_count + project_doc_count;
    let unselected_stream_has_candidates = match kind {
        None => false,
        Some(HomeAttentionKind::Approval) => project_doc_count > 0,
        Some(HomeAttentionKind::OpenQuestion) => approval_count > 0,
    };
    let merged_limit = if kind.is_some() {
        candidate_limit
    } else {
        2 * candidate_limit
    };
    if approval_count > candidate_limit
        || project_doc_count > candidate_limit
        || merged_count > merged_limit
        || unselected_stream_has_candidates
    {
        return Err(AppError::from_error(G5Error::Internal {
            message: format!(
                "Home attention candidate bound violated: approvals={approval_count}, \
                 project_docs={project_doc_count}, candidate_limit={candidate_limit}"
            ),
        }));
    }
    Ok(())
}

fn validate_limit(limit: Option<usize>) -> Result<usize, AppError> {
    let limit = limit.unwrap_or(DEFAULT_HOME_ATTENTION_LIMIT);
    if !(1..=MAX_HOME_ATTENTION_LIMIT).contains(&limit) {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "limit",
                "message": "limit must be between 1 and 50",
            }),
        }));
    }
    Ok(limit)
}

fn decode_cursor(encoded: &str) -> Result<HomeAttentionCursor, AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| invalid_cursor())?;
    let cursor: HomeAttentionCursor =
        serde_json::from_slice(&bytes).map_err(|_| invalid_cursor())?;
    if cursor.version != 1 || cursor.kind_rank > 1 || cursor.id.is_empty() {
        return Err(invalid_cursor());
    }
    Ok(cursor)
}

fn encode_cursor(
    item: &OrderedHomeAttentionItem,
    order: HomeAttentionOrder,
) -> Result<String, AppError> {
    let cursor = HomeAttentionCursor {
        version: 1,
        actionable_at: item.item.actionable_at,
        kind_rank: item.item.kind.rank(),
        workspace_id: item.item.workspace_id,
        id: item.item.id.clone(),
        order,
    };
    let bytes = serde_json::to_vec(&cursor).map_err(|error| {
        AppError::from_error(G5Error::Internal {
            message: format!("Failed to encode Home attention cursor: {error}"),
        })
    })?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn invalid_cursor() -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({
            "field": "cursor",
            "message": "cursor is invalid",
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(order: HomeAttentionOrder) -> HomeAttentionCursor {
        HomeAttentionCursor {
            version: 1,
            actionable_at: DateTime::from_timestamp(1_000, 0).expect("timestamp"),
            kind_rank: 1,
            workspace_id: Uuid::from_u128(5),
            id: "open_question:b".to_string(),
            order,
        }
    }

    #[test]
    fn newest_is_exactly_the_reverse_of_oldest() {
        let keys = [(1, 0, 2), (1, 1, 1), (2, 0, 1), (1, 1, 2)];
        let mut oldest = keys.to_vec();
        oldest.sort_by(|left, right| HomeAttentionOrder::Oldest.compare(left, right));
        let mut newest = keys.to_vec();
        newest.sort_by(|left, right| HomeAttentionOrder::Newest.compare(left, right));
        newest.reverse();
        assert_eq!(oldest, newest);
    }

    #[test]
    fn cursor_positions_follow_their_own_order() {
        let at = DateTime::from_timestamp(1_000, 0).expect("timestamp");
        let later = DateTime::from_timestamp(2_000, 0).expect("timestamp");
        let workspace = Uuid::from_u128(5);
        let oldest = cursor(HomeAttentionOrder::Oldest);
        let newest = cursor(HomeAttentionOrder::Newest);

        assert!(oldest.is_after(later, 0, workspace, "approval:a"));
        assert!(!newest.is_after(later, 0, workspace, "approval:a"));
        assert!(newest.is_after(at, 1, workspace, "open_question:a"));
        assert!(!oldest.is_after(at, 1, workspace, "open_question:a"));
        // The position itself is never after itself, in either order.
        assert!(!oldest.is_after(at, 1, workspace, "open_question:b"));
        assert!(!newest.is_after(at, 1, workspace, "open_question:b"));

        // A whole workspace is passed only when its prefix lists before the
        // cursor; an equal prefix still needs the item-level check.
        assert!(newest.passes_prefix(later, 1, workspace));
        assert!(!oldest.passes_prefix(later, 1, workspace));
        assert!(!newest.passes_prefix(at, 1, workspace));
        assert!(!oldest.passes_prefix(at, 1, workspace));
    }

    #[test]
    fn cursors_round_trip_their_order_and_legacy_cursors_read_oldest() {
        let item = OrderedHomeAttentionItem {
            item: HomeAttentionItem {
                id: "approval:x".to_string(),
                kind: HomeAttentionKind::Approval,
                workspace_id: Uuid::from_u128(7),
                workspace_name: "w".to_string(),
                owner_user_id: Some(Uuid::from_u128(11)),
                source_id: "d".to_string(),
                display_name: None,
                open_question_count: None,
                actionable_at: DateTime::from_timestamp(3_000, 0).expect("timestamp"),
            },
        };
        for order in [HomeAttentionOrder::Oldest, HomeAttentionOrder::Newest] {
            let decoded =
                decode_cursor(&encode_cursor(&item, order).expect("encode")).expect("decode");
            assert_eq!(decoded.order, order);
            assert_eq!(decoded.id, "approval:x");
        }

        let legacy = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "actionable_at": "2026-10-01T00:00:00Z",
                "kind_rank": 0,
                "workspace_id": Uuid::from_u128(7),
                "id": "approval:x",
            }))
            .expect("legacy cursor json"),
        );
        assert_eq!(
            decode_cursor(&legacy).expect("legacy cursor").order,
            HomeAttentionOrder::Oldest
        );
    }
}
