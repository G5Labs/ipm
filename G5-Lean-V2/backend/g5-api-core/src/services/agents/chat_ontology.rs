//! Ontology changes cards in workspace chats (`g5.chat.ontology-card`).
//!
//! The in-VM runner records a turn's start snapshot here before its provider
//! starts and reads it back on the turn's later attempts; the agent records the
//! unchanged statements governing its changes (`g5.chat.ontology-card.governing`).
//! Both are kept in `chat_turn_ontology` until the turn ends. When a turn
//! settles as succeeded, its runner's `ontology.card` event is copied onto the
//! reply's last message (`chat_messages.artifacts.ontology_card`), where the
//! owner's edits on the newest card are recorded too
//! (`g5.chat.ontology-card.edit`).

use chrono::Utc;
use g5_auth::WorkspaceIncarnationClaims;
use g5_authz::ScopedDb;
use g5_semantic::ontology_comparison::{self, OntologyStatement};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::errors::{AppError, G5Error};

/// The agent names at most this many governing statements per turn.
pub const MAX_GOVERNING: usize = 8;
/// A stored start snapshot, as JSON.
pub const MAX_BASELINE_BYTES: usize = 6 * 1024 * 1024;
/// A stored card, as JSON; the runner bounds its event below this.
pub const MAX_CARD_BYTES: usize = 1024 * 1024;
const MAX_REASON_CHARS: usize = 300;
const MAX_STATEMENT_ID_BYTES: usize = 200;
const MAX_EDIT_BYTES: usize = 64 * 1024;
pub const CARD_SCHEMA: &str = "g5.ontology-card.v1";

fn conflict(reason: impl Into<String>) -> AppError {
    AppError::from_error(G5Error::Conflict {
        reason: reason.into(),
    })
}

fn invalid(field: &str, error: impl Into<String>) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: json!({"field": field, "error": error.into()}),
    })
}

fn not_found(resource: &str) -> AppError {
    AppError::from_error(G5Error::NotFound {
        resource: resource.to_owned(),
    })
}

// ─── The runner's and agent's turn record ────────────────────────────────────

/// The calling VM's running turn.
#[derive(Debug, Clone, Copy)]
pub struct RunningTurn {
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub attempt: i32,
}

/// The calling VM's running agent turn in a regular workspace: an agent task of
/// the credential's workspace, running on its VM and epoch, outside the
/// personal workspace. Anything else — another workspace's or chat's task, a
/// turn that has ended, an earlier incarnation — is refused alike.
pub async fn running_turn(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    task_id: Uuid,
) -> Result<RunningTurn, AppError> {
    let row = sqlx::query_as::<_, (Option<Uuid>, i32)>(
        "SELECT task.chat_session_id, task.attempt
         FROM tasks AS task
         JOIN workspaces AS workspace ON workspace.id = task.workspace_id
         WHERE task.id = $1 AND task.workspace_id = $2 AND task.kind = 'agent'
           AND task.status = 'running' AND task.vm_id = $3 AND task.epoch = $4
           AND workspace.personal_user_id IS NULL
         FOR SHARE OF task",
    )
    .bind(task_id)
    .bind(claims.workspace_id)
    .bind(claims.vm_id.to_string())
    .bind(i64::try_from(claims.epoch).unwrap_or(i64::MAX))
    .fetch_optional(db.conn())
    .await?;
    match row {
        Some((Some(session_id), attempt)) => Ok(RunningTurn {
            task_id,
            session_id,
            attempt,
        }),
        _ => Err(conflict(
            "this chat turn is not running in a workspace chat on this VM",
        )),
    }
}

