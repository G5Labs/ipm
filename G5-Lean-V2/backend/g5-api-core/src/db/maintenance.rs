//! The maintenance actor's database scope: one organization at a time on
//! the tenant pool, under the same row rules as any other tenant caller.
//! Background jobs and the service paths that run as the maintenance
//! actor (host reconciliation, the integration worker, cloud rotation)
//! open their transactions here.

use crate::services::git::repo_store::TenantRepoStorage;
use std::sync::Arc;

use g5_authz::{AuthzService, ScopedDb};
use g5_contracts::types::ids::{OrgId, UserId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Clone)]
pub struct TenantMaintenanceDeps {
    pub system_pool: PgPool,
    pub tenant_pool: PgPool,
    pub authz: Arc<dyn AuthzService>,
    /// Repository storage, used to resolve branch heads from git refs.
    pub repo_storage: TenantRepoStorage,
}

pub const SYSTEM_MAINTENANCE_ACTOR_UUID: Uuid =
    Uuid::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

pub async fn maintenance_org_ids(system_pool: &PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT id AS "id!" FROM orgs ORDER BY created_at ASC"#)
        .fetch_all(system_pool)
        .await
}

pub async fn scoped_db_for_maintenance_org(
    deps: &TenantMaintenanceDeps,
    org_id: Uuid,
) -> Result<ScopedDb, AppError> {
    maintenance_org_scope(deps.authz.as_ref(), &deps.tenant_pool, org_id).await
}

/// Open the maintenance actor's transaction scoped to one organization on the
/// tenant pool, under the same row rules as any other tenant caller.
pub(crate) async fn maintenance_org_scope(
    authz: &dyn AuthzService,
    tenant_pool: &PgPool,
    org_id: Uuid,
) -> Result<ScopedDb, AppError> {
    let capability = authz
        .mint_service_db_capability(
            UserId::from_uuid(SYSTEM_MAINTENANCE_ACTOR_UUID),
            OrgId::from_uuid(org_id),
            &["*".to_string()],
        )
        .await
        .map_err(AppError::from)?;

    ScopedDb::from_capability(tenant_pool, capability)
        .await
        .map_err(AppError::from)
}
