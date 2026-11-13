//! Org service — business logic for organization management.
//!
//! Implements org CRUD, the multi-step creation cascade, member management,
//! invitation lifecycle, and repo listing per `API_AND_STORAGE.md` §3.12.
//!
//! The stateless `OrgService` exposes async methods that sit between route
//! handlers and the database layer. Callers own transaction boundaries and
//! pass the active `ScopedDb` to methods that need atomic multi-step writes.

use chrono::{DateTime, Duration, Utc};
use g5_authz::{AuthzService, ScopedDb};
use serde::{Deserialize, Serialize};
use sqlx::{Acquire, PgConnection, PgPool};
use uuid::Uuid;

use crate::models::{Org, OrgInvitation, Repo, Workspace};
use crate::services::email::{
    DeliveryGate, DeliveryStatus, Email, EmailError, EmailRequest, Message, OrgInvitationEmail,
    PurposeKind, Sender, Validity,
};
use crate::services::permission_service::PermissionService;
use g5_contracts::types::worker_host::validate_base_image_ref;
use g5_contracts::{
    error::G5Error,
    types::ids::{OrgId, UserId},
};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// DTO structs
// ---------------------------------------------------------------------------

/// A G5-hosted repository's recorded URL: its organization's and its own
/// slugs, never a disk path (`g5.repo.hosted-url`). Repository storage
/// is named by ids and never reads it.
#[must_use]
pub fn local_repo_git_url(org_slug: &str, repo_slug: &str) -> String {
    format!("g5-local://{org_slug}/{repo_slug}")
}

/// Org with the requesting user's role in that org.
///
/// Returned by `list_orgs` — combines data from `orgs` and `org_memberships`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrgWithRole {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// Request body for creating a new org.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    /// Auto-generated from `name` if omitted.
    pub slug: Option<String>,
    /// Only platform-operator bootstrap creates the operator organization,
    /// whose slug is always [`OPERATOR_ORG_SLUG`].
    #[serde(default)]
    pub kind: OrgKind,
}

/// What an organization is for (`g5.organization.platform-operator.identity`):
/// an ordinary tenant, or the one organization whose memberships are the
/// platform-operator roster. The kind, never the slug, confers platform
/// authority.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrgKind {
    #[default]
    Tenant,
    Operator,
}

impl OrgKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::Operator => "operator",
        }
    }
}

/// The operator organization's fixed, well-known slug.
pub const OPERATOR_ORG_SLUG: &str = "operator";

/// Slugs no tenant organization may take: the operator slug, its possible
/// `admin` alias, `api`, the frontend's top-level route segments
/// (`frontend/src/lib/shellRoutes.ts` `STATIC_ROUTES`; keep them aligned) and
/// the retired `create-org` page, so old links never resolve to a tenant.
pub const RESERVED_ORG_SLUGS: &[&str] = &[
    OPERATOR_ORG_SLUG,
    "admin",
    "api",
    "login",
    "landing",
    "account",
    "cli",
    "forbidden",
    "settings",
    "projects",
    "no-organization",
    "create-org",
];

/// Response from creating a new org, including the provisioned base workspace
/// and its docs + code repos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrgResponse {
    pub org: Org,
    pub base_workspace: Workspace,
    pub docs_repo: Repo,
    /// The Base workspace's initial G5-managed local code repo.
    pub code_repo: Repo,
}

/// Request body for updating an org's name and/or slug.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrgRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
}

/// Request body for inviting a user to an org by email.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteMemberRequest {
    pub email: String,
    /// One of `"admin"`, `"member"`, `"viewer"`. `"owner"` is NOT allowed.
    pub role: String,
}

/// Response from inviting a member — either an immediate membership (user
/// exists) or a pending invitation (user does not exist).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteMemberResponse {
    pub membership: Option<OrgMemberDTO>,
    pub invitation: Option<OrgInvitation>,
}

/// DTO combining user profile information with their org membership role.
///
/// Returned by `list_members`, `accept_invitation`, and other member-related
/// methods.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrgMemberDTO {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub joined_at: DateTime<Utc>,
    pub deactivated_at: Option<DateTime<Utc>>,
    pub activity_revision: i64,
    pub shutdown_pending: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MemberShutdownWorkspace {
    pub workspace_id: Uuid,
    pub name: String,
    pub state: String,
    /// The VM's last recorded lifecycle problem while it is not yet stopped,
    /// such as an unreachable host (`g5.lifecycle.stop.by-user`).
    pub blocker: Option<String>,
}

/// The execution hold a stop-by-user request set or joined.
#[derive(Debug, Clone, Copy)]
pub struct MemberExecutionHold {
    pub hold_id: Uuid,
    /// An earlier request's hold (or a deactivation's) was still pending.
    pub joined: bool,
}

/// A complete membership transition, including any personal-workspace retirement.
#[derive(Debug, Clone)]
pub enum MemberChange {
    Role(String),
    Activity {
        active: bool,
        expected_revision: i64,
    },
    Remove,
}

#[derive(Debug)]
pub struct MemberChangeResult {
    pub member: Option<OrgMemberDTO>,
    pub retired: Option<RetiredPersonalWorkspace>,
}

/// Durable retirement recorded by a membership transition; runtime effects
/// may use these identifiers only after the transaction commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetiredPersonalWorkspace {
    pub workspace_id: Uuid,
    pub operation_id: Uuid,
}

/// Retirement owned by a later context, installed by the composition root.
/// It must share the membership transaction and perform no external effects.
#[async_trait::async_trait]
pub trait MembershipRetirement: Send + Sync {
    async fn retire_personal_workspace(
        &self,
        conn: &mut PgConnection,
        org_id: Uuid,
        user_id: Uuid,
        actor_id: Uuid,
    ) -> Result<Option<RetiredPersonalWorkspace>, G5Error>;
}

// ---------------------------------------------------------------------------
// OrgService
// ---------------------------------------------------------------------------

/// Result of resolving slugs to UUIDs.
pub struct ResolveResult {
    pub org_id: Uuid,
    pub repo_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    /// The workspace's current name: what the caller named it, or the current
    /// name when a former name resolved (`g5.workspace.former-names`).
    pub workspace_name: Option<String>,
}

/// Service for organization management business logic.
///
/// Stateless service — all methods receive a `&PgPool` parameter.
///
/// `create_org` receives the **system pool** (non-RLS) because the org
/// does not exist yet. Other read methods receive whichever pool the
/// caller has available (typically `AppState::db`).
/// One invitation this login accepted (`g5.organization.invitation-authority`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedInvitation {
    pub invitation_id: Uuid,
    pub org_id: Uuid,
    pub role: String,
}

#[derive(Default)]
pub struct OrgService {
    /// Invitation emails (`g5.organization.invitation-email`); disabled
    /// unless the composition root supplies a configured handle.
    email: Email,
    /// What later contexts add to a new organization inside its creation
    /// transaction (the native issue domain, issues
    /// `g5.issue.native.domain`); installed by the composition root.
    created_hooks: Vec<Arc<dyn OrganizationCreated>>,
    /// Required for transitions that retire a personal workspace. Services
    /// constructed only for organization creation may leave it unconfigured.
    membership_retirement: Option<Arc<dyn MembershipRetirement>>,
}

/// A new organization as its creation hooks see it.
#[derive(Debug, Clone)]
pub struct CreatedOrganization {
    pub org_id: Uuid,
    pub owner_user_id: Uuid,
    pub slug: String,
    pub kind: OrgKind,
}

/// Work a later context owns for every new organization, run inside the
/// creation transaction after the owner's membership and capability exist;
/// an error rolls the creation back. Installed in `services/mod.rs`, so the
/// Identity context never names the owning context.
#[async_trait::async_trait]
pub trait OrganizationCreated: Send + Sync {
    async fn organization_created(
        &self,
        db: &mut ScopedDb,
        org: &CreatedOrganization,
    ) -> Result<(), G5Error>;
}

/// Invitation emails come from `invitations@<sending domain>`
/// (`g5.email.sender`).
const INVITATION_SENDER: Sender = Sender::new("invitations");

const MEMBERSHIP_RUNTIME_GUARD_CONSTRAINTS: &[&str] = &[
    "workspaces_owner_same_org_fk",
    "workspace_ownership_transfers_from_fk",
    "workspace_ownership_transfers_to_fk",
    "workspace_ownership_transfers_requested_by_fk",
    "workspace_forks_requester_fk",
    "workspace_forks_owner_fk",
    "provider_requests_response_author_same_org_fk",
];

async fn reject_active_workspace_owner_change(
    conn: &mut PgConnection,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<(), G5Error> {
    let workspace_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces \
         WHERE org_id = $1 AND active_owner_user_id = $2 \
         ORDER BY id",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_all(conn)
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to check active workspace ownership: {error}"),
    })?;

    if workspace_ids.is_empty() {
        return Ok(());
    }

    let blockers = workspace_ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Err(G5Error::Conflict {
        reason: format!(
            "Member owns active workspaces [{blockers}]; transfer or delete those workspaces \
             before removing the member or changing their role to viewer"
        ),
    })
}

