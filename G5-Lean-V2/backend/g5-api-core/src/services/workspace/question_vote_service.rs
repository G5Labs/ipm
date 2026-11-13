//! Votes on referenced questions (`g5.documents.question.votes`).
//!
//! A vote is a reader's preference on a question of a set file: one durable
//! platform row per workspace, set path, question id and voter, beside the
//! Git-owned question and answer. Like a thread reply it changes no
//! repository content, needs no running VM, and is not a decision — the
//! owner's Answer through [`QuestionSetService`](super::question_set_service)
//! stays the decision. The service validates a vote against the set entry
//! the voter can resolve (live for the attached owner, committed for every
//! other reader), upserts or withdraws the voter's row, lists the rows of a
//! set with voter names for the cards' tally, and publishes `question.vote`
//! after the caller commits.

use std::collections::BTreeSet;

use g5_authz::ScopedDb;
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::scoped::{ensure_user_workspace_access_until_commit, resolve_org_id_for_workspace};
use crate::errors::{AppError, G5Error};
use crate::services::question_set_service::QuestionSetView;
use crate::services::user_service::{UserService, UserSummary};
use crate::services::EventBus;
use crate::ws::{Channel, WsEvent};
use g5_contracts::types::question_set::parse_question_set_path;

/// Longest free text a vote may carry.
pub const MAX_VOTE_TEXT_CHARS: usize = 2_000;

/// One voter's row as every card sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionVote {
    pub question_id: String,
    pub user_id: Uuid,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub selected_option_ids: Vec<String>,
    pub text: String,
    /// The entry revision the vote was cast against.
    pub question_revision: u64,
    pub updated_at: String,
}

/// The votes of one set, for every question in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionVotesView {
    pub path: String,
    /// The signed-in viewer, so a card can mark its own vote.
    pub viewer_id: Uuid,
    pub votes: Vec<QuestionVote>,
}

/// A cast or changed vote. Who votes comes from the authenticated caller;
/// `expected_revision`, when given, is the entry revision the voter saw and
/// a moved revision refuses the vote with the current entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteCommand {
    pub path: String,
    pub question_id: String,
    pub expected_revision: Option<u64>,
    pub selected_option_ids: Vec<String>,
    pub text: String,
}

/// A withdrawal: the voter comes from the authenticated caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawCommand {
    pub path: String,
    pub question_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawVoteOutcome {
    pub path: String,
    pub question_id: String,
    /// False when the voter had no vote to withdraw.
    pub withdrawn: bool,
}

#[derive(sqlx::FromRow)]
struct VoteRow {
    question_id: String,
    user_id: Uuid,
    selected_option_ids: Vec<String>,
    text: String,
    question_revision: i64,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl VoteRow {
    /// The row as cards see it; a voter without a user row keeps the id.
    fn into_dto(self, voter: Option<&UserSummary>) -> QuestionVote {
        QuestionVote {
            question_id: self.question_id,
            display_name: voter
                .map(|voter| voter.display_name.clone())
                .unwrap_or_else(|| self.user_id.to_string()),
            avatar_url: voter.and_then(|voter| voter.avatar_url.clone()),
            user_id: self.user_id,
            selected_option_ids: self.selected_option_ids,
            text: self.text,
            question_revision: u64::try_from(self.question_revision).unwrap_or(0),
            updated_at: self.updated_at.to_rfc3339(),
        }
    }
}

fn validation(field: &str, code: &str, message: impl Into<String>) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: json!({"field": field, "error": code, "message": message.into()}),
    })
}

fn valid_path(path: &str) -> Result<String, AppError> {
    parse_question_set_path(path)
        .map(|parsed| parsed.path)
        .ok_or_else(|| {
            validation(
                "path",
                "invalid-question-set-path",
                "a question set path is docs/<workspace>/questions/<set>.g5q.json",
            )
        })
}

/// The one writer and reader of `question_votes`.
pub struct QuestionVoteService<'a> {
    system_pool: &'a PgPool,
    event_bus: &'a EventBus,
}

impl<'a> QuestionVoteService<'a> {
    pub fn new(system_pool: &'a PgPool, event_bus: &'a EventBus) -> Self {
        Self {
            system_pool,
            event_bus,
        }
    }

    /// Every vote of the set at `path`, in question order of casting, with
    /// the voters' current display names.
    pub async fn list_votes(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        viewer_id: Uuid,
        path: &str,
    ) -> Result<QuestionVotesView, AppError> {
        let path = valid_path(path)?;
        let rows = sqlx::query_as::<_, VoteRow>(
            "SELECT question_id, user_id, selected_option_ids, text, question_revision, updated_at \
             FROM question_votes WHERE workspace_id = $1 AND set_path = $2 \
             ORDER BY question_id, created_at, user_id",
        )
        .bind(workspace_id)
        .bind(&path)
        .fetch_all(db.conn())
        .await
        .map_err(|error| AppError::from_error(G5Error::from(error)))?;
        let voters =
            UserService::load_summaries(self.system_pool, rows.iter().map(|row| row.user_id))
                .await?;
        let votes = rows
            .into_iter()
            .map(|row| {
                let voter = voters.get(&row.user_id);
                row.into_dto(voter)
            })
            .collect();
        Ok(QuestionVotesView {
            path,
            viewer_id,
            votes,
        })
    }

