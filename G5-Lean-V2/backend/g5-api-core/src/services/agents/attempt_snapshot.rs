//! The attempt policy snapshot, `task_attempts.policy_snapshot_ref`.
//!
//! TaskService writes it when it reserves an attempt and is its only reader
//! (`g5.architecture.task_owner`): other owners receive decoded values, as
//! ChatService receives a session's launch. Its `agent_launch` member has two
//! stored shapes: the `{preferences, resolved}` envelope, and the flat
//! resolved contract of rows written before the envelope.

use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{
    AgentLaunchPreferencesV1, PersistedAgentLaunchV1, ResolvedAgentExecutionV1,
};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::errors::AppError;

/// A resolved-launch member (`provider`, `model`, ...) of an attempt policy
/// snapshot. The dispatcher persists `agent_launch` as the
/// `{preferences, resolved}` envelope; rows written before that carry the flat
/// resolved contract, so both are consulted before falling back to `unknown`.
pub(crate) fn snapshot_launch_value<'a>(
    snapshot: Option<&'a Value>,
    member: &str,
) -> Option<&'a str> {
    let snapshot = snapshot?;
    snapshot
        .pointer(&format!("/agent_launch/resolved/{member}/value"))
        .or_else(|| snapshot.pointer(&format!("/agent_launch/{member}/value")))
        .and_then(Value::as_str)
}

/// The resolved launch contract an attempt runs, from either stored shape of
/// its `agent_launch` member. Preferences are not read, so a later change to
/// their shape cannot block a frozen launch.
pub(crate) fn resolved_launch(snapshot: &Value) -> Option<ResolvedAgentExecutionV1> {
    let member = &snapshot["agent_launch"];
    let candidate = member.get("resolved").unwrap_or(member);
    serde_json::from_value(candidate.clone()).ok()
}

/// A chat session's newest launch, or its first when `newest_first` is false,
/// as its attempts' snapshots recorded it.
pub(crate) async fn session_agent_launch(
    db: &mut ScopedDb,
    session_id: Uuid,
    newest_first: bool,
) -> Result<Option<PersistedAgentLaunchV1>, AppError> {
    session_agent_launch_values(db.conn(), &[session_id], newest_first)
        .await?
        .pop()
        .map(|(_, value)| persisted_agent_launch_from_snapshot(value))
        .transpose()
        .map_err(|error| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "task_attempts.policy_snapshot_ref.agent_launch",
                    "code": "invalid_persisted_agent_launch",
                    "message": format!("persisted chat launch contract is invalid: {error}"),
                }),
            })
        })
}

async fn session_agent_launch_values(
    conn: &mut sqlx::PgConnection,
    session_ids: &[Uuid],
    newest_first: bool,
) -> Result<Vec<(Uuid, Value)>, AppError> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (Uuid, Value)>(
        "SELECT DISTINCT ON (task.chat_session_id) task.chat_session_id, \
                attempt.policy_snapshot_ref -> 'agent_launch' \
         FROM task_attempts AS attempt \
         JOIN tasks AS task ON task.id = attempt.task_id \
         WHERE task.chat_session_id = ANY($1) \
           AND attempt.policy_snapshot_ref ? 'agent_launch' \
         ORDER BY task.chat_session_id, \
           CASE WHEN $2 THEN attempt.started_at END DESC, \
           CASE WHEN NOT $2 THEN attempt.started_at END ASC, \
           CASE WHEN $2 THEN attempt.attempt END DESC, \
           CASE WHEN NOT $2 THEN attempt.attempt END ASC, task.id",
    )
    .bind(session_ids)
    .bind(newest_first)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// Decode the `agent_launch` member of a task attempt's policy snapshot.
///
/// Current attempts store the full `{preferences, resolved}` envelope. Older
/// workspace task attempts contain the flat resolved contract; read those
/// independent task records with inherited preferences.
fn persisted_agent_launch_from_snapshot(value: Value) -> Result<PersistedAgentLaunchV1, String> {
    if value.get("resolved").is_some() {
        return serde_json::from_value(value).map_err(|error| error.to_string());
    }
    match serde_json::from_value::<ResolvedAgentExecutionV1>(value.clone()) {
        Ok(resolved) => Ok(PersistedAgentLaunchV1 {
            preferences: AgentLaunchPreferencesV1::default(),
            resolved,
        }),
        Err(flat_error) => match serde_json::from_value::<PersistedAgentLaunchV1>(value) {
            Ok(persisted) => Ok(persisted),
            Err(envelope_error) => Err(format!(
                "neither the persisted envelope ({envelope_error}) nor the resolved launch ({flat_error}) matched"
            )),
        },
    }
}

