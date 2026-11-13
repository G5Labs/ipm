//! Durable reservation and invocation of `agent.turn`.
//!
//! Reservation, attempt identity, immutable policy, and inbox ownership are
//! committed before the workspace RPC.  A crash in that gap leaves an
//! unconfirmed attempt which is reconstructed and invoked with the same
//! canonical `exec_id` on the next maintenance pass.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::task_env::TaskEnvSecretError;
use g5_contracts::types::{
    AgentLaunchPreferencesV1, ChatAgentProvider, PersistedAgentLaunchV1, ResolvedAgentExecutionV1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::errors::AppError;

use crate::services::agents::attempt_snapshot::resolved_launch;
use crate::services::agents::provider_account_service::ProviderAccountService;
use crate::services::agents::task_lifecycle::{canonical_exec_id, TaskLifecycleEvent};
use crate::services::workspace::workspace_runtime::RuntimeRequest;
use crate::services::workspace::workspace_vm_service::live_owner_start_intent;
use crate::services::{ConfigService, TaskService};

const AGENT_TASK_SPEC_SCHEMA: &str = "g5.agent-task-spec/v1";
const ATTEMPT_POLICY_SCHEMA: &str = "g5.agent-attempt-policy/v1";
const AGENT_MODULE_VERSION: &str = "1.0.5";
const CODEX_APP_SERVER_MODULE_VERSION: &str = "1.0.8";
const CODEX_APP_SERVER_CATALOG: &str = "2026.10.24-codex-app-server";
const CODEX_FORMAL_APP_SERVER_MODULE_VERSION: &str = "1.0.10";
/// The first catalog whose guest runs Codex formal review over App Server
/// and has no `codex exec` launcher (`g5.chat.codex-exec-retirement`).
const CODEX_FORMAL_APP_SERVER_CATALOG: &str = "2026.10.28-codex-formal-app-server";
/// Schema 5 approval reports (agent-authored summary) run under new formal
/// module identities (`g5.agent-approval.contract-rollout`): Claude's
/// per-schema module, and a Codex App Server module accepting schemas 2–5.
/// Older schemas keep their modules, so `1.0.10` never receives schema 5.
const CLAUDE_FORMAL_SUMMARY_MODULE_VERSION: &str = "1.0.11";
const CODEX_FORMAL_SUMMARY_APP_SERVER_MODULE_VERSION: &str = "1.0.12";
/// The first catalog carrying both schema 5 formal modules.
const APPROVAL_SUMMARY_CATALOG: &str = "2026.10.30-approval-summary";
/// Assignment schema introduced with [`APPROVAL_SUMMARY_CATALOG`].
const APPROVAL_SUMMARY_SCHEMA: u8 = 5;
/// Schema 6 reports (schema 5's frontmatter, ordered report body) run under
/// their own formal module identities: Claude's per-schema `1.0.13` and a
/// Codex App Server `1.0.14` accepting schemas 2–6, so `1.0.12` never
/// receives schema 6.
const CLAUDE_FORMAL_REPORT_ORDER_MODULE_VERSION: &str = "1.0.13";
const CODEX_FORMAL_REPORT_ORDER_APP_SERVER_MODULE_VERSION: &str = "1.0.14";
/// The first catalog carrying both schema 6 formal modules.
const APPROVAL_REPORT_ORDER_CATALOG: &str = "2026.11.05-approval-report-order";
/// Assignment schema introduced with [`APPROVAL_REPORT_ORDER_CATALOG`].
const APPROVAL_REPORT_ORDER_SCHEMA: u8 = 6;
/// `g5.chat.codex-runtime-refusal`, phrased for the admission refusal notice.
const CODEX_RUNTIME_REFUSAL: &str = "this workspace's runtime predates the current Codex integration. Stop and start the workspace to update it";
/// A review workspace is human-read-only; an explicit retry forks a new one.
const CODEX_REVIEW_RUNTIME_REFUSAL: &str = "this review's runtime predates the current Codex integration. Retry the review to run it on an updated runtime";
/// A review VM whose catalog predates its assignment schema's module.
const REVIEW_CONTRACT_RUNTIME_REFUSAL: &str = "this review's runtime predates the current approval contract. Retry the review to run it on an updated runtime";
/// The first catalog whose preview module accepts its isolated `origin`
/// (`g5.workspace.preview-origin-security`).
const PREVIEW_ORIGIN_CATALOG: &str = "2026.10.26-workspace-preview-origin";
/// Known shipped module catalogs in capability order. A VM records the
/// catalog its host verified at Ready, so a contract the guest must
/// understand is sent only to a VM whose catalog is at or after the one that
/// introduced it. An unknown catalog stays on the established contract, or
/// refuses work that has none (Codex: `g5.chat.codex-runtime-refusal`), so
/// every bundled catalog bump appends here.
const SHIPPED_MODULE_CATALOGS: &[&str] = &[
    "2026.09.19-p4.2b",
    "2026.09.20-mvp-agent-turn",
    "2026.09.29-agent-turn-recovery",
    "2026.09.30-agent-turn-background",
    "2026.10.03-agent-turn-native-resume",
    "2026.10.06-workspace-changes-rebase",
    "2026.10.08-sixteen-agent-turns",
    "2026.10.11-agent-execution",
    "2026.10.12-workspace-changes-adopt",
    "2026.10.17-agent-direct-provider",
    "2026.10.17-task-only-direct-provider",
    "2026.10.18-formal-agent-approvals",
    "2026.10.19-workspace-formal-reviews",
    "2026.10.22-workspace-base-reviews",
    CODEX_APP_SERVER_CATALOG,
    // This parallel catalog has App Server but predates preview-origin support.
    "2026.10.27-merge-base",
    PREVIEW_ORIGIN_CATALOG,
    "2026.10.27-merge-base-preview-origin",
    CODEX_FORMAL_APP_SERVER_CATALOG,
    APPROVAL_SUMMARY_CATALOG,
    // Same modules as the catalog before it; the guest keeps a deferred question resumable only while
    // its saved session awaits it (#968). No API path is conditioned on it.
    "2026.10.30-agent-turn-question-gate",
    APPROVAL_REPORT_ORDER_CATALOG,
    // Same modules as the catalog before it, without an output cap on any agent turn (#1161).
    // No API path is conditioned on it: every guest is still sent `max_output_bytes`.
    "2026.11.10-agent-turn-output-unbounded",
];

/// Whether a VM started from `vm_catalog` understands what `since` introduced.
fn catalog_supports(vm_catalog: Option<&str>, since: &str) -> bool {
    let position = |catalog: &str| SHIPPED_MODULE_CATALOGS.iter().position(|c| *c == catalog);
    match (vm_catalog.and_then(position), position(since)) {
        (Some(vm), Some(since)) => vm >= since,
        _ => false,
    }
}
use g5_contracts::codex_models::CODEX_APP_SERVER_TRANSPORT;
const AGENT_INPUT_SCHEMA: &str = "g5.agent.turn.input/v1";
/// Content of the internal continuation input that carries a saved answer to
/// a deferred question. It describes the delivery in the queued tray and the
/// transcript; the provider never receives it.
pub const NATIVE_ANSWER_CONTINUATION_LABEL: &str = "Saved answer for the agent's question";
/// Bounds startup/input delivery only; running agent turns have no time cap.
const AGENT_SETUP_TIMEOUT_MS: u64 = 300_000;
/// Retired: an agent turn's output has no cumulative limit
/// (`g5.agents.output-bounds`). The value still travels in every turn's
/// limits and frozen policy, because a guest from before the removal refuses
/// a turn without it (or with zero) and stops the turn there; a current
/// guest accepts and ignores it.
const DEFAULT_MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PROMPT_BYTES: usize = 512 * 1024;
const MAX_SYSTEM_NOTICES_PER_ATTEMPT: i64 = 16;
const MAX_SYSTEM_NOTICE_PREAMBLE_BYTES: usize = 8 * 1024;
const MAX_RECOVERY_MESSAGES: i64 = 256;
const MAX_RECOVERY_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_RECOVERY_CONTEXT_BYTES: usize = 256 * 1024;
const PREVIEW_MODULE_VERSION: &str = "1.0.0";

const AGENT_CAPABILITIES: &[&str] = &[
    "workspace.native_files",
    "workspace.native_git",
    "workspace.native_shell",
    "workspace.native_tests",
    "network.agent_control_plane",
    "network.agent_provider",
];

const PREVIEW_CAPABILITIES: &[&str] = &[
    "workspace.read",
    "workspace.write",
    "workspace.native_shell",
    "preview.service",
];

/// The workspace's agent and command slots on the reserved VM
/// (`g5.agents.admission.vm-slots`). Every catalog shipped before sixteen-turn
/// support caps a VM at four bounded executions in total, so on a VM started
/// from one of them admission stays at two turns, which leaves two slots for
/// task-less Changes and diagnostics operations, until the VM is replaced. A
/// running VM always records the catalog its host verified at Ready, and this
/// list is closed: later catalogs are never capped.
macro_rules! workspace_slot_limit_sql {
    () => {
        "CASE WHEN vm.module_catalog_version IN (
               '2026.09.19-p4.2b', '2026.09.20-mvp-agent-turn',
               '2026.09.29-agent-turn-recovery', '2026.09.30-agent-turn-background',
               '2026.10.03-agent-turn-native-resume',
               '2026.10.06-workspace-changes-rebase')
             THEN LEAST(settings.bounded_execution_slots, 2)
             ELSE settings.bounded_execution_slots END"
    };
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CommandTaskSpecV1 {
    schema_version: String,
    module_id: String,
    module_version: String,
    operation: String,
    input_schema: String,
    input: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentTaskSpecV1 {
    schema_version: String,
    agent_launch: AgentLaunchPreferencesV1,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default)]
    limits: AgentTaskLimits,
    #[serde(default)]
    workspace_target: WorkspaceTarget,
    #[serde(default)]
    system_notice_ids: Vec<Uuid>,
    #[serde(default)]
    formal_review: Option<g5_contracts::types::FormalApprovalInvocation>,
    #[serde(default)]
    formal_review_launch: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentTaskLimits {
    #[serde(default)]
    max_turns: Option<u32>,
    #[serde(default = "default_max_output_bytes")]
    max_output_bytes: u64,
    #[serde(default = "default_native_response_timeout_ms")]
    native_response_timeout_ms: u64,
}

impl Default for AgentTaskLimits {
    fn default() -> Self {
        Self {
            max_turns: None,
            max_output_bytes: default_max_output_bytes(),
            native_response_timeout_ms: default_native_response_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceTarget {
    repo_id: Option<Uuid>,
    relative_path: Option<String>,
    /// A personal-workspace launch (`g5.personal.project-docs`).
    #[serde(default)]
    personal: bool,
}

/// The dispatcher's `workspace_target`; `personal` is sent only when true,
/// so ordinary launches keep their wire shape.
fn workspace_target_input(target: &WorkspaceTarget) -> Value {
    let mut value = json!({
        "repo_id": target.repo_id,
        "relative_path": target.relative_path,
    });
    if target.personal {
        value["personal"] = json!(true);
    }
    value
}

#[derive(Debug, Clone)]
pub struct AgentDispatch {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub owner_user_id: Uuid,
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub attempt: i32,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub exec_id: String,
    pub invoke_params: Value,
    /// Set when this reserved attempt is re-invoked with part of its secret
    /// set withheld because the configuration turned invalid after admission
    /// (`g5.chat.secret-admission`): the owner-facing sentence the guest fails
    /// the turn with if it had not already started it.
    pub secret_preflight_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommandDispatch {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub owner_user_id: Uuid,
    pub task_id: Uuid,
    pub attempt: i32,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub exec_id: String,
    pub invoke_params: Value,
    /// As on [`AgentDispatch`]; a preview has no preflight channel, so the
    /// withheld entries are only logged.
    pub secret_preflight_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EnqueueAgentInput {
    pub session_id: Uuid,
    pub actor_user_id: Uuid,
    /// The name of a task this input creates; "Agent turn" when absent. An
    /// input that joins the session's active task keeps that task's name.
    pub task_name: Option<String>,
    pub content: String,
    pub artifacts: Value,
    pub input_origin: String,
    pub template_ref: Option<String>,
    pub template_revision: Option<String>,
    pub agent_launch: AgentLaunchPreferencesV1,
    /// Confirmation retained until the input's first attempt is admitted.
    pub confirmed_launch: Option<PersistedAgentLaunchV1>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueuedAgentInput {
    pub input_id: Uuid,
    pub task_id: Uuid,
    pub task_status: String,
    pub delivery: String,
    pub created_at: DateTime<Utc>,
    pub created_task: bool,
}

/// The published queued-input wire contract: the frontend reads camelCase
/// fields (`SessionInputDTO`), so serialization is explicit rather than the
/// derive default.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInputView {
    pub id: Uuid,
    pub session_id: Uuid,
    pub input_index: i64,
    pub content: String,
    pub artifacts: Value,
    pub revision: i32,
    pub input_origin: String,
    pub delivery: String,
    pub delivery_generation: i32,
    pub task_id: Option<Uuid>,
    pub attempt: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub withdrawn_at: Option<DateTime<Utc>>,
    /// Whether `edit_queued_session_input` would currently admit this input:
    /// the UI must not offer an action the API always refuses, including a
    /// steering reservation or a frozen possibly-delivered revision.
    pub editable: bool,
    /// Whether `withdraw_queued_session_input` would currently admit it.
    pub withdrawable: bool,
}

impl AgentDispatch {
    pub fn start_disposition(&self, disposition: Value) -> TaskLifecycleEvent {
        TaskLifecycleEvent::StartDisposition {
            task_id: self.task_id,
            attempt: self.attempt,
            vm_id: self.vm_id,
            epoch: self.epoch,
            disposition,
        }
    }
}

impl CommandDispatch {
    pub fn start_disposition(&self, disposition: Value) -> TaskLifecycleEvent {
        TaskLifecycleEvent::StartDisposition {
            task_id: self.task_id,
            attempt: self.attempt,
            vm_id: self.vm_id,
            epoch: self.epoch,
            disposition,
        }
    }
}

/// One pass over the queued candidates of a tenant: a reservation, a
/// candidate settled at admission (the next one may be tried), or nothing
/// eligible.
enum QueuedCandidate<Row> {
    Reserved(Row),
    Settled,
    Exhausted,
}

/// Most candidates one reservation settles before yielding the transaction.
const SETTLED_CANDIDATES_PER_RESERVATION: usize = 8;

#[derive(Debug)]
struct CommandDispatchRow {
    org_id: Uuid,
    workspace_id: Uuid,
    owner_user_id: Uuid,

    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    spec: CommandTaskSpecV1,
    grant_id: Uuid,
    /// The catalog the attempt's VM was verified with at Ready.
    vm_catalog: Option<String>,
}

#[derive(Debug)]
struct DispatchRow {
    org_id: Uuid,
    workspace_id: Uuid,
    owner_user_id: Uuid,
    task_id: Uuid,
    session_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    task_spec: AgentTaskSpecV1,
    continuation_reason: Option<String>,
    provider_session_id: Option<String>,
    input_id: Uuid,
    input_content: String,
    input_artifacts: Value,
    input_origin: String,
    input_revision: i32,
    reservation_id: Uuid,
    delivery_generation: i32,
    policy_snapshot: Value,
}

/// The columns every `SessionInputView` read selects or returns, including
/// the two state bits the view's permission flags are computed from. An
/// input whose redelivery an owner authorized went back to `queued` with
/// content the provider may already hold, so that revision stays frozen
/// (`g5.chat.uncertain-delivery`): the marker is the audit event the
/// authorization appended.
macro_rules! session_input_view_sql {
    () => {
        "id, session_id, input_index, content, artifacts, revision,
         input_origin, delivery, delivery_generation, task_id, attempt,
         created_at, delivered_at, withdrawn_at, steered,
         delivery_audit @> '[{\"event\":\"redelivery_authorized\"}]'::jsonb
             AS possibly_delivered"
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FrozenRecoveryPrompt {
    text: String,
    #[serde(default)]
    current_input: Option<String>,
    recovery: Option<RecoveryNotice>,
    evidence_cutoff: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryNotice {
    included_messages: u32,
    omitted_messages: u64,
    truncated_messages: u32,
    attachment_references: u32,
    omitted_attachment_references: u64,
    unavailable_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct RecoveryMessage {
    index: i32,
    role: String,
    status: String,
    content: String,
    attachments: Vec<String>,
    omitted_attachments: u64,
    anchor: bool,
}

impl TaskService {
    pub async fn list_session_inputs(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
    ) -> Result<Vec<SessionInputView>, AppError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            session_input_view_sql!(),
            " FROM session_inputs
             WHERE session_id = $1
               AND delivery IN ('queued','reserved','delivery_unknown','withdrawn')
             ORDER BY input_index, id",
        ))
        .bind(session_id)
        .fetch_all(db.conn())
        .await?;
        rows.into_iter().map(session_input_view).collect()
    }

    pub async fn edit_queued_session_input(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        session_id: Uuid,
        input_id: Uuid,
        expected_revision: i32,
        content: String,
    ) -> Result<SessionInputView, AppError> {
        if expected_revision <= 0 || content.trim().is_empty() || content.len() > MAX_PROMPT_BYTES {
            return Err(conflict("queued input edit is invalid"));
        }
        assert_session_owner(db, session_id, actor_user_id).await?;
        // The edited text may become the first delivered input of a persona
        // session and gain the frozen prefix at reservation: cap the expanded
        // bytes now (g5.workbench.persona).
        ensure_persona_prefixed_size(db, session_id, &content).await?;
        let row = sqlx::query(concat!(
            "UPDATE session_inputs
             SET content = $5, revision = revision + 1,
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object('event','edited','actor_user_id',$4))
             WHERE id = $1 AND session_id = $2 AND revision = $3
               AND input_origin = 'human' AND delivery = 'queued' AND NOT steered
               AND NOT delivery_audit @> '[{\"event\":\"redelivery_authorized\"}]'::jsonb
             RETURNING ",
            session_input_view_sql!(),
        ))
        .bind(input_id)
        .bind(session_id)
        .bind(expected_revision)
        .bind(actor_user_id)
        .bind(content)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            conflict(
                "queued input changed, was reserved, may already have been \
                 delivered, or is not editable",
            )
        })?;
        session_input_view(row)
    }

    pub async fn withdraw_queued_session_input(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        session_id: Uuid,
        input_id: Uuid,
        expected_revision: i32,
    ) -> Result<SessionInputView, AppError> {
        assert_session_owner(db, session_id, actor_user_id).await?;
        // The "task left without input" decision below must not interleave
        // with a Send attaching a new input to the same task: every path that
        // attaches or re-queues a session's inputs allocates under this lock,
        // so holding it makes withdrawal's check-then-cancel atomic against
        // them (chat_service D53 order: session lock before any row).
        crate::services::agents::chat_service::lock_session_turn_allocation(db.conn(), session_id)
            .await?;
        let row = sqlx::query(concat!(
            "UPDATE session_inputs
             SET delivery = 'withdrawn', withdrawn_at = now(),
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object('event','withdrawn','actor_user_id',$4))
             WHERE id = $1 AND session_id = $2 AND revision = $3
               AND input_origin = 'human' AND delivery = 'queued' AND NOT steered
             RETURNING ",
            session_input_view_sql!(),
        ))
        .bind(input_id)
        .bind(session_id)
        .bind(expected_revision)
        .bind(actor_user_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("queued input changed, was reserved, or is not withdrawable"))?;
        if let Some(task_id) = row.try_get::<Option<Uuid>, _>("task_id")? {
            self.cancel_task_left_without_input(db, actor_user_id, task_id)
                .await?;
        }
        session_input_view(row)
    }

    /// Stop's withdrawal of a message still waiting, behind another turn or
    /// in a turn that has not started (`g5.agents.lifecycle.chat-stop`): the
    /// owner's or one the platform sent, whatever its revision. The caller has
    /// checked the session's owner and holds its turn allocation lock.
    pub(crate) async fn withdraw_waiting_input_for_stop(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        session_id: Uuid,
        input_id: Uuid,
    ) -> Result<(), AppError> {
        let task_id = sqlx::query_scalar::<_, Option<Uuid>>(
            "UPDATE session_inputs
             SET delivery = 'withdrawn', withdrawn_at = now(),
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object('event','stopped','actor_user_id',$3))
             WHERE id = $1 AND session_id = $2 AND delivery = 'queued'
               AND input_origin <> 'continuation' AND NOT steered
             RETURNING task_id",
        )
        .bind(input_id)
        .bind(session_id)
        .bind(actor_user_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("queued input was reserved or is not withdrawable"))?;
        if let Some(task_id) = task_id {
            self.cancel_task_left_without_input(db, actor_user_id, task_id)
                .await?;
        }
        Ok(())
    }

    /// A queued task whose last queued input was withdrawn, and which has no
    /// continuation to run, has nothing left to do. Every caller holds the
    /// session turn-allocation lock, so a concurrent Send cannot attach a new
    /// input between this check and the cancellation it decides.
    ///
    /// The settlement locks only the task row. The caller's route already
    /// holds the workspace authority share lock, which keeps ownership
    /// steady; upgrading it (as the general `request_workspace_task_cancel`
    /// does through its workspace `FOR UPDATE`) would deadlock with a Send
    /// holding its own share lock while waiting for the turn-allocation lock
    /// this withdrawal holds.
    async fn cancel_task_left_without_input(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        task_id: Uuid,
    ) -> Result<(), AppError> {
        let row = sqlx::query(
            "SELECT org_id, workspace_id, status, attempt,
                    continuation_reason IS NOT NULL AS has_continuation,
                    EXISTS(SELECT 1 FROM session_inputs
                           WHERE task_id = task.id AND delivery = 'queued')
                        AS has_queued_input
             FROM tasks AS task
             WHERE id = $1 AND kind = 'agent'
             FOR UPDATE OF task",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?;
        let Some(row) = row else {
            return Ok(());
        };
        if row.try_get::<bool, _>("has_queued_input")?
            || row.try_get::<bool, _>("has_continuation")?
            || row.try_get::<String, _>("status")? != "queued"
        {
            return Ok(());
        }
        sqlx::query(
            "UPDATE tasks
             SET status = 'cancelled',
                 outcome = jsonb_build_object('status','cancelled','reason','owner_cancel'),
                 hold_reason = NULL, continuation_reason = NULL,
                 ended_at = now(), updated_at = now()
             WHERE id = $1 AND status = 'queued'",
        )
        .bind(task_id)
        .execute(db.conn())
        .await?;
        sqlx::query(
            "UPDATE session_inputs
             SET delivery = 'withdrawn', withdrawn_at = now(),
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object('event','task_cancelled','actor_user_id',$2))
             WHERE task_id = $1
               AND delivery IN ('queued','reserved','delivery_unknown')",
        )
        .bind(task_id)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        crate::services::agents::task_lifecycle::fail_task_dependents(
            db,
            row.try_get("org_id")?,
            row.try_get("workspace_id")?,
            task_id,
        )
        .await?;
        crate::services::agents::task_lifecycle::invalidate_attempt_requests(
            db,
            task_id,
            row.try_get("attempt")?,
            "owner_cancel",
        )
        .await?;
        // Withdrawing the last input of a turn a drain left queued ends that
        // turn; an automatic continuation still waiting for it is cancelled
        // with it (`g5.lifecycle.start.automatic-continuation`).
        crate::services::agents::task_lifecycle::release_automatic_consent_without_work(
            db.conn(),
            row.try_get("workspace_id")?,
        )
        .await?;
        Ok(())
    }

    pub async fn authorize_unknown_input_redelivery(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        session_id: Uuid,
        input_id: Uuid,
    ) -> Result<SessionInputView, AppError> {
        assert_session_owner(db, session_id, actor_user_id).await?;
        crate::services::agents::chat_service::lock_session_turn_allocation(db.conn(), session_id)
            .await?;
        self.announce_session_chat_activity(db, session_id).await?;

        let row = sqlx::query(
            "SELECT task_id, attempt, input_origin FROM session_inputs
             WHERE id = $1 AND session_id = $2 AND delivery = 'delivery_unknown'
             FOR UPDATE",
        )
        .bind(input_id)
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("input is not awaiting an explicit redelivery decision"))?;
        let task_id: Uuid = row.try_get("task_id")?;
        let attempt: i32 = row.try_get("attempt")?;
        let input_origin: String = row.try_get("input_origin")?;
        let continuation_input = input_origin == "continuation";
        let task_updated = sqlx::query(
            "UPDATE tasks
             SET status = 'queued', hold_reason = NULL,
                 continuation_reason = CASE WHEN $4 THEN continuation_reason ELSE NULL END,
                 spec = jsonb_set(
                     spec, '{lifecycle}',
                     jsonb_build_object(
                         'redelivery', jsonb_build_object(
                             'prior_attempt',$2,'authorized_by',$3::text,
                             'authorized_at',now())), true),
                 updated_at = now()
             WHERE id = $1 AND attempt = $2 AND status = 'waiting'
               AND hold_reason = 'delivery_unknown'",
        )
        .bind(task_id)
        .bind(attempt)
        .bind(actor_user_id)
        .bind(continuation_input)
        .execute(db.conn())
        .await?;
        if task_updated.rows_affected() != 1 {
            return Err(conflict("delivery-unknown task is no longer held"));
        }
        let updated = sqlx::query(concat!(
            "UPDATE session_inputs
             SET delivery = CASE WHEN $5 THEN 'withdrawn' ELSE 'queued' END,
                 withdrawn_at = CASE WHEN $5 THEN now() ELSE NULL END,
                 reservation_id = NULL, reservation_revision = NULL,
                 attempt = CASE WHEN $5 THEN attempt ELSE NULL END,
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object('event','redelivery_authorized',
                                        'actor_user_id',$3,'prior_attempt',$4))
             WHERE id = $1 AND session_id = $2 AND delivery = 'delivery_unknown'
             RETURNING ",
            session_input_view_sql!(),
        ))
        .bind(input_id)
        .bind(session_id)
        .bind(actor_user_id)
        .bind(attempt)
        .bind(continuation_input)
        .fetch_one(db.conn())
        .await?;
        session_input_view(updated)
    }

    /// Cancel the task behind a delivery-unknown input. A delivery-unknown row
    /// is already attempt-bound, so the queued-only withdrawal route cannot
    /// safely settle it. The common task cancellation path fences that attempt,
    /// withdraws its input and settles dependants.
    pub async fn cancel_unknown_session_input(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        session_id: Uuid,
        input_id: Uuid,
    ) -> Result<SessionInputView, AppError> {
        assert_session_owner(db, session_id, actor_user_id).await?;
        crate::services::agents::chat_service::lock_session_turn_allocation(db.conn(), session_id)
            .await?;

        let task_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT task_id FROM session_inputs
             WHERE id = $1 AND session_id = $2 AND delivery = 'delivery_unknown'",
        )
        .bind(input_id)
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("input is not awaiting an explicit cancel decision"))?;
        self.request_workspace_task_cancel(db, actor_user_id, task_id)
            .await?;
        let row = sqlx::query(concat!(
            "SELECT ",
            session_input_view_sql!(),
            " FROM session_inputs WHERE id = $1 AND session_id = $2",
        ))
        .bind(input_id)
        .bind(session_id)
        .fetch_one(db.conn())
        .await?;
        session_input_view(row)
    }

    /// Enqueue one typed or templated input under the session serialization
    /// cut. The inbox row is the only mutable pre-delivery authority; no
    /// transcript row exists until `input.accepted` is journaled.
    pub async fn enqueue_agent_input(
        &self,
        db: &mut ScopedDb,
        input: EnqueueAgentInput,
    ) -> Result<QueuedAgentInput, AppError> {
        if input.content.trim().is_empty() {
            return Err(conflict("agent input cannot be empty"));
        }
        if input.content.len() > MAX_PROMPT_BYTES {
            return Err(conflict("agent input exceeds the prompt byte limit"));
        }
        // A persona session's first delivered input is prefixed with the
        // frozen instructions at reservation (g5.workbench.persona), so the
        // cap applies to the expanded bytes here, not at dispatch time.
        ensure_persona_prefixed_size(db, input.session_id, &input.content).await?;
        if !matches!(input.input_origin.as_str(), "human" | "workflow") {
            return Err(conflict("initial agent input origin is invalid"));
        }
        if !input.artifacts.is_array() {
            return Err(conflict("agent input artifacts must be an array"));
        }
        if input.input_origin == "workflow"
            && (input.template_ref.is_none() != input.template_revision.is_none())
        {
            return Err(conflict(
                "workflow input template reference and revision must be paired",
            ));
        }
        if input.input_origin != "workflow"
            && (input.template_ref.is_some() || input.template_revision.is_some())
        {
            return Err(conflict(
                "human input cannot carry workflow template identity",
            ));
        }
        input
            .agent_launch
            .validate()
            .map_err(AppError::from_error)?;
        if let Some(confirmed) = &input.confirmed_launch {
            confirmed.validate().map_err(AppError::from_error)?;
            if confirmed.preferences != input.agent_launch {
                return Err(conflict(
                    "confirmed launch preferences do not match the input",
                ));
            }
        }

        crate::services::agents::chat_service::lock_session_turn_allocation(
            db.conn(),
            input.session_id,
        )
        .await?;
        // Input ordering belongs to the session. A shared workspace lock holds
        // ownership steady without upgrading the route's authority lock and
        // deadlocking another conversation admission.
        let session = sqlx::query(
            "SELECT session.org_id, session.workspace_id, workspace.owner_user_id,
                    workspace.is_base, workspace.deleted_at
             FROM chat_sessions AS session
             JOIN workspaces AS workspace
               ON workspace.org_id = session.org_id
              AND workspace.id = session.workspace_id
             WHERE session.id = $1
             FOR UPDATE OF session FOR SHARE OF workspace",
        )
        .bind(input.session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("agent session is absent"))?;
        let owner_user_id = session
            .try_get::<Option<Uuid>, _>("owner_user_id")?
            .ok_or_else(|| conflict("workspace has no current owner"))?;
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            owner_user_id,
            session.try_get("org_id")?,
            true,
        )
        .await
        .map_err(AppError::from_error)?;
        if owner_user_id != input.actor_user_id {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }));
        }
        if session
            .try_get::<Option<DateTime<Utc>>, _>("deleted_at")?
            .is_some()
        {
            return Err(conflict("workspace is deleted"));
        }
        if session.try_get::<bool, _>("is_base")? {
            return Err(conflict("base workspaces are read-only; fork first"));
        }
        let org_id: Uuid = session.try_get("org_id")?;
        let workspace_id: Uuid = session.try_get("workspace_id")?;

        let mut active_task = sqlx::query(
            "SELECT id, status, hold_reason
             FROM tasks
             WHERE org_id = $1 AND workspace_id = $2 AND chat_session_id = $3
               AND kind = 'agent' AND status IN ('queued','running','waiting')
             ORDER BY created_at, id
             FOR UPDATE",
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(input.session_id)
        .fetch_all(db.conn())
        .await?;
        if active_task.len() > 1 {
            return Err(conflict("agent session has multiple active logical tasks"));
        }
        // A message sent while the running turn waits for Codex device login
        // joins that turn's queue like any other follow-up; the login keeps its
        // challenge, deadline and original prompt (`g5.chat.login-before-prompt`).
        // The owner's new message ends a chat turn held for Continue, which
        // the chat does not offer (`g5.agents.lifecycle.chat-held-turn`). An
        // input of unknown delivery keeps its Redeliver choice, and a workflow
        // turn, or one the platform started (a merge-conflict resolution,
        // which its merge run follows by task), waits for Continue.
        let mut ended_turn = None;
        if let Some(task) = active_task.first() {
            if input.input_origin == "human"
                && task.try_get::<String, _>("status")? == "waiting"
                && task.try_get::<Option<String>, _>("hold_reason")?.as_deref()
                    != Some("delivery_unknown")
                && !crate::services::agents::task_lifecycle::has_unanswered_native_question(
                    db,
                    task.try_get("id")?,
                )
                .await?
                && !crate::services::agents::task_lifecycle::turn_kept_for_its_owner(
                    db,
                    task.try_get("id")?,
                )
                .await?
            {
                let held: Uuid = task.try_get("id")?;
                crate::services::agents::task_lifecycle::end_chat_turn_now(
                    db,
                    held,
                    crate::services::agents::task_lifecycle::ChatTurnEnd::NextMessage,
                )
                .await?;
                ended_turn = Some(held);
                active_task.clear();
            }
        }

        let input_id = Uuid::new_v4();
        let created_at = Utc::now();
        let input_index = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(input_index), -1) + 1
             FROM session_inputs WHERE session_id = $1",
        )
        .bind(input.session_id)
        .fetch_one(db.conn())
        .await?;
        sqlx::query(
            "INSERT INTO session_inputs(
                 id, org_id, session_id, input_index, content, artifacts,
                 revision, input_origin, template_ref, template_revision,
                 delivery, created_at, delivery_audit, launch_preferences, confirmed_launch)
             VALUES ($1,$2,$3,$4,$5,$6,1,$7,$8,$9,'queued',$10,
                     jsonb_build_array(jsonb_build_object('event','enqueued','actor_user_id',$11)), $12, $13)",
        )
        .bind(input_id)
        .bind(org_id)
        .bind(input.session_id)
        .bind(input_index)
        .bind(&input.content)
        .bind(input.artifacts)
        .bind(&input.input_origin)
        .bind(input.template_ref)
        .bind(input.template_revision)
        .bind(created_at)
        .bind(input.actor_user_id)
        .bind(serde_json::to_value(&input.agent_launch)?)
        .bind(input.confirmed_launch.as_ref().map(serde_json::to_value).transpose()?)
        .execute(db.conn())
        .await?;

        let (task_id, task_status, created_task) = if let Some(task) = active_task.first() {
            (
                task.try_get("id")?,
                task.try_get::<String, _>("status")?,
                false,
            )
        } else {
            let task_id = Uuid::new_v4();
            // A personal workspace's agent launches in its private repository,
            // so Claude's native memory binds there and reference checkouts
            // stay outside the launch directory (g5.personal.project-docs).
            let personal = sqlx::query_scalar::<_, bool>(
                "SELECT personal_user_id IS NOT NULL FROM workspaces WHERE id = $1",
            )
            .bind(workspace_id)
            .fetch_optional(db.conn())
            .await?
            .unwrap_or(false);
            let workspace_target = if personal {
                json!({
                    "relativePath": crate::services::workspace::personal_workspace::PERSONAL_ROOT_PATH,
                    "personal": true,
                })
            } else {
                json!({})
            };
            let spec = json!({
                "agent": {
                    "schemaVersion": AGENT_TASK_SPEC_SCHEMA,
                    "agentLaunch": input.agent_launch,
                    "allowedTools": [],
                    "limits": AgentTaskLimits::default(),
                    "workspaceTarget": workspace_target,
                    "systemNoticeIds": [],
                },
                "dependency_failure_policy": "fail_dependents",
            });
            sqlx::query(
                "INSERT INTO tasks(
                     id, name, status, retry_count,
                     depends_on, input_tokens, output_tokens, cost_usd, org_id,
                     workspace_id, kind, spec, attempt, start_confirmed,
                     chat_session_id, created_by, created_at, updated_at)
                 VALUES ($1,$8,'queued',0,'{}'::uuid[],0,0,0,
                         $2,$3,'agent',$4,1,false,$5,$6,$7,$7)",
            )
            .bind(task_id)
            .bind(org_id)
            .bind(workspace_id)
            .bind(spec)
            .bind(input.session_id)
            .bind(input.actor_user_id)
            .bind(created_at)
            .bind(input.task_name.as_deref().unwrap_or("Agent turn"))
            .execute(db.conn())
            .await?;
            (task_id, "queued".to_string(), true)
        };

        sqlx::query(
            "UPDATE session_inputs SET task_id = $2, attempt = NULL, steered = false, reservation_id = NULL, reservation_revision = NULL
             WHERE id = $1 AND delivery = 'queued' AND task_id IS NULL",
        )
        .bind(input_id)
        .bind(task_id)
        .execute(db.conn())
        .await?;
        if let Some(ended) = ended_turn {
            // The ended turn's queued messages, including one the agent never
            // received, run in this turn ahead of the new message.
            sqlx::query(
                "UPDATE session_inputs SET task_id = $1, attempt = NULL, steered = false, reservation_id = NULL, reservation_revision = NULL
                 WHERE org_id = $2 AND session_id = $3 AND task_id = $4
                   AND delivery = 'queued' AND input_origin <> 'continuation'",
            )
            .bind(task_id)
            .bind(org_id)
            .bind(input.session_id)
            .bind(ended)
            .execute(db.conn())
            .await?;
        }
        if created_task {
            // A turn refused at admission for something the owner must
            // correct keeps its queued message attached to the failed task
            // (`g5.chat.secret-admission`). The owner's next message, once
            // the correction is made, runs that message ahead of itself. For
            // refused first admission, that Send accepts current values with
            // the input's saved preferences. Admitted inputs stay frozen.
            sqlx::query(
                "UPDATE session_inputs input
                 SET task_id = $1, attempt = NULL, steered = false,
                     reservation_id = NULL, reservation_revision = NULL,
                     confirmed_launch = CASE
                         WHEN task.status = 'failed'
                           AND NOT input.delivery_audit @> '[{\"event\":\"reserved\"}]'::jsonb
                         THEN NULL ELSE input.confirmed_launch END
                 FROM tasks task
                 WHERE input.org_id = $2 AND input.session_id = $3 AND input.task_id <> $1
                   AND input.delivery = 'queued' AND input.input_origin <> 'continuation'
                   AND task.id = input.task_id AND task.org_id = $2
                   AND task.chat_session_id = $3 AND task.kind = 'agent'
                   AND task.status IN ('failed', 'cancelled')",
            )
            .bind(task_id)
            .bind(org_id)
            .bind(input.session_id)
            .execute(db.conn())
            .await?;
        }

        if created_task {
            // A new turn makes the chat active (chat g5.chat.activity.refresh).
            self.announce_chat_activity(db, task_id).await?;
        }
        Ok(QueuedAgentInput {
            input_id,
            task_id,
            task_status,
            delivery: "queued".to_string(),
            created_at,
            created_task,
        })
    }

    /// Prefer a previously committed unconfirmed attempt, then reserve one
    /// dependency-ready queued agent task on a locally reachable host.
    /// Eligibility precedes reservation/backoff writes. This method performs no network
    /// I/O and leaves the caller in control of the commit boundary.
    pub async fn reserve_agent_dispatch(
        &self,
        db: &mut ScopedDb,
    ) -> Result<Option<AgentDispatch>, AppError> {
        if let Some(row) = self.load_unconfirmed_agent_dispatch(db).await? {
            return self.build_agent_dispatch(db, row).await.map(Some);
        }
        // A candidate settled at admission is finished work, not a reason to
        // end the tenant's pass (`g5.chat.dispatch-eligibility`): the next
        // candidate is tried in the same transaction, within a bound that
        // keeps it short.
        for _ in 0..SETTLED_CANDIDATES_PER_RESERVATION {
            match self.reserve_queued_agent_dispatch(db).await? {
                QueuedCandidate::Reserved(row) => {
                    return self.build_agent_dispatch(db, row).await.map(Some);
                }
                QueuedCandidate::Settled => continue,
                QueuedCandidate::Exhausted => return Ok(None),
            }
        }
        Ok(None)
    }

    /// After a `started` disposition for a dispatch that withheld secrets:
    /// the guest launched the reduced request and will fail the turn before
    /// the provider runs, so the owner learns the reason in the chat. No-op
    /// for an ordinary dispatch.
    pub async fn record_secret_preflight_notice(
        &self,
        db: &mut ScopedDb,
        dispatch: &AgentDispatch,
    ) -> Result<(), AppError> {
        let Some(detail) = &dispatch.secret_preflight_error else {
            return Ok(());
        };
        let notice = format!(
            "This turn could not run with its Runtime Config secrets: {}. Once corrected, send another message.",
            detail.trim_end_matches('.')
        );
        crate::services::agents::chat_service::record_auth_failure_turn(
            db.conn(),
            crate::services::agents::chat_service::AssistantMessage {
                id: crate::services::agents::task_lifecycle::canonical_agent_message_id(
                    dispatch.task_id,
                    dispatch.attempt,
                ),
                org_id: dispatch.org_id,
                session_id: dispatch.session_id,
                task_id: dispatch.task_id,
                attempt: dispatch.attempt,
            },
            &[],
            &notice,
        )
        .await?;
        Ok(())
    }

    /// Invoke only the exact incarnation persisted on the reservation.  A
    /// failure leaves `start_confirmed=false` and is intentionally retryable.
    pub async fn invoke_agent_dispatch(&self, dispatch: &AgentDispatch) -> Result<Value, AppError> {
        let handle = self
            .workspace_runtime
            .owner_handle(dispatch.workspace_id, dispatch.owner_user_id)
            .await
            .map_err(AppError::from_error)?;
        if handle.org_id != dispatch.org_id
            || handle.vm_id != dispatch.vm_id
            || handle.epoch != dispatch.epoch
        {
            return Err(conflict("agent dispatch incarnation changed before invoke"));
        }
        let request = RuntimeRequest::owner(
            &handle,
            dispatch.owner_user_id,
            unix_time_ms().saturating_add(15_000),
            format!("agent-invoke:{}", dispatch.exec_id),
        );
        self.workspace_runtime
            .modules_invoke(&handle, &request, dispatch.invoke_params.clone())
            .await
            .map_err(runtime_error)
    }

    /// Reserve or reconstruct one command invocation. Commands share the
    /// workspace's bounded slots with agent turns and are admitted only from
    /// server-authored, exact module specs.
    pub async fn reserve_command_dispatch(
        &self,
        db: &mut ScopedDb,
    ) -> Result<Option<CommandDispatch>, AppError> {
        if let Some(row) = self.load_unconfirmed_command_dispatch(db).await? {
            return self.build_command_dispatch(db, row).await.map(Some);
        }
        for _ in 0..SETTLED_CANDIDATES_PER_RESERVATION {
            match self.reserve_queued_command_dispatch(db).await? {
                QueuedCandidate::Reserved(row) => {
                    return self.build_command_dispatch(db, row).await.map(Some);
                }
                QueuedCandidate::Settled => continue,
                QueuedCandidate::Exhausted => return Ok(None),
            }
        }
        Ok(None)
    }

    pub async fn invoke_command_dispatch(
        &self,
        dispatch: &CommandDispatch,
    ) -> Result<Value, AppError> {
        let handle = self
            .workspace_runtime
            .owner_handle(dispatch.workspace_id, dispatch.owner_user_id)
            .await
            .map_err(AppError::from_error)?;
        if handle.org_id != dispatch.org_id
            || handle.vm_id != dispatch.vm_id
            || handle.epoch != dispatch.epoch
        {
            return Err(conflict(
                "command dispatch incarnation changed before invoke",
            ));
        }
        let request = RuntimeRequest::owner(
            &handle,
            dispatch.owner_user_id,
            unix_time_ms().saturating_add(15_000),
            format!("command-invoke:{}", dispatch.exec_id),
        );
        self.workspace_runtime
            .modules_invoke(&handle, &request, dispatch.invoke_params.clone())
            .await
            .map_err(runtime_error)
    }

    async fn load_unconfirmed_command_dispatch(
        &self,
        db: &mut ScopedDb,
    ) -> Result<Option<CommandDispatchRow>, AppError> {
        let row = sqlx::query(
            "SELECT task.org_id, task.workspace_id, workspace.owner_user_id,
                    task.id AS task_id, task.attempt, task.vm_id,
                    task.epoch, task.spec, attempt.policy_snapshot_ref,
                    vm.module_catalog_version
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             JOIN workspace_vms AS vm
               ON vm.org_id = task.org_id AND vm.workspace_id = task.workspace_id
             JOIN task_attempts AS attempt
               ON attempt.org_id = task.org_id AND attempt.task_id = task.id
              AND attempt.attempt = task.attempt
             WHERE task.kind IN ('command','preview') AND task.status = 'running'
               AND NOT task.start_confirmed AND attempt.ended_at IS NULL
               AND NOT attempt.start_confirmed
               AND task.updated_at <= now() - interval '2 seconds'
               AND vm.host_id = ANY($1)
               AND vm.status = 'running' AND vm.vm_id = task.vm_id
               AND vm.epoch = task.epoch AND attempt.vm_id = task.vm_id
               AND attempt.epoch = task.epoch
               AND workspace.deleted_at IS NULL
               AND authz.workspace_execution_allowed(workspace.id)
               AND NOT EXISTS (
                   SELECT 1 FROM workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = task.workspace_id
                     AND transfer.status = 'pending')
             ORDER BY task.updated_at, task.id
             FOR UPDATE OF task, attempt SKIP LOCKED
             LIMIT 1",
        )
        .bind(self.workspace_runtime.reachable_host_ids())
        .fetch_optional(db.conn())
        .await?;
        let row = row.map(parse_command_dispatch_row).transpose()?;
        if let Some(row) = &row {
            crate::services::PermissionService::require_active_member_on_conn(
                db.conn(),
                row.owner_user_id,
                row.org_id,
                true,
            )
            .await
            .map_err(AppError::from_error)?;
            sqlx::query("UPDATE tasks SET updated_at = now() WHERE id = $1")
                .bind(row.task_id)
                .execute(db.conn())
                .await?;
        }
        Ok(row)
    }

    async fn reserve_queued_command_dispatch(
        &self,
        db: &mut ScopedDb,
    ) -> Result<QueuedCandidate<CommandDispatchRow>, AppError> {
        // clock_timestamp(), as in `reserve_queued_agent_dispatch`.
        let candidate = sqlx::query(concat!(
            "SELECT task.org_id, task.workspace_id, workspace.owner_user_id,
                    task.id AS task_id, task.spec, vm.vm_id,
                    vm.epoch, vm.module_catalog_version
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             JOIN workspace_vms AS vm
               ON vm.org_id = task.org_id AND vm.workspace_id = task.workspace_id
             JOIN workspace_settings AS settings
               ON settings.workspace_id = task.workspace_id
             WHERE task.kind IN ('command','preview') AND task.status = 'queued'
               AND task.start_consent_id IS NULL
               AND vm.host_id = ANY($1)
               AND vm.status = 'running' AND vm.vm_id IS NOT NULL
               AND vm.lease_expires_at > clock_timestamp() AND workspace.deleted_at IS NULL
               AND authz.workspace_execution_allowed(workspace.id)
               AND NOT EXISTS (
                   SELECT 1 FROM task_dependencies edge
                   JOIN tasks dependency
                     ON dependency.org_id = edge.org_id
                    AND dependency.id = edge.depends_on_task_id
                   WHERE edge.task_id = task.id
                     AND dependency.status <> 'succeeded')
               AND NOT EXISTS (
                   SELECT 1 FROM workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = task.workspace_id
                     AND transfer.status = 'pending')
               AND NOT EXISTS (
                   SELECT 1 FROM workspace_forks fork
                   WHERE fork.target_workspace_id = task.workspace_id
                     AND fork.status <> 'completed')
               AND (task.kind = 'preview' OR (SELECT count(*) FROM tasks AS active
                    WHERE active.workspace_id = task.workspace_id
                      AND active.kind IN ('agent','command')
                      AND active.status = 'running') < ",
            workspace_slot_limit_sql!(),
            ")
               AND NOT EXISTS (
                   SELECT 1 FROM tasks AS active_preview
                   WHERE task.kind = 'preview'
                     AND active_preview.workspace_id = task.workspace_id
                     AND active_preview.kind = 'preview'
                     AND active_preview.id <> task.id
                     AND active_preview.status = 'running')
             ORDER BY task.created_at, task.id
             FOR UPDATE OF task, settings SKIP LOCKED
             LIMIT 1",
        ))
        .bind(self.workspace_runtime.reachable_host_ids())
        .fetch_optional(db.conn())
        .await?;
        let Some(candidate) = candidate else {
            return Ok(QueuedCandidate::Exhausted);
        };
        let task_id: Uuid = candidate.try_get("task_id")?;
        let org_id: Uuid = candidate.try_get("org_id")?;
        let workspace_id: Uuid = candidate.try_get("workspace_id")?;
        let owner_user_id = candidate
            .try_get::<Option<Uuid>, _>("owner_user_id")?
            .ok_or_else(|| conflict("workspace has no current owner"))?;
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            owner_user_id,
            org_id,
            true,
        )
        .await
        .map_err(AppError::from_error)?;
        let vm_id = parse_uuid_text(candidate.try_get::<String, _>("vm_id")?, "workspace VM")?;
        let epoch = parse_nonnegative(candidate.try_get::<i64, _>("epoch")?, "workspace epoch")?;
        let spec = match parse_command_spec(candidate.try_get("spec")?)
            .and_then(|spec| validate_command_spec(&spec).map(|()| spec))
        {
            Ok(spec) => spec,
            Err(error) => {
                // A spec dispatch cannot run fails its own task. Returning the
                // error would end the coordinator's pass for the organization,
                // and the same task would block it again on every pass.
                settle_dispatch_admission_failure(db, org_id, workspace_id, task_id, &error)
                    .await?;
                tracing::warn!(%task_id, %workspace_id, %error, "command dispatch admission failed permanently");
                return Ok(QueuedCandidate::Settled);
            }
        };
        // The effective secrets are admitted before the reservation
        // (`g5.chat.secret-admission`), for the same reason as the spec.
        let effective = self
            .dispatch_config_service()
            .resolve_effective(db, org_id, Some(workspace_id), Some(owner_user_id))
            .await?;
        if let Err(error) = validate_dispatch_secrets(&effective.secrets, false) {
            let error = conflict(error.to_string());
            settle_dispatch_admission_failure(db, org_id, workspace_id, task_id, &error).await?;
            tracing::warn!(%task_id, %workspace_id, %error, "command dispatch refused the effective secrets");
            return Ok(QueuedCandidate::Settled);
        }
        let attempt = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(attempt), 0) + 1
             FROM task_attempts WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(db.conn())
        .await?;
        let grant_id = Uuid::new_v4();
        let policy_snapshot = json!({
            "schema_version": "g5.command-attempt-policy/v1",
            "grant_id": grant_id,
            "command": spec,
        });
        sqlx::query(
            "INSERT INTO task_attempts(
                 org_id, task_id, attempt, vm_id, epoch, start_confirmed,
                 policy_snapshot_ref, session_files_state,
                 session_manifest_generation, started_at, acked_event_seq)
             VALUES ($1,$2,$3,$4,$5,false,$6,'absent',0,now(),0)",
        )
        .bind(org_id)
        .bind(task_id)
        .bind(attempt)
        .bind(vm_id.to_string())
        .bind(i64::try_from(epoch).map_err(|_| conflict("workspace epoch exceeds bigint"))?)
        .bind(policy_snapshot)
        .execute(db.conn())
        .await?;
        let started = sqlx::query(
            "UPDATE tasks
             SET status = 'running', attempt = $2, vm_id = $3, epoch = $4,
                 start_confirmed = false, hold_reason = NULL,
                 spec = spec #- '{lifecycle,interrupt_operation_id}'
                             #- '{lifecycle,interrupt_reason}'
                             #- '{lifecycle,interrupt_requested_at}',
                 started_at = COALESCE(started_at, now()), ended_at = NULL,
                 updated_at = now()
             WHERE id = $1 AND status = 'queued'",
        )
        .bind(task_id)
        .bind(attempt)
        .bind(vm_id.to_string())
        .bind(i64::try_from(epoch).unwrap_or(i64::MAX))
        .execute(db.conn())
        .await?;
        if started.rows_affected() != 1 {
            return Err(conflict("command task reservation lost its race"));
        }
        Ok(QueuedCandidate::Reserved(CommandDispatchRow {
            org_id,
            workspace_id,
            owner_user_id,

            task_id,
            attempt,
            vm_id,
            epoch,
            spec,
            grant_id,
            vm_catalog: candidate.try_get("module_catalog_version")?,
        }))
    }

    async fn build_command_dispatch(
        &self,
        db: &mut ScopedDb,
        row: CommandDispatchRow,
    ) -> Result<CommandDispatch, AppError> {
        validate_command_spec(&row.spec)?;
        let exec_id = canonical_exec_id(row.task_id, row.attempt, row.vm_id);
        let mut input = row.spec.input;
        // A guest from an older catalog refuses unknown preview input; it
        // runs the preview without learning its origin.
        if !catalog_supports(row.vm_catalog.as_deref(), PREVIEW_ORIGIN_CATALOG) {
            if let Some(input) = input.as_object_mut() {
                input.remove("origin");
            }
        }
        let capabilities = PREVIEW_CAPABILITIES;
        let effective = self
            .dispatch_config_service()
            .resolve_effective(
                db,
                row.org_id,
                Some(row.workspace_id),
                Some(row.owner_user_id),
            )
            .await?;
        // Admission validated this set before the reservation; see
        // `build_agent_dispatch` for why a set that turned invalid since is
        // still carried, reduced, rather than ending the pass.
        let (secrets, secret_preflight_error) =
            reconciled_dispatch_secrets(effective.secrets, false);
        if let Some(detail) = &secret_preflight_error {
            tracing::warn!(task_id = %row.task_id, attempt = row.attempt, %detail, "command re-invoke withholds secrets that no longer pass admission");
        }
        Ok(CommandDispatch {
            org_id: row.org_id,
            workspace_id: row.workspace_id,
            owner_user_id: row.owner_user_id,
            task_id: row.task_id,
            attempt: row.attempt,
            vm_id: row.vm_id,
            epoch: row.epoch,
            exec_id: exec_id.clone(),
            secret_preflight_error,
            invoke_params: json!({
                "exec_id": exec_id,
                "module_id": row.spec.module_id,
                "version": row.spec.module_version,
                "operation": row.spec.operation,
                "input_schema": row.spec.input_schema,
                "input": input,
                "grant": {
                    "grant_id": row.grant_id,
                    "capabilities": capabilities,
                },
                "secrets": secrets,
            }),
        })
    }

    async fn load_unconfirmed_agent_dispatch(
        &self,
        db: &mut ScopedDb,
    ) -> Result<Option<DispatchRow>, AppError> {
        let row = sqlx::query(
            "SELECT task.org_id, task.workspace_id, workspace.owner_user_id,
                    task.id AS task_id, task.chat_session_id, task.attempt,
                    task.vm_id, task.epoch, task.spec, task.continuation_reason,
                    session.provider_session_id,
                    input.id AS input_id, input.content, input.artifacts AS input_artifacts, input.input_origin,
                    input.revision, input.reservation_id, input.delivery_generation,
                    attempt.policy_snapshot_ref
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             JOIN workspace_vms AS vm
               ON vm.org_id = task.org_id AND vm.workspace_id = task.workspace_id
             JOIN chat_sessions AS session
               ON session.org_id = task.org_id AND session.id = task.chat_session_id
             JOIN task_attempts AS attempt
               ON attempt.org_id = task.org_id AND attempt.task_id = task.id
              AND attempt.attempt = task.attempt
             JOIN session_inputs AS input
               ON input.org_id = task.org_id AND input.task_id = task.id
              AND input.attempt = task.attempt
             WHERE task.kind = 'agent' AND task.status = 'running'
               AND NOT task.start_confirmed AND attempt.ended_at IS NULL
               AND NOT attempt.start_confirmed AND input.delivery = 'reserved'
               AND task.updated_at <= now() - interval '2 seconds'
               AND vm.host_id = ANY($1)
               AND vm.status = 'running' AND vm.vm_id = task.vm_id
               AND vm.epoch = task.epoch AND attempt.vm_id = task.vm_id
               AND attempt.epoch = task.epoch
               AND workspace.deleted_at IS NULL
               AND authz.workspace_execution_allowed(workspace.id)
               AND NOT EXISTS (
                   SELECT 1 FROM workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = task.workspace_id
                     AND transfer.status = 'pending')
             ORDER BY task.updated_at, task.id
             FOR UPDATE OF task, attempt, input SKIP LOCKED
             LIMIT 1",
        )
        .bind(self.workspace_runtime.reachable_host_ids())
        .fetch_optional(db.conn())
        .await?;
        let row = row.map(parse_dispatch_row).transpose()?;
        if let Some(row) = &row {
            crate::services::PermissionService::require_active_member_on_conn(
                db.conn(),
                row.owner_user_id,
                row.org_id,
                true,
            )
            .await
            .map_err(AppError::from_error)?;
            sqlx::query("UPDATE tasks SET updated_at = now() WHERE id = $1")
                .bind(row.task_id)
                .execute(db.conn())
                .await?;
        }
        Ok(row)
    }

    async fn reserve_queued_agent_dispatch(
        &self,
        db: &mut ScopedDb,
    ) -> Result<QueuedCandidate<DispatchRow>, AppError> {
        // A session's messages run in the order they were sent: a turn takes
        // its earliest queued message (a turn's own message is its earliest,
        // `task_lifecycle.rs::turn_message`). While another transaction (an
        // edit or a withdrawal) holds that message, the turn waits for a later
        // pass and the next candidate is tried, so other sessions go on. A
        // candidate is tried under a savepoint, and rolling back to it releases
        // a passed-over turn's rows: the transaction holding its message may be
        // waiting for them while this one goes on to rows it holds.
        let mut held_back = Vec::<Uuid>::new();
        let (candidate, first_input) = loop {
            sqlx::query("SAVEPOINT agent_dispatch_candidate")
                .execute(db.conn())
                .await?;
            // The pass reserves an agent turn in the transaction in which it may
            // first have settled an unrunnable command, which updates and can wait
            // for that command's dependents, so the lease is compared with
            // clock_timestamp(), not now(), when the transaction began.
            let candidate = sqlx::query(concat!(
                "SELECT task.org_id, task.workspace_id, workspace.owner_user_id,
                    task.id AS task_id, task.chat_session_id, task.spec,
                    task.continuation_reason, vm.vm_id, vm.epoch, vm.module_catalog_version,
                    session.provider_session_id
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             JOIN workspace_vms AS vm
               ON vm.org_id = task.org_id AND vm.workspace_id = task.workspace_id
             JOIN workspace_settings AS settings
               ON settings.workspace_id = task.workspace_id
             JOIN chat_sessions AS session
               ON session.org_id = task.org_id AND session.id = task.chat_session_id
             WHERE task.kind = 'agent' AND task.status = 'queued'
               AND task.id <> ALL($1)
               AND NOT EXISTS (SELECT 1 FROM session_inputs uncertain
                   WHERE uncertain.session_id = task.chat_session_id AND uncertain.delivery = 'delivery_unknown')
               AND (
                   task.continuation_reason IS NOT NULL
                   OR EXISTS (
                       SELECT 1 FROM session_inputs AS queued_input
                       WHERE queued_input.org_id = task.org_id
                         AND queued_input.session_id = task.chat_session_id
                         AND queued_input.task_id = task.id
                         AND queued_input.delivery = 'queued'
                         AND queued_input.input_origin <> 'continuation'
                   )
               )
               AND vm.host_id = ANY($2)
               AND vm.status = 'running' AND vm.vm_id IS NOT NULL
               AND vm.lease_expires_at > clock_timestamp() AND workspace.deleted_at IS NULL
               AND authz.workspace_execution_allowed(workspace.id)
               AND NOT EXISTS (
                   SELECT 1 FROM task_dependencies edge
                   JOIN tasks dependency
                     ON dependency.org_id = edge.org_id
                    AND dependency.id = edge.depends_on_task_id
                   WHERE edge.task_id = task.id
                     AND dependency.status <> 'succeeded')
               AND NOT EXISTS (
                   SELECT 1 FROM workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = task.workspace_id
                     AND transfer.status = 'pending')
               AND NOT EXISTS (
                   SELECT 1 FROM workspace_forks fork
                   WHERE fork.target_workspace_id = task.workspace_id
                     AND fork.status <> 'completed')
               AND (SELECT count(*) FROM tasks AS active
                    WHERE active.workspace_id = task.workspace_id
                      AND active.kind IN ('agent','command')
                      AND active.status = 'running') < ",
                workspace_slot_limit_sql!(),
                "
             ORDER BY task.created_at, task.id
             FOR UPDATE OF task, settings SKIP LOCKED
             LIMIT 1",
            ))
            .bind(&held_back)
            .bind(self.workspace_runtime.reachable_host_ids())
        .fetch_optional(db.conn())
            .await?;
            let Some(candidate) = candidate else {
                return Ok(QueuedCandidate::Exhausted);
            };
            if candidate
                .try_get::<Option<String>, _>("continuation_reason")?
                .is_some()
            {
                release_dispatch_candidate(db).await?;
                break (candidate, None);
            }
            let first_input = sqlx::query(
                "SELECT id, content, artifacts, input_origin, revision, delivery_generation,
                        launch_preferences, confirmed_launch,
                        delivery_audit @> '[{\"event\":\"reserved\"}]'::jsonb AS previously_admitted
             FROM session_inputs
             WHERE id = (
                     SELECT id FROM session_inputs
                     WHERE org_id = $1 AND session_id = $2 AND task_id = $3
                       AND delivery = 'queued'
                       AND input_origin <> 'continuation'
                     ORDER BY input_index, id
                     LIMIT 1)
               AND delivery = 'queued'
             FOR UPDATE SKIP LOCKED",
            )
            .bind(candidate.try_get::<Uuid, _>("org_id")?)
            .bind(candidate.try_get::<Option<Uuid>, _>("chat_session_id")?)
            .bind(candidate.try_get::<Uuid, _>("task_id")?)
            .fetch_optional(db.conn())
            .await?;
            if first_input.is_some() {
                release_dispatch_candidate(db).await?;
                break (candidate, first_input);
            }
            held_back.push(candidate.try_get("task_id")?);
            sqlx::query("ROLLBACK TO SAVEPOINT agent_dispatch_candidate")
                .execute(db.conn())
                .await?;
            release_dispatch_candidate(db).await?;
        };
        let task_id: Uuid = candidate.try_get("task_id")?;

        let org_id: Uuid = candidate.try_get("org_id")?;
        let workspace_id: Uuid = candidate.try_get("workspace_id")?;
        let owner_user_id = candidate
            .try_get::<Option<Uuid>, _>("owner_user_id")?
            .ok_or_else(|| conflict("workspace has no current owner"))?;
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            owner_user_id,
            org_id,
            true,
        )
        .await
        .map_err(AppError::from_error)?;
        let session_id = candidate
            .try_get::<Option<Uuid>, _>("chat_session_id")?
            .ok_or_else(|| conflict("agent task has no chat session"))?;
        if !crate::services::agents::chat_service::try_lock_session_turn_allocation(
            db.conn(),
            session_id,
        )
        .await?
        {
            // Enqueue owns the session lock and may be waiting for this task
            // row. Do not wait in the inverse order: the caller commits this
            // no-op reservation, releases the row, and a later sweep retries.
            return Ok(QueuedCandidate::Exhausted);
        }
        let vm_id = parse_uuid_text(candidate.try_get::<String, _>("vm_id")?, "workspace VM")?;
        let epoch = parse_nonnegative(candidate.try_get::<i64, _>("epoch")?, "workspace epoch")?;
        let continuation_reason: Option<String> = candidate.try_get("continuation_reason")?;
        // The restore warning follows the loss evidence, not only the
        // continuation reason: a turn a drain re-queued whose VM was then lost
        // keeps `interrupted` while its last attempt ended `vm_lost`, or while
        // the release stamped the queued turn before the save completed.
        let marker_lost = candidate
            .try_get::<Value, _>("spec")?
            .pointer("/lifecycle/interrupt_reason")
            .and_then(Value::as_str)
            == Some("vm_lost");
        let prior_attempt_lost = if marker_lost {
            true
        } else if continuation_reason.is_some() {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (
                     SELECT 1 FROM task_attempts AS prior
                     WHERE prior.task_id = $1 AND prior.ended_at IS NOT NULL
                       AND prior.reason = 'vm_lost'
                       AND prior.attempt = (
                           SELECT max(attempt) FROM task_attempts
                           WHERE task_id = $1 AND ended_at IS NOT NULL
                       )
                 )",
            )
            .bind(task_id)
            .fetch_one(db.conn())
            .await?
        } else {
            false
        };
        let mut native_answer_author = None;
        let native_question_reply = if continuation_reason.is_some() {
            sqlx::query(
                "SELECT attempt, provider_request_id, resume_session_id, response, response_author_id
                FROM provider_requests WHERE task_id = $1 AND status = 'reply_pending'
                  AND resume_session_id IS NOT NULL ORDER BY attempt DESC LIMIT 1",
            )
            .bind(task_id)
            .fetch_optional(db.conn())
            .await?
            .map(|request| {
                native_answer_author = request.try_get::<Option<Uuid>, _>("response_author_id")?;
                let response: Value = request.try_get("response")?;
                Ok::<Value, sqlx::Error>(json!({
                    "source_attempt":request.try_get::<i32,_>("attempt")?,
                    "provider_request_id":request.try_get::<String,_>("provider_request_id")?,
                    "provider_session_id":request.try_get::<String,_>("resume_session_id")?,
                    "request_id":response["request_id"], "response_digest":response["digest"],
                    "validated_response":response["body"],
                }))
            })
            .transpose()?
        } else {
            None
        };
        let mut requeue_after_admission_failure = true;
        let mut confirmed_choices_changed = false;
        let vm_catalog: Option<String> = candidate.try_get("module_catalog_version")?;
        let admission = async {
            let mut spec = parse_agent_spec(candidate.try_get("spec")?)?;
            let confirmed: Option<PersistedAgentLaunchV1> = if let Some(input) = &first_input {
                if spec.formal_review_launch.is_none() {
                    if let Some(preferences) = input.try_get::<Option<Value>, _>("launch_preferences")? {
                        spec.agent_launch = serde_json::from_value(preferences)?;
                    }
                }
                input.try_get::<Option<Value>, _>("confirmed_launch")?
                    .map(serde_json::from_value).transpose()?
            } else { None };
            let previously_admitted = first_input.as_ref()
                .map(|input| input.try_get::<bool, _>("previously_admitted"))
                .transpose()?.unwrap_or(false);
            if confirmed.is_some() {
                requeue_after_admission_failure = false;
            }
            validate_agent_spec(&spec)?;
            let review_binding: Option<(Uuid, Option<Uuid>, String, Option<Value>)> = sqlx::query_as(
                r#"
                SELECT r.id, r.task_id, r.stage, r.invocation
                FROM workspaces w JOIN agent_approval_reviews r
                  ON r.id = w.agent_approval_review_id AND r.org_id = w.org_id AND r.review_workspace_id = w.id
                WHERE w.id = $1
                "#,
            )
            .bind(workspace_id)
            .fetch_optional(db.conn())
            .await?;
            if let Some((_, bound, stage, invocation)) = review_binding {
                if bound != Some(task_id)
                    || !["start", "execute"].contains(&stage.as_str())
                    || spec
                        .formal_review
                        .as_ref()
                        .map(serde_json::to_value)
                        .transpose()?
                        != invocation
                {
                    requeue_after_admission_failure = false;
                    return Err(conflict("temporary review admits only its assigned active formal task"));
                }
            } else if spec.formal_review.is_some() {
                requeue_after_admission_failure = false;
                return Err(conflict("formal review task has no assigned temporary workspace"));
            }
            let accounts = ProviderAccountService::new(
                self.dispatch_system_pool().clone(),
                self.dispatch_config_service().clone(),
            );
            if spec.formal_review.is_some() {
                let attempted: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM task_attempts WHERE task_id = $1)",
                )
                .bind(task_id)
                .fetch_one(db.conn())
                .await?;
                if attempted {
                    requeue_after_admission_failure = false;
                    return Err(conflict("formal review never automatically reruns an agent"));
                }
            }
            let (effective, launch) = if let Some(snapshot) = &spec.formal_review_launch {
                let launch = resolved_launch(snapshot)
                    .ok_or_else(|| conflict("frozen retry launch is invalid"))?;
                let effective = self
                    .dispatch_config_service()
                    .resolve_effective_for_authorized_frozen_launch(
                        db,
                        org_id,
                        workspace_id,
                        owner_user_id,
                        &launch,
                        &accounts,
                    )
                    .await?;
                (effective, launch)
            } else if let Some(confirmed) = confirmed.as_ref().filter(|_| previously_admitted) {
                let effective = self.dispatch_config_service()
                    .resolve_effective_for_authorized_frozen_launch(db, org_id, workspace_id,
                        owner_user_id, &confirmed.resolved, &accounts).await?;
                (effective, confirmed.resolved.clone())
            } else {
                let effective = self
                    .dispatch_config_service()
                    .resolve_effective_for_agent_launch(
                        db,
                        org_id,
                        Some(workspace_id),
                        Some(owner_user_id),
                        &[],
                        &spec.agent_launch,
                        &accounts,
                    )
                    .await?;
                if let Some(confirmed) = &confirmed {
                    // Start still checks the full resolution id. Queued first
                    // admission compares only the choices the owner saw, not
                    // unrelated settings, secrets or resolution metadata.
                    let current = ConfigService::preview_agent_launch(&effective, &spec.agent_launch)?;
                    let changes = changed_confirmed_launch_choices(&confirmed.resolved, &current);
                    if !changes.is_empty() {
                        confirmed_choices_changed = true;
                        return Err(conflict(format!("Confirmed launch choices changed: {}", changes.join("; "))));
                    }
                }
                let launch = ConfigService::resolve_agent_launch_for_dispatch(&effective, &spec.agent_launch)?;
                (effective, launch)
            };
            ensure_launch_is_runnable(&launch)?;
            // The effective secrets are admitted here, before anything is
            // reserved (`g5.chat.secret-admission`): a stored invalid
            // configuration settles this task visibly and keeps its queued
            // input for correction, since a follow-up would copy the same
            // configuration and fail every pass.
            if let Err(error) = validate_dispatch_secrets(&effective.secrets, spec.formal_review.is_some()) {
                requeue_after_admission_failure = false;
                return Err(conflict(error.to_string()));
            }
            if let Some(answer) = &native_question_reply {
                if native_answer_author != Some(owner_user_id) {
                    return Err(conflict(
                        "pending answer was accepted from a previous workspace owner",
                    ));
                }
                let session_id: Option<String> = candidate.try_get("provider_session_id")?;
                if launch.provider.value != ChatAgentProvider::ClaudeCode
                    || answer["provider_session_id"].as_str() != session_id.as_deref()
                {
                    return Err(conflict(
                        "pending question does not match the native session",
                    ));
                }
            }

            if launch.provider.value == ChatAgentProvider::Codex && spec.limits.max_turns.is_some()
            {
                return Err(conflict("Codex does not support the max_turns limit"));
            }
            let permission_policy = permission_policy(&launch, &spec).inspect_err(|_| {
                // A follow-up would copy this same unsupported policy and fail
                // again. Keep the queued input attached for explicit correction.
                requeue_after_admission_failure = false;
            })?;
            let formal_schema = spec
                .formal_review
                .as_ref()
                .map(|review| review.resolution.schema_version);
            // `g5.agent-approval.contract-rollout`: a schema 5 or 6 review
            // never reaches a guest whose catalog lacks its module.
            let required_catalog = match formal_schema {
                Some(APPROVAL_SUMMARY_SCHEMA) => Some(APPROVAL_SUMMARY_CATALOG),
                Some(APPROVAL_REPORT_ORDER_SCHEMA) => Some(APPROVAL_REPORT_ORDER_CATALOG),
                _ => None,
            };
            if required_catalog
                .is_some_and(|catalog| !catalog_supports(vm_catalog.as_deref(), catalog))
            {
                requeue_after_admission_failure = false;
                return Err(conflict(REVIEW_CONTRACT_RUNTIME_REFUSAL));
            }
            // `g5.chat.codex-runtime-refusal`: Codex runs only over App Server.
            // A VM started before its module never receives exec instead.
            if launch.provider.value == ChatAgentProvider::Codex {
                let required = if spec.formal_review.is_some() {
                    CODEX_FORMAL_APP_SERVER_CATALOG
                } else {
                    CODEX_APP_SERVER_CATALOG
                };
                if !catalog_supports(vm_catalog.as_deref(), required) {
                    requeue_after_admission_failure = false;
                    return Err(conflict(if spec.formal_review.is_some() {
                        CODEX_REVIEW_RUNTIME_REFUSAL
                    } else {
                        CODEX_RUNTIME_REFUSAL
                    }));
                }
            }
            let provider_base_url = if let Some(snapshot)=&spec.formal_review_launch {
                snapshot.get("provider_base_url").and_then(Value::as_str).map(str::to_string)
            } else {configured_provider_base_url(&launch, &effective.settings)};
            Ok::<_, AppError>((spec, launch, permission_policy, provider_base_url))
        }
        .await;
        let (mut spec, launch, permission_policy, provider_base_url) = match admission {
            Ok(admission) => admission,
            Err(error) => {
                settle_dispatch_admission_failure(db, org_id, workspace_id, task_id, &error)
                    .await?;
                if requeue_after_admission_failure {
                    crate::services::agents::task_lifecycle::ensure_next_queued_agent_task(
                        db,
                        org_id,
                        workspace_id,
                        Some(session_id),
                        task_id,
                    )
                    .await?;
                } else {
                    // The owner must correct something before this input can
                    // run; say so in the chat, not only in a server log.
                    record_admission_refusal_notice(
                        db,
                        org_id,
                        session_id,
                        task_id,
                        &error,
                        confirmed_choices_changed,
                    )
                    .await?;
                }
                tracing::warn!(%task_id, %workspace_id, %error, "agent dispatch admission failed permanently");
                self.announce_chat_activity(db, task_id).await?;
                return Ok(QueuedCandidate::Settled);
            }
        };
        let grant_id = Uuid::new_v4();
        sqlx::query("UPDATE tasks SET spec=jsonb_set(spec,'{agent,agentLaunch}',$2) WHERE id=$1")
            .bind(task_id)
            .bind(serde_json::to_value(&spec.agent_launch)?)
            .execute(db.conn())
            .await?;
        let attempt = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(MAX(attempt), 0) + 1
             FROM task_attempts WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(db.conn())
        .await?;
        // Persist the `{preferences, resolved}` envelope that chat_service's
        // persisted-launch readers and the attempt DTO decode, so every reader
        // of this member agrees on one shape. Rows written before this carry
        // the flat resolved contract; the readers accept either form.
        let persisted_launch = PersistedAgentLaunchV1 {
            preferences: spec.agent_launch.clone(),
            resolved: launch.clone(),
        };
        let mut policy_snapshot = json!({
            "schema_version": ATTEMPT_POLICY_SCHEMA,
            "grant_id": grant_id,
            "agent_launch": persisted_launch,
            "provider_base_url": provider_base_url,
            "permission_policy": permission_policy,
            "session_files_state": "absent",
        });
        // This is a frozen protocol capability, not a rollout setting: every
        // new Claude process supports native steering. Older attempts
        // must retain the capability with which they were actually launched.
        policy_snapshot["native_mid_turn_steering"] =
            json!(launch.provider.value == ChatAgentProvider::ClaudeCode);
        // Every new Codex attempt freezes App Server; admission above already
        // refused a VM whose catalog lacks the required module.
        // Older formal schemas keep the module they shipped with; schemas 5
        // and 6 freeze their own (`g5.agent-approval.contract-rollout`).
        let formal_schema = spec
            .formal_review
            .as_ref()
            .map(|review| review.resolution.schema_version);
        let frozen_module = if launch.provider.value == ChatAgentProvider::Codex {
            policy_snapshot["codex_transport"] = json!(CODEX_APP_SERVER_TRANSPORT);
            match formal_schema {
                Some(APPROVAL_REPORT_ORDER_SCHEMA) => {
                    CODEX_FORMAL_REPORT_ORDER_APP_SERVER_MODULE_VERSION
                }
                Some(APPROVAL_SUMMARY_SCHEMA) => CODEX_FORMAL_SUMMARY_APP_SERVER_MODULE_VERSION,
                Some(_) => CODEX_FORMAL_APP_SERVER_MODULE_VERSION,
                None => CODEX_APP_SERVER_MODULE_VERSION,
            }
        } else {
            match formal_schema {
                Some(APPROVAL_REPORT_ORDER_SCHEMA) => CLAUDE_FORMAL_REPORT_ORDER_MODULE_VERSION,
                Some(APPROVAL_SUMMARY_SCHEMA) => CLAUDE_FORMAL_SUMMARY_MODULE_VERSION,
                _ => AGENT_MODULE_VERSION,
            }
        };
        policy_snapshot["agent_module_version"] = json!(frozen_module);
        if let Some(answer) = &native_question_reply {
            policy_snapshot["native_question_reply"] = answer.clone();
        }
        let (
            input_id,
            input_content,
            input_artifacts,
            input_origin,
            input_revision,
            delivery_generation,
        ) = if let Some(reason) = continuation_reason.as_deref() {
            (
                Uuid::new_v4(),
                if native_question_reply.is_some() {
                    // Shown in the queued tray while the answer travels and
                    // as the system row once it is delivered; never sent to
                    // the provider, which receives only the initialize frame
                    // and the native reply
                    // (`g5.chat.native-question-delivery-state`).
                    NATIVE_ANSWER_CONTINUATION_LABEL.to_string()
                } else {
                    continuation_instruction(reason, prior_attempt_lost)
                },
                json!([]),
                "continuation".to_string(),
                1,
                1,
            )
        } else {
            let input = first_input.ok_or_else(|| conflict("queued agent turn has no message"))?;
            (
                input.try_get("id")?,
                input.try_get("content")?,
                input.try_get("artifacts")?,
                input.try_get("input_origin")?,
                input.try_get("revision")?,
                input
                    .try_get::<i32, _>("delivery_generation")?
                    .checked_add(1)
                    .ok_or_else(|| conflict("input delivery generation overflow"))?,
            )
        };
        // Persona bootstrap (workbench g5.workbench.persona): a persona
        // session's first message carries the frozen persona instructions. If
        // that input was withdrawn or edited before delivery, or the session
        // is a fork whose copied transcript predates the prefix, the first
        // input delivered here is prefixed from `chat_sessions.
        // persona_instructions`, so a persona-labelled session never reaches
        // the provider without them. The persisted input is updated with the
        // reservation so the transcript row and any recovery read the same
        // bytes, and the byte cap below applies to the prefixed content.
        let input_content = if continuation_reason.is_none() {
            bootstrap_persona_prefix(db, session_id, input_content).await?
        } else {
            input_content
        };
        if input_content.len() > MAX_PROMPT_BYTES {
            // Enqueue and edit cap the expanded size, so this is a row that
            // slipped past them (an older input, or instructions frozen after
            // it was queued). Settle it as an isolated, visible admission
            // failure instead of erroring the pass: an error here would abort
            // the tenant's dispatch pass and every later pass would pick the
            // same task again, stalling every other session.
            let error = conflict(
                "agent input exceeds the prompt byte limit once the persona instructions are prepended; shorten or withdraw it",
            );
            settle_dispatch_admission_failure(db, org_id, workspace_id, task_id, &error).await?;
            tracing::warn!(%task_id, %workspace_id, %error, "agent dispatch refused an oversized persona input");
            self.announce_chat_activity(db, task_id).await?;
            return Ok(QueuedCandidate::Settled);
        }
        let reservation_id = Uuid::new_v4();
        spec.system_notice_ids = if native_question_reply.is_some() {
            vec![]
        } else {
            reserve_system_notices(
                db,
                session_id,
                task_id,
                attempt,
                MAX_PROMPT_BYTES.saturating_sub(input_content.len()),
            )
            .await?
        };
        let mut row = DispatchRow {
            org_id,
            workspace_id,
            owner_user_id,
            task_id,
            session_id,
            attempt,
            vm_id,
            epoch,
            task_spec: spec,
            continuation_reason,
            provider_session_id: candidate.try_get("provider_session_id")?,
            input_id,
            input_content,
            input_artifacts,
            input_origin,
            input_revision,
            reservation_id,
            delivery_generation,
            policy_snapshot: policy_snapshot.clone(),
        };
        let current_input = render_system_notice_preamble(
            db,
            row.session_id,
            &row.task_spec.system_notice_ids,
            &row.input_content,
        )
        .await?;
        let frozen = if native_question_reply.is_some() {
            FrozenRecoveryPrompt {
                text: String::new(),
                current_input: Some(String::new()),
                recovery: None,
                evidence_cutoff: None,
            }
        } else {
            freeze_recovery_prompt(db, &row, current_input).await?
        };
        policy_snapshot["frozen_recovery_prompt"] = serde_json::to_value(frozen)?;
        row.policy_snapshot = policy_snapshot.clone();
        sqlx::query(
            "INSERT INTO task_attempts(
                 org_id, task_id, attempt, vm_id, epoch, start_confirmed,
                 policy_snapshot_ref, session_files_state,
                 session_manifest_generation, started_at, acked_event_seq, codex_auth_state)
             VALUES ($1,$2,$3,$4,$5,false,$6,'absent',0,now(),0,$7)",
        )
        .bind(org_id)
        .bind(task_id)
        .bind(attempt)
        .bind(vm_id.to_string())
        .bind(i64::try_from(epoch).map_err(|_| conflict("workspace epoch exceeds bigint"))?)
        .bind(policy_snapshot)
        // Arm the gate before dispatch, so the attempt counts as undelivered
        // until login reports ready, even before the module's first HTTP
        // request (`g5.chat.login-before-prompt`).
        .bind(
            (launch.auth_mode == g5_contracts::types::AgentAuthModeV1::ChatGptSubscription)
                .then(|| json!({"phase":"checking"})),
        )
        .execute(db.conn())
        .await?;
        if let Some(reason) = row.continuation_reason.as_deref() {
            sqlx::query(
                "INSERT INTO session_inputs(
                     id, org_id, session_id, input_index, content, artifacts,
                     revision, input_origin, delivery, task_id, attempt,
                     delivery_audit)
                 VALUES ($1,$2,$3,
                         (SELECT COALESCE(MAX(input_index), -1) + 1
                          FROM session_inputs WHERE session_id = $3),
                         $4,'[]'::jsonb,1,'continuation','queued',$5,$6,
                         jsonb_build_array(jsonb_build_object(
                             'event','continuation_created','reason',$7::text)))",
            )
            .bind(row.input_id)
            .bind(org_id)
            .bind(session_id)
            .bind(&row.input_content)
            .bind(task_id)
            .bind(attempt)
            .bind(reason)
            .execute(db.conn())
            .await?;
        }
        let reserved = sqlx::query(
            "UPDATE session_inputs
             SET delivery = 'reserved', steered = false, reservation_id = $2,
                 reservation_revision = revision, delivery_generation = $3,
                 task_id = $4, attempt = $5, content = $8,
                 confirmed_launch = jsonb_set(confirmed_launch, '{resolved}', $9::jsonb),
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object('event','reserved','reservation_id',$2,
                                        'attempt',$5,'vm_id',$6))
             WHERE id = $1 AND delivery = 'queued' AND revision = $7",
        )
        .bind(row.input_id)
        .bind(row.reservation_id)
        .bind(row.delivery_generation)
        .bind(task_id)
        .bind(attempt)
        .bind(vm_id.to_string())
        .bind(row.input_revision)
        .bind(&row.input_content)
        // Freeze the admitted resolution with the reservation. An unrelated
        // config revision may differ from Start; retries must retain this exact
        // resolution. jsonb_set preserves NULL for unconfirmed inputs.
        .bind(serde_json::to_value(&launch)?)
        .execute(db.conn())
        .await?;
        if reserved.rows_affected() != 1 {
            return Err(conflict("session input reservation lost its race"));
        }
        let system_notice_ids = serde_json::to_value(&row.task_spec.system_notice_ids)?;
        let started = sqlx::query(
            "UPDATE tasks
             SET status = 'running', attempt = $2, vm_id = $3, epoch = $4,
                 start_confirmed = false, hold_reason = NULL,
                 spec = jsonb_set(
                     spec #- '{lifecycle,interrupt_operation_id}'
                          #- '{lifecycle,interrupt_reason}'
                          #- '{lifecycle,interrupt_requested_at}',
                     '{agent,systemNoticeIds}', $5::jsonb, true),
                 started_at = COALESCE(started_at, now()), ended_at = NULL,
                 updated_at = now()
             WHERE id = $1 AND status = 'queued'",
        )
        .bind(task_id)
        .bind(attempt)
        .bind(vm_id.to_string())
        .bind(i64::try_from(epoch).unwrap_or(i64::MAX))
        .bind(system_notice_ids)
        .execute(db.conn())
        .await?;
        if started.rows_affected() != 1 {
            return Err(conflict("agent task reservation lost its race"));
        }
        Ok(QueuedCandidate::Reserved(row))
    }

    /// Decode the task owner's terminal receipt and expose only durable review
    /// evidence/provenance, without sharing its policy-row storage contract.
    pub(crate) async fn formal_approval_receipt(
        &self,
        db: &mut ScopedDb,
        org: Uuid,
        task: Uuid,
    ) -> Result<(Value, Value), AppError> {
        Ok(sqlx::query_as(
            r#"
            SELECT outcome, jsonb_build_object(
                'launch', policy_snapshot_ref->'agent_launch', 'taskId', task_id,
                'attempt', attempt, 'vmId', vm_id, 'epoch', epoch,
                'startedAt', started_at, 'endedAt', ended_at
            )
            FROM task_attempts
            WHERE task_id = $1 AND org_id = $2 AND attempt = 1 AND ended_at IS NOT NULL
            "#,
        )
        .bind(task)
        .bind(org)
        .fetch_one(db.conn())
        .await?)
    }

    pub(crate) async fn bind_formal_approval(
        &self,
        db: &mut ScopedDb,
        review_id: Uuid,
        task_id: Uuid,
        invocation: &g5_contracts::types::FormalApprovalInvocation,
    ) -> Result<(), AppError> {
        invocation.validate().map_err(conflict)?;
        let changed = sqlx::query(
            r#"
            UPDATE tasks task SET spec = jsonb_set(task.spec,'{agent,formalReview}',$3)
            FROM agent_approval_reviews review
            WHERE review.id = $1 AND review.org_id = task.org_id
              AND review.review_workspace_id = task.workspace_id AND review.stage = 'start'
              AND task.id = $2 AND task.kind = 'agent' AND task.status = 'queued'
              AND NOT EXISTS(SELECT 1 FROM task_attempts attempt WHERE attempt.task_id = task.id)
            "#,
        )
        .bind(review_id)
        .bind(task_id)
        .bind(serde_json::to_value(invocation)?)
        .execute(db.conn())
        .await?;
        if changed.rows_affected() != 1 {
            return Err(conflict("formal review cannot bind this task"));
        }
        let prior: Option<Value> = sqlx::query_scalar(
            r#"
            SELECT attempt.policy_snapshot_ref
            FROM agent_approval_reviews current
            JOIN agent_approval_reviews prior
              ON prior.id = current.retry_of AND prior.org_id = current.org_id
            JOIN task_attempts attempt
              ON attempt.task_id = prior.task_id AND attempt.org_id = prior.org_id
             AND attempt.attempt = 1
            WHERE current.id = $1
            "#,
        )
        .bind(review_id)
        .fetch_optional(db.conn())
        .await?
        .flatten();
        if let Some(snapshot) = prior {
            let launch = resolved_launch(&snapshot)
                .ok_or_else(|| conflict("retry launch history is invalid"))?;
            let mut preferences = crate::services::config_service::legacy_agent_launch_preferences(
                Some(launch.provider.value),
                Some(&launch.model.value),
                Some(&launch.permission_mode.value),
                Some(&launch.effort.value),
            );
            preferences.account = g5_contracts::types::AgentLaunchFieldPreferenceV1::Explicit(
                launch.account_id.map_or(
                    g5_contracts::types::AgentAccountSelectionV1::RuntimeConfig,
                    |account_id| g5_contracts::types::AgentAccountSelectionV1::Account {
                        account_id,
                    },
                ),
            );
            sqlx::query(
                r#"
                UPDATE tasks
                SET spec = jsonb_set(jsonb_set(spec,'{agent,formalReviewLaunch}',$2),'{agent,agentLaunch}',$3)
                WHERE id = $1 AND status = 'queued'
                "#,
            )
            .bind(task_id)
            .bind(snapshot)
            .bind(serde_json::to_value(preferences)?)
            .execute(db.conn())
            .await?;
        }
        Ok(())
    }

    async fn build_agent_dispatch(
        &self,
        db: &mut ScopedDb,
        row: DispatchRow,
    ) -> Result<AgentDispatch, AppError> {
        let launch = resolved_launch(&row.policy_snapshot)
            .ok_or_else(|| conflict("attempt agent launch snapshot is invalid"))?;
        let permission_policy = row.policy_snapshot["permission_policy"].clone();
        if !permission_policy.is_object() {
            return Err(conflict("attempt permission policy snapshot is invalid"));
        }
        let grant_id = row.policy_snapshot["grant_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| conflict("attempt grant identity is invalid"))?;
        let accounts = ProviderAccountService::new(
            self.dispatch_system_pool().clone(),
            self.dispatch_config_service().clone(),
        );
        let effective = self
            .dispatch_config_service()
            .resolve_effective_for_authorized_frozen_launch(
                db,
                row.org_id,
                row.workspace_id,
                row.owner_user_id,
                &launch,
                &accounts,
            )
            .await?;
        // The endpoint is attempt configuration, while credentials are
        // reauthorized for the current owner and can rotate at dispatch.
        // Pre-upgrade snapshots did not capture an endpoint.
        let provider_base_url = match row.policy_snapshot.get("provider_base_url") {
            Some(Value::Null) => None,
            Some(Value::String(base)) => Some(base.clone()),
            Some(_) => return Err(conflict("attempt provider endpoint snapshot is invalid")),
            None => configured_provider_base_url(&launch, &effective.settings),
        };
        // Admission validated this set before the reservation. A set that
        // turned invalid since (a bypassed writer, or a policy that tightened
        // across a deploy while this attempt stayed unconfirmed) must still
        // reconcile the invoke that may already have reached the guest:
        // reservation makes no network calls, so the request carries the
        // entries that pass on their own plus a preflight error. A guest that
        // already runs the execution answers from its own state; one that
        // does not fails the turn before launch with that sentence.
        let (secrets, secret_preflight_error) =
            reconciled_dispatch_secrets(effective.secrets, row.task_spec.formal_review.is_some());
        let policy_bytes = serde_json::to_vec(&permission_policy)?;
        let policy_ref = format!("sha256:{:x}", Sha256::digest(policy_bytes));
        let frozen = if let Some(value) = row.policy_snapshot.get("frozen_recovery_prompt") {
            serde_json::from_value::<FrozenRecoveryPrompt>(value.clone())
                .map_err(|_| conflict("frozen recovery prompt is invalid"))?
        } else {
            // Pre-upgrade unconfirmed attempts have no immutable recovery
            // snapshot. Their policy row cannot be amended after reservation.
            let current_input = render_system_notice_preamble(
                db,
                row.session_id,
                &row.task_spec.system_notice_ids,
                &row.input_content,
            )
            .await?;
            FrozenRecoveryPrompt {
                text: current_input,
                current_input: None,
                recovery: Some(RecoveryNotice {
                    included_messages: 0,
                    omitted_messages: 0,
                    truncated_messages: 0,
                    attachment_references: 0,
                    omitted_attachment_references: 0,
                    unavailable_reason: Some(
                        "This attempt predates frozen recovery evidence and cannot resume safely."
                            .to_string(),
                    ),
                }),
                evidence_cutoff: None,
            }
        };
        let prompt_sha = format!("{:x}", Sha256::digest(frozen.text.as_bytes()));
        let resume_prompt = frozen.current_input.as_ref().map(|text| {
            json!({
                "text": text,
                "sha256": format!("{:x}", Sha256::digest(text.as_bytes())),
            })
        });
        let has_resume_prompt = resume_prompt.is_some();
        let continuation = continuation_binding(db, &row).await?;
        let session_title_wanted = row.task_spec.formal_review.is_none()
            && session_title_wanted(db, row.org_id, row.session_id).await?;
        let exec_id = canonical_exec_id(row.task_id, row.attempt, row.vm_id);
        let has_recovery = frozen.recovery.is_some();
        // A malformed stored manifest must settle this reserved input visibly;
        // failing construction here would roll back reservation and poison the
        // tenant's dispatch loop on every retry.
        let (attachments, attachment_preflight_error) =
            match frozen_attachment_descriptors(row.input_id, &row.input_artifacts) {
                Ok(attachments) => (attachments, None),
                Err(_) => (vec![], Some("reserved attachment descriptors are invalid")),
            };
        let mut input = json!({
            "schema_version": AGENT_INPUT_SCHEMA,
            "task": {
                "task_id": row.task_id,
                "attempt": row.attempt,
                "session_id": row.session_id,
                "input_origin": row.input_origin,
                "initial_input": {
                    "reservation_id": row.reservation_id,
                    "input_id": row.input_id,
                    "revision": row.input_revision,
                    "delivery_generation": row.delivery_generation,
                    "input_origin": row.input_origin,
                    "system_notice_ids": row.task_spec.system_notice_ids,
                }
            },
            "provider": provider_dispatch_input(
                &launch, provider_base_url.as_deref(), &policy_ref, permission_policy, session_title_wanted,
            ),
            "prompt": {"text":frozen.text,"sha256":prompt_sha},
            "resume_prompt": resume_prompt,
            "attachments": attachments,
            "attachment_preflight_error": attachment_preflight_error,
            "workspace_target": workspace_target_input(&row.task_spec.workspace_target),
            "session_binding": {
                "session_files_state": "absent",
                "provider_session_id": row.provider_session_id,
                "expected_manifest_generation": null,
            },
            "recovery": frozen.recovery,
            "continuation": continuation,
            "limits": {
                "setup_timeout_ms": AGENT_SETUP_TIMEOUT_MS,
                "max_turns": row.task_spec.limits.max_turns,
                "max_output_bytes": row.task_spec.limits.max_output_bytes,
            }
        });
        // Older guests reject unknown input fields, so the key travels only
        // when there is something to say.
        if let Some(detail) = &secret_preflight_error {
            input["secret_preflight_error"] = json!(detail);
        }
        if let Some(review) = &row.task_spec.formal_review {
            input["provider"]["options"]["formal_review"] = serde_json::to_value(review)?;
            input["provider"]["options"]["session_title"] = json!(false);
        }
        if let Some(answer) = row.policy_snapshot.get("native_question_reply") {
            input["native_question_reply"] = answer.clone();
        }
        input["provider"]["options"]["native_mid_turn_steering"] =
            json!(row.policy_snapshot["native_mid_turn_steering"] == true);
        // Never infer transport from today's guest or latest code when rebuilding
        // a reservation. Absent snapshot fields are the historical exec contract.
        let (module_version, app_server) = frozen_module(
            &row.policy_snapshot,
            row.task_spec
                .formal_review
                .as_ref()
                .map(|review| review.resolution.schema_version),
        )?;
        if app_server {
            input["provider"]["options"]["codex_transport"] = json!(CODEX_APP_SERVER_TRANSPORT);
        }
        if !has_resume_prompt {
            input
                .as_object_mut()
                .expect("agent input is an object")
                .remove("resume_prompt");
        }
        if !has_recovery {
            input
                .as_object_mut()
                .expect("agent input is an object")
                .remove("recovery");
        }
        Ok(AgentDispatch {
            org_id: row.org_id,
            workspace_id: row.workspace_id,
            owner_user_id: row.owner_user_id,
            task_id: row.task_id,
            session_id: row.session_id,
            attempt: row.attempt,
            vm_id: row.vm_id,
            epoch: row.epoch,
            exec_id: exec_id.clone(),
            secret_preflight_error,
            invoke_params: json!({
                "exec_id": exec_id,
                "module_id": "agent.turn",
                "version": module_version,
                "operation": "run",
                "input_schema": AGENT_INPUT_SCHEMA,
                "input": input,
                "grant": {
                    "grant_id": grant_id,
                    "capabilities": AGENT_CAPABILITIES,
                },
                "secrets": secrets,
            }),
        })
    }
}

