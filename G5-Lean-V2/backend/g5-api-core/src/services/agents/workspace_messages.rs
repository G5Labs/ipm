//! Sending a chat message to a workspace for its agent to work on.
//!
//! One entry point for every sender: a person starting a chat, and a fork
//! that starts work in the workspace it creates. The message is queued as a
//! session input in the caller's transaction, and the agent task it creates
//! runs through the normal dispatch path, so sending is database-local and
//! commits atomically with whatever else the caller records.
//!
//! A sender that may retry names the request with a key. The first request's
//! chat and input are returned for the same key and message; a key reused for
//! a different message is refused.

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{AgentLaunchPreferencesV1, AgentLaunchSelectionV1, ChatAgentProvider};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::agents::agent_dispatch::{EnqueueAgentInput, QueuedAgentInput};
use crate::services::agents::chat_service::{
    self, ChatService, CreateSessionRequest, SessionDetail,
};
use crate::services::git::git_conflict_resolution::{
    ConflictResolutionLaunch, ConflictResolutionLauncher, ConflictResolutionTask,
};
use crate::services::workspace::workspace_fork_service::{ForkChatStart, ForkMessenger};
use crate::services::{ConfigService, ProviderAccountService, TaskService};

/// Where the message goes.
#[derive(Debug, Clone)]
pub enum MessageTarget {
    /// A new chat. Without a title, the chat is named from the message.
    NewSession {
        title: Option<String>,
        scope_type: Option<String>,
        scope_ref: Option<String>,
    },
    /// A fork of an existing chat that continues under a persona ("follow
    /// view", workbench `g5.workbench.persona`): the copied transcript is
    /// historical context and the message is the fork's first new input.
    ForkSession {
        source_session_id: Uuid,
        from_message_index: i32,
        title: Option<String>,
    },
    /// A follow-up in an existing chat. Only its request key is claimed
    /// here; the follow-up path queues the input (it inherits the chat's
    /// launch) and records the result with [`WorkspaceMessenger::record_claim`].
    ExistingSession { session_id: Uuid },
}

/// The workbench part of a request's identity (workbench
/// `g5.workbench.actions`): the persona, the template and the configuration
/// snapshot the browser resolved them against. It joins the request digest
/// so a retry with the same key replays the same expansion
/// (`g5.conversation.inbox.reservation`).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchIdentity {
    pub persona: String,
    /// Ribbon phase the action was clicked in; with `action_id` it selects
    /// exactly that phase's template, independent of the persona.
    pub phase: Option<String>,
    pub action_id: Option<String>,
    pub context: Option<serde_json::Value>,
    pub config_revision: String,
}

/// What a persona start adds once the request key is claimed: the resolved
/// instructions frozen on the session and the expanded first-input text
/// (`g5.workbench.persona`: instructions prepended before the bounded prompt
/// is frozen).
#[derive(Debug, Clone)]
pub struct PersonaExpansion {
    pub persona: String,
    pub instructions: String,
    /// The complete first input: instructions, template, context, user text.
    pub content: String,
}

/// A message for a workspace's agent.
#[derive(Debug, Clone)]
pub struct WorkspaceMessage {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    /// The sender. Only the workspace owner's agent runs their messages.
    pub actor_user_id: Uuid,
    pub target: MessageTarget,
    pub content: String,
    /// The requested agent launch; `None` inherits the configured defaults.
    pub agent_launch: Option<AgentLaunchSelectionV1>,
    /// Individual launch fields from older clients.
    pub legacy_preferences: AgentLaunchPreferencesV1,
    /// Validated first-turn attachment identity, when the request has files.
    pub attachment_digest: Option<String>,
    /// Names the request so a retry returns the first result.
    pub request_key: Option<String>,
    /// Workbench identity of a templated/persona start; `None` for an
    /// ordinary message, whose digest is unchanged.
    pub workbench: Option<WorkbenchIdentity>,
}

/// The chat and queued input a message produced.
#[derive(Debug, Clone)]
pub struct SentWorkspaceMessage {
    pub session: SessionDetail,
    pub queued: QueuedAgentInput,
    /// An earlier request with the same key produced this result.
    pub replayed: bool,
}

/// Sends messages to workspaces. Holds only database-local services.
#[derive(Clone)]
pub struct WorkspaceMessenger {
    chat: ChatService,
    tasks: TaskService,
    config: ConfigService,
    accounts: ProviderAccountService,
}

