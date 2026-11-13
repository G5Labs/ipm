// ---------------------------------------------------------------------------
// g5-api/src/services/identity/provider_quota_service.rs — Provider Quota Service
//
// Persistence for passive, display-only provider subscription-quota snapshots
// retained from OAuth response headers; the native runtime has no collector
// (system_invariants/external_agent_provider_contract D13). This service is
// deliberately separate from `spend_service`: a quota snapshot is
// provider-owned budget EVIDENCE for display only. It never gates admission or
// dispatch, never joins the dollar/token cost-accounting `usage_records` path,
// and never stores credential material.
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use g5_authz::ScopedDb;
use sqlx::FromRow;
use uuid::Uuid;

use crate::errors::G5Error;
use crate::models::{ProviderQuotaSnapshot, ProviderQuotaSnapshotInput};

/// Stateless persistence service for provider-quota snapshots.
///
/// Queries use the runtime-checked `sqlx::query` API (not the compile-time
/// `sqlx::query!` macros) to match neighboring worker-scoped internal-route
/// helpers and to keep `SQLX_OFFLINE` builds independent of freshly-added
/// tables that have no cached offline metadata yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProviderQuotaService;

impl ProviderQuotaService {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Latest-wins upsert of a provider-quota snapshot keyed by
    /// `(org_id, workspace_id, provider, auth_mode, account_id)`.
    ///
    /// The input is validated first (utilization range + key/text shape) so a
    /// malformed report is rejected with a typed [`G5Error::ValidationError`]
    /// before any row is written. On conflict, the stored row is replaced only
    /// when the incoming `captured_at` is strictly newer than the stored one;
    /// an equal-or-older report is ignored.
    ///
    /// Returns `true` when the snapshot was stored (first insert or a
    /// strictly-newer replacement) and `false` when the report was ignored as
    /// not newer than the stored snapshot.
    ///
    /// # Errors
    ///
    /// - [`G5Error::ValidationError`] — malformed snapshot input.
    /// - [`G5Error::Internal`] (via `sqlx::Error`) — database error.
    pub async fn upsert_snapshot(
        &self,
        db: &mut ScopedDb,
        input: ProviderQuotaSnapshotInput,
    ) -> Result<bool, G5Error> {
        input.validate()?;

        let row = sqlx::query(
            r#"
            INSERT INTO provider_quota_snapshots (
                org_id, workspace_id, provider, auth_mode, account_id,
                five_hour_utilization, five_hour_reset_at,
                seven_day_utilization, seven_day_reset_at,
                representative_claim, unified_status, captured_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (org_id, workspace_id, provider, auth_mode, account_id)
            DO UPDATE SET
                five_hour_utilization = EXCLUDED.five_hour_utilization,
                five_hour_reset_at    = EXCLUDED.five_hour_reset_at,
                seven_day_utilization = EXCLUDED.seven_day_utilization,
                seven_day_reset_at    = EXCLUDED.seven_day_reset_at,
                representative_claim  = EXCLUDED.representative_claim,
                unified_status        = EXCLUDED.unified_status,
                captured_at           = EXCLUDED.captured_at,
                updated_at            = now()
            WHERE provider_quota_snapshots.captured_at < EXCLUDED.captured_at
            RETURNING 1 AS stored
            "#,
        )
        .bind(input.org_id)
        .bind(input.workspace_id)
        .bind(&input.provider)
        .bind(&input.auth_mode)
        .bind(input.account_id)
        .bind(input.five_hour_utilization)
        .bind(input.five_hour_reset_at)
        .bind(input.seven_day_utilization)
        .bind(input.seven_day_reset_at)
        .bind(&input.representative_claim)
        .bind(&input.unified_status)
        .bind(input.captured_at)
        .fetch_optional(db.conn())
        .await
        .map_err(G5Error::from)?;

        Ok(row.is_some())
    }

    /// Fetch the current snapshot for the full five-column key
    /// `(org_id, workspace_id, provider, auth_mode, account_id)`
    /// (models/provider_account D4), or `None` when no such snapshot has been
    /// captured yet. `Uuid::nil()` selects the account-less row — a launch
    /// that used raw Runtime Config keys — while a real id selects the row
    /// keyed by that frozen provider account (D13). A missing account row
    /// never falls back to the nil row.
    ///
    /// # Errors
    ///
    /// - [`G5Error::Internal`] (via `sqlx::Error`) — database error.
    pub async fn fetch_snapshot(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        provider: &str,
        auth_mode: &str,
        account_id: Uuid,
    ) -> Result<Option<ProviderQuotaSnapshot>, G5Error> {
        let row = sqlx::query(
            r#"
            SELECT
                org_id, workspace_id, provider, auth_mode, account_id,
                five_hour_utilization, five_hour_reset_at,
                seven_day_utilization, seven_day_reset_at,
                representative_claim, unified_status,
                captured_at, created_at, updated_at
            FROM provider_quota_snapshots
            WHERE org_id = $1
              AND workspace_id = $2
              AND provider = $3
              AND auth_mode = $4
              AND account_id = $5
            "#,
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(provider)
        .bind(auth_mode)
        .bind(account_id)
        .fetch_optional(db.conn())
        .await
        .map_err(G5Error::from)?;

        row.map(|row| ProviderQuotaSnapshot::from_row(&row))
            .transpose()
            .map_err(G5Error::from)
    }

    /// Latest snapshot per provider account across every workspace of the
    /// org (models/provider_account D4): for each requested account id, the
    /// row with the greatest `captured_at` (ties broken by workspace id so the
    /// result is deterministic). Provider and auth mode are not filtered —
    /// an account id already fixes both. Accounts with no snapshot are absent
    /// from the map; an empty request returns an empty map without a query.
    ///
    /// This backs the Settings account list's per-account usage display
    /// (external_agent_provider_contract D13). It is display-only evidence.
    ///
    /// # Errors
    ///
    /// - [`G5Error::Internal`] (via `sqlx::Error`) — database error.
    pub async fn latest_for_accounts(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        account_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, ProviderQuotaSnapshot>, G5Error> {
        if account_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT DISTINCT ON (account_id)
                org_id, workspace_id, provider, auth_mode, account_id,
                five_hour_utilization, five_hour_reset_at,
                seven_day_utilization, seven_day_reset_at,
                representative_claim, unified_status,
                captured_at, created_at, updated_at
            FROM provider_quota_snapshots
            WHERE org_id = $1
              AND account_id = ANY($2)
            ORDER BY account_id, captured_at DESC, workspace_id
            "#,
        )
        .bind(org_id)
        .bind(account_ids)
        .fetch_all(db.conn())
        .await
        .map_err(G5Error::from)?;

        rows.iter()
            .map(|row| {
                ProviderQuotaSnapshot::from_row(row)
                    .map(|snapshot| (snapshot.account_id, snapshot))
                    .map_err(G5Error::from)
            })
            .collect()
    }
}