fn frozen_attachment_descriptors(
    input_id: Uuid,
    input_artifacts: &Value,
) -> Result<Vec<Value>, AppError> {
    let contexts: Vec<g5_contracts::types::ChatAttachmentContext> =
        serde_json::from_value(input_artifacts.clone())
            .map_err(|_| conflict("reserved input attachment descriptors are invalid"))?;
    let total_bytes = contexts
        .iter()
        .try_fold(0i64, |total, item| total.checked_add(item.size_bytes));
    if contexts.len() > 8 || total_bytes.is_none_or(|total| total > 10 * 1024 * 1024) {
        return Err(conflict("reserved attachment set exceeds chat limits"));
    }
    let mut seen = std::collections::HashSet::new();
    contexts
        .into_iter()
        .map(|attachment| {
            let id = Uuid::parse_str(&attachment.id)
                .map_err(|_| conflict("reserved attachment ID is invalid"))?;
            if !seen.insert(id)
                || attachment.size_bytes <= 0
                || attachment.sha256.len() != 64
                || !attachment
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(conflict("reserved attachment identity is invalid"));
            }
            let kind = match attachment.content_type.as_deref() {
                Some(mime) if mime.to_ascii_lowercase().starts_with("image/") => "image",
                Some(_) => "file",
                None => "reference",
            };
            Ok(json!({
                "kind": kind,
                "id": id,
                "input_id": input_id,
                "file_name": attachment.file_name,
                "content_type": attachment.content_type,
                "size_bytes": attachment.size_bytes,
                "sha256": attachment.sha256,
            }))
        })
        .collect()
}