/// The turn's recorded start snapshot (`null` until recorded) and governing list.
pub async fn turn_record(db: &mut ScopedDb, task_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query_as::<_, (Option<Value>, Value)>(
        "SELECT baseline, governing FROM chat_turn_ontology WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_optional(db.conn())
    .await?;
    let (baseline, governing) = row.unwrap_or((None, json!([])));
    Ok(json!({"baseline": baseline, "governing": governing}))
}

/// Record the turn's start once; a later attempt's or retry's snapshot never
/// replaces the first. Returns the record as it then stands.
pub async fn record_baseline(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    turn: RunningTurn,
    baseline: &Value,
) -> Result<Value, AppError> {
    let members_ok = baseline
        .get("members")
        .and_then(Value::as_array)
        .is_some_and(|members| {
            members
                .iter()
                .all(|m| m.get("repo_id").and_then(Value::as_str).is_some())
        });
    if !baseline.is_object() || !members_ok {
        return Err(invalid("baseline", "must list the code members' starts"));
    }
    if serde_json::to_vec(baseline).map_or(usize::MAX, |b| b.len()) > MAX_BASELINE_BYTES {
        return Err(invalid("baseline", "exceeds 6 MiB"));
    }
    sqlx::query(
        "INSERT INTO chat_turn_ontology (org_id, workspace_id, task_id, baseline, baseline_attempt)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (task_id) DO UPDATE
         SET baseline = COALESCE(chat_turn_ontology.baseline, EXCLUDED.baseline),
             baseline_attempt = COALESCE(chat_turn_ontology.baseline_attempt, EXCLUDED.baseline_attempt),
             updated_at = now()",
    )
    .bind(claims.org_id)
    .bind(claims.workspace_id)
    .bind(turn.task_id)
    .bind(baseline)
    .bind(turn.attempt)
    .execute(db.conn())
    .await?;
    turn_record(db, turn.task_id).await
}

/// One governing statement the agent names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoverningEntry {
    pub statement_id: String,
    pub repo_id: Uuid,
    pub path: String,
    pub reason: String,
}

fn is_statement_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_STATEMENT_ID_BYTES
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// An ontology file path inside a code member: `.g5/ontology/…/*.g5.md`.
pub fn is_ontology_path(path: &str) -> bool {
    path.len() <= 1024
        && path.starts_with(".g5/ontology/")
        && path.ends_with(".g5.md")
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Add or replace (by statement ID) one governing statement; a ninth distinct
/// statement is refused. Returns the turn's list.
pub async fn record_governing(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    turn: RunningTurn,
    entry: GoverningEntry,
) -> Result<Value, AppError> {
    if !is_statement_id(&entry.statement_id) {
        return Err(invalid("statementId", "is not a statement ID"));
    }
    if !is_ontology_path(&entry.path) {
        return Err(invalid(
            "path",
            "must be an ontology file under .g5/ontology/",
        ));
    }
    let reason = entry.reason.trim();
    if reason.is_empty() || reason.chars().count() > MAX_REASON_CHARS || reason.contains('\n') {
        return Err(invalid("reason", "must be one line of 1-300 characters"));
    }
    sqlx::query(
        "INSERT INTO chat_turn_ontology (org_id, workspace_id, task_id)
         VALUES ($1, $2, $3) ON CONFLICT (task_id) DO NOTHING",
    )
    .bind(claims.org_id)
    .bind(claims.workspace_id)
    .bind(turn.task_id)
    .execute(db.conn())
    .await?;
    let current: Value = sqlx::query_scalar(
        "SELECT governing FROM chat_turn_ontology WHERE task_id = $1 FOR UPDATE",
    )
    .bind(turn.task_id)
    .fetch_one(db.conn())
    .await?;
    let mut list: Vec<GoverningEntry> = serde_json::from_value(current).unwrap_or_default();
    let entry = GoverningEntry {
        reason: reason.to_owned(),
        ..entry
    };
    match list
        .iter()
        .position(|e| e.statement_id == entry.statement_id)
    {
        Some(index) => list[index] = entry,
        None if list.len() >= MAX_GOVERNING => {
            return Err(conflict(format!(
                "a turn names at most {MAX_GOVERNING} governing statements"
            )))
        }
        None => list.push(entry),
    }
    sqlx::query(
        "UPDATE chat_turn_ontology SET governing = $2, updated_at = now() WHERE task_id = $1",
    )
    .bind(turn.task_id)
    .bind(serde_json::to_value(&list).unwrap_or_else(|_| json!([])))
    .execute(db.conn())
    .await?;
    turn_record(db, turn.task_id).await
}

/// Drop an ended turn's record: its card, if any, is on its reply by now.
pub(crate) async fn drop_turn_record(
    conn: &mut sqlx::PgConnection,
    task_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM chat_turn_ontology WHERE task_id = $1")
        .bind(task_id)
        .execute(conn)
        .await?;
    Ok(())
}

// ─── The card on the reply ───────────────────────────────────────────────────

/// Copy the attempt's `ontology.card` event onto its reply's last message,
/// for a turn that settled as succeeded. A malformed or oversized event is
/// dropped rather than shown.
pub(crate) async fn attach_card(
    conn: &mut sqlx::PgConnection,
    task_id: Uuid,
    attempt: i32,
    message_id: Uuid,
) -> Result<(), sqlx::Error> {
    let card = sqlx::query_scalar::<_, Value>(
        "SELECT metadata -> 'payload' FROM task_events
         WHERE task_id = $1 AND attempt = $2 AND metadata ->> 'kind' = 'ontology.card'
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(task_id)
    .bind(attempt)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(mut card) = card else {
        return Ok(());
    };
    if card.get("schema").and_then(Value::as_str) != Some(CARD_SCHEMA)
        || serde_json::to_vec(&card).map_or(usize::MAX, |b| b.len()) > MAX_CARD_BYTES
    {
        tracing::warn!(%task_id, attempt, "ontology changes card dropped: malformed or oversized");
        return Ok(());
    }
    card["edits"] = json!([]);
    super::chat_service::attach_ontology_card(conn, message_id, task_id, card).await
}

// ─── Editing a statement on the newest card ──────────────────────────────────

/// The owner's edit of one statement on a card, as the browser sends it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardEditRequest {
    pub repo_id: Uuid,
    pub path: String,
    pub statement_id: String,
    /// The statement's body as the card shows it.
    pub expected_text: String,
    pub text: String,
    pub idempotency_key: String,
}

/// One recorded edit on a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardEdit {
    pub idempotency_key: String,
    pub repo_id: Uuid,
    pub path: String,
    pub statement_id: String,
    pub text: String,
    pub edited_by: Uuid,
    pub edited_at: String,
}

