//! Durable admission and recovery for explicit formal agent approvals.
pub mod inspection;
mod store;
pub mod workflow;

use crate::services::approval_config_service::{approval_request_key, workspace_approval_phase};

use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{
    approval_review_workspace_branch, AgentLaunchSelectionV1, ApprovalReviewIdentity,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateApprovalReviewRequest {
    pub client_request_id: Option<Uuid>,
    pub definition_id: Uuid,
    pub definition_version: i32,
    pub agent_launch: Option<AgentLaunchSelectionV1>,
    pub retry_review_id: Option<Uuid>,
    /// Phase this review settles (`be` | `se` | `ie`); validated against
    /// legacy phase vocabulary; associations only recommend request timing.
    pub phase: String,
}

#[derive(Debug, Clone)]
pub struct ApprovalReviewAdmission {
    pub request_id: Uuid,
    pub status: String,
    pub reused: bool,
    pub task_id: Option<Uuid>,
    pub review_id: Uuid,
    pub(crate) request_key: String,
    operation_binding: Option<(String, serde_json::Value)>,
}

impl ApprovalReviewAdmission {
    pub fn request_key(&self) -> &str {
        &self.request_key
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SourceWorkspaceRow {
    org_id: Uuid,
    target_workspace_id: Option<Uuid>,
    is_base: bool,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct DefinitionRow {
    version: i32,
    name: String,
    agent_type: Option<String>,
    agent_prompt: Option<String>,
    agent_harness: Option<String>,
    agent_model: Option<String>,
    agent_effort: Option<String>,
}

#[derive(sqlx::FromRow)]
struct FrozenRetryRow {
    org_id: Uuid,
    target_workspace_id: Uuid,
    definition_snapshot: serde_json::Value,
    target_heads: serde_json::Value,
    invocation: serde_json::Value,
    approval_definition_id: Uuid,
    approval_definition_version: i32,
    phase: String,
}

struct AttemptDraft {
    org_id: Uuid,
    source_workspace_id: Uuid,
    requested_by: Uuid,
    target_workspace_id: Uuid,
    definition_id: Uuid,
    definition_version: i32,
    phase: String,
    definition_snapshot: serde_json::Value,
    target_heads: Option<serde_json::Value>,
    invocation: Option<serde_json::Value>,
    retry_of: Option<Uuid>,
}

#[derive(Clone, Default)]
pub struct ApprovalReviewService {
    /// Shared with human requests (g5.governance.approval.request-pr).
    pr_required: bool,
}

impl ApprovalReviewService {
    pub fn new() -> Self {
        Self::default()
    }

    /// The same request PR rule human requests use.
    pub fn with_policy(policy: g5_contracts::types::ApprovalPolicy) -> Self {
        Self {
            pr_required: policy.pr_required,
        }
    }

    /// Admit an explicit formal review; the coordinator performs its effects.
    #[allow(clippy::too_many_arguments)]
    pub async fn admit(
        &self,
        mut db: ScopedDb,
        system_pool: &sqlx::PgPool,
        source_workspace_id: Uuid,
        actor_user_id: Uuid,
        req: CreateApprovalReviewRequest,
    ) -> Result<(ScopedDb, ApprovalReviewAdmission), G5Error> {
        let phase = workspace_approval_phase(&req.phase)?;
        // This fence precedes both replay and new admission. It is distinct
        // from the slot fence: different operations can reuse the same run.
        crate::services::workspace_mutation_policy::ensure_workspace_owner_is(
            &mut db,
            source_workspace_id,
            actor_user_id,
        )
        .await?;
        let operation = req
            .client_request_id
            .map(|id| format!("{actor_user_id}:{id}"));
        let inputs = serde_json::json!({
            "definitionId": req.definition_id,
            "definitionVersion": req.definition_version,
            "phase": phase,
            "retryReviewId": req.retry_review_id,
        });
        if let Some(key) = &operation {
            acquire_approval_review_admission_lock(
                &mut db,
                &format!("agent-operation:{source_workspace_id}:{key}"),
            )
            .await?;
            let row: Option<(Uuid, Uuid, String, Option<Uuid>, String, serde_json::Value)> = sqlx::query_as(
                "SELECT r.id, q.id, q.state, r.task_id, q.request_key, r.client_operations->$2 FROM agent_approval_reviews r JOIN approval_requests q ON q.id=r.approval_request_id AND q.org_id=r.org_id WHERE r.source_workspace_id=$1 AND r.client_operations ? $2",
            ).bind(source_workspace_id).bind(key).fetch_optional(db.conn()).await?;
            if let Some((review_id, request_id, status, task_id, request_key, saved)) = row {
                if saved != inputs || req.agent_launch.is_some() {
                    return Err(G5Error::Conflict {
                        reason: "clientRequestId already binds different agent review inputs"
                            .into(),
                    });
                }
                return Ok((
                    db,
                    ApprovalReviewAdmission {
                        request_id,
                        status,
                        reused: true,
                        task_id,
                        review_id,
                        request_key,
                        operation_binding: Some((key.clone(), inputs)),
                    },
                ));
            }
        }
        let (mut db, mut receipt) = self
            .admit_new(db, system_pool, source_workspace_id, actor_user_id, req)
            .await?;
        if let Some(key) = operation {
            sqlx::query("UPDATE agent_approval_reviews SET client_operations=client_operations || jsonb_build_object($2::text,$3::jsonb) WHERE id=$1")
                .bind(receipt.review_id).bind(&key).bind(&inputs).execute(db.conn()).await?;
            receipt.operation_binding = Some((key, inputs));
        }
        Ok((db, receipt))
    }

    async fn admit_new(
        &self,
        mut db: ScopedDb,
        system_pool: &sqlx::PgPool,
        source_workspace_id: Uuid,
        actor_user_id: Uuid,
        req: CreateApprovalReviewRequest,
    ) -> Result<(ScopedDb, ApprovalReviewAdmission), G5Error> {
        if req.definition_version < 1 {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "definitionVersion",
                    "message": "definitionVersion must be positive",
                }),
            });
        }
        if req.agent_launch.is_some() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "agentLaunch",
                    "code": "approval_definition_owns_agent_launch",
                    "message": "approval review launch is owned by the exact approval definition version",
                }),
            });
        }

        if let Some(retry) = req.retry_review_id {
            return self
                .admit_frozen_retry(
                    db,
                    system_pool,
                    source_workspace_id,
                    actor_user_id,
                    retry,
                    req,
                )
                .await;
        }

        let req = CreateApprovalReviewRequest {
            phase: workspace_approval_phase(&req.phase)?,
            ..req
        };
        let source: SourceWorkspaceRow = sqlx::query_as(
            r#"
            SELECT org_id,
                   COALESCE(merge_target_workspace_id, cloned_from_id) AS target_workspace_id,
                   is_base
            FROM workspaces
            WHERE id = $1 AND deleted_at IS NULL FOR SHARE
            "#,
        )
        .bind(source_workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {source_workspace_id}"),
        })?;

        crate::services::workspace_mutation_policy::ensure_workspace_owner_is(
            &mut db,
            source_workspace_id,
            actor_user_id,
        )
        .await?;
        if source.is_base {
            return Err(G5Error::Conflict {
                reason: "agent review requires a writable workspace".into(),
            });
        }
        // A new agent request follows the same PR rule as a human one; a
        // frozen retry continues an admitted request and is not re-gated.
        crate::services::workspace::approval_config_service::request_pull_request(
            &mut db,
            source_workspace_id,
            self.pr_required,
        )
        .await?;

        let target_workspace_id =
            source
                .target_workspace_id
                .ok_or_else(|| G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "sourceWorkspaceId",
                        "message": "source workspace has no merge target",
                    }),
                })?;

        crate::db::scoped::ensure_user_workspace_access_until_commit(
            system_pool,
            &mut db,
            actor_user_id,
            source.org_id,
            target_workspace_id,
        )
        .await
        .map_err(|error| error.inner)?;

        let definition: DefinitionRow = sqlx::query_as(
            r#"
            SELECT version, name, agent_type, agent_prompt,
                   agent_harness, agent_model, agent_effort
            FROM approval_definitions
            WHERE org_id = $1 AND id = $2
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(source.org_id)
        .bind(req.definition_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("approval definition {}", req.definition_id),
        })?;
        if definition.version != req.definition_version {
            return Err(G5Error::Conflict {
                reason: format!(
                    "stale approval definition version: requested {}, current {}",
                    req.definition_version, definition.version
                ),
            });
        }
        if definition.agent_type.as_deref() != Some("simple")
            || definition
                .agent_prompt
                .as_deref()
                .is_none_or(|prompt| prompt.trim().is_empty())
        {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "definitionId",
                    "message": "approval definition is not backed by the supported simple agent",
                }),
            });
        }

        let active_attachment: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT NOT retired
            FROM approval_configs
            WHERE org_id = $1
              AND target_workspace_id = $2
              AND approval_definition_id = $3
            ORDER BY seq DESC
            LIMIT 1
            "#,
        )
        .bind(source.org_id)
        .bind(target_workspace_id)
        .bind(req.definition_id)
        .fetch_optional(db.conn())
        .await?;
        if active_attachment != Some(true) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "definitionId",
                    "message": "approval definition is not an active requirement of the merge target",
                }),
            });
        }

        let definition_snapshot =
            serde_json::to_value(&definition).map_err(|error| G5Error::Internal {
                message: error.to_string(),
            })?;
        self.admit_attempt(
            db,
            AttemptDraft {
                org_id: source.org_id,
                source_workspace_id,
                requested_by: actor_user_id,
                target_workspace_id,
                definition_id: req.definition_id,
                definition_version: req.definition_version,
                phase: req.phase,
                definition_snapshot,
                target_heads: None,
                invocation: None,
                retry_of: None,
            },
        )
        .await
    }

    async fn admit_frozen_retry(
        &self,
        mut db: ScopedDb,
        system_pool: &sqlx::PgPool,
        source: Uuid,
        requester: Uuid,
        prior: Uuid,
        req: CreateApprovalReviewRequest,
    ) -> Result<(ScopedDb, ApprovalReviewAdmission), G5Error> {
        crate::services::workspace_mutation_policy::ensure_workspace_owner_is(
            &mut db, source, requester,
        )
        .await?;
        let old: Option<FrozenRetryRow> = sqlx::query_as(
            r#"
            SELECT r.org_id, r.target_workspace_id, r.definition_snapshot, r.target_heads,
                   r.invocation, q.approval_definition_id, q.approval_definition_version, q.phase
            FROM agent_approval_reviews r
            JOIN approval_requests q ON q.id = r.approval_request_id AND q.org_id = r.org_id
            WHERE r.id = $1 AND r.source_workspace_id = $2
              AND r.stage = 'failed' AND r.invocation IS NOT NULL
            "#,
        )
        .bind(prior)
        .bind(source)
        .fetch_optional(db.conn())
        .await?;
        let FrozenRetryRow {
            org_id: org,
            target_workspace_id: target,
            definition_snapshot: definition,
            target_heads: heads,
            invocation,
            approval_definition_id: definition_id,
            approval_definition_version: version,
            phase,
        } = old.ok_or_else(|| G5Error::Conflict {
            reason: "Only a failed assigned execution can be retried; request a new review for current inputs".into(),
        })?;
        if req.definition_id != definition_id || req.definition_version != version {
            return Err(G5Error::Conflict {
                reason: "retry must preserve the original definition".into(),
            });
        }
        crate::db::scoped::ensure_user_workspace_access_until_commit(
            system_pool,
            &mut db,
            requester,
            org,
            target,
        )
        .await
        .map_err(|e| e.inner)?;
        self.admit_attempt(
            db,
            AttemptDraft {
                org_id: org,
                source_workspace_id: source,
                requested_by: requester,
                target_workspace_id: target,
                definition_id,
                definition_version: version,
                phase,
                definition_snapshot: definition,
                target_heads: Some(heads),
                invocation: Some(invocation),
                retry_of: Some(prior),
            },
        )
        .await
    }

    async fn admit_attempt(
        &self,
        mut db: ScopedDb,
        mut draft: AttemptDraft,
    ) -> Result<(ScopedDb, ApprovalReviewAdmission), G5Error> {
        let mut key = approval_request_key(
            draft.org_id,
            draft.source_workspace_id,
            draft.definition_id,
            draft.definition_version,
            &workspace_approval_phase(&draft.phase)?,
            "agent",
        )?;
        acquire_approval_review_admission_lock(&mut db, &key).await?;
        // Preserve the immutable identity of a pending legacy phase chain.
        // New admission is serialized on the workspace-wide key above.
        let pending: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT request_key,phase FROM approval_requests WHERE org_id=$1 AND source_workspace_id=$2 AND approval_definition_id=$3 AND approval_definition_version=$4 AND actor_type='agent' AND state='pending' FOR UPDATE",
        ).bind(draft.org_id).bind(draft.source_workspace_id).bind(draft.definition_id)
            .bind(draft.definition_version).fetch_optional(db.conn()).await?;
        if let Some((prior_key, phase)) = pending {
            // Ambiguous-COMMIT confirmation uses the immutable stored key.
            // Retain its existing fence alongside the canonical slot lock.
            if prior_key != key {
                acquire_approval_review_admission_lock(&mut db, &prior_key).await?;
            }
            key = prior_key;
            draft.phase = phase
                .unwrap_or_else(|| g5_contracts::types::WORKSPACE_APPROVAL_COMPAT_PHASE.to_owned());
        }
        let proposed_request = Uuid::new_v4();
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO approval_requests (
                id, org_id, source_workspace_id, approval_definition_id,
                approval_definition_version, request_key, actor_type, requested_by, phase
            ) VALUES ($1, $2, $3, $4, $5, $6, 'agent', $7, $8)
            ON CONFLICT (org_id, source_workspace_id, request_key) WHERE state = 'pending'
            DO NOTHING
            RETURNING id
            "#,
        )
        .bind(proposed_request)
        .bind(draft.org_id)
        .bind(draft.source_workspace_id)
        .bind(draft.definition_id)
        .bind(draft.definition_version)
        .bind(&key)
        .bind(draft.requested_by)
        .bind(&draft.phase)
        .fetch_optional(db.conn())
        .await?;
        let request_id = if let Some(id) = inserted {
            id
        } else {
            let row: Option<(Uuid, Uuid, i32, String, Option<Uuid>)> = sqlx::query_as(
                r#"
                SELECT id, approval_definition_id, approval_definition_version,
                       actor_type, actor_user_id
                FROM approval_requests
                WHERE org_id = $1 AND source_workspace_id = $2
                  AND request_key = $3 AND state = 'pending'
                FOR UPDATE
                "#,
            )
            .bind(draft.org_id)
            .bind(draft.source_workspace_id)
            .bind(&key)
            .fetch_optional(db.conn())
            .await?;
            let row = row.ok_or_else(|| G5Error::Conflict {
                reason: "approval request admission changed concurrently; retry the request".into(),
            })?;
            if row.1 != draft.definition_id
                || row.2 != draft.definition_version
                || row.3 != "agent"
                || row.4.is_some()
            {
                return Err(G5Error::Conflict {
                    reason: "approval request key collides with unequal pending state".into(),
                });
            }
            row.0
        };
        let active: Option<(Uuid, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
            r#"
            SELECT id, retry_of, task_id
            FROM agent_approval_reviews
            WHERE org_id = $1 AND approval_request_id = $2
              AND stage NOT IN ('effective','failed','cancelled')
            FOR UPDATE
            "#,
        )
        .bind(draft.org_id)
        .bind(request_id)
        .fetch_optional(db.conn())
        .await?;
        if let Some((review_id, retry_of, task_id)) = active {
            if retry_of != draft.retry_of {
                return Err(G5Error::Conflict {
                    reason: "A review attempt is running for this request; cancel it before starting another.".into(),
                });
            }
            return Ok((
                db,
                ApprovalReviewAdmission {
                    request_id,
                    status: "pending".into(),
                    reused: true,
                    task_id,
                    review_id,
                    request_key: key,
                    operation_binding: None,
                },
            ));
        }
        let review_id = Uuid::new_v4();
        let child = ApprovalReviewIdentity::new(review_id).workspace_id();
        sqlx::query(
            r#"
            INSERT INTO agent_approval_reviews (
                id, org_id, source_workspace_id, approval_request_id, requested_by,
                target_workspace_id, review_workspace_id, definition_snapshot,
                target_heads, invocation, retry_of
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(review_id)
        .bind(draft.org_id)
        .bind(draft.source_workspace_id)
        .bind(request_id)
        .bind(draft.requested_by)
        .bind(draft.target_workspace_id)
        .bind(child)
        .bind(draft.definition_snapshot)
        .bind(draft.target_heads)
        .bind(draft.invocation)
        .bind(draft.retry_of)
        .execute(db.conn())
        .await?;
        Ok((
            db,
            ApprovalReviewAdmission {
                request_id,
                status: "pending".into(),
                reused: false,
                task_id: None,
                review_id,
                request_key: key,
                operation_binding: None,
            },
        ))
    }

    /// Git audience policy follows the same durable review/resource binding.
    /// The route supplies its authenticated user or exact incarnation scope.
    pub async fn hidden_git_refs(
        &self,
        db: &mut ScopedDb,
        repo: Uuid,
        bound_workspace: Option<Uuid>,
    ) -> Result<Vec<String>, G5Error> {
        let reviews: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT review.id, review.review_workspace_id FROM agent_approval_reviews review
            JOIN repos repo ON repo.org_id = review.org_id
            WHERE repo.id = $1 AND review.cleanup_completed_at IS NULL
            "#,
        )
        .bind(repo)
        .fetch_all(db.conn())
        .await?;
        let mut hidden = vec![g5_contracts::types::APPROVAL_EVIDENCE_REF_PREFIX.to_string()];
        hidden.extend(
            reviews
                .into_iter()
                .filter(|(_, child)| bound_workspace != Some(*child))
                .flat_map(|(review, child)| {
                    let mut refs = g5_scm::native::NativeGit::private_workspace_refs(
                        &approval_review_workspace_branch(child),
                        child,
                    );
                    refs.extend([
                        format!(
                            "refs/heads/{}",
                            ApprovalReviewIdentity::new(review).base_branch()
                        ),
                        format!(
                            "refs/g5/forks/{}",
                            ApprovalReviewIdentity::new(review).base_proof_id()
                        ),
                    ]);
                    refs
                }),
        );
        Ok(hidden)
    }

    pub async fn report(
        &self,
        db: &mut ScopedDb,
        source: Uuid,
        review: Uuid,
    ) -> Result<serde_json::Value, G5Error> {
        let result: Option<serde_json::Value> = sqlx::query_scalar(
            r#"
            SELECT jsonb_build_object(
                'reviewId', r.id, 'requestId', r.approval_request_id, 'request', (SELECT jsonb_build_object('note',q.note,'requestedAt',q.created_at,'requestedBy',q.requested_by) FROM approval_requests q WHERE q.id=r.approval_request_id),
                'createdAt', r.created_at, 'startedAt', r.claimed_at, 'endedAt', r.ended_at,
                'stage', r.stage, 'result', r.result, 'assignment', r.invocation,
                'definition', r.definition_snapshot, 'execution', r.execution_provenance,
                'failureStage', r.failure_stage, 'failureCause', r.failure_cause,
                'cleanupPending', r.cleanup_completed_at IS NULL, 'cleanupError', r.cleanup_error,
                'cleanupOverdue', public.formal_review_cleanup_overdue(r.ended_at,r.cleanup_completed_at,r.cleanup_error),
                'document', a.document, 'sha256', a.content_sha256)
            FROM agent_approval_reviews r LEFT JOIN agent_approval_artifacts a
                ON a.id = r.artifact_id AND a.org_id = r.org_id
            WHERE r.id = $1 AND r.source_workspace_id = $2
            "#,
        )
        .bind(review)
        .bind(source)
        .fetch_optional(db.conn())
        .await?;
        result.ok_or_else(|| G5Error::NotFound {
            resource: "formal review evidence".into(),
        })
    }
    pub async fn retry_cleanup(
        &self,
        db: &mut ScopedDb,
        source: Uuid,
        review: Uuid,
    ) -> Result<(), G5Error> {
        let result = sqlx::query(
            r#"
            UPDATE agent_approval_reviews SET cleanup_due_at = now(), updated_at = now()
            WHERE id = $1 AND source_workspace_id = $2 AND stage IN ('effective','failed','cancelled')
                AND cleanup_completed_at IS NULL
            "#,
        )
        .bind(review)
        .bind(source)
        .execute(db.conn())
        .await?;
        if result.rows_affected() != 1 {
            return Err(G5Error::Conflict {
                reason: "review has no unfinished cleanup".into(),
            });
        }
        Ok(())
    }

    /// Reconcile an ambiguous admission COMMIT under the same request-key
    /// fence used by the application transaction. `None` is confirmed absence
    /// after the potentially disconnected transaction released that fence.
    pub async fn confirm_admission_after_commit_error(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        expected: &ApprovalReviewAdmission,
    ) -> Result<Option<ApprovalReviewAdmission>, G5Error> {
        acquire_approval_review_admission_lock(db, expected.request_key()).await?;
        let row: Option<(Uuid, String, Uuid, Option<Uuid>)> = sqlx::query_as(
            r#"
            SELECT request.id, request.request_key, review.id, review.task_id
            FROM approval_requests request JOIN agent_approval_reviews review
              ON review.org_id = request.org_id AND review.approval_request_id = request.id
            WHERE request.source_workspace_id = $1 AND request.request_key = $2
              AND request.id = $3 AND review.id = $4 AND request.actor_type = 'agent'
              AND request.actor_user_id IS NULL
              AND ($5::text IS NULL OR review.client_operations->$5 = $6::jsonb)
            "#,
        )
        .bind(source_workspace_id)
        .bind(expected.request_key())
        .bind(expected.request_id)
        .bind(expected.review_id)
        .bind(expected.operation_binding.as_ref().map(|(key, _)| key))
        .bind(
            expected
                .operation_binding
                .as_ref()
                .map(|(_, inputs)| inputs),
        )
        .fetch_optional(db.conn())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.0 != expected.request_id
            || row.1 != expected.request_key
            || row.2 != expected.review_id
            || (expected.task_id.is_some() && row.3 != expected.task_id)
        {
            return Err(G5Error::Conflict {
                reason: "durable approval-review admission conflicts with the expected receipt"
                    .into(),
            });
        }
        Ok(Some(expected.clone()))
    }
}

async fn acquire_approval_review_admission_lock(
    db: &mut ScopedDb,
    request_key: &str,
) -> Result<(), G5Error> {
    sqlx::query(
        r#"
        SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))
        "#,
    )
    .bind(request_key)
    .execute(db.conn())
    .await?;
    Ok(())
}