fn map_org_mutation_error(error: sqlx::Error, action: &str) -> G5Error {
    if g5_contracts::error::sqlx_error_is_retryable(&error) {
        return error.into();
    }
    let guarded_reference = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::constraint)
        .is_some_and(|constraint| MEMBERSHIP_RUNTIME_GUARD_CONSTRAINTS.contains(&constraint));

    if guarded_reference {
        return G5Error::Conflict {
            reason: format!(
                "Cannot {action}: the member is referenced by active workspace ownership or a \
                 durable runtime operation; transfer/delete the workspace or finish the operation first"
            ),
        };
    }

    G5Error::Internal {
        message: format!("Failed to {action}: {error}"),
    }
}

impl OrgService {
    /// Create a new (stateless) `OrgService`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Use `email` for invitation emails.
    #[must_use]
    pub fn with_email(mut self, email: Email) -> Self {
        self.email = email;
        self
    }

    /// Install the owner of personal retirement for membership transitions.
    #[must_use]
    pub fn with_membership_retirement(mut self, owner: Arc<dyn MembershipRetirement>) -> Self {
        self.membership_retirement = Some(owner);
        self
    }

    /// Run `hook` for every organization this service creates.
    #[must_use]
    pub fn with_created_hook(mut self, hook: Arc<dyn OrganizationCreated>) -> Self {
        self.created_hooks.push(hook);
        self
    }

    /// Run every hook in `hooks` for every organization this service creates.
    #[must_use]
    pub fn with_created_hooks(mut self, hooks: Vec<Arc<dyn OrganizationCreated>>) -> Self {
        self.created_hooks.extend(hooks);
        self
    }

    // -----------------------------------------------------------------------
    // Org CRUD
    // -----------------------------------------------------------------------

    /// List all orgs the specified user belongs to, including the user's role.
    ///
    /// Returns orgs ordered by `orgs.name ASC`.
    pub async fn list_orgs(
        &self,
        system_pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<OrgWithRole>, G5Error> {
        let orgs = sqlx::query_as!(
            OrgWithRole,
            r#"
            SELECT o.id, o.name, o.slug, m.role, o.created_at
            FROM orgs o
            JOIN org_memberships m ON o.id = m.org_id
            WHERE m.user_id = $1 AND m.deactivated_at IS NULL
            ORDER BY o.name ASC
            "#,
            user_id
        )
        .fetch_all(system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list orgs: {e}"),
        })?;

        tracing::info!(user_id = %user_id, count = orgs.len(), "Listed orgs for user");
        Ok(orgs)
    }

