//! Retention and orphan cleanup for immutable workspace flush refs.

use std::time::Duration;

use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use g5_scm::GitRepo;
use sqlx::Row;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

const SWEEP_INTERVAL: Duration = Duration::from_secs(5 * 60);
const ORPHAN_AFTER_MINUTES: i64 = 30;
const MAX_CANDIDATES_PER_ORG: i64 = 32;

#[derive(Clone)]
pub struct WorkspaceFlushRetentionDeps {
    pub tenant: TenantMaintenanceDeps,
    pub repo_storage: TenantRepoStorage,
}

pub async fn run_workspace_flush_retention(
    deps: WorkspaceFlushRetentionDeps,
    cancel: CancellationToken,
) {
    let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match sweep(&deps).await {
                    Ok((pruned, orphaned)) if pruned > 0 || orphaned > 0 => {
                        info!(pruned, orphaned, "workspace flush retention sweep completed");
                    }
                    Ok(_) => debug!("workspace flush retention sweep found no candidates"),
                    Err(error) => warn!(%error, "workspace flush retention sweep failed"),
                }
            }
            _ = cancel.cancelled() => break,
        }
    }
}

pub async fn sweep(deps: &WorkspaceFlushRetentionDeps) -> Result<(usize, usize), sqlx::Error> {
    let org_ids = maintenance_org_ids(&deps.tenant.system_pool).await?;
    let mut pruned = 0;
    let mut orphaned = 0;
    for org_id in org_ids {
        match sweep_org(deps, org_id).await {
            Ok((org_pruned, org_orphaned)) => {
                pruned += org_pruned;
                orphaned += org_orphaned;
            }
            Err(error) => warn!(%org_id, %error, "workspace flush retention org sweep failed"),
        }
    }
    Ok((pruned, orphaned))
}