async fn assert_session_owner(
    db: &mut ScopedDb,
    session_id: Uuid,
    actor_user_id: Uuid,
) -> Result<(), AppError> {
    let owner_user_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT workspace.owner_user_id
         FROM chat_sessions AS session
         JOIN workspaces AS workspace
           ON workspace.org_id = session.org_id AND workspace.id = session.workspace_id
         WHERE session.id = $1 AND workspace.deleted_at IS NULL
               AND authz.workspace_execution_allowed(workspace.id)",
    )
    .bind(session_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| conflict("agent session is absent"))?;
    if owner_user_id != Some(actor_user_id) {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    Ok(())
}

/// Builds the view from a `session_input_view_sql!` row. The permission flags
/// mirror the admission predicates of `edit_queued_session_input` and
/// `withdraw_queued_session_input` so the UI offers exactly what those
/// methods would accept at read time.
fn session_input_view(row: sqlx::postgres::PgRow) -> Result<SessionInputView, AppError> {
    let input_origin: String = row.try_get("input_origin")?;
    let delivery: String = row.try_get("delivery")?;
    let steered: bool = row.try_get("steered")?;
    let possibly_delivered: bool = row.try_get("possibly_delivered")?;
    let withdrawable = input_origin == "human" && delivery == "queued" && !steered;
    Ok(SessionInputView {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        input_index: row.try_get("input_index")?,
        content: row.try_get("content")?,
        artifacts: row.try_get("artifacts")?,
        revision: row.try_get("revision")?,
        input_origin,
        delivery,
        delivery_generation: row.try_get("delivery_generation")?,
        task_id: row.try_get("task_id")?,
        attempt: row.try_get("attempt")?,
        created_at: row.try_get("created_at")?,
        delivered_at: row.try_get("delivered_at")?,
        withdrawn_at: row.try_get("withdrawn_at")?,
        editable: withdrawable && !possibly_delivered,
        withdrawable,
    })
}

