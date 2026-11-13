//! Approval definition service — org-scoped reads/writes of the append-only
//! `approval_definitions` table (migration 186, B6 governance PR-1).
//!
//! Definitions only: no config attachment, no evaluation, no status writes.
//! The service never issues UPDATE or DELETE — edits append the next version;
//! the migration's grant surface (tenant SELECT+INSERT) and RLS backstop that
//! invariant at the database layer.

use g5_authz::ScopedDb;
use g5_contracts::claude_models::{claude_model_catalog, CLAUDE_DIRECT_CHAT_DEFAULT_MODEL};
use g5_contracts::codex_models::codex_model_catalog;
use g5_contracts::error::G5Error;
use g5_contracts::types::is_approval_phase;
use sqlx::PgPool;
use uuid::Uuid;

/// One human approver on a definition version, profile-enriched.
#[derive(Debug, Clone)]
pub struct ApprovalHumanRecord {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
    /// False when the user remains in the version's `humans_list` but is no
    /// longer an org member. Departed approvers are returned flagged — a
    /// version's approver set is historical fact and is never silently
    /// dropped.
    pub still_member: bool,
    /// Activity is separate from membership and historical attribution.
    pub deactivated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A definition at its latest version, with v1 ("created by") provenance.
#[derive(Debug, Clone)]
pub struct ApprovalDefinitionRecord {
    pub id: Uuid,
    pub latest_version: i32,
    pub name: String,
    pub description: Option<String>,
    pub humans: Vec<ApprovalHumanRecord>,
    pub agent_type: Option<String>,
    pub agent_prompt: Option<String>,
    pub agent_harness: Option<String>,
    pub agent_model: Option<String>,
    pub agent_effort: Option<String>,
    pub invalidates_previous: bool,
    /// Earliest-relevant phase recommendation; legacy multiple/empty
    /// associations remain readable and never restrict merge requirements.
    pub phases: Vec<String>,
    pub require_all_reviewers: bool,
    pub allow_self_approval: bool,
    pub human_approvals_override_agent: bool,
    pub created_by: Uuid,
    pub created_by_display_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub first_created_by: Uuid,
    pub first_created_by_display_name: String,
    pub first_created_at: chrono::DateTime<chrono::Utc>,
}

/// Create-v1 request (application-level shape; wire DTOs live in the route).
#[derive(Debug, Clone)]
pub struct CreateApprovalDefinitionRequest {
    pub name: String,
    pub description: Option<String>,
    pub human_user_ids: Vec<Uuid>,
    pub agent_type: Option<String>,
    pub agent_prompt: Option<String>,
    pub agent_harness: Option<String>,
    pub agent_model: Option<String>,
    pub agent_effort: Option<String>,
    /// Phases to bind; `None` resolves to the Integration Engineering
    /// default (`["ie"]`), the column default.
    pub phases: Option<Vec<String>>,
    pub require_all_reviewers: bool,
    pub allow_self_approval: bool,
    pub human_approvals_override_agent: bool,
}

/// Publish-next-version request.
#[derive(Debug, Clone)]
pub struct PublishApprovalDefinitionVersionRequest {
    pub name: String,
    pub description: Option<String>,
    pub human_user_ids: Vec<Uuid>,
    pub agent_type: Option<String>,
    pub agent_prompt: Option<String>,
    pub agent_harness: Option<String>,
    pub agent_model: Option<String>,
    pub agent_effort: Option<String>,
    pub invalidates_previous: bool,
    /// Phases to bind; `None` resolves to the Integration Engineering
    /// default (`["ie"]`).
    pub phases: Option<Vec<String>>,
    pub require_all_reviewers: bool,
    pub allow_self_approval: bool,
    pub human_approvals_override_agent: bool,
}

/// Row shape shared by the latest-version listing and single-definition fetch.
#[derive(Debug, sqlx::FromRow)]
struct DefinitionRow {
    id: Uuid,
    version: i32,
    name: String,
    description: Option<String>,
    humans_list: Vec<Uuid>,
    agent_type: Option<String>,
    agent_prompt: Option<String>,
    agent_harness: Option<String>,
    agent_model: Option<String>,
    agent_effort: Option<String>,
    invalidates_previous: bool,
    phases: Vec<String>,
    require_all_reviewers: bool,
    allow_self_approval: bool,
    human_approvals_override_agent: bool,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    first_created_by: Uuid,
    first_created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct HumanProfileRow {
    id: Uuid,
    display_name: String,
    email: String,
    avatar_url: Option<String>,
    still_member: bool,
    deactivated_at: Option<chrono::DateTime<chrono::Utc>>,
}

const LATEST_WITH_PROVENANCE_SQL: &str = r#"
    WITH latest AS (
        SELECT DISTINCT ON (id) *
        FROM approval_definitions
        WHERE org_id = $1
        ORDER BY id, version DESC
    ),
    first AS (
        SELECT id, created_by, created_at
        FROM approval_definitions
        WHERE org_id = $1 AND version = 1
    )
    SELECT l.id, l.version, l.name, l.description, l.humans_list,
           l.agent_type, l.agent_prompt,
           l.agent_harness, l.agent_model, l.agent_effort,
           l.invalidates_previous, l.phases, l.require_all_reviewers, l.allow_self_approval, l.human_approvals_override_agent,
           l.created_by, l.created_at,
           f.created_by AS first_created_by,
           f.created_at AS first_created_at
    FROM latest l
    JOIN first f ON f.id = l.id
"#;

/// Stateless service registered on `Services`, mirroring `OrgService`.
#[derive(Debug, Clone, Default)]
pub struct ApprovalDefinitionService;

impl ApprovalDefinitionService {
    pub fn new() -> Self {
        Self
    }

