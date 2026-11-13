//! Read projections for the shared review surface. Callers enter an authorized
//! workspace scope; secondary identifiers are always resolved within that scope.
use crate::services::git::workspace_revision_history::Revision;
use chrono::{DateTime, Utc};
use g5_contracts::error::G5Error;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

const LINEAGE: &str = "WITH RECURSIVE lineage(id,cut) AS (SELECT $1::uuid,'infinity'::timestamptz UNION ALL SELECT f.source_workspace_id,LEAST(l.cut,f.created_at) FROM lineage l JOIN workspace_forks f ON f.target_workspace_id=l.id AND f.status='completed' WHERE f.source_workspace_id=authz.readable_parent_workspace(l.id,$4)) ";

pub struct ReviewRecords {
    pub workspace: Value,
    pub pr: Option<Value>,
    pub requests: Vec<Value>,
    pub decisions: Vec<Value>,
    pub latest: Option<Revision>,
}

pub async fn overview(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<ReviewRecords, G5Error> {
    let workspace = sqlx::query_scalar("SELECT jsonb_build_object('ownerUserId',owner_user_id,'targetWorkspaceId',COALESCE(merge_target_workspace_id,cloned_from_id),'phase',phase,'readOnly',mutation_policy<>'read_write' OR is_base) FROM workspaces WHERE id=$1")
        .bind(workspace_id).fetch_one(&mut *conn).await?;
    let pr: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'title',title,'description',description_override,'sourceHeads',source_heads,'createdAt',created_at) FROM workspace_pull_requests WHERE source_workspace_id=$1 AND status='open' ORDER BY created_at DESC LIMIT 1")
        .bind(workspace_id).fetch_optional(&mut *conn).await?;
    let pr_id = pr
        .as_ref()
        .and_then(|p| p["id"].as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    let requests = sqlx::query_scalar("SELECT jsonb_build_object('id',r.id,'definitionId',r.approval_definition_id,'definitionVersion',r.approval_definition_version,'name',d.name,'allowSelfApproval',d.allow_self_approval AND (SELECT current.allow_self_approval FROM approval_definitions current WHERE current.org_id=r.org_id AND current.id=r.approval_definition_id ORDER BY current.version DESC LIMIT 1),'note',r.note,'phase',r.phase,'actorUserId',r.actor_user_id,'requestedBy',r.requested_by,'state',r.state,'createdAt',r.created_at,'closedAt',r.closed_at) FROM approval_requests r JOIN approval_definitions d ON d.org_id=r.org_id AND d.id=r.approval_definition_id AND d.version=r.approval_definition_version WHERE r.source_workspace_id=$1 AND (r.pull_request_id=$2 OR r.without_pull_request) AND r.actor_type='human' ORDER BY r.seq DESC")
        .bind(workspace_id).bind(pr_id).fetch_all(&mut *conn).await?;
    let decisions = sqlx::query_scalar("SELECT jsonb_build_object('id',s.id,'requestId',s.request_id,'definitionId',s.approval_definition_id,'name',d.name,'phase',s.phase,'actorUserId',s.actor_user_id,'outcome',s.outcome,'comment',s.comment,'reviewedGitHeads',s.reviewed_git_heads,'createdAt',s.created_at) FROM approval_statuses s JOIN approval_definitions d ON d.org_id=s.org_id AND d.id=s.approval_definition_id AND d.version=s.approval_definition_version WHERE s.source_workspace_id=$1 AND (s.pull_request_id=$2 OR (s.pull_request_id IS NULL AND EXISTS(SELECT 1 FROM approval_requests q WHERE q.id=s.request_id AND q.without_pull_request))) AND s.actor_type='human' ORDER BY s.created_at DESC,s.id DESC")
        .bind(workspace_id).bind(pr_id).fetch_all(&mut *conn).await?;
    let latest = sqlx::query_as("SELECT id,workspace_id,observed_at,heads,pushed_at FROM workspace_revision_events WHERE workspace_id=$1 ORDER BY observed_at DESC,id DESC LIMIT 1")
        .bind(workspace_id).fetch_optional(conn).await?;
    Ok(ReviewRecords {
        workspace,
        pr,
        requests,
        decisions,
        latest,
    })
}

pub async fn revisions(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    actor: Uuid,
    before_at: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
) -> Result<Vec<Revision>, G5Error> {
    let sql = format!("{LINEAGE} SELECT e.id,e.workspace_id,e.observed_at,e.heads,e.pushed_at FROM workspace_revision_events e JOIN lineage l ON l.id=e.workspace_id WHERE e.observed_at<=l.cut AND ($2::timestamptz IS NULL OR (e.observed_at,e.id)<($2,$3)) ORDER BY e.observed_at DESC,e.id DESC LIMIT 26");
    Ok(sqlx::query_as(&sql)
        .bind(workspace_id)
        .bind(before_at)
        .bind(before_id)
        .bind(actor)
        .fetch_all(conn)
        .await?)
}

pub async fn line_anchor(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    comment_id: Uuid,
) -> Result<Option<(Uuid, String, Option<String>)>, G5Error> {
    Ok(sqlx::query_as("SELECT repo_id,file_path,commit_sha FROM comment_index WHERE id=$1 AND workspace_id=$2 AND scope='line'").bind(comment_id).bind(workspace_id).fetch_optional(conn).await?)
}

pub async fn anchor_heads(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
    sha: &str,
) -> Result<Option<Value>, G5Error> {
    Ok(sqlx::query_scalar("SELECT heads FROM workspace_revision_events e WHERE workspace_id=$1 AND EXISTS(SELECT 1 FROM jsonb_array_elements(e.heads) h WHERE h->>'repoId'=$2 AND h->>'headCommit'=$3) ORDER BY observed_at DESC,id DESC LIMIT 1").bind(workspace_id).bind(repo_id.to_string()).bind(sha).fetch_optional(conn).await?)
}

pub async fn checkpoint(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    actor: Uuid,
    definition_id: Option<Uuid>,
    _phase: Option<&str>,
) -> Result<Option<Value>, G5Error> {
    Ok(sqlx::query_scalar("SELECT s.reviewed_git_heads FROM approval_statuses s WHERE (s.pull_request_id IN (SELECT p.id FROM workspace_pull_requests p WHERE p.source_workspace_id=$1 AND p.status='open') OR (s.pull_request_id IS NULL AND EXISTS(SELECT 1 FROM approval_requests q WHERE q.id=s.request_id AND q.without_pull_request))) AND s.source_workspace_id=$1 AND s.actor_user_id=$2 AND s.approval_definition_id=$3 AND s.actor_type='human' ORDER BY s.seq DESC LIMIT 1")
        .bind(workspace_id).bind(actor).bind(definition_id).fetch_optional(conn).await?)
}

pub async fn revision(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    actor: Uuid,
    revision_id: Option<Uuid>,
) -> Result<Option<Revision>, G5Error> {
    let sql = format!("{LINEAGE} SELECT e.id,e.workspace_id,e.observed_at,e.heads,e.pushed_at FROM workspace_revision_events e JOIN lineage l ON e.workspace_id=l.id WHERE e.id=$2 AND e.observed_at<=l.cut AND $3::uuid IS NULL");
    Ok(sqlx::query_as(&sql)
        .bind(workspace_id)
        .bind(revision_id)
        .bind(None::<Uuid>)
        .bind(actor)
        .fetch_optional(conn)
        .await?)
}

pub async fn preceding(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    actor: Uuid,
    selected: &Revision,
) -> Result<Option<Value>, G5Error> {
    let sql = format!("{LINEAGE} SELECT e.heads FROM workspace_revision_events e JOIN lineage l ON e.workspace_id=l.id WHERE e.observed_at<=l.cut AND (e.observed_at,e.id)<($2,$3) ORDER BY e.observed_at DESC,e.id DESC LIMIT 1");
    Ok(sqlx::query_scalar(&sql)
        .bind(workspace_id)
        .bind(selected.observed_at)
        .bind(selected.id)
        .bind(actor)
        .fetch_optional(conn)
        .await?)
}