fn parse_dispatch_row(row: sqlx::postgres::PgRow) -> Result<DispatchRow, AppError> {
    let task_spec = parse_agent_spec(row.try_get("spec")?)?;
    validate_agent_spec(&task_spec)?;
    Ok(DispatchRow {
        org_id: row.try_get("org_id")?,
        workspace_id: row.try_get("workspace_id")?,
        owner_user_id: row
            .try_get::<Option<Uuid>, _>("owner_user_id")?
            .ok_or_else(|| conflict("workspace has no current owner"))?,
        task_id: row.try_get("task_id")?,
        session_id: row
            .try_get::<Option<Uuid>, _>("chat_session_id")?
            .ok_or_else(|| conflict("agent task has no chat session"))?,
        attempt: row.try_get("attempt")?,
        vm_id: parse_uuid_text(row.try_get::<String, _>("vm_id")?, "workspace VM")?,
        epoch: parse_nonnegative(row.try_get::<i64, _>("epoch")?, "task epoch")?,
        task_spec,
        continuation_reason: row.try_get("continuation_reason")?,
        provider_session_id: row.try_get("provider_session_id")?,
        input_id: row.try_get("input_id")?,
        input_content: row.try_get("content")?,
        input_artifacts: row.try_get("input_artifacts")?,
        input_origin: row.try_get("input_origin")?,
        input_revision: row.try_get("revision")?,
        reservation_id: row
            .try_get::<Option<Uuid>, _>("reservation_id")?
            .ok_or_else(|| conflict("reserved input has no reservation identity"))?,
        delivery_generation: row.try_get("delivery_generation")?,
        policy_snapshot: row
            .try_get::<Option<Value>, _>("policy_snapshot_ref")?
            .ok_or_else(|| conflict("attempt has no immutable policy snapshot"))?,
    })
}

