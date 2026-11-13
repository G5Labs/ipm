//! API key (PAT) CRUD and validation.
//!
//! Implements TASK-17 from WORKER_STORAGE_SEPARATION.md: API key management
//! for programmatic access. Follows the same hash-and-prefix pattern as
//! `worker_token_service`.

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use g5_auth::Scope;
use g5_contracts::error::G5Error;

use crate::errors::AppError;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// An API key row from the database (no plaintext key).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Result of creating an API key — includes the plaintext key (shown once).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyResult {
    pub api_key: ApiKey,
    /// The full plaintext key. Only returned at creation time.
    pub key: String,
}

/// Validated API key info extracted during auth middleware lookup.
#[derive(Debug, Clone)]
pub struct ValidatedApiKey {
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub scopes: Vec<Scope>,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Request body for `POST /api/v1/orgs/:orgId/api-keys`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyRequest {
    /// Human-readable label for the key.
    pub name: String,
    /// Scopes the key should grant (e.g. `["workspaces:read", "tasks:read"]`).
    /// Each scope string must parse to a valid `Scope` variant.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Optional expiry duration in seconds from now.
    #[serde(default)]
    pub expires_in_secs: Option<i64>,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// API key management service.
pub struct ApiKeyService;

/// Token prefix for all API keys.
const TOKEN_PREFIX: &str = g5_auth::API_KEY_PREFIX;

impl ApiKeyService {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generate a cryptographically random API key.
    fn generate_key() -> String {
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 32] = rng.gen();
        let encoded = hex::encode(random_bytes);
        format!("{TOKEN_PREFIX}{encoded}")
    }

    /// SHA-256 hash of the key for storage.
    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Create a new API key.
    pub async fn create_key(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        user_id: Uuid,
        req: CreateApiKeyRequest,
    ) -> Result<CreateApiKeyResult, AppError> {
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            user_id,
            org_id,
            false,
        )
        .await
        .map_err(AppError::from_error)?;
        // Validate name — reject blank/whitespace-only.
        let name = req.name.trim().to_string();
        if name.is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({ "field": "name", "error": "API key name must not be blank" }),
            }.into());
        }

        // Validate expiry — reject negative or zero durations.
        if let Some(secs) = req.expires_in_secs {
            if secs <= 0 {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({ "field": "expiresInSecs", "error": "Expiry duration must be a positive number of seconds" }),
                }.into());
            }
        }

        // Validate that all requested scopes are known.
        let mut parsed_scopes = Vec::with_capacity(req.scopes.len());
        for s in &req.scopes {
            let scope: Scope = s.parse().map_err(|_| G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "scopes",
                    "error": format!("Unknown scope '{s}'"),
                }),
            })?;
            parsed_scopes.push(scope);
        }
        // Deduplicate scopes.
        let mut seen = std::collections::HashSet::new();
        parsed_scopes.retain(|s| seen.insert(s.as_str().to_string()));

        let key = Self::generate_key();
        let key_hash = Self::hash_key(&key);
        let key_prefix = &key[..TOKEN_PREFIX.len() + 8]; // "g5k_" + 8 hex chars

        let expires_at = req
            .expires_in_secs
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

        let scope_strings: Vec<String> = parsed_scopes
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();

        let id = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO api_keys (id, org_id, user_id, name, key_prefix, key_hash, scopes, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            id,
            org_id,
            user_id,
            req.name,
            key_prefix,
            key_hash,
            &scope_strings,
            expires_at,
        )
        .execute(db.conn())
        .await?;

        let api_key = ApiKey {
            id,
            org_id,
            user_id,
            name: req.name,
            key_prefix: key_prefix.to_string(),
            scopes: scope_strings,
            created_at: Utc::now(),
            expires_at,
            last_used_at: None,
            revoked_at: None,
        };

        Ok(CreateApiKeyResult { api_key, key })
    }

    /// List all API keys for an org (without plaintext keys).
    pub async fn list_keys(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
    ) -> Result<Vec<ApiKey>, AppError> {
        let rows = sqlx::query_as!(
            ApiKey,
            r#"
            SELECT id, org_id, user_id, name, key_prefix, scopes,
                   created_at, expires_at, last_used_at, revoked_at
            FROM api_keys
            WHERE org_id = $1
            ORDER BY created_at DESC
            "#,
            org_id,
        )
        .fetch_all(db.conn())
        .await?;

        Ok(rows)
    }

    /// Revoke an API key (soft delete — sets revoked_at).
    pub async fn revoke_key(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        key_id: Uuid,
    ) -> Result<(), AppError> {
        let result = sqlx::query!(
            "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND org_id = $2 AND revoked_at IS NULL",
            key_id,
            org_id,
        )
        .execute(db.conn())
        .await?;

        if result.rows_affected() == 0 {
            return Err(G5Error::NotFound {
                resource: format!("api key {key_id}"),
            }
            .into());
        }

        Ok(())
    }

    /// Validate an API key during authentication.
    ///
    /// Looks up the key by SHA-256 hash, checks expiry and revocation,
    /// parses scopes, and updates `last_used_at` / `last_used_ip`.
    ///
    /// Uses a raw `PgPool` because this runs in the auth middleware
    /// before RLS context is established.
    pub async fn validate_api_key(
        pool: &sqlx::PgPool,
        plaintext_key: &str,
        remote_ip: Option<&str>,
    ) -> Result<ValidatedApiKey, G5Error> {
        let key_hash = Self::hash_key(plaintext_key);

        #[derive(sqlx::FromRow)]
        struct KeyRow {
            id: Uuid,
            org_id: Uuid,
            user_id: Uuid,
            scopes: Vec<String>,
            expires_at: Option<DateTime<Utc>>,
            revoked_at: Option<DateTime<Utc>>,
        }

        let row: KeyRow = sqlx::query_as!(
            KeyRow,
            r#"
            SELECT
                id AS "id!",
                org_id AS "org_id!",
                user_id AS "user_id!",
                scopes AS "scopes!",
                expires_at,
                revoked_at
            FROM authz.lookup_api_key_by_hash($1)
            "#,
            key_hash,
        )
        .fetch_optional(pool)
        .await
        .map_err(G5Error::from)?
        .ok_or(G5Error::Unauthorized)?;

        // Check revocation.
        if row.revoked_at.is_some() {
            return Err(G5Error::Unauthorized);
        }

        // Check expiry.
        if let Some(expires_at) = row.expires_at {
            if Utc::now() > expires_at {
                return Err(G5Error::Unauthorized);
            }
        }

        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM org_memberships WHERE org_id = $1
             AND user_id = $2 AND deactivated_at IS NULL)",
        )
        .bind(row.org_id)
        .bind(row.user_id)
        .fetch_one(pool)
        .await?;
        if !active {
            return Err(G5Error::Unauthorized);
        }

        // Parse scope strings into Scope enum values.
        let scopes: Vec<Scope> = row
            .scopes
            .iter()
            .filter_map(|s| s.parse::<Scope>().ok())
            .collect();

        // Update last_used_at and last_used_ip (best-effort, don't fail auth).
        let _ = sqlx::query!(
            r#"SELECT authz.touch_api_key_last_used($1, $2) AS touched"#,
            row.id,
            remote_ip,
        )
        .fetch_optional(pool)
        .await;

        Ok(ValidatedApiKey {
            id: row.id,
            org_id: row.org_id,
            user_id: row.user_id,
            scopes,
        })
    }
}

impl Default for ApiKeyService {
    fn default() -> Self {
        Self::new()
    }
}
