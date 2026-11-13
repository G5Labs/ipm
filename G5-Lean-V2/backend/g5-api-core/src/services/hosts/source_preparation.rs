//! Optional source preparation uses existing workspace access, never org ownership
//! alone. A grant is read-only and revalidated on every Git request.
use crate::services::git::repo_store::TenantRepoStorage;
use crate::services::git::workspace_repositories::{
    load_workspace_repositories, WorkspaceRepository,
};
use g5_auth::{HostSourceClaims, JwtMinter, HOST_SOURCE_TTL_SECS};
use g5_authz::{AuthzService, ScopedDb};
use g5_contracts::G5Error;
use g5_internal_api::host_ctl::{HostSourcePreparation, VmRepoSpec};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

fn denied() -> G5Error {
    G5Error::Unauthorized
}
fn database(error: impl std::fmt::Display) -> G5Error {
    G5Error::Internal {
        message: format!("host source authorization: {error}"),
    }
}

fn membership_digest(members: &[WorkspaceRepository], image: &str) -> String {
    let mut ordered: Vec<_> = members
        .iter()
        .map(|m| (m.repo_id, &m.root_path, &m.role, &m.branch_name))
        .collect();
    ordered.sort();
    // Length-delimited JSON avoids ambiguous field concatenation.
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(image, ordered)).expect("strings serialize"))
    )
}

// Prefer an unambiguous Base before the first Start. Otherwise derive a source
// only from an already admitted boot on this host, never a host-supplied repo list.
async fn selected_workspace(
    db: &mut ScopedDb,
    host: Uuid,
    org: Uuid,
    image: &str,
) -> Result<Uuid, G5Error> {
    let bases: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE org_id=$1 AND is_base AND deleted_at IS NULL ORDER BY id LIMIT 2"
    ).bind(org).fetch_all(db.conn()).await?;
    if bases.len() == 1 {
        return Ok(bases[0]);
    }
    sqlx::query_scalar(
        "SELECT v.workspace_id FROM workspace_vms v JOIN workspaces w ON w.id=v.workspace_id AND w.org_id=v.org_id
         WHERE v.host_id=$1 AND v.org_id=$2 AND v.status IN ('starting','running')
           AND v.boot_spec IS NOT NULL AND v.image_ref=$3 AND w.deleted_at IS NULL
         ORDER BY v.updated_at DESC, v.workspace_id LIMIT 1"
    ).bind(host).bind(org).bind(image).fetch_optional(db.conn()).await?.ok_or_else(denied)
}

async fn authorize(
    pool: &PgPool,
    db: &mut ScopedDb,
    host: Uuid,
    org: Uuid,
    actor: Uuid,
    workspace: Uuid,
) -> Result<(String, Vec<WorkspaceRepository>), G5Error> {
    let image = sqlx::query_scalar::<_, Option<String>>(
        "SELECT base_image_ref FROM worker_hosts WHERE host_id=$1 AND org_id=$2
         AND created_by_user_id=$3 AND status='ready' AND last_heartbeat >= now()-interval '30 seconds'"
    ).bind(host).bind(org).bind(actor).fetch_optional(db.conn()).await?.flatten().ok_or_else(denied)?;
    if !crate::db::scoped::user_can_access_workspace(pool, db, actor, org, workspace)
        .await
        .map_err(database)?
    {
        return Err(denied());
    }
    if selected_workspace(db, host, org, &image).await? != workspace {
        return Err(denied());
    }
    let members = load_workspace_repositories(db.conn(), workspace, false).await?;
    if members.is_empty() || members.len() > 64 || members.iter().any(|m| m.org_id != org) {
        return Err(denied());
    }
    Ok((image, members))
}

#[derive(Clone)]
pub(crate) struct SourcePreparationContext {
    pub storage: TenantRepoStorage,
    pub git_url: String,
}
impl SourcePreparationContext {
    pub async fn manifest(
        &self,
        pool: &PgPool,
        tenant_pool: &PgPool,
        authz: &dyn AuthzService,
        minter: &JwtMinter,
        host: Uuid,
        org: Uuid,
    ) -> Result<HostSourcePreparation, G5Error> {
        let mut db = crate::db::maintenance::maintenance_org_scope(authz, tenant_pool, org)
            .await
            .map_err(database)?;
        let actor = sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT created_by_user_id FROM worker_hosts WHERE host_id=$1 AND org_id=$2",
        )
        .bind(host)
        .bind(org)
        .fetch_optional(db.conn())
        .await?
        .flatten()
        .ok_or_else(denied)?;
        let image: String = sqlx::query_scalar(
            "SELECT base_image_ref FROM worker_hosts WHERE host_id=$1 AND org_id=$2 AND base_image_ref IS NOT NULL"
        ).bind(host).bind(org).fetch_optional(db.conn()).await?.ok_or_else(denied)?;
        let workspace = selected_workspace(&mut db, host, org, &image).await?;
        let (image_ref, members) = authorize(pool, &mut db, host, org, actor, workspace).await?;
        let digest = membership_digest(&members, &image_ref);
        let refs = members
            .iter()
            .map(|m| (m.repo_id, m.branch_name.clone()))
            .collect::<Vec<_>>();
        let mut heads = crate::services::git::workspace_reader::read_branch_head_oids(
            &self.storage.scoped(&db),
            org,
            &refs,
        )
        .await?;
        let repositories = members
            .iter()
            .map(|m| {
                Ok(VmRepoSpec {
                    repo_id: m.repo_id,
                    root_path: m.root_path.clone(),
                    role: Some(m.role.clone()),
                    git_url: format!("{}/git/{}", self.git_url.trim_end_matches('/'), m.repo_id),
                    branch: m.branch_name.clone(),
                    branch_oid: heads
                        .remove(&(m.repo_id, m.branch_name.clone()))
                        .flatten()
                        .ok_or_else(denied)?,
                    upstream_branch: Some(m.branch_name.clone()),
                    follow_branch: false,
                })
            })
            .collect::<Result<Vec<_>, G5Error>>()?;
        let token = minter.mint_host_source(
            org,
            host,
            workspace,
            actor,
            members.iter().map(|m| m.repo_id).collect(),
            digest.clone(),
        )?;
        db.rollback().await.map_err(database)?;
        Ok(HostSourcePreparation {
            org_id: org,
            host_id: host,
            source_workspace_id: workspace,
            image_ref,
            membership_digest: digest,
            repositories,
            expires_at: chrono::Utc::now().timestamp() + HOST_SOURCE_TTL_SECS,
            read_token: token,
        })
    }
}

pub async fn require_source_read(
    state: &crate::services::AppState,
    claims: &HostSourceClaims,
    repo: Uuid,
) -> Result<(), G5Error> {
    if !claims.repo_ids.contains(&repo) || claims.exp <= chrono::Utc::now().timestamp() {
        return Err(denied());
    }
    let mut db = crate::db::maintenance::maintenance_org_scope(
        state.authz.as_ref(),
        &state.tenant_pool,
        claims.org_id,
    )
    .await
    .map_err(database)?;
    let (image, members) = authorize(
        &state.system_pool,
        &mut db,
        claims.host_id,
        claims.org_id,
        claims.actor_user_id,
        claims.source_workspace_id,
    )
    .await?;
    let mut ids = members.iter().map(|m| m.repo_id).collect::<Vec<_>>();
    ids.sort();
    let mut granted = claims.repo_ids.clone();
    granted.sort();
    if ids != granted || membership_digest(&members, &image) != claims.membership_digest {
        return Err(denied());
    }
    db.rollback().await.map_err(database)?;
    Ok(())
}