impl WorkspaceMessenger {
    #[must_use]
    pub fn new(
        chat: ChatService,
        tasks: TaskService,
        config: ConfigService,
        accounts: ProviderAccountService,
    ) -> Self {
        Self {
            chat,
            tasks,
            config,
            accounts,
        }
    }

    /// Queue `message` in the caller's transaction. Nothing is committed
    /// here; the caller owns the commit boundary.
    pub async fn send(
        &self,
        db: &mut ScopedDb,
        message: WorkspaceMessage,
    ) -> Result<SentWorkspaceMessage, AppError> {
        if let Some(sent) = self.claim(db, &message).await? {
            return Ok(sent);
        }
        self.send_claimed(db, message, None).await
    }

    /// Claim the request key of `message`, or return the result an earlier
    /// request with the key produced. `None` means the key is now ours (or
    /// the message has none) and nothing has been created yet: a caller that
    /// must resolve more state before queuing (a persona start reading the
    /// workbench configuration) does so now and then calls
    /// [`Self::send_claimed`] in the same transaction, so a failure between
    /// the two rolls the claim back with everything else
    /// (`g5.conversation.inbox.reservation`).
    /// Whether `request_key` was already claimed by this person in the
    /// organization, so a retry will replay the stored chat
    /// (`g5.chat.request-key`) and must not record another Start
    /// (`g5.chat.start-and-send`).
    pub async fn request_key_claimed(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_user_id: Uuid,
        request_key: &str,
    ) -> Result<bool, AppError> {
        let claimed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_message_requests
             WHERE org_id = $1 AND requested_by = $2 AND request_key = $3)",
        )
        .bind(org_id)
        .bind(actor_user_id)
        .bind(request_key)
        .fetch_one(db.conn())
        .await?;
        Ok(claimed)
    }

    pub async fn claim(
        &self,
        db: &mut ScopedDb,
        message: &WorkspaceMessage,
    ) -> Result<Option<SentWorkspaceMessage>, AppError> {
        if let Some(key) = message.request_key.as_deref() {
            if key.is_empty() || key.trim() != key || key.len() > 200 {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: json!({
                        "field": "requestKey",
                        "error": "must be 1-200 characters without surrounding whitespace",
                    }),
                }));
            }
        }
        let digest = request_digest(message)?;
        if let Some(key) = message.request_key.as_deref() {
            return self.claim_key(db, message, key, &digest).await;
        }
        Ok(None)
    }

    /// Create the chat and queue the input after [`Self::claim`] returned
    /// `None`. With `expansion`, the session records the persona and its
    /// frozen instructions and the queued input is the expanded text.
    pub async fn send_claimed(
        &self,
        db: &mut ScopedDb,
        message: WorkspaceMessage,
        expansion: Option<PersonaExpansion>,
    ) -> Result<SentWorkspaceMessage, AppError> {
        let launch = self
            .config
            .resolve_launch(
                db,
                message.org_id,
                message.workspace_id,
                Some(message.actor_user_id),
                message.agent_launch.as_ref(),
                message.legacy_preferences.clone(),
                &self.accounts,
            )
            .await?;
        let agent_provider = launch.resolved.provider.value;
        let (persona, persona_instructions, content) = match &expansion {
            Some(expansion) => (
                Some(expansion.persona.clone()),
                Some(expansion.instructions.clone()),
                expansion.content.clone(),
            ),
            None => (None, None, message.content.clone()),
        };
        let mut session = match message.target.clone() {
            MessageTarget::NewSession {
                title,
                scope_type,
                scope_ref,
            } => {
                self.chat
                    .create_session(
                        db,
                        message.workspace_id,
                        message.actor_user_id,
                        CreateSessionRequest {
                            scope_type,
                            scope_ref,
                            model: Some(launch.resolved.model.value.clone()),
                            // Name the session from its first request right
                            // away; the agent's first turn later replaces this
                            // provisional title with a one-line summary unless
                            // the user renamed it.
                            derived_title: title.is_none(),
                            title: title
                                .or_else(|| chat_service::derive_session_title(&message.content)),
                            agent_provider,
                            persona,
                            persona_instructions,
                        },
                    )
                    .await?
            }
            MessageTarget::ForkSession {
                source_session_id,
                from_message_index,
                title,
            } => {
                let forked = self
                    .chat
                    .fork_session(
                        db,
                        source_session_id,
                        chat_service::ForkSessionRequest {
                            from_message_index,
                            title,
                            persona: persona.clone().zip(persona_instructions),
                        },
                    )
                    .await?;
                self.chat.get_session(db, forked.session_id).await?
            }
            MessageTarget::ExistingSession { .. } => {
                return Err(AppError::from_error(G5Error::Internal {
                    message: "a follow-up is queued by the follow-up path, not send_claimed".into(),
                }));
            }
        };
        // The first turn's resolved settings, account included, become the
        // chat's next-turn defaults, so a chat begun on a named account keeps
        // that account for later turns (routes/chat D33). The returned
        // session carries them: the client hydrates its composer from it.
        session.provider_state = self
            .chat
            .patch_provider_state_namespace(
                db,
                session.id,
                provider_state_namespace(agent_provider),
                json!({}),
                chat_service::first_turn_settings(&launch.resolved)?,
                json!({}),
            )
            .await?;
        let queued = self
            .tasks
            .enqueue_agent_input(
                db,
                EnqueueAgentInput {
                    task_name: None,
                    session_id: session.id,
                    actor_user_id: message.actor_user_id,
                    content,
                    artifacts: json!([]),
                    input_origin: "human".to_string(),
                    template_ref: None,
                    template_revision: None,
                    confirmed_launch: message.agent_launch.as_ref().map(|_| launch.clone()),
                    agent_launch: launch.preferences,
                },
            )
            .await?;
        self.record_claim(db, &message, session.id, queued.input_id)
            .await?;
        Ok(SentWorkspaceMessage {
            session,
            queued,
            replayed: false,
        })
    }

    /// Record what a claimed request produced, so a retry with its key
    /// returns that chat and input. Nothing to record without a key.
    pub async fn record_claim(
        &self,
        db: &mut ScopedDb,
        message: &WorkspaceMessage,
        session_id: Uuid,
        input_id: Uuid,
    ) -> Result<(), AppError> {
        if let Some(key) = message.request_key.as_deref() {
            sqlx::query(
                "UPDATE workspace_message_requests SET session_id = $3, input_id = $4 \
                 WHERE requested_by = $1 AND request_key = $2 AND org_id = $5",
            )
            .bind(message.actor_user_id)
            .bind(key)
            .bind(session_id)
            .bind(input_id)
            .bind(message.org_id)
            .execute(db.conn())
            .await?;
        }
        Ok(())
    }

    /// Claim `key` for this request, or return the result an earlier request
    /// with the key produced. A concurrent claim waits for the other
    /// transaction and then replays its result.
    async fn claim_key(
        &self,
        db: &mut ScopedDb,
        message: &WorkspaceMessage,
        key: &str,
        digest: &str,
    ) -> Result<Option<SentWorkspaceMessage>, AppError> {
        let claimed = sqlx::query(
            "INSERT INTO workspace_message_requests \
                 (org_id, workspace_id, requested_by, request_key, request_digest) \
             VALUES ($5, $1, $2, $3, $4) \
             ON CONFLICT (org_id, requested_by, request_key) DO NOTHING \
             RETURNING request_key",
        )
        .bind(message.workspace_id)
        .bind(message.actor_user_id)
        .bind(key)
        .bind(digest)
        .bind(message.org_id)
        .fetch_optional(db.conn())
        .await?;
        if claimed.is_some() {
            return Ok(None);
        }
        let existing = sqlx::query(
            "SELECT request.workspace_id, request.request_digest, request.session_id, \
                    request.input_id, input.delivery, input.created_at, input.task_id, \
                    task.status AS task_status \
             FROM workspace_message_requests AS request \
             LEFT JOIN session_inputs AS input ON input.id = request.input_id \
             LEFT JOIN tasks AS task ON task.id = input.task_id \
             WHERE request.org_id = $1 AND request.requested_by = $2 \
               AND request.request_key = $3",
        )
        .bind(message.org_id)
        .bind(message.actor_user_id)
        .bind(key)
        .fetch_one(db.conn())
        .await?;
        if existing.try_get::<Uuid, _>("workspace_id")? != message.workspace_id
            || existing.try_get::<String, _>("request_digest")? != digest
        {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: json!({
                    "field": "requestKey",
                    "error": "already used for a different message",
                }),
            }));
        }
        let (Some(session_id), Some(input_id), Some(task_id)) = (
            existing.try_get::<Option<Uuid>, _>("session_id")?,
            existing.try_get::<Option<Uuid>, _>("input_id")?,
            existing.try_get::<Option<Uuid>, _>("task_id")?,
        ) else {
            // The chat or its input was deleted since.
            return Err(AppError::from_error(G5Error::Gone {
                resource: format!("the chat started by request {key}"),
            }));
        };
        let session = self.chat.get_session(db, session_id).await?;
        Ok(Some(SentWorkspaceMessage {
            session,
            queued: QueuedAgentInput {
                input_id,
                task_id,
                task_status: existing
                    .try_get::<Option<String>, _>("task_status")?
                    .unwrap_or_default(),
                delivery: existing
                    .try_get::<Option<String>, _>("delivery")?
                    .unwrap_or_default(),
                created_at: existing
                    .try_get::<Option<DateTime<Utc>>, _>("created_at")?
                    .unwrap_or_else(Utc::now),
                created_task: false,
            },
            replayed: true,
        }))
    }
}