    /// Create a new org with the full creation cascade in a single transaction.
    ///
    /// Steps (all within one transaction):
    /// 1. INSERT org
    /// 2. INSERT org_membership (current user as Admin)
    /// 3. Provision docs repo
    /// 4. Create base workspace
    /// 5. Create workspace branch for docs repo
    /// 6. Create workspace settings with defaults
    ///
    /// Returns the created org, base workspace, and docs repo.
    pub async fn create_org(
        &self,
        authz: &dyn AuthzService,
        db: &mut ScopedDb,
        user_id: Uuid,
        req: CreateOrgRequest,
    ) -> Result<CreateOrgResponse, G5Error> {
        // Validate name
        let name = req.name.trim().to_string();
        if name.is_empty() || name.len() > 255 {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "name",
                    "error": "Name must be between 1 and 255 characters"
                }),
            });
        }

        // Generate or validate slug. Reserved slugs are refused whether the
        // caller chose them or they were generated from the name.
        let slug = match req.kind {
            OrgKind::Operator => OPERATOR_ORG_SLUG.to_string(),
            OrgKind::Tenant => {
                let slug = match req.slug {
                    Some(s) if !s.is_empty() => {
                        validate_slug(&s)?;
                        s
                    }
                    _ => generate_slug(&name),
                };
                reject_reserved_slug(&slug)?;
                slug
            }
        };

        let default_image_policy = new_org_image_policy(
            std::env::var("G5_NEW_ORG_DEFAULT_IMAGE_TRACK")
                .ok()
                .as_deref(),
            std::env::var("G5_PROMOTED_BASE_IMAGES").ok().as_deref(),
        )?;

        // Step 1: INSERT org
        let org_id = Uuid::now_v7();
        let now = Utc::now();
        let org = sqlx::query_as!(
            Org,
            r#"
            INSERT INTO orgs (id, name, slug, created_at, kind)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, name, slug, created_at
            "#,
            org_id,
            name,
            slug,
            now,
            req.kind.as_str()
        )
        .fetch_one(db.conn())
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                G5Error::Conflict {
                    reason: "Org slug already exists".to_string(),
                }
            } else {
                G5Error::Internal {
                    message: format!("Failed to insert org: {e}"),
                }
            }
        })?;

        if let Some((track, image_ref)) = default_image_policy {
            sqlx::query(
                "UPDATE orgs SET default_base_image_track = $2, default_base_image_ref = $3 WHERE id = $1",
            )
            .bind(org.id)
            .bind(track)
            .bind(image_ref)
            .execute(db.conn())
            .await?;
        }

        // Step 2: INSERT the initial Admin membership (also owns Base)
        sqlx::query!(
            r#"
            INSERT INTO org_memberships (org_id, user_id, role, created_at)
            VALUES ($1, $2, 'admin', $3)
            "#,
            org.id,
            user_id,
            now
        )
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to insert Admin membership: {e}"),
        })?;

        // Step 2b: Mint and activate the bounded server-internal capability
        // after staging the initial Admin. This preserves RLS for tenant rows while
        // keeping the full durable cascade on this one transaction connection.
        let capability = authz
            .mint_service_db_capability(
                UserId::from_uuid(user_id),
                OrgId::from_uuid(org.id),
                &["workspaces:write".to_string()],
            )
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to mint org creation session: {e}"),
            })?;

        sqlx::query("SELECT authz.set_ctx($1)")
            .bind(capability.session_id.into_uuid())
            .execute(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to activate session context for org creation: {e}"),
            })?;

        // Step 2c: What later contexts add to every new organization (the
        // native issue domain, issues g5.issue.native.domain), inside this
        // transaction.
        let created = CreatedOrganization {
            org_id: org.id,
            owner_user_id: user_id,
            slug: slug.clone(),
            kind: req.kind,
        };
        for hook in &self.created_hooks {
            hook.organization_created(db, &created).await?;
        }

        // Step 3: Provision docs repo
        let docs_repo_id = Uuid::now_v7();
        let git_url = local_repo_git_url(&org.slug, "docs");
        let docs_repo = sqlx::query_as::<_, Repo>(
            r#"
            INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
            VALUES ($1, $2, 'docs', 'docs', $3, 'main', $4)
            RETURNING id, org_id, name, slug, git_url, default_branch,
                      provider, provider_repo_id, provider_full_name, created_at
            "#,
        )
        .bind(docs_repo_id)
        .bind(org.id)
        .bind(&git_url)
        .bind(now)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to insert docs repo: {e}"),
        })?;

        // Step 4: Create base workspace
        let base_workspace_id = Uuid::now_v7();
        let base_workspace = sqlx::query_as::<_, Workspace>(
            r#"
            INSERT INTO workspaces (
                id, org_id, name, is_base,
                docs_repo_id, push_mode,
                owner_user_id, created_by, created_at, updated_at
            )
            VALUES ($1, $2, 'Base', true,
                    $3, 'manual',
                    $4, $4, $5, $5)
            RETURNING id, org_id, name, is_base, description,
                      cloned_from_id, docs_repo_id, owner_user_id,
                      lifecycle_generation, push_mode,
                      mutation_policy,
                      mutation_policy_transition,
                      mutation_policy_updated_by, mutation_policy_updated_at,
                      merge_target_workspace_id, personal_user_id,
                      created_by, created_at, updated_at
            "#,
        )
        .bind(base_workspace_id)
        .bind(org.id)
        .bind(docs_repo.id)
        .bind(user_id)
        .bind(now)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to insert base workspace: {e}"),
        })?;

        // Step 6: Provision the Base code repo (G5-managed local; mirrors the
        // docs repo above so every new org's Base is born docs+code).
        let code_repo_id = Uuid::now_v7();
        let code_git_url = local_repo_git_url(&org.slug, "code");
        let code_repo = sqlx::query_as::<_, Repo>(
            r#"
            INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
            VALUES ($1, $2, 'code', 'code', $3, 'main', $4)
            RETURNING id, org_id, name, slug, git_url, default_branch,
                      provider, provider_repo_id, provider_full_name, created_at
            "#,
        )
        .bind(code_repo_id)
        .bind(org.id)
        .bind(&code_git_url)
        .bind(now)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to insert code repo: {e}"),
        })?;

        // Store structural membership; Git initialization owns materialization.
        sqlx::query(
            r#"
            INSERT INTO workspace_repos (
                org_id, workspace_id, repo_id, root_path, role
            ) VALUES
                ($1, $2, $3, 'docs', 'docs'),
                ($1, $2, $4, 'code', 'code')
            "#,
        )
        .bind(org.id)
        .bind(base_workspace.id)
        .bind(docs_repo.id)
        .bind(code_repo.id)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to insert Base repository mapping: {e}"),
        })?;

        // Step 8: Create workspace settings with defaults; Base, like every new
        // workspace, starts with the org's default idle timeout.
        let idle_timeout_secs =
            super::org_admin_settings::default_idle_timeout_secs(db.conn(), org.id).await?;
        sqlx::query(
            r#"
            INSERT INTO workspace_settings (org_id, workspace_id, idle_timeout_secs)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(org.id)
        .bind(base_workspace.id)
        .bind(idle_timeout_secs)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to insert workspace settings: {e}"),
        })?;

        // The caller owns the metadata transaction and clears its RLS context
        // before commit.

        tracing::info!(org_id = %org.id, user_id = %user_id, "Created org");

        Ok(CreateOrgResponse {
            org,
            base_workspace,
            docs_repo,
            code_repo,
        })
    }
    /// Get an org by ID.
    ///
    /// Returns `G5Error::NotFound` if the org does not exist.
    pub async fn get_org<'e>(
        &self,
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
        org_id: Uuid,
    ) -> Result<Org, G5Error> {
        sqlx::query_as!(
            Org,
            r#"
            SELECT id, name, slug, created_at
            FROM orgs
            WHERE id = $1
            "#,
            org_id
        )
        .fetch_optional(executor)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to get org: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("org {org_id}"),
        })
    }

    /// Update an org's name and/or slug.
    ///
    /// Returns the updated org. If both fields are `None`, returns the org
    /// unchanged.
    pub async fn update_org(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        req: UpdateOrgRequest,
    ) -> Result<Org, G5Error> {
        PermissionService::require_org_admin_on_conn(db.conn(), actor_id, org_id).await?;
        // If nothing to update, just return the current org
        if req.name.is_none() && req.slug.is_none() {
            return self.get_org(db.conn(), org_id).await;
        }

        // Lock the row first: a concurrent rename committing between this
        // read and the UPDATE would otherwise have its slug written back
        // while its repository URLs (`g5.repo.hosted-url`) keep the new one.
        sqlx::query("SELECT 1 FROM orgs WHERE id = $1 FOR UPDATE")
            .bind(org_id)
            .execute(db.conn())
            .await?;
        // Fetch existing org to fill in unchanged fields
        let existing = self.get_org(db.conn(), org_id).await?;
        let slug_changes = req.slug.as_ref().is_some_and(|slug| *slug != existing.slug);

        let new_name = req.name.unwrap_or(existing.name);
        let new_slug = req.slug.unwrap_or(existing.slug);

        if slug_changes {
            let kind: String = sqlx::query_scalar("SELECT kind FROM orgs WHERE id = $1")
                .bind(org_id)
                .fetch_one(db.conn())
                .await?;
            if kind == OrgKind::Operator.as_str() {
                return Err(G5Error::Conflict {
                    reason: "The operator organization's slug cannot change".to_string(),
                });
            }
            validate_slug(&new_slug)?;
            reject_reserved_slug(&new_slug)?;
        }

        let org = sqlx::query_as!(
            Org,
            r#"
            UPDATE orgs
            SET name = $2, slug = $3
            WHERE id = $1
            RETURNING id, name, slug, created_at
            "#,
            org_id,
            new_name,
            new_slug
        )
        .fetch_one(db.conn())
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                G5Error::Conflict {
                    reason: "Org slug already exists".to_string(),
                }
            } else {
                map_org_mutation_error(e, "update org")
            }
        })?;

        if slug_changes {
            // A G5-hosted repository's URL names its organization's slug
            // (`g5.repo.hosted-url`); it follows the rename in the same
            // transaction. Storage is keyed by ids and never reads it.
            sqlx::query(
                "UPDATE repos
                 SET git_url = 'g5-local://' || $2 || '/' || slug
                 WHERE org_id = $1 AND provider IS NULL AND git_url LIKE 'g5-local://%'",
            )
            .bind(org_id)
            .bind(&org.slug)
            .execute(db.conn())
            .await
            .map_err(|e| map_org_mutation_error(e, "update repository URLs"))?;
        }

        tracing::info!(org_id = %org.id, "Updated org");
        Ok(org)
    }

    // -----------------------------------------------------------------------
    // Member Management
    // -----------------------------------------------------------------------

    /// Serialize membership transitions before current-authority admission.
    /// Other authority writers can contend for these rows; callers retry
    /// the entire transaction on a typed deadlock/serialization failure.
    pub async fn lock_membership_transition(
        conn: &mut PgConnection,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), G5Error> {
        // A real row version makes a stale stronger-isolation snapshot fail
        // with a retryable serialization error after another transition wins.
        // The value is unchanged: refusal must not revoke existing authority.
        sqlx::query(
            "UPDATE orgs SET access_generation = access_generation WHERE id = $1 RETURNING id",
        )
        .bind(org_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "organization".into(),
        })?;
        sqlx::query(
            "SELECT user_id FROM org_memberships WHERE org_id = $1
                     AND user_id = ANY($2) ORDER BY user_id FOR UPDATE",
        )
        .bind(org_id)
        .bind(vec![actor_id, user_id])
        .fetch_all(conn)
        .await?;
        Ok(())
    }

    /// The membership owner coordinates the whole durable change. Retirement
    /// belongs to PersonalWorkspace and shares this transaction. No external
    /// effects may run until the caller commits. Errors roll back the operation.
    pub async fn change_member(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
        change: MemberChange,
    ) -> Result<MemberChangeResult, G5Error> {
        // SQLx rolls this savepoint back on error or cancellation. A caller
        // committing its outer transaction cannot publish partial retirement.
        let mut transition = db.tx().begin().await?;
        let result = self
            .change_member_on_conn(&mut transition, org_id, actor_id, user_id, change)
            .await?;
        transition.commit().await?;
        Ok(result)
    }

    async fn change_member_on_conn(
        &self,
        conn: &mut PgConnection,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
        change: MemberChange,
    ) -> Result<MemberChangeResult, G5Error> {
        Self::lock_membership_transition(&mut *conn, org_id, actor_id, user_id).await?;
        PermissionService::require_org_admin_on_conn(&mut *conn, actor_id, org_id).await?;
        // Activity management retains its stricter active-Admin and self-action
        // rules; additive org:admin alone does not admit that operation.
        if matches!(&change, MemberChange::Activity { .. }) {
            if actor_id == user_id {
                return Err(G5Error::Conflict {
                    reason: "Cannot change your own membership activity".into(),
                });
            }
            let active_admin: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM org_memberships WHERE org_id=$1
                 AND user_id=$2 AND role='admin' AND deactivated_at IS NULL)",
            )
            .bind(org_id)
            .bind(actor_id)
            .fetch_one(&mut *conn)
            .await?;
            if !active_admin {
                return Err(G5Error::Forbidden {
                    missing_scopes: vec![],
                });
            }
        }
        Self::require_admin_continuity(conn, org_id, user_id, &change).await?;
        let retire = matches!(&change, MemberChange::Remove)
            || matches!(&change, MemberChange::Role(role) if role == "viewer");
        let retired = if retire {
            self.membership_retirement
                .as_ref()
                .ok_or_else(|| G5Error::Internal {
                    message: "Membership retirement owner is not configured".into(),
                })?
                .retire_personal_workspace(&mut *conn, org_id, user_id, actor_id)
                .await?
        } else {
            None
        };
        let member = match change {
            MemberChange::Role(role) => Some(
                self.update_member_role(conn, org_id, actor_id, user_id, role)
                    .await?,
            ),
            MemberChange::Activity {
                active,
                expected_revision,
            } => Some(
                self.set_member_active(conn, org_id, actor_id, user_id, active, expected_revision)
                    .await?,
            ),
            MemberChange::Remove => {
                self.remove_member(conn, org_id, actor_id, user_id).await?;
                None
            }
        };
        Ok(MemberChangeResult { member, retired })
    }

    /// Decide continuity under the organization lock, before any membership
    /// or personal-workspace mutation. Privileged manual SQL is outside this
    /// application boundary; only live Admin memberships satisfy the rule.
    async fn require_admin_continuity(
        conn: &mut PgConnection,
        org_id: Uuid,
        user_id: Uuid,
        change: &MemberChange,
    ) -> Result<(), G5Error> {
        if let MemberChange::Role(role) = change {
            if !["admin", "member", "viewer"].contains(&role.as_str()) {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "role",
                        "error": "Role must be one of: admin, member, viewer"
                    }),
                });
            }
        }
        let reduces = matches!(change, MemberChange::Remove)
            || matches!(change, MemberChange::Role(role) if role != "admin")
            || matches!(change, MemberChange::Activity { active: false, .. });
        if !reduces {
            return Ok(());
        }
        let last_admin: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM orgs o JOIN org_memberships target ON target.org_id=o.id
                WHERE o.id=$1 AND o.kind='tenant' AND target.user_id=$2
                  AND target.role='admin' AND target.deactivated_at IS NULL
                  AND NOT EXISTS (
                    SELECT 1 FROM org_memberships other WHERE other.org_id=o.id
                    AND other.user_id<>$2 AND other.role='admin'
                    AND other.deactivated_at IS NULL))",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_one(conn)
        .await?;
        if last_admin {
            return Err(G5Error::Conflict {
                reason: "A tenant organization must retain at least one active Admin".into(),
            });
        }
        Ok(())
    }

    /// Activity changes retain membership and ownership. Cleanup is durable and
    /// deliberately survives Reactivate; lifecycle reconciliation owns its end.
    async fn set_member_active(
        &self,
        conn: &mut PgConnection,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
        active: bool,
        expected_revision: i64,
    ) -> Result<OrgMemberDTO, G5Error> {
        if actor_id == user_id {
            return Err(G5Error::Conflict {
                reason: "Cannot change your own membership activity".into(),
            });
        }
        let rows = sqlx::query_as::<_, (Uuid, String, Option<DateTime<Utc>>, i64)>(
            "SELECT user_id, role, deactivated_at, activity_revision
             FROM org_memberships WHERE org_id = $1 AND user_id = ANY($2)
             ORDER BY user_id FOR UPDATE",
        )
        .bind(org_id)
        .bind(vec![actor_id, user_id])
        .fetch_all(&mut *conn)
        .await?;
        if !rows.iter().any(|(id, role, inactive, _)| {
            *id == actor_id && inactive.is_none() && matches!(role.as_str(), "admin")
        }) {
            return Err(G5Error::Forbidden {
                missing_scopes: vec![],
            });
        }
        PermissionService::require_org_admin_on_conn(&mut *conn, actor_id, org_id).await?;
        let (_, _, inactive, revision) =
            rows.iter()
                .find(|row| row.0 == user_id)
                .ok_or_else(|| G5Error::NotFound {
                    resource: "organization membership".into(),
                })?;
        if active != inactive.is_none() {
            if *revision != expected_revision {
                return Err(G5Error::Conflict {
                    reason: "Membership changed; refresh before trying again".into(),
                });
            }
            sqlx::query(
                "UPDATE org_memberships SET deactivated_at = CASE WHEN $3 THEN NULL ELSE now() END,
                 activity_revision = activity_revision + 1,
                 deactivation_cleanup_id = CASE WHEN $3 THEN deactivation_cleanup_id
                    ELSE COALESCE(deactivation_cleanup_id, $4) END
                 WHERE org_id = $1 AND user_id = $2",
            )
            .bind(org_id)
            .bind(user_id)
            .bind(active)
            .bind(Uuid::new_v4())
            .execute(&mut *conn)
            .await?;
        }
        sqlx::query_as::<_, OrgMemberDTO>(
            "SELECT u.id AS user_id, u.email, u.display_name, u.avatar_url, m.role,
                    m.created_at AS joined_at, m.deactivated_at, m.activity_revision,
                    m.deactivation_cleanup_id IS NOT NULL AS shutdown_pending
             FROM org_memberships m JOIN users u ON u.id = m.user_id
             WHERE m.org_id = $1 AND m.user_id = $2",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(Into::into)
    }

    /// Stop every workspace `user_id` owns in the organization
    /// (`g5.lifecycle.stop.by-user`): commit the membership's execution hold,
    /// or join the pending one. Access is unchanged; the lifecycle reconciler
    /// stops each workspace and clears the hold once all have settled.
    ///
    /// The user themself or an active organization administrator may ask. A
    /// workspace agent asks as its owner, so it passes `actor_id == user_id`.
    pub async fn stop_member_workspaces(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
    ) -> Result<MemberExecutionHold, G5Error> {
        let rows = sqlx::query_as::<_, (Uuid, String, Option<DateTime<Utc>>)>(
            "SELECT user_id, role, deactivated_at
             FROM org_memberships WHERE org_id = $1 AND user_id = ANY($2)
             ORDER BY user_id FOR UPDATE",
        )
        .bind(org_id)
        .bind(vec![actor_id, user_id])
        .fetch_all(db.conn())
        .await?;
        let Some((_, actor_role, None)) = rows.iter().find(|row| row.0 == actor_id) else {
            return Err(G5Error::Forbidden {
                missing_scopes: vec![],
            });
        };
        if actor_id != user_id {
            if !matches!(actor_role.as_str(), "admin") {
                return Err(G5Error::Forbidden {
                    missing_scopes: vec!["org:admin".into()],
                });
            }
            PermissionService::require_org_admin_on_conn(db.conn(), actor_id, org_id).await?;
        }
        let (_, _, inactive) =
            rows.iter()
                .find(|row| row.0 == user_id)
                .ok_or_else(|| G5Error::NotFound {
                    resource: "organization membership".into(),
                })?;
        if inactive.is_some() {
            return Err(G5Error::Conflict {
                reason: "The member is deactivated; their workspaces are already being stopped"
                    .into(),
            });
        }
        let requested = Uuid::new_v4();
        let hold_id: Uuid = sqlx::query_scalar(
            "UPDATE org_memberships
             SET deactivation_cleanup_id = COALESCE(deactivation_cleanup_id, $3)
             WHERE org_id = $1 AND user_id = $2
             RETURNING deactivation_cleanup_id",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(requested)
        .fetch_one(db.conn())
        .await?;
        tracing::info!(
            %org_id, %actor_id, %user_id, %hold_id, joined = hold_id != requested,
            "stop workspaces by user: execution hold set"
        );
        Ok(MemberExecutionHold {
            hold_id,
            joined: hold_id != requested,
        })
    }

    /// Per-workspace shutdown state for a member's pending execution hold.
    /// The member themself or an organization administrator may read it.
    pub async fn member_shutdown_progress(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<MemberShutdownWorkspace>, G5Error> {
        if actor_id == user_id {
            PermissionService::require_active_member_on_conn(db.conn(), actor_id, org_id, false)
                .await?;
        } else {
            PermissionService::require_org_admin_on_conn(db.conn(), actor_id, org_id).await?;
        }
        Ok(sqlx::query_as(
            "SELECT workspace_id, name, state,
                    CASE WHEN state <> 'stopped' THEN last_error->>'reason' END AS blocker
             FROM (
             SELECT w.id AS workspace_id, w.name, vm.last_error,
             CASE WHEN vm.status IN ('starting','running') THEN 'pending'
                  WHEN vm.status = 'stopping' OR vm.vm_id IS NOT NULL THEN 'stopping'
                  WHEN vm.start_intent->>'status' IN ('pending','queued_for_capacity','starting')
                       OR EXISTS (SELECT 1 FROM tasks t WHERE t.workspace_id=w.id AND t.status IN ('queued','running'))
                    THEN 'pending' ELSE 'stopped' END AS state
             FROM workspaces w LEFT JOIN workspace_vms vm ON vm.workspace_id=w.id
             WHERE w.org_id=$1 AND w.owner_user_id=$2 AND NOT w.is_base
               AND (w.deleted_at IS NULL OR vm.status IN ('starting','running','stopping')
                 OR vm.vm_id IS NOT NULL
                 OR vm.start_intent->>'status' IN ('pending','queued_for_capacity','starting')
                 OR EXISTS (SELECT 1 FROM tasks t WHERE t.workspace_id=w.id AND t.status IN ('queued','running')))
             ) progress
             ORDER BY name, workspace_id")
            .bind(org_id).bind(user_id).fetch_all(db.conn()).await?)
    }

    /// List all members of the org with their roles and user profile info.
    ///
    /// Returns members ordered by role priority (admin first, then
    /// member, viewer) then by `display_name ASC`.
    pub async fn list_members(
        &self,
        system_pool: &PgPool,
        org_id: Uuid,
    ) -> Result<Vec<OrgMemberDTO>, G5Error> {
        let members = sqlx::query_as!(
            OrgMemberDTO,
            r#"
            SELECT u.id as user_id, u.email, u.display_name, u.avatar_url,
                   m.role, m.created_at as joined_at, m.deactivated_at, m.activity_revision,
                   (m.deactivation_cleanup_id IS NOT NULL) as "shutdown_pending!"
            FROM org_memberships m
            JOIN users u ON m.user_id = u.id
            WHERE m.org_id = $1
            ORDER BY
                CASE m.role
                    WHEN 'admin' THEN 1
                    WHEN 'member' THEN 2
                    WHEN 'viewer' THEN 3
                END,
                u.display_name ASC
            "#,
            org_id
        )
        .fetch_all(system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list members: {e}"),
        })?;

        Ok(members)
    }

    /// Invite a user to the org by email.
    ///
    /// The behavior depends on whether the email matches an existing G5 user:
    ///
    /// 1. Validate role is one of `admin`, `member`, `viewer`.
    /// 2. If the email already belongs to an existing org member, return the
    ///    existing membership (idempotent).
    /// 3. If there's already a pending invitation for this email, return it
    ///    (idempotent).
    /// 4. If the user exists: create membership immediately.
    /// 5. If the user doesn't exist: create a pending invitation.
    pub async fn invite_member(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        invited_by: Uuid,
        req: InviteMemberRequest,
    ) -> Result<InviteMemberResponse, G5Error> {
        PermissionService::require_org_admin_on_conn(db.conn(), invited_by, org_id).await?;
        // Step 1: Validate role
        if !["admin", "member", "viewer"].contains(&req.role.as_str()) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "role",
                    "error": "Role must be one of: admin, member, viewer"
                }),
            });
        }

        // Step 2: Check if the email already belongs to an existing org member
        let existing_member = sqlx::query_as!(
            OrgMemberDTO,
            r#"
            SELECT u.id as user_id, u.email, u.display_name, u.avatar_url,
                   m.role, m.created_at as joined_at, m.deactivated_at, m.activity_revision,
                   (m.deactivation_cleanup_id IS NOT NULL) as "shutdown_pending!"
            FROM org_memberships m
            JOIN users u ON m.user_id = u.id
            WHERE m.org_id = $1 AND u.email = $2
            "#,
            org_id,
            req.email
        )
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check existing member: {e}"),
        })?;

        if let Some(member) = existing_member {
            return Ok(InviteMemberResponse {
                membership: Some(member),
                invitation: None,
            });
        }

        // Step 3: Check if there's already a pending invitation for this email
        let existing_invitation = sqlx::query_as!(
            OrgInvitation,
            r#"
            SELECT id, org_id, email, role, invited_by, status,
                   accepted_by, expires_at, created_at, updated_at
            FROM org_invitations
            WHERE org_id = $1 AND email = $2 AND status = 'pending'
            "#,
            org_id,
            req.email
        )
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check existing invitation: {e}"),
        })?;

        if let Some(invitation) = existing_invitation {
            return Ok(InviteMemberResponse {
                membership: None,
                invitation: Some(invitation),
            });
        }

        // Step 4: Look up the email in the users table
        let existing_user = sqlx::query!(
            r#"
            SELECT id, email, display_name, avatar_url
            FROM users
            WHERE email = $1
            "#,
            req.email
        )
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to look up user: {e}"),
        })?;

        match existing_user {
            Some(user) => {
                // User exists — create membership immediately
                let now = Utc::now();
                sqlx::query!(
                    r#"
                    INSERT INTO org_memberships (org_id, user_id, role, created_at)
                    VALUES ($1, $2, $3, $4)
                    "#,
                    org_id,
                    user.id,
                    req.role,
                    now
                )
                .execute(db.conn())
                .await
                .map_err(|e| map_org_mutation_error(e, "create membership"))?;

                let member = OrgMemberDTO {
                    user_id: user.id,
                    email: user.email,
                    display_name: user.display_name,
                    avatar_url: user.avatar_url,
                    role: req.role.clone(),
                    joined_at: now,
                    deactivated_at: None,
                    activity_revision: 0,
                    shutdown_pending: false,
                };

                tracing::info!(
                    org_id = %org_id,
                    user_id = %user.id,
                    invited_by_user_id = %invited_by,
                    "Invited member (immediate membership)"
                );

                Ok(InviteMemberResponse {
                    membership: Some(member),
                    invitation: None,
                })
            }
            None => {
                // User does not exist — create pending invitation
                let invitation_id = Uuid::now_v7();
                let now = Utc::now();
                let expires_at = now + Duration::days(7);

                let invitation = sqlx::query_as!(
                    OrgInvitation,
                    r#"
                    INSERT INTO org_invitations (
                        id, org_id, email, role, invited_by, status,
                        expires_at, created_at, updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7, $7)
                    RETURNING id, org_id, email, role, invited_by, status,
                              accepted_by, expires_at, created_at, updated_at
                    "#,
                    invitation_id,
                    org_id,
                    req.email,
                    req.role,
                    invited_by,
                    expires_at,
                    now
                )
                .fetch_one(db.conn())
                .await
                .map_err(|e| map_org_mutation_error(e, "create invitation"))?;

                tracing::info!(
                    org_id = %org_id,
                    invitation_id = %invitation.id,
                    invited_by_user_id = %invited_by,
                    "Invited member (pending invitation)"
                );
                self.request_invitation_email(db, &invitation).await?;

                Ok(InviteMemberResponse {
                    membership: None,
                    invitation: Some(invitation),
                })
            }
        }
    }

    /// The organization an invitation belongs to.
    pub async fn invitation_org_id(
        &self,
        system_pool: &sqlx::PgPool,
        invitation_id: Uuid,
    ) -> Result<Option<Uuid>, G5Error> {
        sqlx::query_scalar::<_, Uuid>("SELECT org_id FROM org_invitations WHERE id = $1")
            .bind(invitation_id)
            .fetch_optional(system_pool)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to load invitation organization: {e}"),
            })
    }

    /// Change a member's role within the org.
    ///
    /// - Roles outside Admin, Member and Viewer are refused.
    /// - If the membership does not exist, returns `G5Error::NotFound`.
    async fn update_member_role(
        &self,
        conn: &mut PgConnection,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
        new_role: String,
    ) -> Result<OrgMemberDTO, G5Error> {
        PermissionService::require_org_admin_on_conn(&mut *conn, actor_id, org_id).await?;
        // Validate against the complete supported organization role set.
        if !["admin", "member", "viewer"].contains(&new_role.as_str()) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "role",
                    "error": format!("Invalid role '{}'. Must be one of: admin, member, viewer", new_role)
                }),
            });
        }

        // Check if the membership exists and get current role
        let _existing = sqlx::query!(
            r#"
            SELECT role
            FROM org_memberships
            WHERE org_id = $1 AND user_id = $2
            "#,
            org_id,
            user_id
        )
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check membership: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("membership for user {user_id} in org {org_id}"),
        })?;

        if new_role == "viewer" {
            reject_active_workspace_owner_change(conn, org_id, user_id).await?;
        }

        // Update the role
        sqlx::query!(
            r#"
            UPDATE org_memberships
            SET role = $3
            WHERE org_id = $1 AND user_id = $2
            "#,
            org_id,
            user_id,
            new_role
        )
        .execute(&mut *conn)
        .await
        .map_err(|error| map_org_mutation_error(error, "update member role"))?;
        if new_role == "viewer" {
            sqlx::query("SET CONSTRAINTS workspaces_owner_same_org_fk IMMEDIATE")
                .execute(&mut *conn)
                .await
                .map_err(|error| map_org_mutation_error(error, "update member role"))?;
        }

        // Fetch the updated member DTO
        let member = sqlx::query_as!(
            OrgMemberDTO,
            r#"
            SELECT u.id as user_id, u.email, u.display_name, u.avatar_url,
                   m.role, m.created_at as joined_at, m.deactivated_at, m.activity_revision,
                   (m.deactivation_cleanup_id IS NOT NULL) as "shutdown_pending!"
            FROM org_memberships m
            JOIN users u ON m.user_id = u.id
            WHERE m.org_id = $1 AND m.user_id = $2
            "#,
            org_id,
            user_id
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch updated member: {e}"),
        })?;

        tracing::info!(org_id = %org_id, user_id = %user_id, new_role = %new_role, "Updated member role");
        Ok(member)
    }

    /// Remove a member from the org.
    ///
    /// - If the membership does not exist, returns `G5Error::NotFound`.
    async fn remove_member(
        &self,
        conn: &mut PgConnection,
        org_id: Uuid,
        actor_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), G5Error> {
        PermissionService::require_org_admin_on_conn(&mut *conn, actor_id, org_id).await?;
        let pending: Option<Option<Uuid>> = sqlx::query_scalar(
            "SELECT deactivation_cleanup_id FROM org_memberships
             WHERE org_id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?;
        if pending.flatten().is_some() {
            return Err(G5Error::Conflict {
                reason: "Membership shutdown is still pending".into(),
            });
        }
        // Check if the membership exists and get current role
        let _existing = sqlx::query!(
            r#"
            SELECT role
            FROM org_memberships
            WHERE org_id = $1 AND user_id = $2
            "#,
            org_id,
            user_id
        )
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check membership: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("membership for user {user_id} in org {org_id}"),
        })?;

        reject_active_workspace_owner_change(conn, org_id, user_id).await?;

        sqlx::query!(
            r#"
            DELETE FROM org_memberships
            WHERE org_id = $1 AND user_id = $2
            "#,
            org_id,
            user_id
        )
        .execute(&mut *conn)
        .await
        .map_err(|error| map_org_mutation_error(error, "remove member"))?;
        sqlx::query("SET CONSTRAINTS workspaces_owner_same_org_fk IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|error| map_org_mutation_error(error, "remove member"))?;

        tracing::info!(org_id = %org_id, user_id = %user_id, "Removed member");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Invitation Lifecycle
    // -----------------------------------------------------------------------

    /// List all invitations for the org (all statuses).
    ///
    /// Returns ordered by `created_at DESC`.
    pub async fn list_invitations(
        &self,
        system_pool: &PgPool,
        org_id: Uuid,
    ) -> Result<Vec<OrgInvitation>, G5Error> {
        let invitations = sqlx::query_as!(
            OrgInvitation,
            r#"
            SELECT id, org_id, email, role, invited_by, status,
                   accepted_by, expires_at, created_at, updated_at
            FROM org_invitations
            WHERE org_id = $1
            ORDER BY created_at DESC
            "#,
            org_id
        )
        .fetch_all(system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list invitations: {e}"),
        })?;

        Ok(invitations)
    }

    /// Batch-load display names for a set of user IDs.
    ///
    /// Returns a map of user_id → display_name (falls back to email).
    pub async fn get_user_display_names(
        &self,
        system_pool: &PgPool,
        user_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, String>, G5Error> {
        if user_ids.is_empty() {
            return Ok(Default::default());
        }
        let rows = sqlx::query!(
            r#"SELECT id, COALESCE(display_name, email) AS "name!" FROM users WHERE id = ANY($1)"#,
            user_ids
        )
        .fetch_all(system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load user display names: {e}"),
        })?;
        Ok(rows.into_iter().map(|r| (r.id, r.name)).collect())
    }

    /// Accept, inside the caller's login transaction, every pending unexpired
    /// invitation addressed to one of `verified_emails`
    /// (`g5.organization.invitation-authority`). Each matching row is locked
    /// first, then accepted only if it is still pending and unexpired at that
    /// moment, and membership is created only for an invitation this call
    /// accepted. A revoke or expiry that committed first yields nothing; one
    /// that arrives while this runs waits for the commit and finds the row
    /// accepted (the revoke then reports a conflict). Acceptance never
    /// reactivates a retained inactive membership.
    pub async fn accept_pending_invitations_on_conn(
        conn: &mut PgConnection,
        user_id: Uuid,
        verified_emails: &[String],
    ) -> Result<Vec<AcceptedInvitation>, G5Error> {
        let candidates: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM org_invitations
            WHERE lower(email) = ANY($1::text[])
              AND status = 'pending'
              AND expires_at > now()
            ORDER BY id
            FOR UPDATE
            "#,
        )
        .bind(verified_emails)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to lock pending invitations: {e}"),
        })?;

        let mut accepted = Vec::with_capacity(candidates.len());
        for invitation_id in candidates {
            // The locked row is re-read by this conditional update, so an
            // invitation that stopped being pending while the lock was awaited
            // is skipped rather than overwritten as accepted.
            let claimed: Option<(Uuid, String)> = sqlx::query_as(
                r#"
                UPDATE org_invitations
                SET status = 'accepted', accepted_by = $2, updated_at = now()
                WHERE id = $1 AND status = 'pending' AND expires_at > now()
                RETURNING org_id, role
                "#,
            )
            .bind(invitation_id)
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to accept invitation: {e}"),
            })?;
            let Some((org_id, role)) = claimed else {
                continue;
            };
            sqlx::query(
                r#"
                INSERT INTO org_memberships (org_id, user_id, role)
                VALUES ($1, $2, $3)
                ON CONFLICT (org_id, user_id) DO NOTHING
                "#,
            )
            .bind(org_id)
            .bind(user_id)
            .bind(&role)
            .execute(&mut *conn)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to create org membership: {e}"),
            })?;
            accepted.push(AcceptedInvitation {
                invitation_id,
                org_id,
                role,
            });
        }
        Ok(accepted)
    }

    /// Accept a pending invitation.
    ///
    /// The user's email MUST match the invitation email.
    ///
    /// 1. Lock and fetch the invitation. If not found, return `G5Error::NotFound`.
    /// 2. If not pending, return `G5Error::Conflict`.
    /// 3. If email doesn't match, return `G5Error::Conflict`.
    /// 4. Within the caller's transaction: update invitation status + create membership.
    pub async fn accept_invitation(
        &self,
        db: &mut ScopedDb,
        invitation_id: Uuid,
        user_id: Uuid,
    ) -> Result<OrgMemberDTO, G5Error> {
        // Lock the invitation so state validation and mutation cannot race with
        // another acceptance attempt on the same row.
        let invitation = sqlx::query_as::<_, OrgInvitation>(
            r#"
            SELECT id, org_id, email, role, invited_by, status,
                   accepted_by, expires_at, created_at, updated_at
            FROM org_invitations
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(invitation_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch invitation: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("invitation {invitation_id}"),
        })?;

        // Must be pending
        if invitation.status != "pending" {
            return Err(G5Error::Conflict {
                reason: "Invitation is no longer pending".to_string(),
            });
        }

        // Must not be expired
        if invitation.expires_at < Utc::now() {
            return Err(G5Error::Conflict {
                reason: "Invitation has expired".to_string(),
            });
        }

        // Fetch user and validate email match
        let user = sqlx::query!(
            r#"
            SELECT id, email, display_name, avatar_url
            FROM users
            WHERE id = $1
            "#,
            user_id
        )
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch user: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("user {user_id}"),
        })?;

        if user.email != invitation.email {
            return Err(G5Error::Conflict {
                reason: "Email does not match invitation".to_string(),
            });
        }

        let now = Utc::now();

        // Update invitation status
        sqlx::query!(
            r#"
            UPDATE org_invitations
            SET status = 'accepted', accepted_by = $2, updated_at = $3
            WHERE id = $1
            "#,
            invitation_id,
            user_id,
            now
        )
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to update invitation: {e}"),
        })?;

        // Create membership — idempotent in case of concurrent acceptance (§38.122)
        sqlx::query!(
            r#"
            INSERT INTO org_memberships (org_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT DO NOTHING
            "#,
            invitation.org_id,
            user_id,
            invitation.role,
            now
        )
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to create membership: {e}"),
        })?;

        let member = sqlx::query_as::<_, OrgMemberDTO>(
            "SELECT u.id AS user_id, u.email, u.display_name, u.avatar_url, m.role,
             m.created_at AS joined_at, m.deactivated_at, m.activity_revision,
             m.deactivation_cleanup_id IS NOT NULL AS shutdown_pending
             FROM org_memberships m JOIN users u ON u.id = m.user_id
             WHERE m.org_id = $1 AND m.user_id = $2",
        )
        .bind(invitation.org_id)
        .bind(user_id)
        .fetch_one(db.conn())
        .await?;

        tracing::info!(invitation_id = %invitation_id, user_id = %user_id, "Accepted invitation");
        Ok(member)
    }

    /// Decline a pending invitation.
    ///
    /// 1. Fetch the invitation. If not found, return `G5Error::NotFound`.
    /// 2. If not pending, return `G5Error::Conflict`.
    /// 3. If email doesn't match, return `G5Error::Conflict`.
    /// 4. Update status to `'declined'`.
    pub async fn decline_invitation(
        &self,
        system_pool: &PgPool,
        invitation_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), G5Error> {
        let invitation = self.get_invitation(system_pool, invitation_id).await?;

        if invitation.status != "pending" {
            return Err(G5Error::Conflict {
                reason: "Invitation is no longer pending".to_string(),
            });
        }

        let user_email = sqlx::query_scalar::<_, String>(
            r#"
            SELECT email
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch user: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("user {user_id}"),
        })?;

        if user_email != invitation.email {
            return Err(G5Error::Conflict {
                reason: "Email does not match invitation".to_string(),
            });
        }

        // Resolve against the current row (`g5.organization.invitation-authority`):
        // an acceptance that claimed the invitation meanwhile is not
        // overwritten as declined.
        let now = Utc::now();
        let declined = sqlx::query!(
            r#"
            UPDATE org_invitations
            SET status = 'declined', updated_at = $2
            WHERE id = $1 AND status = 'pending'
            "#,
            invitation_id,
            now
        )
        .execute(system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to decline invitation: {e}"),
        })?;
        if declined.rows_affected() == 0 {
            return Err(G5Error::Conflict {
                reason: "Invitation is no longer pending".to_string(),
            });
        }

        tracing::info!(invitation_id = %invitation_id, user_id = %user_id, "Declined invitation");
        Ok(())
    }

    /// Resend a pending invitation by resetting its expiry and requesting a
    /// new invitation email.
    ///
    /// 1. Fetch the invitation. If not found, return `G5Error::NotFound`.
    /// 2. If not pending, return `G5Error::Conflict`.
    /// 3. Reset `expires_at` to `now() + 7 days`.
    /// 4. Request the email (`g5.organization.invitation-resend`).
    pub async fn resend_invitation(
        &self,
        db: &mut ScopedDb,
        invitation_id: Uuid,
        actor_id: Uuid,
    ) -> Result<OrgInvitation, G5Error> {
        let invitation = self.get_invitation(db.conn(), invitation_id).await?;
        PermissionService::require_org_admin_on_conn(db.conn(), actor_id, invitation.org_id)
            .await?;

        if invitation.status != "pending" {
            return Err(G5Error::Conflict {
                reason: "Can only resend pending invitations".to_string(),
            });
        }

        let now = Utc::now();
        let new_expires_at = now + Duration::days(7);

        let updated = sqlx::query_as::<_, OrgInvitation>(
            r#"
            UPDATE org_invitations
            SET expires_at = $2, updated_at = $3
            WHERE id = $1 AND status = 'pending'
            RETURNING id, org_id, email, role, invited_by, status,
                      accepted_by, expires_at, created_at, updated_at
            "#,
        )
        .bind(invitation_id)
        .bind(new_expires_at)
        .bind(now)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| map_org_mutation_error(e, "resend invitation"))?
        .ok_or_else(|| G5Error::Conflict {
            reason: "Invitation is no longer pending".into(),
        })?;

        tracing::info!(invitation_id = %invitation_id, "Resent invitation");
        self.request_invitation_email(db, &updated).await?;
        Ok(updated)
    }

    /// Request the invitation's email in the caller's transaction
    /// (`g5.organization.invitation-email`, `.invitation-resend`). A limit
    /// refusal rolls back the invite or resend with it
    /// (`g5.organization.invitation-email-limit`).
    async fn request_invitation_email(
        &self,
        db: &mut ScopedDb,
        invitation: &OrgInvitation,
    ) -> Result<(), G5Error> {
        if !self.email.is_enabled() {
            return Ok(());
        }
        let (org_name, org_slug, inviter_name): (String, String, String) = sqlx::query_as(
            "SELECT o.name, o.slug, COALESCE(NULLIF(btrim(u.display_name), ''), u.email)
               FROM orgs o, users u
              WHERE o.id = $1 AND u.id = $2",
        )
        .bind(invitation.org_id)
        .bind(invitation.invited_by)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load invitation email context: {e}"),
        })?;
        let requested = self
            .email
            .request(
                db.conn(),
                EmailRequest {
                    org_id: invitation.org_id,
                    sender: INVITATION_SENDER,
                    recipient: invitation.email.clone(),
                    message: Message::OrgInvitation(OrgInvitationEmail {
                        invitation_id: invitation.id,
                        org_name,
                        inviter_name,
                        role: invitation.role.clone(),
                        link_path: format!("/{org_slug}"),
                    }),
                },
            )
            .await;
        match requested {
            Ok(outcome) => {
                tracing::debug!(invitation_id = %invitation.id, ?outcome, "Invitation email requested");
            }
            // An address or link G5 cannot send to does not undo a valid
            // invitation (`g5.organization.invitation-email`).
            Err(EmailError::InvalidRequest(reason)) => {
                tracing::warn!(invitation_id = %invitation.id, reason, "Invitation email not requested");
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    /// The latest invitation email status for each of `invitation_ids`
    /// (`g5.organization.invitation-email-status`).
    pub async fn invitation_email_statuses(
        &self,
        system_pool: &PgPool,
        org_id: Uuid,
        invitation_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, DeliveryStatus>, G5Error> {
        let keys: Vec<String> = invitation_ids.iter().map(Uuid::to_string).collect();
        let statuses = self
            .email
            .latest_status(system_pool, org_id, PurposeKind::OrgInvitation, &keys)
            .await?;
        Ok(statuses
            .into_iter()
            .filter_map(|(key, status)| Uuid::parse_str(&key).ok().map(|id| (id, status)))
            .collect())
    }

    /// Revoke (delete) a pending invitation.
    ///
    /// 1. Fetch the invitation. If not found, return `G5Error::NotFound`.
    /// 2. If not pending, return `G5Error::Conflict`.
    /// 3. Delete the invitation row.
    pub async fn revoke_invitation(
        &self,
        db: &mut ScopedDb,
        invitation_id: Uuid,
        actor_id: Uuid,
    ) -> Result<(), G5Error> {
        let invitation = self.get_invitation(db.conn(), invitation_id).await?;
        PermissionService::require_org_admin_on_conn(db.conn(), actor_id, invitation.org_id)
            .await?;

        if invitation.status != "pending" {
            return Err(G5Error::Conflict {
                reason: "Can only revoke pending invitations".to_string(),
            });
        }

        let deleted =
            sqlx::query("DELETE FROM org_invitations WHERE id = $1 AND status = 'pending'")
                .bind(invitation_id)
                .execute(db.conn())
                .await
                .map_err(|e| map_org_mutation_error(e, "revoke invitation"))?;
        if deleted.rows_affected() == 0 {
            return Err(G5Error::Conflict {
                reason: "Invitation is no longer pending".into(),
            });
        }

        tracing::info!(invitation_id = %invitation_id, "Revoked invitation");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Repo Listing
    // -----------------------------------------------------------------------

    /// List all repos belonging to the org, ordered by `name ASC`.
    pub async fn list_repos(&self, db: &mut ScopedDb, org_id: Uuid) -> Result<Vec<Repo>, G5Error> {
        let repos = sqlx::query_as::<_, Repo>(
            r#"
            SELECT id, org_id, name, slug, git_url, default_branch,
                   provider, provider_repo_id, provider_full_name, created_at
            FROM repos
            WHERE org_id = $1
            ORDER BY name ASC
            "#,
        )
        .bind(org_id)
        .fetch_all(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list repos: {e}"),
        })?;

        Ok(repos)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Fetch an invitation by ID, returning `G5Error::NotFound` if missing.
    async fn get_invitation<'e>(
        &self,
        executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
        invitation_id: Uuid,
    ) -> Result<OrgInvitation, G5Error> {
        sqlx::query_as!(
            OrgInvitation,
            r#"
            SELECT id, org_id, email, role, invited_by, status,
                   accepted_by, expires_at, created_at, updated_at
            FROM org_invitations
            WHERE id = $1
            "#,
            invitation_id
        )
        .fetch_optional(executor)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch invitation: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("invitation {invitation_id}"),
        })
    }

    /// Resolve org/repo/workspace slugs to UUIDs.
    ///
    /// Looks up the org by its `slug` column. If `repo_slug` is provided,
    /// looks up the repo by `(org_id, slug)`. If `workspace_slug` is provided,
    /// performs a case-insensitive name match against workspaces in the org
    /// (workspaces don't have a dedicated slug column).
    pub async fn resolve_slugs(
        &self,
        system_pool: &PgPool,
        db: &mut ScopedDb,
        org_slug: &str,
        repo_slug: Option<&str>,
        workspace_slug: Option<&str>,
    ) -> Result<ResolveResult, G5Error> {
        // 1. Resolve org, by slug or, for callers that hold only the id
        // (the CLI), by id.
        let org_id: Uuid = if let Ok(id) = Uuid::parse_str(org_slug) {
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM orgs WHERE id = $1")
                .bind(id)
                .fetch_optional(system_pool)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve org id: {e}"),
                })?
        } else {
            sqlx::query_scalar!(r#"SELECT id AS "id!" FROM orgs WHERE slug = $1"#, org_slug)
                .fetch_optional(system_pool)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve org slug: {e}"),
                })?
        }
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("org with slug '{org_slug}'"),
        })?;

        // 2. Resolve repo (optional)
        let repo_id = if let Some(slug) = repo_slug {
            let id: Option<Uuid> = sqlx::query_scalar!(
                r#"SELECT id AS "id!" FROM repos WHERE org_id = $1 AND slug = $2"#,
                org_id,
                slug
            )
            .fetch_optional(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to resolve repo slug: {e}"),
            })?;
            if id.is_none() {
                return Err(G5Error::NotFound {
                    resource: format!("repo with slug '{slug}' in org '{org_slug}'"),
                });
            }
            id
        } else {
            None
        };

        // 3. Resolve workspace (optional): the current name of a live
        // workspace, else a former name it still reserves, both ignoring case
        // (`g5.workspace.former-names`). The current name comes back so a
        // caller can replace a former name with it.
        let (workspace_id, workspace_name) = if let Some(ws_slug) = workspace_slug {
            let resolved: Option<(Uuid, String)> = sqlx::query_as(
                r#"
                SELECT w.id, w.name
                FROM workspaces w
                WHERE w.org_id = $1
                  AND LOWER(w.name) = LOWER($2)
                  AND w.deleted_at IS NULL
                UNION ALL
                SELECT w.id, w.name
                FROM workspace_former_names f
                JOIN workspaces w ON w.id = f.workspace_id AND w.org_id = f.org_id
                WHERE f.org_id = $1
                  AND LOWER(f.name) = LOWER($2)
                  AND w.deleted_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(org_id)
            .bind(ws_slug)
            .fetch_optional(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to resolve workspace slug: {e}"),
            })?;
            let Some((id, name)) = resolved else {
                return Err(G5Error::NotFound {
                    resource: format!("workspace '{ws_slug}' in org '{org_slug}'"),
                });
            };
            (Some(id), Some(name))
        } else {
            (None, None)
        };

        Ok(ResolveResult {
            org_id,
            repo_id,
            workspace_id,
            workspace_name,
        })
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Generate a URL-safe slug from a name.
///
/// Algorithm:
/// 1. Convert to lowercase.
/// 2. Replace spaces and special characters with hyphens.
/// 3. Remove any character that is not alphanumeric or a hyphen.
/// 4. Collapse consecutive hyphens into a single hyphen.
/// 5. Trim leading/trailing hyphens.
/// 6. If the result is empty, use a UUID-based fallback.
fn generate_slug(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens
    let mut collapsed = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push(c);
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }

    // Trim leading/trailing hyphens
    let trimmed = collapsed.trim_matches('-').to_string();

    if trimmed.is_empty() {
        // UUID-based fallback
        Uuid::now_v7().to_string()[..8].to_string()
    } else {
        trimmed
    }
}

