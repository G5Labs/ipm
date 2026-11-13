//! Worker-host JWT DB-liveness validation (rev-7 §5.7).
//!
//! A `host_jwt` is a long-lived bearer token (hours), so JWT signature
//! validity alone is not enough to authenticate the host: the underlying
//! `worker_hosts` row may have been deleted (org cascade), reassigned to
//! a different org, or moved into a `draining`/`terminated` state since
//! the token was minted.
//!
//! The middleware therefore consults the registry on every host-scoped
//! request through this trait. The production implementation
//! ([`PgHostAuthValidator`]) calls a narrowly-scoped SECURITY DEFINER
//! function (`authz.lookup_worker_host_for_auth`, migration 080) on the
//! privileged system pool. The function returns at most one row for
//! `(host_id)` and bypasses the `worker_hosts` `tenant_isolation` RLS
//! policy by design: the auth path runs BEFORE the org has been bound
//! to the request, so `authz.current_org_ids()` is empty and the policy
//! would reject every lookup. The system pool (`g5_app` in production)
//! holds EXECUTE only on the function, not direct SELECT on the table,
//! which keeps the broader `worker_hosts` surface scoped to `g5_tenant`.
//! Tests inject a deterministic in-memory implementation so the rejection
//! paths are exercised without a live Postgres.
//!
//! See `g5/g5-api/worker_hosts` [D5] and [D7] for the contract, and
//! `authz.lookup_worker_host_for_auth` in the baseline schema for the lookup function.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// The outcome of `worker_hosts` DB-liveness validation for a `host_jwt`.
///
/// The middleware translates each variant into an HTTP status and a
/// structured error envelope; the trait does not concern itself with
/// HTTP shape so this enum stays usable from other call sites (smoke
/// tests, ops tooling).
#[derive(Debug, PartialEq, Eq)]
pub enum HostAuthError {
    /// The `host_id` claim does not correspond to any row in `worker_hosts`.
    /// Either the host was deleted or the token was minted for a host that
    /// never existed (tampering).
    NotFound,
    /// The `host_id` exists, but its `org_id` does not match the `org` claim
    /// in the JWT. This is a token-reassignment / token-theft signal — the
    /// JWT was minted for one org but is being presented with another org's
    /// claim. The middleware rejects unconditionally.
    OrgMismatch,
    /// The host row exists but is no longer in an acceptable state. Carries
    /// the offending status (`"terminated"`, or an unknown future state) for tracing /
    /// audit; the wire response does not surface it.
    StatusForbidden(String),
    /// Underlying DB error (connection lost, query failed, etc.). The
    /// middleware logs and returns 503 / 500 — does not silently pass.
    Db(String),
}

/// Pluggable DB-liveness validator for `host_jwt`.
///
/// The auth middleware calls this on every authenticated host request,
/// after JWT signature/exp/aud/scope checks but before constructing
/// [`AuthContext::Host`](crate::middleware::AuthContext::Host). Returning
/// `Ok(())` means "row exists, org matches, status acceptable".
///
/// Implementations must be cheap (single PK lookup) and cancellation-safe.
#[async_trait]
pub trait HostAuthValidator: Send + Sync {
    async fn validate(&self, host_id: Uuid, claimed_org_id: Uuid) -> Result<(), HostAuthError>;
}

/// Production `worker_hosts` validator backed by a Postgres pool.
///
/// Uses the privileged `system_pool` because the auth middleware runs
/// before any tenant org binding has been set up. Routes the lookup
/// through `authz.lookup_worker_host_for_auth` (migration 080) — a
/// SECURITY DEFINER function — so the system pool role (`g5_app` in
/// production) does not need direct SELECT on `worker_hosts` and does
/// not get caught by the table's RLS policy at pre-org-binding time.
pub struct PgHostAuthValidator {
    pool: PgPool,
}

