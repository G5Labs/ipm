//! Personal workspaces (`g5.personal.*`, G5-Lean#624): one per eligible
//! (organization, user), with a single private providerless repository.
//!
//! Creation is one idempotent operation keyed by (organization, user):
//! [`ensure`] inserts the registry row, private repository, member row and
//! settings in one transaction, then initializes the branch. A retry, the
//! reconciler or `/{org}/me` completes a partial creation. Retirement runs in
//! the transaction of the membership change that removes or demotes the owner
//! ([`retire_in_tx`]); the VM stop follows that commit.

use std::sync::Arc;

use g5_contracts::error::G5Error;
use g5_contracts::types::AuthorityTarget;
use serde::Serialize;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_system_org;
use crate::services::git::branch_commit::{
    map_branch_commit_error, prepare_initial_branch_commit_in_tx, InitialWorkspaceHeadInput,
};
use crate::services::git::repo_store::ScopedRepoStorage;
use crate::services::workspace_names::PERSONAL_WORKSPACE_NAME_PREFIX;
use crate::services::AppState;

/// Root path of the private repository inside the VM's workspace root. The
/// agent launches here, so Claude's native memory binds to it.
pub const PERSONAL_ROOT_PATH: &str = "personal";
/// Repository name shown for the private repository.
pub const PERSONAL_REPO_NAME: &str = "personal";
/// Longest handle, before any collision suffix is applied.
const MAX_HANDLE_LEN: usize = 64;
/// Handle candidates tried before giving up.
const MAX_HANDLE_ATTEMPTS: usize = 1000;

/// A live personal workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PersonalWorkspace {
    pub workspace_id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub repo_id: Uuid,
    pub name: String,
}

impl PersonalWorkspace {
    /// The handle part of `users/<handle>`.
    pub fn handle(&self) -> &str {
        self.name
            .strip_prefix(PERSONAL_WORKSPACE_NAME_PREFIX)
            .unwrap_or(&self.name)
    }
}

/// Derive the base handle from an email (`g5.personal.names`): the local part,
/// lowercased, with characters outside `a-z`, `0-9` and `-` mapped to `-`,
/// repeated `-` collapsed, leading and trailing `-` dropped, at most 64
/// characters, and `user` when nothing remains.
pub fn derive_handle(email: &str) -> String {
    let local = email.split('@').next().unwrap_or_default();
    let mut handle = String::with_capacity(local.len());
    for ch in local.chars().flat_map(char::to_lowercase) {
        let mapped = if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            ch
        } else {
            '-'
        };
        if mapped == '-' && (handle.is_empty() || handle.ends_with('-')) {
            continue;
        }
        handle.push(mapped);
    }
    let mut handle = handle.trim_end_matches('-').to_string();
    handle.truncate(MAX_HANDLE_LEN);
    let handle = handle.trim_end_matches('-').to_string();
    if handle.is_empty() {
        "user".to_string()
    } else {
        handle
    }
}

/// The `attempt`-th handle candidate: the base, then `base-2`, `base-3`, …
pub fn handle_candidate(base: &str, attempt: usize) -> String {
    if attempt <= 1 {
        base.to_string()
    } else {
        format!("{base}-{attempt}")
    }
}

const SELECT_PERSONAL: &str = "SELECT workspace.id AS workspace_id, workspace.org_id,
            workspace.personal_user_id AS user_id, member.repo_id, workspace.name
     FROM workspaces AS workspace
     JOIN workspace_repos AS member
       ON member.org_id = workspace.org_id AND member.workspace_id = workspace.id
     WHERE workspace.deleted_at IS NULL AND workspace.personal_user_id IS NOT NULL";

fn scope_error(error: g5_authz::ScopedDbError) -> G5Error {
    G5Error::Internal {
        message: format!("Personal workspace database scope failed: {error}"),
    }
}

