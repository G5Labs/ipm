//! Durable receipt ownership for workspace flushes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::services::git::repo_store::TenantRepoStorage;
use chrono::{DateTime, Utc};
use g5_auth::WorkspaceIncarnationClaims;
use g5_internal_api::workspace_flush::{
    FlushCompleteResponse, FlushFailureRequest, FlushFailureResponse, FlushReason,
    FlushRepoReceipt, FlushStartResponse,
};
use g5_scm::native::{FailureKind, NativeGit, NativeGitError, NativeObjectKind};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::services::identity::workspace_incarnation::{
    current_incarnation_on_system, IncarnationLock, LiveIncarnation,
};

/// Budget for each Git proof read during completion.
const PROOF_READ_DEADLINE: Duration = Duration::from_secs(30);
/// Budget for the whole proof of one completion, across every member.
const PROOF_DEADLINE: Duration = Duration::from_secs(60);
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceFlushError {
    #[error("workspace incarnation is stale")]
    StaleIncarnation,
    #[error("workspace flush member vector is invalid")]
    MemberVector,
    #[error("another workspace flush is still open")]
    FlushInProgress,
    #[error("workspace flush completion conflicts with the recorded receipt")]
    CompletionConflict,
    #[error("workspace flush is not available")]
    NotFound,
    #[error("workspace flush request is invalid: {0}")]
    Invalid(String),
    /// The receipt's Git proof failed; the code names the failed check.
    #[error("workspace flush receipt is not proved: {0}")]
    Proof(&'static str),
    /// A proof read failed. The message is the runner's public outcome:
    /// operation and effect guidance, without storage paths or stderr.
    #[error("workspace repository storage is unavailable: {0}")]
    GitUnavailable(String),
    #[error("workspace flush database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
struct FlushVerification {
    required_repo_ids: Vec<Uuid>,
    branch_name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct FlushRow {
    flush_id: Uuid,
    workspace_id: Uuid,
    vm_id: Option<String>,
    epoch: Option<i64>,
    reason: String,
    required_repo_ids: Vec<Uuid>,
    repos: Value,
    started_at: DateTime<Utc>,
    complete_at: Option<DateTime<Utc>>,
    pruned_at: Option<DateTime<Utc>>,
    error: Option<Value>,
}

#[derive(Clone)]
pub struct WorkspaceFlushService {
    pool: PgPool,
    repo_storage: TenantRepoStorage,
    git_bin: PathBuf,
}

impl WorkspaceFlushService {
    pub fn new(pool: PgPool, repo_storage: TenantRepoStorage, git_bin: PathBuf) -> Self {
        Self {
            pool,
            repo_storage,
            git_bin,
        }
    }

    pub async fn start(
        &self,
        claims: &WorkspaceIncarnationClaims,
        reason: FlushReason,
        request_key: &str,
    ) -> Result<FlushStartResponse, WorkspaceFlushError> {
        if request_key.trim() != request_key
            || request_key.is_empty()
            || request_key.len() > 256
            || request_key.chars().any(char::is_control)
        {
            return Err(WorkspaceFlushError::Invalid("idempotency_key".into()));
        }
        let mut tx = self.pool.begin().await?;
        let current = self.lock_current(&mut tx, claims).await?;
        if let Some(row) = sqlx::query_as::<_, FlushRow>(
            r#"
            SELECT flush_id, workspace_id, vm_id, epoch, reason, required_repo_ids,
                   repos, started_at, complete_at, pruned_at, error
            FROM workspace_flushes
            WHERE workspace_id = $1 AND vm_id = $2 AND request_key = $3
            FOR UPDATE
            "#,
        )
        .bind(claims.workspace_id)
        .bind(claims.vm_id.to_string())
        .bind(request_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return start_response(row, reason);
        }
        let required_repo_ids = current.repo_ids;
        if required_repo_ids.is_empty() {
            return Err(WorkspaceFlushError::MemberVector);
        }
        // workspace_flushes_one_active_origin_idx: one open flush per
        // workspace. A second key while one is open is a conflict the guest
        // must not retry blindly, not a database fault.
        let inserted = sqlx::query_as::<_, FlushRow>(
            r#"
            INSERT INTO workspace_flushes(
                org_id, workspace_id, origin, vm_id, epoch, reason,
                required_repo_ids, request_key
            )
            VALUES ($1, $2, 'flush', $3, $4, $5, $6, $7)
            RETURNING flush_id, workspace_id, vm_id, epoch, reason, required_repo_ids,
                      repos, started_at, complete_at, pruned_at, error
            "#,
        )
        .bind(claims.org_id)
        .bind(claims.workspace_id)
        .bind(claims.vm_id.to_string())
        .bind(claims.epoch as i64)
        .bind(match reason {
            FlushReason::Shutdown => "shutdown",
            FlushReason::Fork => "fork",
        })
        .bind(&required_repo_ids)
        .bind(request_key)
        .fetch_one(&mut *tx)
        .await;
        let row = match inserted {
            Ok(row) => row,
            Err(sqlx::Error::Database(error))
                if error.constraint() == Some("workspace_flushes_one_active_origin_idx") =>
            {
                return Err(WorkspaceFlushError::FlushInProgress);
            }
            Err(error) => return Err(error.into()),
        };
        tx.commit().await?;
        start_response(row, reason)
    }

    async fn load_for_verification(
        &self,
        claims: &WorkspaceIncarnationClaims,
        flush_id: Uuid,
    ) -> Result<FlushVerification, WorkspaceFlushError> {
        let mut tx = self.pool.begin().await?;
        let current = self.lock_current(&mut tx, claims).await?;
        let row = lock_flush(&mut tx, claims, flush_id).await?;
        if row.error.is_some() || row.pruned_at.is_some() {
            return Err(WorkspaceFlushError::CompletionConflict);
        }
        tx.commit().await?;
        Ok(FlushVerification {
            required_repo_ids: row.required_repo_ids,
            branch_name: current
                .branch_name
                .ok_or(WorkspaceFlushError::StaleIncarnation)?,
        })
    }

    /// Record a flush's completion once its receipt is proved against the
    /// workspace's repositories (`g5.git.receipt`). Git runs between two
    /// short transactions, never under the incarnation's row locks.
    pub async fn complete(
        &self,
        claims: &WorkspaceIncarnationClaims,
        flush_id: Uuid,
        mut repos: Vec<FlushRepoReceipt>,
    ) -> Result<FlushCompleteResponse, WorkspaceFlushError> {
        let workspace_id = claims.workspace_id;
        let verification = self.load_for_verification(claims, flush_id).await?;
        tracing::warn!(%workspace_id, %flush_id, "workspace flush completion authority loaded");
        self.prove(claims, flush_id, &verification, &repos).await?;
        tracing::warn!(%workspace_id, %flush_id, "workspace flush completion proof verified");
        repos.sort_by_key(|repo| repo.repo_id);
        let ids: Vec<_> = repos.iter().map(|repo| repo.repo_id).collect();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(WorkspaceFlushError::MemberVector);
        }
        let mut tx = self.pool.begin().await?;
        self.lock_current(&mut tx, claims).await?;
        let row = lock_flush(&mut tx, claims, flush_id).await?;
        let mut required = row.required_repo_ids.clone();
        required.sort();
        if ids != required || row.error.is_some() || row.pruned_at.is_some() {
            return Err(WorkspaceFlushError::MemberVector);
        }
        let manifest = serde_json::to_value(&repos)
            .map_err(|_| WorkspaceFlushError::Invalid("receipt".into()))?;
        if let Some(complete_at) = row.complete_at {
            if row.repos != manifest {
                return Err(WorkspaceFlushError::CompletionConflict);
            }
            tx.commit().await?;
            return Ok(FlushCompleteResponse {
                flush_id,
                complete_at,
                repos,
            });
        }
        let complete_at: DateTime<Utc> = sqlx::query_scalar(
            r#"
            UPDATE workspace_flushes
            SET repos = $2, complete_at = now()
            WHERE flush_id = $1 AND complete_at IS NULL AND error IS NULL
            RETURNING complete_at
            "#,
        )
        .bind(flush_id)
        .bind(&manifest)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE workspace_vms
            SET last_flush_id = $2,
                idle_retry_after = NULL,
                updated_at = now()
            WHERE workspace_id = $1 AND vm_id = $3 AND epoch = $4
            "#,
        )
        .bind(claims.workspace_id)
        .bind(flush_id)
        .bind(claims.vm_id.to_string())
        .bind(claims.epoch as i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(FlushCompleteResponse {
            flush_id,
            complete_at,
            repos,
        })
    }

    pub async fn fail(
        &self,
        claims: &WorkspaceIncarnationClaims,
        flush_id: Uuid,
        failure: &FlushFailureRequest,
    ) -> Result<FlushFailureResponse, WorkspaceFlushError> {
        if failure.code != "FLUSH_TOO_LARGE"
            || failure.fingerprint.len() != 64
            || !failure
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || failure.offenders.len() > 16
        {
            return Err(WorkspaceFlushError::Invalid("failure".into()));
        }
        // The failed flush keeps its error. The stop that asked for it gives
        // up saving and kills the VM, so the workspace keeps no block and its
        // owner gets no repair notice (`g5.git.flush-failure.block`).
        let mut tx = self.pool.begin().await?;
        self.lock_current(&mut tx, claims).await?;
        let row = lock_flush(&mut tx, claims, flush_id).await?;
        if row.complete_at.is_some() {
            return Err(WorkspaceFlushError::CompletionConflict);
        }
        // A replay of the same failure keeps its first time.
        let blocked_since = row
            .error
            .as_ref()
            .map(|error| &error["flush_block"])
            .filter(|block| block["fingerprint"] == failure.fingerprint)
            .and_then(|block| block["blocked_since"].as_str())
            .and_then(|value| value.parse::<DateTime<Utc>>().ok())
            .unwrap_or_else(Utc::now);
        let block = json!({
            "code": failure.code,
            "fingerprint": failure.fingerprint,
            "limits": failure.limits,
            "offenders": failure.offenders,
            "detail": failure.detail,
            "blocked_since": blocked_since,
        });
        sqlx::query("UPDATE workspace_flushes SET error = $2 WHERE flush_id = $1")
            .bind(flush_id)
            .bind(json!({"flush_block": block}))
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"
            UPDATE workspace_vms
            SET last_error = jsonb_set(
                    COALESCE(last_error, '{}'::jsonb), '{latest}', $2, true),
                updated_at = now()
            WHERE workspace_id = $1
            "#,
        )
        .bind(claims.workspace_id)
        .bind(json!({"code": failure.code, "at": blocked_since}))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(FlushFailureResponse {
            flush_id,
            blocked_since,
        })
    }

    /// The receipt's Git proof: each member repository is reported once, the
    /// workspace branch exists, the flush leaves hold exactly the reported
    /// commits, and the reported head is reachable from the snapshot.
    async fn prove(
        &self,
        claims: &WorkspaceIncarnationClaims,
        flush_id: Uuid,
        verification: &FlushVerification,
        repos: &[FlushRepoReceipt],
    ) -> Result<(), WorkspaceFlushError> {
        let workspace_id = claims.workspace_id;
        let required = &verification.required_repo_ids;
        let expected: BTreeSet<_> = required.iter().copied().collect();
        let actual: BTreeSet<_> = repos.iter().map(|repo| repo.repo_id).collect();
        if expected.len() != required.len() || actual.len() != repos.len() || actual != expected {
            return Err(WorkspaceFlushError::MemberVector);
        }
        let mapped_ref = format!("refs/heads/{}", verification.branch_name);
        let runner = NativeGit::api(&self.git_bin, PROOF_READ_DEADLINE)
            .map_err(unavailable)?
            .with_deadline(std::time::Instant::now() + PROOF_DEADLINE);
        for receipt in repos {
            for oid in [
                &receipt.branch_oid,
                &receipt.local_branch_oid,
                &receipt.local_head_oid,
                &receipt.snapshot_oid,
            ] {
                if !valid_oid(oid) {
                    return Err(WorkspaceFlushError::Proof("FLUSH_INVALID_PROOF"));
                }
            }
            if receipt.dirty != (receipt.snapshot_oid != receipt.local_head_oid) {
                return Err(WorkspaceFlushError::Proof("FLUSH_CLEAN_MISMATCH"));
            }
            if receipt.head_ref.as_deref().is_some_and(|name| {
                !name.starts_with("refs/heads/")
                    || name.contains("..")
                    || name.contains(['\0', '\n'])
            }) {
                return Err(WorkspaceFlushError::Proof("FLUSH_INVALID_PROOF"));
            }
            let repo = self
                .repo_storage
                .repo_path(claims, claims.org_id, receipt.repo_id)
                .map_err(|_| WorkspaceFlushError::Proof("FLUSH_REPO_NOT_FOUND"))?;
            let git = RepoGit {
                git: runner.clone(),
                repo: &repo,
            };
            if git.ref_tip(&mapped_ref).await?.is_none() {
                return Err(WorkspaceFlushError::Proof("FLUSH_BRANCH_MISMATCH"));
            }
            // branch_oid is the VM's observation at capture time, not a CAS
            // expectation. A provider import or another push may advance G5
            // while the snapshot is uploaded, or before completion is retried.
            git.require_commit(&receipt.branch_oid).await?;
            if receipt.head_ref.as_deref() == Some(mapped_ref.as_str())
                && receipt.local_head_oid != receipt.local_branch_oid
            {
                return Err(WorkspaceFlushError::Proof("FLUSH_ASSOCIATION_MISMATCH"));
            }
            let prefix = format!("refs/g5/flush/{workspace_id}/{flush_id}");
            for (leaf, oid) in [
                ("snapshot", receipt.snapshot_oid.as_str()),
                ("branch", receipt.local_branch_oid.as_str()),
            ] {
                if git.ref_tip(&format!("{prefix}/{leaf}")).await?.as_deref() != Some(oid) {
                    return Err(WorkspaceFlushError::Proof("FLUSH_REF_MISMATCH"));
                }
                git.require_commit(oid).await?;
            }
            git.require_commit(&receipt.local_head_oid).await?;
            if !git
                .is_ancestor(&receipt.local_head_oid, &receipt.snapshot_oid)
                .await?
            {
                return Err(WorkspaceFlushError::Proof("FLUSH_HEAD_UNREACHABLE"));
            }
        }
        Ok(())
    }

    /// The token's incarnation, its VM row locked `FOR UPDATE`.
    async fn lock_current(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        claims: &WorkspaceIncarnationClaims,
    ) -> Result<LiveIncarnation, WorkspaceFlushError> {
        current_incarnation_on_system(tx, claims, IncarnationLock::Update)
            .await?
            .ok_or(WorkspaceFlushError::StaleIncarnation)
    }
}

/// Typed proof reads against one stored repository, through the native
/// runner's API context (exact refs, explicit Git directory, deadline).
struct RepoGit<'a> {
    git: NativeGit,
    repo: &'a Path,
}