/// Validate a caller-supplied slug is URL-safe.
///
/// Slugs must be 1–100 characters, consist only of lowercase ASCII
/// alphanumerics and hyphens, and not start or end with a hyphen.
fn validate_slug(slug: &str) -> Result<(), G5Error> {
    if slug.is_empty() || slug.len() > 100 {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "slug",
                "error": "Slug must be between 1 and 100 characters"
            }),
        });
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "slug",
                "error": "Slug may only contain lowercase letters, digits, and hyphens"
            }),
        });
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "slug",
                "error": "Slug must not start or end with a hyphen"
            }),
        });
    }
    Ok(())
}

/// Refuse a reserved slug for a tenant organization
/// (`g5.organization.platform-operator.identity`).
fn reject_reserved_slug(slug: &str) -> Result<(), G5Error> {
    if RESERVED_ORG_SLUGS.contains(&slug) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "slug",
                "error": format!("The slug '{slug}' is reserved")
            }),
        });
    }
    Ok(())
}

/// Check if a sqlx error is a unique constraint violation.
fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(ref db_err) = e {
        // Postgres unique violation code: 23505
        db_err.code().is_some_and(|code| code == "23505")
    } else {
        false
    }
}

/// Resolve the deployment's default image policy for new organizations and
/// startup reconciliation of organizations that have no policy. An absent or
/// blank track disables the default; a nonblank track must be present in the
/// promoted image catalog.
pub fn new_org_image_policy(
    configured_track: Option<&str>,
    promoted_images: Option<&str>,
) -> Result<Option<(String, String)>, G5Error> {
    let Some(track) = configured_track
        .map(str::trim)
        .filter(|track| !track.is_empty())
    else {
        return Ok(None);
    };
    parse_promoted_base_images(promoted_images.unwrap_or_default())
        .into_iter()
        .find(|(name, _)| name == track)
        .map(Some)
        .ok_or_else(|| {
            G5Error::Config(format!(
                "new organization default image track {track} is absent from promoted images"
            ))
        })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Parse the `G5_PROMOTED_BASE_IMAGES` env value (ontology [D52]): a
/// comma-separated list of `track=digest-pinned-ref` entries injected by the
/// helm-deploy step (infra [D27]). A bare digest-pinned ref remains accepted
/// for backwards compatibility and derives its track from the path basename.
///
/// Returns `(track, ref)` pairs. Entries with an invalid track, an invalid
/// digest pin, or a duplicate track are error-logged and skipped so malformed
/// pipeline input cannot crash startup, poison another entry, or make track
/// resolution depend on list order.
pub fn parse_promoted_base_images(raw: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for part in raw.split(',') {
        let entry = part.trim();
        if entry.is_empty() {
            continue;
        }
        let (explicit_track, image_ref) = entry
            .split_once('=')
            .map_or((None, entry), |(track, image_ref)| {
                (Some(track.trim()), image_ref.trim())
            });
        if let Err(e) = validate_base_image_ref(image_ref) {
            tracing::error!(
                target: "g5_audit",
                event = "org_default_seed.invalid_ref",
                image_ref,
                error = e.as_str(),
                "G5_PROMOTED_BASE_IMAGES entry failed digest-pin validation; skipping"
            );
            continue;
        }
        let track = explicit_track.unwrap_or_else(|| {
            image_ref
                .rsplit_once('@')
                .map(|(name, _)| name)
                .and_then(|name| name.rsplit('/').next())
                .unwrap_or_default()
        });
        let valid_track = track
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
            && track.len() <= 63
            && track.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '.' | '_' | '-')
            });
        if !valid_track {
            tracing::error!(
                target: "g5_audit",
                event = "org_default_seed.invalid_track",
                track,
                image_ref,
                "G5_PROMOTED_BASE_IMAGES entry has an invalid track name; skipping"
            );
            continue;
        }
        if entries.iter().any(|(existing, _)| existing == track) {
            tracing::error!(
                target: "g5_audit",
                event = "org_default_seed.duplicate_track",
                track,
                image_ref,
                "G5_PROMOTED_BASE_IMAGES contains a duplicate track; keeping the first entry"
            );
            continue;
        }
        entries.push((track.to_string(), image_ref.to_string()));
    }
    entries
}

