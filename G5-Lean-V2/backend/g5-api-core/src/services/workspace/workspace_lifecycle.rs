//! Re-drive durable workspace VM lifecycle intents owned by the API.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::services::workspace_vm_service::IdleExemptionDiagnostics;
use crate::services::{AppState, WorkspaceVmService};
use crate::ws::{Channel, WsEvent};
use crate::{errors::AppError, maintenance::TenantMaintenanceDeps};

use crate::maintenance::{maintenance_org_ids, scoped_db_for_maintenance_org};

const RECONCILE_INTERVAL: Duration = Duration::from_secs(2);
const RECONCILE_BATCH: i64 = 16;
/// Personal workspace creation retries and backfill run less often than the
/// VM lifecycle intents (`g5.personal.lifecycle`).
const PERSONAL_RECONCILE_EVERY: u32 = 15;
const PERSONAL_RECONCILE_BATCH: i64 = 8;

pub fn spawn(
    state: Arc<AppState>,
    cancel: CancellationToken,
) -> Option<tokio::task::JoinHandle<()>> {
    let minter = state.jwt_minter.clone()?;
    Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(RECONCILE_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut idle_diagnostics = BTreeMap::new();
        let mut tick = 0_u32;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    let personal = tick.is_multiple_of(PERSONAL_RECONCILE_EVERY);
                    tick = tick.wrapping_add(1);
                    if let Err(error) =
                        run_once(&state, &minter, &mut idle_diagnostics, personal).await
                    {
                        tracing::warn!(%error, "workspace lifecycle org scan failed; retrying");
                    }
                },
            }
        }
    }))
}

async fn run_once(
    state: &Arc<AppState>,
    minter: &Arc<g5_auth::JwtMinter>,
    idle_diagnostics: &mut BTreeMap<uuid::Uuid, IdleExemptionDiagnostics>,
    personal: bool,
) -> Result<(), AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let org_ids: BTreeSet<_> = maintenance_org_ids(&state.system_pool)
        .await?
        .into_iter()
        .collect();
    idle_diagnostics.retain(|org_id, _| org_ids.contains(org_id));
    for org_id in org_ids {
        let db = scoped_db_for_maintenance_org(&deps, org_id).await?;
        let scope = db.reopener();
        db.rollback().await?;
        let service = WorkspaceVmService::new(crate::services::WorkspaceVmServiceConfig {
            system_pool: state.system_pool.clone(),
            scope: scope.clone(),
            hosts: state.preview_tunnels.clone(),
            minter: minter.clone(),
            runtime: state.workspace_runtime.clone(),
            agent_work: state.agent_work.clone(),
            repo_storage: state.repo_storage.clone(),
            git_external_url: state.git_external_url.clone(),
            ws_hub: state.event_bus.ws_hub().clone(),
        });
        run_org_once(&service, &state.event_bus).await;
        if let Err(error) = service
            .log_idle_exemptions(idle_diagnostics.entry(org_id).or_default(), RECONCILE_BATCH)
            .await
        {
            tracing::warn!(%error, %org_id, "idle exemption diagnostics deferred");
        }
        if personal {
            if let Err(error) = crate::services::workspace::personal_workspace::reconcile_org(
                state,
                org_id,
                PERSONAL_RECONCILE_BATCH,
            )
            .await
            {
                tracing::warn!(%error, %org_id, "personal workspace reconciliation deferred");
            }
        }
        if let Err(error) =
            crate::services::hosts::host_capacity::maintain(state, scope, org_id).await
        {
            tracing::warn!(%error, %org_id, "managed host capacity reconciliation deferred");
        }
    }
    Ok(())
}

async fn run_org_once(service: &WorkspaceVmService, event_bus: &crate::services::EventBus) {
    if let Err(error) = service.reconcile_member_deactivation(RECONCILE_BATCH).await {
        tracing::warn!(%error, "membership shutdown reconciliation failed; retrying");
    }
    if let Err(error) = service.reconcile_expired_leases().await {
        tracing::warn!(%error, "workspace lease reconciliation failed; retrying");
    }
    if let Err(error) = service.reconcile_host_stop_triggers(RECONCILE_BATCH).await {
        tracing::warn!(%error, "workspace host stop trigger reconciliation failed; retrying");
    }
    if let Err(error) = service.reconcile_pending_stops(RECONCILE_BATCH).await {
        tracing::warn!(%error, "workspace pending stop reconciliation failed; retrying");
    }
    if let Err(error) = service.reconcile_stopping(RECONCILE_BATCH).await {
        tracing::warn!(%error, "workspace stop reconciliation failed; retrying");
    }
    if let Err(error) = service
        .reconcile_automatic_continuations(RECONCILE_BATCH)
        .await
    {
        tracing::warn!(%error, "automatic continuation reconciliation failed; retrying");
    }
    match service.reconcile_pending_transfers(RECONCILE_BATCH).await {
        Ok(completed) => {
            for outcome in completed {
                event_bus.publish(
                    &Channel::Workspace(outcome.workspace_id),
                    &WsEvent::WorkspaceOwnershipTransferred {
                        workspace_id: outcome.workspace_id,
                        from_user_id: outcome.from_user_id,
                        to_user_id: outcome.to_user_id,
                        transfer_id: outcome.transfer_id,
                    },
                );
            }
        }
        Err(error) => {
            tracing::warn!(%error, "workspace transfer reconciliation failed; retrying");
        }
    }
    if let Err(error) = service.reconcile_start_queue(RECONCILE_BATCH).await {
        tracing::warn!(%error, "workspace start queue reconciliation failed; retrying");
    }
    if let Err(error) = service.reconcile_idle(RECONCILE_BATCH).await {
        tracing::warn!(%error, "workspace idle reconciliation failed; retrying");
    }
    if let Err(error) = service.reconcile_retired_personal(RECONCILE_BATCH).await {
        tracing::warn!(%error, "retired personal workspace stop reconciliation failed; retrying");
    }
}