impl PgHostAuthValidator {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HostAuthValidator for PgHostAuthValidator {
    async fn validate(&self, host_id: Uuid, claimed_org_id: Uuid) -> Result<(), HostAuthError> {
        // sqlx::query_as (not query!) so the validator compiles without
        // requiring offline metadata — `worker_hosts` lands in this PR
        // and PR-9b is the first PR to actually call this from a code
        // path covered by sqlx-data regeneration. The runtime query is
        // dispatched through `authz.lookup_worker_host_for_auth`, a
        // SECURITY DEFINER function (migration 080) that bypasses the
        // table's tenant-isolation RLS policy and the per-table grant —
        // necessary because the system pool runs as `g5_app` and the
        // auth path executes BEFORE org binding.
        let row: Option<(Uuid, String)> =
            sqlx::query_as("SELECT org_id, status FROM authz.lookup_worker_host_for_auth($1)")
                .bind(host_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| HostAuthError::Db(e.to_string()))?;

        match row {
            None => Err(HostAuthError::NotFound),
            Some((row_org, status)) => {
                if row_org != claimed_org_id {
                    return Err(HostAuthError::OrgMismatch);
                }
                match status.as_str() {
                    // Fresh hosts present their first `host_jwt` before the
                    // ready transition has been committed — accepting
                    // `provisioning` is required to avoid a chicken-and-egg
                    // deadlock at bring-up. The host moves itself to `ready`
                    // through the same host-scoped surface, so the window is
                    // narrow.
                    // A draining or unreachable host must reconnect to finish
                    // event replay, stop processing, and report host.drained.
                    // Placement independently admits only `ready` hosts.
                    "provisioning" | "ready" | "unreachable" | "draining" => Ok(()),
                    other => Err(HostAuthError::StatusForbidden(other.to_string())),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test-only in-memory validator
// ---------------------------------------------------------------------------

/// In-memory `HostAuthValidator` for unit tests. Holds a small map of
/// known hosts and returns the configured status; absent hosts return
/// `NotFound`.
#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct StaticHostAuthValidator {
        /// Map host_id → (org_id, status).
        hosts: HashMap<Uuid, (Uuid, String)>,
    }

    impl StaticHostAuthValidator {
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        #[must_use]
        pub fn with_host(mut self, host_id: Uuid, org_id: Uuid, status: &str) -> Self {
            self.hosts.insert(host_id, (org_id, status.to_string()));
            self
        }
    }

    #[async_trait]
    impl HostAuthValidator for StaticHostAuthValidator {
        async fn validate(&self, host_id: Uuid, claimed_org_id: Uuid) -> Result<(), HostAuthError> {
            match self.hosts.get(&host_id) {
                None => Err(HostAuthError::NotFound),
                Some((row_org, status)) => {
                    if *row_org != claimed_org_id {
                        return Err(HostAuthError::OrgMismatch);
                    }
                    match status.as_str() {
                        "provisioning" | "ready" | "unreachable" | "draining" | "idle"
                        | "assigned" | "executing" => Ok(()),
                        other => Err(HostAuthError::StatusForbidden(other.to_string())),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::StaticHostAuthValidator;
    use super::*;

    #[tokio::test]
    async fn validates_ready_host() {
        let host = Uuid::new_v4();
        let org = Uuid::new_v4();
        let v = StaticHostAuthValidator::new().with_host(host, org, "ready");
        assert!(v.validate(host, org).await.is_ok());
    }

    #[tokio::test]
    async fn validates_provisioning_host() {
        let host = Uuid::new_v4();
        let org = Uuid::new_v4();
        let v = StaticHostAuthValidator::new().with_host(host, org, "provisioning");
        assert!(v.validate(host, org).await.is_ok());
    }

    #[tokio::test]
    async fn validates_active_external_worker_states() {
        let org = Uuid::new_v4();
        for status in ["idle", "assigned", "executing"] {
            let host = Uuid::new_v4();
            let v = StaticHostAuthValidator::new().with_host(host, org, status);
            assert!(
                v.validate(host, org).await.is_ok(),
                "{status} must stay accepted by the test validator just as it is in production"
            );
        }
    }

    #[tokio::test]
    async fn validates_reconciliation_only_host_states() {
        let org = Uuid::new_v4();
        for status in ["unreachable", "draining"] {
            let host = Uuid::new_v4();
            let validator = StaticHostAuthValidator::new().with_host(host, org, status);
            assert!(validator.validate(host, org).await.is_ok());
        }
    }

    #[tokio::test]
    async fn rejects_missing_host() {
        let v = StaticHostAuthValidator::new();
        assert_eq!(
            v.validate(Uuid::new_v4(), Uuid::new_v4()).await,
            Err(HostAuthError::NotFound)
        );
    }

    #[tokio::test]
    async fn rejects_org_mismatch() {
        let host = Uuid::new_v4();
        let org = Uuid::new_v4();
        let v = StaticHostAuthValidator::new().with_host(host, org, "ready");
        assert_eq!(
            v.validate(host, Uuid::new_v4()).await,
            Err(HostAuthError::OrgMismatch)
        );
    }

    #[tokio::test]
    async fn rejects_terminated_host() {
        let host = Uuid::new_v4();
        let org = Uuid::new_v4();
        let v = StaticHostAuthValidator::new().with_host(host, org, "terminated");
        assert_eq!(
            v.validate(host, org).await,
            Err(HostAuthError::StatusForbidden("terminated".into()))
        );
    }
}