    /// List every definition in the org at its latest version, name ASC,
    /// with v1 provenance and profile-enriched approvers.
    pub async fn list_latest(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        org_id: Uuid,
    ) -> Result<Vec<ApprovalDefinitionRecord>, G5Error> {
        let sql = format!("{LATEST_WITH_PROVENANCE_SQL} ORDER BY l.name ASC, l.id");
        let rows: Vec<DefinitionRow> = sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to list approval definitions: {e}"),
            })?;

        self.enrich(system_pool, org_id, rows).await
    }

    /// Fetch one definition (latest version) in the same shape as the list.
    pub async fn get_latest(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        org_id: Uuid,
        definition_id: Uuid,
    ) -> Result<ApprovalDefinitionRecord, G5Error> {
        let sql = format!("{LATEST_WITH_PROVENANCE_SQL} WHERE l.id = $2");
        let row: Option<DefinitionRow> = sqlx::query_as(&sql)
            .bind(org_id)
            .bind(definition_id)
            .fetch_optional(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to fetch approval definition: {e}"),
            })?;
        let row = row.ok_or_else(|| G5Error::NotFound {
            resource: format!("approval definition {definition_id}"),
        })?;

        let mut records = self.enrich(system_pool, org_id, vec![row]).await?;
        Ok(records.remove(0))
    }

    /// Create version 1 of a new definition. `invalidates_previous` is always
    /// false at v1 — there is nothing to invalidate.
    pub async fn create(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        org_id: Uuid,
        actor_user_id: Uuid,
        req: CreateApprovalDefinitionRequest,
    ) -> Result<ApprovalDefinitionRecord, G5Error> {
        let phases = resolve_phase_binding(req.phases)?;
        let name = validate_definition_shape(
            &req.name,
            req.description.as_deref(),
            &req.human_user_ids,
            req.agent_type.as_deref(),
            req.agent_prompt.as_deref(),
        )?;
        let (agent_harness, agent_model, agent_effort) = resolve_agent_launch_policy(
            req.agent_type.as_deref(),
            req.agent_harness.as_deref(),
            req.agent_model.as_deref(),
            req.agent_effort.as_deref(),
        )?;
        self.validate_humans_are_active_members(db, org_id, &req.human_user_ids)
            .await?;

        let definition_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO approval_definitions
                (version, org_id, name, description, humans_list,
                 agent_type, agent_prompt, agent_harness, agent_model,
                 agent_effort, invalidates_previous, created_by, phases,
                 require_all_reviewers, allow_self_approval, human_approvals_override_agent)
            VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, $9, false, $10, $11, $12, $13, $14)
            RETURNING id
            "#,
        )
        .bind(org_id)
        .bind(&name)
        .bind(normalized_description(req.description.as_deref()))
        .bind(&req.human_user_ids)
        .bind(req.agent_type.as_deref())
        .bind(req.agent_prompt.as_deref().map(str::trim))
        .bind(agent_harness.as_deref())
        .bind(agent_model.as_deref())
        .bind(agent_effort.as_deref())
        .bind(actor_user_id)
        .bind(&phases)
        .bind(req.require_all_reviewers)
        .bind(req.allow_self_approval)
        .bind(req.human_approvals_override_agent)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to create approval definition: {e}"),
        })?;

        self.get_latest(db, system_pool, org_id, definition_id)
            .await
    }

    /// Append the next version of an existing definition.
    ///
    /// The next version number is computed and inserted in a single statement
    /// guarded by the `(id, version)` primary key; a raced concurrent publish
    /// inserts zero rows and surfaces as `Conflict` (HTTP 409) with nothing
    /// written. There is deliberately no silent retry: the loser's content was
    /// never reviewed against the winner's version.
    pub async fn publish_version(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        org_id: Uuid,
        definition_id: Uuid,
        actor_user_id: Uuid,
        req: PublishApprovalDefinitionVersionRequest,
    ) -> Result<ApprovalDefinitionRecord, G5Error> {
        let phases = resolve_phase_binding(req.phases)?;
        let name = validate_definition_shape(
            &req.name,
            req.description.as_deref(),
            &req.human_user_ids,
            req.agent_type.as_deref(),
            req.agent_prompt.as_deref(),
        )?;
        let (agent_harness, agent_model, agent_effort) = resolve_agent_launch_policy(
            req.agent_type.as_deref(),
            req.agent_harness.as_deref(),
            req.agent_model.as_deref(),
            req.agent_effort.as_deref(),
        )?;
        self.validate_humans_are_active_members(db, org_id, &req.human_user_ids)
            .await?;

        // Approval policy lock protocol step 2 (see
        // `approval_config_service::lock_target_requirement_set`): a new
        // version serializes with the admissions and decisions reading this
        // definition, taken before the bounded lock wait below.
        crate::services::workspace::approval_config_service::lock_approval_definition(
            db.conn(),
            org_id,
            definition_id,
        )
        .await?;

        let exists: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM approval_definitions WHERE org_id = $1 AND id = $2 LIMIT 1",
        )
        .bind(org_id)
        .bind(definition_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check approval definition existence: {e}"),
        })?;
        if exists.is_none() {
            return Err(G5Error::NotFound {
                resource: format!("approval definition {definition_id}"),
            });
        }

        // Bound the conflict wait: the ON CONFLICT insert blocks on a raced
        // publisher's uncommitted (id, version) key until that transaction
        // resolves. A publish-specific lock timeout keeps "fails fast"
        // truthful — a lock timeout is the same user-visible outcome as
        // losing the race and maps to the same 409.
        sqlx::query("SET LOCAL lock_timeout = '2s'")
            .execute(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to bound publish lock wait: {e}"),
            })?;

        let inserted: Option<i32> = sqlx::query_scalar(
            r#"
            INSERT INTO approval_definitions
                (id, version, org_id, name, description, humans_list,
                 agent_type, agent_prompt, agent_harness, agent_model,
                 agent_effort, invalidates_previous, created_by, phases,
                 require_all_reviewers, allow_self_approval, human_approvals_override_agent)
            SELECT $2, MAX(version) + 1, $1, $3, $4, $5, $6, $7, $8, $9,
                   $10, $11, $12, $13, $14, $15, $16
            FROM approval_definitions
            WHERE org_id = $1 AND id = $2
            HAVING MAX(version) IS NOT NULL
            ON CONFLICT (id, version) DO NOTHING
            RETURNING version
            "#,
        )
        .bind(org_id)
        .bind(definition_id)
        .bind(&name)
        .bind(normalized_description(req.description.as_deref()))
        .bind(&req.human_user_ids)
        .bind(req.agent_type.as_deref())
        .bind(req.agent_prompt.as_deref().map(str::trim))
        .bind(agent_harness.as_deref())
        .bind(agent_model.as_deref())
        .bind(agent_effort.as_deref())
        .bind(req.invalidates_previous)
        .bind(actor_user_id)
        .bind(&phases)
        .bind(req.require_all_reviewers)
        .bind(req.allow_self_approval)
        .bind(req.human_approvals_override_agent)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| {
            // 55P03 lock_not_available: the bounded wait expired while a
            // concurrent publish held the next version uncommitted — the
            // same user-visible outcome as losing the race.
            let lock_timeout = e
                .as_database_error()
                .and_then(|db_err| db_err.code())
                .is_some_and(|code| code == "55P03");
            if lock_timeout {
                G5Error::Conflict {
                    reason: "A concurrent publish of this approval definition is in \
                             progress; reload the latest version and re-apply your edit"
                        .to_string(),
                }
            } else {
                G5Error::Internal {
                    message: format!("Failed to publish approval definition version: {e}"),
                }
            }
        })?;

        if inserted.is_none() {
            return Err(G5Error::Conflict {
                reason: "A new version of this approval definition was published concurrently; \
                         reload the latest version and re-apply your edit"
                    .to_string(),
            });
        }

        self.get_latest(db, system_pool, org_id, definition_id)
            .await
    }

    /// Profile-enrich `humans_list` for a batch of definition rows, preserving
    /// each list's stored order.
    async fn enrich(
        &self,
        pool: &PgPool,
        org_id: Uuid,
        rows: Vec<DefinitionRow>,
    ) -> Result<Vec<ApprovalDefinitionRecord>, G5Error> {
        let mut all_ids: Vec<Uuid> = rows.iter().flat_map(|r| r.humans_list.clone()).collect();
        all_ids.extend(rows.iter().map(|r| r.created_by));
        all_ids.extend(rows.iter().map(|r| r.first_created_by));
        all_ids.sort();
        all_ids.dedup();

        let profiles: Vec<HumanProfileRow> = if all_ids.is_empty() {
            Vec::new()
        } else {
            sqlx::query_as(
                r#"
                SELECT u.id, u.display_name, u.email, u.avatar_url,
                       EXISTS (
                           SELECT 1 FROM org_memberships m
                           WHERE m.org_id = $1 AND m.user_id = u.id
                       ) AS still_member,
                       (SELECT m.deactivated_at FROM org_memberships m
                        WHERE m.org_id = $1 AND m.user_id = u.id) AS deactivated_at
                FROM users u
                WHERE u.id = ANY($2)
                "#,
            )
            .bind(org_id)
            .bind(&all_ids)
            .fetch_all(pool)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to enrich approval definition humans: {e}"),
            })?
        };
        let by_id: std::collections::HashMap<Uuid, &HumanProfileRow> =
            profiles.iter().map(|p| (p.id, p)).collect();

        Ok(rows
            .into_iter()
            .map(|r| {
                let humans = r
                    .humans_list
                    .iter()
                    .map(|uid| match by_id.get(uid) {
                        Some(p) => ApprovalHumanRecord {
                            user_id: p.id,
                            display_name: p.display_name.clone(),
                            email: p.email.clone(),
                            avatar_url: p.avatar_url.clone(),
                            still_member: p.still_member,
                            deactivated_at: p.deactivated_at,
                        },
                        // A humans_list entry with no users row should be
                        // impossible (FK-less array, but ids come from
                        // members); render it inert rather than erroring the
                        // whole library read.
                        None => ApprovalHumanRecord {
                            user_id: *uid,
                            display_name: "Unknown user".to_string(),
                            email: String::new(),
                            avatar_url: None,
                            still_member: false,
                            deactivated_at: None,
                        },
                    })
                    .collect();
                let display_name_of = |uid: Uuid| {
                    by_id
                        .get(&uid)
                        .map(|p| p.display_name.clone())
                        .unwrap_or_else(|| "Unknown user".to_string())
                };
                ApprovalDefinitionRecord {
                    id: r.id,
                    latest_version: r.version,
                    name: r.name,
                    description: r.description,
                    humans,
                    agent_type: r.agent_type,
                    agent_prompt: r.agent_prompt,
                    agent_harness: r.agent_harness,
                    agent_model: r.agent_model,
                    agent_effort: r.agent_effort,
                    invalidates_previous: r.invalidates_previous,
                    phases: r.phases,
                    require_all_reviewers: r.require_all_reviewers,
                    allow_self_approval: r.allow_self_approval,
                    human_approvals_override_agent: r.human_approvals_override_agent,
                    created_by: r.created_by,
                    created_by_display_name: display_name_of(r.created_by),
                    created_at: r.created_at,
                    first_created_by: r.first_created_by,
                    first_created_by_display_name: display_name_of(r.first_created_by),
                    first_created_at: r.first_created_at,
                }
            })
            .collect())
    }

    /// New versions can only assign active members; old versions remain intact.
    async fn validate_humans_are_active_members(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        human_user_ids: &[Uuid],
    ) -> Result<(), G5Error> {
        if human_user_ids.is_empty() {
            return Ok(());
        }
        let member_ids = lock_active_approval_recipients(db, org_id, human_user_ids).await?;
        let missing: Vec<String> = human_user_ids
            .iter()
            .filter(|id| !member_ids.contains(id))
            .map(|id| id.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "humanUserIds": format!(
                        "not active members of this org: {}",
                        missing.join(", ")
                    )
                }),
            });
        }
        Ok(())
    }
}