/// Provider-state JSON namespace for per-session control settings. Codex
/// sessions persist under `codex`; every other provided agent uses the
/// historical `claudeCode` namespace.
#[must_use]
pub fn provider_state_namespace(agent_provider: ChatAgentProvider) -> &'static str {
    match agent_provider {
        ChatAgentProvider::Codex => "codex",
        _ => "claudeCode",
    }
}

/// The request's identity: its workspace, target, message and launch choice,
/// plus the workbench persona/template/revision when the request names them.
/// A message without workbench fields keeps the pre-workbench digest.
fn request_digest(message: &WorkspaceMessage) -> Result<String, AppError> {
    let target = match &message.target {
        MessageTarget::NewSession {
            title,
            scope_type,
            scope_ref,
        } => json!({
            "kind": "new_session",
            "title": title,
            "scopeType": scope_type,
            "scopeRef": scope_ref,
        }),
        MessageTarget::ForkSession {
            source_session_id,
            from_message_index,
            title,
        } => json!({
            "kind": "fork_session",
            "sourceSessionId": source_session_id,
            "fromMessageIndex": from_message_index,
            "title": title,
        }),
        MessageTarget::ExistingSession { session_id } => json!({
            "kind": "existing_session",
            "sessionId": session_id,
        }),
    };
    let mut identity = json!({
        "version": 1,
        "workspaceId": message.workspace_id,
        "target": target,
        "content": message.content,
        "agentLaunch": message.agent_launch,
        "legacyPreferences": message.legacy_preferences,
    });
    if let Some(attachment_digest) = &message.attachment_digest {
        identity["attachmentDigest"] = json!(attachment_digest);
    }
    if let Some(workbench) = &message.workbench {
        identity["workbench"] = serde_json::to_value(workbench).map_err(|error| {
            AppError::from_error(G5Error::Internal {
                message: format!("message request digest: {error}"),
            })
        })?;
    }
    let canonical = serde_json::to_vec(&identity).map_err(|error| {
        AppError::from_error(G5Error::Internal {
            message: format!("message request digest: {error}"),
        })
    })?;
    Ok(hex::encode(Sha256::digest(canonical)))
}

