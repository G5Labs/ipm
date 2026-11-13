use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::db::scoped::require_current_workspace_owner;
use crate::errors::AppError;
use crate::services::provider_account_service::{
    account_not_visible_error, account_validation_error, ProviderAccountService,
    ResolvedAccountCredential, CLAUDE_ACCOUNT_ID_SETTING, CODEX_ACCOUNT_ID_SETTING,
};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{
    preview_agent_launch as preview_agent_launch_projection, project_provider_account,
    resolve_agent_launch as resolve_agent_launch_for_dispatch, resolve_launch_provider,
    AgentAccountSelectionV1, AgentLaunchAccountInput, AgentLaunchFieldPreferenceV1,
    AgentLaunchPreferencesV1, AgentLaunchResolutionInput, AgentLaunchSelectionV1,
    ChatAgentProvider, PersistedAgentLaunchV1, ProviderAccountScope, ResolvedAgentExecutionV1,
    CODEX_AUTH_JSON_ENV_KEY, CODEX_SUBSCRIPTION_ENABLED_SETTING,
};

pub fn legacy_agent_launch_preferences(
    provider: Option<ChatAgentProvider>,
    model: Option<&str>,
    permission_mode: Option<&str>,
    effort: Option<&str>,
) -> AgentLaunchPreferencesV1 {
    let string_preference = |value: Option<&str>| match value.map(str::trim) {
        Some(value) if !value.is_empty() => {
            AgentLaunchFieldPreferenceV1::Explicit(value.to_string())
        }
        _ => AgentLaunchFieldPreferenceV1::InheritConfigured,
    };
    AgentLaunchPreferencesV1 {
        schema_version: g5_contracts::types::AGENT_LAUNCH_SCHEMA_VERSION,
        provider: provider
            .map(AgentLaunchFieldPreferenceV1::Explicit)
            .unwrap_or_default(),
        model: string_preference(model),
        permission_mode: string_preference(permission_mode),
        effort: string_preference(effort),
        account: AgentLaunchFieldPreferenceV1::InheritConfigured,
    }
}

/// Exact length of the bounded secret display hint (config_service D9;
/// security_policy secret_custody_and_redaction D9): the smallest bound that
/// keeps provider scheme discriminators legible (`sk-ant-api03` vs
/// `sk-ant-oat01` vs `sk-or-v1-…`), matching the platform's `g5k_` API-key
/// `key_prefix` precedent. Never widen without amending D9.
pub const SECRET_HINT_CHARS: usize = 12;

/// Minimum decrypted-value length (in characters) for hint eligibility
/// (config_service D9): caps worst-case exposure at half of a minimum-length
/// value and suppresses hints for empty/sentinel and short human-chosen
/// secrets.
pub const SECRET_HINT_MIN_VALUE_CHARS: usize = 24;

#[derive(Clone)]
pub struct ConfigService {
    scm_token_manager: Option<Arc<crate::auth::ScmTokenManager>>,
}

#[derive(Debug, Clone)]
pub struct ScopeConfigView {
    pub settings: Value,
    pub secret_keys: Vec<String>,
    /// Bounded beginning-of-key display hints for this scope's own secrets
    /// (config_service D9; sanctioned by security_policy
    /// secret_custody_and_redaction D9): key → first 12 characters of the
    /// decrypted value, present only for hint-eligible values. Derived at read
    /// time, never persisted or logged.
    pub secret_hints: HashMap<String, String>,
    pub updated_by: Option<Uuid>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub settings: Value,
    pub secret_keys: Vec<String>,
    pub secrets: HashMap<String, String>,
    /// Bounded beginning-of-key display hints for the merged effective secrets
    /// (config_service D9): key → first 12 characters of the winning scope's
    /// decrypted value, present only for hint-eligible values. Each hint
    /// reflects the same winning scope as the effective value itself.
    pub secret_hints: HashMap<String, String>,
    /// The provider account projected into this resolution (config_service
    /// D12): `Some` only after `resolve_effective_for_agent_launch` or
    /// `resolve_effective_for_authorized_frozen_launch` applied an account, in which case
    /// `settings`/`secrets` already carry the projected keys and none of the
    /// family's raw keys. Plain `resolve_effective` never sets it.
    pub account: Option<AgentLaunchAccountInput>,
}

/// Writing scope of a scoped config update or an in-memory preview patch,
/// used to validate assignment keys (config_service D13) and to address the
/// scope whose own assignments an account's creation or removal touches
/// (chat g5.chat.account-defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentWriteScope {
    Org,
    Workspace(Uuid),
    User(Uuid),
}

/// Non-secret per-agent account assignment settings (config_service D13;
/// external_agent_provider_contract D14).
const ASSIGNMENT_SETTING_KEYS: &[&str] = &[CLAUDE_ACCOUNT_ID_SETTING, CODEX_ACCOUNT_ID_SETTING];

pub use crate::services::git::external_issues::EXTERNAL_ISSUES_ENABLED_SETTING;

/// Organization switch for blocking merge on failing/pending external PR checks
/// (governance g5.governance.merge.external-policy). Like the external-issues
/// switch, a boolean set only on the organization's own settings; off unless
/// explicitly set to `true`.
pub const EXTERNAL_CHECKS_BLOCK_MERGE_SETTING: &str = "external_checks_block_merge";

/// Organization switch offering the workbench's Re-run Verification action
/// and Base re-verification (ui g5.workbench.review-prompts.switch). The client
/// greys the action unless the organization's own setting is exactly `true`;
/// Home's status and a run's start read it on the server too.
pub const VERIFICATION_ACTION_ENABLED_SETTING: &str = "verification_action_enabled";

/// Organization-only boolean switches: refused at a narrower scope, required to
/// be boolean. Read from the organization scope, never a merged override.
pub const ORG_ONLY_BOOLEAN_SETTINGS: &[&str] = &[
    EXTERNAL_ISSUES_ENABLED_SETTING,
    EXTERNAL_CHECKS_BLOCK_MERGE_SETTING,
    VERIFICATION_ACTION_ENABLED_SETTING,
];

/// Read an organization-only boolean switch from the org's own settings (never
/// a workspace or user override). Off unless explicitly set to `true`.
pub async fn org_boolean_setting(
    db: &mut ScopedDb,
    org_id: Uuid,
    key: &str,
) -> Result<bool, G5Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE((SELECT settings_json -> $2 = 'true'::jsonb FROM org_configs \
                          WHERE org_id = $1), false)",
    )
    .bind(org_id)
    .bind(key)
    .fetch_one(db.conn())
    .await
    .map_err(|error| {
        if g5_contracts::error::sqlx_error_is_retryable(&error) {
            G5Error::DatabaseRetryable {
                message: format!("read org setting {key}: {error}"),
            }
        } else {
            G5Error::Internal {
                message: format!("read org setting {key}: {error}"),
            }
        }
    })
}

/// Whether the organization requires external PR checks to pass before merge
/// (governance g5.governance.merge.external-policy). Off unless set to `true`.
pub async fn org_external_checks_block_merge(
    db: &mut ScopedDb,
    org_id: Uuid,
) -> Result<bool, G5Error> {
    org_boolean_setting(db, org_id, EXTERNAL_CHECKS_BLOCK_MERGE_SETTING).await
}

