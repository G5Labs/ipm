// ---------------------------------------------------------------------------
// g5-api/src/services/agents/provider_account_service.rs — Provider Account Service
//
// Custody owner for named, scoped provider accounts (`provider_accounts`,
// migration 292): visibility by scope, create/update/delete under row-scope
// authority, kind/base-URL/secret validation, encrypted credential storage,
// the read-time bounded display hint, the assignment delete guard, one audit
// event per mutation, and the launch-time credential resolution.
//
// Ontology: software_architecture/g5-api/services/provider_account_service
// D1–D9; system_invariants/external_agent_provider_contract D14 (scoped
// credential bundles, closed kind catalog, scope authority, refuse delete
// while assigned, audit event, hint-only clients) and D11 (`account_not_visible`
// with no fallback); security_policy/secret_custody_and_redaction D1, D5, D9,
// D10; data_model/tenancy_persistence D2/D3; observability_audit D3;
// role_based_access_control D9.
// ---------------------------------------------------------------------------

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{
    ChatAgentProvider, ProviderAccountFamily, ProviderAccountKind, ProviderAccountScope,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::scoped::require_current_workspace_owner;
use crate::errors::AppError;
use crate::models::ProviderAccount;
use crate::services::{AssignmentWriteScope, ConfigService, PermissionService};

/// Non-secret scoped setting naming the Claude Code account assignment
/// (external_agent_provider_contract D14).
pub const CLAUDE_ACCOUNT_ID_SETTING: &str = "claude_account_id";
/// Non-secret scoped setting naming the Codex account assignment (D14).
pub const CODEX_ACCOUNT_ID_SETTING: &str = "codex_account_id";
/// Upper bound on a trimmed account name, mirroring the migration-292 CHECK.
pub const PROVIDER_ACCOUNT_NAME_MAX_CHARS: usize = 80;

/// Effective scope a caller must hold to manage org-scoped accounts (D3).
const ORG_ACCOUNT_SCOPE: &str = "org:admin";
/// Effective workspace scope for managing workspace-scoped accounts (D3).
const WORKSPACE_WRITE_SCOPE: &str = "workspaces:write";
/// Effective workspace scope for seeing workspace-scoped accounts (D2).
const WORKSPACE_READ_SCOPE: &str = "workspaces:read";
/// Pseudo-scope reported when a non-owner touches a user account (D3).
const OWNER_PSEUDO_SCOPE: &str = "provider_account:owner";

const ACCOUNT_COLUMNS: &str = "id, org_id, scope, workspace_id, user_id, name, kind, base_url, \
                               secret_enc, created_by, created_at, updated_at";

/// Service owning every `provider_accounts` read and mutation (I1).
///
/// Queries use the runtime-checked `sqlx::query` API so `SQLX_OFFLINE` builds
/// stay independent of cached metadata for the new table (D1).
#[derive(Clone)]
pub struct ProviderAccountService {
    /// Privileged pool for membership/role/grant reads.
    system_pool: PgPool,
    /// Cipher and hint owner; the account secret shares the Runtime Config
    /// envelope (config_service D2/D9).
    config_service: ConfigService,
}

impl std::fmt::Debug for ProviderAccountService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderAccountService")
            .finish_non_exhaustive()
    }
}

/// Client-safe projection of one account row (D5): at most the bounded hint,
/// never ciphertext or plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAccountView {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub kind: ProviderAccountKind,
    pub family: ProviderAccountFamily,
    pub scope: ProviderAccountScope,
    pub workspace_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub base_url: Option<String>,
    pub has_secret: bool,
    pub secret_hint: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create input. `kind` and `scope` arrive as wire strings and are parsed
/// against the closed catalogs so an unknown value yields a typed code (D4).
#[derive(Clone, Default)]
pub struct CreateProviderAccount {
    pub name: String,
    pub kind: String,
    pub scope: String,
    pub workspace_id: Option<Uuid>,
    pub base_url: Option<String>,
    pub secret: Option<String>,
}