/// What the edit needs from the card, checked before the file is touched.
#[derive(Debug, Clone)]
pub enum EditPlan {
    /// This key was already recorded: the earlier edit stands.
    Recorded(CardEdit),
    /// Write `text` over the statement whose body is now `expected_body`.
    Write { expected_body: String },
}

/// A code member's checkout folder in the workspace, as its agent sees it.
pub async fn member_root_path(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT root_path FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace_id)
    .bind(repo_id)
    .fetch_optional(db.conn())
    .await?
    .filter(|root| !root.is_empty()))
}

/// Only the workspace's owner edits on a card.
pub async fn require_workspace_owner(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let owner = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT owner_user_id FROM workspaces WHERE id = $1 AND personal_user_id IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(db.conn())
    .await?
    .flatten();
    if owner == Some(user_id) {
        Ok(())
    } else {
        Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["workspace owner".into()],
        }))
    }
}

/// The card on `message_id`, locked.
async fn locked_card(
    db: &mut ScopedDb,
    session_id: Uuid,
    message_id: Uuid,
) -> Result<Value, AppError> {
    sqlx::query_scalar::<_, Value>(
        "SELECT artifacts -> 'ontology_card' FROM chat_messages
         WHERE id = $1 AND session_id = $2 AND role = 'assistant'
           AND artifacts ? 'ontology_card'
         FOR UPDATE",
    )
    .bind(message_id)
    .bind(session_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| not_found("ontology changes card"))
}

/// The card on `message_id`, locked, when it is the session's newest card and
/// no turn of the session is queued, running or waiting.
async fn editable_card(
    db: &mut ScopedDb,
    session_id: Uuid,
    message_id: Uuid,
) -> Result<Value, AppError> {
    let card = locked_card(db, session_id, message_id).await?;
    let newest = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM chat_messages
         WHERE session_id = $1 AND role = 'assistant' AND artifacts ? 'ontology_card'
         ORDER BY message_index DESC, id DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(db.conn())
    .await?;
    if newest != message_id {
        return Err(conflict(
            "only the newest ontology changes card can be edited",
        ));
    }
    let running = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM tasks
         WHERE chat_session_id = $1 AND kind = 'agent'
           AND status IN ('queued', 'running', 'waiting'))",
    )
    .bind(session_id)
    .fetch_one(db.conn())
    .await?;
    if running {
        return Err(conflict(
            "a turn is running in this chat; edit the card when it has ended",
        ));
    }
    Ok(card)
}