fn db_error(context: &'static str) -> impl FnOnce(sqlx::Error) -> G5Error {
    move |error| {
        if g5_contracts::error::sqlx_error_is_retryable(&error) {
            error.into()
        } else {
            G5Error::Internal {
                message: format!("{context}: {error}"),
            }
        }
    }
}

/// The caller's live personal workspace in `org_id`, if any.
pub async fn find_live(
    conn: &mut PgConnection,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<Option<PersonalWorkspace>, G5Error> {
    sqlx::query_as::<_, PersonalWorkspace>(&format!(
        "{SELECT_PERSONAL} AND workspace.org_id = $1 AND workspace.personal_user_id = $2"
    ))
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await
    .map_err(db_error("Failed to load personal workspace"))
}

/// The live personal workspace named `users/<handle>` in `org_id`, matched
/// by registry kind and name, never by name alone.
pub async fn find_by_handle(
    conn: &mut PgConnection,
    org_id: Uuid,
    handle: &str,
) -> Result<Option<PersonalWorkspace>, G5Error> {
    sqlx::query_as::<_, PersonalWorkspace>(&format!(
        "{SELECT_PERSONAL} AND workspace.org_id = $1 AND lower(workspace.name) = lower($2)"
    ))
    .bind(org_id)
    .bind(format!("{PERSONAL_WORKSPACE_NAME_PREFIX}{handle}"))
    .fetch_optional(conn)
    .await
    .map_err(db_error("Failed to resolve personal workspace handle"))
}

/// Whether `user_id` may own workspaces in `org_id` now.
async fn is_owner_eligible(
    system_pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<bool, G5Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1 FROM org_memberships
             WHERE org_id = $1 AND user_id = $2 AND role IN ('admin', 'member'))",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(system_pool)
    .await
    .map_err(db_error("Failed to check personal workspace eligibility"))
}