/// Hold recipient activity through assignment commit. The verified-context
/// lookup also works for tenant connections that cannot lock membership rows
/// directly. A concurrent deactivation either precedes validation or waits
/// for this assignment to finish; history is never removed.
pub(crate) async fn lock_active_approval_recipients(
    db: &mut ScopedDb,
    org_id: Uuid,
    user_ids: &[Uuid],
) -> Result<Vec<Uuid>, G5Error> {
    let mut ordered_ids = user_ids.to_vec();
    ordered_ids.sort_unstable();
    ordered_ids.dedup();
    let mut active_ids = Vec::with_capacity(ordered_ids.len());
    for user_id in ordered_ids {
        let active: Option<bool> = sqlx::query_scalar(
            "SELECT deactivated_at IS NULL FROM authz.lock_org_membership($1, $2)",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(db.conn())
        .await?;
        if active == Some(true) {
            active_ids.push(user_id);
        }
    }
    Ok(active_ids)
}

/// Input bounds for definition text fields and the approver set. The
/// migration-186 CHECKs bound emptiness/shape but not length, so the service
/// caps sizes here with typed 422s — write-path enforcement is sufficient
/// because read paths only ever return previously validated rows.
const MAX_NAME_LEN: usize = 200;
const MAX_DESCRIPTION_LEN: usize = 4_000;
const MAX_AGENT_PROMPT_LEN: usize = 20_000;
const MAX_HUMAN_APPROVERS: usize = 50;

const DEFAULT_AGENT_HARNESS: &str = "claude_code";
const DEFAULT_AGENT_EFFORT: &str = "high";

type ResolvedAgentLaunchPolicy = (Option<String>, Option<String>, Option<String>);

/// Resolve omitted launch fields for legacy callers and validate exact values
/// against the same provider model catalogs used by the other launch surfaces.
/// The returned tuple is all-null for human-only definitions and all-explicit
/// for agent-backed definitions; Mode is deliberately absent because it is
/// harness-forced execution policy.
fn resolve_agent_launch_policy(
    agent_type: Option<&str>,
    agent_harness: Option<&str>,
    agent_model: Option<&str>,
    agent_effort: Option<&str>,
) -> Result<ResolvedAgentLaunchPolicy, G5Error> {
    if agent_type.is_none() {
        if agent_harness.is_some() || agent_model.is_some() || agent_effort.is_some() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "agentHarness": "agent launch fields require agentType 'simple'"
                }),
            });
        }
        return Ok((None, None, None));
    }

    let provided_count = [agent_harness, agent_model, agent_effort]
        .into_iter()
        .flatten()
        .count();
    if !matches!(provided_count, 0 | 3) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "agentHarness": "agentHarness, agentModel, and agentEffort must be supplied together"
            }),
        });
    }
    let (harness, model, effort) = if provided_count == 0 {
        (
            DEFAULT_AGENT_HARNESS,
            CLAUDE_DIRECT_CHAT_DEFAULT_MODEL,
            DEFAULT_AGENT_EFFORT,
        )
    } else {
        let harness = agent_harness
            .expect("count proves harness is present")
            .trim();
        let model = agent_model.expect("count proves model is present").trim();
        let effort = agent_effort.expect("count proves effort is present").trim();
        if harness.is_empty() || model.is_empty() || effort.is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "agentHarness": "agentHarness, agentModel, and agentEffort must be non-empty"
                }),
            });
        }
        (harness, model, effort)
    };

    let supported_efforts = match harness {
        "claude_code" => claude_model_catalog()
            .iter()
            .find(|entry| {
                (entry.openrouter_offered && entry.openrouter == model)
                    || entry
                        .variants
                        .iter()
                        .any(|variant| variant.offered && variant.id == model)
            })
            .map(|entry| entry.efforts),
        "codex" => codex_model_catalog()
            .iter()
            .find(|entry| {
                entry.offered && (entry.direct == model || entry.openrouter == Some(model))
            })
            .map(|entry| entry.efforts),
        other => {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "agentHarness": format!(
                        "unsupported agent harness '{other}'; expected 'claude_code' or 'codex'"
                    )
                }),
            });
        }
    };

    let Some(supported_efforts) = supported_efforts else {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "agentModel": format!("model '{model}' is not offered for agent '{harness}'")
            }),
        });
    };
    if !supported_efforts.contains(&effort) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "agentEffort": format!(
                    "effort '{effort}' is not supported by model '{model}'"
                )
            }),
        });
    }

    Ok((
        Some(harness.to_string()),
        Some(model.to_string()),
        Some(effort.to_string()),
    ))
}

