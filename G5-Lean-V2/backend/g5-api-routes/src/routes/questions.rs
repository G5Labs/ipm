//! Referenced-question routes under `/api/v1/workspaces/:workspaceId/questions`
//! (`g5.documents.question.answer-checkpoint`, `g5.architecture.question_owner`).
//!
//! Thin adapters: they authorize the caller, capture the docs cut, close the
//! transaction, and hand the signed-in user to [`QuestionSetService`] as the
//! actor. A request body never carries provenance; one that tries is refused.
//! Every answer needs the owner's attached VM; nothing here starts compute.
//!
//! Votes (`g5.documents.question.votes`) are the one reader write: anyone who
//! can read the workspace casts, changes or withdraws a vote through
//! [`QuestionVoteService`] under the read scope, like a thread reply, with no
//! VM call; the set is resolved first so a vote names only offered options.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace;
use crate::errors::{AppError, G5Error};
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::doc_service::CapturedDocsList;
use crate::services::question_set_service::{
    AnswerCommand, AnswerOutcome, CreateSetCommand, CreateSetOutcome, NewOption, NewQuestion,
    QuestionActor, QuestionSetService, QuestionSetView,
};
use crate::services::question_vote_service::{
    QuestionVote, QuestionVoteService, QuestionVotesView, VoteCommand, WithdrawCommand,
    WithdrawVoteOutcome,
};
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::types::question_set::{AnswerProposal, QuestionSeverity};

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_question_sets), &[Scope::WorkspacesRead])
        .audited_route("/", post(create_question_set), &[Scope::WorkspacesWrite])
        .audited_route("/set", get(get_question_set), &[Scope::WorkspacesRead])
        .audited_route("/answer", post(answer_question), &[Scope::WorkspacesWrite])
        .audited_route("/clear", post(clear_question), &[Scope::WorkspacesWrite])
        .audited_route("/votes", get(list_question_votes), &[Scope::WorkspacesRead])
        .audited_route("/vote", post(vote_question), &[Scope::WorkspacesRead])
        .audited_route(
            "/unvote",
            post(withdraw_question_vote),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

#[derive(Debug, Deserialize)]
pub struct QuestionsPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct QuestionSetQuery {
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSetListDTO {
    pub sets: Vec<QuestionSetView>,
}

/// Body of `POST /answer` and `POST /clear`. `answeredBy` and `provenance`
/// exist only to refuse them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerQuestionRequestDTO {
    pub path: String,
    pub question_id: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    #[serde(default)]
    pub selected_option_ids: Vec<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub applied_hash: Option<String>,
    #[serde(default)]
    pub answered_by: Option<Value>,
    #[serde(default)]
    pub provenance: Option<Value>,
}

impl AnswerQuestionRequestDTO {
    /// The service command; a body that names its own provenance is refused.
    pub fn into_command(self, clear: bool) -> Result<AnswerCommand, AppError> {
        if self.answered_by.is_some() || self.provenance.is_some() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "provenance",
                    "error": "provenance-not-accepted",
                    "message": "the server records who answered; a request cannot name it",
                }),
            }));
        }
        Ok(AnswerCommand {
            path: self.path,
            question_id: self.question_id,
            expected_revision: self.expected_revision,
            idempotency_key: self.idempotency_key,
            proposal: (!clear).then_some(AnswerProposal {
                selected_option_ids: self.selected_option_ids,
                text: self.text,
                rationale: self.rationale,
            }),
            applied_hash: self.applied_hash,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewOptionDTO {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tradeoffs: Option<String>,
    #[serde(default)]
    pub risks: Option<String>,
    #[serde(default)]
    pub recommended: Option<bool>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewQuestionDTO {
    #[serde(default)]
    pub id: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub severity: Option<QuestionSeverity>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default = "default_true")]
    pub allow_free_text: bool,
    #[serde(default)]
    pub options: Vec<NewOptionDTO>,
}

/// Body of `POST /` (create or append). A create never carries an answer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuestionSetRequestDTO {
    pub set_id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub questions: Vec<NewQuestionDTO>,
}