fn parse_command_dispatch_row(row: sqlx::postgres::PgRow) -> Result<CommandDispatchRow, AppError> {
    let spec = parse_command_spec(row.try_get("spec")?)?;
    validate_command_spec(&spec)?;
    let policy_snapshot = row
        .try_get::<Option<Value>, _>("policy_snapshot_ref")?
        .ok_or_else(|| conflict("command attempt has no immutable policy snapshot"))?;
    let grant_id = policy_snapshot["grant_id"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| conflict("command attempt grant identity is invalid"))?;
    Ok(CommandDispatchRow {
        org_id: row.try_get("org_id")?,
        workspace_id: row.try_get("workspace_id")?,
        owner_user_id: row
            .try_get::<Option<Uuid>, _>("owner_user_id")?
            .ok_or_else(|| conflict("workspace has no current owner"))?,

        task_id: row.try_get("task_id")?,
        attempt: row.try_get("attempt")?,
        vm_id: parse_uuid_text(row.try_get::<String, _>("vm_id")?, "workspace VM")?,
        epoch: parse_nonnegative(row.try_get::<i64, _>("epoch")?, "task epoch")?,
        spec,
        grant_id,
        vm_catalog: row.try_get("module_catalog_version")?,
    })
}

async fn continuation_binding(
    db: &mut ScopedDb,
    row: &DispatchRow,
) -> Result<Option<Value>, AppError> {
    let Some(reason) = row.continuation_reason.as_deref() else {
        return Ok(None);
    };
    if row.attempt <= 1 {
        return Err(conflict("first attempt cannot be a continuation"));
    }
    let prior_vm = sqlx::query_scalar::<_, String>(
        "SELECT vm_id FROM task_attempts
         WHERE task_id = $1 AND attempt = $2 AND ended_at IS NOT NULL",
    )
    .bind(row.task_id)
    .bind(row.attempt - 1)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| conflict("continuation has no settled prior attempt"))?;
    let prior_vm = parse_uuid_text(prior_vm, "prior attempt VM")?;
    Ok(Some(json!({
        "prior_exec_id": canonical_exec_id(row.task_id, row.attempt - 1, prior_vm),
        "reason": reason,
    })))
}

fn parse_agent_spec(spec: Value) -> Result<AgentTaskSpecV1, AppError> {
    let value = spec
        .get("agent")
        .cloned()
        .ok_or_else(|| conflict("agent task spec is absent"))?;
    serde_json::from_value(value)
        .map_err(|error| conflict(format!("agent task spec is invalid: {error}")))
}

async fn settle_dispatch_admission_failure(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    task_id: Uuid,

    error: &AppError,
) -> Result<(), AppError> {
    let detail = error.to_string().chars().take(2_048).collect::<String>();
    sqlx::query(
        "UPDATE tasks
         SET status = 'failed',
             outcome = jsonb_build_object(
                 'status','failed',
                 'reason','dispatch_admission_failed',
                 'detail',$2),
             hold_reason = NULL, continuation_reason = NULL,
             ended_at = now(), updated_at = now()
         WHERE id = $1 AND status = 'queued'",
    )
    .bind(task_id)
    .bind(&detail)
    .execute(db.conn())
    .await?;
    sqlx::query(
        "UPDATE provider_requests SET status = 'invalidated', ended_at = now(),
        end_reason = 'dispatch_admission_failed' WHERE task_id = $1
        AND resume_session_id IS NOT NULL AND status IN ('pending','reply_pending')",
    )
    .bind(task_id)
    .execute(db.conn())
    .await?;
    crate::services::agents::task_lifecycle::fail_task_dependents(
        db,
        org_id,
        workspace_id,
        task_id,
    )
    .await?;
    // A preview refused at admission settles its intent too, so a retry with
    // the same idempotency key starts a new preview instead of replaying
    // the failed one. A no-op for any other task.
    crate::services::agents::task_lifecycle::settle_preview_intent(
        db,
        workspace_id,
        task_id,
        "settled",
    )
    .await?;
    Ok(())
}

fn parse_command_spec(spec: Value) -> Result<CommandTaskSpecV1, AppError> {
    let value = spec
        .get("command")
        .cloned()
        .ok_or_else(|| conflict("command task spec is absent"))?;
    serde_json::from_value(value)
        .map_err(|error| conflict(format!("command task spec is invalid: {error}")))
}

fn validate_command_spec(spec: &CommandTaskSpecV1) -> Result<(), AppError> {
    let preview = spec.schema_version == "g5.preview-task-spec/v1"
        && spec.module_id == "workspace.preview"
        && spec.module_version == PREVIEW_MODULE_VERSION
        && spec.operation == "start"
        && spec.input_schema == "g5.workspace.preview.start/v1";
    if !preview || !spec.input.is_object() {
        return Err(conflict("unsupported command task spec"));
    }
    Ok(())
}

fn validate_agent_spec(spec: &AgentTaskSpecV1) -> Result<(), AppError> {
    if let Some(review) = &spec.formal_review {
        review.validate().map_err(conflict)?;
    }

    if spec.schema_version != AGENT_TASK_SPEC_SCHEMA {
        return Err(conflict("unsupported agent task spec schema"));
    }
    spec.agent_launch.validate().map_err(AppError::from_error)?;
    // `max_output_bytes` is no longer a limit to keep within a ceiling; it
    // must only stay positive for the guests that still require it.
    if spec.limits.max_output_bytes == 0
        || spec.limits.max_turns == Some(0)
        || spec.limits.native_response_timeout_ms == 0
        || spec.limits.native_response_timeout_ms > 900_000
    {
        return Err(conflict("agent task limits are invalid"));
    }
    if spec.allowed_tools.len() > 256
        || spec.allowed_tools.iter().any(|tool| {
            tool.trim().is_empty() || tool.len() > 256 || tool.chars().any(char::is_control)
        })
    {
        return Err(conflict("agent allowed-tools policy is invalid"));
    }
    if spec
        .allowed_tools
        .iter()
        .any(|tool| g5_contracts::types::names_removed_agent_tool(tool))
    {
        return Err(conflict(
            "agent allowed-tools names a tool G5 no longer provides",
        ));
    }
    if spec
        .workspace_target
        .relative_path
        .as_deref()
        .is_some_and(|path| {
            path.starts_with('/')
                || path.split('/').any(|segment| segment == "..")
                || path.contains('\0')
        })
    {
        return Err(conflict("agent workspace target path is invalid"));
    }
    Ok(())
}

fn ensure_launch_is_runnable(launch: &ResolvedAgentExecutionV1) -> Result<(), AppError> {
    launch.validate().map_err(AppError::from_error)?;
    // Subscription credentials are checked by the owner-bound VM, which can
    // perform native device authorization before delivering the prompt.
    if launch.auth_mode != g5_contracts::types::AgentAuthModeV1::ChatGptSubscription
        && (launch.requires_interactive_auth || !launch.credential_ready)
    {
        return Err(conflict(
            "agent provider credentials are not ready for unattended dispatch",
        ));
    }
    Ok(())
}

fn configured_provider_base_url(
    launch: &ResolvedAgentExecutionV1,
    settings: &Value,
) -> Option<String> {
    let key = match launch.provider.value {
        ChatAgentProvider::ClaudeCode => "ANTHROPIC_BASE_URL",
        ChatAgentProvider::Codex => "OPENAI_BASE_URL",
    };
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .map(str::to_owned)
}