impl std::fmt::Debug for CreateProviderAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateProviderAccount")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("scope", &self.scope)
            .field("workspace_id", &self.workspace_id)
            .field("base_url", &self.base_url)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Update input: absent fields are unchanged; a blank secret is unchanged (D4).
#[derive(Clone, Default)]
pub struct UpdateProviderAccount {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub secret: Option<String>,
}

impl std::fmt::Debug for UpdateProviderAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateProviderAccount")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// The only plaintext exit (D7): consumed by the effective-config resolver's
/// account projection and handed to the task secret channel only.
#[derive(Clone)]
pub struct ResolvedAccountCredential {
    pub id: Uuid,
    pub kind: ProviderAccountKind,
    pub scope: ProviderAccountScope,
    pub base_url: Option<String>,
    pub secret: Option<String>,
}

/// Non-secret assignment-validation projection of one account row (D10):
/// what a scoped config write or preview patch needs to check family and
/// scope-at-or-above without touching the credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderAccountAssignmentTarget {
    pub kind: ProviderAccountKind,
    pub scope: ProviderAccountScope,
    pub workspace_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

impl std::fmt::Debug for ResolvedAccountCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedAccountCredential")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("scope", &self.scope)
            .field("base_url", &self.base_url)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl ProviderAccountService {
    #[must_use]
    pub fn new(system_pool: PgPool, config_service: ConfigService) -> Self {
        Self {
            system_pool,
            config_service,
        }
    }

    // -----------------------------------------------------------------------
    // Reads
    // -----------------------------------------------------------------------

    /// Accounts visible to `caller` in `org_id` (D2): every org account, the
    /// accounts of `workspace_id` when the caller can read that workspace,
    /// and the caller's own user accounts; sorted by scope then name.
    pub async fn list_visible(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        caller: Uuid,
        workspace_id: Option<Uuid>,
    ) -> Result<Vec<ProviderAccountView>, AppError> {
        let readable_workspace = match workspace_id {
            Some(ws) if self.workspace_readable(db, caller, org_id, ws).await? => Some(ws),
            _ => None,
        };
        let rows = sqlx::query_as::<_, ProviderAccount>(&format!(
            "SELECT {ACCOUNT_COLUMNS} FROM provider_accounts \
             WHERE org_id = $1 \
               AND (scope = 'org' \
                    OR (scope = 'workspace' AND workspace_id = $2) \
                    OR (scope = 'user' AND user_id = $3)) \
             ORDER BY CASE scope WHEN 'org' THEN 0 WHEN 'workspace' THEN 1 ELSE 2 END, \
                      lower(name), id"
        ))
        .bind(org_id)
        .bind(readable_workspace)
        .bind(caller)
        .fetch_all(db.conn())
        .await?;
        rows.into_iter().map(|row| self.view_of(row)).collect()
    }

    /// One visible account by id, or `NotFound` when it is absent from the
    /// org or not visible to the caller (D2).
    pub async fn get_visible(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        caller: Uuid,
        account_id: Uuid,
    ) -> Result<ProviderAccountView, AppError> {
        let row = self
            .load_visible(db, org_id, caller, account_id, false)
            .await?;
        self.view_of(row)
    }

    // -----------------------------------------------------------------------
    // Mutations
    // -----------------------------------------------------------------------

    /// Create an account after validation (D4) and row-scope authority (D3).
    pub async fn create(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        caller: Uuid,
        input: CreateProviderAccount,
    ) -> Result<ProviderAccountView, AppError> {
        let name = validate_name(&input.name)?;
        let kind: ProviderAccountKind = input.kind.parse().map_err(AppError::from_error)?;
        let scope: ProviderAccountScope = input.scope.parse().map_err(AppError::from_error)?;
        let (workspace_id, user_id) = match scope {
            ProviderAccountScope::Org => {
                if input.workspace_id.is_some() {
                    return Err(scope_forbidden("org accounts do not take a workspaceId"));
                }
                (None, None)
            }
            ProviderAccountScope::Workspace => {
                let Some(ws) = input.workspace_id else {
                    return Err(scope_forbidden("workspace accounts require a workspaceId"));
                };
                if !self.workspace_in_org(db, org_id, ws).await? {
                    return Err(scope_forbidden(
                        "workspaceId is not a workspace of this org",
                    ));
                }
                (Some(ws), None)
            }
            ProviderAccountScope::User => {
                if input.workspace_id.is_some() {
                    return Err(scope_forbidden("user accounts do not take a workspaceId"));
                }
                (None, Some(caller))
            }
        };
        let base_url = normalize_base_url(input.base_url.as_deref());
        kind.validate_base_url(base_url.as_deref())
            .map_err(AppError::from_error)?;
        let secret = validate_secret(kind, input.secret.as_deref(), base_url.as_deref())?;

        self.ensure_authority(db, org_id, caller, scope, workspace_id, user_id)
            .await?;

        let secret_enc = match secret.as_deref() {
            Some(plain) => Some(self.config_service.encrypt_task_credential(plain)?),
            None => None,
        };
        let row = sqlx::query_as::<_, ProviderAccount>(&format!(
            "INSERT INTO provider_accounts \
                (org_id, scope, workspace_id, user_id, name, kind, base_url, secret_enc, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             RETURNING {ACCOUNT_COLUMNS}"
        ))
        .bind(org_id)
        .bind(scope.as_str())
        .bind(workspace_id)
        .bind(user_id)
        .bind(&name)
        .bind(kind.as_str())
        .bind(base_url.as_deref())
        .bind(secret_enc.as_deref())
        .bind(caller)
        .fetch_one(db.conn())
        .await
        .map_err(map_write_err)?;

        audit(
            "provider_account.created",
            caller,
            org_id,
            row.id,
            scope,
            kind,
        );

        // The new account becomes the default for the agents it serves at
        // its own scope, where that scope sets none yet (chat
        // g5.chat.account-defaults). Same transaction as the insert, so no
        // launch sees the account without its default.
        let keys: Vec<&'static str> = AGENT_ASSIGNMENT_KEYS
            .iter()
            .filter(|(agent, _)| kind.supports_agent(*agent))
            .map(|(_, key)| *key)
            .collect();
        let defaulted = self
            .config_service
            .assign_account_where_unset(
                db,
                org_id,
                caller,
                own_assignment_scope(scope, workspace_id, user_id)?,
                &keys,
                row.id,
            )
            .await?;
        if !defaulted.is_empty() {
            audit(
                "provider_account.default_assigned",
                caller,
                org_id,
                row.id,
                scope,
                kind,
            );
        }
        self.view_of(row)
    }

    /// Update name, base URL, and/or secret of a visible account the caller
    /// has authority over (D3, D4). Kind and scope are immutable.
    pub async fn update(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        caller: Uuid,
        account_id: Uuid,
        input: UpdateProviderAccount,
    ) -> Result<ProviderAccountView, AppError> {
        let row = self
            .load_visible(db, org_id, caller, account_id, true)
            .await?;
        let (kind, scope) = typed_kind_scope(&row)?;
        self.ensure_authority(db, org_id, caller, scope, row.workspace_id, row.user_id)
            .await?;

        let name = match input.name.as_deref() {
            Some(candidate) => validate_name(candidate)?,
            None => row.name.clone(),
        };
        let base_url = match input.base_url.as_deref() {
            Some(candidate) => {
                let normalized = normalize_base_url(Some(candidate));
                kind.validate_base_url(normalized.as_deref())
                    .map_err(AppError::from_error)?;
                normalized
            }
            None => row.base_url.clone(),
        };
        // Blank or absent secret means unchanged — never a deletion
        // (secret_custody D1; experience: empty apply must not delete).
        let new_secret = input
            .secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let secret_enc = match new_secret {
            Some(plain) => {
                validate_secret(kind, Some(plain), base_url.as_deref())?;
                Some(self.config_service.encrypt_task_credential(plain)?)
            }
            None => row.secret_enc.clone(),
        };

        let updated = sqlx::query_as::<_, ProviderAccount>(&format!(
            "UPDATE provider_accounts \
             SET name = $3, base_url = $4, secret_enc = $5, updated_at = now() \
             WHERE id = $1 AND org_id = $2 \
             RETURNING {ACCOUNT_COLUMNS}"
        ))
        .bind(account_id)
        .bind(org_id)
        .bind(&name)
        .bind(base_url.as_deref())
        .bind(secret_enc.as_deref())
        .fetch_one(db.conn())
        .await
        .map_err(map_write_err)?;

        audit(
            "provider_account.updated",
            caller,
            org_id,
            account_id,
            scope,
            kind,
        );
        self.view_of(updated)
    }

    /// Delete a visible account the caller has authority over, unless an
    /// org, workspace, or any member's user assignment outside the account's
    /// own scope still names it (D6; chat g5.chat.account-defaults clears the
    /// own scope's assignments with the row). The row is locked `FOR UPDATE`
    /// so the guard serializes with the assignment validator's `FOR SHARE`
    /// read (config_service D13).
    pub async fn delete(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        caller: Uuid,
        account_id: Uuid,
    ) -> Result<(), AppError> {
        let row = self
            .load_visible(db, org_id, caller, account_id, false)
            .await?;
        let (kind, scope) = typed_kind_scope(&row)?;
        self.ensure_authority(db, org_id, caller, scope, row.workspace_id, row.user_id)
            .await?;

        // The account's own scope lets it go: its assignments there are
        // cleared with it (chat g5.chat.account-defaults). The scope lock is
        // taken before the row lock, in the order Settings writers use
        // (scope lock, then a share lock on the row), so the two cannot
        // deadlock; a same-scope assignment committing meanwhile is seen
        // and cleared here.
        let cleared = self
            .config_service
            .clear_account_assignments_at(
                db,
                org_id,
                caller,
                own_assignment_scope(scope, row.workspace_id, row.user_id)?,
                account_id,
            )
            .await?;
        if self
            .fetch_row(db, org_id, account_id, true)
            .await?
            .is_none()
        {
            return Err(not_found(account_id));
        }
        if !cleared.is_empty() {
            audit(
                "provider_account.default_cleared",
                caller,
                org_id,
                account_id,
                scope,
                kind,
            );
        }

        if self.is_assigned(db, org_id, account_id).await? {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: "account_assigned: a Claude Code or Codex account assignment still \
                         references this provider account; clear the assignment first"
                    .to_string(),
            }));
        }

        let deleted = sqlx::query("DELETE FROM provider_accounts WHERE id = $1 AND org_id = $2")
            .bind(account_id)
            .bind(org_id)
            .execute(db.conn())
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(not_found(account_id));
        }

        audit(
            "provider_account.deleted",
            caller,
            org_id,
            account_id,
            scope,
            kind,
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Launch resolution
    // -----------------------------------------------------------------------

    /// Resolve an account for a launch from the (org, workspace, caller)
    /// triple (D7). Any invisibility — absent, cross-org, wrong workspace, or
    /// another user's account — fails typed `account_not_visible`. A launch
    /// without an initiating user (`caller == None`) sees org accounts only.
    pub async fn resolve_for_launch(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        caller: Option<Uuid>,
        account_id: Uuid,
    ) -> Result<ResolvedAccountCredential, AppError> {
        let Some(row) = self.fetch_row(db, org_id, account_id, false).await? else {
            return Err(account_not_visible());
        };
        let (_, scope) = typed_kind_scope(&row)?;
        let visible = match scope {
            ProviderAccountScope::Org => true,
            ProviderAccountScope::Workspace => match (row.workspace_id, workspace_id, caller) {
                (Some(owner), Some(launch), Some(caller)) if owner == launch => {
                    self.workspace_readable(db, caller, org_id, owner).await?
                }
                _ => false,
            },
            ProviderAccountScope::User => caller.is_some_and(|caller| row.user_id == Some(caller)),
        };
        if !visible {
            return Err(account_not_visible());
        }
        self.credential_of(row)
    }

    /// Non-secret assignment target of an account in the org (D10), or
    /// `None` when no such row exists in the org. Needs no service instance so
    /// scoped config writes can validate an assignment key under their own
    /// advisory lock without a second `provider_accounts` reader.
    pub async fn assignment_target(
        db: &mut ScopedDb,
        org_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<ProviderAccountAssignmentTarget>, AppError> {
        // `FOR SHARE` serializes with the delete guard's `FOR UPDATE` (D6):
        // an in-flight delete blocks this read until it commits, after which
        // the row is gone and the assignment fails `account_not_visible`.
        let row = sqlx::query_as::<_, ProviderAccount>(&format!(
            "SELECT {ACCOUNT_COLUMNS} FROM provider_accounts \
             WHERE id = $1 AND org_id = $2 FOR SHARE"
        ))
        .bind(account_id)
        .bind(org_id)
        .fetch_optional(db.conn())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let (kind, scope) = typed_kind_scope(&row)?;
        Ok(Some(ProviderAccountAssignmentTarget {
            kind,
            scope,
            workspace_id: row.workspace_id,
            user_id: row.user_id,
        }))
    }

    /// The one plaintext exit: decrypt the row's secret for the caller's
    /// secret channel (D7, D10). Never logged.
    fn credential_of(&self, row: ProviderAccount) -> Result<ResolvedAccountCredential, AppError> {
        let (kind, scope) = typed_kind_scope(&row)?;
        let secret = match row.secret_enc.as_deref() {
            Some(enc) => Some(self.config_service.decrypt_task_credential(enc)?),
            None => None,
        };
        Ok(ResolvedAccountCredential {
            id: row.id,
            kind,
            scope,
            base_url: row.base_url,
            secret,
        })
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    async fn fetch_row(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        account_id: Uuid,
        for_update: bool,
    ) -> Result<Option<ProviderAccount>, AppError> {
        let lock = if for_update { " FOR UPDATE" } else { "" };
        let row = sqlx::query_as::<_, ProviderAccount>(&format!(
            "SELECT {ACCOUNT_COLUMNS} FROM provider_accounts WHERE id = $1 AND org_id = $2{lock}"
        ))
        .bind(account_id)
        .bind(org_id)
        .fetch_optional(db.conn())
        .await?;
        Ok(row)
    }

    /// Load a row the caller can see, locking it when `for_update` (D2, D3).
    async fn load_visible(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        caller: Uuid,
        account_id: Uuid,
        for_update: bool,
    ) -> Result<ProviderAccount, AppError> {
        let Some(row) = self.fetch_row(db, org_id, account_id, for_update).await? else {
            return Err(not_found(account_id));
        };
        let (_, scope) = typed_kind_scope(&row)?;
        let visible = match scope {
            ProviderAccountScope::Org => true,
            ProviderAccountScope::Workspace => match row.workspace_id {
                Some(ws) => self.workspace_readable(db, caller, org_id, ws).await?,
                None => false,
            },
            ProviderAccountScope::User => row.user_id == Some(caller),
        };
        if visible {
            Ok(row)
        } else {
            Err(not_found(account_id))
        }
    }

    async fn workspace_readable(
        &self,
        db: &mut ScopedDb,
        caller: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<bool, AppError> {
        PermissionService::user_has_workspace_scope(
            &self.system_pool,
            db,
            caller,
            org_id,
            workspace_id,
            WORKSPACE_READ_SCOPE,
        )
        .await
        .map_err(AppError::from_error)
    }

    async fn workspace_in_org(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<bool, AppError> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM workspaces \
             WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL)",
        )
        .bind(workspace_id)
        .bind(org_id)
        .fetch_one(db.conn())
        .await?;
        Ok(exists)
    }

    /// Row-scope authority against current membership/role/grant state (D3).
    async fn ensure_authority(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        caller: Uuid,
        scope: ProviderAccountScope,
        workspace_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        let (allowed, missing) = match scope {
            ProviderAccountScope::Org => (
                PermissionService::user_has_org_scope(
                    &self.system_pool,
                    caller,
                    org_id,
                    ORG_ACCOUNT_SCOPE,
                )
                .await
                .map_err(AppError::from_error)?,
                ORG_ACCOUNT_SCOPE,
            ),
            ProviderAccountScope::Workspace => {
                let allowed = match workspace_id {
                    Some(ws) => PermissionService::user_has_workspace_scope(
                        &self.system_pool,
                        db,
                        caller,
                        org_id,
                        ws,
                        WORKSPACE_WRITE_SCOPE,
                    )
                    .await
                    .map_err(AppError::from_error)?,
                    None => false,
                };
                (allowed, WORKSPACE_WRITE_SCOPE)
            }
            ProviderAccountScope::User => (user_id == Some(caller), OWNER_PSEUDO_SCOPE),
        };
        if allowed {
            if let (ProviderAccountScope::Workspace, Some(workspace_id)) = (scope, workspace_id) {
                require_current_workspace_owner(db, org_id, workspace_id, caller).await?;
            }
            Ok(())
        } else {
            Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec![missing.to_string()],
            }))
        }
    }

    /// True when an org, any workspace of the org, or any user config of the
    /// org assigns this account through either assignment key (D6). The
    /// `user_configs` tenant policy is org-scoped, so the tenant transaction
    /// sees every member's row, not only the caller's.
    async fn is_assigned(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        account_id: Uuid,
    ) -> Result<bool, AppError> {
        let needle = account_id.to_string();
        let assigned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS( \
                SELECT 1 FROM org_configs \
                WHERE org_id = $1 \
                  AND (settings_json->>$3 = $2 OR settings_json->>$4 = $2) \
                UNION ALL \
                SELECT 1 FROM workspace_configs wc \
                JOIN workspaces w ON w.id = wc.workspace_id \
                WHERE w.org_id = $1 \
                  AND (wc.settings_json->>$3 = $2 OR wc.settings_json->>$4 = $2) \
                UNION ALL \
                SELECT 1 FROM user_configs \
                WHERE org_id = $1 \
                  AND (settings_json->>$3 = $2 OR settings_json->>$4 = $2) \
             )",
        )
        .bind(org_id)
        .bind(&needle)
        .bind(CLAUDE_ACCOUNT_ID_SETTING)
        .bind(CODEX_ACCOUNT_ID_SETTING)
        .fetch_one(db.conn())
        .await?;
        Ok(assigned)
    }

    /// Project a row into the client-safe view, deriving the hint at read
    /// time from the decrypted value (D5); nothing else secret-derived.
    fn view_of(&self, row: ProviderAccount) -> Result<ProviderAccountView, AppError> {
        let (kind, scope) = typed_kind_scope(&row)?;
        let secret_hint = match row.secret_enc.as_deref() {
            Some(enc) => {
                let plain = self.config_service.decrypt_task_credential(enc)?;
                ConfigService::secret_display_hint(kind.as_str(), &plain)
            }
            None => None,
        };
        Ok(ProviderAccountView {
            id: row.id,
            org_id: row.org_id,
            name: row.name,
            kind,
            family: kind.family(),
            scope,
            workspace_id: row.workspace_id,
            user_id: row.user_id,
            base_url: row.base_url,
            has_secret: row.secret_enc.is_some(),
            secret_hint,
            created_by: row.created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn typed_kind_scope(
    row: &ProviderAccount,
) -> Result<(ProviderAccountKind, ProviderAccountScope), AppError> {
    let kind = ProviderAccountKind::parse(&row.kind).ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: format!("provider account {} has unknown kind {}", row.id, row.kind),
        })
    })?;
    let scope = ProviderAccountScope::parse(&row.scope).ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: format!(
                "provider account {} has unknown scope {}",
                row.id, row.scope
            ),
        })
    })?;
    Ok((kind, scope))
}