/// Refuse an organization-only setting at a narrower scope, and a
/// non-boolean switch value.
fn validate_org_only_settings(
    scope: AssignmentWriteScope,
    settings: Option<&Value>,
) -> Result<(), AppError> {
    for key in ORG_ONLY_BOOLEAN_SETTINGS {
        let Some(value) = settings.and_then(|settings| settings.get(key)) else {
            continue;
        };
        if !matches!(scope, AssignmentWriteScope::Org) {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": format!("settings.{key}"),
                    "code": "org_only_setting",
                    "message": format!("{key} can only be set on the organization"),
                }),
            }));
        }
        if !value.is_boolean() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": format!("settings.{key}"),
                    "code": "invalid_setting_type",
                    "message": format!("{key} must be true or false"),
                }),
            }));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct ScopeUpdateInput {
    pub settings: Option<Value>,
    pub remove_setting_keys: Vec<String>,
    pub set_secrets: HashMap<String, String>,
    pub remove_secret_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentLaunchSettingsPreviewScope {
    Org,
    Workspace,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLaunchSettingsPreview {
    pub scope: AgentLaunchSettingsPreviewScope,
    pub settings: Option<Value>,
    pub remove_setting_keys: Vec<String>,
}

const AGENT_LAUNCH_PREVIEW_SETTING_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "OPENAI_BASE_URL",
    "OPENAI_MODEL",
    CLAUDE_ACCOUNT_ID_SETTING,
    "claude_effort",
    "claude_permission_mode",
    CODEX_ACCOUNT_ID_SETTING,
    "codex_effort",
    "codex_permission_mode",
    CODEX_SUBSCRIPTION_ENABLED_SETTING,
    "default_agent_provider",
];

#[derive(Debug, FromRow)]
struct ScopeConfigRow {
    settings_json: Value,
    secrets_enc: Option<Vec<u8>>,
    #[allow(dead_code)] // Persisted index is intentionally distrusted on reads.
    secret_keys: Vec<String>,
    updated_by: Option<Uuid>,
    updated_at: DateTime<Utc>,
}

impl ConfigService {
    #[must_use]
    pub fn new(scm_token_manager: Option<Arc<crate::auth::ScmTokenManager>>) -> Self {
        Self { scm_token_manager }
    }

    /// Encrypt a task-bound opaque credential using the same managed key as
    /// Runtime Config secrets. The caller owns scope, lifetime, and redaction;
    /// this helper intentionally performs no persistence or logging.
    pub fn encrypt_task_credential(&self, plaintext: &str) -> Result<Vec<u8>, AppError> {
        let manager = self.scm_token_manager.as_ref().ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message:
                    "SCM encryption key is not configured; task credential storage is unavailable"
                        .into(),
            })
        })?;
        manager.encrypt_token(plaintext)
    }

    /// Decrypt a task-bound opaque credential. The plaintext must remain on
    /// the worker-assignment secrets channel and must never be logged.
    pub fn decrypt_task_credential(&self, ciphertext: &[u8]) -> Result<String, AppError> {
        let manager = self.scm_token_manager.as_ref().ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message:
                    "Encrypted task credentials exist but G5_SCM_ENCRYPTION_KEY is not configured"
                        .into(),
            })
        })?;
        manager.decrypt_token(ciphertext)
    }

    /// Remove the retired Runtime Config `CODEX_AUTH_JSON` key from every
    /// persisted scope without disturbing sibling settings or secrets. This
    /// privileged, idempotent startup cleanup is deliberately implemented in
    /// application code because the secret maps are encrypted and cannot be
    /// edited safely by a SQL migration.
    pub async fn purge_retired_codex_auth_json(&self, pool: &PgPool) -> Result<u64, AppError> {
        #[derive(sqlx::FromRow)]
        struct CandidateRow {
            scope_kind: String,
            org_id: Option<Uuid>,
            workspace_id: Option<Uuid>,
            user_id: Option<Uuid>,
            settings_json: Value,
            secrets_enc: Option<Vec<u8>>,
            secret_keys: Vec<String>,
        }

        let mut tx = pool.begin().await.map_err(config_db_err)?;
        let rows = sqlx::query_as::<_, CandidateRow>(
            "SELECT scope_kind, org_id, workspace_id, user_id, settings_json, \
                    secrets_enc, secret_keys \
             FROM authz.list_retired_codex_auth_config_rows()",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(config_db_err)?;
        let mut updated = 0_u64;
        for row in rows {
            let next_settings = without_retired_codex_auth_setting(row.settings_json.clone());
            let current_secrets = self.decrypt_secret_map_raw(row.secrets_enc.as_deref())?;
            let secrets = self.without_retired_secrets(current_secrets.clone());
            if next_settings == row.settings_json && secrets == current_secrets {
                continue;
            }
            let state = self.secret_state_from_map(secrets)?;
            let applied = sqlx::query_scalar::<_, bool>(
                "SELECT authz.apply_retired_codex_auth_config_row( \
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10 \
                )",
            )
            .bind(&row.scope_kind)
            .bind(row.org_id)
            .bind(row.workspace_id)
            .bind(row.user_id)
            .bind(&row.settings_json)
            .bind(&row.secrets_enc)
            .bind(&row.secret_keys)
            .bind(next_settings)
            .bind(state.encrypted)
            .bind(state.secret_keys)
            .fetch_one(&mut *tx)
            .await
            .map_err(config_db_err)?;
            if !applied {
                return Err(AppError::from_error(G5Error::ServiceUnavailable {
                    message: format!(
                        "Runtime Config scope {} changed while retired CODEX_AUTH_JSON was being removed; retry startup",
                        row.scope_kind
                    ),
                }));
            }
            updated += 1;
        }

        tx.commit().await.map_err(config_db_err)?;
        Ok(updated)
    }

    pub async fn get_org_scope(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
    ) -> Result<ScopeConfigView, AppError> {
        let row = sqlx::query_as!(
            ScopeConfigRow,
            "SELECT settings_json, secrets_enc, secret_keys, updated_by, updated_at \
             FROM org_configs WHERE org_id = $1",
            org_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(config_db_err)?;

        self.scope_view_from_row(row)
    }

    pub async fn get_workspace_scope(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<ScopeConfigView, AppError> {
        let row = sqlx::query_as!(
            ScopeConfigRow,
            "SELECT settings_json, secrets_enc, secret_keys, updated_by, updated_at \
             FROM workspace_configs WHERE workspace_id = $1",
            workspace_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(config_db_err)?;

        self.scope_view_from_row(row)
    }

    pub async fn get_user_scope(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<ScopeConfigView, AppError> {
        let row = sqlx::query_as!(
            ScopeConfigRow,
            "SELECT settings_json, secrets_enc, secret_keys, updated_by, updated_at \
             FROM user_configs WHERE org_id = $1 AND user_id = $2",
            org_id,
            user_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(config_db_err)?;

        self.scope_view_from_row(row)
    }

    pub async fn update_org_scope(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        input: ScopeUpdateInput,
    ) -> Result<ScopeConfigView, AppError> {
        lock_scope_for_update(db, &org_scope_lock_key(org_id)).await?;
        let current = self.get_org_scope(db, org_id).await?;
        let ScopeUpdateInput {
            settings,
            remove_setting_keys,
            set_secrets,
            remove_secret_keys,
        } = input;
        validate_org_only_settings(AssignmentWriteScope::Org, settings.as_ref())?;
        validate_assignment_keys(db, org_id, AssignmentWriteScope::Org, settings.as_ref()).await?;
        let next_settings = merge_settings(current.settings, settings, remove_setting_keys)?;
        let next_secret_state = self.merge_secret_maps(
            self.fetch_org_secret_map(db, org_id).await?,
            set_secrets,
            remove_secret_keys,
        )?;
        let encrypted = next_secret_state.encrypted;
        let secret_keys = next_secret_state.secret_keys;

        sqlx::query!(
            "INSERT INTO org_configs (org_id, settings_json, secrets_enc, secret_keys, updated_by, updated_at) \
             VALUES ($1, $2, $3, $4, $5, now()) \
             ON CONFLICT (org_id) DO UPDATE SET \
               settings_json = EXCLUDED.settings_json, \
               secrets_enc = EXCLUDED.secrets_enc, \
               secret_keys = EXCLUDED.secret_keys, \
               updated_by = EXCLUDED.updated_by, \
               updated_at = now()",
            org_id,
            next_settings,
            encrypted,
            &secret_keys,
            actor_id,
        )
        .execute(db.conn())
        .await
        .map_err(config_db_err)?;

        self.get_org_scope(db, org_id).await
    }

    pub async fn update_workspace_scope(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        actor_id: Uuid,
        input: ScopeUpdateInput,
    ) -> Result<ScopeConfigView, AppError> {
        require_current_workspace_owner(db, org_id, workspace_id, actor_id).await?;
        lock_scope_for_update(db, &workspace_scope_lock_key(workspace_id)).await?;
        let current = self.get_workspace_scope(db, workspace_id).await?;
        let ScopeUpdateInput {
            settings,
            remove_setting_keys,
            set_secrets,
            remove_secret_keys,
        } = input;
        validate_org_only_settings(
            AssignmentWriteScope::Workspace(workspace_id),
            settings.as_ref(),
        )?;
        validate_assignment_keys(
            db,
            org_id,
            AssignmentWriteScope::Workspace(workspace_id),
            settings.as_ref(),
        )
        .await?;
        let next_settings = merge_settings(current.settings, settings, remove_setting_keys)?;
        let next_secret_state = self.merge_secret_maps(
            self.fetch_workspace_secret_map(db, workspace_id).await?,
            set_secrets,
            remove_secret_keys,
        )?;
        let encrypted = next_secret_state.encrypted;
        let secret_keys = next_secret_state.secret_keys;

        sqlx::query!(
            "INSERT INTO workspace_configs \
             (workspace_id, org_id, settings_json, secrets_enc, secret_keys, updated_by, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now()) \
             ON CONFLICT (workspace_id) DO UPDATE SET \
               settings_json = EXCLUDED.settings_json, \
               secrets_enc = EXCLUDED.secrets_enc, \
               secret_keys = EXCLUDED.secret_keys, \
               updated_by = EXCLUDED.updated_by, \
               updated_at = now()",
            workspace_id,
            org_id,
            next_settings,
            encrypted,
            &secret_keys,
            actor_id,
        )
        .execute(db.conn())
        .await
        .map_err(config_db_err)?;

        self.get_workspace_scope(db, workspace_id).await
    }

    pub async fn update_user_scope(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        user_id: Uuid,
        actor_id: Uuid,
        input: ScopeUpdateInput,
    ) -> Result<ScopeConfigView, AppError> {
        lock_scope_for_update(db, &user_scope_lock_key(org_id, user_id)).await?;
        let current = self.get_user_scope(db, org_id, user_id).await?;
        let ScopeUpdateInput {
            settings,
            remove_setting_keys,
            set_secrets,
            remove_secret_keys,
        } = input;
        validate_org_only_settings(AssignmentWriteScope::User(user_id), settings.as_ref())?;
        validate_assignment_keys(
            db,
            org_id,
            AssignmentWriteScope::User(user_id),
            settings.as_ref(),
        )
        .await?;
        let next_settings = merge_settings(current.settings, settings, remove_setting_keys)?;
        let next_secret_state = self.merge_secret_maps(
            self.fetch_user_secret_map(db, org_id, user_id).await?,
            set_secrets,
            remove_secret_keys,
        )?;
        let encrypted = next_secret_state.encrypted;
        let secret_keys = next_secret_state.secret_keys;

        sqlx::query!(
            "INSERT INTO user_configs \
             (org_id, user_id, settings_json, secrets_enc, secret_keys, updated_by, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now()) \
             ON CONFLICT (org_id, user_id) DO UPDATE SET \
               settings_json = EXCLUDED.settings_json, \
               secrets_enc = EXCLUDED.secrets_enc, \
               secret_keys = EXCLUDED.secret_keys, \
               updated_by = EXCLUDED.updated_by, \
               updated_at = now()",
            org_id,
            user_id,
            next_settings,
            encrypted,
            &secret_keys,
            actor_id,
        )
        .execute(db.conn())
        .await
        .map_err(config_db_err)?;

        self.get_user_scope(db, org_id, user_id).await
    }

    /// Read one scope's OWN settings object (not the effective merge).
    async fn own_settings_at(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        scope: AssignmentWriteScope,
    ) -> Result<Value, AppError> {
        let view = match scope {
            AssignmentWriteScope::Org => self.get_org_scope(db, org_id).await?,
            AssignmentWriteScope::Workspace(workspace_id) => {
                self.get_workspace_scope(db, workspace_id).await?
            }
            AssignmentWriteScope::User(user_id) => self.get_user_scope(db, org_id, user_id).await?,
        };
        Ok(view.settings)
    }

    /// Apply one settings delta at a scope through the ordinary scoped
    /// update, so assignment validation, the scope lock and the audit trail
    /// are the same as a Settings write.
    async fn update_at(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        scope: AssignmentWriteScope,
        input: ScopeUpdateInput,
    ) -> Result<(), AppError> {
        match scope {
            AssignmentWriteScope::Org => {
                self.update_org_scope(db, org_id, actor_id, input).await?;
            }
            AssignmentWriteScope::Workspace(workspace_id) => {
                self.update_workspace_scope(db, org_id, workspace_id, actor_id, input)
                    .await?;
            }
            AssignmentWriteScope::User(user_id) => {
                self.update_user_scope(db, org_id, user_id, actor_id, input)
                    .await?;
            }
        }
        Ok(())
    }

    /// Make `account_id` the scope's own assignment under each of `keys`
    /// that the scope does not set yet (chat g5.chat.account-defaults: a new
    /// account becomes the default for the agents it serves at its own
    /// scope). Keys the scope already sets are left alone, other scopes are
    /// untouched. Runs inside the caller's transaction under the scope lock
    /// and the assignment validator, so the write is atomic with the account
    /// row and validated like a Settings write. Returns the keys written.
    pub async fn assign_account_where_unset(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        scope: AssignmentWriteScope,
        keys: &[&'static str],
        account_id: Uuid,
    ) -> Result<Vec<&'static str>, AppError> {
        lock_scope_for_update(db, &scope_lock_key(org_id, scope)).await?;
        let current = self.own_settings_at(db, org_id, scope).await?;
        let unset: Vec<&'static str> = keys
            .iter()
            .copied()
            .filter(|key| matches!(current.get(*key), None | Some(Value::Null)))
            .collect();
        if unset.is_empty() {
            return Ok(unset);
        }
        let mut settings = Map::new();
        for key in &unset {
            settings.insert((*key).to_string(), Value::String(account_id.to_string()));
        }
        self.update_at(
            db,
            org_id,
            actor_id,
            scope,
            ScopeUpdateInput {
                settings: Some(Value::Object(settings)),
                remove_setting_keys: Vec::new(),
                set_secrets: HashMap::new(),
                remove_secret_keys: Vec::new(),
            },
        )
        .await?;
        Ok(unset)
    }

    /// Remove every assignment at the scope that names `account_id` (chat
    /// g5.chat.account-defaults: removing an account clears its own scope's
    /// assignments with it). Returns the keys removed.
    pub async fn clear_account_assignments_at(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        scope: AssignmentWriteScope,
        account_id: Uuid,
    ) -> Result<Vec<&'static str>, AppError> {
        lock_scope_for_update(db, &scope_lock_key(org_id, scope)).await?;
        let current = self.own_settings_at(db, org_id, scope).await?;
        let needle = account_id.to_string();
        let naming: Vec<&'static str> = ASSIGNMENT_SETTING_KEYS
            .iter()
            .copied()
            .filter(|key| current.get(*key).and_then(Value::as_str) == Some(needle.as_str()))
            .collect();
        if naming.is_empty() {
            return Ok(naming);
        }
        self.update_at(
            db,
            org_id,
            actor_id,
            scope,
            ScopeUpdateInput {
                settings: None,
                remove_setting_keys: naming.iter().map(|key| (*key).to_string()).collect(),
                set_secrets: HashMap::new(),
                remove_secret_keys: Vec::new(),
            },
        )
        .await?;
        Ok(naming)
    }

    pub async fn resolve_effective(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<EffectiveConfig, AppError> {
        self.resolve_effective_with_settings_preview(db, org_id, workspace_id, user_id, &[])
            .await
    }

    /// Resolve effective Runtime Config after applying allowlisted, non-secret
    /// Settings patches in memory. The same scope merge/removal and precedence
    /// functions used by persisted config are reused; no patch is persisted.
    pub async fn resolve_effective_with_settings_preview(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        user_id: Option<Uuid>,
        previews: &[AgentLaunchSettingsPreview],
    ) -> Result<EffectiveConfig, AppError> {
        let org_row = sqlx::query_as!(
            ScopeConfigRow,
            "SELECT settings_json, secrets_enc, secret_keys, updated_by, updated_at \
             FROM org_configs WHERE org_id = $1",
            org_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(config_db_err)?;

        let workspace_row = match workspace_id {
            Some(workspace_id) => sqlx::query_as!(
                ScopeConfigRow,
                "SELECT settings_json, secrets_enc, secret_keys, updated_by, updated_at \
                     FROM workspace_configs WHERE workspace_id = $1",
                workspace_id,
            )
            .fetch_optional(db.conn())
            .await
            .map_err(config_db_err)?,
            None => None,
        };

        let user_row = match user_id {
            Some(user_id) => sqlx::query_as!(
                ScopeConfigRow,
                "SELECT settings_json, secrets_enc, secret_keys, updated_by, updated_at \
                     FROM user_configs WHERE org_id = $1 AND user_id = $2",
                org_id,
                user_id,
            )
            .fetch_optional(db.conn())
            .await
            .map_err(config_db_err)?,
            None => None,
        };

        // Staged assignment keys are validated exactly like a persisted write
        // for the patch's scope (config_service D13) before the in-memory
        // merge; the sync validator below still rejects every non-allowlisted
        // key and unavailable scope.
        for preview in previews {
            let scope = match (preview.scope, workspace_id, user_id) {
                (AgentLaunchSettingsPreviewScope::Org, _, _) => AssignmentWriteScope::Org,
                (AgentLaunchSettingsPreviewScope::Workspace, Some(workspace_id), _) => {
                    AssignmentWriteScope::Workspace(workspace_id)
                }
                (AgentLaunchSettingsPreviewScope::User, _, Some(user_id)) => {
                    AssignmentWriteScope::User(user_id)
                }
                _ => continue,
            };
            validate_assignment_keys(db, org_id, scope, preview.settings.as_ref()).await?;
        }

        let mut org_settings = org_row.as_ref().map_or_else(
            || Value::Object(Map::new()),
            |row| without_retired_codex_auth_setting(row.settings_json.clone()),
        );
        let mut workspace_settings = workspace_row.as_ref().map_or_else(
            || Value::Object(Map::new()),
            |row| without_retired_codex_auth_setting(row.settings_json.clone()),
        );
        let mut user_settings = user_row.as_ref().map_or_else(
            || Value::Object(Map::new()),
            |row| without_retired_codex_auth_setting(row.settings_json.clone()),
        );
        let settings = effective_settings_with_previews(
            &mut org_settings,
            &mut workspace_settings,
            &mut user_settings,
            workspace_id.is_some(),
            user_id.is_some(),
            previews,
        )?;
        let mut secrets = HashMap::new();

        for row in [org_row.as_ref(), workspace_row.as_ref(), user_row.as_ref()]
            .into_iter()
            .flatten()
        {
            secrets.extend(self.decrypt_secret_map(row.secrets_enc.as_deref())?);
        }

        let mut secret_keys: Vec<String> = secrets.keys().cloned().collect();
        secret_keys.sort();

        let secret_hints = Self::secret_hints_from_map(&secrets);

        Ok(EffectiveConfig {
            settings,
            secret_keys,
            secrets,
            secret_hints,
            account: None,
        })
    }

    /// Resolve and confirm an agent launch without interpreting the task's label.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_launch(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        user_id: Option<Uuid>,
        selection: Option<&AgentLaunchSelectionV1>,
        preferences: AgentLaunchPreferencesV1,
        accounts: &ProviderAccountService,
    ) -> Result<PersistedAgentLaunchV1, AppError> {
        let preferences = selection
            .map(|s| s.preferences.clone())
            .unwrap_or(preferences);
        let effective = self
            .resolve_effective_for_agent_launch(
                db,
                org_id,
                Some(workspace_id),
                user_id,
                &[],
                &preferences,
                accounts,
            )
            .await?;
        let resolved = match selection {
            Some(selection) => Self::confirm_agent_launch(&effective, selection)?,
            None => Self::resolve_agent_launch_for_dispatch(&effective, &preferences)?,
        };
        Ok(PersistedAgentLaunchV1 {
            preferences,
            resolved,
        })
    }

    /// Resolve effective Runtime Config for an agent launch (config_service
    /// D12): the ordinary merge plus in-memory previews, then the provider
    /// account the preferences select — an explicit account, or the effective
    /// `<provider>_account_id` assignment when inheriting — projected into
    /// both channels after the family's raw keys are scrubbed. Visibility is
    /// decided from the launch triple by `ProviderAccountService`; an
    /// invisible, missing, or wrong-family account fails `account_not_visible`
    /// with no fallback (external_agent_provider_contract D11).
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_effective_for_agent_launch(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        user_id: Option<Uuid>,
        previews: &[AgentLaunchSettingsPreview],
        preferences: &AgentLaunchPreferencesV1,
        accounts: &ProviderAccountService,
    ) -> Result<EffectiveConfig, AppError> {
        let mut effective = self
            .resolve_effective_with_settings_preview(db, org_id, workspace_id, user_id, previews)
            .await?;
        let provider = resolve_launch_provider(preferences, &effective.settings)
            .map_err(AppError::from_error)?;
        let Some(account_id) =
            account_selection_for_launch(preferences, provider, &effective.settings)?
        else {
            return Ok(effective);
        };
        let credential = accounts
            .resolve_for_launch(db, org_id, workspace_id, user_id, account_id)
            .await?;
        apply_provider_account_projection(&mut effective, &credential, provider)?;
        Ok(effective)
    }

    /// Rehydrate an immutable attempt resolution for a current workspace
    /// owner. This repeats the
    /// account visibility check against the current owner so an ownership
    /// transfer can never retain the prior owner's billing principal.
    pub async fn resolve_effective_for_authorized_frozen_launch(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        owner_user_id: Uuid,
        frozen: &ResolvedAgentExecutionV1,
        accounts: &ProviderAccountService,
    ) -> Result<EffectiveConfig, AppError> {
        frozen.validate().map_err(AppError::from_error)?;
        let mut effective = self
            .resolve_effective(db, org_id, Some(workspace_id), Some(owner_user_id))
            .await?;
        let Some(account) = frozen.account() else {
            return Ok(effective);
        };
        let credential = accounts
            .resolve_for_launch(
                db,
                org_id,
                Some(workspace_id),
                Some(owner_user_id),
                account.id,
            )
            .await?;
        if credential.kind != account.kind || credential.scope != account.scope {
            return Err(account_not_visible_error());
        }
        apply_provider_account_projection(&mut effective, &credential, frozen.provider.value)?;
        Ok(effective)
    }

    pub fn preview_agent_launch(
        effective: &EffectiveConfig,
        preferences: &AgentLaunchPreferencesV1,
    ) -> Result<ResolvedAgentExecutionV1, AppError> {
        let config_revision = Self::effective_config_revision(effective)?;
        let resolved = preview_agent_launch_projection(AgentLaunchResolutionInput {
            preferences,
            effective_settings: &effective.settings,
            effective_secrets: &effective.secrets,
            config_revision: &config_revision,
            account: effective.account,
        })
        .map_err(AppError::from_error)?;
        ensure_account_projection_applied(effective, preferences, &resolved)?;
        Ok(resolved)
    }

    pub fn confirm_agent_launch(
        effective: &EffectiveConfig,
        selection: &AgentLaunchSelectionV1,
    ) -> Result<ResolvedAgentExecutionV1, AppError> {
        selection.validate().map_err(AppError::from_error)?;
        let resolved = Self::preview_agent_launch(effective, &selection.preferences)?;
        if resolved.resolution_id != selection.confirmed_resolution_id {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: format!(
                    "stale_agent_launch_resolution: confirmed {}, current {}; review the updated resolved values and confirm again",
                    selection.confirmed_resolution_id, resolved.resolution_id
                ),
            }));
        }
        Self::resolve_agent_launch_for_dispatch(effective, &selection.preferences)
    }

    pub(crate) fn resolve_agent_launch_for_dispatch(
        effective: &EffectiveConfig,
        preferences: &AgentLaunchPreferencesV1,
    ) -> Result<ResolvedAgentExecutionV1, AppError> {
        let config_revision = Self::effective_config_revision(effective)?;
        let resolved = resolve_agent_launch_for_dispatch(AgentLaunchResolutionInput {
            preferences,
            effective_settings: &effective.settings,
            effective_secrets: &effective.secrets,
            config_revision: &config_revision,
            account: effective.account,
        })
        .map_err(AppError::from_error)?;
        ensure_account_projection_applied(effective, preferences, &resolved)?;
        Ok(resolved)
    }

    fn effective_config_revision(effective: &EffectiveConfig) -> Result<String, AppError> {
        let mut secret_fingerprints = effective
            .secrets
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    format!("{:x}", Sha256::digest(value.as_bytes())),
                )
            })
            .collect::<Vec<_>>();
        secret_fingerprints.sort_by(|left, right| left.0.cmp(&right.0));
        let bytes = serde_json::to_vec(&(effective.settings.clone(), secret_fingerprints))
            .map_err(|error| {
                AppError::from_error(G5Error::Internal {
                    message: format!("failed to fingerprint effective agent config: {error}"),
                })
            })?;
        let digest = Sha256::digest(bytes);
        Ok(format!("cfg_{digest:x}"))
    }

    async fn fetch_org_secret_map(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
    ) -> Result<HashMap<String, String>, AppError> {
        let encrypted = sqlx::query_scalar!(
            "SELECT secrets_enc FROM org_configs WHERE org_id = $1",
            org_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(config_db_err)?
        .flatten();
        self.decrypt_secret_map(encrypted.as_deref())
    }

    async fn fetch_workspace_secret_map(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<HashMap<String, String>, AppError> {
        let encrypted = sqlx::query_scalar!(
            "SELECT secrets_enc FROM workspace_configs WHERE workspace_id = $1",
            workspace_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(config_db_err)?
        .flatten();
        self.decrypt_secret_map(encrypted.as_deref())
    }

    async fn fetch_user_secret_map(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<HashMap<String, String>, AppError> {
        let encrypted = sqlx::query_scalar!(
            "SELECT secrets_enc FROM user_configs WHERE org_id = $1 AND user_id = $2",
            org_id,
            user_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(config_db_err)?
        .flatten();
        self.decrypt_secret_map(encrypted.as_deref())
    }

    fn merge_secret_maps(
        &self,
        mut current: HashMap<String, String>,
        set_secrets: HashMap<String, String>,
        remove_secret_keys: Vec<String>,
    ) -> Result<SecretState, AppError> {
        for key in remove_secret_keys {
            current.remove(&key);
        }
        // Key-local rules are final here (`g5.chat.secret-admission`): a
        // violating key is refused before it is stored. A write that only
        // removes always succeeds, so a stored key that predates the policy
        // can always be taken out.
        let sets_anything = !set_secrets.is_empty();
        for (key, value) in set_secrets {
            if is_retired_codex_auth_key(&key) {
                return Err(retired_codex_auth_config_error());
            }
            g5_contracts::types::task_env::validate_task_env_secret(&key, &value)
                .map_err(task_env_config_error)?;
            current.insert(key, value);
        }
        let current = self.without_retired_secrets(current);
        if sets_anything {
            // One scope can never exceed the budget on its own; the merged
            // effective set is checked again at admission, where it is final.
            g5_contracts::types::task_env::validate_task_env_secret_bounds(
                current
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .map_err(task_env_config_error)?;
        }
        self.secret_state_from_map(current)
    }

    /// Read the workspace-wide preferred coding-agent provider from a merged
    /// effective `settings` JSON object (see config_service ontology D3).
    ///
    /// Returns `None` when `default_agent_provider` is absent, not a string, or
    /// not a recognized provider id, so callers fall back to their own default
    /// (resolve against advertised worker capabilities, then first available).
    /// Pure (no I/O) so it is unit-testable without a database. The value is a
    /// plain provider discriminator, not a credential, so it lives in the
    /// readable settings channel.
    #[must_use]
    pub fn default_agent_provider_from_settings(settings: &Value) -> Option<ChatAgentProvider> {
        settings
            .get("default_agent_provider")
            .and_then(Value::as_str)
            .and_then(|raw| ChatAgentProvider::from_str(raw).ok())
    }

    /// Derive the bounded beginning-of-key display hint for one decrypted
    /// secret value (config_service D9; sanctioned by security_policy
    /// secret_custody_and_redaction D9): exactly the first
    /// [`SECRET_HINT_CHARS`] characters, and only when the value is
    /// hint-eligible — at least [`SECRET_HINT_MIN_VALUE_CHARS`] characters
    /// long, single-line (no Unicode mandatory line break, not just CR/LF),
    /// not `{`/`[`-leading after trimming leading whitespace and invisible
    /// formatting characters (BOM/zero-width), and the key is not
    /// `CODEX_AUTH_JSON` (structured secrets surface only their own
    /// non-secret diagnostics). Ineligible values return `None` and keep the
    /// masked display. Pure (no I/O) so the eligibility rules are
    /// unit-testable without a database. Everything beyond the returned
    /// prefix remains secret material and must never be projected anywhere.
    #[must_use]
    pub fn secret_display_hint(key: &str, value: &str) -> Option<String> {
        // Every Unicode mandatory line break (config_service D9): LF, VT, FF,
        // CR, NEL, LS, PS. CR/LF alone would let a U+2028-style separator
        // smuggle a multi-line value past the single-line rule.
        const LINE_BREAKS: [char; 7] = [
            '\n', '\u{0B}', '\u{0C}', '\r', '\u{85}', '\u{2028}', '\u{2029}',
        ];
        // Invisible formatting characters that could hide a leading `{`/`[`
        // from the JSON-shape check: BOM, zero-width space/joiners, word
        // joiner.
        const INVISIBLE_PREFIX: [char; 5] =
            ['\u{FEFF}', '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}'];

        if key.trim() == CODEX_AUTH_JSON_ENV_KEY {
            return None;
        }
        if value.contains(LINE_BREAKS) {
            return None;
        }
        if value.chars().count() < SECRET_HINT_MIN_VALUE_CHARS {
            return None;
        }
        let trimmed =
            value.trim_start_matches(|c: char| c.is_whitespace() || INVISIBLE_PREFIX.contains(&c));
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return None;
        }
        Some(value.chars().take(SECRET_HINT_CHARS).collect())
    }

    /// Derive the display-hint map for a decrypted secret map (config_service
    /// D9): key → bounded hint for each hint-eligible value; ineligible keys
    /// are absent. Used for both the own-scope maps and the merged effective
    /// map, so each effective hint reflects the same winning scope as the
    /// effective value.
    #[must_use]
    fn secret_hints_from_map(secrets: &HashMap<String, String>) -> HashMap<String, String> {
        secrets
            .iter()
            .filter_map(|(key, value)| {
                Self::secret_display_hint(key, value).map(|hint| (key.clone(), hint))
            })
            .collect()
    }

    /// Build a [`ScopeConfigView`] from an optional scope row, decrypting the
    /// scope's own secret map only to derive the bounded display hints
    /// (config_service D9). The decrypted values never leave this function;
    /// only key names and sanctioned hints reach the view.
    fn scope_view_from_row(
        &self,
        row: Option<ScopeConfigRow>,
    ) -> Result<ScopeConfigView, AppError> {
        match row {
            Some(row) => {
                let secrets = self.decrypt_secret_map(row.secrets_enc.as_deref())?;
                let mut secret_keys: Vec<String> = secrets.keys().cloned().collect();
                secret_keys.sort();
                Ok(ScopeConfigView {
                    settings: without_retired_codex_auth_setting(row.settings_json),
                    secret_keys,
                    secret_hints: Self::secret_hints_from_map(&secrets),
                    updated_by: row.updated_by,
                    updated_at: Some(row.updated_at),
                })
            }
            None => Ok(ScopeConfigView {
                settings: Value::Object(Map::new()),
                secret_keys: Vec::new(),
                secret_hints: HashMap::new(),
                updated_by: None,
                updated_at: None,
            }),
        }
    }

    fn decrypt_secret_map(
        &self,
        encrypted: Option<&[u8]>,
    ) -> Result<HashMap<String, String>, AppError> {
        self.decrypt_secret_map_raw(encrypted)
            .map(|secrets| self.without_retired_secrets(secrets))
    }

    fn decrypt_secret_map_raw(
        &self,
        encrypted: Option<&[u8]>,
    ) -> Result<HashMap<String, String>, AppError> {
        let Some(encrypted) = encrypted else {
            return Ok(HashMap::new());
        };
        let manager = self.scm_token_manager.as_ref().ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message:
                    "Encrypted config secrets exist but G5_SCM_ENCRYPTION_KEY is not configured"
                        .into(),
            })
        })?;
        let plaintext = manager.decrypt_token(encrypted)?;
        serde_json::from_str::<HashMap<String, String>>(&plaintext).map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to decode stored secrets: {e}"),
            })
        })
    }

    fn without_retired_secrets(
        &self,
        mut secrets: HashMap<String, String>,
    ) -> HashMap<String, String> {
        secrets.retain(|key, _| !is_retired_codex_auth_key(key));
        secrets
    }

    fn secret_state_from_map(
        &self,
        secrets: HashMap<String, String>,
    ) -> Result<SecretState, AppError> {
        let mut secret_keys: Vec<String> = secrets.keys().cloned().collect();
        secret_keys.sort();
        let encrypted = if secrets.is_empty() {
            None
        } else {
            let payload = serde_json::to_string(&secrets).map_err(|e| {
                AppError::from_error(G5Error::Internal {
                    message: format!("Failed to serialize secret map: {e}"),
                })
            })?;
            let manager = self.scm_token_manager.as_ref().ok_or_else(|| {
                AppError::from_error(G5Error::Internal {
                    message: "SCM encryption key is not configured; secret storage is unavailable"
                        .into(),
                })
            })?;
            Some(manager.encrypt_token(&payload)?)
        };
        Ok(SecretState {
            encrypted,
            secret_keys,
        })
    }
}

