//! The one typed writer of question set files
//! (`g5.architecture.question_owner`, `g5.documents.question.writer`).
//!
//! It composes existing operations and holds no state: the captured docs cut
//! (committed reads), the attached owner's live file over that cut, the
//! carried contract module (parse, gates, canonical bytes), the
//! `WorkspaceRuntime` file write under the file-version and VM-instance
//! preconditions, and — for a person's answer — the Workspace owner's
//! selected-file checkpoint (`workspace_file_save`). No table, no key, no
//! cache, no event.
//!
//! Provenance always comes from the authenticated caller ([`QuestionActor`]);
//! a person's answer is an explicit Save that publishes the set file
//! (`g5.documents.question.answer-checkpoint`), an agent's command changes
//! the live file only and lands an answer as a proposal
//! (`g5.documents.question.agent-access`).

use crate::services::git::repo_store::ScopedRepoStorage;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::doc_service::{CapturedDocsList, DocService};
use crate::services::workspace_file_save::{self, SaveOutcome};
use crate::services::workspace_runtime::{WorkspaceRuntime, WorkspaceRuntimeHandle};
use g5_contracts::error::G5Error;
use g5_contracts::types::question_set::{
    answer_receipt, apply_answer_transition, apply_clear_transition, generate_option_id,
    generate_question_id, is_decision, parse_question_set, parse_question_set_path, parse_set_id,
    question_set_dir, question_set_path, render_file, replay_check, revision_gate, AnswerProposal,
    Provenance, Question, QuestionOption, QuestionSetFile, QuestionSetParseError, QuestionSeverity,
    QuestionStatus, ReceiptOperation, ReplayCheck, RevisionOutcome, SetMeta, QUESTION_SET_EXT,
};

/// Tracing target of the question audit events.
pub const AUDIT_TARGET: &str = "g5.audit.question_set";
/// Longest idempotency key a command may carry.
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 160;
/// Attempts of one command when a concurrent write moved the file's hash
/// under an unchanged question revision.
const HASH_RACE_ATTEMPTS: usize = 3;
/// Most sets one list or metrics scan reads.
const MAX_SETS_PER_SCAN: usize = 256;

// ---------------------------------------------------------------------------
// Callers, reads and commands
// ---------------------------------------------------------------------------

/// The authenticated caller of a command. The route chooses the variant from
/// its credential; nothing in a request body can change it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionActor {
    /// The signed-in workspace owner, through the public route.
    Owner { user_id: Uuid },
    /// The owner's agent in the workspace VM, through the incarnation route.
    Agent { owner_user_id: Uuid, vm_id: Uuid },
}

impl QuestionActor {
    fn owner_user_id(&self) -> Uuid {
        match self {
            Self::Owner { user_id } => *user_id,
            Self::Agent { owner_user_id, .. } => *owner_user_id,
        }
    }

    fn is_person(&self) -> bool {
        matches!(self, Self::Owner { .. })
    }
}

/// Where a read's bytes came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadSource {
    /// The attached owner's running checkout.
    Live,
    /// The committed docs cut.
    Committed,
}

/// One question of a set as every consumer sees it: the entry, the decision
/// flag and the 1-based display number from set order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionEntry {
    #[serde(flatten)]
    pub question: Question,
    pub decision: bool,
    pub display_number: u32,
}

impl QuestionEntry {
    fn new(question: &Question, index: usize) -> Self {
        Self {
            decision: is_decision(question),
            display_number: u32::try_from(index + 1).unwrap_or(u32::MAX),
            question: question.clone(),
        }
    }
}

/// A set file that did not parse: shown as a plain notice, never actionable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSetInvalid {
    pub code: String,
    pub message: String,
}

/// One set as a reader sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSetView {
    pub path: String,
    pub set_id: String,
    pub source: ReadSource,
    /// Live blob hash (`source: live` only); a write is checked against it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_id: Option<Uuid>,
    /// True only for the attached owner reading the live file.
    pub answerable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub questions: Vec<QuestionEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid: Option<QuestionSetInvalid>,
}

/// An answer or clear command. The provenance is never part of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerCommand {
    pub path: String,
    pub question_id: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    /// `None` clears the answer.
    pub proposal: Option<AnswerProposal>,
    /// A person's retry of an answer that landed but did not publish: the
    /// blob hash the applied write produced.
    pub applied_hash: Option<String>,
}

/// Whether the set file holding an answer is published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum Publication {
    /// Committed and pushed: readers and agents see it.
    Published,
    /// Written to the live file; its commit or push failed. Retrying the
    /// same command with `appliedHash` publishes exactly that version.
    NotPublished { message: String },
    /// An agent's live write; the agent's own push publishes it.
    Live,
}

/// Result of `answer`/`clear`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerOutcome {
    pub path: String,
    pub entry: QuestionEntry,
    pub replayed: bool,
    /// Blob hash of the file holding this entry.
    pub hash: String,
    pub vm_id: Uuid,
    pub publication: Publication,
}

/// One option of a new or edited question.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewOption {
    pub id: Option<String>,
    pub label: String,
    pub description: String,
    pub tradeoffs: Option<String>,
    pub risks: Option<String>,
    pub recommended: Option<bool>,
}

/// One question to create, or — when `id` names an existing entry — to edit
/// under the revision gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewQuestion {
    pub id: Option<String>,
    pub prompt: String,
    pub context: Option<String>,
    pub severity: Option<QuestionSeverity>,
    pub multi_select: bool,
    pub allow_free_text: bool,
    pub options: Vec<NewOption>,
}