/// An invitation email is still wanted only while its invitation exists, is
/// pending and unexpired, and is addressed to the same recipient
/// (`g5.organization.invitation-email-validity`). Revocation deletes the row.
pub struct InvitationDeliveryGate;

#[async_trait::async_trait]
impl DeliveryGate for InvitationDeliveryGate {
    async fn still_wanted(
        &self,
        pool: &PgPool,
        purpose_key: &str,
        recipient: &str,
    ) -> Result<Validity, sqlx::Error> {
        let Ok(invitation_id) = Uuid::parse_str(purpose_key) else {
            return Ok(Validity::Cancelled("invalid_purpose"));
        };
        let row: Option<(String, String, bool)> = sqlx::query_as(
            "SELECT status, email, expires_at > now()
               FROM org_invitations
              WHERE id = $1",
        )
        .bind(invitation_id)
        .fetch_optional(pool)
        .await?;
        Ok(match row {
            None => Validity::Cancelled("invitation_revoked"),
            Some((status, _, _)) if status == "accepted" => {
                Validity::Cancelled("invitation_accepted")
            }
            Some((status, _, _)) if status == "declined" => {
                Validity::Cancelled("invitation_declined")
            }
            Some((status, _, unexpired)) if status != "pending" || !unexpired => {
                Validity::Cancelled("invitation_expired")
            }
            Some((_, email, _)) if email.trim() != recipient => {
                Validity::Cancelled("recipient_changed")
            }
            Some(_) => Validity::Wanted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_org_policy_requires_a_promoted_digest_for_the_configured_track() {
        let image = format!(
            "us-docker.pkg.dev/g5-test/runtime/g5d-g5dev@sha256:{}",
            "a".repeat(64)
        );
        let catalog = format!("g5d-g5dev={image}");
        assert_eq!(
            new_org_image_policy(Some("g5d-g5dev"), Some(&catalog)).unwrap(),
            Some(("g5d-g5dev".into(), image))
        );
        assert!(new_org_image_policy(Some("g5d-g5dev"), None).is_err());
        assert_eq!(new_org_image_policy(None, Some(&catalog)).unwrap(), None);
    }

    #[test]
    fn new_org_policy_treats_blank_track_as_unconfigured() {
        let catalog = format!(
            "local-dev=registry.example/workspace@sha256:{}",
            "a".repeat(64)
        );
        assert_eq!(
            new_org_image_policy(Some(""), Some(&catalog)).unwrap(),
            None
        );
        assert_eq!(
            new_org_image_policy(Some(" \t "), Some(&catalog)).unwrap(),
            None
        );
        assert_eq!(new_org_image_policy(Some(" \t "), None).unwrap(), None);
    }

    #[test]
    fn test_generate_slug_basic() {
        assert_eq!(generate_slug("Acme Corp"), "acme-corp");
    }

    #[test]
    fn test_generate_slug_special_chars() {
        assert_eq!(generate_slug("My Org! @#$%"), "my-org");
    }

    #[test]
    fn test_generate_slug_consecutive_hyphens() {
        assert_eq!(generate_slug("hello   world"), "hello-world");
    }

    #[test]
    fn test_generate_slug_leading_trailing() {
        assert_eq!(generate_slug("  hello  "), "hello");
    }

    #[test]
    fn test_generate_slug_empty_fallback() {
        let slug = generate_slug("!@#$%");
        // Should produce a UUID-based fallback (8 chars)
        assert_eq!(slug.len(), 8);
    }

    #[test]
    fn test_generate_slug_already_clean() {
        assert_eq!(generate_slug("my-org"), "my-org");
    }

    #[test]
    fn test_generate_slug_unicode() {
        // Non-ASCII characters should be replaced with hyphens
        let slug = generate_slug("café résumé");
        assert!(slug.contains("caf"));
    }

    #[test]
    fn test_generate_slug_numbers() {
        assert_eq!(generate_slug("org123"), "org123");
    }

    #[test]
    fn test_org_with_role_derives() {
        // Verify OrgWithRole can be serialized/deserialized
        let owr = OrgWithRole {
            id: Uuid::nil(),
            name: "Test".to_string(),
            slug: "test".to_string(),
            role: "admin".to_string(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&owr).unwrap();
        assert!(json.contains("test"));
    }

    #[test]
    fn test_create_org_request_serialization() {
        let req = CreateOrgRequest {
            name: "Acme".to_string(),
            slug: Some("acme".to_string()),
            kind: OrgKind::Tenant,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Acme"));
        assert!(json.contains("acme"));
        assert!(json.contains(r#""kind":"tenant""#));
        let defaulted: CreateOrgRequest = serde_json::from_str(r#"{"name":"Acme"}"#).unwrap();
        assert_eq!(
            defaulted.kind,
            OrgKind::Tenant,
            "requests default to a tenant"
        );
    }

    #[test]
    fn reserved_slugs_are_refused_for_tenants() {
        for slug in RESERVED_ORG_SLUGS {
            assert!(reject_reserved_slug(slug).is_err(), "{slug} is reserved");
        }
        assert!(reject_reserved_slug("acme").is_ok());
        assert!(reject_reserved_slug("operators").is_ok());
    }

    #[test]
    fn test_invite_member_request_serialization() {
        let req = InviteMemberRequest {
            email: "user@example.com".to_string(),
            role: "member".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("user@example.com"));
        assert!(json.contains("member"));
    }

    #[test]
    fn test_org_member_dto_derives() {
        let dto = OrgMemberDTO {
            user_id: Uuid::nil(),
            email: "test@test.com".to_string(),
            display_name: "Test".to_string(),
            avatar_url: None,
            role: "member".to_string(),
            joined_at: Utc::now(),
            deactivated_at: None,
            activity_revision: 0,
            shutdown_pending: false,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("test@test.com"));
    }

    #[test]
    fn test_update_org_request_both_none() {
        let req = UpdateOrgRequest {
            name: None,
            slug: None,
        };
        assert!(req.name.is_none());
        assert!(req.slug.is_none());
    }

    #[test]
    fn test_invite_member_response_serialization() {
        let resp = InviteMemberResponse {
            membership: None,
            invitation: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("null"));
    }
}
