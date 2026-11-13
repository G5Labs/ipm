//! Cheap remote-tip observations for the UI. Policy is durable; observations
//! expire in the shared cache and never authorize Git mutations. Each changed
//! observation also leaves an operation audit record for troubleshooting.

use crate::services::git::repo_store::TenantRepoStorage;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgConnection;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::auth::RepositoryOperation;
use crate::errors::AppError;
use crate::services::remote_integrations::{self, RemoteIntegration};
use crate::services::EventBus;
use crate::ws::{channels::Channel, events::WsEvent};
use g5_auth::credential_scrub::scrub_opt;
use g5_scm::GitAuth;

use crate::maintenance::{scoped_db_for_maintenance_org, TenantMaintenanceDeps};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Global advisory-lock key — only one replica runs the pinger at a
/// time. "RAPN" = Remote Advisory PiNger.
const ADVISORY_LOCK_KEY_GLOBAL: i64 = 0x5241_504E;

/// Maximum time the pinger waits for a single ls-remote round-trip
/// before considering it stalled.
const LS_REMOTE_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum number of branches pinged in one sweep.
const MAX_BRANCHES_PER_SWEEP: usize = 64;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RemoteAdvisoryPingerDeps {
    pub system_pool: sqlx::PgPool,
    pub tenant_pool: sqlx::PgPool,
    pub authz: Arc<dyn g5_authz::AuthzService>,
    pub repository_credentials: Arc<crate::auth::RepositoryCredentials>,
    pub event_bus: EventBus,
    pub repo_storage: TenantRepoStorage,
}

impl RemoteAdvisoryPingerDeps {
    fn tenant_deps(&self) -> TenantMaintenanceDeps {
        TenantMaintenanceDeps {
            system_pool: self.system_pool.clone(),
            tenant_pool: self.tenant_pool.clone(),
            authz: self.authz.clone(),
            repo_storage: self.repo_storage.clone(),
        }
    }
}

