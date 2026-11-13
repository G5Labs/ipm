//! Open PRs are the durable inventory; published Git heads detect missed pushes.
//! Events shorten latency, while startup and periodic scans recover lost hints.
use std::{sync::Arc, time::Duration};

use sqlx::{Connection, PgConnection};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::maintenance::{
    maintenance_org_ids, scoped_db_for_maintenance_org, TenantMaintenanceDeps,
};
use crate::{
    errors::AppError,
    services::{
        workspace_pull_request_create, workspace_pull_request_provider_lifecycle, AppState,
    },
};

pub fn spawn(state: Arc<AppState>, cancel: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_loop(state, cancel))
}

/// One scan owner across API replicas; per-PR locks still authorize effects.
const SCAN_LOCK_KEY: i64 = 0x4735_5052_5343_414e;

async fn claim_scan(state: &Arc<AppState>) -> Result<Option<PgConnection>, AppError> {
    claim_scan_lock(state, SCAN_LOCK_KEY).await
}

/// A dedicated connection holding the session advisory lock `key`, or `None`
/// when another replica owns that scan. Do not reserve a pooled slot across
/// provider I/O: dropping this connection releases the lock even if the
/// owning future is cancelled.
pub async fn claim_scan_lock(
    state: &Arc<AppState>,
    key: i64,
) -> Result<Option<PgConnection>, AppError> {
    let mut connection = PgConnection::connect_with(&state.system_pool.connect_options()).await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(key)
        .fetch_one(&mut connection)
        .await?;
    Ok(acquired.then_some(connection))
}

pub async fn run_loop(state: Arc<AppState>, cancel: CancellationToken) {
    let mut owner: Option<PgConnection> = None;
    // Settling runs span passes, so the loop owns their budget.
    let mut settling = workspace_pull_request_create::SettlingRefreshes::new(
        workspace_pull_request_create::SETTLING_REFRESH_BUDGET,
    );
    let mut hints = state.event_bus.subscribe_branch_updates();
    let mut interval = tokio::time::interval(Duration::from_secs(15));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let workspace = tokio::select! {
            _ = cancel.cancelled() => return,
            _ = interval.tick() => None,
            event = hints.recv() => match event {
                Ok(crate::ws::events::WsEvent::WorkspaceBranchUpdated { workspace_id, .. }) => Some(workspace_id),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                Ok(_) => continue,
            }
        };
        tokio::select! {
            _ = cancel.cancelled() => return,
            result = async {
                if owner.is_none() {
                    owner = claim_scan(&state).await?;
                }
                let Some(connection) = owner.as_mut() else { return Ok(0) };
                // A lost connection also loses leadership. Re-admit before
                // discovery on every tick, rather than retaining a stale claim.
                if let Err(error) = connection.ping().await {
                    owner = None;
                    return Err(AppError::from(error));
                }
                scan(&state, workspace, workspace_pull_request_create::PUSH_REFRESH_TIMEOUT, &mut settling).await
            } => if let Err(error) = result {
                tracing::warn!(%error, "PR push refresh scan remains pending");
            }
        }
    }
}

/// One full pass always visits later PRs despite an earlier failure. Scoped
/// discovery grants no publication authority; each operation re-admits its owner.
pub async fn run_once(state: &Arc<AppState>, source: Option<Uuid>) -> Result<usize, AppError> {
    run_once_with_timeout(
        state,
        source,
        workspace_pull_request_create::PUSH_REFRESH_TIMEOUT,
    )
    .await
}

/// Run the same scan with an explicit per-PR publication budget. Failure recording
/// retains its separate bounded budget, including authorization admission.
pub async fn run_once_with_timeout(
    state: &Arc<AppState>,
    source: Option<Uuid>,
    timeout: Duration,
) -> Result<usize, AppError> {
    let mut settling = workspace_pull_request_create::SettlingRefreshes::new(
        workspace_pull_request_create::SETTLING_REFRESH_BUDGET,
    );
    run_once_tracked(state, source, timeout, &mut settling).await
}

/// One pass that continues the caller's settling runs, as successive loop ticks do.
pub async fn run_once_tracked(
    state: &Arc<AppState>,
    source: Option<Uuid>,
    timeout: Duration,
    settling: &mut workspace_pull_request_create::SettlingRefreshes,
) -> Result<usize, AppError> {
    let Some(owner) = claim_scan(state).await? else {
        return Ok(0);
    };
    let result = scan(state, source, timeout, settling).await;
    owner.close().await?;
    result
}

async fn scan(
    state: &Arc<AppState>,
    source: Option<Uuid>,
    timeout: Duration,
    settling: &mut workspace_pull_request_create::SettlingRefreshes,
) -> Result<usize, AppError> {
    let deps = TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    };
    let mut refreshed = 0;
    let mut open = std::collections::HashSet::new();
    for org in maintenance_org_ids(&state.system_pool).await? {
        let mut db = scoped_db_for_maintenance_org(&deps, org).await?;
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM workspace_pull_requests WHERE status='open'
               AND ($1::uuid IS NULL OR source_workspace_id=$1) ORDER BY id",
        )
        .bind(source)
        .fetch_all(db.conn())
        .await?;
        db.rollback().await?;
        open.extend(ids.iter().copied());
        for id in ids {
            // The shared owner bounds the attempt and records deadline failure
            // before returning, so a timeout does not cancel error persistence.
            match workspace_pull_request_create::refresh_after_push_tracked(
                state, org, id, timeout, settling,
            )
            .await
            {
                Ok(true) => refreshed += 1,
                Ok(false) => {}
                Err(error) if workspace_pull_request_create::is_settling(&error.inner) => {
                    tracing::debug!(pull_request_id=%id, %error, "Automatic PR refresh is settling")
                }
                // Reported once in the PR's history; later passes make no provider read.
                Err(error)
                    if workspace_pull_request_provider_lifecycle::is_provider_pull_request_completed(
                        &error.inner,
                    ) =>
                {
                    tracing::debug!(pull_request_id=%id, %error, "Automatic PR refresh is paused for a completed provider PR")
                }
                Err(error) => {
                    tracing::warn!(pull_request_id=%id, %error, "Automatic PR refresh will retry")
                }
            }
        }
    }
    if source.is_none() {
        settling.retain(&open);
    }
    Ok(refreshed)
}