struct SecretState {
    encrypted: Option<Vec<u8>>,
    secret_keys: Vec<String>,
}

/// The provider-namespaced assignment setting for a launch provider
/// (external_agent_provider_contract D14).
pub(crate) fn account_assignment_setting_key(provider: ChatAgentProvider) -> &'static str {
    match provider {
        ChatAgentProvider::ClaudeCode => CLAUDE_ACCOUNT_ID_SETTING,
        ChatAgentProvider::Codex => CODEX_ACCOUNT_ID_SETTING,
    }
}

/// Which account, if any, the launch preferences select before any visibility
/// check (config_service D12). Pure: an explicit account wins; an explicit
/// `runtime_config` or `canonical_default` selects none; `inherit_configured`
/// reads the effective (post-preview) `<provider>_account_id`, which must be a
/// UUID string when present.
fn account_selection_for_launch(
    preferences: &AgentLaunchPreferencesV1,
    provider: ChatAgentProvider,
    settings: &Value,
) -> Result<Option<Uuid>, AppError> {
    match &preferences.account {
        AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::RuntimeConfig)
        | AgentLaunchFieldPreferenceV1::CanonicalDefault => Ok(None),
        AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::Account { account_id }) => {
            Ok(Some(*account_id))
        }
        AgentLaunchFieldPreferenceV1::InheritConfigured => {
            let key = account_assignment_setting_key(provider);
            match settings.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(raw)) => Uuid::parse_str(raw.trim())
                    .map(Some)
                    .map_err(|_| account_not_visible_error()),
                Some(_) => Err(account_not_visible_error()),
            }
        }
    }
}

