//! Reconcile process-local host tunnels with the durable host registry.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{stream, StreamExt};
use g5_internal_api::host_ctl::CommandResult;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::AppState;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};

const RECONCILE_INTERVAL: Duration = Duration::from_secs(2);
const DELIVERY_WAIT: Duration = Duration::from_secs(1);

pub fn spawn(state: Arc<AppState>, cancel: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(RECONCILE_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    if let Err(error) = run_once(&state).await {
                        tracing::warn!(%error, "host registry reconciliation failed; retrying");
                    }
                }
            }
        }
    })
}

pub async fn run_once(state: &Arc<AppState>) -> Result<(), AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let mut drains = Vec::new();
    let mut terminated = Vec::new();
    for org_id in maintenance_org_ids(&state.system_pool).await? {
        let mut db = scoped_db_for_maintenance_org(&deps, org_id).await?;
        let unreachable = state.worker_host_service.mark_unreachable(&mut db).await?;
        if unreachable > 0 {
            tracing::warn!(
                target: "g5_audit",
                event = "worker_host.unreachable",
                %org_id,
                count = unreachable,
                "Hosts missed three control heartbeat intervals"
            );
        }
        drains.extend(
            sqlx::query_as::<_, (Uuid, Uuid)>(
                "SELECT host_id, drain_id FROM worker_hosts
                 WHERE status = 'draining' AND drain_id IS NOT NULL",
            )
            .fetch_all(db.conn())
            .await?,
        );
        terminated.extend(
            sqlx::query_scalar::<_, Uuid>(
                "SELECT host_id FROM worker_hosts WHERE status = 'terminated'",
            )
            .fetch_all(db.conn())
            .await?,
        );
        db.commit().await?;
    }
    stream::iter(drains)
        .for_each_concurrent(Some(16), |(host_id, drain_id)| {
            let state = state.clone();
            async move {
                let Some(delivery) = state
                    .worker_host_service
                    .deliver_recorded_drain(
                        &state.preview_tunnels,
                        host_id,
                        drain_id,
                        DELIVERY_WAIT,
                        1,
                    )
                    .await
                else {
                    return;
                };
                match delivery {
                    Ok(CommandResult::Accepted) => {}
                    Ok(result) => tracing::debug!(
                        %host_id,
                        %drain_id,
                        ?result,
                        "durable host drain delivery deferred"
                    ),
                    Err(error) => tracing::debug!(
                        %host_id,
                        %drain_id,
                        %error,
                        "durable host drain delivery remains pending"
                    ),
                }
            }
        })
        .await;

    for host_id in terminated {
        state.preview_tunnels.terminate(&host_id).await;
    }
    Ok(())
}