/// The create/append command: a set in the current workspace's questions
/// directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSetCommand {
    pub set_id: String,
    pub title: Option<String>,
    pub questions: Vec<NewQuestion>,
}

/// Result of `create_or_append`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSetOutcome {
    pub set: QuestionSetView,
    pub created: bool,
    pub changed: bool,
    /// The ids of every question the command named or generated, in order.
    pub question_ids: Vec<String>,
    pub publication: Publication,
}

/// One `open` or `proposed` entry of a set, for metrics and Home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionSetOpenQuestion {
    pub question_id: String,
    pub display_number: u32,
    pub title: String,
    pub severity: Option<String>,
    pub status: String,
}

/// One set the metrics scan read (`g5.documents.question.metrics`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionSetOpenSource {
    pub path: String,
    pub set_id: String,
    pub title: String,
    pub open_question_count: i64,
    pub open_questions: Vec<QuestionSetOpenQuestion>,
    /// Stable doc ids of the Project Docs whose references name an entry of
    /// this set, in path order.
    pub referenced_by: Vec<String>,
    /// The set did not parse; it counts nothing and marks the summary
    /// incomplete.
    pub invalid: bool,
}

/// A reference a Project Doc holds: the set path and question id it names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct QuestionReference {
    pub path: String,
    pub question_id: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn validation(field: &str, code: &str, message: impl Into<String>) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: json!({"field": field, "error": code, "message": message.into()}),
    })
}

fn not_found(resource: String) -> AppError {
    AppError::from_error(G5Error::NotFound { resource })
}

fn question_conflict(details: Value) -> AppError {
    AppError::from_error(G5Error::QuestionConflict { details })
}

fn unparseable(path: &str, error: &QuestionSetParseError) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: json!({
            "field": "content",
            "error": "question-set-unparseable",
            "code": error.code(),
            "path": path,
            "message": format!("{path} is not a valid question set: {error}"),
        }),
    })
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// A raw read of one set file.
struct RawRead {
    source: ReadSource,
    bytes: Option<Vec<u8>>,
    hash: Option<String>,
    vm_id: Option<Uuid>,
}

/// The question set service over one request's owners. Cheap: it borrows.
pub struct QuestionSetService<'a> {
    docs: &'a DocService,
    runtime: &'a WorkspaceRuntime,
}

impl<'a> QuestionSetService<'a> {
    #[must_use]
    pub fn new(docs: &'a DocService, runtime: &'a WorkspaceRuntime) -> Self {
        Self { docs, runtime }
    }

    // -- Schema gate for the owner file routes ------------------------------

    /// The owner file routes' check before landing bytes: a set file path
    /// must carry a parseable UTF-8 set whose id matches its file name. Every
    /// other path passes.
    pub fn check_file_write(path: &str, content: &str, encoding: &str) -> Result<(), AppError> {
        let Some(set_path) = parse_question_set_path(path) else {
            return Ok(());
        };
        if encoding != "utf-8" {
            return Err(validation(
                "encoding",
                "question-set-unparseable",
                "a question set file is UTF-8 JSON",
            ));
        }
        let file =
            parse_question_set(content.as_bytes()).map_err(|error| unparseable(path, &error))?;
        if file.set.id != set_path.set_id {
            return Err(validation(
                "content",
                "question-set-unparseable",
                format!(
                    "{path} names set {:?}; its file name requires {:?}",
                    file.set.id, set_path.set_id
                ),
            ));
        }
        Ok(())
    }

    /// The attached owner's provenance: the user row's email and avatar.
    pub async fn user_provenance(pool: &PgPool, user_id: Uuid) -> Result<Provenance, AppError> {
        let row: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT email, avatar_url FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(pool)
                .await
                .map_err(|error| AppError::from_error(G5Error::from(error)))?;
        let (email, avatar_url) = row.unwrap_or((None, None));
        Ok(Provenance {
            email,
            avatar_url,
            ..Provenance::user(user_id.to_string())
        })
    }

    fn provenance(actor: &QuestionActor, user: Option<Provenance>) -> Provenance {
        match actor {
            QuestionActor::Owner { user_id } => {
                user.unwrap_or_else(|| Provenance::user(user_id.to_string()))
            }
            QuestionActor::Agent { owner_user_id, .. } => Provenance {
                via: Some("g5d".to_string()),
                ..Provenance::agent(owner_user_id.to_string())
            },
        }
    }

    // -- Reads ----------------------------------------------------------------

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

    /// The running attachment when `viewer` is its owner. Never wakes a
    /// stopped workspace and never serves a reader.
    async fn attached_owner(
        &self,
        workspace_id: Uuid,
        viewer: Option<Uuid>,
    ) -> Option<WorkspaceRuntimeHandle> {
        let viewer = viewer?;
        let handle = self.runtime.get(workspace_id).await.ok()??;
        (handle.status == "running" && handle.owner_user_id == viewer && !handle.pending_transfer)
            .then_some(handle)
    }

    async fn read_raw(
        &self,
        captured: &CapturedDocsList,
        live: Option<&WorkspaceRuntimeHandle>,
        path: &str,
    ) -> Result<RawRead, AppError> {
        if let (Some(handle), Some(repo_id)) = (live, captured.docs_repo_id()) {
            return match workspace_file_save::read_live_file(
                self.runtime,
                handle,
                handle.owner_user_id,
                repo_id,
                path,
            )
            .await
            {
                Ok(file) => Ok(RawRead {
                    source: ReadSource::Live,
                    bytes: Some(if file.encoding == "utf-8" {
                        file.content.into_bytes()
                    } else {
                        // Base64 bytes are not a set file; they fail to parse.
                        Vec::from(&b"\0"[..])
                    }),
                    hash: Some(file.hash),
                    vm_id: Some(handle.vm_id),
                }),
                Err(error) if matches!(error.inner, G5Error::NotFound { .. }) => Ok(RawRead {
                    source: ReadSource::Live,
                    bytes: None,
                    hash: None,
                    vm_id: Some(handle.vm_id),
                }),
                Err(error) => Err(error),
            };
        }
        let mut committed = self
            .docs
            .read_committed_docs_files(captured, &[path.to_string()])
            .await?;
        Ok(RawRead {
            source: ReadSource::Committed,
            bytes: committed.remove(path).flatten(),
            hash: None,
            vm_id: None,
        })
    }

