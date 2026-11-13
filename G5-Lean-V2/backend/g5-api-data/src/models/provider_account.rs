//! Provider account model.
//!
//! Maps to the `provider_accounts` table (migration 292). A provider account is
//! a named row holding one credential for one provider family at one of three
//! scopes — org, workspace, or user — with no model, effort, or permission-mode
//! preference on the row (system_invariants/external_agent_provider_contract
//! D14; software_architecture/g5-api/models/provider_account D1).
//!
//! This struct is a passive data container — account CRUD, scope authorization,
//! credential encryption, the bounded display hint, and assignment validation
//! all live in the provider-account service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Closed `scope` vocabulary, mirroring the `provider_accounts.scope` CHECK.
pub const PROVIDER_ACCOUNT_SCOPES: &[&str] = &["org", "workspace", "user"];

/// Closed `kind` vocabulary, mirroring the `provider_accounts.kind` CHECK. Each
/// kind maps to exactly one already-approved route and auth mode of the
/// harness auth catalog (D14); a new kind must join the catalog first.
pub const PROVIDER_ACCOUNT_KINDS: &[&str] = &[
    "claude_subscription",
    "anthropic_api_key",
    "anthropic_compatible_bearer",
    "openrouter_api_key",
    "openai_api_key",
    "chatgpt_subscription",
];

/// A stored provider account row. Maps to the `provider_accounts` table.
///
/// `scope` and `kind` are carried as `String` at this layer; the typed enums
/// live in the shared contracts crate and are parsed at the service boundary.
/// `secret_enc` is the AES-256-GCM ciphertext written by the service (`None`
/// for `chatgpt_subscription`, which stores no credential); it is never
/// serialized toward a client — clients see at most the bounded display hint
/// (security_policy/secret_custody_and_redaction D9).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccount {
    pub id: Uuid,
    pub org_id: Uuid,
    pub scope: String,
    pub workspace_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    #[serde(skip)]
    pub secret_enc: Option<Vec<u8>>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_ciphertext_is_never_serialized() {
        let account = ProviderAccount {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            scope: "org".to_string(),
            workspace_id: None,
            user_id: None,
            name: "Team key".to_string(),
            kind: "anthropic_api_key".to_string(),
            base_url: None,
            secret_enc: Some(b"ciphertext-bytes".to_vec()),
            created_by: Uuid::nil(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_value(&account).expect("serialize account");
        assert!(json.get("secretEnc").is_none());
        assert!(json.get("secret_enc").is_none());
        assert_eq!(json["kind"], "anthropic_api_key");
        assert_eq!(json["scope"], "org");
    }

    #[test]
    fn vocabularies_match_the_sql_check_lists() {
        assert_eq!(PROVIDER_ACCOUNT_SCOPES, &["org", "workspace", "user"]);
        assert_eq!(PROVIDER_ACCOUNT_KINDS.len(), 6);
        assert!(PROVIDER_ACCOUNT_KINDS.contains(&"chatgpt_subscription"));
    }
}
