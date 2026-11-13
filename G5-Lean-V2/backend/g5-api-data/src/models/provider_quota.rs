//! Provider subscription-quota snapshot model.
//!
//! Maps to the `provider_quota_snapshots` table (migration 231, re-keyed by
//! migration 293 with the provider-account dimension). A snapshot is
//! passive, display-only, provider-owned budget evidence captured from OAuth
//! response headers by the host LLM proxy
//! (system_invariants/external_agent_provider_contract D13). It never gates
//! dispatch, never joins the dollar/token cost-accounting `usage_records` path,
//! and never stores credential material.
//!
//! These structs are passive data containers — all upsert/fetch/validation
//! behavior lives in `services::provider_quota_service`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use g5_contracts::error::G5Error;

/// Maximum byte length accepted for the `provider` / `auth_mode` key components
/// and the free-text `representative_claim` / `unified_status` fields, matching
/// the `usage_records` provider/model bound. Canonically defined in the shared
/// wire-contract crate so the producing LLM proxy truncates to the exact bound
/// this validator enforces.
pub use g5_internal_api::dtos::PROVIDER_QUOTA_TEXT_MAX_LEN;

/// A validated provider-quota snapshot ready to upsert. Constructed only via
/// [`ProviderQuotaSnapshotInput::validate`], which enforces the D13 shape
/// constraints before any database write.
#[derive(Debug, Clone)]
pub struct ProviderQuotaSnapshotInput {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub provider: String,
    pub auth_mode: String,
    /// Provider account id of the task's frozen resolution; `Uuid::nil()` when
    /// the launch used raw Runtime Config keys (D13). Derived server-side by
    /// the internal report route, never trusted from the reporting client.
    pub account_id: Uuid,
    pub five_hour_utilization: Option<f32>,
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    pub seven_day_utilization: Option<f32>,
    pub seven_day_reset_at: Option<DateTime<Utc>>,
    pub representative_claim: Option<String>,
    pub unified_status: Option<String>,
    pub captured_at: DateTime<Utc>,
}

impl ProviderQuotaSnapshotInput {
    /// Validate the D13 snapshot shape as malformed-input rejection with a typed
    /// [`G5Error::ValidationError`]:
    ///
    /// * each present utilization must be finite and within `0.0..=1.0`;
    /// * `provider` and `auth_mode` must be non-empty and within
    ///   [`PROVIDER_QUOTA_TEXT_MAX_LEN`];
    /// * `representative_claim` / `unified_status`, when present, must be within
    ///   [`PROVIDER_QUOTA_TEXT_MAX_LEN`].
    ///
    /// This is a pure function so callers (the route and the service) can reject
    /// a bad report before any row is written.
    pub fn validate(&self) -> Result<(), G5Error> {
        Self::validate_utilization("fiveHourUtilization", self.five_hour_utilization)?;
        Self::validate_utilization("sevenDayUtilization", self.seven_day_utilization)?;
        Self::validate_key_text("provider", &self.provider, true)?;
        Self::validate_key_text("authMode", &self.auth_mode, true)?;
        if let Some(claim) = &self.representative_claim {
            Self::validate_key_text("representativeClaim", claim, false)?;
        }
        if let Some(status) = &self.unified_status {
            Self::validate_key_text("unifiedStatus", status, false)?;
        }
        Ok(())
    }

    fn validate_utilization(field: &str, value: Option<f32>) -> Result<(), G5Error> {
        if let Some(v) = value {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": field,
                        "message": format!(
                            "{field} must be a finite fraction within 0.0..=1.0, got {v}"
                        ),
                    }),
                });
            }
        }
        Ok(())
    }

    fn validate_key_text(field: &str, value: &str, require_non_empty: bool) -> Result<(), G5Error> {
        if require_non_empty && value.is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": field,
                    "message": format!("{field} must not be empty"),
                }),
            });
        }
        if value.len() > PROVIDER_QUOTA_TEXT_MAX_LEN {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": field,
                    "message": format!(
                        "{field} exceeds maximum length of {PROVIDER_QUOTA_TEXT_MAX_LEN}"
                    ),
                }),
            });
        }
        Ok(())
    }
}

