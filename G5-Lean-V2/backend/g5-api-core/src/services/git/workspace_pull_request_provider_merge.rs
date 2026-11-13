//! Provider target ownership lives on the existing PR member, before effects.
//! A transaction/session lock alone cannot retain a possibly accepted request.
use super::*;
use g5_authz::ScopedDbReopener;
use std::collections::HashSet;

impl WorkspacePullRequestProviderLifecycleService {
    /// Own reserve/commit/execute as one operation. The returned transaction
    /// retains the existing PR lock; the caller commits its provider outcomes.
    pub async fn merge_provider_pull_requests<F, Fut>(
        &self,
        mut db: ScopedDb,
        pull_request_id: Uuid,
        target_heads: &[ProviderMergeTargetHead],
        actor_id: Uuid,
        reauthorize: F,
    ) -> Result<ScopedDb, G5Error>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<ScopedDb, G5Error>>,
    {
        let execution_id = Uuid::new_v4();
        let scope = db.reopener();
        let ProviderMergeCapture { rows, target, .. } = self
            .capture_provider_merge(&mut db, pull_request_id)
            .await?;
        Self::lock_merge_targets(&mut db, &rows).await?;
        for row in rows
            .iter()
            .filter(|row| row.provider_status != "merged" && !row.merge_accepted)
        {
            if row.provider_pr_number.is_none() {
                continue;
            }
            let target_head = if row.merge_pending {
                row.merge_target_head_sha.as_deref()
            } else {
                target_heads
                    .iter()
                    .find(|head| head.repo_id == row.repo_id)
                    .map(|head| head.head_commit.as_str())
            }
            .ok_or_else(|| G5Error::Conflict {
                reason: "Merge did not capture this provider target; merge again".into(),
            })?;
            if !row.merge_pending
                && !target.repos.iter().any(|member| {
                    member.repo_id == row.repo_id && member.head_commit == target_head
                })
            {
                return Err(G5Error::Conflict {
                    reason: "Merge target changed after preflight; merge again".into(),
                });
            }
            let occupied: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM workspace_pull_request_provider_repos
                 WHERE repo_id=$1 AND target_remote_branch=$2 AND merge_pending AND id<>$3)",
            )
            .bind(row.repo_id)
            .bind(&row.target_remote_branch)
            .bind(row.id)
            .fetch_one(db.conn())
            .await?;
            if occupied {
                return Err(Self::target_busy());
            }
            // A resumed execution may overlap an earlier request still running
            // at the provider. Its refusal cannot clear that earlier uncertainty.
            sqlx::query(
                "UPDATE workspace_pull_request_provider_repos
                 SET merge_attempt_id=$2, merge_execution_id=$3, merge_target_head_sha=$4,
                     merge_uncertain=merge_pending, merge_pending=true, merge_accepted=false
                 WHERE id=$1",
            )
            .bind(row.id)
            .bind(if row.merge_pending {
                row.merge_attempt_id.unwrap()
            } else {
                Uuid::new_v4()
            })
            .bind(execution_id)
            .bind(target_head)
            .execute(db.conn())
            .await?;
        }
        db.commit()
            .await
            .map_err(|error| crate::errors::AppError::from(error).inner)?;
        let mut dispatched = HashSet::new();
        let mut db = match reauthorize().await {
            Ok(db) => db,
            Err(error) => {
                Self::cleanup_unsent(&scope, pull_request_id, execution_id, &dispatched).await?;
                return Err(error);
            }
        };
        let result = self
            .execute_provider_merges(
                &mut db,
                pull_request_id,
                target_heads,
                actor_id,
                execution_id,
                &mut dispatched,
            )
            .await;
        match result {
            Ok(()) => {
                Self::release_unsent(&mut db, pull_request_id, execution_id, &dispatched).await?;
                Ok(db)
            }
            Err(error) => {
                db.rollback()
                    .await
                    .map_err(|error| crate::errors::AppError::from(error).inner)?;
                // Only executions known not to have dispatched may be released.
                // Cancellation bypasses this cleanup and conservatively retains
                // the reservation, which the same operation can recover later.
                Self::cleanup_unsent(&scope, pull_request_id, execution_id, &dispatched).await?;
                Err(error)
            }
        }
    }

    async fn cleanup_unsent(
        scope: &ScopedDbReopener,
        pull_request_id: Uuid,
        execution_id: Uuid,
        dispatched: &HashSet<Uuid>,
    ) -> Result<(), G5Error> {
        let mut cleanup = scope.open().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to reopen provider cleanup: {error}"),
        })?;
        Self::release_unsent(&mut cleanup, pull_request_id, execution_id, dispatched).await?;
        cleanup
            .commit()
            .await
            .map_err(|error| crate::errors::AppError::from(error).inner)
    }

    fn target_busy() -> G5Error {
        // Workspaces can share a remote target while their PRs remain private.
        // The mechanical conflict must not disclose the other PR's identity.
        G5Error::Conflict {
            reason: "Another provider merge is using this repository's target branch. Retry after its outcome is reconciled.".into(),
        }
    }

    pub(super) async fn lock_merge_targets(
        db: &mut ScopedDb,
        rows: &[ProviderRepoRow],
    ) -> Result<(), G5Error> {
        let mut keys: Vec<_> = rows
            .iter()
            .filter(|row| row.provider_status != "merged")
            .map(|row| (row.repo_id, &row.target_remote_branch))
            .collect();
        keys.sort();
        keys.dedup();
        for (repo, branch) in keys {
            // Do not queue behind provider I/O with a partial reservation batch.
            // The durable pending index, not this short-lived lock, owns recovery.
            let held: bool = sqlx::query_scalar(
                "SELECT pg_try_advisory_xact_lock(hashtextextended('g5:provider-merge-target:' || $1::text || ':' || $2, 0))")
                .bind(repo).bind(branch).fetch_one(db.conn()).await?;
            if !held {
                return Err(Self::target_busy());
            }
        }
        Ok(())
    }

    async fn release_unsent(
        db: &mut ScopedDb,
        pr: Uuid,
        execution: Uuid,
        dispatched: &HashSet<Uuid>,
    ) -> Result<(), G5Error> {
        sqlx::query(
            "UPDATE workspace_pull_request_provider_repos SET merge_pending=false
             WHERE pull_request_id=$1 AND merge_execution_id=$2 AND merge_pending
               AND NOT merge_uncertain AND NOT (repo_id=ANY($3))",
        )
        .bind(pr)
        .bind(execution)
        .bind(dispatched.iter().copied().collect::<Vec<_>>())
        .execute(db.conn())
        .await?;
        Ok(())
    }

    pub(super) async fn release_refused_attempt(
        &self,
        db: &mut ScopedDb,
        row: &ProviderRepoRow,
    ) -> Result<(), G5Error> {
        sqlx::query(
            "UPDATE workspace_pull_request_provider_repos SET merge_pending=false
             WHERE id=$1 AND merge_attempt_id=$2 AND merge_execution_id=$3
               AND merge_pending AND NOT merge_uncertain",
        )
        .bind(row.id)
        .bind(row.merge_attempt_id)
        .bind(row.merge_execution_id)
        .execute(db.conn())
        .await?;
        Ok(())
    }
}