impl CreateQuestionSetRequestDTO {
    pub fn into_command(self) -> CreateSetCommand {
        CreateSetCommand {
            set_id: self.set_id,
            title: self.title,
            questions: self
                .questions
                .into_iter()
                .map(|question| NewQuestion {
                    id: question.id,
                    prompt: question.prompt,
                    context: question.context,
                    severity: question.severity,
                    multi_select: question.multi_select,
                    allow_free_text: question.allow_free_text,
                    options: question
                        .options
                        .into_iter()
                        .map(|option| NewOption {
                            id: option.id,
                            label: option.label,
                            description: option.description,
                            tradeoffs: option.tradeoffs,
                            risks: option.risks,
                            recommended: option.recommended,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// Body of `POST /vote`. `userId` exists only to refuse it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteRequestDTO {
    pub path: String,
    pub question_id: String,
    /// The entry revision the voter saw; a moved revision is refused.
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub selected_option_ids: Vec<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub user_id: Option<Value>,
}

impl VoteRequestDTO {
    /// The service command; a body that names its voter is refused.
    pub fn into_command(self) -> Result<VoteCommand, AppError> {
        if self.user_id.is_some() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "userId",
                    "error": "voter-not-accepted",
                    "message": "the server records who voted; a request cannot name it",
                }),
            }));
        }
        Ok(VoteCommand {
            path: self.path,
            question_id: self.question_id,
            expected_revision: self.expected_revision,
            selected_option_ids: self.selected_option_ids,
            text: self.text,
        })
    }
}

/// Body of `POST /unvote`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawVoteRequestDTO {
    pub path: String,
    pub question_id: String,
}

impl WithdrawVoteRequestDTO {
    pub fn into_command(self) -> WithdrawCommand {
        WithdrawCommand {
            path: self.path,
            question_id: self.question_id,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteOutcomeDTO {
    pub path: String,
    pub vote: QuestionVote,
}

fn service(state: &AppState) -> QuestionSetService<'_> {
    QuestionSetService::new(&state.doc_service, &state.workspace_runtime)
}

fn votes(state: &AppState) -> QuestionVoteService<'_> {
    QuestionVoteService::new(&state.system_pool, &state.event_bus)
}

/// Capture the docs cut on the caller's authorized transaction and close it
/// before any repository or VM I/O.
async fn capture_docs(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
) -> Result<CapturedDocsList, AppError> {
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(captured)
}

/// Capture the docs cut for the owner writing `workspace_id`'s docs member:
/// the same file-mutation policy as the editor Save.
async fn capture_for_owner_write(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    user: &AuthenticatedUser,
    workspace_id: Uuid,
) -> Result<CapturedDocsList, AppError> {
    let captured = state
        .doc_service
        .capture_docs_list(&mut db, workspace_id)
        .await?;
    let repo_id = captured.docs_repo_id().ok_or_else(|| {
        AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"field": "path", "message": "the workspace has no docs repository"}),
        })
    })?;
    crate::routes::workspaces::ensure_workspace_file_mutation_allowed(
        &mut db,
        workspace_id,
        user.user_uuid(),
        repo_id,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(captured)
}

async fn list_question_sets(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
) -> Result<Json<ResponseEnvelope<QuestionSetListDTO>>, AppError> {
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let captured = capture_docs(&state, db, params.workspace_id).await?;
    let sets = service(&state)
        .list_sets(&captured, params.workspace_id, Some(user.user_uuid()))
        .await?;
    Ok(Json(ResponseEnvelope::ok(QuestionSetListDTO { sets })))
}

async fn get_question_set(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
    Query(query): Query<QuestionSetQuery>,
) -> Result<Json<ResponseEnvelope<QuestionSetView>>, AppError> {
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let captured = capture_docs(&state, db, params.workspace_id).await?;
    let set = service(&state)
        .get_set(
            &captured,
            params.workspace_id,
            Some(user.user_uuid()),
            &query.path,
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(set)))
}