/// Shared shape validation for create and publish. Mirrors (and precedes) the
/// migration-186 CHECKs so users get typed 422 field errors instead of
/// constraint-violation 500s. Returns the trimmed name.
fn validate_definition_shape(
    name: &str,
    description: Option<&str>,
    human_user_ids: &[Uuid],
    agent_type: Option<&str>,
    agent_prompt: Option<&str>,
) -> Result<String, G5Error> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({ "name": "must not be blank" }),
        });
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "name": format!("must be at most {MAX_NAME_LEN} characters")
            }),
        });
    }

    if description.map(|d| d.trim().chars().count()).unwrap_or(0) > MAX_DESCRIPTION_LEN {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "description": format!("must be at most {MAX_DESCRIPTION_LEN} characters")
            }),
        });
    }
    if agent_prompt.map(|p| p.trim().chars().count()).unwrap_or(0) > MAX_AGENT_PROMPT_LEN {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "agentPrompt": format!("must be at most {MAX_AGENT_PROMPT_LEN} characters")
            }),
        });
    }

    if human_user_ids.len() > MAX_HUMAN_APPROVERS {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "humanUserIds": format!("at most {MAX_HUMAN_APPROVERS} approvers are supported")
            }),
        });
    }

    let mut deduped = human_user_ids.to_vec();
    deduped.sort();
    deduped.dedup();
    if deduped.len() != human_user_ids.len() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({ "humanUserIds": "must not contain duplicates" }),
        });
    }

    match (agent_type, agent_prompt) {
        (None, None) => {}
        (Some("simple"), Some(prompt)) if !prompt.trim().is_empty() => {}
        (Some("simple"), _) => {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "agentPrompt": "an agent approver requires a non-empty prompt"
                }),
            });
        }
        (Some(other), _) => {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "agentType": format!("unsupported agent type '{other}'; only 'simple' exists")
                }),
            });
        }
        (None, Some(_)) => {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "agentPrompt": "an agent prompt requires agentType 'simple'"
                }),
            });
        }
    }

    if human_user_ids.is_empty() && agent_type.is_none() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "humanUserIds": "select at least one human approver or add an agent approver"
            }),
        });
    }

    Ok(trimmed.to_string())
}

