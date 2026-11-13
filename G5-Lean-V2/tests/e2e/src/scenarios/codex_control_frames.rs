//! Codex Next-Turn Settings E2E Tests
//!
//! Verifies Codex model/effort choices travel with the next session input and
//! leave provider runtime state, sibling namespaces, and unknown fields alone.
//!
//! Test view: `views/test_views/codex_support.test_view.md`
//!   TC-CODEX-009  Codex control frames honored mid-session  (AC-9)
//!
//! Deterministic: a Codex chat session is created directly in Postgres
//! (mirroring `sync_job_one_task.rs`'s `agent_provider` fixture), and the
//! current session-message route is driven over HTTP. No LLM. The queued task
//! records the next-turn launch preferences without needing a running sidecar.
//!
//! Run:
//!   cd generation && cargo test -p g5-e2e -- codex_control_frames --test-threads=1 --nocapture

use crate::helpers::api_client::TestApiClient;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

async fn e2e_db_connection(org_id: Uuid) -> anyhow::Result<PgConnection> {
    let mut db = PgConnection::connect(&crate::helpers::env::e2e_db_url()).await?;
    let authz_session_id: Uuid = sqlx::query_scalar(
        "SELECT session_id FROM authz.user_sessions \
         WHERE $1 = ANY(org_ids) AND expires_at > now() \
         ORDER BY refreshed_at DESC LIMIT 1",
    )
    .bind(org_id)
    .fetch_one(&mut db)
    .await?;
    sqlx::query("SELECT authz.set_ctx($1)")
        .bind(authz_session_id)
        .execute(&mut db)
        .await?;
    Ok(db)
}

/// Insert a Codex session bound to a workspace so the message route sees
/// `agent_provider = 'codex'` (not ClaudeCode).
async fn insert_codex_chat_session(
    db: &mut PgConnection,
    workspace_id: Uuid,
) -> anyhow::Result<Uuid> {
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions \
         (id, workspace_id, created_by, scope_type, scope_ref, message_count, org_id, agent_provider) \
         VALUES ($1, $2, (SELECT created_by FROM workspaces WHERE id = $2), 'workspace', $3, 0, \
                 (SELECT org_id FROM workspaces WHERE id = $2), 'codex')",
    )
    .bind(session_id)
    .bind(workspace_id)
    .bind(workspace_id.to_string())
    .execute(&mut *db)
    .await?;
    Ok(session_id)
}

/// Read the persisted `provider_state` JSON for a session.
async fn provider_state(
    db: &mut PgConnection,
    session_id: Uuid,
) -> anyhow::Result<serde_json::Value> {
    use sqlx::Row;
    let row = sqlx::query("SELECT provider_state FROM chat_sessions WHERE id = $1")
        .bind(session_id)
        .fetch_one(&mut *db)
        .await?;
    Ok(row
        .try_get::<Option<serde_json::Value>, _>("provider_state")
        .unwrap_or(None)
        .unwrap_or_else(|| serde_json::json!({})))
}

/// TC-CODEX-009 / chat_agent I42 — Codex-native mutable choices are frozen into
/// the queued turn while the retired out-of-band provider draft stays absent.
#[tokio::test]
async fn test_codex_next_turn_settings_are_queued_verbatim() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

    let org = api.create_org("codex-control-org").await?;
    let ws = api.create_workspace(org.id, "codex-control-ws").await?;

    let mut db = e2e_db_connection(org.id).await?;
    let session_id = insert_codex_chat_session(&mut db, ws.id).await?;
    sqlx::query("UPDATE chat_sessions SET provider_state = $2 WHERE id = $1")
        .bind(session_id)
        .bind(serde_json::json!({
            "claudeCode": { "providerSessionId": "claude-keep", "unknown": true },
            "codex": {
                "providerSessionId": "codex-runtime-keep",
                "providerSessionState": "present",
                "unknownRuntime": { "keep": true },
                "settings": {
                    "permissionMode": "never/workspace-write",
                    "unknownSetting": "keep"
                }
            },
            "queuedTurnContexts": [{ "model": "already-queued" }],
            "unknownRoot": { "keep": true }
        }))
        .execute(&mut db)
        .await?;

    let result: anyhow::Result<()> = async {
        let resp = api
            .authed_post(&format!(
                "{}/api/v1/sessions/{}/messages",
                api.base_url(),
                session_id
            ))
            .json(&serde_json::json!({
                "content": "Use the selected Codex settings",
                "agentProvider": "codex",
                "effort": "xhigh",
                "model": "gpt-5.5"
            }))
            .send()
            .await?;
        let status = resp.status();
        let response_body = resp.text().await?;
        anyhow::ensure!(
            status.is_success(),
            "queueing the Codex turn got {status}: {response_body}"
        );
        let body: serde_json::Value = serde_json::from_str(&response_body)?;
        let task_id = body["data"]["taskId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("queued turn omitted taskId: {body}"))?
            .parse::<Uuid>()?;
        let launch: serde_json::Value =
            sqlx::query_scalar("SELECT spec->'agent'->'agentLaunch' FROM tasks WHERE id = $1")
                .bind(task_id)
                .fetch_one(&mut db)
                .await?;
        anyhow::ensure!(
            launch["provider"] == serde_json::json!({"source":"explicit","value":"codex"})
                && launch["model"] == serde_json::json!({"source":"explicit","value":"gpt-5.5"})
                && launch["permissionMode"]
                    == serde_json::json!({"source":"explicit","value":"never/workspace-write"})
                && launch["effort"] == serde_json::json!({"source":"explicit","value":"xhigh"}),
            "the queued task must carry the exact next-turn Codex choices: {launch}"
        );

        let state = provider_state(&mut db, session_id).await?;
        anyhow::ensure!(
            state["codex"]["providerSessionId"] == "codex-runtime-keep"
                && state["codex"]["providerSessionState"] == "present"
                && state["codex"]["unknownRuntime"]["keep"] == true
                && state["codex"]["settings"]
                    == serde_json::json!({
                        "permissionMode":"never/workspace-write",
                        "unknownSetting":"keep"
                    }),
            "queueing the turn must not create an out-of-band provider draft: {state}"
        );
        anyhow::ensure!(
            state["claudeCode"]["providerSessionId"] == "claude-keep"
                && state["queuedTurnContexts"][0]["model"] == "already-queued"
                && state["unknownRoot"]["keep"] == true,
            "sibling namespace, queued snapshot, and root unknown state must survive: {state}"
        );

        Ok(())
    }
    .await;

    let _ = api.cancel_all_workspace_tasks(ws.id).await;
    result
}
