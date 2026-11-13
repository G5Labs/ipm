//! Stub implementations for unit tests.
//!
//! A placeholder `AuthzService` for unit tests that build an `AppState`
//! without exercising authorization.

use async_trait::async_trait;

use g5_contracts::types::ids::{OrgId, SessionId, UserId};

// ---------------------------------------------------------------------------
// StubAuthz
// ---------------------------------------------------------------------------

/// Stub authz that always grants access. Test-only.
pub struct StubAuthz;

#[async_trait]
impl g5_authz::AuthzService for StubAuthz {
    async fn is_org_member(
        &self,
        _actor_id: UserId,
        _org_id: OrgId,
    ) -> Result<bool, g5_authz::AuthzError> {
        Ok(true)
    }

    async fn mint_db_capability(
        &self,
        actor_id: UserId,
        target_org_ids: &[OrgId],
        scopes: &[String],
    ) -> Result<g5_authz::DbCapability, g5_authz::AuthzError> {
        Ok(g5_authz::DbCapability {
            org_ids: target_org_ids.to_vec(),
            actor_id,
            scopes: scopes.to_vec(),
            session_id: SessionId::new(),
            ttl_secs: 3600,
        })
    }

    async fn mint_service_db_capability(
        &self,
        actor_id: UserId,
        org_id: OrgId,
        scopes: &[String],
    ) -> Result<g5_authz::DbCapability, g5_authz::AuthzError> {
        Ok(g5_authz::DbCapability {
            org_ids: vec![org_id],
            actor_id,
            scopes: scopes.to_vec(),
            session_id: SessionId::new(),
            ttl_secs: 3600,
        })
    }

    async fn user_org_ids(&self, _actor_id: UserId) -> Result<Vec<OrgId>, g5_authz::AuthzError> {
        Ok(vec![])
    }
}