/// Resolve the optional phase binding (g5.workbench.gates): omitted means
/// the Integration Engineering default; present values must be workbench
/// phases without duplicates (mirrors the migration-066 CHECK so a bad
/// binding is a typed 422, not a constraint-violation 500). An empty list is
/// allowed and means attached but inert.
fn resolve_phase_binding(phases: Option<Vec<String>>) -> Result<Vec<String>, G5Error> {
    let Some(phases) = phases else {
        return Ok(vec!["ie".to_string()]);
    };
    let mut seen = std::collections::HashSet::new();
    for phase in &phases {
        if !is_approval_phase(phase) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "phases": format!("'{phase}' is not a workbench phase (be, se, ie)")
                }),
            });
        }
        if !seen.insert(phase.as_str()) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({ "phases": "must not contain duplicates" }),
            });
        }
    }
    Ok(phases)
}

fn normalized_description(description: Option<&str>) -> Option<String> {
    description
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_launch_policy_defaults_are_exact_and_mode_is_not_stored() {
        assert_eq!(
            resolve_agent_launch_policy(Some("simple"), None, None, None).unwrap(),
            (
                Some("claude_code".to_string()),
                Some("claude-opus-5-5".to_string()),
                Some("high".to_string()),
            )
        );
    }

    #[test]
    fn agent_launch_policy_accepts_catalog_supported_explicit_codex_values() {
        assert_eq!(
            resolve_agent_launch_policy(
                Some("simple"),
                Some("codex"),
                Some("gpt-5.6-sol"),
                Some("ultra"),
            )
            .unwrap(),
            (
                Some("codex".to_string()),
                Some("gpt-5.6-sol".to_string()),
                Some("ultra".to_string()),
            )
        );
    }

    #[test]
    fn agent_launch_policy_rejects_partial_or_human_only_launch_fields() {
        assert!(resolve_agent_launch_policy(
            Some("simple"),
            Some("claude_code"),
            None,
            Some("high"),
        )
        .is_err());
        assert!(resolve_agent_launch_policy(
            None,
            Some("claude_code"),
            Some("claude-opus-5-5"),
            Some("high"),
        )
        .is_err());
    }

    #[test]
    fn agent_launch_policy_rejects_unsupported_model_effort_pair() {
        assert!(resolve_agent_launch_policy(
            Some("simple"),
            Some("claude_code"),
            Some("claude-sonnet-4-6"),
            Some("ultra"),
        )
        .is_err());
    }
}