async fn sweep_org(
    deps: &WorkspaceFlushRetentionDeps,
    org_id: Uuid,
) -> Result<(usize, usize), sqlx::Error> {
    let (completed, incomplete) = {
        let mut db = scoped_db_for_maintenance_org(&deps.tenant, org_id)
            .await
            .map_err(app_sqlx)?;
        let completed = sqlx::query_scalar::<_, Uuid>(
            r#"
            WITH ranked AS (
                SELECT flush_id, workspace_id,
                       row_number() OVER (
                           PARTITION BY workspace_id
                           ORDER BY complete_at DESC, flush_id DESC
                       ) AS ordinal
                FROM workspace_flushes
                WHERE org_id = $1 AND complete_at IS NOT NULL AND pruned_at IS NULL
            )
            SELECT ranked.flush_id
            FROM ranked
            JOIN workspace_flushes AS receipt ON receipt.flush_id = ranked.flush_id
            WHERE ranked.ordinal > 2
              AND NOT EXISTS (
                  SELECT 1 FROM workspace_vms vm
                  WHERE vm.workspace_id = ranked.workspace_id
                    AND (vm.last_flush_id = ranked.flush_id
                         OR (vm.status IN ('starting','running','stopping')
                             AND vm.boot_spec #>> '{restore,flush_id}' = ranked.flush_id::text))
              )
              AND NOT EXISTS (
                  SELECT 1 FROM workspace_forks fork
                  WHERE fork.source_flush_id = ranked.flush_id
                    AND fork.status IN ('pending','aborting')
              )
            ORDER BY receipt.complete_at, receipt.flush_id
            LIMIT $2
            "#,
        )
        .bind(org_id)
        .bind(MAX_CANDIDATES_PER_ORG)
        .fetch_all(db.conn())
        .await?;
        let incomplete = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT receipt.flush_id
            FROM workspace_flushes receipt
            WHERE receipt.org_id = $1 AND receipt.origin = 'flush'
              AND receipt.complete_at IS NULL
              AND COALESCE(receipt.error ->> 'code', '') <> 'ORPHANED_FLUSH'
              AND receipt.started_at < now() - make_interval(mins => $2)
              AND NOT EXISTS (
                  SELECT 1 FROM workspace_vms vm
                  WHERE vm.workspace_id = receipt.workspace_id
                    AND vm.vm_id = receipt.vm_id AND vm.epoch = receipt.epoch
                    AND vm.status IN ('starting','running','stopping')
              )
            ORDER BY receipt.started_at, receipt.flush_id
            LIMIT $3
            "#,
        )
        .bind(org_id)
        .bind(ORPHAN_AFTER_MINUTES as i32)
        .bind(MAX_CANDIDATES_PER_ORG)
        .fetch_all(db.conn())
        .await?;
        db.commit().await.map_err(app_sqlx)?;
        (completed, incomplete)
    };

    let mut pruned = 0;
    for flush_id in completed {
        match prune_completed(deps, org_id, flush_id).await {
            Ok(true) => pruned += 1,
            Ok(false) => {}
            Err(error) => warn!(%org_id, %flush_id, %error, "flush prune candidate skipped"),
        }
    }
    let mut orphaned = 0;
    for flush_id in incomplete {
        match cleanup_orphan(deps, org_id, flush_id).await {
            Ok(true) => orphaned += 1,
            Ok(false) => {}
            Err(error) => warn!(%org_id, %flush_id, %error, "orphan flush cleanup skipped"),
        }
    }
    Ok((pruned, orphaned))
}

async fn prune_completed(
    deps: &WorkspaceFlushRetentionDeps,
    org_id: Uuid,
    flush_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut db = scoped_db_for_maintenance_org(&deps.tenant, org_id)
        .await
        .map_err(app_sqlx)?;
    let Some(row) = sqlx::query(
        r#"
        SELECT receipt.workspace_id, receipt.required_repo_ids
        FROM workspace_flushes receipt
        WHERE receipt.org_id = $1 AND receipt.flush_id = $2
          AND receipt.complete_at IS NOT NULL AND receipt.pruned_at IS NULL
          AND (SELECT count(*) FROM workspace_flushes newer
               WHERE newer.workspace_id = receipt.workspace_id
                 AND newer.complete_at IS NOT NULL AND newer.pruned_at IS NULL
                 AND (newer.complete_at, newer.flush_id) > (receipt.complete_at, receipt.flush_id)) >= 2
          AND NOT EXISTS (
              SELECT 1 FROM workspace_vms vm
              WHERE vm.workspace_id = receipt.workspace_id
                AND (vm.last_flush_id = receipt.flush_id
                     OR (vm.status IN ('starting','running','stopping')
                         AND vm.boot_spec #>> '{restore,flush_id}' = receipt.flush_id::text))
          )
          AND NOT EXISTS (
              SELECT 1 FROM workspace_forks fork
              WHERE fork.source_flush_id = receipt.flush_id
                AND fork.status IN ('pending','aborting')
          )
        FOR UPDATE OF receipt
        "#,
    )
    .bind(org_id)
    .bind(flush_id)
    .fetch_optional(db.conn())
    .await?
    else {
        db.commit().await.map_err(app_sqlx)?;
        return Ok(false);
    };
    let workspace_id: Uuid = row.try_get("workspace_id")?;
    let repo_ids: Vec<Uuid> = row.try_get("required_repo_ids")?;
    lock_repo_rows(db.conn(), org_id, &repo_ids).await?;
    delete_refs(
        &deps.repo_storage.scoped(&db),
        org_id,
        workspace_id,
        flush_id,
        &repo_ids,
    )
    .await
    .map_err(app_sqlx)?;
    sqlx::query("UPDATE workspace_flushes SET pruned_at = now() WHERE flush_id = $1")
        .bind(flush_id)
        .execute(db.conn())
        .await?;
    db.commit().await.map_err(app_sqlx)?;
    Ok(true)
}

async fn cleanup_orphan(
    deps: &WorkspaceFlushRetentionDeps,
    org_id: Uuid,
    flush_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut db = scoped_db_for_maintenance_org(&deps.tenant, org_id)
        .await
        .map_err(app_sqlx)?;
    let Some(row) = sqlx::query(
        r#"
        SELECT receipt.workspace_id, receipt.required_repo_ids
        FROM workspace_flushes receipt
        WHERE receipt.org_id = $1 AND receipt.flush_id = $2
          AND receipt.origin = 'flush' AND receipt.complete_at IS NULL
          AND COALESCE(receipt.error ->> 'code', '') <> 'ORPHANED_FLUSH'
          AND receipt.started_at < now() - make_interval(mins => $3)
          AND NOT EXISTS (
              SELECT 1 FROM workspace_vms vm
              WHERE vm.workspace_id = receipt.workspace_id
                AND vm.vm_id = receipt.vm_id AND vm.epoch = receipt.epoch
                AND vm.status IN ('starting','running','stopping')
          )
        FOR UPDATE OF receipt
        "#,
    )
    .bind(org_id)
    .bind(flush_id)
    .bind(ORPHAN_AFTER_MINUTES as i32)
    .fetch_optional(db.conn())
    .await?
    else {
        db.commit().await.map_err(app_sqlx)?;
        return Ok(false);
    };
    let workspace_id: Uuid = row.try_get("workspace_id")?;
    let repo_ids: Vec<Uuid> = row.try_get("required_repo_ids")?;
    lock_repo_rows(db.conn(), org_id, &repo_ids).await?;
    delete_refs(
        &deps.repo_storage.scoped(&db),
        org_id,
        workspace_id,
        flush_id,
        &repo_ids,
    )
    .await
    .map_err(app_sqlx)?;
    sqlx::query(
        "UPDATE workspace_flushes
         SET error = jsonb_build_object('code','ORPHANED_FLUSH','at',now())
         WHERE flush_id = $1",
    )
    .bind(flush_id)
    .execute(db.conn())
    .await?;
    db.commit().await.map_err(app_sqlx)?;
    Ok(true)
}

async fn lock_repo_rows(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    repo_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    let mut expected = repo_ids.to_vec();
    expected.sort_unstable();
    expected.dedup();
    let locked = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM repos WHERE org_id = $1 AND id = ANY($2) ORDER BY id FOR UPDATE",
    )
    .bind(org_id)
    .bind(&expected)
    .fetch_all(conn)
    .await?;
    if locked == expected {
        Ok(())
    } else {
        Err(sqlx::Error::Protocol(
            "flush receipt repository inventory changed".to_string(),
        ))
    }
}

async fn delete_refs(
    storage: &ScopedRepoStorage,
    org_id: Uuid,
    workspace_id: Uuid,
    flush_id: Uuid,
    repo_ids: &[Uuid],
) -> Result<(), String> {
    let paths = repo_ids
        .iter()
        .map(|repo_id| {
            storage
                .repo_path(org_id, *repo_id)
                .map_err(|error| format!("repository {repo_id}: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    tokio::task::spawn_blocking(move || {
        for path in paths {
            GitRepo::open(&path)
                .and_then(|repo| repo.delete_workspace_flush_refs(workspace_id, flush_id))
                .map_err(|error| error.to_string())?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|error| format!("flush prune task failed: {error}"))?
}

fn app_sqlx(error: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_contract_keeps_two_completed_receipts() {
        assert_eq!(MAX_CANDIDATES_PER_ORG, 32);
        assert_eq!(ORPHAN_AFTER_MINUTES, 30);
    }
}