fn validate_edit(request: &CardEditRequest) -> Result<(), AppError> {
    let key = &request.idempotency_key;
    if key.is_empty() || key.trim() != key || key.len() > 190 {
        return Err(invalid("idempotencyKey", "must be 1-190 characters"));
    }
    if !is_statement_id(&request.statement_id) {
        return Err(invalid("statementId", "is not a statement ID"));
    }
    if !is_ontology_path(&request.path) {
        return Err(invalid(
            "path",
            "must be an ontology file under .g5/ontology/",
        ));
    }
    if request.text.trim().is_empty() || request.text.len() > MAX_EDIT_BYTES {
        return Err(invalid(
            "text",
            "must be 1 byte to 64 KiB of statement text",
        ));
    }
    Ok(())
}

/// The statement as the card now shows it: its owner's latest edit, else the
/// agent's text; only an added or modified statement of the card is editable.
fn card_body(card: &Value, request: &CardEditRequest) -> Result<String, AppError> {
    let edited = card["edits"].as_array().and_then(|edits| {
        edits.iter().rev().find(|e| {
            e["statementId"] == request.statement_id.as_str()
                && e["path"] == request.path.as_str()
                && e["repoId"] == request.repo_id.to_string().as_str()
        })
    });
    if let Some(edit) = edited {
        return Ok(edit["text"].as_str().unwrap_or_default().to_owned());
    }
    let after = card["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| {
            f["repoId"] == request.repo_id.to_string().as_str()
                && f["path"] == request.path.as_str()
        })
        .flat_map(|f| f["changes"].as_array().cloned().unwrap_or_default())
        .find(|c| c["id"] == request.statement_id.as_str() && c["change"] != "deleted")
        .map(|c| c["after"].clone())
        .ok_or_else(|| not_found("editable statement on this card"))?;
    let statement = statement_from_parts(
        &request.statement_id,
        after["source"].as_str().unwrap_or_default(),
        after["metadata"].as_str().unwrap_or_default(),
    )
    .ok_or_else(|| conflict("the card's statement cannot be read"))?;
    Ok(statement.body.trim().to_owned())
}

/// Rebuild one stored statement version with the shared rule.
pub fn statement_from_parts(id: &str, source: &str, metadata: &str) -> Option<OntologyStatement> {
    let text = format!("{source}## Metadata\n\n### {id}{metadata}");
    ontology_comparison::parse_version(&text)
        .ok()?
        .into_iter()
        .find(|s| s.id == id)
}

/// Check an edit before the file is read: the owner, the newest card with no
/// turn running, and the statement as the card shows it. A key already
/// recorded replays the earlier edit.
pub async fn plan_edit(
    db: &mut ScopedDb,
    session_id: Uuid,
    message_id: Uuid,
    request: &CardEditRequest,
) -> Result<EditPlan, AppError> {
    validate_edit(request)?;
    // A recorded edit replays even after a turn has started or a later card
    // arrived: its reply was lost, not the edit.
    let card = locked_card(db, session_id, message_id).await?;
    if let Some(edit) = recorded_edit(&card, request)? {
        return Ok(EditPlan::Recorded(edit));
    }
    let card = editable_card(db, session_id, message_id).await?;
    let shown = card_body(&card, request)?;
    if request.text.trim() == shown {
        return Err(invalid("text", "is unchanged"));
    }
    if shown != request.expected_text.trim() {
        return Err(conflict(
            "the card changed since it was opened; reload it before editing",
        ));
    }
    Ok(EditPlan::Write {
        expected_body: shown,
    })
}

/// The edit already recorded under the request's key; the same key for a
/// different edit is refused rather than answered with the earlier one.
fn recorded_edit(card: &Value, request: &CardEditRequest) -> Result<Option<CardEdit>, AppError> {
    let Some(edit) = card["edits"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["idempotencyKey"] == request.idempotency_key.as_str())
        .and_then(|e| serde_json::from_value::<CardEdit>(e.clone()).ok())
    else {
        return Ok(None);
    };
    if edit.repo_id != request.repo_id
        || edit.path != request.path
        || edit.statement_id != request.statement_id
        || edit.text != request.text.trim()
    {
        return Err(conflict(
            "this idempotency key was already used for a different edit",
        ));
    }
    Ok(Some(edit))
}