impl RepoGit<'_> {
    /// The ref's commit, or `None` when it does not exist.
    async fn ref_tip(&self, name: &str) -> Result<Option<String>, WorkspaceFlushError> {
        self.git.ref_tip(self.repo, name).await.map_err(unavailable)
    }

    async fn require_commit(&self, oid: &str) -> Result<(), WorkspaceFlushError> {
        match self.git.object_kind(self.repo, oid).await {
            Ok(Some(NativeObjectKind::Commit)) => Ok(()),
            Ok(_)
            | Err(NativeGitError {
                kind: FailureKind::InvalidInput,
                ..
            }) => Err(WorkspaceFlushError::Proof("FLUSH_NOT_COMMIT")),
            Err(error) => Err(unavailable(error)),
        }
    }

    async fn is_ancestor(
        &self,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, WorkspaceFlushError> {
        self.git
            .is_ancestor(self.repo, ancestor, descendant)
            .await
            .map_err(unavailable)
    }
}

fn unavailable(error: NativeGitError) -> WorkspaceFlushError {
    tracing::warn!(%error, "workspace flush Git proof read failed");
    WorkspaceFlushError::GitUnavailable(error.public_message())
}

fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn start_response(
    row: FlushRow,
    expected_reason: FlushReason,
) -> Result<FlushStartResponse, WorkspaceFlushError> {
    let reason = match row.reason.as_str() {
        "shutdown" => FlushReason::Shutdown,
        "fork" => FlushReason::Fork,
        _ => return Err(WorkspaceFlushError::CompletionConflict),
    };
    if row.error.is_some() || row.pruned_at.is_some() || reason != expected_reason {
        return Err(WorkspaceFlushError::CompletionConflict);
    }
    Ok(FlushStartResponse {
        flush_id: row.flush_id,
        workspace_id: row.workspace_id,
        vm_id: row
            .vm_id
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(WorkspaceFlushError::StaleIncarnation)?,
        epoch: row.epoch.ok_or(WorkspaceFlushError::StaleIncarnation)? as u64,
        reason,
        required_repo_ids: row.required_repo_ids,
        started_at: row.started_at,
    })
}

async fn lock_flush(
    tx: &mut Transaction<'_, Postgres>,
    claims: &WorkspaceIncarnationClaims,
    flush_id: Uuid,
) -> Result<FlushRow, WorkspaceFlushError> {
    let row = sqlx::query_as::<_, FlushRow>(
        r#"
        SELECT flush_id, workspace_id, vm_id, epoch, reason, required_repo_ids,
               repos, started_at, complete_at, pruned_at, error
        FROM workspace_flushes
        WHERE flush_id = $1 AND workspace_id = $2 AND org_id = $3
          AND origin = 'flush' AND vm_id = $4 AND epoch = $5
        FOR UPDATE
        "#,
    )
    .bind(flush_id)
    .bind(claims.workspace_id)
    .bind(claims.org_id)
    .bind(claims.vm_id.to_string())
    .bind(claims.epoch as i64)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(WorkspaceFlushError::NotFound)?;
    Ok(row)
}