async fn create_question_set(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
    Json(body): Json<CreateQuestionSetRequestDTO>,
) -> Result<Json<ResponseEnvelope<CreateSetOutcome>>, AppError> {
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let captured = capture_for_owner_write(&state, db, &user, params.workspace_id).await?;
    let outcome = service(&state)
        .create_or_append(
            &state.system_pool,
            &captured,
            params.workspace_id,
            &QuestionActor::Owner {
                user_id: user.user_uuid(),
            },
            body.into_command(),
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

async fn answer_or_clear(
    state: &Arc<AppState>,
    captured: CapturedDocsList,
    user: &AuthenticatedUser,
    workspace_id: Uuid,
    command: AnswerCommand,
) -> Result<Json<ResponseEnvelope<AnswerOutcome>>, AppError> {
    let outcome = service(state)
        .answer(
            &state.system_pool,
            &captured,
            workspace_id,
            &QuestionActor::Owner {
                user_id: user.user_uuid(),
            },
            command,
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

async fn answer_question(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
    Json(body): Json<AnswerQuestionRequestDTO>,
) -> Result<Json<ResponseEnvelope<AnswerOutcome>>, AppError> {
    let command = body.into_command(false)?;
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let captured = capture_for_owner_write(&state, db, &user, params.workspace_id).await?;
    answer_or_clear(&state, captured, &user, params.workspace_id, command).await
}

async fn clear_question(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
    Json(body): Json<AnswerQuestionRequestDTO>,
) -> Result<Json<ResponseEnvelope<AnswerOutcome>>, AppError> {
    let command = body.into_command(true)?;
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let captured = capture_for_owner_write(&state, db, &user, params.workspace_id).await?;
    answer_or_clear(&state, captured, &user, params.workspace_id, command).await
}

async fn list_question_votes(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
    Query(query): Query<QuestionSetQuery>,
) -> Result<Json<ResponseEnvelope<QuestionVotesView>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let view = votes(&state)
        .list_votes(&mut db, params.workspace_id, user.user_uuid(), &query.path)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(view)))
}

/// Cast or change the signed-in reader's vote. The set is resolved on the
/// caller's read authority (live for the attached owner, committed for every
/// other reader) before the row is written; no VM is started.
async fn vote_question(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
    Json(body): Json<VoteRequestDTO>,
) -> Result<Json<ResponseEnvelope<VoteOutcomeDTO>>, AppError> {
    let command = body.into_command()?;
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let captured = capture_docs(&state, db, params.workspace_id).await?;
    let set = service(&state)
        .get_set(
            &captured,
            params.workspace_id,
            Some(user.user_uuid()),
            &command.path,
        )
        .await?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let vote = votes(&state)
        .vote(
            &mut db,
            params.workspace_id,
            user.user_uuid(),
            &set,
            command,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(VoteOutcomeDTO {
        path: set.path,
        vote,
    })))
}

async fn withdraw_question_vote(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<QuestionsPathParams>,
    Json(body): Json<WithdrawVoteRequestDTO>,
) -> Result<Json<ResponseEnvelope<WithdrawVoteOutcome>>, AppError> {
    let command = body.into_command();
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let outcome = votes(&state)
        .withdraw(&mut db, params.workspace_id, user.user_uuid(), command)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(extra: Value) -> AnswerQuestionRequestDTO {
        let mut value = serde_json::json!({
            "path": "docs/ws/questions/plan.g5q.json",
            "questionId": "q",
            "expectedRevision": 1,
            "idempotencyKey": "k",
            "selectedOptionIds": ["a"],
        });
        if let (Value::Object(map), Value::Object(extra)) = (&mut value, extra) {
            map.extend(extra);
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn vote_bodies_never_name_the_voter() {
        let vote: VoteRequestDTO = serde_json::from_value(serde_json::json!({
            "path": "docs/ws/questions/plan.g5q.json",
            "questionId": "q",
            "selectedOptionIds": ["a"],
        }))
        .unwrap();
        assert_eq!(vote.into_command().unwrap().selected_option_ids, vec!["a"]);
        let forged: VoteRequestDTO = serde_json::from_value(serde_json::json!({
            "path": "docs/ws/questions/plan.g5q.json",
            "questionId": "q",
            "selectedOptionIds": ["a"],
            "userId": "someone",
        }))
        .unwrap();
        let error = forged.into_command().unwrap_err();
        let G5Error::ValidationError { details } = &error.inner else {
            panic!("{error:?}");
        };
        assert_eq!(details["error"], "voter-not-accepted");
    }

    #[test]
    fn bodies_never_carry_provenance() {
        let command = body(serde_json::json!({})).into_command(false).unwrap();
        assert_eq!(command.proposal.unwrap().selected_option_ids, vec!["a"]);
        assert!(body(serde_json::json!({}))
            .into_command(true)
            .unwrap()
            .proposal
            .is_none());
        for extra in [
            serde_json::json!({"answeredBy": {"kind": "user", "id": "someone"}}),
            serde_json::json!({"provenance": {"kind": "agent"}}),
        ] {
            let error = body(extra).into_command(false).unwrap_err();
            let G5Error::ValidationError { details } = &error.inner else {
                panic!("{error:?}");
            };
            assert_eq!(details["error"], "provenance-not-accepted");
        }
    }
}