/// Apply one resolved account to the merged effective config (config_service
/// D12): scrub the provider family's credential and route keys from BOTH the
/// settings object and the secret map, overlay the account's projected keys,
/// recompute the secret index and hints, and record the account triple. Only
/// that family is touched; model keys and the other provider's keys stay. A
/// family that does not support the launch provider is the same typed
/// `account_not_visible` failure as a missing or invisible account (D11).
fn apply_provider_account_projection(
    effective: &mut EffectiveConfig,
    credential: &ResolvedAccountCredential,
    provider: ChatAgentProvider,
) -> Result<(), AppError> {
    if !credential.kind.supports_agent(provider) {
        return Err(account_not_visible_error());
    }
    let projection = project_provider_account(
        credential.kind,
        credential.base_url.as_deref(),
        credential.secret.as_deref(),
        provider,
    )
    .map_err(AppError::from_error)?;
    if !effective.settings.is_object() {
        effective.settings = Value::Object(Map::new());
    }
    let settings = effective
        .settings
        .as_object_mut()
        .expect("effective settings normalized to an object");
    for key in &projection.scrub_keys {
        settings.remove(*key);
        effective.secrets.remove(*key);
    }
    for (key, value) in projection.settings {
        settings.insert(key, value);
    }
    for (key, value) in projection.secrets {
        effective.secrets.insert(key, value);
    }
    let mut secret_keys: Vec<String> = effective.secrets.keys().cloned().collect();
    secret_keys.sort();
    effective.secret_keys = secret_keys;
    effective.secret_hints = ConfigService::secret_hints_from_map(&effective.secrets);
    effective.account = Some(AgentLaunchAccountInput {
        id: credential.id,
        kind: credential.kind,
        scope: credential.scope,
    });
    Ok(())
}

/// Fail closed when a launch that selects an account was resolved against an
/// unprojected effective config (config_service D11): the resolver must never
/// silently run on raw keys when the preferences or the effective assignment
/// name an account.
fn ensure_account_projection_applied(
    effective: &EffectiveConfig,
    preferences: &AgentLaunchPreferencesV1,
    resolved: &ResolvedAgentExecutionV1,
) -> Result<(), AppError> {
    if effective.account.is_some() {
        return Ok(());
    }
    let selected =
        account_selection_for_launch(preferences, resolved.provider.value, &effective.settings)?;
    if selected.is_some() {
        return Err(AppError::from_error(G5Error::Internal {
            message: "agent launch selects a provider account but the effective config was not \
                      resolved through the account projection"
                .to_string(),
        }));
    }
    Ok(())
}

/// Validate every assignment key present in an incoming settings overlay for
/// the writing scope (config_service D13): a UUID string naming an account of
/// this org, whose family supports the key's provider, and whose scope sits at
/// or above the writing scope. Runs on the caller's tenant transaction (under
/// the D7 advisory lock for persisted writes).
async fn validate_assignment_keys(
    db: &mut ScopedDb,
    org_id: Uuid,
    scope: AssignmentWriteScope,
    settings: Option<&Value>,
) -> Result<(), AppError> {
    let Some(Value::Object(map)) = settings else {
        return Ok(());
    };
    for key in ASSIGNMENT_SETTING_KEYS {
        let Some(value) = map.get(*key) else {
            continue;
        };
        let Some(raw) = value.as_str() else {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": format!("settings.{key}"),
                    "code": "invalid_setting_type",
                    "message": format!("{key} must be a provider account id string"),
                }),
            }));
        };
        let account_id = Uuid::parse_str(raw.trim()).map_err(|_| account_not_visible_error())?;
        let Some(target) =
            ProviderAccountService::assignment_target(db, org_id, account_id).await?
        else {
            return Err(account_not_visible_error());
        };
        let provider = if *key == CLAUDE_ACCOUNT_ID_SETTING {
            ChatAgentProvider::ClaudeCode
        } else {
            ChatAgentProvider::Codex
        };
        if !target.kind.supports_agent(provider) {
            return Err(account_validation_error(
                key,
                "account_kind_mismatch",
                format!(
                    "{} accounts cannot be assigned as the {} account",
                    target.kind.as_str(),
                    provider.as_str()
                ),
            ));
        }
        let at_or_above = match (target.scope, scope) {
            (ProviderAccountScope::Org, _) => true,
            (ProviderAccountScope::Workspace, AssignmentWriteScope::Workspace(workspace_id)) => {
                target.workspace_id == Some(workspace_id)
            }
            (ProviderAccountScope::User, AssignmentWriteScope::User(user_id)) => {
                target.user_id == Some(user_id)
            }
            _ => false,
        };
        if !at_or_above {
            return Err(account_validation_error(
                key,
                "account_scope_forbidden",
                format!(
                    "a {} account cannot be assigned at this scope; assignments may only name \
                     accounts at or above the writing scope",
                    target.scope.as_str()
                ),
            ));
        }
    }
    Ok(())
}