    fn build_view(path: &str, raw: RawRead, answerable: bool) -> Option<QuestionSetView> {
        let bytes = raw.bytes?;
        let set_id = parse_question_set_path(path)
            .map(|parsed| parsed.set_id)
            .unwrap_or_default();
        let parsed = parse_question_set(&bytes).and_then(|file| {
            if file.set.id == set_id {
                Ok(file)
            } else {
                Err(QuestionSetParseError::InvalidSetId {
                    set_id: file.set.id,
                })
            }
        });
        let mut view = QuestionSetView {
            path: path.to_string(),
            set_id,
            source: raw.source,
            hash: raw.hash,
            vm_id: raw.vm_id,
            answerable: false,
            title: None,
            questions: Vec::new(),
            invalid: None,
        };
        match parsed {
            Ok(file) => {
                view.answerable = answerable;
                view.title = Some(file.set.title.clone());
                view.questions = file
                    .questions
                    .iter()
                    .enumerate()
                    .map(|(index, question)| QuestionEntry::new(question, index))
                    .collect();
            }
            Err(error) => {
                view.invalid = Some(QuestionSetInvalid {
                    code: error.code().to_string(),
                    message: error.to_string(),
                });
            }
        }
        Some(view)
    }

    /// One set at `path`, from the attached owner's live checkout or else the
    /// committed docs cut. The path is resolved in the current workspace's
    /// docs member as given; it is never derived from the workspace id.
    pub async fn get_set(
        &self,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        viewer: Option<Uuid>,
        path: &str,
    ) -> Result<QuestionSetView, AppError> {
        let path = Self::valid_path(path)?;
        let live = self.attached_owner(workspace_id, viewer).await;
        let raw = self.read_raw(captured, live.as_ref(), &path).await?;
        let answerable = live.is_some();
        Self::build_view(&path, raw, answerable)
            .ok_or_else(|| not_found(format!("question set '{path}'")))
    }