    /// Cast or replace `voter`'s vote. `set` is the set as the voter
    /// resolves it; the vote must name a question in it and only options the
    /// entry offers, and carry a selection or text.
    pub async fn vote(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        voter_id: Uuid,
        set: &QuestionSetView,
        command: VoteCommand,
    ) -> Result<QuestionVote, AppError> {
        let path = valid_path(&command.path)?;
        if set.path != path {
            return Err(validation(
                "path",
                "set-mismatch",
                "the resolved set does not match the vote's path",
            ));
        }
        if let Some(invalid) = &set.invalid {
            return Err(validation(
                "path",
                "question-set-invalid",
                format!("{path} is not a valid question set ({})", invalid.code),
            ));
        }
        let entry = set
            .questions
            .iter()
            .find(|entry| entry.question.id == command.question_id)
            .ok_or_else(|| {
                validation(
                    "questionId",
                    "question-missing",
                    format!("question {} is not in {path}", command.question_id),
                )
            })?;
        if let Some(expected) = command.expected_revision {
            if expected != entry.question.revision {
                return Err(AppError::from_error(G5Error::QuestionConflict {
                    details: json!({
                        "code": "revision-moved",
                        "path": path,
                        "questionId": entry.question.id,
                        "expectedRevision": expected,
                        "currentRevision": entry.question.revision,
                        "current": entry,
                    }),
                }));
            }
        }
        let offered: BTreeSet<&str> = entry
            .question
            .options
            .iter()
            .map(|option| option.id.as_str())
            .collect();
        let mut selected: Vec<String> = Vec::new();
        for id in &command.selected_option_ids {
            if !offered.contains(id.as_str()) {
                return Err(validation(
                    "selectedOptionIds",
                    "unknown-option",
                    format!(
                        "option {id} is not offered by question {}",
                        entry.question.id
                    ),
                ));
            }
            if !selected.contains(id) {
                selected.push(id.clone());
            }
        }
        if !entry.question.multi_select && selected.len() > 1 {
            return Err(validation(
                "selectedOptionIds",
                "single-select",
                "this question takes one option",
            ));
        }
        let text = command.text.trim().to_string();
        if text.chars().count() > MAX_VOTE_TEXT_CHARS {
            return Err(validation(
                "text",
                "text-too-long",
                format!("a vote's text is at most {MAX_VOTE_TEXT_CHARS} characters"),
            ));
        }
        if selected.is_empty() && text.is_empty() {
            return Err(validation(
                "selectedOptionIds",
                "empty-vote",
                "pick an option or write something",
            ));
        }
        let org_id = resolve_org_id_for_workspace(db, workspace_id).await?;
        // Hold the voter's audience grant until the vote commits
        // (`g5.security.current-authority`): a membership, activity or lifecycle
        // change that would withdraw it waits behind this write instead of
        // landing between admission and commit.
        ensure_user_workspace_access_until_commit(
            self.system_pool,
            db,
            voter_id,
            org_id,
            workspace_id,
        )
        .await?;
        let revision = i64::try_from(entry.question.revision).unwrap_or(i64::MAX);
        let row = sqlx::query_as::<_, VoteRow>(
            "INSERT INTO question_votes \
               (org_id, workspace_id, set_path, question_id, user_id, selected_option_ids, text, question_revision) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (workspace_id, set_path, question_id, user_id) DO UPDATE SET \
               selected_option_ids = EXCLUDED.selected_option_ids, text = EXCLUDED.text, \
               question_revision = EXCLUDED.question_revision, updated_at = now() \
             RETURNING question_id, user_id, selected_option_ids, text, question_revision, updated_at",
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(&path)
        .bind(&entry.question.id)
        .bind(voter_id)
        .bind(&selected)
        .bind(&text)
        .bind(revision)
        .fetch_one(db.conn())
        .await
        .map_err(|error| AppError::from_error(G5Error::from(error)))?;
        let voters = UserService::load_summaries(self.system_pool, [voter_id]).await?;
        self.publish_after_commit(db, workspace_id, &path, &entry.question.id);
        tracing::info!(
            target: "g5.audit.question_vote",
            %workspace_id, %voter_id, path = %path, question_id = %entry.question.id,
            options = ?selected, "question vote cast"
        );
        Ok(row.into_dto(voters.get(&voter_id)))
    }

    /// Withdraw `voter`'s vote; withdrawing an absent vote is a no-op that
    /// reports `withdrawn: false`.
    pub async fn withdraw(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        voter_id: Uuid,
        command: WithdrawCommand,
    ) -> Result<WithdrawVoteOutcome, AppError> {
        let path = valid_path(&command.path)?;
        let question_id = command.question_id;
        let org_id = resolve_org_id_for_workspace(db, workspace_id).await?;
        ensure_user_workspace_access_until_commit(
            self.system_pool,
            db,
            voter_id,
            org_id,
            workspace_id,
        )
        .await?;
        let deleted = sqlx::query(
            "DELETE FROM question_votes \
             WHERE workspace_id = $1 AND set_path = $2 AND question_id = $3 AND user_id = $4",
        )
        .bind(workspace_id)
        .bind(&path)
        .bind(&question_id)
        .bind(voter_id)
        .execute(db.conn())
        .await
        .map_err(|error| AppError::from_error(G5Error::from(error)))?
        .rows_affected()
            > 0;
        if deleted {
            self.publish_after_commit(db, workspace_id, &path, &question_id);
            tracing::info!(
                target: "g5.audit.question_vote",
                %workspace_id, %voter_id, path = %path, question_id = %question_id,
                "question vote withdrawn"
            );
        }
        Ok(WithdrawVoteOutcome {
            path,
            question_id,
            withdrawn: deleted,
        })
    }

    fn publish_after_commit(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        path: &str,
        question_id: &str,
    ) {
        let event_bus = self.event_bus.clone();
        let event = WsEvent::QuestionVote {
            workspace_id,
            path: path.to_string(),
            question_id: question_id.to_string(),
        };
        db.on_commit(move || {
            event_bus.publish(&Channel::Workspace(workspace_id), &event);
        });
    }
}