fn is_retired_codex_auth_key(key: &str) -> bool {
    key.trim() == CODEX_AUTH_JSON_ENV_KEY
}

/// The owner-facing refusal of a secret write under the shared policy; the
/// Runtime Config editor shows `message`.
fn task_env_config_error(error: g5_contracts::types::task_env::TaskEnvSecretError) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({
            "field": "runtimeConfig",
            "code": error.code(),
            "message": error.to_string(),
        }),
    })
}

fn retired_codex_auth_config_error() -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({
            "field": "runtimeConfig",
            "code": "retired_runtime_config_key",
            "message": "CODEX_AUTH_JSON is retired; sign in to Codex in chat using device authorization",
        }),
    })
}

fn without_retired_codex_auth_setting(mut settings: Value) -> Value {
    if let Some(object) = settings.as_object_mut() {
        object.retain(|key, _| !is_retired_codex_auth_key(key));
    }
    settings
}

/// Advisory-lock key of an assignment write scope (config_service D7).
fn scope_lock_key(org_id: Uuid, scope: AssignmentWriteScope) -> String {
    match scope {
        AssignmentWriteScope::Org => org_scope_lock_key(org_id),
        AssignmentWriteScope::Workspace(workspace_id) => workspace_scope_lock_key(workspace_id),
        AssignmentWriteScope::User(user_id) => user_scope_lock_key(org_id, user_id),
    }
}

/// Per-scope advisory-lock key for the org scope (config_service D7).
fn org_scope_lock_key(org_id: Uuid) -> String {
    format!("g5:config-scope:org:{org_id}")
}

/// Per-scope advisory-lock key for the workspace scope (config_service D7).
fn workspace_scope_lock_key(workspace_id: Uuid) -> String {
    format!("g5:config-scope:workspace:{workspace_id}")
}

/// Per-scope advisory-lock key for the user scope (config_service D7). The org
/// id is part of the key because `user_configs` is keyed by `(org_id, user_id)`.
fn user_scope_lock_key(org_id: Uuid, user_id: Uuid) -> String {
    format!("g5:config-scope:user:{org_id}:{user_id}")
}

/// Serialize the read-merge-write sequence for one config scope
/// (config_service D7).
///
/// D1/D6 promise that a partial write preserves unspecified settings and secret
/// keys, but the merge is computed in memory from a snapshot read. Without a
/// per-scope fence two overlapping updates read the same stored state and the
/// later upsert resurrects the earlier writer's pre-change values, silently
/// losing that change. Taking a transaction advisory lock BEFORE the read makes
/// the loser block until the winner commits, so it merges against committed
/// state instead of a stale snapshot.
///
/// Transaction-scoped: it releases on commit OR rollback, so it MUST be taken on
/// the same `ScopedDb` transaction as the subsequent read and upsert — never a
/// separate pool acquire. This follows the repo's single-bigint
/// `hashtextextended` advisory convention used by repository operations,
/// whose keyspace is distinct from the two-argument
/// `pg_advisory_xact_lock(classid, objid)` space used by `chat_service`. Each
/// update takes exactly one scope key, so no lock-ordering deadlock is possible.
///
/// The wait is bounded (config_service D7): a transaction-local `lock_timeout`
/// is set first so a stalled holder cannot pin this `ScopedDb`/server-pool
/// connection indefinitely. `lock_timeout` bounds every statement that WAITS to
/// acquire any lock, which covers the whole write: the advisory fence, the reads
/// (a plain `SELECT` still takes an `ACCESS SHARE` table lock and can wait behind
/// an `ACCESS EXCLUSIVE` holder such as `ALTER TABLE`/`VACUUM FULL`), and the
/// settings/secret upsert (the scope row lock). On timeout Postgres raises
/// `lock_not_available` (SQLSTATE `55P03`); every config-table statement routes
/// its error through [`config_db_err`], mapping `55P03` to
/// `G5Error::ServiceUnavailable` (retryable 503) rather than a 500, so any stall
/// anywhere in the write surfaces the same retryable way. A timed-out writer has
/// committed nothing, so the per-scope serialization guarantee is intact.
async fn lock_scope_for_update(db: &mut ScopedDb, scope_key: &str) -> Result<(), AppError> {
    // The value is a validated integer (never caller input), so interpolating
    // it into `SET LOCAL` is injection-safe; `SET` cannot bind parameters.
    let timeout_ms = scope_lock_timeout_ms();
    sqlx::query(&format!("SET LOCAL lock_timeout = '{timeout_ms}ms'"))
        .execute(db.conn())
        .await?;

    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(scope_key)
        .execute(db.conn())
        .await
        .map_err(config_db_err)?;
    Ok(())
}

/// Map a DB error from any config-table statement inside a bounded config write
/// to the promised retryable response (config_service D7): a `lock_timeout`
/// abort (`55P03`) becomes `G5Error::ServiceUnavailable` (503); anything else
/// flows through the standard `sqlx::Error` conversion (typically a 500).
///
/// Applied to every config-table statement the write executes — the advisory
/// fence, each scope/secret read (whose `ACCESS SHARE` table lock can wait behind
/// an `ACCESS EXCLUSIVE` holder), and the settings/secret upsert — so a stall
/// anywhere surfaces the same retryable way rather than one path being a 503 and
/// another a 500. Harmless on the pure-read (GET) paths, which set no
/// `lock_timeout` and so cannot raise `55P03`.
fn config_db_err(err: sqlx::Error) -> AppError {
    if is_lock_timeout(&err) {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "configuration for this scope is being updated concurrently; \
                      please retry"
                .to_string(),
        })
    } else {
        err.into()
    }
}

/// Default per-scope `lock_timeout` for config writes, in milliseconds
/// (config_service D7). Config writes are interactive and fast, so a wait past a
/// few seconds means a genuinely stalled holder — better surfaced as a
/// retryable 503 than left to pin a pool connection.
const DEFAULT_SCOPE_LOCK_TIMEOUT_MS: u64 = 3000;

/// Resolve the config-scope `lock_timeout`, honoring the
/// `G5_CONFIG_SCOPE_LOCK_TIMEOUT_MS` override (clamped to a sane range) so tests
/// can exercise the timeout path quickly.
fn scope_lock_timeout_ms() -> u64 {
    std::env::var("G5_CONFIG_SCOPE_LOCK_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|ms| ms.clamp(1, 60_000))
        .unwrap_or(DEFAULT_SCOPE_LOCK_TIMEOUT_MS)
}

/// True when a sqlx error is Postgres `lock_not_available` (SQLSTATE `55P03`),
/// i.e. the `lock_timeout` fired while waiting for the per-scope fence.
fn is_lock_timeout(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("55P03")
    )
}

fn validate_agent_launch_settings_preview(
    preview: &AgentLaunchSettingsPreview,
) -> Result<(), AppError> {
    let setting_keys = match preview.settings.as_ref() {
        Some(Value::Object(settings)) => settings.keys().map(String::as_str).collect(),
        Some(_) => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "settingsPreview.settings": "settings must be a JSON object"
                }),
            }));
        }
        None => Vec::new(),
    };
    let invalid_key = setting_keys
        .into_iter()
        .chain(preview.remove_setting_keys.iter().map(String::as_str))
        .find(|key| !AGENT_LAUNCH_PREVIEW_SETTING_KEYS.contains(key));
    if let Some(key) = invalid_key {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "settingsPreview": format!(
                    "{key} is not an allowlisted non-secret agent-launch setting"
                )
            }),
        }));
    }
    Ok(())
}

fn effective_settings_with_previews(
    org_settings: &mut Value,
    workspace_settings: &mut Value,
    user_settings: &mut Value,
    workspace_available: bool,
    user_available: bool,
    previews: &[AgentLaunchSettingsPreview],
) -> Result<Value, AppError> {
    let mut seen_scopes = HashSet::new();
    for preview in previews {
        validate_agent_launch_settings_preview(preview)?;
        if !seen_scopes.insert(preview.scope) {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "settingsPreview": "each scope may appear at most once"
                }),
            }));
        }
        let current = match preview.scope {
            AgentLaunchSettingsPreviewScope::Org => &mut *org_settings,
            AgentLaunchSettingsPreviewScope::Workspace => {
                if !workspace_available {
                    return Err(AppError::from_error(G5Error::ValidationError {
                        details: serde_json::json!({
                            "settingsPreview": "workspace scope is unavailable"
                        }),
                    }));
                }
                &mut *workspace_settings
            }
            AgentLaunchSettingsPreviewScope::User => {
                if !user_available {
                    return Err(AppError::from_error(G5Error::ValidationError {
                        details: serde_json::json!({
                            "settingsPreview": "user scope is unavailable"
                        }),
                    }));
                }
                &mut *user_settings
            }
        };
        *current = merge_settings(
            current.clone(),
            preview.settings.clone(),
            preview.remove_setting_keys.clone(),
        )?;
    }

    let mut effective = Value::Object(Map::new());
    for scope_settings in [org_settings, workspace_settings, user_settings] {
        merge_json(&mut effective, scope_settings);
    }
    Ok(effective)
}

/// Merge an incoming partial `settings` object into the stored scope settings,
/// then drop any explicitly listed top-level keys. Keys the caller does not
/// mention are preserved, so a single-key write flips exactly that key and
/// leaves sibling keys intact (issue #1425; config_service ontology D1/D6). The
/// merge is the same recursive `merge_json` used for cross-scope effective
/// resolution. `None` settings leaves the stored object untouched except for the
/// requested removals; a present-but-non-object `settings` value is rejected.
fn merge_settings(
    current: Value,
    replacement: Option<Value>,
    remove_setting_keys: Vec<String>,
) -> Result<Value, AppError> {
    let mut next = match without_retired_codex_auth_setting(current) {
        Value::Object(map) => Value::Object(map),
        _ => Value::Object(Map::new()),
    };

    if let Some(overlay) = replacement {
        if !overlay.is_object() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "settings": "settings must be a JSON object"
                }),
            }));
        }
        if overlay
            .as_object()
            .is_some_and(|map| map.keys().any(|key| is_retired_codex_auth_key(key)))
        {
            return Err(retired_codex_auth_config_error());
        }
        if overlay
            .get(CODEX_SUBSCRIPTION_ENABLED_SETTING)
            .is_some_and(|value| !value.is_boolean())
        {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": format!("settings.{CODEX_SUBSCRIPTION_ENABLED_SETTING}"),
                    "code": "invalid_setting_type",
                    "message": format!("{CODEX_SUBSCRIPTION_ENABLED_SETTING} must be a boolean")
                }),
            }));
        }
        merge_json(&mut next, &overlay);
    }

    if !remove_setting_keys.is_empty() {
        if let Value::Object(map) = &mut next {
            for key in &remove_setting_keys {
                map.remove(key);
            }
        }
    }

    Ok(next)
}