/// Trim and bound the name (D4): 1..=80 characters after trimming.
fn validate_name(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    let count = trimmed.chars().count();
    if count == 0 || count > PROVIDER_ACCOUNT_NAME_MAX_CHARS {
        return Err(validation_error(
            "name",
            "account_name_invalid",
            format!("name must be 1 to {PROVIDER_ACCOUNT_NAME_MAX_CHARS} characters"),
        ));
    }
    Ok(trimmed.to_string())
}

fn normalize_base_url(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Secret presence and shape must match the kind (D4, secret_custody D10):
/// the catalog owns the rule (`ProviderAccountKind::validate_secret`), and
/// the shape applies only on the provider's own host.
fn validate_secret(
    kind: ProviderAccountKind,
    raw: Option<&str>,
    base_url: Option<&str>,
) -> Result<Option<String>, AppError> {
    kind.validate_secret(raw, base_url)
        .map_err(AppError::from_error)
}

fn scope_forbidden(error: &str) -> AppError {
    validation_error("scope", "account_scope_forbidden", error.to_string())
}

fn account_not_visible() -> AppError {
    account_not_visible_error()
}

/// Typed `account_not_visible` failure shared with the config service's launch
/// projection and assignment validation (D7, D10; config_service D12, D13).
pub(crate) fn account_not_visible_error() -> AppError {
    validation_error(
        "accountId",
        "account_not_visible",
        "the provider account does not exist or is not visible from this org, workspace, and user"
            .to_string(),
    )
}

/// Typed provider-account validation failure (`field`, `code`, `error`),
/// shared with the config service's assignment-key validation.
pub(crate) fn account_validation_error(field: &str, code: &str, error: String) -> AppError {
    validation_error(field, code, error)
}

fn validation_error(field: &str, code: &str, error: String) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({
            "field": field,
            "code": code,
            "error": error,
        }),
    })
}