/// Batch the same first-attempt policy used by send validation for session
/// lists and history copies. Read both historical snapshot shapes.
/// A malformed historical row must not hide every chat in a workspace.
/// Display/copy callers retain its saved settings; sends still decode strictly.
pub(crate) async fn initial_session_permission_modes(
    conn: &mut sqlx::PgConnection,
    session_ids: &[Uuid],
) -> Result<HashMap<Uuid, String>, AppError> {
    Ok(session_agent_launch_values(conn, session_ids, false)
        .await?
        .into_iter()
        .filter_map(|(id, value)| match persisted_agent_launch_from_snapshot(value) {
            Ok(launch) => Some((id, launch.resolved.permission_mode.value)),
            Err(error) => {
                tracing::warn!(session_id = %id, %error, "Keeping saved chat settings: invalid historical launch");
                None
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_contracts::types::agent_launch::{
        AgentAuthModeV1, AgentEgressModeV1, AgentLaunchProvenanceV1, AgentProviderRouteV1,
        ResolvedAgentLaunchFieldV1, AGENT_LAUNCH_CATALOG_VERSION,
    };
    use g5_contracts::types::{ChatAgentProvider, ProviderAccountKind, ProviderAccountScope};
    use serde_json::json;

    #[test]
    fn snapshot_launch_value_reads_the_envelope_and_the_flat_contract() {
        let envelope = json!({"agent_launch": {
            "preferences": {},
            "resolved": {"provider": {"value": "claude_code"}, "model": {"value": "opus"}},
        }});
        let flat = json!({"agent_launch": {
            "provider": {"value": "codex"}, "model": {"value": "gpt"},
        }});
        assert_eq!(
            snapshot_launch_value(Some(&envelope), "provider"),
            Some("claude_code")
        );
        assert_eq!(
            snapshot_launch_value(Some(&envelope), "model"),
            Some("opus")
        );
        assert_eq!(
            snapshot_launch_value(Some(&flat), "provider"),
            Some("codex")
        );
        assert_eq!(snapshot_launch_value(Some(&flat), "model"), Some("gpt"));
        assert_eq!(snapshot_launch_value(None, "provider"), None);
        assert_eq!(snapshot_launch_value(Some(&json!({})), "provider"), None);
    }

    /// The resolved contract exactly as the workspace-VM dispatcher persisted
    /// it on QA (2026-09-22): a Claude subscription account frozen into the
    /// snapshot, so the flat JSON carries `accountId`/`accountKind`/`accountScope`.
    fn qa_resolved_launch() -> ResolvedAgentExecutionV1 {
        let explicit = |value: &str| ResolvedAgentLaunchFieldV1 {
            value: value.to_string(),
            provenance: AgentLaunchProvenanceV1::Explicit,
            config_key: None,
        };
        ResolvedAgentExecutionV1 {
            schema_version: 1,
            resolution_id: "alr_325323dd46dbec1e90ae6436e27a110d".into(),
            config_revision: "cfg_test".into(),
            catalog_version: AGENT_LAUNCH_CATALOG_VERSION.into(),
            provider: ResolvedAgentLaunchFieldV1 {
                value: ChatAgentProvider::ClaudeCode,
                provenance: AgentLaunchProvenanceV1::Explicit,
                config_key: None,
            },
            provider_route: AgentProviderRouteV1::AnthropicDirect,
            model: explicit("claude-opus-4-8[1m]"),
            upstream_model: "claude-opus-4-8[1m]".into(),
            permission_mode: explicit("default"),
            effort: explicit("high"),
            auth_mode: AgentAuthModeV1::AnthropicOauth,
            egress_mode: AgentEgressModeV1::DirectProvider,
            credential_ready: true,
            requires_interactive_auth: false,
            account_id: Some(Uuid::parse_str("8110d68f-7b2f-4585-a05b-225e9e94d793").unwrap()),
            account_kind: Some(ProviderAccountKind::ClaudeSubscription),
            account_scope: Some(ProviderAccountScope::User),
        }
    }

    #[test]
    fn persisted_launch_accepts_the_flat_resolved_contract_the_dispatcher_wrote() {
        let flat = serde_json::to_value(qa_resolved_launch()).unwrap();
        assert!(
            flat.get("accountId").is_some(),
            "fixture must carry the frozen account"
        );
        assert!(flat.get("resolved").is_none());

        let persisted = persisted_agent_launch_from_snapshot(flat).expect("flat row is lifted");
        assert_eq!(persisted.resolved.permission_mode.value, "default");
        assert_eq!(persisted.resolved.model.value, "claude-opus-4-8[1m]");
        assert_eq!(persisted.preferences, AgentLaunchPreferencesV1::default());
    }

    #[test]
    fn persisted_launch_accepts_the_dispatcher_envelope() {
        let envelope = serde_json::to_value(PersistedAgentLaunchV1 {
            preferences: AgentLaunchPreferencesV1::default(),
            resolved: qa_resolved_launch(),
        })
        .unwrap();
        let persisted =
            persisted_agent_launch_from_snapshot(envelope).expect("envelope parses as before");
        assert_eq!(persisted.resolved.effort.value, "high");
    }

    #[test]
    fn persisted_launch_rejects_neither_shape_with_both_errors() {
        let error = persisted_agent_launch_from_snapshot(serde_json::json!({"model": 1}))
            .expect_err("garbage is rejected");
        assert!(error.contains("persisted envelope"), "{error}");
        assert!(error.contains("resolved launch"), "{error}");
    }
}