fn merge_json(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_value) => merge_json(base_value, overlay_value),
                    None => {
                        base_map.insert(key.clone(), overlay_value.clone());
                    }
                }
            }
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ScmTokenManager;
    use g5_contracts::types::{AgentAuthModeV1, AgentProviderRouteV1, ProviderAccountKind};
    use serde_json::json;

    #[test]
    fn agent_launch_preview_reports_missing_credential_but_confirm_enforces_it() {
        let effective = EffectiveConfig {
            settings: json!({
                "ANTHROPIC_MODEL": "claude-sonnet-4-6",
                "claude_permission_mode": "default"
            }),
            secret_keys: Vec::new(),
            secrets: HashMap::new(),
            secret_hints: HashMap::new(),
            account: None,
        };
        let preferences = AgentLaunchPreferencesV1::default();
        let preview = ConfigService::preview_agent_launch(&effective, &preferences)
            .expect("preview should return concrete defaults without a credential");

        assert_eq!(preview.model.value, "claude-sonnet-4-6");
        assert!(!preview.credential_ready);
        assert!(!preview.requires_interactive_auth);

        let selection = AgentLaunchSelectionV1 {
            preferences,
            confirmed_resolution_id: preview.resolution_id,
        };
        let error = ConfigService::confirm_agent_launch(&effective, &selection)
            .expect_err("non-interactive dispatch still requires a credential");
        assert!(format!("{:?}", error.inner).contains("missing_llm_credential"));
    }

    fn credential(
        kind: ProviderAccountKind,
        base_url: Option<&str>,
        secret: Option<&str>,
    ) -> ResolvedAccountCredential {
        ResolvedAccountCredential {
            id: Uuid::from_u128(0x5eed),
            kind,
            scope: ProviderAccountScope::User,
            base_url: base_url.map(str::to_string),
            secret: secret.map(str::to_string),
        }
    }

    /// Org raw keys of BOTH families plus model keys, as merged before the
    /// account projection (config_service D12 fixture).
    fn raw_effective() -> EffectiveConfig {
        let secrets = HashMap::from([
            (
                "ANTHROPIC_AUTH_TOKEN".to_string(),
                "sk-or-v1-raw-openrouter-bearer-0123456789".to_string(),
            ),
            (
                "ANTHROPIC_API_KEY".to_string(),
                "sk-ant-api03-raw-key-0123456789abcdef".to_string(),
            ),
            (
                "OPENAI_API_KEY".to_string(),
                "sk-openai-raw-key-0123456789abcdef".to_string(),
            ),
            (
                "GH_TOKEN".to_string(),
                "ghp_task_token_0123456789abcdefghij".to_string(),
            ),
        ]);
        let mut secret_keys: Vec<String> = secrets.keys().cloned().collect();
        secret_keys.sort();
        EffectiveConfig {
            settings: json!({
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "ANTHROPIC_MODEL": "claude-opus-4-8[1m]",
                "CLAUDE_CODE_SUBAGENT_MODEL": "claude-sonnet-4-6",
                "OPENAI_BASE_URL": "https://openrouter.ai/api/v1",
                "OPENAI_MODEL": "gpt-5.6-sol",
                "codex_subscription_enabled": true,
                "claude_effort": "high",
                "claude_permission_mode": "default",
                "codex_permission_mode": "never/workspace-write",
            }),
            secret_keys,
            secret_hints: ConfigService::secret_hints_from_map(&secrets),
            secrets,
            account: None,
        }
    }

    #[test]
    fn account_projection_scrubs_only_its_family_from_both_channels_then_overlays() {
        let mut effective = raw_effective();
        let account = credential(
            ProviderAccountKind::ClaudeSubscription,
            None,
            Some("sk-ant-oat01-subscription-token-0123456789"),
        );
        apply_provider_account_projection(&mut effective, &account, ChatAgentProvider::ClaudeCode)
            .expect("claude subscription projects for Claude Code");

        // Claude family scrubbed from both channels, then overlaid.
        assert!(!effective.secrets.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert!(!effective.secrets.contains_key("ANTHROPIC_API_KEY"));
        assert_eq!(
            effective
                .secrets
                .get("CLAUDE_CODE_OAUTH_TOKEN")
                .map(String::as_str),
            Some("sk-ant-oat01-subscription-token-0123456789")
        );
        assert_eq!(
            effective.settings["ANTHROPIC_BASE_URL"],
            json!("https://api.anthropic.com")
        );
        // Model keys and unrelated task keys are untouched.
        assert_eq!(
            effective.settings["ANTHROPIC_MODEL"],
            json!("claude-opus-4-8[1m]")
        );
        assert_eq!(
            effective.settings["CLAUDE_CODE_SUBAGENT_MODEL"],
            json!("claude-sonnet-4-6")
        );
        assert_eq!(effective.settings["claude_effort"], json!("high"));
        assert!(effective.secrets.contains_key("GH_TOKEN"));
        // The other provider's family is untouched.
        assert!(effective.secrets.contains_key("OPENAI_API_KEY"));
        assert_eq!(
            effective.settings["OPENAI_BASE_URL"],
            json!("https://openrouter.ai/api/v1")
        );
        assert_eq!(effective.settings["OPENAI_MODEL"], json!("gpt-5.6-sol"));
        assert_eq!(
            effective.settings["codex_subscription_enabled"],
            json!(true)
        );
        // Secret index and hints follow the projected map; the triple is set.
        let mut expected_keys: Vec<String> = effective.secrets.keys().cloned().collect();
        expected_keys.sort();
        assert_eq!(effective.secret_keys, expected_keys);
        assert_eq!(
            effective
                .secret_hints
                .get("CLAUDE_CODE_OAUTH_TOKEN")
                .map(String::as_str),
            Some("sk-ant-oat01")
        );
        assert!(!effective.secret_hints.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert_eq!(
            effective.account,
            Some(AgentLaunchAccountInput {
                id: Uuid::from_u128(0x5eed),
                kind: ProviderAccountKind::ClaudeSubscription,
                scope: ProviderAccountScope::User,
            })
        );

        // Fed through the unchanged resolver: direct OAuth, never the shadowing
        // org bearer.
        let resolved = ConfigService::resolve_agent_launch_for_dispatch(
            &effective,
            &AgentLaunchPreferencesV1::default(),
        )
        .expect("projected config resolves");
        assert_eq!(resolved.auth_mode, AgentAuthModeV1::AnthropicOauth);
        assert_eq!(
            resolved.provider_route,
            AgentProviderRouteV1::AnthropicDirect
        );
        assert_eq!(resolved.account_id, Some(Uuid::from_u128(0x5eed)));
        assert_eq!(
            resolved.account_kind,
            Some(ProviderAccountKind::ClaudeSubscription)
        );
        assert_eq!(resolved.account_scope, Some(ProviderAccountScope::User));
    }

    #[test]
    fn codex_account_projection_scrubs_the_codex_family_and_forces_subscription_off() {
        let mut effective = raw_effective();
        let account = credential(
            ProviderAccountKind::OpenaiApiKey,
            None,
            Some("sk-openai-account-key-0123456789abcdef"),
        );
        apply_provider_account_projection(&mut effective, &account, ChatAgentProvider::Codex)
            .expect("openai api key projects for Codex");
        assert_eq!(
            effective.secrets.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-openai-account-key-0123456789abcdef")
        );
        assert_eq!(
            effective.settings["OPENAI_BASE_URL"],
            json!("https://api.openai.com")
        );
        assert_eq!(
            effective.settings["codex_subscription_enabled"],
            json!(false)
        );
        assert_eq!(effective.settings["OPENAI_MODEL"], json!("gpt-5.6-sol"));
        // Claude family untouched.
        assert!(effective.secrets.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert_eq!(
            effective.settings["ANTHROPIC_BASE_URL"],
            json!("https://openrouter.ai/api")
        );
    }

    #[test]
    fn openrouter_account_projects_per_agent() {
        let account = credential(
            ProviderAccountKind::OpenrouterApiKey,
            None,
            Some("sk-or-v1-account-key-0123456789abcdef"),
        );
        let mut claude = raw_effective();
        apply_provider_account_projection(&mut claude, &account, ChatAgentProvider::ClaudeCode)
            .expect("openrouter projects for Claude Code");
        assert_eq!(
            claude.settings["ANTHROPIC_BASE_URL"],
            json!("https://openrouter.ai/api")
        );
        assert_eq!(
            claude
                .secrets
                .get("ANTHROPIC_AUTH_TOKEN")
                .map(String::as_str),
            Some("sk-or-v1-account-key-0123456789abcdef")
        );
        assert!(claude.secrets.contains_key("OPENAI_API_KEY"));

        let mut codex = raw_effective();
        apply_provider_account_projection(&mut codex, &account, ChatAgentProvider::Codex)
            .expect("openrouter projects for Codex");
        assert_eq!(
            codex.settings["OPENAI_BASE_URL"],
            json!("https://openrouter.ai/api/v1")
        );
        assert_eq!(
            codex.secrets.get("OPENROUTER_API_KEY").map(String::as_str),
            Some("sk-or-v1-account-key-0123456789abcdef")
        );
        assert!(!codex.secrets.contains_key("OPENAI_API_KEY"));
        assert!(codex.secrets.contains_key("ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn wrong_family_account_fails_account_not_visible_before_projecting() {
        let mut effective = raw_effective();
        let before = effective.settings.clone();
        let account = credential(
            ProviderAccountKind::OpenaiApiKey,
            None,
            Some("sk-openai-account-key-0123456789abcdef"),
        );
        let error = apply_provider_account_projection(
            &mut effective,
            &account,
            ChatAgentProvider::ClaudeCode,
        )
        .expect_err("codex-family account cannot serve Claude Code");
        assert!(format!("{:?}", error.inner).contains("account_not_visible"));
        assert_eq!(effective.settings, before);
        assert!(effective.account.is_none());
    }

    #[test]
    fn account_selection_explicit_beats_inherit_and_runtime_config_skips_projection() {
        let assigned = Uuid::from_u128(0xa55);
        let explicit = Uuid::from_u128(0xe11);
        let settings = json!({ "claude_account_id": assigned.to_string() });

        let mut preferences = AgentLaunchPreferencesV1::default();
        assert_eq!(
            account_selection_for_launch(&preferences, ChatAgentProvider::ClaudeCode, &settings)
                .unwrap(),
            Some(assigned)
        );
        // The assignment is provider-namespaced: a Codex launch ignores it.
        assert_eq!(
            account_selection_for_launch(&preferences, ChatAgentProvider::Codex, &settings)
                .unwrap(),
            None
        );
        preferences.account =
            AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::Account {
                account_id: explicit,
            });
        assert_eq!(
            account_selection_for_launch(&preferences, ChatAgentProvider::ClaudeCode, &settings)
                .unwrap(),
            Some(explicit)
        );
        preferences.account =
            AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::RuntimeConfig);
        assert_eq!(
            account_selection_for_launch(&preferences, ChatAgentProvider::ClaudeCode, &settings)
                .unwrap(),
            None
        );
        preferences.account = AgentLaunchFieldPreferenceV1::CanonicalDefault;
        assert_eq!(
            account_selection_for_launch(&preferences, ChatAgentProvider::ClaudeCode, &settings)
                .unwrap(),
            None
        );
        // No assignment: nothing to project.
        assert_eq!(
            account_selection_for_launch(
                &AgentLaunchPreferencesV1::default(),
                ChatAgentProvider::ClaudeCode,
                &json!({})
            )
            .unwrap(),
            None
        );
        // A malformed stored assignment is typed, never silently ignored.
        let error = account_selection_for_launch(
            &AgentLaunchPreferencesV1::default(),
            ChatAgentProvider::ClaudeCode,
            &json!({ "claude_account_id": "not-a-uuid" }),
        )
        .expect_err("malformed assignment is rejected");
        assert!(format!("{:?}", error.inner).contains("account_not_visible"));
    }

    #[test]
    fn no_account_and_no_assignment_resolves_byte_identical_to_the_account_less_contract() {
        let effective = raw_effective();
        let resolved =
            ConfigService::preview_agent_launch(&effective, &AgentLaunchPreferencesV1::default())
                .expect("account-less preview");
        let wire = serde_json::to_value(&resolved).unwrap();
        assert!(wire
            .as_object()
            .unwrap()
            .keys()
            .all(|key| !key.starts_with("account")));
        assert!(resolved.account().is_none());
        // Explicit runtime_config with an assignment present is equally
        // account-less and never trips the projection guard.
        let preferences = AgentLaunchPreferencesV1 {
            account: AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::RuntimeConfig),
            ..Default::default()
        };
        let mut with_assignment = raw_effective();
        with_assignment.settings["claude_account_id"] = json!(Uuid::from_u128(0xa55).to_string());
        let runtime = ConfigService::preview_agent_launch(&with_assignment, &preferences)
            .expect("runtime_config skips projection");
        assert!(runtime.account().is_none());
        assert_eq!(runtime.auth_mode, AgentAuthModeV1::OpenRouterApiKey);
    }

    #[test]
    fn unprojected_effective_with_a_selected_account_fails_closed() {
        let mut effective = raw_effective();
        effective.settings["claude_account_id"] = json!(Uuid::from_u128(0xa55).to_string());
        let inherit =
            ConfigService::preview_agent_launch(&effective, &AgentLaunchPreferencesV1::default())
                .expect_err("inherited assignment on an unprojected config is a programming error");
        assert!(
            matches!(inherit.inner, G5Error::Internal { .. }),
            "{:?}",
            inherit.inner
        );

        let preferences = AgentLaunchPreferencesV1 {
            account: AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::Account {
                account_id: Uuid::from_u128(0xe11),
            }),
            ..Default::default()
        };
        let explicit =
            ConfigService::resolve_agent_launch_for_dispatch(&raw_effective(), &preferences)
                .expect_err("explicit account on an unprojected config is a programming error");
        assert!(
            matches!(explicit.inner, G5Error::Internal { .. }),
            "{:?}",
            explicit.inner
        );
    }

    #[test]
    fn config_revision_changes_when_the_account_secret_rotates_or_the_assignment_changes() {
        let projected = |secret: &str| {
            let mut effective = raw_effective();
            apply_provider_account_projection(
                &mut effective,
                &credential(ProviderAccountKind::AnthropicApiKey, None, Some(secret)),
                ChatAgentProvider::ClaudeCode,
            )
            .unwrap();
            effective
        };
        let first = projected("sk-ant-api03-account-a-0123456789abcdef");
        let rotated = projected("sk-ant-api03-account-b-0123456789abcdef");
        let first_revision = ConfigService::effective_config_revision(&first).unwrap();
        let rotated_revision = ConfigService::effective_config_revision(&rotated).unwrap();
        assert_ne!(first_revision, rotated_revision);
        assert!(!first_revision.contains("account-a"));

        // Confirming against the rotated config is stale.
        let preview =
            ConfigService::preview_agent_launch(&first, &AgentLaunchPreferencesV1::default())
                .unwrap();
        let selection = AgentLaunchSelectionV1 {
            preferences: AgentLaunchPreferencesV1::default(),
            confirmed_resolution_id: preview.resolution_id.clone(),
        };
        assert!(ConfigService::confirm_agent_launch(&first, &selection).is_ok());
        let stale = ConfigService::confirm_agent_launch(&rotated, &selection)
            .expect_err("rotated secret changes the resolution id");
        assert!(format!("{:?}", stale.inner).contains("stale_agent_launch_resolution"));

        // An assignment change is a settings change and moves the revision too.
        let mut reassigned = first.clone();
        reassigned.settings["claude_account_id"] = json!(Uuid::from_u128(0xb0b).to_string());
        assert_ne!(
            ConfigService::effective_config_revision(&reassigned).unwrap(),
            first_revision
        );
    }

    #[test]
    fn effective_config_revision_changes_when_a_secret_rotates() {
        let effective = |secret: &str| EffectiveConfig {
            settings: json!({"OPENAI_BASE_URL": "https://api.openai.com/v1"}),
            secret_keys: vec!["OPENAI_API_KEY".to_string()],
            secrets: HashMap::from([("OPENAI_API_KEY".to_string(), secret.to_string())]),
            secret_hints: HashMap::new(),
            account: None,
        };

        let first = ConfigService::effective_config_revision(&effective("secret-a"))
            .expect("first config revision");
        let rotated = ConfigService::effective_config_revision(&effective("secret-b"))
            .expect("rotated config revision");
        assert_ne!(first, rotated);
        assert!(!first.contains("secret-a"));
        assert!(!rotated.contains("secret-b"));
    }

    #[test]
    fn secret_display_hint_returns_first_twelve_chars_of_eligible_values() {
        // Real provider token shapes (config_service D9): the hint must carry
        // the full scheme discriminator so API key vs OAuth vs gateway is
        // legible.
        let api_key = "sk-ant-api03-0123456789abcdefghijklmnopqrstuvwxyz0123456789";
        let oauth = "sk-ant-oat01-0123456789abcdefghijklmnopqrstuvwxyz0123456789";
        let gateway = "sk-or-v1-0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            ConfigService::secret_display_hint("ANTHROPIC_API_KEY", api_key).as_deref(),
            Some("sk-ant-api03")
        );
        assert_eq!(
            ConfigService::secret_display_hint("ANTHROPIC_AUTH_TOKEN", oauth).as_deref(),
            Some("sk-ant-oat01")
        );
        assert_eq!(
            ConfigService::secret_display_hint("ANTHROPIC_AUTH_TOKEN", gateway).as_deref(),
            Some("sk-or-v1-012")
        );
    }

    #[test]
    fn secret_display_hint_rejects_ineligible_values() {
        // Below the 24-character floor (including the empty OpenRouter
        // sentinel for ANTHROPIC_API_KEY).
        assert_eq!(
            ConfigService::secret_display_hint("ANTHROPIC_API_KEY", ""),
            None
        );
        assert_eq!(
            ConfigService::secret_display_hint("ANTHROPIC_AUTH_TOKEN", "sk-short-token-value"),
            None
        );
        // Exactly at the floor is eligible; one below is not.
        let at_floor = "a".repeat(SECRET_HINT_MIN_VALUE_CHARS);
        let below_floor = "a".repeat(SECRET_HINT_MIN_VALUE_CHARS - 1);
        assert!(ConfigService::secret_display_hint("CUSTOM_TOKEN", &at_floor).is_some());
        assert_eq!(
            ConfigService::secret_display_hint("CUSTOM_TOKEN", &below_floor),
            None
        );
        // Multiline values never hint — including every Unicode mandatory
        // line break, not just CR/LF (Codex security review, 2026-08-11).
        for brk in [
            '\n', '\r', '\u{0B}', '\u{0C}', '\u{85}', '\u{2028}', '\u{2029}',
        ] {
            assert_eq!(
                ConfigService::secret_display_hint(
                    "CUSTOM_TOKEN",
                    &format!("sk-ant-api03-line-one-long-enough{brk}line-two"),
                ),
                None,
                "value containing U+{:04X} must be ineligible",
                brk as u32
            );
        }
        // Structured / JSON-shaped values never hint, with or without leading
        // whitespace.
        assert_eq!(
            ConfigService::secret_display_hint(
                "CUSTOM_TOKEN",
                "{\"tokens\":{\"access_token\":\"secret-material-here\"}}"
            ),
            None
        );
        assert_eq!(
            ConfigService::secret_display_hint(
                "CUSTOM_TOKEN",
                "  [\"secret-material-in-an-array-shape\"]"
            ),
            None
        );
        // An invisible prefix (BOM, zero-width, word joiner) must not hide a
        // JSON shape from the check (Codex security review, 2026-08-11).
        for invisible in ['\u{FEFF}', '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}'] {
            assert_eq!(
                ConfigService::secret_display_hint(
                    "CUSTOM_TOKEN",
                    &format!("{invisible}{{\"tokens\":{{\"access\":\"secret-material-here\"}}}}"),
                ),
                None,
                "U+{:04X}-prefixed JSON shape must be ineligible",
                invisible as u32
            );
        }
        // CODEX_AUTH_JSON is excluded by name even for token-shaped values.
        assert_eq!(
            ConfigService::secret_display_hint(
                CODEX_AUTH_JSON_ENV_KEY,
                "sk-ant-api03-0123456789abcdefghijklmnop"
            ),
            None
        );
    }

    #[test]
    fn secret_display_hint_is_character_bounded_for_multibyte_values() {
        // Character counting must never split a multibyte codepoint or widen
        // the bound: 12 characters exactly, regardless of byte length.
        let value = "ééééééééééééééééééééééééé"; // 25 chars, 50 bytes
        let hint = ConfigService::secret_display_hint("CUSTOM_TOKEN", value).expect("eligible");
        assert_eq!(hint.chars().count(), SECRET_HINT_CHARS);
        assert_eq!(hint, "éééééééééééé");
    }

    #[test]
    fn secret_hints_from_map_skips_ineligible_entries() {
        let mut secrets = HashMap::new();
        secrets.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            "sk-ant-oat01-0123456789abcdefghijklmnop".to_string(),
        );
        secrets.insert("SHORT".to_string(), "tiny".to_string());
        secrets.insert(
            CODEX_AUTH_JSON_ENV_KEY.to_string(),
            "{\"tokens\":{}}".to_string(),
        );

        let hints = ConfigService::secret_hints_from_map(&secrets);

        assert_eq!(hints.len(), 1);
        assert_eq!(
            hints.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str),
            Some("sk-ant-oat01")
        );
    }

    #[test]
    fn merge_json_overlays_nested_objects() {
        let mut base = json!({
            "llm": {
                "provider": "openai",
                "model": "gpt-4o-mini"
            },
            "embeddings": {
                "provider": "openai"
            }
        });
        let overlay = json!({
            "llm": {
                "model": "claude-sonnet-4"
            },
            "featureFlags": {
                "smartHttp": true
            }
        });

        merge_json(&mut base, &overlay);

        assert_eq!(
            base,
            json!({
                "llm": {
                    "provider": "openai",
                    "model": "claude-sonnet-4"
                },
                "embeddings": {
                    "provider": "openai"
                },
                "featureFlags": {
                    "smartHttp": true
                }
            })
        );
    }

    #[test]
    fn settings_write_preserves_unspecified_keys() {
        // Repro for issue #1425: a partial settings write must merge, not replace.
        // Two independent flags are set, then only one is toggled — the other must
        // survive (config_service ontology D1/D6).
        let current = json!({
            "fork_chat_enabled": true,
            "default_agent_provider": "claude_code"
        });
        let partial = json!({ "default_agent_provider": "codex" });

        let next =
            merge_settings(current, Some(partial), Vec::new()).expect("settings write succeeds");

        assert_eq!(
            next,
            json!({
                "fork_chat_enabled": true,
                "default_agent_provider": "codex"
            })
        );
    }

    #[test]
    fn settings_write_rejects_non_boolean_codex_subscription_preference() {
        for invalid in [json!("true"), json!(1), Value::Null] {
            let error = merge_settings(
                json!({}),
                Some(json!({"codex_subscription_enabled": invalid})),
                Vec::new(),
            )
            .expect_err("subscription preference must be a JSON boolean");

            assert!(format!("{:?}", error.inner).contains("codex_subscription_enabled"));
        }
    }

    #[test]
    fn agent_launch_settings_preview_reuses_scope_precedence_and_removals() {
        let mut org = json!({"OPENAI_BASE_URL": "https://api.openai.com/v1"});
        let mut workspace = json!({"OPENAI_BASE_URL": "https://openrouter.ai/api/v1"});
        let mut user = json!({"OPENAI_BASE_URL": "https://user.example.test/v1"});
        let previews = vec![AgentLaunchSettingsPreview {
            scope: AgentLaunchSettingsPreviewScope::User,
            settings: None,
            remove_setting_keys: vec!["OPENAI_BASE_URL".into()],
        }];

        let effective = effective_settings_with_previews(
            &mut org,
            &mut workspace,
            &mut user,
            true,
            true,
            &previews,
        )
        .expect("preview applies without persistence");

        assert_eq!(
            effective.get("OPENAI_BASE_URL").and_then(Value::as_str),
            Some("https://openrouter.ai/api/v1"),
            "clearing the user scope must reveal the workspace value"
        );
    }

    #[test]
    fn agent_launch_settings_preview_rejects_secrets_and_unrelated_keys() {
        for key in ["OPENAI_API_KEY", "unrelated_setting"] {
            let settings = Value::Object(Map::from_iter([(
                key.to_string(),
                Value::String("must-not-enter-preview".into()),
            )]));
            let preview = AgentLaunchSettingsPreview {
                scope: AgentLaunchSettingsPreviewScope::User,
                settings: Some(settings),
                remove_setting_keys: Vec::new(),
            };
            assert!(validate_agent_launch_settings_preview(&preview).is_err());
        }
    }

    #[test]
    fn settings_write_deep_merges_nested_objects() {
        let current = json!({ "llm": { "provider": "openai", "model": "gpt-4o-mini" } });
        let partial = json!({ "llm": { "model": "claude-sonnet-4" } });

        let next = merge_settings(current, Some(partial), Vec::new()).expect("nested merge");

        assert_eq!(
            next,
            json!({ "llm": { "provider": "openai", "model": "claude-sonnet-4" } })
        );
    }

    #[test]
    fn settings_write_removes_listed_keys_after_merge() {
        let current = json!({ "default_agent_provider": "codex", "keep": true });
        let partial = json!({ "keep": true });

        let next = merge_settings(
            current,
            Some(partial),
            vec!["default_agent_provider".to_string()],
        )
        .expect("removal applies");

        assert_eq!(next, json!({ "keep": true }));
    }

    #[test]
    fn settings_write_none_preserves_current_and_still_applies_removals() {
        let current = json!({ "a": 1, "b": 2 });

        // None settings, no removals → unchanged.
        let unchanged = merge_settings(current.clone(), None, Vec::new()).expect("unchanged");
        assert_eq!(unchanged, json!({ "a": 1, "b": 2 }));

        // None settings, explicit removal → key dropped without touching others.
        let pruned = merge_settings(current, None, vec!["a".to_string()]).expect("pruned");
        assert_eq!(pruned, json!({ "b": 2 }));
    }

    #[test]
    fn settings_write_rejects_non_object() {
        let err = match merge_settings(json!({}), Some(json!("nope")), Vec::new()) {
            Ok(_) => panic!("non-object settings must be rejected"),
            Err(err) => err,
        };
        assert!(matches!(err.inner, G5Error::ValidationError { .. }));
    }

    #[test]
    fn default_agent_provider_reads_codex_from_settings() {
        let settings = json!({ "default_agent_provider": "codex", "other": 1 });
        assert_eq!(
            ConfigService::default_agent_provider_from_settings(&settings),
            Some(ChatAgentProvider::Codex)
        );
    }

    #[test]
    fn default_agent_provider_reads_claude_code_from_settings() {
        let settings = json!({ "default_agent_provider": "claude_code" });
        assert_eq!(
            ConfigService::default_agent_provider_from_settings(&settings),
            Some(ChatAgentProvider::ClaudeCode)
        );
    }

    #[test]
    fn the_external_issues_switch_is_an_org_only_boolean() {
        let on = json!({ EXTERNAL_ISSUES_ENABLED_SETTING: true });
        assert!(validate_org_only_settings(AssignmentWriteScope::Org, Some(&on)).is_ok());
        assert!(validate_org_only_settings(AssignmentWriteScope::Org, None).is_ok());
        let error = |scope, settings: &Value| {
            format!(
                "{:?}",
                validate_org_only_settings(scope, Some(settings))
                    .unwrap_err()
                    .inner
            )
        };
        assert!(error(AssignmentWriteScope::Workspace(Uuid::new_v4()), &on)
            .contains("org_only_setting"));
        assert!(error(AssignmentWriteScope::User(Uuid::new_v4()), &on).contains("org_only_setting"));
        assert!(error(
            AssignmentWriteScope::Org,
            &json!({ EXTERNAL_ISSUES_ENABLED_SETTING: "true" })
        )
        .contains("invalid_setting_type"));
        // The external-check merge policy is an org-only boolean too.
        let block = json!({ EXTERNAL_CHECKS_BLOCK_MERGE_SETTING: true });
        assert!(validate_org_only_settings(AssignmentWriteScope::Org, Some(&block)).is_ok());
        assert!(
            error(AssignmentWriteScope::Workspace(Uuid::new_v4()), &block)
                .contains("org_only_setting")
        );
        assert!(error(
            AssignmentWriteScope::Org,
            &json!({ EXTERNAL_CHECKS_BLOCK_MERGE_SETTING: 1 })
        )
        .contains("invalid_setting_type"));
        // So is the Re-run Verification switch.
        let verify = json!({ VERIFICATION_ACTION_ENABLED_SETTING: false });
        assert!(validate_org_only_settings(AssignmentWriteScope::Org, Some(&verify)).is_ok());
        assert!(
            error(AssignmentWriteScope::User(Uuid::new_v4()), &verify).contains("org_only_setting")
        );
        assert!(error(
            AssignmentWriteScope::Org,
            &json!({ VERIFICATION_ACTION_ENABLED_SETTING: "on" })
        )
        .contains("invalid_setting_type"));
        // Other settings are not affected.
        assert!(validate_org_only_settings(
            AssignmentWriteScope::User(Uuid::new_v4()),
            Some(&json!({ "claude_effort": "high" }))
        )
        .is_ok());
    }

    #[test]
    fn default_agent_provider_absent_or_invalid_is_none() {
        assert_eq!(
            ConfigService::default_agent_provider_from_settings(&json!({})),
            None
        );
        assert_eq!(
            ConfigService::default_agent_provider_from_settings(
                &json!({ "default_agent_provider": "nonsense" })
            ),
            None
        );
        assert_eq!(
            ConfigService::default_agent_provider_from_settings(
                &json!({ "default_agent_provider": 7 })
            ),
            None
        );
    }

    #[test]
    fn codex_auth_json_secret_is_rejected_from_runtime_config() {
        let service = ConfigService::new(None);
        let mut set_secrets = HashMap::new();
        set_secrets.insert(
            CODEX_AUTH_JSON_ENV_KEY.to_string(),
            r#"{"tokens":{"access_token":"valid-but-task-bound"}}"#.to_string(),
        );

        let err = match service.merge_secret_maps(HashMap::new(), set_secrets, Vec::new()) {
            Ok(_) => panic!("CODEX_AUTH_JSON must never enter Runtime Config"),
            Err(err) => err,
        };

        assert!(matches!(err.inner, G5Error::ValidationError { .. }));
    }

    /// `g5.chat.secret-admission`: a write validates each key it sets under
    /// the shared policy and the written scope's own bounds; removals always
    /// succeed, so a stored key that predates the policy can be taken out.
    #[test]
    fn secret_writes_follow_the_shared_policy_and_removals_always_succeed() {
        use g5_contracts::types::task_env::{MAX_TASK_ENV_SECRETS, MAX_TASK_ENV_VALUE_BYTES};
        let manager = Arc::new(ScmTokenManager::new(
            [7; 32],
            g5_scm::providers::ProviderClients::new(),
        ));
        let service = ConfigService::new(Some(manager));
        let code = |err: &AppError| match &err.inner {
            G5Error::ValidationError { details } => details["code"].as_str().unwrap().to_string(),
            other => panic!("unexpected error: {other}"),
        };
        let set = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        };

        // Key-local refusals name their code.
        for (pairs, expected) in [
            (set(&[("lower", "v")]), "secret_name_invalid"),
            (set(&[("HOME", "/x")]), "secret_name_reserved"),
            (set(&[("GIT_CONFIG_GLOBAL", "/x")]), "secret_name_reserved"),
            (set(&[("G5D_API_URL", "x")]), "secret_name_reserved"),
            (set(&[("NUL", "a\0b")]), "secret_value_nul"),
            (
                set(&[("BIG", &"x".repeat(MAX_TASK_ENV_VALUE_BYTES + 1))]),
                "secret_value_too_large",
            ),
        ] {
            let err = service
                .merge_secret_maps(HashMap::new(), pairs, Vec::new())
                .err()
                .expect("refused");
            assert_eq!(code(&err), expected);
        }

        // Boundaries pass: the longest name, the largest value, the most keys.
        let longest = "A".repeat(128);
        let state = service
            .merge_secret_maps(
                HashMap::new(),
                set(&[(longest.as_str(), &"x".repeat(MAX_TASK_ENV_VALUE_BYTES))]),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(state.secret_keys, vec![longest.clone()]);
        let full: HashMap<String, String> = (0..MAX_TASK_ENV_SECRETS)
            .map(|index| (format!("K{index}"), "v".to_string()))
            .collect();
        let state = service
            .merge_secret_maps(full.clone(), HashMap::new(), Vec::new())
            .unwrap();
        assert_eq!(state.secret_keys.len(), MAX_TASK_ENV_SECRETS);

        // One more key than the scope may hold is refused; swapping one out
        // for another in the same write stays within the bound.
        let err = service
            .merge_secret_maps(full.clone(), set(&[("EXTRA", "v")]), Vec::new())
            .err()
            .expect("over count");
        assert_eq!(code(&err), "secrets_too_many");
        let state = service
            .merge_secret_maps(full.clone(), set(&[("EXTRA", "v")]), vec!["K0".into()])
            .unwrap();
        assert_eq!(state.secret_keys.len(), MAX_TASK_ENV_SECRETS);
        assert!(state.secret_keys.contains(&"EXTRA".to_string()));

        // The scope's serialized size is bounded too.
        let big = "x".repeat(MAX_TASK_ENV_VALUE_BYTES);
        let three = set(&[("A", &big), ("B", &big), ("C", &big)]);
        assert!(service
            .merge_secret_maps(three.clone(), HashMap::new(), Vec::new())
            .is_ok());
        let err = service
            .merge_secret_maps(three, set(&[("D", &big)]), Vec::new())
            .err()
            .expect("over budget");
        assert_eq!(code(&err), "secrets_too_large");

        // A stored key that predates the policy neither blocks other writes
        // nor its own removal.
        let legacy = set(&[("lower", "old"), ("GOOD", "v")]);
        let state = service
            .merge_secret_maps(legacy.clone(), set(&[("OTHER", "v")]), Vec::new())
            .unwrap();
        assert_eq!(state.secret_keys, vec!["GOOD", "OTHER", "lower"]);
        let state = service
            .merge_secret_maps(legacy, HashMap::new(), vec!["lower".into()])
            .unwrap();
        assert_eq!(state.secret_keys, vec!["GOOD"]);
    }

    #[test]
    fn retired_codex_auth_json_is_removed_without_dropping_sibling_secrets() {
        let manager = Arc::new(ScmTokenManager::new(
            [7; 32],
            g5_scm::providers::ProviderClients::new(),
        ));
        let service = ConfigService::new(Some(manager));
        let current = HashMap::from([
            (
                CODEX_AUTH_JSON_ENV_KEY.to_string(),
                r#"{"tokens":{"access_token":"legacy"}}"#.to_string(),
            ),
            ("OPENAI_API_KEY".to_string(), "keep-me".to_string()),
        ]);

        let state = service
            .secret_state_from_map(service.without_retired_secrets(current))
            .expect("remaining secret re-encrypts");
        let decoded = service
            .decrypt_secret_map(state.encrypted.as_deref())
            .expect("decrypt stored secret map");

        assert!(!decoded.contains_key(CODEX_AUTH_JSON_ENV_KEY));
        assert_eq!(
            decoded.get("OPENAI_API_KEY").map(String::as_str),
            Some("keep-me")
        );
        assert_eq!(state.secret_keys, vec!["OPENAI_API_KEY".to_string()]);
    }
}