/// Runs the pinger loop until `cancel` fires.
pub async fn run_remote_advisory_pinger(
    deps: RemoteAdvisoryPingerDeps,
    interval: Duration,
    passive_auto_pull_enabled: bool,
    cancel: CancellationToken,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // Runtime kill switch (Option 1, #1198; scheduler ontology D7):
                // skip the advisory sweep — no candidate query, no ls-remote —
                // when the passive policy is disabled.
                if !passive_auto_pull_enabled {
                    debug!(
                        passive_auto_pull_enabled = false,
                        "remote_advisory_pinger: passive auto-pull disabled by kill switch; skipping sweep"
                    );
                    continue;
                }
                match do_pinger_sweep(&deps).await {
                    Ok(updated) if updated > 0 => {
                        info!(
                            updated,
                            "remote_advisory_pinger: sweep completed"
                        );
                    }
                    Ok(_) => {
                        debug!("remote_advisory_pinger: no dirty remote-tracking branches with advisory drift");
                    }
                    Err(e) => {
                        error!(error = %e, "remote_advisory_pinger: sweep failed");
                    }
                }
            }
            _ = cancel.cancelled() => {
                info!("remote_advisory_pinger: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sweep
// ---------------------------------------------------------------------------

async fn do_pinger_sweep(deps: &RemoteAdvisoryPingerDeps) -> Result<usize, AppError> {
    // Resolve the org list BEFORE taking the advisory-lock connection — see the
    // scheduler's note: holding a `system_pool` connection for the replica lock
    // while `maintenance_org_ids` acquires another deadlocks a small pool when
    // the scheduler holds its own lock connection concurrently. Per-org pings
    // use `tenant_pool`, so the lock connection is the only `system_pool`
    // connection held here.
    let org_ids = crate::maintenance::maintenance_org_ids(&deps.system_pool)
        .await
        .map_err(AppError::from)?;

    let mut system_conn = deps.system_pool.acquire().await.map_err(AppError::from)?;
    let acquired: bool = sqlx::query_scalar(r#"SELECT pg_try_advisory_lock($1)"#)
        .bind(ADVISORY_LOCK_KEY_GLOBAL)
        .fetch_one(&mut *system_conn)
        .await
        .map_err(AppError::from)?;

    if !acquired {
        return Ok(0);
    }

    let result = ping_orgs(deps, org_ids).await;

    let _: bool = sqlx::query_scalar(r#"SELECT pg_advisory_unlock($1)"#)
        .bind(ADVISORY_LOCK_KEY_GLOBAL)
        .fetch_one(&mut *system_conn)
        .await
        .map_err(AppError::from)?;

    result
}

async fn ping_orgs(deps: &RemoteAdvisoryPingerDeps, org_ids: Vec<Uuid>) -> Result<usize, AppError> {
    let tenant_deps = deps.tenant_deps();

    let mut updated = 0;
    for org_id in org_ids {
        if updated >= MAX_BRANCHES_PER_SWEEP {
            break;
        }
        match ping_one_org(deps, &tenant_deps, org_id).await {
            Ok(n) => updated += n,
            Err(e) => {
                warn!(
                    org_id = %org_id,
                    error = %e,
                    "remote_advisory_pinger: org sweep failed"
                );
            }
        }
    }
    Ok(updated)
}

type PingTarget = RemoteIntegration;

async fn ping_one_org(
    deps: &RemoteAdvisoryPingerDeps,
    tenant_deps: &TenantMaintenanceDeps,
    org_id: Uuid,
) -> Result<usize, AppError> {
    // Phase 1: load targets in a short transaction.
    let targets = {
        let mut db = scoped_db_for_maintenance_org(tenant_deps, org_id).await?;
        let targets = load_targets(db.conn()).await?;
        db.commit().await.map_err(AppError::from)?;
        targets
    };

    let mut updated = 0;
    for target in targets {
        // Each target gets its own transaction so a slow ls-remote
        // does not hold the tenant connection longer than necessary.
        match ping_one_target(deps, tenant_deps, &target).await {
            Ok(was_updated) => {
                if was_updated {
                    updated += 1;
                }
            }
            Err(e) => {
                warn!(
                    workspace_id = %target.workspace_id,
                    repo_id = %target.repo_id,
                    error = %e,
                    "remote_advisory_pinger: target ping failed"
                );
            }
        }
    }
    Ok(updated)
}

async fn load_targets(conn: &mut PgConnection) -> Result<Vec<PingTarget>, AppError> {
    Ok(
        remote_integrations::passive_candidates(conn, MAX_BRANCHES_PER_SWEEP as i64)
            .await?
            .into_iter()
            .filter(|target| target.acting_user_id.is_some())
            .collect(),
    )
}

async fn ping_one_target(
    deps: &RemoteAdvisoryPingerDeps,
    tenant_deps: &TenantMaintenanceDeps,
    target: &PingTarget,
) -> Result<bool, AppError> {
    let outcome = match resolve_pinger_auth(deps, target).await {
        Ok(auth) => ls_remote(&target.git_url, &target.remote_branch, &auth).await,
        Err(error) => Err(error),
    };
    let mut db = scoped_db_for_maintenance_org(tenant_deps, target.org_id).await?;
    let Some(current) = remote_integrations::load(db.conn(), target.key(), true).await? else {
        return Ok(false);
    };
    if current.git_url != target.git_url
        || current.remote_branch != target.remote_branch
        || current.local_branch != target.local_branch
        || current.acting_user_id != target.acting_user_id
        || !current.auto_pull_enabled
        || current.sync_paused_reason.is_some()
    {
        return Ok(false);
    }
    match outcome {
        Ok(oid) => {
            // The hint and its audit row commit together; the row lock inside
            // `observe_tip` makes `changed` exact across concurrent reporters.
            let changed = remote_integrations::observe_tip(db.conn(), &current, &oid).await?;
            if changed {
                write_advisory_audit_row(db.conn(), &current, Some(&oid), None).await?;
            }
            db.commit().await.map_err(AppError::from)?;
            if changed {
                emit_advisory_event(&deps.event_bus, &current, &oid);
            }
            Ok(changed)
        }
        Err(error) => {
            write_advisory_audit_row(db.conn(), &current, None, scrub_opt(Some(error))).await?;
            db.commit().await.map_err(AppError::from)?;
            Ok(false)
        }
    }
}

async fn ls_remote(git_url: &str, remote_branch: &str, auth: &GitAuth) -> Result<String, String> {
    crate::git_runtime::remote_branch_tip(git_url, remote_branch, auth, LS_REMOTE_TIMEOUT)
        .await
        .map_err(|error| error.to_string())
}

async fn resolve_pinger_auth(
    deps: &RemoteAdvisoryPingerDeps,
    target: &PingTarget,
) -> Result<GitAuth, String> {
    deps.repository_credentials
        .resolve(
            target.org_id,
            target.repo_id,
            target
                .acting_user_id
                .ok_or_else(|| "workspace_owner_missing".to_string())?,
            &target.git_url,
            RepositoryOperation::ReadGit,
        )
        .await
        .map_err(|error| error.to_string())
}

async fn write_advisory_audit_row(
    conn: &mut PgConnection,
    target: &PingTarget,
    advisory_sha: Option<&str>,
    error_detail: Option<String>,
) -> Result<(), AppError> {
    let status = if advisory_sha.is_some() {
        "advisory_updated"
    } else {
        "fetch_failed"
    };
    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, workspace_id, repo_id, acting_user_id,
            kind, trigger_kind, status,
            new_mirror_tip, error_detail,
            started_at, completed_at
        ) VALUES (
            $1, $2, $3, $4,
            'advisory', 'ping', $5,
            $6, $7,
            now(), now()
        )
        "#,
    )
    .bind(target.org_id)
    .bind(target.workspace_id)
    .bind(target.repo_id)
    .bind(target.acting_user_id)
    .bind(status)
    .bind(advisory_sha)
    .bind(error_detail)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

fn emit_advisory_event(event_bus: &EventBus, target: &PingTarget, new_sha: &str) {
    event_bus.publish(
        &Channel::Workspace(target.workspace_id),
        &WsEvent::RemoteTipAdvisoryUpdated {
            workspace_id: target.workspace_id,
            repo_id: target.repo_id,
            remote_branch: target.remote_branch.clone(),
            remote_tip_advisory_sha: new_sha.to_string(),
        },
    );
}