/// The file with one statement's body replaced, or `None` when the file
/// already holds the edited text (a retry after the write landed). Refused
/// when the statement no longer matches the card, when the file uses CRLF
/// line endings, and when the result is not valid ontology.
pub fn replace_statement_body(
    file: &str,
    statement_id: &str,
    expected_body: &str,
    text: &str,
) -> Result<Option<String>, AppError> {
    if file.contains('\r') {
        return Err(conflict(
            "this file uses CRLF line endings; edit it in Files instead",
        ));
    }
    let statements = ontology_comparison::parse_version(file)
        .map_err(|refusal| conflict(refusal.to_string()))?;
    let statement = statements
        .iter()
        .find(|s| s.id == statement_id)
        .ok_or_else(|| conflict("the statement is no longer in its file"))?;
    let current = statement.body.trim();
    let text = text.trim();
    if current == text {
        return Ok(None);
    }
    if current != expected_body.trim() {
        return Err(conflict(
            "the statement changed since the card was made; reload before editing",
        ));
    }
    // `source` spans whole lines from `start_line`; the body is its suffix.
    let line_start: usize = file
        .split_inclusive('\n')
        .take(statement.start_line - 1)
        .map(str::len)
        .sum();
    let source_end = line_start + statement.source.len();
    if file.get(line_start..source_end) != Some(statement.source.as_str()) {
        return Err(conflict("the statement's position could not be confirmed"));
    }
    let body_start = source_end - statement.body.len();
    let body = &statement.body;
    let leading = &body[..body.len() - body.trim_start().len()];
    let trailing = &body[body.trim_end().len()..];
    let edited = format!(
        "{}{leading}{text}{trailing}{}",
        &file[..body_start],
        &file[source_end..]
    );
    // The file parsed before the splice, so a refusal now is the text's doing:
    // name the text, not the parser's advice to repair the file.
    let after = ontology_comparison::parse_version(&edited)
        .map_err(|refusal| not_one_statement(Some(&refusal.diagnostic)))?;
    // Only this statement's text may change: no statement added, removed or
    // re-typed, and every other body intact.
    let same_shape = after.len() == statements.len()
        && after.iter().zip(&statements).all(|(a, b)| {
            a.id == b.id
                && a.kind == b.kind
                && a.metadata == b.metadata
                && (a.id == statement_id || a.body == b.body)
        })
        && after
            .iter()
            .find(|s| s.id == statement_id)
            .is_some_and(|s| s.body.trim() == text);
    if !same_shape {
        return Err(not_one_statement(None));
    }
    Ok(Some(edited))
}

fn not_one_statement(diagnostic: Option<&str>) -> AppError {
    let mut error = "the text must stay one statement: no markers, anchors or headings".to_owned();
    if let Some(diagnostic) = diagnostic {
        error.push_str(&format!(" ({diagnostic})"));
    }
    AppError::from_error(G5Error::ValidationError {
        details: json!({"field": "text", "error": error}),
    })
}