/// Insert the personal workspace's rows unless a live one exists. Returns the
/// workspace and whether this call created it, or `None` when the user is not
/// an eligible owner. The caller commits.
///
/// Name choice is serialized per organization, so concurrent creations never
/// pick the same handle; ordinary admission refuses `users/` names, so no fork
/// or rename competes for them. The owner foreign key, checked at commit, refuses a
/// creation that races the owner's removal or demotion.
pub async fn ensure_records(
    conn: &mut PgConnection,
    system_pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<Option<(PersonalWorkspace, bool)>, G5Error> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('g5:personal-names:' || $1::text, 0))",
    )
    .bind(org_id)
    .execute(&mut *conn)
    .await
    .map_err(db_error("Failed to serialize personal workspace naming"))?;
    if let Some(existing) = find_live(conn, org_id, user_id).await? {
        return Ok(Some((existing, false)));
    }
    if !is_owner_eligible(system_pool, org_id, user_id).await? {
        return Ok(None);
    }
    let email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(system_pool)
        .await
        .map_err(db_error("Failed to load personal workspace user"))?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("user {user_id}"),
        })?;
    let base = derive_handle(&email);
    let mut name = None;
    for attempt in 1..=MAX_HANDLE_ATTEMPTS {
        let candidate = format!(
            "{PERSONAL_WORKSPACE_NAME_PREFIX}{}",
            handle_candidate(&base, attempt)
        );
        // A current, former or claimed name is taken (`g5.workspace.former-names`).
        let holder = crate::services::workspace_names::name_holder(conn, org_id, &candidate, None)
            .await
            .map_err(db_error("Failed to check personal workspace name"))?;
        if holder.is_none() {
            name = Some(candidate);
            break;
        }
    }
    let name = name.ok_or_else(|| G5Error::Conflict {
        reason: "No free personal workspace name was found".to_string(),
    })?;

    let workspace_id = Uuid::now_v7();
    let repo_id = Uuid::now_v7();
    // Slugs are unique per organization; the name stays `personal`.
    let repo_slug = format!("personal-{workspace_id}");
    // The tenant role cannot read `orgs`; the slug only names the URL.
    let org_slug: String = sqlx::query_scalar("SELECT slug FROM orgs WHERE id = $1")
        .bind(org_id)
        .fetch_one(system_pool)
        .await
        .map_err(db_error("Failed to read the organization slug"))?;
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
         VALUES ($1, $2, $3, $6, $4, $5, now())",
    )
    .bind(repo_id)
    .bind(org_id)
    .bind(PERSONAL_REPO_NAME)
    .bind(crate::services::identity::org_service::local_repo_git_url(
        &org_slug, &repo_slug,
    ))
    .bind(&name)
    .bind(&repo_slug)
    .execute(&mut *conn)
    .await
    .map_err(db_error("Failed to insert personal repository"))?;
    sqlx::query(
        "INSERT INTO workspaces (
             id, org_id, name, is_base, docs_repo_id, push_mode,
             owner_user_id, created_by, personal_user_id, created_at, updated_at)
         VALUES ($1, $2, $3, false, NULL, 'manual', $4, $4, $4, now(), now())",
    )
    .bind(workspace_id)
    .bind(org_id)
    .bind(&name)
    .bind(user_id)
    .execute(&mut *conn)
    .await
    .map_err(db_error("Failed to insert personal workspace"))?;
    sqlx::query(
        "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role)
         VALUES ($1, $2, $3, $4, 'code')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(repo_id)
    .bind(PERSONAL_ROOT_PATH)
    .execute(&mut *conn)
    .await
    .map_err(db_error("Failed to insert personal repository member"))?;
    let idle_timeout_secs =
        crate::services::identity::org_admin_settings::default_personal_idle_timeout_secs(
            conn, org_id,
        )
        .await?;
    sqlx::query(
        "INSERT INTO workspace_settings (org_id, workspace_id, idle_timeout_secs)
         VALUES ($1, $2, $3)",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(idle_timeout_secs)
    .execute(&mut *conn)
    .await
    .map_err(db_error("Failed to insert personal workspace settings"))?;
    tracing::info!(%org_id, %user_id, %workspace_id, %name, "Created personal workspace");
    Ok(Some((
        PersonalWorkspace {
            workspace_id,
            org_id,
            user_id,
            repo_id,
            name,
        },
        true,
    )))
}

/// Give the private repository its first commit on the personal branch, unless
/// the branch already exists. Safe to repeat. `repos` is the store narrowed
/// to a scope the caller already held for the record's organization, so the
/// common already-initialized case opens no transaction.
pub async fn ensure_branch(
    state: &Arc<AppState>,
    repos: &ScopedRepoStorage,
    record: &PersonalWorkspace,
) -> Result<(), G5Error> {
    if branch_initialized(repos, record).await {
        return Ok(());
    }
    let mut db = scoped_db_for_system_org(state, record.org_id, &["workspaces:write"])
        .await
        .map_err(|error| error.inner)?;
    // Concurrent first uses serialize here; the later one finds the branch
    // its predecessor landed.
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('g5:personal-branch:' || $1::text, 0))",
    )
    .bind(record.workspace_id)
    .execute(db.conn())
    .await
    .map_err(db_error(
        "Failed to serialize personal branch initialization",
    ))?;
    if branch_initialized(repos, record).await {
        db.rollback().await.map_err(scope_error)?;
        return Ok(());
    }
    let head = state
        .workspace_service
        .initialize_local_workspace_repo(
            &crate::services::git::repo_store::TenantRepoStorage::key(
                &db,
                record.org_id,
                record.repo_id,
            )?,
            PERSONAL_REPO_NAME,
            &record.name,
        )
        .await?;
    let advance = prepare_initial_branch_commit_in_tx(
        &mut db,
        &state.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: record.workspace_id,
        },
        vec![InitialWorkspaceHeadInput {
            repo_id: record.repo_id,
            proposed_new_head: head,
        }],
        "personal_workspace_initializer",
    )
    .await
    .map_err(map_branch_commit_error)?;
    advance
        .land(db, &state.event_bus, &state.repo_storage)
        .await
        .map_err(G5Error::from)?;
    Ok(())
}