fn provider_dispatch_input(
    launch: &ResolvedAgentExecutionV1,
    base_url: Option<&str>,
    policy_ref: &str,
    permission_policy: Value,
    session_title_wanted: bool,
) -> Value {
    let mut provider = json!({
        "provider_id": launch.provider.value,
        "model_id": launch.model.value,
        "policy_ref": policy_ref,
        "policy_snapshot": permission_policy,
        "options": {
            "effort": launch.effort.value,
            // Provider options are extensible even on older guest runtimes.
            "session_title": session_title_wanted,
        },
    });
    provider["auth_mode"] = json!(launch.auth_mode);
    if let Some(base) = base_url {
        provider["base_url"] = json!(base);
    }
    provider
}

/// The module an attempt was reserved with, and whether it froze App Server.
/// An absent or `1.0.5` snapshot is the historical contract: ordinary turns
/// use `1.0.5` and formal reviews the per-schema module, which for Codex were
/// exec (`g5.chat.codex-exec-retirement`); they are never upgraded here.
///
/// Schemas 5 and 6 always freeze their explicit module: `1.0.11`/`1.0.13`
/// (Claude) or `1.0.12`/`1.0.14` (Codex App Server). Codex `1.0.10` accepts
/// only schemas 2–4 and `1.0.12` only schemas 2–5.
fn frozen_module(snapshot: &Value, formal: Option<u8>) -> Result<(&'static str, bool), AppError> {
    let historical = || match formal {
        Some(4) => "1.0.9",
        Some(3) => "1.0.7",
        Some(_) => "1.0.6",
        None => AGENT_MODULE_VERSION,
    };
    // The explicit Claude module a schema 5 or 6 attempt must have frozen.
    let claude_explicit = match formal {
        Some(APPROVAL_SUMMARY_SCHEMA) => Some(CLAUDE_FORMAL_SUMMARY_MODULE_VERSION),
        Some(APPROVAL_REPORT_ORDER_SCHEMA) => Some(CLAUDE_FORMAL_REPORT_ORDER_MODULE_VERSION),
        _ => None,
    };
    let schema = formal.unwrap_or(0);
    let app_server = |module: &str| -> Option<&'static str> {
        match snapshot.get("codex_transport") {
            Some(Value::String(transport)) if transport == CODEX_APP_SERVER_TRANSPORT => [
                (CODEX_APP_SERVER_MODULE_VERSION, formal.is_none()),
                (
                    CODEX_FORMAL_APP_SERVER_MODULE_VERSION,
                    formal.is_some() && schema < APPROVAL_SUMMARY_SCHEMA,
                ),
                (
                    CODEX_FORMAL_SUMMARY_APP_SERVER_MODULE_VERSION,
                    formal.is_some() && schema <= APPROVAL_SUMMARY_SCHEMA,
                ),
                (
                    CODEX_FORMAL_REPORT_ORDER_APP_SERVER_MODULE_VERSION,
                    formal.is_some() && schema <= APPROVAL_REPORT_ORDER_SCHEMA,
                ),
            ]
            .into_iter()
            .find(|(version, accepts)| *accepts && *version == module)
            .map(|(version, _)| version),
            _ => None,
        }
    };
    let unsupported = || conflict("attempt has an unsupported frozen transport/module contract");
    match (
        snapshot.get("agent_module_version"),
        snapshot.get("codex_transport"),
    ) {
        (None, None) | (Some(Value::String(_)), None) if claude_explicit.is_some() => {
            match snapshot.get("agent_module_version") {
                Some(Value::String(module)) if Some(module.as_str()) == claude_explicit => {
                    Ok((claude_explicit.expect("checked above"), false))
                }
                _ => Err(unsupported()),
            }
        }
        (None, None) => Ok((historical(), false)),
        (Some(Value::String(module)), None) if module == AGENT_MODULE_VERSION => {
            Ok((historical(), false))
        }
        (Some(Value::String(module)), Some(_)) => app_server(module)
            .map(|version| (version, true))
            .ok_or_else(unsupported),
        _ => Err(unsupported()),
    }
}

/// The permission modes an `agent.turn` can run for `provider`; admission
/// refuses every other catalog mode (see [`permission_policy`]). Callers that
/// queue on an agent's behalf check this first, so a turn that could only fail
/// admission is refused before it is queued.
pub fn agent_turn_permission_modes(provider: ChatAgentProvider) -> &'static [&'static str] {
    match provider {
        ChatAgentProvider::ClaudeCode => &["default", "bypassPermissions"],
        ChatAgentProvider::Codex => &[
            "never/read-only",
            "never/workspace-write",
            "never/danger-full-access",
        ],
    }
}

fn permission_policy(
    launch: &ResolvedAgentExecutionV1,
    spec: &AgentTaskSpecV1,
) -> Result<Value, AppError> {
    if spec.formal_review.is_some() {
        return Ok(
            json!({"schema_version":"g5.agent.permission-policy/v1","provider":launch.provider.value,"questions":"disabled","tool_approval":"auto_deny","sandbox":if launch.provider.value==ChatAgentProvider::ClaudeCode {"none"}else{"read-only"},"allowed_tools":[],"limits":{"setup_timeout_ms":AGENT_SETUP_TIMEOUT_MS,"max_turns":null,"max_output_bytes":spec.limits.max_output_bytes,"native_response_timeout_ms":spec.limits.native_response_timeout_ms}}),
        );
    }
    let (questions, tool_approval, sandbox) =
        match (launch.provider.value, launch.permission_mode.value.as_str()) {
            (ChatAgentProvider::ClaudeCode, "default") => ("enabled", "prompt", "none"),
            (ChatAgentProvider::ClaudeCode, "bypassPermissions") => {
                ("enabled", "auto_allow", "none")
            }
            (ChatAgentProvider::Codex, "never/read-only") => ("disabled", "auto_deny", "read-only"),
            (ChatAgentProvider::Codex, "never/workspace-write") => {
                ("disabled", "auto_deny", "workspace-write")
            }
            (ChatAgentProvider::Codex, "never/danger-full-access") => {
                ("disabled", "auto_deny", "danger-full-access")
            }
            (ChatAgentProvider::ClaudeCode, mode) => {
                return Err(conflict(format!(
                    "Claude permission mode {mode} is not supported by agent.turn"
                )))
            }
            (ChatAgentProvider::Codex, mode) => {
                return Err(conflict(format!(
                    "interactive Codex permission mode {mode} is not supported in MVP"
                )))
            }
        };
    g5_contracts::types::validate_agent_tool_selection(
        launch.provider.value,
        questions == "enabled",
        &spec.allowed_tools,
    )
    .map_err(|reason| {
        conflict(format!(
            "{reason}: correct allowedTools for the resolved provider before retrying"
        ))
    })?;
    Ok(json!({
        "schema_version": "g5.agent.permission-policy/v1",
        "provider": launch.provider.value,
        "questions": questions,
        "tool_approval": tool_approval,
        "sandbox": sandbox,
        "allowed_tools": spec.allowed_tools,
        "limits": {
            "setup_timeout_ms": AGENT_SETUP_TIMEOUT_MS,
            "max_turns": spec.limits.max_turns,
            "max_output_bytes": spec.limits.max_output_bytes,
            "native_response_timeout_ms": spec.limits.native_response_timeout_ms,
        }
    }))
}

/// Whether one effective secret is carried by a dispatch: the retired Codex
/// key never is, and a formal-review turn carries only approval credentials.
fn carried_by_dispatch(key: &str, formal_review: bool) -> bool {
    key != g5_contracts::types::CODEX_AUTH_JSON_ENV_KEY
        && (!formal_review || g5_contracts::types::FORMAL_APPROVAL_CREDENTIAL_KEYS.contains(&key))
}

/// The entries a dispatch carries from the effective secrets.
pub(crate) fn dispatch_secret_set(
    mut secrets: HashMap<String, String>,
    formal_review: bool,
) -> HashMap<String, String> {
    secrets.retain(|key, _| carried_by_dispatch(key, formal_review));
    secrets
}

/// Admit the effective secrets a dispatch would carry under the shared policy
/// (`g5.chat.secret-admission`).
pub(crate) fn validate_dispatch_secrets(
    secrets: &HashMap<String, String>,
    formal_review: bool,
) -> Result<(), TaskEnvSecretError> {
    g5_contracts::types::task_env::validate_task_env_secrets(
        secrets
            .iter()
            .filter(|(key, _)| carried_by_dispatch(key, formal_review))
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
}

/// The set a reserved attempt still carries once its configuration no longer
/// passes admission, and the sentence explaining what was withheld: every
/// entry that passes on its own, or nothing when the set as a whole is over
/// budget. `None` when the set is valid.
fn reconciled_dispatch_secrets(
    secrets: HashMap<String, String>,
    formal_review: bool,
) -> (HashMap<String, String>, Option<String>) {
    match validate_dispatch_secrets(&secrets, formal_review) {
        Ok(()) => (dispatch_secret_set(secrets, formal_review), None),
        Err(error @ (TaskEnvSecretError::TooMany { .. } | TaskEnvSecretError::TooLarge { .. })) => {
            (HashMap::new(), Some(error.to_string()))
        }
        Err(error) => {
            let mut carried = dispatch_secret_set(secrets, formal_review);
            carried.retain(|key, value| {
                g5_contracts::types::task_env::validate_task_env_secret(key, value).is_ok()
            });
            (carried, Some(error.to_string()))
        }
    }
}

fn changed_confirmed_launch_choices(
    confirmed: &ResolvedAgentExecutionV1,
    current: &ResolvedAgentExecutionV1,
) -> Vec<String> {
    let mut changes = [
        (
            "provider",
            confirmed.provider.value.as_str(),
            current.provider.value.as_str(),
        ),
        (
            "model",
            confirmed.model.value.as_str(),
            current.model.value.as_str(),
        ),
        (
            "effort",
            confirmed.effort.value.as_str(),
            current.effort.value.as_str(),
        ),
        (
            "permission mode",
            confirmed.permission_mode.value.as_str(),
            current.permission_mode.value.as_str(),
        ),
    ]
    .into_iter()
    .filter(|(_, before, after)| before != after)
    .map(|(name, before, after)| format!("{name} changed from {before:?} to {after:?}"))
    .collect::<Vec<_>>();
    if confirmed.account() != current.account() {
        let describe = |launch: &ResolvedAgentExecutionV1| {
            launch.account().map_or_else(
                || "Runtime Config".to_string(),
                |account| format!("{} / {} / {}", account.kind, account.scope, account.id),
            )
        };
        changes.push(format!(
            "account changed from {} to {}",
            describe(confirmed),
            describe(current)
        ));
    }
    changes
}

/// The owner-facing sentence for an admission refusal.
fn admission_refusal_sentence(error: &AppError) -> String {
    match &error.inner {
        G5Error::Conflict { reason } => reason.clone(),
        other => other.to_string(),
    }
}

/// Keep a refused turn visible in its chat: an assistant error message with
/// the reason, so the owner learns what to correct. The queued input stays
/// attached to the failed task and runs with the owner's next message.
async fn record_admission_refusal_notice(
    db: &mut ScopedDb,
    org_id: Uuid,
    session_id: Uuid,
    task_id: Uuid,
    error: &AppError,
    confirmed_choices_changed: bool,
) -> Result<(), AppError> {
    let recovery = if confirmed_choices_changed {
        "Your next message will run it with the current values and its saved preferences, or you can withdraw it."
    } else {
        "Once corrected, your next message runs it."
    };
    let notice = format!(
        "This message was not sent to the agent: {}. {recovery}",
        admission_refusal_sentence(error).trim_end_matches('.')
    );
    // No attempt exists for a task refused at admission, and chat messages
    // bind to attempts in pairs, so the notice carries neither. Its id is
    // the canonical one for the task's missing first attempt, so a repeated
    // settlement cannot duplicate it.
    crate::services::agents::chat_service::record_refusal_notice(
        db.conn(),
        org_id,
        session_id,
        crate::services::agents::task_lifecycle::canonical_agent_message_id(task_id, 0),
        &notice,
    )
    .await?;
    Ok(())
}

fn parse_uuid_text(value: String, name: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(&value).map_err(|_| conflict(format!("{name} is not a UUID")))
}

fn parse_nonnegative(value: i64, name: &str) -> Result<u64, AppError> {
    u64::try_from(value).map_err(|_| conflict(format!("{name} is negative")))
}

fn default_max_output_bytes() -> u64 {
    DEFAULT_MAX_OUTPUT_BYTES
}

fn default_native_response_timeout_ms() -> u64 {
    900_000
}

/// Record the attachment manifest of a queued inbox input. False when the
/// input is no longer queued.
pub(crate) async fn set_queued_input_artifacts(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    session_id: Uuid,
    input_id: Uuid,
    artifacts: Value,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE session_inputs SET artifacts = $4
         WHERE id = $1 AND org_id = $2 AND session_id = $3
           AND delivery = 'queued'",
    )
    .bind(input_id)
    .bind(org_id)
    .bind(session_id)
    .bind(artifacts)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn runtime_error(
    error: crate::services::workspace::workspace_runtime::RuntimeRpcError,
) -> AppError {
    AppError::from_error(match error.message.as_str() {
        "WORKSPACE_STOPPING" => G5Error::WorkspaceStopping,
        _ if error.code == 403 => G5Error::Forbidden {
            missing_scopes: Vec::new(),
        },
        _ if error.code == 409 => G5Error::Conflict {
            reason: error.message,
        },
        _ => G5Error::ServiceUnavailable {
            message: format!("workspace runtime call failed: {}", error.message),
        },
    })
}

fn conflict(reason: impl Into<String>) -> AppError {
    AppError::from_error(G5Error::Conflict {
        reason: reason.into(),
    })
}

/// Ends the savepoint a dispatch candidate was tried under, keeping its work.
async fn release_dispatch_candidate(db: &mut ScopedDb) -> Result<(), AppError> {
    sqlx::query("RELEASE SAVEPOINT agent_dispatch_candidate")
        .execute(db.conn())
        .await?;
    Ok(())
}

/// Prefix `content` with the session's frozen persona instructions when the
/// session has a persona and no delivered input or transcript row carries the
/// prefix yet (workbench g5.workbench.persona bootstrap). The check reads the
/// session's own rows only: the first-start input already carries the prefix,
/// a fork's copied transcript carries the source's, so neither is prefixed
/// twice.
async fn bootstrap_persona_prefix(
    db: &mut ScopedDb,
    session_id: Uuid,
    content: String,
) -> Result<String, AppError> {
    match persona_prefix_to_apply(db, session_id, &content).await? {
        Some(instructions) => Ok(format!("{instructions}\n\n{content}")),
        None => Ok(content),
    }
}

/// Refuse `content` when the frozen persona prefix that delivery would add
/// pushes the input over the prompt byte limit (the existing too-large error).
async fn ensure_persona_prefixed_size(
    db: &mut ScopedDb,
    session_id: Uuid,
    content: &str,
) -> Result<(), AppError> {
    if let Some(instructions) = persona_prefix_to_apply(db, session_id, content).await? {
        if instructions.len() + 2 + content.len() > MAX_PROMPT_BYTES {
            return Err(conflict("agent input exceeds the prompt byte limit"));
        }
    }
    Ok(())
}

/// The frozen persona instructions delivery would prepend to `content`, or
/// `None` when the session has no persona, the content already carries them,
/// or a delivered input / transcript row already does.
async fn persona_prefix_to_apply(
    db: &mut ScopedDb,
    session_id: Uuid,
    content: &str,
) -> Result<Option<String>, AppError> {
    let instructions: Option<String> =
        sqlx::query_scalar("SELECT persona_instructions FROM chat_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_optional(db.conn())
            .await?
            .flatten();
    let Some(instructions) = instructions.filter(|text| !text.trim().is_empty()) else {
        return Ok(None);
    };
    if content.starts_with(&instructions) {
        return Ok(None);
    }
    let delivered: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM chat_messages
             WHERE session_id = $1 AND role = 'user'
               AND left(content, length($2)) = $2)
         OR EXISTS (
             SELECT 1 FROM session_inputs
             WHERE session_id = $1
               AND delivery IN ('reserved', 'delivered', 'delivery_unknown')
               AND left(content, length($2)) = $2)",
    )
    .bind(session_id)
    .bind(&instructions)
    .fetch_one(db.conn())
    .await?;
    if delivered {
        return Ok(None);
    }
    Ok(Some(instructions))
}

async fn reserve_system_notices(
    db: &mut ScopedDb,
    session_id: Uuid,
    task_id: Uuid,
    attempt: i32,
    prompt_budget: usize,
) -> Result<Vec<Uuid>, AppError> {
    let rows = crate::services::agents::chat_service::lock_queued_notices(
        db.conn(),
        session_id,
        MAX_SYSTEM_NOTICES_PER_ATTEMPT,
    )
    .await?;
    let max_preamble = prompt_budget.min(MAX_SYSTEM_NOTICE_PREAMBLE_BYTES);
    let mut selected = Vec::new();
    for row in rows {
        let mut candidate = selected.clone();
        candidate.push(row);
        if render_notice_preamble(&candidate).len() > max_preamble {
            break;
        }
        selected = candidate;
    }
    let ids = selected.iter().map(|notice| notice.0).collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(ids);
    }
    let updated = crate::services::agents::chat_service::reserve_notices(
        db.conn(),
        &ids,
        session_id,
        task_id,
        attempt,
    )
    .await?;
    if updated != ids.len() as u64 {
        return Err(conflict("system notice reservation lost its state race"));
    }
    Ok(ids)
}

/// Whether this turn should produce a `session.title` summary: only while the
/// session's title is still platform-owned (`title_source = 'derived'`, or a
/// legacy row with no title). Once the user renames or clears it, or a
/// summary already landed, later turns spend nothing on titles.
async fn session_title_wanted(
    db: &mut ScopedDb,
    org_id: Uuid,
    session_id: Uuid,
) -> Result<bool, AppError> {
    Ok(
        crate::services::agents::chat_service::session_title_wanted(db.conn(), org_id, session_id)
            .await?,
    )
}

async fn render_system_notice_preamble(
    db: &mut ScopedDb,
    session_id: Uuid,
    notice_ids: &[Uuid],
    input: &str,
) -> Result<String, AppError> {
    if notice_ids.is_empty() {
        return Ok(input.to_string());
    }
    let notices = crate::services::agents::chat_service::reserved_notice_contents(
        db.conn(),
        session_id,
        notice_ids,
    )
    .await?;
    if notices.len() != notice_ids.len() {
        return Err(conflict("reserved system notice is unavailable"));
    }
    let preamble = render_notice_preamble(&notices);
    if preamble.len() > MAX_SYSTEM_NOTICE_PREAMBLE_BYTES
        || preamble.len().saturating_add(input.len()) > MAX_PROMPT_BYTES
    {
        return Err(conflict("system notice preamble exceeds the prompt limit"));
    }
    Ok(format!("{preamble}\n\n{input}"))
}

fn render_notice_preamble(notices: &[(Uuid, String)]) -> String {
    let mut rendered = String::from("Platform notices for this turn:\n");
    for (_, content) in notices {
        rendered.push_str("- ");
        rendered.push_str(content);
        rendered.push('\n');
    }
    rendered
}

