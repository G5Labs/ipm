//! Capability type for scoped DB access.

use g5_contracts::types::ids::{OrgId, SessionId, UserId};

/// Capability to execute queries scoped to one or more orgs.
///
/// Contains the signed context needed for DB session injection.
/// `session_id` references a row in `authz.user_sessions`, looked up
/// by `authz.set_ctx(UUID)`. Sessions are **reusable** — the same
/// session_id is valid for multiple requests while the reusable row is fresh.
#[derive(Debug, Clone)]
pub struct DbCapability {
    /// The org(s) this capability is scoped to.
    pub org_ids: Vec<OrgId>,
    /// The actor (user or worker) who requested this capability.
    pub actor_id: UserId,
    /// Effective scopes for this session.
    pub scopes: Vec<String>,
    /// Reusable UUID referencing an `authz.user_sessions` row.
    pub session_id: SessionId,
    /// Remaining session TTL in seconds.
    pub ttl_secs: u32,
}