fn not_found(account_id: Uuid) -> AppError {
    AppError::from_error(G5Error::NotFound {
        resource: format!("provider account {account_id}"),
    })
}

/// Map the per-scope unique name index to a typed 409 (D4); everything else
/// follows the standard `sqlx::Error` conversion.
fn map_write_err(err: sqlx::Error) -> AppError {
    match &err {
        sqlx::Error::Database(db_err)
            if db_err.code().as_deref() == Some("23505")
                && db_err.constraint() == Some("provider_accounts_scope_name_key") =>
        {
            AppError::from_error(G5Error::Conflict {
                reason: "account_name_conflict: an account with this name already exists at \
                         this scope"
                    .to_string(),
            })
        }
        _ => err.into(),
    }
}

/// Each agent with the assignment key that names its default account
/// (external_agent_provider_contract D14).
const AGENT_ASSIGNMENT_KEYS: &[(ChatAgentProvider, &str)] = &[
    (ChatAgentProvider::ClaudeCode, CLAUDE_ACCOUNT_ID_SETTING),
    (ChatAgentProvider::Codex, CODEX_ACCOUNT_ID_SETTING),
];

/// The config scope whose own assignments an account fills on creation and
/// releases on removal: the scope the account belongs to (chat
/// g5.chat.account-defaults).
fn own_assignment_scope(
    scope: ProviderAccountScope,
    workspace_id: Option<Uuid>,
    user_id: Option<Uuid>,
) -> Result<AssignmentWriteScope, AppError> {
    match (scope, workspace_id, user_id) {
        (ProviderAccountScope::Org, _, _) => Ok(AssignmentWriteScope::Org),
        (ProviderAccountScope::Workspace, Some(workspace_id), _) => {
            Ok(AssignmentWriteScope::Workspace(workspace_id))
        }
        (ProviderAccountScope::User, _, Some(user_id)) => Ok(AssignmentWriteScope::User(user_id)),
        _ => Err(AppError::from_error(G5Error::Internal {
            message: "provider account row lacks its scope owner".to_string(),
        })),
    }
}