/// A stored provider-quota snapshot row as returned by a fetch. Maps to the
/// `provider_quota_snapshots` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuotaSnapshot {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub provider: String,
    pub auth_mode: String,
    /// Provider account id key component; nil for account-less launches.
    pub account_id: Uuid,
    pub five_hour_utilization: Option<f32>,
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    pub seven_day_utilization: Option<f32>,
    pub seven_day_reset_at: Option<DateTime<Utc>>,
    pub representative_claim: Option<String>,
    pub unified_status: Option<String>,
    pub captured_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> ProviderQuotaSnapshotInput {
        ProviderQuotaSnapshotInput {
            org_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            provider: "claude_code".to_string(),
            auth_mode: "anthropic_oauth".to_string(),
            account_id: Uuid::nil(),
            five_hour_utilization: Some(0.5),
            five_hour_reset_at: None,
            seven_day_utilization: Some(0.25),
            seven_day_reset_at: None,
            representative_claim: None,
            unified_status: None,
            captured_at: Utc::now(),
        }
    }

    #[test]
    fn accepts_valid_snapshot() {
        assert!(base_input().validate().is_ok());
    }

    #[test]
    fn accepts_boundary_utilizations() {
        let mut input = base_input();
        input.five_hour_utilization = Some(0.0);
        input.seven_day_utilization = Some(1.0);
        assert!(input.validate().is_ok());
    }

    #[test]
    fn accepts_partial_snapshot_with_only_five_hour() {
        let mut input = base_input();
        input.seven_day_utilization = None;
        assert!(input.validate().is_ok());
    }

    #[test]
    fn rejects_utilization_above_one() {
        let mut input = base_input();
        input.five_hour_utilization = Some(1.5);
        assert!(matches!(
            input.validate(),
            Err(G5Error::ValidationError { .. })
        ));
    }

    #[test]
    fn rejects_negative_utilization() {
        let mut input = base_input();
        input.seven_day_utilization = Some(-0.1);
        assert!(matches!(
            input.validate(),
            Err(G5Error::ValidationError { .. })
        ));
    }

    #[test]
    fn rejects_non_finite_utilization() {
        let mut input = base_input();
        input.five_hour_utilization = Some(f32::NAN);
        assert!(matches!(
            input.validate(),
            Err(G5Error::ValidationError { .. })
        ));
    }

    #[test]
    fn rejects_empty_provider() {
        let mut input = base_input();
        input.provider = String::new();
        assert!(matches!(
            input.validate(),
            Err(G5Error::ValidationError { .. })
        ));
    }

    #[test]
    fn rejects_empty_auth_mode() {
        let mut input = base_input();
        input.auth_mode = String::new();
        assert!(matches!(
            input.validate(),
            Err(G5Error::ValidationError { .. })
        ));
    }

    /// Pins the producer-consumer text bound: exactly
    /// `PROVIDER_QUOTA_TEXT_MAX_LEN` bytes is accepted, one byte over is
    /// rejected, and the bound counts UTF-8 bytes (not chars) — a multi-byte
    /// string within the char count but over the byte count must fail. The
    /// LLM proxy truncates below this bound before reporting; moving it
    /// silently would break that contract.
    #[test]
    fn text_len_bound_is_exact_and_byte_based() {
        let mut input = base_input();
        input.representative_claim = Some("a".repeat(PROVIDER_QUOTA_TEXT_MAX_LEN));
        assert!(input.validate().is_ok());

        input.representative_claim = Some("a".repeat(PROVIDER_QUOTA_TEXT_MAX_LEN + 1));
        assert!(matches!(
            input.validate(),
            Err(G5Error::ValidationError { .. })
        ));

        // 101 'é' chars = 202 bytes: under the bound in chars, over in bytes.
        let multibyte = "\u{e9}".repeat(PROVIDER_QUOTA_TEXT_MAX_LEN / 2 + 1);
        assert!(multibyte.chars().count() <= PROVIDER_QUOTA_TEXT_MAX_LEN);
        assert!(multibyte.len() > PROVIDER_QUOTA_TEXT_MAX_LEN);
        input.representative_claim = Some(multibyte);
        assert!(matches!(
            input.validate(),
            Err(G5Error::ValidationError { .. })
        ));
    }
}
