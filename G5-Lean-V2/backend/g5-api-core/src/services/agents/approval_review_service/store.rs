//! Typed persistence for the formal-review coordinator's existing durable facts.
use crate::errors::AppError;
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::{types::FormalApprovalInvocation, G5Error};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct Review {
    pub id: Uuid,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub approval_request_id: Uuid,
    pub requested_by: Uuid,
    pub target_workspace_id: Uuid,
    pub review_workspace_id: Uuid,
    pub definition_snapshot: Value,
    pub target_heads: Option<Value>,
    pub invocation: Option<Value>,
    pub task_id: Option<Uuid>,
    pub artifact_id: Option<Uuid>,
    pub candidate_summary: Option<String>,
    pub stage: String,
    pub deadline_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub cleanup_due_at: Option<DateTime<Utc>>,
    pub cleanup_attempts: i32,
}
pub(super) struct CandidateEvidence<'a> {
    pub id: Uuid,
    pub result: &'a str,
    pub document: &'a str,
    pub digest: &'a str,
    pub execution: Value,
    pub summary: &'a str,
}
pub(super) struct FormalReviewStore;
impl FormalReviewStore {
    pub async fn due(db: &mut ScopedDb, org: Uuid) -> Result<Vec<Uuid>, AppError> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT id FROM agent_approval_reviews
            WHERE org_id = $1 AND cleanup_completed_at IS NULL
              AND (stage NOT IN ('effective','failed','cancelled') OR cleanup_due_at <= clock_timestamp())
            ORDER BY CASE WHEN stage IN ('effective','failed','cancelled')
                          THEN cleanup_due_at ELSE deadline_at END, created_at, id
            "#,
        )
        .bind(org)
        .fetch_all(db.conn())
        .await?)
    }
    pub async fn try_lock_owner(db: &mut ScopedDb, id: Uuid) -> Result<bool, AppError> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT pg_try_advisory_xact_lock(hashtextextended($1,640))
            "#,
        )
        .bind(id.to_string())
        .fetch_one(db.conn())
        .await?)
    }
    pub async fn load(db: &mut ScopedDb, org: Uuid, id: Uuid) -> Result<Review, AppError> {
        Ok(sqlx::query_as(
            r#"
            SELECT id, org_id, source_workspace_id, approval_request_id, requested_by,
                target_workspace_id, review_workspace_id, definition_snapshot, target_heads,
                invocation, task_id, artifact_id, candidate_summary, stage, deadline_at,
                claimed_at, cleanup_due_at, cleanup_attempts
            FROM agent_approval_reviews WHERE id = $1 AND org_id = $2
            "#,
        )
        .bind(id)
        .bind(org)
        .fetch_one(db.conn())
        .await?)
    }
    pub async fn request_state(
        db: &mut ScopedDb,
        review: &Review,
        lock: bool,
    ) -> Result<String, AppError> {
        let query = if lock {
            "SELECT state FROM approval_requests WHERE id = $1 AND org_id = $2 FOR UPDATE"
        } else {
            "SELECT state FROM approval_requests WHERE id = $1 AND org_id = $2"
        };
        Ok(sqlx::query_scalar(query)
            .bind(review.approval_request_id)
            .bind(review.org_id)
            .fetch_one(db.conn())
            .await?)
    }
    pub async fn first_claim(
        db: &mut ScopedDb,
        review: &Review,
    ) -> Result<Option<(DateTime<Utc>, DateTime<Utc>)>, AppError> {
        Ok(sqlx::query_as(
            r#"
            UPDATE agent_approval_reviews
            SET claimed_at = clock_timestamp(), stage_started_at = clock_timestamp(),
                deadline_at = clock_timestamp() + interval '5 minutes'
            WHERE id = $1 AND org_id = $2 AND stage = 'capture' AND claimed_at IS NULL
            RETURNING claimed_at, deadline_at
            "#,
        )
        .bind(review.id)
        .bind(review.org_id)
        .fetch_optional(db.conn())
        .await?)
    }
    pub async fn input_accepted_at(
        db: &mut ScopedDb,
        task: Option<Uuid>,
    ) -> Result<Option<DateTime<Utc>>, AppError> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT min(delivered_at) FROM session_inputs WHERE task_id = $1
            "#,
        )
        .bind(task)
        .fetch_one(db.conn())
        .await?)
    }
    pub async fn record_transient(db: &mut ScopedDb, review: &Review) -> Result<i32, AppError> {
        Ok(sqlx::query_scalar(
            r#"
            UPDATE agent_approval_reviews
            SET transient_attempts = transient_attempts + 1, updated_at = now()
            WHERE id = $1 AND stage = $2 RETURNING transient_attempts
            "#,
        )
        .bind(review.id)
        .bind(&review.stage)
        .fetch_one(db.conn())
        .await?)
    }
    pub async fn record_target(
        db: &mut ScopedDb,
        review: &Review,
        heads: Value,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE agent_approval_reviews SET target_heads = $2
            WHERE id = $1 AND target_heads IS NULL AND stage = 'capture'
            "#,
        )
        .bind(review.id)
        .bind(heads)
        .execute(db.conn())
        .await?;
        Ok(())
    }
    pub async fn definition(db: &mut ScopedDb, request: Uuid) -> Result<(Uuid, i32), AppError> {
        Ok(sqlx::query_as(
            r#"
            SELECT approval_definition_id, approval_definition_version
            FROM approval_requests WHERE id = $1
            "#,
        )
        .bind(request)
        .fetch_one(db.conn())
        .await?)
    }
    pub async fn record_assignment(
        db: &mut ScopedDb,
        review: &Review,
        invocation: &FormalApprovalInvocation,
    ) -> Result<(), AppError> {
        let mut invocation = invocation.clone();
        if review.invocation.is_none() && invocation.resolution.has_assignment_metadata() {
            let assigned: chrono::DateTime<chrono::Utc> =
                sqlx::query_scalar("SELECT clock_timestamp()")
                    .fetch_one(db.conn())
                    .await?;
            invocation.resolution.assigned_at =
                Some(assigned.to_rfc3339_opts(chrono::SecondsFormat::Micros, true));
            invocation = FormalApprovalInvocation::render(
                invocation.resolution,
                review.definition_snapshot["name"]
                    .as_str()
                    .unwrap_or("Approval"),
                review.definition_snapshot["agent_prompt"]
                    .as_str()
                    .ok_or_else(|| super::workflow::failure("Pinned policy absent"))?,
            )
            .map_err(super::workflow::failure)?;
        }
        sqlx::query(
            r#"
            UPDATE agent_approval_reviews SET invocation = $2
            WHERE id = $1 AND invocation IS NULL AND stage = 'capture'
            "#,
        )
        .bind(review.id)
        .bind(json!(invocation))
        .execute(db.conn())
        .await?;
        Ok(())
    }
    pub async fn task_status(db: &mut ScopedDb, review: &Review) -> Result<String, AppError> {
        Ok(sqlx::query_scalar(
            "SELECT status FROM tasks WHERE id = $1 AND org_id = $2 AND workspace_id = $3",
        )
        .bind(review.task_id)
        .bind(review.org_id)
        .bind(review.review_workspace_id)
        .fetch_one(db.conn())
        .await?)
    }
    pub async fn bind_task(db: &mut ScopedDb, review: &Review, task: Uuid) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO approval_request_tasks (org_id, approval_request_id, source_workspace_id, task_id)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(review.org_id)
        .bind(review.approval_request_id)
        .bind(review.source_workspace_id)
        .bind(task)
        .execute(db.conn())
        .await?;
        sqlx::query(
            r#"
            UPDATE agent_approval_reviews SET task_id = $2, updated_at = now()
            WHERE id = $1 AND task_id IS NULL AND stage = 'start'
            "#,
        )
        .bind(review.id)
        .bind(task)
        .execute(db.conn())
        .await?;
        Ok(())
    }
    pub async fn record_candidate(
        db: &mut ScopedDb,
        review: &Review,
        candidate: CandidateEvidence<'_>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO agent_approval_artifacts
                (id, org_id, source_workspace_id, approval_request_id, task_id, result, document, content_sha256)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(candidate.id)
        .bind(review.org_id)
        .bind(review.source_workspace_id)
        .bind(review.approval_request_id)

        .bind(review.task_id)
        .bind(candidate.result)
        .bind(candidate.document)
        .bind(candidate.digest)
        .execute(db.conn())
        .await?;
        sqlx::query(
            r#"
            UPDATE agent_approval_reviews
            SET artifact_id = $2, execution_provenance = $3, candidate_summary = $4
            WHERE id = $1 AND stage = 'harvest'
            "#,
        )
        .bind(review.id)
        .bind(candidate.id)
        .bind(candidate.execution)
        // Schema 5 stores its exact authored summary (at most 250 characters);
        // older schemas keep their historical body clip, never shown as a summary.
        .bind(candidate.summary.chars().take(1000).collect::<String>())
        .execute(db.conn())
        .await?;
        Ok(())
    }
    pub async fn settle(
        db: &mut ScopedDb,
        review: &Review,
        invocation: &FormalApprovalInvocation,
    ) -> Result<(), AppError> {
        let result: String = sqlx::query_scalar(
            r#"
            SELECT result FROM agent_approval_artifacts
            WHERE id = $1 AND org_id = $2 AND approval_request_id = $3
            "#,
        )
        .bind(review.artifact_id)
        .bind(review.org_id)
        .bind(review.approval_request_id)
        .fetch_one(db.conn())
        .await?;
        let summary = review
            .candidate_summary
            .as_deref()
            .ok_or_else(|| G5Error::Internal {
                message: "validated candidate summary is absent".into(),
            })?;
        sqlx::query(
            r#"
            UPDATE agent_approval_reviews SET stage = 'effective', sealed_at = now(), effective_at = now(),
                ended_at = clock_timestamp(), result = $2, updated_at = now(), cleanup_due_at = now()
            WHERE id = $1 AND stage = 'seal'
            "#,
        )
        .bind(review.id)
        .bind(&result)
        .execute(db.conn())
        .await?;
        let phase: String = sqlx::query_scalar(
            r#"
            SELECT phase FROM approval_requests WHERE id = $1
            "#,
        )
        .bind(review.approval_request_id)
        .fetch_one(db.conn())
        .await?;
        sqlx::query(
            r#"
            INSERT INTO approval_statuses (id, org_id, source_workspace_id, approval_definition_id,
                approval_definition_version, outcome, comment, actor_type, actor_user_id, actor_task_id,
                reviewed_git_heads, phase, agent_review_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'agent', NULL, $8, $9, $10, $1)
            ON CONFLICT (agent_review_id) WHERE agent_review_id IS NOT NULL DO NOTHING
            "#,
        )
        .bind(review.id)
        .bind(review.org_id)
        .bind(review.source_workspace_id)
        .bind(invocation.resolution.definition_id)

        .bind(invocation.resolution.definition_version)
        .bind(result)
        .bind(summary)
        .bind(review.task_id)

        .bind(json!(invocation.resolution.source_heads))
        .bind(phase)
        .execute(db.conn())
        .await?;
        sqlx::query(
            r#"
            UPDATE approval_requests SET state = 'settled', closed_at = now(), updated_at = now(),
                settlement_fingerprint = (SELECT content_sha256 FROM agent_approval_artifacts WHERE id = $2)
            WHERE id = $1 AND state = 'pending'
            "#,
        )
        .bind(review.approval_request_id)
        .bind(review.artifact_id)
        .execute(db.conn())
        .await?;
        Ok(())
    }
    pub async fn advance(
        db: &mut ScopedDb,
        review: &Review,
        stage: &str,
        deadline: DateTime<Utc>,
    ) -> Result<(), AppError> {
        let changed = sqlx::query(
            r#"
            UPDATE agent_approval_reviews SET stage = $3, stage_started_at = now(), deadline_at = $4,
                transient_attempts = 0, updated_at = now() WHERE id = $1 AND stage = $2
            "#,
        )
        .bind(review.id)
        .bind(&review.stage)
        .bind(stage)
        .bind(deadline)
        .execute(db.conn())
        .await?;
        if changed.rows_affected() != 1 {
            return Err(G5Error::Internal {
                message: "review stage changed concurrently".into(),
            }
            .into());
        }
        Ok(())
    }
    pub async fn terminalize(
        db: &mut ScopedDb,
        review: &Review,
        stage: &str,
        cause: Option<&str>,
    ) -> Result<(), AppError> {
        Self::request_state(db, review, true).await?;
        let failure_stage = cause.map(|cause| {
            if cause.contains("output_validation:") {
                "harvest"
            } else {
                review.stage.as_str()
            }
        });
        sqlx::query(
            r#"
            UPDATE agent_approval_reviews SET stage = $2, failure_stage = $3, failure_cause = $4,
                ended_at = clock_timestamp(), cleanup_due_at = now(), updated_at = now()
            WHERE id = $1 AND stage NOT IN ('effective','failed','cancelled')
            "#,
        )
        .bind(review.id)
        .bind(stage)
        .bind(failure_stage)
        .bind(cause.map(|cause| cause.chars().take(1000).collect::<String>()))
        .execute(db.conn())
        .await?;
        Ok(())
    }
    pub async fn has_vm(db: &mut ScopedDb, review: &Review) -> Result<bool, AppError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM workspace_vms WHERE workspace_id = $1 AND org_id = $2)",
        )
        .bind(review.review_workspace_id)
        .bind(review.org_id)
        .fetch_one(db.conn())
        .await?)
    }
    pub async fn record_cleanup(
        db: &mut ScopedDb,
        review: &Review,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        if let Some(error) = error {
            let delay = match review.cleanup_attempts {
                0 => 30,
                1 => 120,
                2 => 600,
                _ => 3600,
            };
            sqlx::query(
                r#"
                UPDATE agent_approval_reviews SET cleanup_error = $2,
                    cleanup_due_at = now() + make_interval(secs => $3),
                    cleanup_attempts = cleanup_attempts + 1, updated_at = now() WHERE id = $1
                "#,
            )
            .bind(review.id)
            .bind(error.chars().take(1000).collect::<String>())
            .bind(delay as f64)
            .execute(db.conn())
            .await?;
        } else {
            sqlx::query(
                r#"
                UPDATE agent_approval_reviews SET cleanup_completed_at = now(), cleanup_error = NULL,
                    cleanup_due_at = NULL, cleanup_attempts = cleanup_attempts + 1, updated_at = now()
                WHERE id = $1
                "#,
            )
        .bind(review.id)
        .execute(db.conn())
        .await?;
        }
        Ok(())
    }
    pub async fn latest_task_outcome(
        db: &mut ScopedDb,
        review: &Review,
    ) -> Result<Option<Value>, AppError> {
        Ok(sqlx::query_scalar(
            "SELECT outcome FROM task_attempts WHERE task_id = $1 ORDER BY attempt DESC LIMIT 1",
        )
        .bind(review.task_id)
        .fetch_optional(db.conn())
        .await?
        .flatten())
    }
}