    /// Set paths of the current workspace's questions directory: the
    /// committed ones, merged with the attached owner's live listing.
    async fn own_set_paths(
        &self,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        live: Option<&WorkspaceRuntimeHandle>,
    ) -> Result<BTreeSet<String>, AppError> {
        let dir = question_set_dir(&workspace_id.to_string());
        let is_set = |path: &str| {
            path.ends_with(QUESTION_SET_EXT) && parse_question_set_path(path).is_some()
        };
        let mut paths: BTreeSet<String> = self
            .docs
            .list_committed_docs_files(captured, &dir)
            .await?
            .into_iter()
            .filter(|path| is_set(path))
            .collect();
        if let (Some(handle), Some(repo_id)) = (live, captured.docs_repo_id()) {
            let request =
                workspace_file_save::file_runtime_request(handle, handle.owner_user_id, "list");
            match self
                .runtime
                .file_list(handle, &request, repo_id, &dir)
                .await
            {
                Ok(listed) => {
                    paths = listed["entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|entry| entry["type"].as_str() == Some("file"))
                        .filter_map(|entry| entry["path"].as_str())
                        .filter(|path| is_set(path))
                        .map(str::to_string)
                        .collect();
                }
                Err(error) if error.message == "FILE_MISSING" => paths.clear(),
                Err(error) => {
                    tracing::warn!(%workspace_id, error = %error.message, "question sets: live listing failed; serving the committed cut");
                }
            }
        }
        Ok(paths)
    }

    /// Every set in the current workspace's questions directory. A corrupt
    /// file is listed with its notice and never hides the others.
    pub async fn list_sets(
        &self,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        viewer: Option<Uuid>,
    ) -> Result<Vec<QuestionSetView>, AppError> {
        let live = self.attached_owner(workspace_id, viewer).await;
        let paths = self
            .own_set_paths(captured, workspace_id, live.as_ref())
            .await?;
        let mut views = Vec::new();
        for path in paths.into_iter().take(MAX_SETS_PER_SCAN) {
            let raw = self.read_raw(captured, live.as_ref(), &path).await?;
            if let Some(view) = Self::build_view(&path, raw, live.is_some()) {
                views.push(view);
            }
        }
        Ok(views)
    }

    // -- Metrics ---------------------------------------------------------------

    /// The question-set family of open-question metrics
    /// (`g5.documents.question.metrics`): the current workspace's sets plus
    /// the inherited sets its documents reference, each `open` or `proposed`
    /// entry counted once. `references` maps every reference the scanned
    /// documents hold to the stable doc ids holding it.
    pub async fn open_question_set_sources(
        &self,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        viewer: Option<Uuid>,
        references: &BTreeMap<QuestionReference, Vec<String>>,
    ) -> Result<Vec<QuestionSetOpenSource>, AppError> {
        let live = self.attached_owner(workspace_id, viewer).await;
        let mut paths = self
            .own_set_paths(captured, workspace_id, live.as_ref())
            .await?;
        paths.extend(references.keys().map(|reference| reference.path.clone()));
        let mut sources = Vec::new();
        for path in paths.into_iter().take(MAX_SETS_PER_SCAN) {
            let raw = match self.read_raw(captured, live.as_ref(), &path).await {
                Ok(raw) => raw,
                Err(error) => {
                    tracing::warn!(%workspace_id, path, %error, "question sets: read failed; summary is incomplete");
                    sources.push(QuestionSetOpenSource {
                        set_id: String::new(),
                        title: String::new(),
                        open_question_count: 0,
                        open_questions: Vec::new(),
                        referenced_by: Vec::new(),
                        invalid: true,
                        path,
                    });
                    continue;
                }
            };
            let Some(view) = Self::build_view(&path, raw, false) else {
                continue;
            };
            let mut referenced_by = references
                .iter()
                .filter(|(reference, _)| reference.path == path)
                .flat_map(|(_, docs)| docs.iter().cloned())
                .collect::<Vec<_>>();
            referenced_by.sort();
            referenced_by.dedup();
            let open_questions = view
                .questions
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.question.status,
                        QuestionStatus::Open | QuestionStatus::Proposed
                    )
                })
                .map(|entry| QuestionSetOpenQuestion {
                    question_id: entry.question.id.clone(),
                    display_number: entry.display_number,
                    title: entry_title(&entry.question),
                    severity: entry
                        .question
                        .severity
                        .map(|severity| severity.as_str().to_string()),
                    status: entry.question.status.as_str().to_string(),
                })
                .collect::<Vec<_>>();
            let invalid = view.invalid.is_some();
            if !invalid && open_questions.is_empty() {
                continue;
            }
            sources.push(QuestionSetOpenSource {
                path,
                set_id: view.set_id,
                title: view.title.unwrap_or_default(),
                open_question_count: open_questions.len() as i64,
                open_questions,
                referenced_by,
                invalid,
            });
        }
        Ok(sources)
    }

    // -- Answer and clear ----------------------------------------------------

    fn valid_key(key: &str) -> Result<(), AppError> {
        if key.trim().is_empty() || key.len() > MAX_IDEMPOTENCY_KEY_LEN {
            return Err(validation(
                "idempotencyKey",
                "validation",
                format!("an idempotency key is 1..={MAX_IDEMPOTENCY_KEY_LEN} bytes"),
            ));
        }
        Ok(())
    }

    /// The owner's attached incarnation for `actor`; an agent's must be the
    /// incarnation its token names.
    async fn writable_handle(
        &self,
        workspace_id: Uuid,
        actor: &QuestionActor,
    ) -> Result<WorkspaceRuntimeHandle, AppError> {
        let handle = self
            .runtime
            .owner_handle(workspace_id, actor.owner_user_id())
            .await?;
        if let QuestionActor::Agent { vm_id, .. } = actor {
            if *vm_id != handle.vm_id {
                return Err(AppError::from_error(G5Error::Forbidden {
                    missing_scopes: Vec::new(),
                }));
            }
        }
        Ok(handle)
    }

    async fn read_live_set(
        &self,
        handle: &WorkspaceRuntimeHandle,
        repo_id: Uuid,
        path: &str,
    ) -> Result<(QuestionSetFile, String), AppError> {
        let live = workspace_file_save::read_live_file(
            self.runtime,
            handle,
            handle.owner_user_id,
            repo_id,
            path,
        )
        .await
        .map_err(|error| match error.inner {
            G5Error::NotFound { .. } => not_found(format!("question set '{path}'")),
            _ => error,
        })?;
        if live.encoding != "utf-8" {
            return Err(validation(
                "content",
                "question-set-unparseable",
                format!("{path} is not UTF-8"),
            ));
        }
        let file = parse_question_set(live.content.as_bytes())
            .map_err(|error| unparseable(path, &error))?;
        let expected_set = parse_question_set_path(path)
            .map(|parsed| parsed.set_id)
            .unwrap_or_default();
        if file.set.id != expected_set {
            return Err(unparseable(
                path,
                &QuestionSetParseError::InvalidSetId {
                    set_id: file.set.id,
                },
            ));
        }
        Ok((file, live.hash))
    }

    /// Answer (or, with no proposal, clear) one referenced question.
    ///
    /// Reads the live file over the committed cut, checks the replay receipt
    /// and the revision token, applies the transition through the shape
    /// gate and writes the canonical whole file under the observed hash and
    /// VM. A person's command then checkpoints that one file; it reports
    /// `published` only after the push, and a landed write whose
    /// publication failed as `not-published`, retryable for exactly that
    /// version (`applied_hash`) without a second transition. An agent's
    /// command stops at the live write.
    pub async fn answer(
        &self,
        pool: &PgPool,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        actor: &QuestionActor,
        command: AnswerCommand,
    ) -> Result<AnswerOutcome, AppError> {
        let result = self
            .answer_inner(
                pool,
                captured.repos(),
                captured,
                workspace_id,
                actor,
                &command,
            )
            .await;
        let operation = if command.proposal.is_some() {
            "answer"
        } else {
            "clear"
        };
        match &result {
            Ok(outcome) => tracing::info!(
                target: AUDIT_TARGET,
                %workspace_id,
                path = %command.path,
                question_id = %command.question_id,
                operation,
                actor = ?actor,
                revision = outcome.entry.question.revision,
                replayed = outcome.replayed,
                publication = ?outcome.publication,
                "question set command applied"
            ),
            Err(error) => tracing::info!(
                target: AUDIT_TARGET,
                %workspace_id,
                path = %command.path,
                question_id = %command.question_id,
                operation,
                actor = ?actor,
                code = error.inner.error_code(),
                "question set command refused"
            ),
        }
        result
    }

    async fn answer_inner(
        &self,
        pool: &PgPool,
        scope: &ScopedRepoStorage,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        actor: &QuestionActor,
        command: &AnswerCommand,
    ) -> Result<AnswerOutcome, AppError> {
        let path = Self::valid_path(&command.path)?;
        Self::valid_key(&command.idempotency_key)?;
        let repo_id = captured.docs_repo_id().ok_or_else(|| {
            validation("path", "validation", "the workspace has no docs repository")
        })?;
        let user = if actor.is_person() {
            Some(Self::user_provenance(pool, actor.owner_user_id()).await?)
        } else {
            None
        };
        let provenance = Self::provenance(actor, user);
        let operation = if command.proposal.is_some() {
            ReceiptOperation::Answer
        } else {
            ReceiptOperation::Clear
        };
        let receipt = answer_receipt(
            &command.idempotency_key,
            &provenance,
            operation,
            command.expected_revision,
            command.proposal.as_ref(),
        );
        let handle = self.writable_handle(workspace_id, actor).await?;
        for attempt in 1..=HASH_RACE_ATTEMPTS {
            let (mut file, hash) = self.read_live_set(&handle, repo_id, &path).await?;
            let index = file
                .questions
                .iter()
                .position(|question| question.id == command.question_id)
                .ok_or_else(|| {
                    not_found(format!(
                        "question '{}' in set '{path}'",
                        command.question_id
                    ))
                })?;
            let previous = file.questions[index].clone();
            match replay_check(&previous, &receipt) {
                ReplayCheck::Replay => {
                    return self
                        .replay(
                            captured,
                            workspace_id,
                            actor,
                            &handle,
                            repo_id,
                            &path,
                            &hash,
                            &previous,
                            index,
                            command,
                        )
                        .await;
                }
                ReplayCheck::KeyReused => {
                    return Err(question_conflict(json!({
                        "code": "idempotency-key-reused",
                        "path": path,
                        "questionId": command.question_id,
                        "message": "this idempotency key already recorded a different request",
                        "current": QuestionEntry::new(&previous, index),
                    })));
                }
                ReplayCheck::Fresh => {}
            }
            if command.expected_revision != previous.revision {
                return Err(question_conflict(json!({
                    "code": "revision-moved",
                    "path": path,
                    "questionId": command.question_id,
                    "expectedRevision": command.expected_revision,
                    "currentRevision": previous.revision,
                    "current": QuestionEntry::new(&previous, index),
                })));
            }
            let next = match &command.proposal {
                Some(proposal) => apply_answer_transition(
                    &previous,
                    proposal,
                    &provenance,
                    &now(),
                    receipt.clone(),
                ),
                None => apply_clear_transition(&previous, &provenance, receipt.clone()),
            }
            .map_err(|gate| validation("answer", gate.code(), gate.to_string()))?;
            file.questions[index] = next.clone();
            let rendered = render_file(&file);
            match self
                .land(
                    scope,
                    workspace_id,
                    actor,
                    &handle,
                    repo_id,
                    &path,
                    &rendered,
                    &hash,
                    &format!("Answer {} in {path}", command.question_id),
                )
                .await
            {
                Ok((hash, publication)) => {
                    return Ok(AnswerOutcome {
                        path,
                        entry: QuestionEntry::new(&next, index),
                        replayed: false,
                        hash,
                        vm_id: handle.vm_id,
                        publication,
                    })
                }
                Err(error)
                    if matches!(error.inner, G5Error::FileVersionConflict { .. })
                        && attempt < HASH_RACE_ATTEMPTS =>
                {
                    // Another write changed the file under an unchanged
                    // question revision: read it again and re-check.
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Err(question_conflict(json!({
            "code": "reload-required",
            "path": path,
            "message": "the set file kept changing; reload it",
        })))
    }

    /// A replayed command: the entry already carries its receipt. A person's
    /// replay publishes the applied version when it is still exactly the
    /// live file; a set changed since then needs a reload.
    #[allow(clippy::too_many_arguments)]
    async fn replay(
        &self,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        actor: &QuestionActor,
        handle: &WorkspaceRuntimeHandle,
        repo_id: Uuid,
        path: &str,
        live_hash: &str,
        entry: &Question,
        index: usize,
        command: &AnswerCommand,
    ) -> Result<AnswerOutcome, AppError> {
        let outcome = |publication| AnswerOutcome {
            path: path.to_string(),
            entry: QuestionEntry::new(entry, index),
            replayed: true,
            hash: live_hash.to_string(),
            vm_id: handle.vm_id,
            publication,
        };
        if !actor.is_person() {
            return Ok(outcome(Publication::Live));
        }
        if let Some(applied) = command.applied_hash.as_deref() {
            if applied != live_hash {
                return Err(question_conflict(json!({
                    "code": "reload-required",
                    "path": path,
                    "questionId": command.question_id,
                    "message": "the set changed after this answer was applied; reload before publishing it",
                    "current": QuestionEntry::new(entry, index),
                })));
            }
            let publication = match workspace_file_save::publish_saved_file(
                self.runtime,
                workspace_id,
                handle.owner_user_id,
                handle.vm_id,
                repo_id,
                path,
                live_hash,
                &format!("Answer {} in {path}", command.question_id),
            )
            .await
            {
                Ok(()) => Publication::Published,
                Err(error) => Publication::NotPublished {
                    message: error.to_string(),
                },
            };
            return Ok(outcome(publication));
        }
        // No applied hash (a lost response): published when the committed
        // cut already holds this receipt, otherwise report the applied
        // version so the caller can publish exactly it.
        let committed = self
            .docs
            .read_committed_docs_files(captured, &[path.to_string()])
            .await?
            .remove(path)
            .flatten()
            .and_then(|bytes| parse_question_set(&bytes).ok())
            .and_then(|file| file.question(&entry.id).cloned());
        let published = committed.is_some_and(|committed| {
            committed.answer.receipt == entry.answer.receipt && committed.revision == entry.revision
        });
        Ok(outcome(if published {
            Publication::Published
        } else {
            Publication::NotPublished {
                message: "the answer is applied but not published; retry to publish it".to_string(),
            }
        }))
    }

    /// Write `rendered` over the file whose hash was `expected_hash`; a
    /// person's write is the explicit Save and checkpoints the file.
    #[allow(clippy::too_many_arguments)]
    async fn land(
        &self,
        scope: &ScopedRepoStorage,
        workspace_id: Uuid,
        actor: &QuestionActor,
        handle: &WorkspaceRuntimeHandle,
        repo_id: Uuid,
        path: &str,
        rendered: &str,
        expected_hash: &str,
        message: &str,
    ) -> Result<(String, Publication), AppError> {
        Self::check_file_write(path, rendered, "utf-8")?;
        let hash_of = |saved: &Value| {
            saved
                .get("hash")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        if actor.is_person() {
            let outcome = workspace_file_save::save_live_file(
                self.runtime,
                scope,
                workspace_id,
                handle.owner_user_id,
                repo_id,
                path,
                rendered,
                "utf-8",
                Some(expected_hash),
                handle.vm_id,
                message,
            )
            .await?;
            let hash = hash_of(outcome.saved());
            return Ok(match outcome {
                SaveOutcome::Published { .. } => (hash, Publication::Published),
                SaveOutcome::Applied { error, .. } => (
                    hash,
                    Publication::NotPublished {
                        message: error.to_string(),
                    },
                ),
            });
        }
        let saved = workspace_file_save::write_live_file(
            self.runtime,
            scope,
            handle,
            handle.owner_user_id,
            repo_id,
            path,
            rendered,
            "utf-8",
            Some(expected_hash),
        )
        .await?;
        Ok((hash_of(&saved), Publication::Live))
    }

    // -- Create and append ---------------------------------------------------

    fn validate_new_questions(questions: &[NewQuestion]) -> Result<(), AppError> {
        if questions.is_empty() {
            return Err(validation(
                "questions",
                "validation",
                "name at least one question",
            ));
        }
        for (index, question) in questions.iter().enumerate() {
            if question.prompt.trim().is_empty() {
                return Err(validation(
                    "questions",
                    "validation",
                    format!("question {} needs a prompt", index + 1),
                ));
            }
            if question
                .id
                .as_deref()
                .is_some_and(|id| !g5_contracts::types::question_set::is_valid_question_id(id))
            {
                return Err(validation(
                    "questions",
                    "validation",
                    format!("question {} has an invalid id", index + 1),
                ));
            }
            if question
                .options
                .iter()
                .filter(|option| option.recommended == Some(true))
                .count()
                > 1
            {
                return Err(validation(
                    "questions",
                    "validation",
                    format!("question {} recommends more than one option", index + 1),
                ));
            }
            if question
                .options
                .iter()
                .any(|option| option.label.trim().is_empty())
            {
                return Err(validation(
                    "questions",
                    "validation",
                    format!("question {} has an option without a label", index + 1),
                ));
            }
            if question.options.is_empty() && !question.allow_free_text {
                return Err(validation(
                    "questions",
                    "validation",
                    format!(
                        "question {} has no options and refuses free text",
                        index + 1
                    ),
                ));
            }
        }
        Ok(())
    }

    fn build_options(options: &[NewOption], previous: &[QuestionOption]) -> Vec<QuestionOption> {
        let mut taken = BTreeSet::new();
        options
            .iter()
            .map(|option| {
                let id = option
                    .id
                    .clone()
                    .or_else(|| {
                        previous
                            .iter()
                            .find(|existing| existing.label == option.label)
                            .map(|existing| existing.id.clone())
                    })
                    .unwrap_or_else(|| generate_option_id(&option.label, &taken));
                taken.insert(id.clone());
                QuestionOption {
                    id,
                    label: option.label.clone(),
                    description: option.description.clone(),
                    tradeoffs: option.tradeoffs.clone(),
                    risks: option.risks.clone(),
                    recommended: option.recommended,
                    extra: previous
                        .iter()
                        .find(|existing| existing.label == option.label)
                        .map(|existing| existing.extra.clone())
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    fn authored(base: &Question, new_question: &NewQuestion) -> Question {
        let mut next = base.clone();
        next.prompt = new_question.prompt.clone();
        next.context = new_question.context.clone();
        next.severity = new_question.severity;
        next.multi_select = new_question.multi_select;
        next.allow_free_text = new_question.allow_free_text;
        next.options = Self::build_options(&new_question.options, &base.options);
        next
    }

    /// Create the set `docs/<workspace>/questions/<set>.g5q.json` when absent
    /// or append to it; a question naming an existing id is an authoring edit
    /// under the revision gate (wording keeps the answer as a new revision,
    /// option changes on a decision need a new question). A create never
    /// carries an answer. A person's create checkpoints the file; an agent's
    /// stays live.
    pub async fn create_or_append(
        &self,
        pool: &PgPool,
        captured: &CapturedDocsList,
        workspace_id: Uuid,
        actor: &QuestionActor,
        command: CreateSetCommand,
    ) -> Result<CreateSetOutcome, AppError> {
        let scope = captured.repos();
        let set_id = parse_set_id(&command.set_id).map_err(|_| {
            validation(
                "setId",
                "invalid-set-id",
                "a set id matches ^[a-z0-9][a-z0-9-]{0,63}$",
            )
        })?;
        Self::validate_new_questions(&command.questions)?;
        let repo_id = captured.docs_repo_id().ok_or_else(|| {
            validation(
                "setId",
                "validation",
                "the workspace has no docs repository",
            )
        })?;
        let user = if actor.is_person() {
            Some(Self::user_provenance(pool, actor.owner_user_id()).await?)
        } else {
            None
        };
        let provenance = Self::provenance(actor, user);
        let path = question_set_path(&workspace_id.to_string(), &set_id);
        let handle = self.writable_handle(workspace_id, actor).await?;
        let title = command
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string);
        for attempt in 1..=HASH_RACE_ATTEMPTS {
            let (mut file, expected_hash, created) =
                match self.read_live_set(&handle, repo_id, &path).await {
                    Ok((file, hash)) => (file, Some(hash), false),
                    Err(error) if matches!(error.inner, G5Error::NotFound { .. }) => (
                        QuestionSetFile::new(SetMeta {
                            id: set_id.clone(),
                            title: title.clone().unwrap_or_else(|| set_id.clone()),
                            created_at: now(),
                            created_by: Some(provenance.clone()),
                            extra: serde_json::Map::new(),
                        }),
                        None,
                        true,
                    ),
                    Err(error) => return Err(error),
                };
            let mut changed = created;
            if let (false, Some(title)) = (created, title.as_deref()) {
                if file.set.title != title {
                    file.set.title = title.to_string();
                    changed = true;
                }
            }
            let mut question_ids = Vec::with_capacity(command.questions.len());
            for new_question in &command.questions {
                let existing = new_question
                    .id
                    .as_deref()
                    .and_then(|id| file.questions.iter().position(|question| question.id == id));
                match existing {
                    Some(index) => {
                        let previous = file.questions[index].clone();
                        let candidate = Self::authored(&previous, new_question);
                        match revision_gate(&previous, &candidate).map_err(|gate| {
                            validation("questions", gate.code(), gate.to_string())
                        })? {
                            RevisionOutcome::Unchanged => {}
                            RevisionOutcome::NewRevision => {
                                let mut next = candidate;
                                next.revision = previous.revision.saturating_add(1);
                                file.questions[index] = next;
                                changed = true;
                            }
                        }
                        question_ids.push(previous.id);
                    }
                    None => {
                        let id = new_question.id.clone().unwrap_or_else(|| {
                            let mut nonce = 0u32;
                            loop {
                                let candidate = generate_question_id(
                                    &new_question.prompt,
                                    format!("{set_id}:{}:{nonce}", file.questions.len()).as_bytes(),
                                );
                                if file.question(&candidate).is_none() {
                                    break candidate;
                                }
                                nonce += 1;
                            }
                        });
                        let base = Question::new_open(id.clone(), new_question.prompt.clone());
                        file.questions.push(Self::authored(&base, new_question));
                        question_ids.push(id);
                        changed = true;
                    }
                }
            }
            if !changed {
                let raw = RawRead {
                    source: ReadSource::Live,
                    bytes: Some(render_file(&file).into_bytes()),
                    hash: expected_hash,
                    vm_id: Some(handle.vm_id),
                };
                return Ok(CreateSetOutcome {
                    set: Self::build_view(&path, raw, actor.is_person())
                        .ok_or_else(|| not_found(format!("question set '{path}'")))?,
                    created: false,
                    changed: false,
                    question_ids,
                    publication: Publication::Live,
                });
            }
            let rendered = render_file(&file);
            let landed = match expected_hash.as_deref() {
                Some(hash) => {
                    self.land(
                        scope,
                        workspace_id,
                        actor,
                        &handle,
                        repo_id,
                        &path,
                        &rendered,
                        hash,
                        &format!("Questions {path}"),
                    )
                    .await
                }
                None => {
                    self.land_new(
                        scope,
                        workspace_id,
                        actor,
                        &handle,
                        repo_id,
                        &path,
                        &rendered,
                    )
                    .await
                }
            };
            match landed {
                Ok((hash, publication)) => {
                    let raw = RawRead {
                        source: ReadSource::Live,
                        bytes: Some(rendered.into_bytes()),
                        hash: Some(hash),
                        vm_id: Some(handle.vm_id),
                    };
                    tracing::info!(
                        target: AUDIT_TARGET,
                        %workspace_id,
                        path,
                        actor = ?actor,
                        created,
                        "question set authored"
                    );
                    return Ok(CreateSetOutcome {
                        set: Self::build_view(&path, raw, actor.is_person())
                            .ok_or_else(|| not_found(format!("question set '{path}'")))?,
                        created,
                        changed: true,
                        question_ids,
                        publication,
                    });
                }
                Err(error)
                    if matches!(
                        error.inner,
                        G5Error::FileVersionConflict { .. } | G5Error::FileExists { .. }
                    ) && attempt < HASH_RACE_ATTEMPTS =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Err(question_conflict(json!({
            "code": "reload-required",
            "path": path,
            "message": "the set file kept changing; retry",
        })))
    }

    /// Create a set file that must not exist yet.
    #[allow(clippy::too_many_arguments)]
    async fn land_new(
        &self,
        scope: &ScopedRepoStorage,
        workspace_id: Uuid,
        actor: &QuestionActor,
        handle: &WorkspaceRuntimeHandle,
        repo_id: Uuid,
        path: &str,
        rendered: &str,
    ) -> Result<(String, Publication), AppError> {
        Self::check_file_write(path, rendered, "utf-8")?;
        // `fs.write` with no expected hash creates; a concurrent creator makes
        // this a version conflict on the retry's read.
        let saved = workspace_file_save::write_live_file(
            self.runtime,
            scope,
            handle,
            handle.owner_user_id,
            repo_id,
            path,
            rendered,
            "utf-8",
            None,
        )
        .await?;
        let hash = saved
            .get("hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !actor.is_person() {
            return Ok((hash, Publication::Live));
        }
        let publication = match workspace_file_save::publish_saved_file(
            self.runtime,
            workspace_id,
            handle.owner_user_id,
            handle.vm_id,
            repo_id,
            path,
            &hash,
            &format!("Questions {path}"),
        )
        .await
        {
            Ok(()) => Publication::Published,
            Err(error) => Publication::NotPublished {
                message: error.to_string(),
            },
        };
        Ok((hash, publication))
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Bounded title of one set entry: the prompt's first non-empty line.
fn entry_title(question: &Question) -> String {
    question
        .prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(160).collect())
        .unwrap_or_else(|| question.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_bytes(set_id: &str) -> String {
        render_file(&QuestionSetFile::new(SetMeta {
            id: set_id.to_string(),
            title: "t".to_string(),
            created_at: "2026-10-09T00:00:00Z".to_string(),
            created_by: None,
            extra: serde_json::Map::new(),
        }))
    }

    #[test]
    fn file_write_gate_checks_only_set_paths() {
        let path = "docs/ws/questions/plan.g5q.json";
        assert!(QuestionSetService::check_file_write(path, &set_bytes("plan"), "utf-8").is_ok());
        let error = QuestionSetService::check_file_write(path, "{not json", "utf-8").unwrap_err();
        let G5Error::ValidationError { details } = &error.inner else {
            panic!("{error:?}");
        };
        assert_eq!(details["error"], "question-set-unparseable");
        assert_eq!(details["code"], "invalid-json");
        assert!(QuestionSetService::check_file_write(path, &set_bytes("other"), "utf-8").is_err());
        assert!(QuestionSetService::check_file_write(path, "e30=", "base64").is_err());
        // Other paths are never inspected.
        assert!(QuestionSetService::check_file_write("docs/ws/plan.md", "{", "utf-8").is_ok());
        assert!(
            QuestionSetService::check_file_write("docs/ws/questions/x.json", "{", "utf-8").is_ok()
        );
    }

    #[test]
    fn views_mark_unparseable_sets_without_hiding_them() {
        let raw = |bytes: &str| RawRead {
            source: ReadSource::Committed,
            bytes: Some(bytes.as_bytes().to_vec()),
            hash: None,
            vm_id: None,
        };
        let path = "docs/ws/questions/plan.g5q.json";
        let valid = QuestionSetService::build_view(path, raw(&set_bytes("plan")), true).unwrap();
        assert!(valid.invalid.is_none());
        assert!(valid.answerable);
        let broken = QuestionSetService::build_view(path, raw("{"), true).unwrap();
        assert_eq!(broken.invalid.as_ref().unwrap().code, "invalid-json");
        assert!(!broken.answerable);
        let renamed = QuestionSetService::build_view(path, raw(&set_bytes("other")), true).unwrap();
        assert_eq!(renamed.invalid.as_ref().unwrap().code, "invalid-set-id");
        let missing = RawRead {
            source: ReadSource::Committed,
            bytes: None,
            hash: None,
            vm_id: None,
        };
        assert!(QuestionSetService::build_view(path, missing, true).is_none());
    }

    #[test]
    fn new_questions_are_validated_before_any_read() {
        let question = |prompt: &str| NewQuestion {
            id: None,
            prompt: prompt.to_string(),
            context: None,
            severity: None,
            multi_select: false,
            allow_free_text: true,
            options: Vec::new(),
        };
        assert!(QuestionSetService::validate_new_questions(&[]).is_err());
        assert!(QuestionSetService::validate_new_questions(&[question(" ")]).is_err());
        assert!(QuestionSetService::validate_new_questions(&[question("Pick one?")]).is_ok());
        let mut closed = question("Pick one?");
        closed.allow_free_text = false;
        assert!(QuestionSetService::validate_new_questions(&[closed]).is_err());
        let mut two = question("Pick one?");
        two.options = vec![
            NewOption {
                label: "A".into(),
                recommended: Some(true),
                ..NewOption::default()
            },
            NewOption {
                label: "B".into(),
                recommended: Some(true),
                ..NewOption::default()
            },
        ];
        assert!(QuestionSetService::validate_new_questions(&[two]).is_err());
    }

    #[test]
    fn authored_options_keep_ids_by_label() {
        let mut base = Question::new_open("q", "p");
        base.options = QuestionSetService::build_options(
            &[
                NewOption {
                    label: "Alpha".into(),
                    ..NewOption::default()
                },
                NewOption {
                    label: "Beta".into(),
                    ..NewOption::default()
                },
            ],
            &[],
        );
        assert_eq!(base.options[0].id, "alpha");
        let edited = QuestionSetService::authored(
            &base,
            &NewQuestion {
                id: Some("q".into()),
                prompt: "p reworded".into(),
                context: None,
                severity: None,
                multi_select: false,
                allow_free_text: true,
                options: vec![
                    NewOption {
                        label: "Beta".into(),
                        description: "now described".into(),
                        ..NewOption::default()
                    },
                    NewOption {
                        label: "Alpha".into(),
                        ..NewOption::default()
                    },
                ],
            },
        );
        assert_eq!(edited.options[0].id, "beta");
        assert_eq!(edited.options[1].id, "alpha");
    }
}