/// One `g5_audit` event per mutation (D8, observability_audit D3): actor,
/// org, account id, scope, kind — never a hint or secret.
fn audit(
    event: &'static str,
    actor_user_id: Uuid,
    org_id: Uuid,
    account_id: Uuid,
    scope: ProviderAccountScope,
    kind: ProviderAccountKind,
) {
    tracing::info!(
        target: "g5_audit",
        event,
        actor_user_id = %actor_user_id,
        org_id = %org_id,
        account_id = %account_id,
        scope = scope.as_str(),
        kind = kind.as_str(),
        "provider account mutation"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_of(error: &AppError) -> String {
        match &error.inner {
            G5Error::ValidationError { details } => details["code"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_default(),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn name_is_trimmed_and_bounded() {
        assert_eq!(validate_name("  Team key ").unwrap(), "Team key");
        assert_eq!(
            code_of(&validate_name("   ").unwrap_err()),
            "account_name_invalid"
        );
        let long = "n".repeat(PROVIDER_ACCOUNT_NAME_MAX_CHARS + 1);
        assert_eq!(
            code_of(&validate_name(&long).unwrap_err()),
            "account_name_invalid"
        );
        let max = "n".repeat(PROVIDER_ACCOUNT_NAME_MAX_CHARS);
        assert_eq!(validate_name(&max).unwrap(), max);
    }

    #[test]
    fn secret_presence_follows_kind() {
        assert_eq!(
            code_of(
                &validate_secret(ProviderAccountKind::ClaudeSubscription, Some("  "), None)
                    .unwrap_err()
            ),
            "account_secret_invalid"
        );
        assert_eq!(
            code_of(
                &validate_secret(ProviderAccountKind::ChatgptSubscription, Some("x"), None)
                    .unwrap_err()
            ),
            "account_secret_invalid"
        );
        assert_eq!(
            validate_secret(ProviderAccountKind::ChatgptSubscription, None, None).unwrap(),
            None
        );
        assert_eq!(
            validate_secret(
                ProviderAccountKind::AnthropicApiKey,
                Some(" sk-ant-api03-0123456789abcdefghijklmnop\n"),
                None
            )
            .unwrap(),
            Some("sk-ant-api03-0123456789abcdefghijklmnop".to_string())
        );
        // Shape failures from the catalog keep the typed code.
        assert_eq!(
            code_of(
                &validate_secret(
                    ProviderAccountKind::ClaudeSubscription,
                    Some("sk-ant-api03-0123456789abcdefghijklmnop"),
                    None
                )
                .unwrap_err()
            ),
            "account_secret_invalid"
        );
    }

    #[test]
    fn secret_bearing_inputs_redact_in_debug() {
        let create = CreateProviderAccount {
            name: "n".into(),
            kind: "anthropic_api_key".into(),
            scope: "org".into(),
            workspace_id: None,
            base_url: None,
            secret: Some("sk-ant-very-secret-value".into()),
        };
        let rendered = format!("{create:?}");
        assert!(!rendered.contains("sk-ant-very-secret-value"));
        assert!(rendered.contains("<redacted>"));
        let resolved = ResolvedAccountCredential {
            id: Uuid::nil(),
            kind: ProviderAccountKind::AnthropicApiKey,
            scope: ProviderAccountScope::Org,
            base_url: None,
            secret: Some("sk-ant-very-secret-value".into()),
        };
        assert!(!format!("{resolved:?}").contains("sk-ant-very-secret-value"));
    }
}