/// Git's conflict-resolution task is a workflow chat: the launch resolves
/// through shared configuration, the session is created and the brief queued in the
/// caller's transaction.
#[async_trait::async_trait]
impl ConflictResolutionLauncher for WorkspaceMessenger {
    async fn launch(
        &self,
        db: &mut ScopedDb,
        launch: ConflictResolutionLaunch<'_>,
    ) -> Result<ConflictResolutionTask, AppError> {
        let org_id = db.primary_org_id()?.into_uuid();
        let resolved = self
            .config
            .resolve_launch(
                db,
                org_id,
                launch.workspace_id,
                Some(launch.actor_user_id),
                launch.agent_launch,
                Default::default(),
                &self.accounts,
            )
            .await?;
        let session = self
            .chat
            .create_session(
                db,
                launch.workspace_id,
                launch.actor_user_id,
                CreateSessionRequest {
                    derived_title: true,
                    scope_type: None,
                    scope_ref: launch.scope_ref,
                    model: Some(resolved.resolved.model.value.clone()),
                    title: Some(launch.title.into()),
                    agent_provider: resolved.resolved.provider.value,
                    persona: None,
                    persona_instructions: None,
                },
            )
            .await?;
        let queued = self
            .tasks
            .enqueue_agent_input(
                db,
                EnqueueAgentInput {
                    session_id: session.id,
                    actor_user_id: launch.actor_user_id,
                    task_name: Some(launch.title.to_string()),
                    content: launch.prompt,
                    artifacts: json!([]),
                    input_origin: "workflow".into(),
                    template_ref: Some(launch.template_ref.into()),
                    template_revision: Some(launch.template_revision.into()),
                    confirmed_launch: launch.agent_launch.map(|_| resolved.clone()),
                    agent_launch: resolved.preferences,
                },
            )
            .await?;
        self.chat
            .patch_provider_state_namespace(
                db,
                session.id,
                provider_state_namespace(resolved.resolved.provider.value),
                json!({}),
                chat_service::first_turn_settings(&resolved.resolved)?,
                json!({}),
            )
            .await?;
        Ok(ConflictResolutionTask {
            task_id: queued.task_id,
            session_id: session.id,
            status: queued.task_status,
            input_id: Some(queued.input_id),
        })
    }
}