async fn branch_initialized(repos: &ScopedRepoStorage, record: &PersonalWorkspace) -> bool {
    crate::services::git::workspace_reader::read_branch_head_oids(
        repos,
        record.org_id,
        &[(record.repo_id, record.name.clone())],
    )
    .await
    .unwrap_or_default()
    .get(&(record.repo_id, record.name.clone()))
    .is_some_and(Option::is_some)
}

/// Ensure the eligible user's personal workspace exists and its branch is
/// initialized. Returns `None` when the user cannot own workspaces in the
/// organization.
pub async fn ensure(
    state: &Arc<AppState>,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<Option<PersonalWorkspace>, G5Error> {
    let mut db = scoped_db_for_system_org(state, org_id, &["workspaces:write"])
        .await
        .map_err(|error| error.inner)?;
    let Some((record, _created)) =
        ensure_records(db.conn(), &state.system_pool, org_id, user_id).await?
    else {
        db.rollback().await.map_err(scope_error)?;
        return Ok(None);
    };
    let repos = state.repo_storage.scoped(&db);
    db.commit().await.map_err(scope_error)?;
    ensure_branch(state, &repos, &record).await?;
    Ok(Some(record))
}

/// [`ensure`] for a membership change that has already committed: failures
/// are logged, never returned, so creation never blocks the change; the
/// reconciler retries.
pub async fn ensure_after_membership_change(state: &Arc<AppState>, org_id: Uuid, user_id: Uuid) {
    if let Err(error) = ensure(state, org_id, user_id).await {
        tracing::warn!(%org_id, %user_id, %error, "personal workspace creation deferred to the reconciler");
    }
}

/// Organizations a login checks at most. Any beyond it are left to the
/// reconciler and `/{org}/me`.
const LOGIN_ENSURE_ORG_LIMIT: i64 = 20;

/// After a login, which may have accepted invitations
/// (`g5.organization.invites`), ensure the user's personal workspace in each
/// organization they can own workspaces in. Failures are logged, never
/// returned: the login has already succeeded.
pub async fn ensure_after_login(state: &Arc<AppState>, user_id: Uuid) {
    let orgs = match sqlx::query_scalar::<_, Uuid>(
        "SELECT org_id FROM org_memberships
         WHERE user_id = $1 AND role IN ('admin', 'member')
         ORDER BY org_id
         LIMIT $2",
    )
    .bind(user_id)
    .bind(LOGIN_ENSURE_ORG_LIMIT)
    .fetch_all(&state.system_pool)
    .await
    {
        Ok(orgs) => orgs,
        Err(error) => {
            tracing::warn!(%user_id, %error, "personal workspace check after login deferred");
            return;
        }
    };
    for org_id in orgs {
        ensure_after_membership_change(state, org_id, user_id).await;
    }
}

pub use crate::services::identity::org_service::RetiredPersonalWorkspace;

/// Retire the user's live personal workspace inside the membership change's
/// transaction (`g5.personal.retirement`): tombstone it, which releases the
/// owner foreign key the membership change would otherwise violate, withdraw
/// Start consent by advancing the lifecycle generation, and cancel any Start
/// waiting for capacity. The caller stops a running VM after commit with
/// [`crate::services::WorkspaceVmService::stop_retired_personal`]; the
/// lifecycle reconciler retries that stop.
pub async fn retire_in_tx(
    conn: &mut PgConnection,
    org_id: Uuid,
    user_id: Uuid,
    actor_user_id: Uuid,
) -> Result<Option<RetiredPersonalWorkspace>, G5Error> {
    let operation_id = Uuid::now_v7();
    let workspace_id = sqlx::query_scalar::<_, Uuid>(
        "UPDATE workspaces
         SET deleted_at = now(), deleted_by = $3, deletion_operation_id = $4,
             lifecycle_generation = lifecycle_generation + 1, updated_at = now()
         WHERE org_id = $1 AND personal_user_id = $2 AND deleted_at IS NULL
         RETURNING id",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(actor_user_id)
    .bind(operation_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(db_error("Failed to retire personal workspace"))?;
    let Some(workspace_id) = workspace_id else {
        return Ok(None);
    };
    crate::services::workspace::workspace_vm_service::cancel_waiting_start_in_tx(
        conn,
        workspace_id,
    )
    .await
    .map_err(db_error(
        "Failed to cancel retired personal workspace Start",
    ))?;
    tracing::info!(%org_id, %user_id, %workspace_id, "Retired personal workspace");
    Ok(Some(RetiredPersonalWorkspace {
        workspace_id,
        operation_id,
    }))
}

/// After the retiring membership change commits, stop the retired workspace's
/// VM in the background with
/// [`crate::services::WorkspaceVmService::stop_retired_personal`]. A failure
/// is logged, never returned: the lifecycle reconciler retries the stop.
pub fn stop_after_retirement(
    state: &Arc<AppState>,
    org_id: Uuid,
    retired: Option<RetiredPersonalWorkspace>,
) {
    let Some(retired) = retired else {
        return;
    };
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(error) = stop_retired(&state, org_id, retired).await {
            tracing::warn!(
                %org_id,
                workspace_id = %retired.workspace_id,
                %error,
                "retired personal workspace stop deferred to the reconciler"
            );
        }
    });
}

async fn stop_retired(
    state: &Arc<AppState>,
    org_id: Uuid,
    retired: RetiredPersonalWorkspace,
) -> Result<(), G5Error> {
    let db = scoped_db_for_system_org(state, org_id, &["workspaces:write"])
        .await
        .map_err(|error| error.inner)?;
    let scope = db.reopener();
    db.rollback().await.map_err(scope_error)?;
    let minter = state
        .jwt_minter
        .clone()
        .ok_or_else(|| G5Error::ServiceUnavailable {
            message: "workspace lifecycle signing is unavailable".to_string(),
        })?;
    crate::services::WorkspaceVmService::new(crate::services::WorkspaceVmServiceConfig {
        system_pool: state.system_pool.clone(),
        scope,
        hosts: state.preview_tunnels.clone(),
        minter,
        runtime: state.workspace_runtime.clone(),
        agent_work: state.agent_work.clone(),
        repo_storage: state.repo_storage.clone(),
        git_external_url: state.git_external_url.clone(),
        ws_hub: state.event_bus.ws_hub().clone(),
    })
    .stop_retired_personal(retired.workspace_id, retired.operation_id)
    .await?;
    Ok(())
}

/// Create or complete the personal workspaces of up to `limit` eligible
/// members of `org_id` who have none or whose branch is missing. This is also
/// the backfill for members who joined before personal workspaces existed.
pub async fn reconcile_org(
    state: &Arc<AppState>,
    org_id: Uuid,
    limit: i64,
) -> Result<u64, G5Error> {
    // Memberships are readable only on the system pool and workspaces only
    // under the organization's tenant scope, so the difference is taken here.
    let eligible = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM org_memberships
         WHERE org_id = $1 AND role IN ('admin', 'member')
         ORDER BY user_id",
    )
    .bind(org_id)
    .fetch_all(&state.system_pool)
    .await
    .map_err(db_error("Failed to list eligible members"))?;
    let mut db = scoped_db_for_system_org(state, org_id, &["workspaces:read"])
        .await
        .map_err(|error| error.inner)?;
    let covered = sqlx::query_scalar::<_, Uuid>(
        "SELECT personal_user_id FROM workspaces
         WHERE org_id = $1 AND personal_user_id IS NOT NULL AND deleted_at IS NULL",
    )
    .bind(org_id)
    .fetch_all(db.conn())
    .await
    .map_err(db_error("Failed to list personal workspaces"))?
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    // Names an ordinary workspace or fork claim took before `me`, `users`
    // and `users/` were reserved stay as they are; report them.
    let legacy = sqlx::query_scalar::<_, String>(
        "SELECT name FROM workspaces
         WHERE org_id = $1 AND deleted_at IS NULL AND personal_user_id IS NULL AND NOT is_base
           AND (lower(name) IN ('me', 'users') OR lower(name) LIKE 'users/%')
         UNION ALL
         SELECT proposed_name FROM workspace_forks
         WHERE org_id = $1 AND status IN ('pending', 'aborting')
           AND (lower(proposed_name) IN ('me', 'users') OR lower(proposed_name) LIKE 'users/%')",
    )
    .bind(org_id)
    .fetch_all(db.conn())
    .await
    .map_err(db_error(
        "Failed to check reserved personal workspace names",
    ))?;
    if !legacy.is_empty() {
        tracing::warn!(%org_id, names = ?legacy, "workspaces hold names reserved for personal workspaces");
    }
    db.rollback().await.map_err(scope_error)?;
    let missing = eligible
        .into_iter()
        .filter(|user_id| !covered.contains(user_id))
        .take(usize::try_from(limit.clamp(1, 64)).unwrap_or(1))
        .collect::<Vec<_>>();
    let mut completed = 0_u64;
    for user_id in missing {
        match ensure(state, org_id, user_id).await {
            Ok(Some(_)) => completed += 1,
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%org_id, %user_id, %error, "personal workspace creation retry failed");
            }
        }
    }
    // A creation whose branch initialization failed after its rows committed.
    let mut db = scoped_db_for_system_org(state, org_id, &["workspaces:read"])
        .await
        .map_err(|error| error.inner)?;
    let live = sqlx::query_as::<_, PersonalWorkspace>(&format!(
        "{SELECT_PERSONAL} AND workspace.org_id = $1 ORDER BY workspace.created_at DESC LIMIT $2"
    ))
    .bind(org_id)
    .bind(limit.clamp(1, 64))
    .fetch_all(db.conn())
    .await
    .map_err(db_error("Failed to list personal workspaces"))?;
    let repos = state.repo_storage.scoped(&db);
    db.rollback().await.map_err(scope_error)?;
    for record in live {
        if let Err(error) = ensure_branch(state, &repos, &record).await {
            tracing::warn!(org_id = %record.org_id, workspace_id = %record.workspace_id, %error, "personal branch initialization retry failed");
        }
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_come_from_the_normalized_email_local_part() {
        assert_eq!(derive_handle("Alice.Smith@example.com"), "alice-smith");
        assert_eq!(derive_handle("bob+g5@example.com"), "bob-g5");
        assert_eq!(derive_handle("__x__@example.com"), "x");
        assert_eq!(derive_handle("a..b--c@example.com"), "a-b-c");
        assert_eq!(derive_handle("Ünïcode@example.com"), "n-code");
        assert_eq!(derive_handle("@example.com"), "user");
        assert_eq!(derive_handle("___@example.com"), "user");
        assert_eq!(
            derive_handle(&format!("{}@example.com", "a".repeat(80))).len(),
            64
        );
        assert_eq!(
            derive_handle(&format!("{}-b@example.com", "a".repeat(63))),
            "a".repeat(63)
        );
    }

    #[test]
    fn handles_are_valid_branch_components_under_users() {
        for email in [
            "Alice.Smith@example.com",
            "x.lock@example.com",
            "@example.com",
            "a/b@example.com",
        ] {
            let name = format!("{PERSONAL_WORKSPACE_NAME_PREFIX}{}", derive_handle(email));
            assert!(g5_scm::is_valid_ref_name(&name), "{name}");
        }
    }

    #[test]
    fn collisions_append_a_numeric_suffix() {
        assert_eq!(handle_candidate("alice", 1), "alice");
        assert_eq!(handle_candidate("alice", 2), "alice-2");
        assert_eq!(handle_candidate("alice", 17), "alice-17");
    }
}