async fn freeze_recovery_prompt(
    db: &mut ScopedDb,
    row: &DispatchRow,
    current_input: String,
) -> Result<FrozenRecoveryPrompt, AppError> {
    // Complete user rows and provider-origin assistant output follow positive
    // acceptance. Platform auth failures can create assistant error notices
    // before delivery, so error rows are not provider history.
    // Forked sessions copy that delivered transcript, without an inbox or
    // provider session identity. The bounded query also retains the original
    // task input for Continue even if many later messages displaced it.
    let rows = sqlx::query(
        "WITH selected AS (
             SELECT * FROM (
                 SELECT id, message_index, role, status
                 FROM chat_messages
                 WHERE session_id = $1
                   AND ((role = 'user' AND status = 'complete')
                        OR (role = 'assistant' AND content <> ''
                            AND status IN ('complete','stopped')))
                 ORDER BY message_index DESC LIMIT $2
             ) latest
             UNION
             SELECT * FROM (
                 SELECT id, message_index, role, status
                 FROM chat_messages
                 WHERE session_id = $1 AND task_id = $3
                   AND role = 'user' AND status = 'complete'
                 ORDER BY message_index ASC LIMIT 1
             ) original
         )
         SELECT selected.id, selected.message_index, selected.role,
                selected.status,
                CASE WHEN selected.id = (SELECT id FROM chat_messages
                               WHERE session_id = $1 AND task_id = $3
                                 AND role = 'user' AND status = 'complete'
                               ORDER BY message_index ASC LIMIT 1)
                     THEN message.content
                     ELSE left(message.content, 16385) END AS content,
                selected.id = (SELECT id FROM chat_messages
                               WHERE session_id = $1 AND task_id = $3
                                 AND role = 'user' AND status = 'complete'
                               ORDER BY message_index ASC LIMIT 1) AS anchor,
                (SELECT count(*) FROM chat_messages
                 WHERE session_id = $1
                   AND ((role = 'user' AND status = 'complete')
                        OR (role = 'assistant' AND content <> ''
                            AND status IN ('complete','stopped')))) AS total_count,
                COALESCE((SELECT array_agg(
                    reference.label ORDER BY reference.file_name, reference.id)
                    FROM (SELECT attachment.id, attachment.file_name,
                                 attachment.file_name || ' (sha256:' || attachment.sha256 ||
                                 ', scan:' || attachment.scan_status || ')' AS label
                          FROM chat_attachments AS attachment
                          WHERE attachment.session_id = $1
                            AND attachment.message_id = selected.id
                          ORDER BY attachment.file_name, attachment.id
                          LIMIT 16) AS reference),
                    ARRAY[]::text[]) AS attachments,
                GREATEST(0, (SELECT count(*) FROM chat_attachments AS attachment
                             WHERE attachment.session_id = $1
                               AND attachment.message_id = selected.id) - 16)
                    AS omitted_attachments
         FROM selected
         JOIN chat_messages AS message ON message.id = selected.id
         ORDER BY selected.message_index",
    )
    .bind(row.session_id)
    .bind(MAX_RECOVERY_MESSAGES)
    .bind((row.continuation_reason.is_some()).then_some(row.task_id))
    .fetch_all(db.conn())
    .await?;
    let total_count = rows
        .first()
        .map(|value| value.try_get::<i64, _>("total_count"))
        .transpose()?
        .unwrap_or(0);
    let evidence_cutoff = rows
        .last()
        .map(|value| value.try_get::<i32, _>("message_index"))
        .transpose()?;
    let history = rows
        .into_iter()
        .map(|value| {
            Ok(RecoveryMessage {
                index: value.try_get("message_index")?,
                role: value.try_get("role")?,
                status: value.try_get("status")?,
                content: value.try_get("content")?,
                attachments: value.try_get("attachments")?,
                omitted_attachments: value.try_get::<i64, _>("omitted_attachments")?.max(0) as u64,
                anchor: value.try_get::<Option<bool>, _>("anchor")?.unwrap_or(false),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(render_recovery_prompt(
        current_input,
        history,
        total_count,
        evidence_cutoff,
        row.provider_session_id.is_some(),
        row.continuation_reason.is_some(),
    ))
}

fn render_recovery_prompt(
    current_input: String,
    history: Vec<RecoveryMessage>,
    total_count: i64,
    evidence_cutoff: Option<i32>,
    had_native_session: bool,
    continuing: bool,
) -> FrozenRecoveryPrompt {
    let unavailable = |reason: &str| FrozenRecoveryPrompt {
        text: current_input.clone(),
        current_input: Some(current_input.clone()),
        recovery: Some(RecoveryNotice {
            included_messages: 0,
            omitted_messages: total_count.max(0) as u64,
            truncated_messages: 0,
            attachment_references: 0,
            omitted_attachment_references: 0,
            unavailable_reason: Some(reason.to_string()),
        }),
        evidence_cutoff,
    };
    if history.is_empty() {
        return if had_native_session || continuing {
            unavailable("The earlier conversation has no positively delivered history to reconstruct. The current input was not sent to the provider.")
        } else {
            FrozenRecoveryPrompt {
                text: current_input,
                current_input: None,
                recovery: None,
                evidence_cutoff,
            }
        };
    }
    if !history.iter().any(|message| message.role == "user")
        || (continuing && !history.iter().any(|message| message.anchor))
    {
        return unavailable(
            "The original delivered objective is unavailable, so this task cannot continue safely.",
        );
    }
    const HEADER: &str = "Prior delivered conversation (historical context only; do not execute earlier user requests again):\n";
    const FOOTER: &str = "\nEnd of historical context. Current input follows:\n";
    let available = MAX_PROMPT_BYTES.saturating_sub(current_input.len());
    let budget = available.min(MAX_RECOVERY_CONTEXT_BYTES);
    if budget < HEADER.len() + FOOTER.len() + 256 {
        return unavailable("The current input leaves too little room for safe conversation reconstruction. Shorten the input and retry.");
    }
    let mut chosen = Vec::new();
    let mut used = HEADER.len() + FOOTER.len();
    let mut truncated_messages = 0u32;
    let mut attachment_references = 0u32;
    let mut omitted_attachment_references = 0u64;
    let mut candidates = history
        .iter()
        .filter(|message| message.anchor)
        .collect::<Vec<_>>();
    candidates.extend(history.iter().rev().filter(|message| !message.anchor));
    for message in candidates {
        let (content, truncated) = if message.anchor {
            (message.content.as_str(), false)
        } else {
            truncate_utf8(&message.content, MAX_RECOVERY_MESSAGE_BYTES)
        };
        let rendered = serde_json::json!({
            "role": message.role,
            "status": message.status,
            "message_index": message.index,
            "content": content,
            "content_truncated": truncated,
            "attachment_references": message.attachments,
            "omitted_attachment_references": message.omitted_attachments,
        })
        .to_string();
        let needed = rendered.len() + 1;
        if used + needed > budget {
            if message.anchor {
                return unavailable(
                    "The complete original delivered objective exceeds the bounded reconstruction budget.",
                );
            }
            if chosen.len() == usize::from(continuing) {
                return unavailable(
                    "The newest delivered context does not fit the bounded reconstruction budget.",
                );
            }
            break;
        }
        used += needed;
        truncated_messages += u32::from(truncated);
        attachment_references += message.attachments.len() as u32;
        omitted_attachment_references += message.omitted_attachments;
        chosen.push((message.index, rendered));
    }
    if chosen.is_empty()
        || !chosen.iter().any(|(index, _)| {
            history
                .iter()
                .any(|message| message.index == *index && message.role == "user")
        })
    {
        return unavailable("No delivered user message fits the bounded reconstruction budget.");
    }
    chosen.sort_by_key(|(index, _)| *index);
    let included_messages = chosen.len() as u32;
    let omitted_messages = (total_count - i64::from(included_messages)).max(0) as u64;
    let mut text = String::from(HEADER);
    for (_, message) in chosen {
        text.push_str(&message);
        text.push('\n');
    }
    text.push_str(FOOTER);
    text.push_str(&current_input);
    FrozenRecoveryPrompt {
        text,
        current_input: Some(current_input),
        recovery: Some(RecoveryNotice {
            included_messages,
            omitted_messages,
            truncated_messages,
            attachment_references,
            omitted_attachment_references,
            unavailable_reason: None,
        }),
        evidence_cutoff,
    }
}

fn truncate_utf8(value: &str, limit: usize) -> (&str, bool) {
    if value.len() <= limit {
        return (value, false);
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (&value[..end], true)
}

/// The instruction a continuation attempt opens with instead of a new input
/// (`g5.chat.recovery`). After a lost VM the workspace came back from its last
/// save, so the agent is told to re-check work from after that save rather
/// than assume it persisted (`g5.lifecycle.start.automatic-continuation`);
/// `vm_lost` carries that evidence when the continuation reason does not.
pub(crate) fn continuation_instruction(reason: &str, vm_lost: bool) -> String {
    let restored = if vm_lost || reason == "vm_lost" {
        " The workspace was restored from its last save; changes made after it may be lost, so re-check them before relying on them."
    } else {
        ""
    };
    format!(
        "Continue the same task after {reason}.{restored} Resume from durable conversation history and do not replay previously delivered input. Background tasks and sub-agents from the earlier attempt are no longer running."
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn continuation_instruction_warns_about_lost_unsaved_work_only_after_vm_loss() {
        let lost = super::continuation_instruction("vm_lost", false);
        assert!(lost.starts_with(
            "Continue the same task after vm_lost. The workspace was restored from its last save;"
        ));
        assert!(lost.ends_with(
            "Background tasks and sub-agents from the earlier attempt are no longer running."
        ));
        let interrupted = super::continuation_instruction("interrupted", false);
        assert!(!interrupted.contains("restored from its last save"));
        assert!(interrupted.contains("do not replay previously delivered input."));
        let drained_then_lost = super::continuation_instruction("interrupted", true);
        assert!(drained_then_lost.contains("restored from its last save"));
    }

    use super::*;

    #[test]
    fn app_server_rollout_matches_the_bundled_guest_catalog() {
        // Unknown catalogs stay on the established contract. A future bundled
        // catalog bump must append to the shipped list too.
        let catalog: Value = serde_json::from_str(include_str!(
            "../../../../g5d-dispatch/modules/catalog.json"
        ))
        .unwrap();
        assert_eq!(
            catalog["catalog_version"].as_str(),
            SHIPPED_MODULE_CATALOGS.last().copied()
        );
        assert!(catalog_supports(
            catalog["catalog_version"].as_str(),
            CODEX_APP_SERVER_CATALOG
        ));
        assert!(catalog_supports(
            catalog["catalog_version"].as_str(),
            PREVIEW_ORIGIN_CATALOG
        ));
        assert!(catalog_supports(
            catalog["catalog_version"].as_str(),
            CODEX_FORMAL_APP_SERVER_CATALOG
        ));
        assert!(catalog_supports(
            catalog["catalog_version"].as_str(),
            APPROVAL_SUMMARY_CATALOG
        ));
        assert!(catalog_supports(
            catalog["catalog_version"].as_str(),
            APPROVAL_REPORT_ORDER_CATALOG
        ));
        for version in [
            CODEX_APP_SERVER_MODULE_VERSION,
            CODEX_FORMAL_APP_SERVER_MODULE_VERSION,
            CLAUDE_FORMAL_SUMMARY_MODULE_VERSION,
            CODEX_FORMAL_SUMMARY_APP_SERVER_MODULE_VERSION,
            CLAUDE_FORMAL_REPORT_ORDER_MODULE_VERSION,
            CODEX_FORMAL_REPORT_ORDER_APP_SERVER_MODULE_VERSION,
        ] {
            assert!(catalog["modules"].as_array().unwrap().iter().any(|module| {
                module["module_id"] == "agent.turn" && module["module_version"] == version
            }));
        }
        // Catalogs before formal App Server keep ordinary App Server only.
        for catalog in [
            "2026.10.27-merge-base",
            "2026.10.27-merge-base-preview-origin",
        ] {
            assert!(catalog_supports(Some(catalog), CODEX_APP_SERVER_CATALOG));
            assert!(!catalog_supports(
                Some(catalog),
                CODEX_FORMAL_APP_SERVER_CATALOG
            ));
        }
        // A guest with formal App Server but no schema 5 modules never
        // receives a schema 5 review.
        assert!(!catalog_supports(
            Some(CODEX_FORMAL_APP_SERVER_CATALOG),
            APPROVAL_SUMMARY_CATALOG
        ));
        assert!(!catalog_supports(None, APPROVAL_SUMMARY_CATALOG));
        // Nor does a guest with schema 5 modules receive a schema 6 review.
        for catalog in [
            APPROVAL_SUMMARY_CATALOG,
            "2026.10.30-agent-turn-question-gate",
        ] {
            assert!(!catalog_supports(
                Some(catalog),
                APPROVAL_REPORT_ORDER_CATALOG
            ));
        }
        assert!(!catalog_supports(None, APPROVAL_REPORT_ORDER_CATALOG));
    }

    #[test]
    fn guest_contracts_follow_the_catalog_that_introduced_them() {
        let preview_origin = |catalog| catalog_supports(catalog, PREVIEW_ORIGIN_CATALOG);
        assert!(preview_origin(Some(PREVIEW_ORIGIN_CATALOG)));
        assert!(!preview_origin(Some(CODEX_APP_SERVER_CATALOG)));
        assert!(!preview_origin(Some("2026.10.27-merge-base")));
        assert!(preview_origin(Some("2026.10.27-merge-base-preview-origin")));
        assert!(catalog_supports(
            Some("2026.10.27-merge-base"),
            CODEX_APP_SERVER_CATALOG
        ));
        assert!(!preview_origin(Some("2026.09.19-p4.2b")));
        assert!(!preview_origin(Some("2099.01.01-unknown")));
        assert!(!preview_origin(None));
        assert!(catalog_supports(
            Some(PREVIEW_ORIGIN_CATALOG),
            CODEX_APP_SERVER_CATALOG
        ));
        assert!(!catalog_supports(
            Some("2026.10.22-workspace-base-reviews"),
            CODEX_APP_SERVER_CATALOG
        ));
    }

    #[test]
    fn historical_reservations_never_infer_a_new_transport() {
        // Ordinary turns: absent fields are the historical 1.0.5 contract.
        for snapshot in [json!({}), json!({"agent_module_version":"1.0.5"})] {
            assert_eq!(frozen_module(&snapshot, None).unwrap(), ("1.0.5", false));
            // Formal reviews keep their per-schema historical module.
            for (schema, module) in [(2, "1.0.6"), (3, "1.0.7"), (4, "1.0.9")] {
                assert_eq!(
                    frozen_module(&snapshot, Some(schema)).unwrap(),
                    (module, false)
                );
            }
        }
        let app_server = |module: &str| json!({"agent_module_version":module,"codex_transport":"app_server_stdio_v1"});
        assert_eq!(
            frozen_module(&app_server("1.0.8"), None).unwrap(),
            ("1.0.8", true)
        );
        for schema in [2, 3, 4] {
            assert_eq!(
                frozen_module(&app_server("1.0.10"), Some(schema)).unwrap(),
                ("1.0.10", true)
            );
            // An ordinary App Server contract never becomes a formal one.
            assert!(frozen_module(&app_server("1.0.8"), Some(schema)).is_err());
        }
        assert!(frozen_module(&app_server("1.0.10"), None).is_err());
        // Schema 5 freezes its own modules; 1.0.10 never receives it, and
        // no absent or historical snapshot is upgraded into it.
        assert_eq!(
            frozen_module(&json!({"agent_module_version":"1.0.11"}), Some(5)).unwrap(),
            ("1.0.11", false)
        );
        assert_eq!(
            frozen_module(&app_server("1.0.12"), Some(5)).unwrap(),
            ("1.0.12", true)
        );
        for schema in [2, 3, 4] {
            assert_eq!(
                frozen_module(&app_server("1.0.12"), Some(schema)).unwrap(),
                ("1.0.12", true),
                "the schema 5 Codex module accepts every retained schema"
            );
            assert!(
                frozen_module(&json!({"agent_module_version":"1.0.11"}), Some(schema)).is_err()
            );
        }
        for snapshot in [
            json!({}),
            json!({"agent_module_version":"1.0.5"}),
            json!({"agent_module_version":"1.0.9"}),
            json!({"agent_module_version":"1.0.12"}),
            app_server("1.0.10"),
            app_server("1.0.8"),
            app_server("1.0.11"),
        ] {
            assert!(frozen_module(&snapshot, Some(5)).is_err(), "{snapshot}");
        }
        assert!(frozen_module(&app_server("1.0.12"), None).is_err());
        assert!(frozen_module(&json!({"agent_module_version":"1.0.11"}), None).is_err());
        // Schema 6 freezes its own modules in the same way; 1.0.12 and the
        // schema 5 Claude module never receive it.
        assert_eq!(
            frozen_module(&json!({"agent_module_version":"1.0.13"}), Some(6)).unwrap(),
            ("1.0.13", false)
        );
        for schema in [2, 3, 4, 5, 6] {
            assert_eq!(
                frozen_module(&app_server("1.0.14"), Some(schema)).unwrap(),
                ("1.0.14", true),
                "the schema 6 Codex module accepts every retained schema"
            );
        }
        for schema in [2, 3, 4, 5] {
            assert!(
                frozen_module(&json!({"agent_module_version":"1.0.13"}), Some(schema)).is_err()
            );
        }
        for snapshot in [
            json!({}),
            json!({"agent_module_version":"1.0.5"}),
            json!({"agent_module_version":"1.0.11"}),
            json!({"agent_module_version":"1.0.14"}),
            app_server("1.0.10"),
            app_server("1.0.12"),
            app_server("1.0.13"),
        ] {
            assert!(frozen_module(&snapshot, Some(6)).is_err(), "{snapshot}");
        }
        assert!(frozen_module(&app_server("1.0.14"), None).is_err());
        assert!(frozen_module(&json!({"agent_module_version":"1.0.13"}), None).is_err());
        for snapshot in [
            json!({"agent_module_version":"1.0.8"}),
            json!({"agent_module_version":"1.0.10"}),
            json!({"codex_transport":"app_server_stdio_v1"}),
            json!({"agent_module_version":"1.0.5","codex_transport":"app_server_stdio_v1"}),
            json!({"agent_module_version":"1.0.8","codex_transport":"experimental"}),
            json!({"agent_module_version":"1.0.7","codex_transport":"app_server_stdio_v1"}),
        ] {
            assert!(frozen_module(&snapshot, None).is_err(), "{snapshot}");
            assert!(frozen_module(&snapshot, Some(4)).is_err(), "{snapshot}");
        }
    }

    #[test]
    fn malformed_frozen_contract_fields_never_fall_back_to_legacy_exec() {
        for malformed in [Value::Null, json!(123), json!(true), json!([]), json!({})] {
            for snapshot in [
                json!({"agent_module_version": malformed}),
                json!({"codex_transport": malformed}),
                json!({"agent_module_version": "1.0.5", "codex_transport": malformed}),
                json!({"agent_module_version": "1.0.8", "codex_transport": malformed}),
                json!({"agent_module_version": "1.0.10", "codex_transport": malformed}),
                json!({"agent_module_version": malformed, "codex_transport": "app_server_stdio_v1"}),
            ] {
                for formal in [None, Some(4)] {
                    assert!(
                        frozen_module(&snapshot, formal).is_err(),
                        "malformed frozen contract was accepted: {snapshot}"
                    );
                }
            }
        }
    }

    use g5_contracts::types::agent_launch::{
        AgentAuthModeV1, AgentEgressModeV1, AgentLaunchProvenanceV1, AgentProviderRouteV1,
        ResolvedAgentLaunchFieldV1,
    };

    #[test]
    fn frozen_attachment_manifest_classifies_and_rejects_invalid_identity() {
        let input_id = Uuid::new_v4();
        let item = |id: String, mime: Option<&str>| {
            json!({
                "id": id,
                "fileName": "image.png",
                "contentType": mime,
                "sizeBytes": 3,
                "sha256": "a".repeat(64),
                "scanStatus": "pending",
                "storageKey": "stored",
                "providerMountPath": "/mnt/stored",
                "downloadPath": "/api/internal/stored",
            })
        };
        let id = Uuid::new_v4().to_string();
        let image =
            frozen_attachment_descriptors(input_id, &json!([item(id.clone(), Some("IMAGE/PNG"))]))
                .unwrap();
        assert_eq!(image[0]["kind"], "image");
        assert_eq!(image[0]["input_id"], input_id.to_string());
        assert_eq!(
            frozen_attachment_descriptors(
                input_id,
                &json!([item(Uuid::new_v4().to_string(), Some("application/pdf"))])
            )
            .unwrap()[0]["kind"],
            "file"
        );
        assert_eq!(
            frozen_attachment_descriptors(
                input_id,
                &json!([item(Uuid::new_v4().to_string(), None)])
            )
            .unwrap()[0]["kind"],
            "reference"
        );
        for invalid in [
            json!([
                item(id.clone(), Some("image/png")),
                item(id.clone(), Some("image/png"))
            ]),
            json!([item("invalid".into(), Some("image/png"))]),
            json!(vec![item(id.clone(), Some("image/png")); 9]),
            json!({"broken": true}),
        ] {
            assert!(frozen_attachment_descriptors(input_id, &invalid).is_err());
        }
        let mut invalid_digest = item(id, Some("image/png"));
        invalid_digest["sha256"] = json!("bad");
        assert!(frozen_attachment_descriptors(input_id, &json!([invalid_digest])).is_err());
    }

    fn history(index: i32, role: &str, content: &str) -> RecoveryMessage {
        RecoveryMessage {
            index,
            role: role.into(),
            status: "complete".into(),
            content: content.into(),
            attachments: vec![],
            omitted_attachments: 0,
            anchor: false,
        }
    }

    #[test]
    fn recovery_prompt_delivers_prior_fact_as_history_and_preserves_current_action() {
        let mut fact = history(0, "user", "The project code is RAVEN-42.");
        fact.attachments = vec!["brief.pdf (sha256:abc)".into()];
        let frozen = render_recovery_prompt(
            "What is the project code?".into(),
            vec![fact, history(1, "assistant", "I will remember it.")],
            2,
            Some(1),
            true,
            false,
        );
        assert!(frozen.text.contains("RAVEN-42"));
        assert!(frozen.text.contains("brief.pdf (sha256:abc)"));
        assert!(frozen
            .text
            .ends_with("Current input follows:\nWhat is the project code?"));
        let notice = frozen.recovery.expect("reconstruction notice");
        assert_eq!(notice.included_messages, 2);
        assert_eq!(notice.attachment_references, 1);
        assert!(notice.unavailable_reason.is_none());
        assert_eq!(frozen.evidence_cutoff, Some(1));
    }

    #[test]
    fn continuation_retains_original_objective_without_redelivering_it() {
        let mut messages = (44..300)
            .map(|index| history(index, "assistant", "Later progress."))
            .collect::<Vec<_>>();
        let mut original = history(0, "user", "Create one issue and then summarize it.");
        original.anchor = true;
        messages.insert(0, original);
        let frozen = render_recovery_prompt(
            "Continue the same task; do not repeat completed actions.".into(),
            messages,
            300,
            Some(299),
            true,
            true,
        );
        assert!(frozen.text.contains("Create one issue"));
        assert!(frozen
            .text
            .ends_with("Continue the same task; do not repeat completed actions."));
        assert_eq!(frozen.recovery.unwrap().omitted_messages, 43);
    }

    #[test]
    fn continuation_never_runs_with_a_partial_original_objective() {
        let mut original = history(
            0,
            "user",
            &format!("{}DO NOT CREATE A SECOND ISSUE", "x".repeat(20_000)),
        );
        original.anchor = true;
        let complete = render_recovery_prompt(
            "Continue".into(),
            vec![original.clone()],
            1,
            Some(0),
            true,
            true,
        );
        assert!(complete.text.contains("DO NOT CREATE A SECOND ISSUE"));
        assert_eq!(complete.recovery.unwrap().truncated_messages, 0);
        original.content = format!(
            "{}DO NOT CREATE A SECOND ISSUE",
            "x".repeat(MAX_RECOVERY_CONTEXT_BYTES)
        );
        let unavailable =
            render_recovery_prompt("Continue".into(), vec![original], 1, Some(0), true, true);
        assert!(unavailable.recovery.unwrap().unavailable_reason.is_some());
    }

    #[test]
    fn bounded_history_cannot_backfill_a_stale_fact_past_a_newer_correction() {
        let current = "x".repeat(MAX_PROMPT_BYTES - 2500);
        let frozen = render_recovery_prompt(
            current,
            vec![
                history(0, "user", "The code is RAVEN."),
                history(
                    1,
                    "user",
                    &format!("The code is now OMEGA.{}", "x".repeat(3000)),
                ),
            ],
            2,
            Some(1),
            true,
            false,
        );
        assert!(frozen.recovery.unwrap().unavailable_reason.is_some());
        assert!(!frozen.text.contains("RAVEN"));
    }

    #[test]
    fn recovery_refuses_missing_objective_and_insufficient_budget() {
        let missing = render_recovery_prompt("Continue".into(), vec![], 0, None, true, true);
        assert!(missing.recovery.unwrap().unavailable_reason.is_some());
        let oversized = render_recovery_prompt(
            "x".repeat(MAX_PROMPT_BYTES),
            vec![history(0, "user", "Prior fact")],
            1,
            Some(0),
            true,
            false,
        );
        assert!(oversized.recovery.unwrap().unavailable_reason.is_some());
    }

    fn launch(provider: ChatAgentProvider, mode: &str) -> ResolvedAgentExecutionV1 {
        ResolvedAgentExecutionV1 {
            schema_version: 1,
            resolution_id: "alr_test".into(),
            config_revision: "cfg_test".into(),
            catalog_version: g5_contracts::types::agent_launch::AGENT_LAUNCH_CATALOG_VERSION.into(),
            provider: ResolvedAgentLaunchFieldV1 {
                value: provider,
                provenance: AgentLaunchProvenanceV1::Explicit,
                config_key: None,
            },
            provider_route: AgentProviderRouteV1::OpenaiDirect,
            model: ResolvedAgentLaunchFieldV1 {
                value: "model".into(),
                provenance: AgentLaunchProvenanceV1::Explicit,
                config_key: None,
            },
            upstream_model: "model".into(),
            permission_mode: ResolvedAgentLaunchFieldV1 {
                value: mode.into(),
                provenance: AgentLaunchProvenanceV1::Explicit,
                config_key: None,
            },
            effort: ResolvedAgentLaunchFieldV1 {
                value: "high".into(),
                provenance: AgentLaunchProvenanceV1::Explicit,
                config_key: None,
            },
            auth_mode: AgentAuthModeV1::OpenAiApiKey,
            egress_mode: AgentEgressModeV1::DirectProvider,
            credential_ready: true,
            requires_interactive_auth: false,
            account_id: None,
            account_kind: None,
            account_scope: None,
        }
    }

    #[test]
    fn resolved_launch_is_read_from_either_snapshot_shape() {
        let resolved = launch(ChatAgentProvider::ClaudeCode, "default");
        let flat = serde_json::to_value(&resolved).unwrap();
        let envelope = serde_json::to_value(PersistedAgentLaunchV1 {
            preferences: AgentLaunchPreferencesV1::default(),
            resolved: resolved.clone(),
        })
        .unwrap();

        let snapshot = |member: Value| json!({ "agent_launch": member });
        let from_flat = resolved_launch(&snapshot(flat)).expect("legacy flat row");
        let from_envelope = resolved_launch(&snapshot(envelope)).expect("envelope row");
        assert_eq!(from_flat.resolution_id, resolved.resolution_id);
        assert_eq!(from_envelope.resolution_id, resolved.resolution_id);
        assert_eq!(from_envelope.permission_mode.value, "default");
        assert!(resolved_launch(&snapshot(json!({"resolved": {"model": 1}}))).is_none());
        assert!(resolved_launch(&json!(null)).is_none());
    }

    #[test]
    fn dispatch_carries_the_selected_endpoint_as_provider_input() {
        for (provider, key) in [
            (ChatAgentProvider::ClaudeCode, "ANTHROPIC_BASE_URL"),
            (ChatAgentProvider::Codex, "OPENAI_BASE_URL"),
        ] {
            let resolved = launch(provider, "default");
            let base = configured_provider_base_url(
                &resolved,
                &json!({key: "  https://provider.example/custom/v1  "}),
            );
            let input = provider_dispatch_input(
                &resolved,
                base.as_deref(),
                "sha256:policy",
                json!({}),
                false,
            );
            assert_eq!(input["base_url"], "https://provider.example/custom/v1");
        }
    }

    #[test]
    fn dispatch_fixes_the_auth_mode_for_every_provider_route() {
        let mut resolved = launch(ChatAgentProvider::Codex, "never/read-only");
        let mut legacy_provider = json!({
            "provider_id": "codex",
            "model_id": "model",
            "policy_ref": "sha256:policy",
            "policy_snapshot": {"sandbox": "read-only"},
            "options": {"effort": "high", "session_title": true},
        });
        for mode in [
            AgentAuthModeV1::NoCredential,
            AgentAuthModeV1::OpenRouterApiKey,
            AgentAuthModeV1::OpenAiApiKey,
            AgentAuthModeV1::AnthropicApiKey,
            AgentAuthModeV1::AnthropicBearer,
            AgentAuthModeV1::AnthropicOauth,
        ] {
            resolved.auth_mode = mode;
            legacy_provider["auth_mode"] = json!(mode);
            assert_eq!(
                provider_dispatch_input(
                    &resolved,
                    None,
                    "sha256:policy",
                    json!({"sandbox": "read-only"}),
                    true,
                ),
                legacy_provider,
            );
        }
        resolved.auth_mode = AgentAuthModeV1::ChatGptSubscription;
        let mut subscription_provider = legacy_provider;
        subscription_provider["auth_mode"] = json!("chat_gpt_subscription");
        assert_eq!(
            provider_dispatch_input(
                &resolved,
                None,
                "sha256:policy",
                json!({"sandbox": "read-only"}),
                true,
            ),
            subscription_provider,
        );
    }

    fn spec() -> AgentTaskSpecV1 {
        AgentTaskSpecV1 {
            schema_version: AGENT_TASK_SPEC_SCHEMA.into(),
            agent_launch: AgentLaunchPreferencesV1::default(),
            formal_review: None,
            formal_review_launch: None,
            allowed_tools: Vec::new(),
            limits: AgentTaskLimits::default(),
            workspace_target: WorkspaceTarget::default(),
            system_notice_ids: Vec::new(),
        }
    }

    #[test]
    fn admission_refuses_tools_g5_no_longer_provides() {
        let mut request = spec();
        request.allowed_tools = vec!["mcp__docs__search".into()];
        assert!(validate_agent_spec(&request).is_ok());
        for removed in [
            "mcp__g5-tools__read_ontology",
            "mcp__g5-tools__browser_click",
            "mcp__g5-tools__list_review_round",
            "mcp__g5-tools__decide_review_suggestion",
        ] {
            request.allowed_tools = vec!["Read".into(), removed.into()];
            assert!(validate_agent_spec(&request).is_err(), "{removed}");
        }
    }

    #[test]
    fn resolved_tool_selection_is_validated_before_dispatch() {
        let mut request = spec();
        for (provider, mode, tools, expected) in [
            (ChatAgentProvider::ClaudeCode, "default", vec![], None),
            (
                ChatAgentProvider::ClaudeCode,
                "default",
                vec!["Read", "AskUserQuestion"],
                None,
            ),
            (
                ChatAgentProvider::ClaudeCode,
                "bypassPermissions",
                vec!["Read"],
                Some("QUESTION_TOOL_EXCLUDED"),
            ),
            (
                ChatAgentProvider::ClaudeCode,
                "bypassPermissions",
                vec!["Read", "AskUserQuestion"],
                None,
            ),
            (
                ChatAgentProvider::ClaudeCode,
                "default",
                vec!["Read"],
                Some("QUESTION_TOOL_EXCLUDED"),
            ),
            (
                ChatAgentProvider::ClaudeCode,
                "default",
                vec!["mcp__docs__search"],
                Some("UNSUPPORTED_ALLOWED_TOOL"),
            ),
            (
                ChatAgentProvider::ClaudeCode,
                "bypassPermissions",
                vec!["Bash(*)"],
                Some("UNSUPPORTED_ALLOWED_TOOL"),
            ),
            (
                ChatAgentProvider::ClaudeCode,
                "bypassPermissions",
                vec!["Read", "Read"],
                Some("DUPLICATE_ALLOWED_TOOL"),
            ),
            (ChatAgentProvider::Codex, "never/read-only", vec![], None),
            (
                ChatAgentProvider::Codex,
                "never/read-only",
                vec!["Read"],
                Some("UNSUPPORTED_PERMISSION_POLICY"),
            ),
        ] {
            request.allowed_tools = tools.iter().map(|tool| (*tool).to_owned()).collect();
            let before = request.allowed_tools.clone();
            let result = permission_policy(&launch(provider, mode), &request);
            match expected {
                Some(error) => assert!(result.unwrap_err().to_string().contains(error)),
                None => {
                    let policy = result.unwrap();
                    assert_eq!(policy["allowed_tools"], json!(before));
                    assert_eq!(
                        policy["tool_approval"],
                        if mode == "default" {
                            "prompt"
                        } else if mode == "bypassPermissions" {
                            "auto_allow"
                        } else {
                            "auto_deny"
                        }
                    );
                }
            }
            assert_eq!(request.allowed_tools, before);
        }
    }

    #[test]
    fn every_catalog_offered_mode_has_a_production_dispatch_policy() {
        for (provider, modes) in [
            (
                ChatAgentProvider::ClaudeCode,
                g5_contracts::claude_models::CLAUDE_CHAT_MODES,
            ),
            (
                ChatAgentProvider::Codex,
                g5_contracts::codex_models::CODEX_CHAT_MODES,
            ),
        ] {
            assert!(!modes.is_empty());
            for mode in modes {
                permission_policy(&launch(provider, mode), &spec()).unwrap_or_else(|error| {
                    panic!("offered {provider:?}/{mode} cannot dispatch: {error}")
                });
            }
        }
    }

    #[test]
    fn policy_mapper_preserves_native_provider_semantics() {
        let claude =
            permission_policy(&launch(ChatAgentProvider::ClaudeCode, "default"), &spec()).unwrap();
        assert_eq!(claude["questions"], "enabled");
        assert_eq!(claude["tool_approval"], "prompt");

        let bypass = permission_policy(
            &launch(ChatAgentProvider::ClaudeCode, "bypassPermissions"),
            &spec(),
        )
        .unwrap();
        assert_eq!(bypass["questions"], "enabled");
        assert_eq!(bypass["tool_approval"], "auto_allow");
        assert_eq!(bypass["sandbox"], "none");

        let codex = permission_policy(
            &launch(ChatAgentProvider::Codex, "never/read-only"),
            &spec(),
        )
        .unwrap();
        assert_eq!(codex["questions"], "disabled");
        assert_eq!(codex["sandbox"], "read-only");
        let full_access = permission_policy(
            &launch(ChatAgentProvider::Codex, "never/danger-full-access"),
            &spec(),
        )
        .unwrap();
        assert_eq!(full_access["questions"], "disabled");
        assert_eq!(full_access["tool_approval"], "auto_deny");
        assert_eq!(full_access["sandbox"], "danger-full-access");
    }

    #[test]
    fn interactive_codex_and_unrepresentable_claude_modes_fail_closed() {
        assert!(permission_policy(
            &launch(ChatAgentProvider::Codex, "on-request/read-only"),
            &spec()
        )
        .is_err());
        assert!(permission_policy(
            &launch(ChatAgentProvider::ClaudeCode, "acceptEdits"),
            &spec()
        )
        .is_err());
    }

    #[test]
    fn agent_turn_permission_modes_are_exactly_the_modes_admission_accepts() {
        for (provider, catalog) in [
            (
                ChatAgentProvider::ClaudeCode,
                g5_contracts::claude_models::CLAUDE_CHAT_MODES,
            ),
            (
                ChatAgentProvider::Codex,
                g5_contracts::codex_models::CODEX_CHAT_MODES,
            ),
        ] {
            let runnable = agent_turn_permission_modes(provider);
            for mode in runnable {
                assert!(catalog.contains(mode), "{mode} is not a catalog mode");
            }
            for mode in catalog {
                assert_eq!(
                    permission_policy(&launch(provider, mode), &spec()).is_ok(),
                    runnable.contains(mode),
                    "{provider:?} {mode}"
                );
            }
        }
    }

    #[test]
    fn dispatch_secret_keys_are_environment_names() {
        let ok = HashMap::from([("OPENAI_API_KEY".to_string(), "secret".to_string())]);
        assert_eq!(validate_dispatch_secrets(&ok, false), Ok(()));
        let bad = HashMap::from([("bad-key".to_string(), "secret".to_string())]);
        assert!(validate_dispatch_secrets(&bad, false).is_err());
        // The retired Codex key is dropped, not refused; a reserved platform
        // key is refused like the terminal would.
        let retired = HashMap::from([
            ("CODEX_AUTH_JSON".to_string(), "{}".to_string()),
            ("NPM_TOKEN".to_string(), "t".to_string()),
        ]);
        assert_eq!(validate_dispatch_secrets(&retired, false), Ok(()));
        let carried = dispatch_secret_set(retired, false);
        assert_eq!(carried.keys().collect::<Vec<_>>(), vec!["NPM_TOKEN"]);
        let reserved = HashMap::from([("GIT_AUTHOR_EMAIL".to_string(), "x".to_string())]);
        assert!(matches!(
            validate_dispatch_secrets(&reserved, false),
            Err(TaskEnvSecretError::ReservedName { .. })
        ));
    }

    #[test]
    fn a_formal_review_admits_only_the_credentials_it_carries() {
        let secrets = HashMap::from([
            ("lower".to_string(), "x".to_string()),
            ("ANTHROPIC_API_KEY".to_string(), "k".to_string()),
        ]);
        assert!(validate_dispatch_secrets(&secrets, false).is_err());
        assert_eq!(validate_dispatch_secrets(&secrets, true), Ok(()));
    }

    #[test]
    fn a_reserved_attempt_reconciles_with_the_entries_that_still_pass() {
        let secrets = HashMap::from([
            ("GOOD".to_string(), "x".to_string()),
            ("bad".to_string(), "y".to_string()),
        ]);
        let (carried, preflight) = reconciled_dispatch_secrets(secrets, false);
        assert_eq!(carried.keys().collect::<Vec<_>>(), vec!["GOOD"]);
        assert!(preflight.unwrap().contains("`bad`"));

        let over: HashMap<String, String> = (0
            ..=g5_contracts::types::task_env::MAX_TASK_ENV_SECRETS)
            .map(|index| (format!("K{index}"), "v".to_string()))
            .collect();
        let (carried, preflight) = reconciled_dispatch_secrets(over, false);
        assert!(carried.is_empty());
        assert!(preflight.unwrap().contains("limit is 128"));

        let valid = HashMap::from([("GOOD".to_string(), "x".to_string())]);
        assert_eq!(
            reconciled_dispatch_secrets(valid.clone(), false),
            (valid, None)
        );
    }

    #[test]
    fn persisted_agent_spec_uses_the_camel_case_wire_contract() {
        let value = json!({
            "agent": {
                "schemaVersion": AGENT_TASK_SPEC_SCHEMA,
                "agentLaunch": AgentLaunchPreferencesV1::default(),
                "allowedTools": [],
                "limits": {},
                "workspaceTarget": {},
                "systemNoticeIds": [],
            }
        });
        assert!(parse_agent_spec(value).is_ok());
        assert!(parse_agent_spec(json!({
            "agent": {
                "schema_version": AGENT_TASK_SPEC_SCHEMA,
                "agent_launch": AgentLaunchPreferencesV1::default(),
            }
        }))
        .is_err());
    }

    /// The retired output limit stays on the wire for the rollout
    /// (`g5.agents.output-bounds`): a stored spec that carries
    /// `maxOutputBytes` still loads, a spec without it defaults to the value
    /// older guests need, the frozen policy still carries it, and no ceiling
    /// applies to it any more. Zero stays refused, because an older guest
    /// refuses the turn.
    #[test]
    fn the_retired_output_limit_still_loads_and_travels_but_has_no_ceiling() {
        let stored = |limits: Value| {
            parse_agent_spec(json!({
                "agent": {
                    "schemaVersion": AGENT_TASK_SPEC_SCHEMA,
                    "agentLaunch": AgentLaunchPreferencesV1::default(),
                    "limits": limits,
                }
            }))
        };
        for (limits, expected) in [
            (json!({}), DEFAULT_MAX_OUTPUT_BYTES),
            (
                json!({"maxOutputBytes": DEFAULT_MAX_OUTPUT_BYTES, "nativeResponseTimeoutMs": 900_000}),
                DEFAULT_MAX_OUTPUT_BYTES,
            ),
            (json!({"maxOutputBytes": 1024}), 1024),
            (
                json!({"maxOutputBytes": 4 * DEFAULT_MAX_OUTPUT_BYTES}),
                4 * DEFAULT_MAX_OUTPUT_BYTES,
            ),
        ] {
            let spec = stored(limits.clone()).unwrap_or_else(|_| panic!("{limits} loads"));
            assert_eq!(spec.limits.max_output_bytes, expected, "{limits}");
            assert!(validate_agent_spec(&spec).is_ok(), "{limits}");
            let policy =
                permission_policy(&launch(ChatAgentProvider::ClaudeCode, "default"), &spec)
                    .unwrap();
            assert_eq!(policy["limits"]["max_output_bytes"], expected, "{limits}");
            // What is stored again carries the field.
            assert_eq!(
                serde_json::to_value(&spec.limits).unwrap()["maxOutputBytes"],
                expected,
                "{limits}"
            );
        }
        let zero = stored(json!({"maxOutputBytes": 0})).unwrap();
        assert!(validate_agent_spec(&zero).is_err());
        assert!(stored(json!({"maxOutputTokens": 1})).is_err());
    }

    #[test]
    fn command_spec_allows_only_the_preview_catalog_operation() {
        let valid = CommandTaskSpecV1 {
            schema_version: "g5.preview-task-spec/v1".to_string(),
            module_id: "workspace.preview".to_string(),
            module_version: PREVIEW_MODULE_VERSION.to_string(),
            operation: "start".to_string(),
            input_schema: "g5.workspace.preview.start/v1".to_string(),
            input: json!({"profile":"setup"}),
        };
        assert!(validate_command_spec(&valid).is_ok());
        let mut arbitrary = valid.clone();
        arbitrary.module_id = "shell.exec".to_string();
        assert!(validate_command_spec(&arbitrary).is_err());
    }
}

/// Cancel up to `limit` agent tasks of `org_id` left with no input after the
/// Start-and-send consent their input was bound to ended
/// (`g5.personal.start-and-send`). The `workspace_vms` trigger withdrew the
/// input in that transition, so such a task can never dispatch.
pub async fn cancel_tasks_left_by_ended_starts_in_org(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    limit: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE tasks
         SET status = 'cancelled',
             outcome = jsonb_build_object('status','cancelled','reason','start_ended'),
             hold_reason = NULL, ended_at = now(), updated_at = now()
         WHERE id IN (
             SELECT task.id FROM tasks AS task
             WHERE task.org_id = $1 AND task.kind = 'agent'
               AND task.status IN ('queued', 'waiting')
               AND task.continuation_reason IS NULL
               AND EXISTS (
                   SELECT 1 FROM session_inputs AS input
                   WHERE input.task_id = task.id AND input.delivery = 'withdrawn'
                     AND input.delivery_audit @> '[{\"event\":\"start_ended\"}]'::jsonb)
               AND NOT EXISTS (
                   SELECT 1 FROM session_inputs AS input
                   WHERE input.task_id = task.id AND input.delivery <> 'withdrawn')
             LIMIT $2)",
    )
    .bind(org_id)
    .bind(limit.clamp(1, 64))
    .execute(conn)
    .await?
    .rows_affected())
}

/// How a queued input relates to the Start-and-send consent it was sent with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartConsentBinding {
    /// The consent still waits or starts; the input is bound to it.
    Bound,
    /// The consent already reached ready; the input is ordinary.
    AlreadyReady,
    /// The consent already ended; the caller must not queue the input.
    Ended,
}

/// Bind `input_id` to Start consent `intent_id` of `workspace_id` inside the
/// queuing transaction (`g5.personal.start-and-send`). The share lock on the
/// VM row orders this against the ready transition, which the
/// `workspace_vms` trigger uses to release the binding.
pub async fn bind_input_to_start_consent(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    intent_id: Uuid,
    input_id: Uuid,
) -> Result<StartConsentBinding, sqlx::Error> {
    let row = sqlx::query_as::<_, (String, Option<Value>)>(
        "SELECT status, start_intent FROM workspace_vms WHERE workspace_id = $1 FOR SHARE",
    )
    .bind(workspace_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((vm_status, intent)) = row else {
        return Ok(StartConsentBinding::Ended);
    };
    if live_owner_start_intent(intent.as_ref(), None, Utc::now()) == Some(intent_id) {
        sqlx::query("UPDATE session_inputs SET start_consent_id = $2 WHERE id = $1")
            .bind(input_id)
            .bind(intent_id)
            .execute(&mut *conn)
            .await?;
        return Ok(StartConsentBinding::Bound);
    }
    let same = intent
        .as_ref()
        .and_then(|intent| intent["id"].as_str())
        .and_then(|id| Uuid::parse_str(id).ok())
        == Some(intent_id);
    let consumed = intent.as_ref().and_then(|intent| intent["status"].as_str()) == Some("consumed");
    Ok(if same && consumed && vm_status == "running" {
        StartConsentBinding::AlreadyReady
    } else {
        StartConsentBinding::Ended
    })
}