/// A fork's chat is an ordinary message to the new workspace; its carried
/// sessions are copied by the chat service.
#[async_trait::async_trait]
impl ForkMessenger for WorkspaceMessenger {
    async fn copy_sessions(
        &self,
        conn: &mut sqlx::PgConnection,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        session_ids: &[Uuid],
        created_by: Uuid,
    ) -> Result<i32, AppError> {
        ChatService::copy_sessions_to_workspace(
            conn,
            source_workspace_id,
            target_workspace_id,
            session_ids,
            created_by,
        )
        .await
    }

    async fn start_chat(&self, db: &mut ScopedDb, chat: ForkChatStart) -> Result<Uuid, AppError> {
        let sent = self
            .send(
                db,
                WorkspaceMessage {
                    org_id: chat.org_id,
                    workspace_id: chat.workspace_id,
                    actor_user_id: chat.actor_user_id,
                    target: MessageTarget::NewSession {
                        title: None,
                        scope_type: None,
                        scope_ref: None,
                    },
                    content: chat.content,
                    // Resolved for the new workspace: a confirmed selection
                    // names a resolution of the source workspace.
                    agent_launch: None,
                    legacy_preferences: chat.legacy_preferences,
                    attachment_digest: None,
                    request_key: None,
                    workbench: None,
                },
            )
            .await?;
        Ok(sent.session.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_message() -> WorkspaceMessage {
        WorkspaceMessage {
            org_id: Uuid::nil(),
            workspace_id: Uuid::from_u128(0x1234),
            actor_user_id: Uuid::from_u128(0x5678),
            target: MessageTarget::NewSession {
                title: Some("Preview triage".to_string()),
                scope_type: Some("workspace".to_string()),
                scope_ref: None,
            },
            content: "can you check the preview?".to_string(),
            agent_launch: None,
            legacy_preferences: AgentLaunchPreferencesV1::default(),
            attachment_digest: Some("abc".to_string()),
            request_key: Some("key-1".to_string()),
            workbench: None,
        }
    }

    /// The digest function as it was before the workbench fields existed
    /// (`origin/main` at 621e842e), kept verbatim as the oracle.
    fn legacy_request_digest(message: &WorkspaceMessage) -> String {
        let MessageTarget::NewSession {
            title,
            scope_type,
            scope_ref,
        } = &message.target
        else {
            panic!("legacy digests cover new sessions only");
        };
        let mut identity = json!({
            "version": 1,
            "workspaceId": message.workspace_id,
            "target": {
                "kind": "new_session",
                "title": title,
                "scopeType": scope_type,
                "scopeRef": scope_ref,
            },
            "content": message.content,
            "agentLaunch": message.agent_launch,
            "legacyPreferences": message.legacy_preferences,
        });
        if let Some(attachment_digest) = &message.attachment_digest {
            identity["attachmentDigest"] = json!(attachment_digest);
        }
        hex::encode(Sha256::digest(serde_json::to_vec(&identity).unwrap()))
    }

    /// A message without workbench fields keeps the pre-Phase-2 identity
    /// byte for byte (`g5.conversation.inbox.reservation`): request keys
    /// claimed before the workbench shipped replay against the same digest.
    #[test]
    fn a_message_without_workbench_fields_keeps_the_legacy_digest() {
        let digest = request_digest(&legacy_message()).unwrap();
        assert_eq!(digest, legacy_request_digest(&legacy_message()));
        assert_eq!(
            digest,
            "a258682f6912f747f112dc6e234999d846082978884934b3420cbde47cfb93fa"
        );
    }

    #[test]
    fn workbench_fields_change_the_digest_only_when_present() {
        let legacy = request_digest(&legacy_message()).unwrap();
        let mut message = legacy_message();
        message.workbench = Some(WorkbenchIdentity {
            persona: "be".into(),
            phase: Some("be".into()),
            action_id: Some("generate-mock".into()),
            context: None,
            config_revision: "rev".into(),
        });
        let workbench = request_digest(&message).unwrap();
        assert_ne!(legacy, workbench);
        // The phase is part of the identity.
        message.workbench.as_mut().unwrap().phase = Some("se".into());
        assert_ne!(workbench, request_digest(&message).unwrap());
    }
}