/// Record a written edit on the card, once per key, and queue the next turn's
/// notice in the same transaction (`g5.chat.ontology-card.edit`). The edit was
/// checked before its file was written; once written it is recorded even if a
/// turn has started since, so the file never holds an edit the card and the
/// next agent do not know of.
pub async fn record_edit(
    db: &mut ScopedDb,
    session_id: Uuid,
    message_id: Uuid,
    request: &CardEditRequest,
    user_id: Uuid,
    root_path: Option<&str>,
) -> Result<CardEdit, AppError> {
    let card = locked_card(db, session_id, message_id).await?;
    if let Some(edit) = recorded_edit(&card, request)? {
        return Ok(edit);
    }
    let edit = CardEdit {
        idempotency_key: request.idempotency_key.clone(),
        repo_id: request.repo_id,
        path: request.path.clone(),
        statement_id: request.statement_id.clone(),
        text: request.text.trim().to_owned(),
        edited_by: user_id,
        edited_at: Utc::now().to_rfc3339(),
    };
    super::chat_service::append_ontology_card_edit(
        db.conn(),
        message_id,
        serde_json::to_value(&edit).unwrap_or_default(),
    )
    .await?;
    let file = match root_path {
        Some(root) => format!("{root}/{}", request.path),
        None => request.path.clone(),
    };
    let content = format!(
        "The workspace owner edited statement `{}` in `{file}` on the ontology changes card of an earlier reply. Work from the edited text in the file and keep it unless they ask otherwise.",
        request.statement_id
    );
    // One notice per edit, even when the request is retried.
    let operation_id = Uuid::new_v5(
        &message_id,
        format!("ontology-card-edit:{}", request.idempotency_key).as_bytes(),
    );
    crate::services::agents::chat_service::append_session_notice(
        db.conn(),
        session_id,
        &content,
        operation_id,
        json!({"kind": crate::services::agents::chat_service::CARD_EDIT_NOTICE_KIND, "statement_id": request.statement_id, "path": file}),
    )
    .await?;
    Ok(edit)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Page\n\n<a id=\"x.a\"></a>\n[I · x.a]\n\nAlpha text.\n\n<a id=\"x.b\"></a>\n[D · x.b]\n\nBeta text.\n\n## Metadata\n\n### x.a\n\n- agent\n\n### x.b\n\n- agent\n";

    #[test]
    fn replaces_only_the_named_statement_body() {
        let edited = replace_statement_body(FILE, "x.a", "Alpha text.", "Alpha, edited.")
            .unwrap()
            .unwrap();
        assert!(edited.contains("[I · x.a]\n\nAlpha, edited.\n\n<a id=\"x.b\">"));
        assert!(edited.contains("Beta text."));
        assert_eq!(
            edited.len(),
            FILE.len() + "Alpha, edited.".len() - "Alpha text.".len()
        );
    }

    #[test]
    fn a_retry_after_the_write_landed_writes_nothing() {
        let edited = replace_statement_body(FILE, "x.a", "Alpha text.", "Alpha, edited.")
            .unwrap()
            .unwrap();
        assert_eq!(
            replace_statement_body(&edited, "x.a", "Alpha text.", "Alpha, edited.").unwrap(),
            None
        );
    }

    #[test]
    fn refuses_a_changed_statement_malformed_text_and_crlf() {
        assert!(replace_statement_body(FILE, "x.a", "Something else.", "New.").is_err());
        assert!(replace_statement_body(
            FILE,
            "x.a",
            "Alpha text.",
            "New.\n\n<a id=\"x.c\"></a>\n[I · x.c]\n\nC."
        )
        .is_err());
        assert!(replace_statement_body(FILE, "x.a", "Alpha text.", "New.\n\n## Heading").is_err());
        assert!(
            replace_statement_body(&FILE.replace('\n', "\r\n"), "x.a", "Alpha text.", "New.")
                .is_err()
        );
        assert!(replace_statement_body(FILE, "x.missing", "Alpha text.", "New.").is_err());
    }

    /// Text that would add a statement or a heading is named as the problem,
    /// even when the spliced file no longer parses: the file itself is fine.
    #[test]
    fn malformed_text_is_blamed_on_the_text_not_the_file() {
        let error = |text: &str| match replace_statement_body(FILE, "x.a", "Alpha text.", text) {
            Err(AppError {
                inner: G5Error::ValidationError { details },
                ..
            }) => details["error"].as_str().unwrap_or_default().to_owned(),
            other => panic!("expected a validation error, got {other:?}"),
        };
        let one_statement = "the text must stay one statement: no markers, anchors or headings";
        // An anchored statement without a Metadata entry fails the parse.
        let unparsed = error("New.\n\n<a id=\"x.c\"></a>\n[I · x.c]\n\nC.");
        assert!(unparsed.starts_with(one_statement), "{unparsed}");
        assert!(!unparsed.contains("Repair in Files"), "{unparsed}");
        // A heading parses but changes the statement's shape.
        assert_eq!(error("New.\n\n## Heading"), one_statement);
    }

    #[test]
    fn stored_parts_rebuild_the_statement() {
        let statements = ontology_comparison::parse_version(FILE).unwrap();
        for s in &statements {
            let rebuilt = statement_from_parts(&s.id, &s.source, &s.metadata).unwrap();
            assert_eq!(
                (rebuilt.body, rebuilt.kind, rebuilt.human),
                (s.body.clone(), s.kind, s.human)
            );
        }
    }

    #[test]
    fn ontology_paths_stay_inside_the_ontology() {
        assert!(is_ontology_path(".g5/ontology/features/chat.g5.md"));
        for bad in [
            ".g5/ontology/../x.g5.md",
            "src/a.g5.md",
            ".g5/ontology/a.md",
            ".g5/ontology//a.g5.md",
        ] {
            assert!(!is_ontology_path(bad), "{bad}");
        }
    }
}
