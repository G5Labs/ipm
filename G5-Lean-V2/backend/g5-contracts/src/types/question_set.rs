//! Question set file contract (`docs/<authoring-workspace-id>/questions/<set>.g5q.json`).
//!
//! The single Rust owner of the referenced-question set file
//! (`g5.documents.question.referenced`, `g5.documents.question.writer`): the
//! serde types, the canonical serialization, intrinsic id generation, the
//! answer shape gate, the authoring revision gate, the decision predicate,
//! the replay receipt and the path rules. Ported from G5 PR #2532's schema-2
//! module without its signature, keyring, classifier and outbox: Lean set
//! files are `schema: 3` and carry no `integrity` block (human decision,
//! 2026-10-09).
//!
//! The module performs no I/O and holds no state; every function is total
//! over its inputs and nothing here panics on untrusted bytes. The embedded
//! schema-1 `g5-question` YAML fence keeps its own owners; both shapes coexist
//! permanently.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Directory (inside `docs/<workspace_id>/`) that holds question set files.
pub const QUESTION_SET_DIR: &str = "questions";
/// File extension of a question set file.
pub const QUESTION_SET_EXT: &str = ".g5q.json";
/// Hard parse cap for one set file.
pub const MAX_QUESTION_SET_BYTES: usize = 1024 * 1024;
/// The only schema this module owns.
pub const QUESTION_SET_SCHEMA: u64 = 3;
/// Longest slug prefix an intrinsic id carries.
const MAX_SLUG_LEN: usize = 48;
/// Longest set id a file may carry.
const MAX_SET_ID_LEN: usize = 64;
/// Longest question id a reference or command may name.
const MAX_QUESTION_ID_LEN: usize = 128;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Lifecycle status of one question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionStatus {
    /// No answer recorded.
    Open,
    /// An agent filled the answer slot; nobody confirmed it.
    Proposed,
    /// An answer is recorded; `is_decision` tells whether a person gave it.
    Answered,
}

impl QuestionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Proposed => "proposed",
            Self::Answered => "answered",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "proposed" => Some(Self::Proposed),
            "answered" => Some(Self::Answered),
            _ => None,
        }
    }
}

impl fmt::Display for QuestionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Severity hint shown on the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl QuestionSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Ordering rank, higher is more severe.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

/// Who recorded an answer: a person or an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceKind {
    User,
    Agent,
}

impl ProvenanceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }
}

/// Identity of the actor that recorded an answer. The write service fills it
/// from the authenticated caller; a request body never supplies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub kind: ProvenanceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Free string naming the surface when it is not the ordinary card
    /// (`g5d` for the workspace CLI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Provenance {
    /// A plain user provenance.
    pub fn user(id: impl Into<String>) -> Self {
        Self {
            kind: ProvenanceKind::User,
            id: Some(id.into()),
            email: None,
            avatar_url: None,
            via: None,
            extra: Map::new(),
        }
    }

    /// A plain agent provenance.
    pub fn agent(id: impl Into<String>) -> Self {
        Self {
            kind: ProvenanceKind::Agent,
            id: Some(id.into()),
            email: None,
            avatar_url: None,
            via: None,
            extra: Map::new(),
        }
    }

    /// `(kind, id)`: who this provenance stands for.
    fn identity(&self) -> Option<(ProvenanceKind, &str)> {
        self.id.as_deref().map(|id| (self.kind, id))
    }

    /// `<kind>:<id>`, the actor a replay receipt binds.
    pub fn actor_key(&self) -> String {
        format!(
            "{}:{}",
            self.kind.as_str(),
            self.id.as_deref().unwrap_or("")
        )
    }
}

/// The operation a replay receipt records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptOperation {
    Answer,
    Clear,
}

impl ReceiptOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Answer => "answer",
            Self::Clear => "clear",
        }
    }
}

/// The record of the command that produced the current answer slot. A retry
/// replays only when all four fields match (`g5.documents.question.writer`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerReceipt {
    pub key: String,
    pub actor: String,
    pub operation: ReceiptOperation,
    pub payload_digest: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer slot of one question. Empty (all defaults) while open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    #[serde(default)]
    pub selected_option_ids: Vec<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub answered_by: Option<Provenance>,
    #[serde(default)]
    pub answered_at: Option<String>,
    #[serde(default)]
    pub answered_at_revision: Option<u64>,
    #[serde(default)]
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<AnswerReceipt>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Answer {
    /// True when nothing is recorded (what returning to `open` leaves
    /// behind). `version`, `receipt` and unknown fields are bookkeeping.
    pub fn is_empty(&self) -> bool {
        self.selected_option_ids.is_empty()
            && self.text.is_empty()
            && self.rationale.is_empty()
            && self.answered_by.is_none()
            && self.answered_at.is_none()
            && self.answered_at_revision.is_none()
    }
}

/// One offered option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tradeoffs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risks: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_true() -> bool {
    true
}

/// One question entry of a set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    /// Intrinsic id, never positional.
    pub id: String,
    /// Compare-and-set token, `>= 1`.
    pub revision: u64,
    pub status: QuestionStatus,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<QuestionSeverity>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default = "default_true")]
    pub allow_free_text: bool,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub answer: Answer,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Question {
    /// A fresh open question with an empty answer at revision 1.
    pub fn new_open(id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            revision: 1,
            status: QuestionStatus::Open,
            prompt: prompt.into(),
            context: None,
            severity: None,
            multi_select: false,
            allow_free_text: true,
            options: Vec::new(),
            answer: Answer::default(),
            extra: Map::new(),
        }
    }

    /// Everything except `revision`, `status`, and `answer`.
    fn non_answer_projection(&self) -> Question {
        let mut projected = self.clone();
        projected.revision = 0;
        projected.status = QuestionStatus::Open;
        projected.answer = Answer::default();
        projected
    }
}

/// Set-level metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Provenance>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One parsed set file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSetFile {
    pub schema: u64,
    pub set: SetMeta,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl QuestionSetFile {
    /// A schema-3 file with no questions.
    pub fn new(set: SetMeta) -> Self {
        Self {
            schema: QUESTION_SET_SCHEMA,
            set,
            questions: Vec::new(),
            extra: Map::new(),
        }
    }

    /// The question with this intrinsic id, if present.
    pub fn question(&self, id: &str) -> Option<&Question> {
        self.questions.iter().find(|question| question.id == id)
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Typed parse failure; `code()` is the stable kebab-case wire code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(
    tag = "code",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum QuestionSetParseError {
    #[error("question set file is {size} bytes; the cap is {max_bytes} bytes")]
    TooLarge { size: usize, max_bytes: usize },
    #[error("question set file is not valid JSON: {message}")]
    InvalidJson { message: String },
    #[error("question set file is not a JSON object")]
    NotAnObject,
    #[error("unsupported question set schema {found:?}; only schema 3 is supported")]
    UnsupportedSchema { found: Option<Value> },
    #[error("non-integer number at {path}; question set files carry integers only")]
    FloatNumber { path: String },
    #[error("question set file does not match the schema-3 shape: {message}")]
    InvalidShape { message: String },
    #[error("invalid set id {set_id:?}")]
    InvalidSetId { set_id: String },
    #[error("invalid status {found:?} on question {question_id:?}")]
    InvalidStatus {
        question_id: Option<String>,
        found: String,
    },
    #[error("duplicate question id {question_id:?}")]
    DuplicateQuestionId { question_id: String },
    #[error("duplicate option id {option_id:?} in question {question_id:?}")]
    DuplicateOptionId {
        question_id: String,
        option_id: String,
    },
    #[error("more than one recommended option in question {question_id:?}")]
    MultipleRecommended { question_id: String },
    #[error("question {question_id:?} is answered without provenance")]
    AnsweredWithoutProvenance { question_id: String },
}

impl QuestionSetParseError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooLarge { .. } => "too-large",
            Self::InvalidJson { .. } => "invalid-json",
            Self::NotAnObject => "not-an-object",
            Self::UnsupportedSchema { .. } => "unsupported-schema",
            Self::FloatNumber { .. } => "float-number",
            Self::InvalidShape { .. } => "invalid-shape",
            Self::InvalidSetId { .. } => "invalid-set-id",
            Self::InvalidStatus { .. } => "invalid-status",
            Self::DuplicateQuestionId { .. } => "duplicate-question-id",
            Self::DuplicateOptionId { .. } => "duplicate-option-id",
            Self::MultipleRecommended { .. } => "multiple-recommended",
            Self::AnsweredWithoutProvenance { .. } => "answered-without-provenance",
        }
    }
}

/// One bounded slug segment: `^[a-z0-9][a-z0-9-]{0,max_len-1}$`. Shared by the
/// question-set ids and the guided-journey step ids (`g5.workbench.journey.marks`).
pub fn is_slug_segment(bytes: &[u8], max_len: usize) -> bool {
    !bytes.is_empty()
        && bytes.len() <= max_len
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// Validate a set id: one path segment matching `^[a-z0-9][a-z0-9-]{0,63}$`.
pub fn parse_set_id(candidate: &str) -> Result<String, QuestionSetParseError> {
    if is_slug_segment(candidate.as_bytes(), MAX_SET_ID_LEN) {
        Ok(candidate.to_string())
    } else {
        Err(QuestionSetParseError::InvalidSetId {
            set_id: candidate.to_string(),
        })
    }
}

/// A question id a reference or command may name: non-empty, bounded, no
/// path separator, no surrounding whitespace.
pub fn is_valid_question_id(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.len() <= MAX_QUESTION_ID_LEN
        && candidate.trim() == candidate
        && !candidate.contains('/')
        && !candidate.chars().any(char::is_control)
}

fn find_float(value: &Value, path: &mut String) -> Option<String> {
    match value {
        Value::Number(number) => (!(number.is_i64() || number.is_u64())).then(|| path.clone()),
        Value::Array(items) => items.iter().enumerate().find_map(|(index, item)| {
            let len = path.len();
            path.push_str(&format!("[{index}]"));
            let found = find_float(item, path);
            path.truncate(len);
            found
        }),
        Value::Object(map) => map.iter().find_map(|(key, item)| {
            let len = path.len();
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(key);
            let found = find_float(item, path);
            path.truncate(len);
            found
        }),
        _ => None,
    }
}

fn check_statuses(root: &Map<String, Value>) -> Result<(), QuestionSetParseError> {
    let Some(Value::Array(questions)) = root.get("questions") else {
        return Ok(());
    };
    for question in questions {
        let Value::Object(object) = question else {
            continue;
        };
        let question_id = object.get("id").and_then(Value::as_str).map(str::to_string);
        let found = match object.get("status") {
            Some(Value::String(status)) => {
                if QuestionStatus::parse(status).is_some() {
                    continue;
                }
                status.clone()
            }
            Some(other) => other.to_string(),
            None => "<missing>".to_string(),
        };
        return Err(QuestionSetParseError::InvalidStatus { question_id, found });
    }
    Ok(())
}

fn validate_file(file: &QuestionSetFile) -> Result<(), QuestionSetParseError> {
    parse_set_id(&file.set.id)?;
    let mut question_ids: BTreeSet<&str> = BTreeSet::new();
    for question in &file.questions {
        if !is_valid_question_id(&question.id) {
            return Err(QuestionSetParseError::InvalidShape {
                message: format!("question id {:?} is not a valid intrinsic id", question.id),
            });
        }
        if !question_ids.insert(question.id.as_str()) {
            return Err(QuestionSetParseError::DuplicateQuestionId {
                question_id: question.id.clone(),
            });
        }
        if question.revision == 0 {
            return Err(QuestionSetParseError::InvalidShape {
                message: format!(
                    "question {:?} has revision 0; revisions start at 1",
                    question.id
                ),
            });
        }
        let mut option_ids: BTreeSet<&str> = BTreeSet::new();
        let mut recommended = 0usize;
        for option in &question.options {
            if !option_ids.insert(option.id.as_str()) {
                return Err(QuestionSetParseError::DuplicateOptionId {
                    question_id: question.id.clone(),
                    option_id: option.id.clone(),
                });
            }
            if option.recommended == Some(true) {
                recommended += 1;
            }
        }
        if recommended > 1 {
            return Err(QuestionSetParseError::MultipleRecommended {
                question_id: question.id.clone(),
            });
        }
        if question.status != QuestionStatus::Open && question.answer.answered_by.is_none() {
            return Err(QuestionSetParseError::AnsweredWithoutProvenance {
                question_id: question.id.clone(),
            });
        }
    }
    Ok(())
}

/// Parse and validate a set file. Never panics; every failure is typed.
pub fn parse_question_set(bytes: &[u8]) -> Result<QuestionSetFile, QuestionSetParseError> {
    if bytes.len() > MAX_QUESTION_SET_BYTES {
        return Err(QuestionSetParseError::TooLarge {
            size: bytes.len(),
            max_bytes: MAX_QUESTION_SET_BYTES,
        });
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| QuestionSetParseError::InvalidJson {
            message: error.to_string(),
        })?;
    let Value::Object(root) = &value else {
        return Err(QuestionSetParseError::NotAnObject);
    };
    match root.get("schema") {
        Some(Value::Number(schema)) if schema.as_u64() == Some(QUESTION_SET_SCHEMA) => {}
        other => {
            return Err(QuestionSetParseError::UnsupportedSchema {
                found: other.cloned(),
            })
        }
    }
    if let Some(path) = find_float(&value, &mut String::new()) {
        return Err(QuestionSetParseError::FloatNumber { path });
    }
    check_statuses(root)?;
    let file: QuestionSetFile =
        serde_json::from_value(value).map_err(|error| QuestionSetParseError::InvalidShape {
            message: error.to_string(),
        })?;
    validate_file(&file)?;
    Ok(file)
}

// ---------------------------------------------------------------------------
// Canonical serialization
// ---------------------------------------------------------------------------

fn write_json_string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_indent(out: &mut String, indent: usize, depth: usize) {
    for _ in 0..indent * depth {
        out.push(' ');
    }
}

/// Deterministic JSON writer: object keys sorted by byte order at every
/// level. `indent` of `0` produces the compact form; otherwise the serde_json
/// pretty layout.
fn write_sorted(out: &mut String, value: &Value, indent: usize, depth: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(boolean) => out.push_str(if *boolean { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::String(text) => write_json_string(out, text),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                if indent > 0 {
                    out.push('\n');
                    write_indent(out, indent, depth + 1);
                }
                write_sorted(out, item, indent, depth + 1);
            }
            if indent > 0 {
                out.push('\n');
                write_indent(out, indent, depth);
            }
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                if indent > 0 {
                    out.push('\n');
                    write_indent(out, indent, depth + 1);
                }
                write_json_string(out, key);
                out.push(':');
                if indent > 0 {
                    out.push(' ');
                }
                if let Some(item) = map.get(*key) {
                    write_sorted(out, item, indent, depth + 1);
                }
            }
            if indent > 0 {
                out.push('\n');
                write_indent(out, indent, depth);
            }
            out.push('}');
        }
    }
}

/// Serialization of these types cannot fail (string keys, no floats); the
/// fallback keeps the function total.
fn file_value(file: &QuestionSetFile) -> Value {
    serde_json::to_value(file).unwrap_or(Value::Null)
}

/// Compact canonical bytes (keys sorted at every level), for digests and
/// equality across key orders.
pub fn canonical_content_bytes(file: &QuestionSetFile) -> Vec<u8> {
    let mut out = String::new();
    write_sorted(&mut out, &file_value(file), 0, 0);
    out.into_bytes()
}

/// The on-disk bytes: keys sorted, two-space indentation, trailing LF.
pub fn render_file(file: &QuestionSetFile) -> String {
    render_sorted_json(&file_value(file))
}

/// Deterministic pretty JSON for any value (sorted keys, two-space indent,
/// trailing LF).
pub fn render_sorted_json(value: &Value) -> String {
    let mut out = String::new();
    write_sorted(&mut out, value, 2, 0);
    out.push('\n');
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_encode(&Sha256::digest(bytes))
}

// ---------------------------------------------------------------------------
// Shape gate
// ---------------------------------------------------------------------------

/// Typed rejection of an answer transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(
    tag = "code",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ShapeGateError {
    #[error("the question id changed from {previous:?} to {next:?}")]
    SequenceChanged { previous: String, next: String },
    #[error("only revision, status, and answer may change on question {question_id:?}")]
    NonAnswerMutation { question_id: String },
    #[error("revision must move from {previous} to {expected}, found {found}")]
    RevisionSkip {
        previous: u64,
        expected: u64,
        found: u64,
    },
    #[error("answer version must move from {previous} to {expected}, found {found}")]
    AnswerVersionSkip {
        previous: u64,
        expected: u64,
        found: u64,
    },
    #[error("the recorded answer on question {question_id:?} belongs to another person")]
    ForeignAnswerImmutable { question_id: String },
    #[error("the answer on question {question_id:?} must carry the caller's own provenance")]
    ProvenanceMismatch { question_id: String },
    #[error("option {option_id:?} does not exist on question {question_id:?}")]
    UnknownOption {
        question_id: String,
        option_id: String,
    },
    #[error("answer shape on question {question_id:?} is invalid: {message}")]
    SelectionShape {
        question_id: String,
        message: String,
    },
    #[error("only the answer's own author may clear question {question_id:?}")]
    ClearNotAuthor { question_id: String },
}

impl ShapeGateError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SequenceChanged { .. } => "sequence-changed",
            Self::NonAnswerMutation { .. } => "non-answer-mutation",
            Self::RevisionSkip { .. } => "revision-skip",
            Self::AnswerVersionSkip { .. } => "answer-version-skip",
            Self::ForeignAnswerImmutable { .. } => "foreign-answer-immutable",
            Self::ProvenanceMismatch { .. } => "provenance-mismatch",
            Self::UnknownOption { .. } => "unknown-option",
            Self::SelectionShape { .. } => "selection-shape",
            Self::ClearNotAuthor { .. } => "clear-not-author",
        }
    }
}

/// The person who owns `question`'s recorded answer: the `user` provenance
/// of an `answered` entry. Proposals have no owner.
fn immutable_owner(question: &Question) -> Option<(ProvenanceKind, &str)> {
    if question.status != QuestionStatus::Answered {
        return None;
    }
    let identity = question.answer.answered_by.as_ref()?.identity()?;
    (identity.0 == ProvenanceKind::User).then_some(identity)
}

fn check_selection(next: &Question, actor: &Provenance) -> Result<(), ShapeGateError> {
    let shape = |message: &str| ShapeGateError::SelectionShape {
        question_id: next.id.clone(),
        message: message.to_string(),
    };
    let Some(answered_by) = &next.answer.answered_by else {
        return Err(ShapeGateError::ProvenanceMismatch {
            question_id: next.id.clone(),
        });
    };
    if answered_by.kind != actor.kind || answered_by.id != actor.id {
        return Err(ShapeGateError::ProvenanceMismatch {
            question_id: next.id.clone(),
        });
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for option_id in &next.answer.selected_option_ids {
        if !next.options.iter().any(|option| &option.id == option_id) {
            return Err(ShapeGateError::UnknownOption {
                question_id: next.id.clone(),
                option_id: option_id.clone(),
            });
        }
        if !seen.insert(option_id.as_str()) {
            return Err(shape("an option is selected more than once"));
        }
    }
    let text = next.answer.text.trim();
    if next.answer.selected_option_ids.is_empty() && text.is_empty() {
        return Err(shape("an answer needs a selection or text"));
    }
    if !next.multi_select && next.answer.selected_option_ids.len() > 1 {
        return Err(shape("this question accepts one selected option"));
    }
    if !next.allow_free_text && !text.is_empty() {
        return Err(shape("this question does not accept free text"));
    }
    Ok(())
}

/// The only mutable region is the answer: `prev` → `next` is admitted when
/// nothing but `revision`, `status`, and `answer` changed, the revision and
/// answer version each advanced by one, another person's recorded answer is
/// untouched, a recorded answer carries `actor`'s own provenance with a valid
/// selection, a `user` answer is `answered` and an `agent` answer is
/// `proposed`, and a clear comes from the answer's author (or a person
/// clearing a proposal).
pub fn answer_shape_gate(
    prev: &Question,
    next: &Question,
    actor: &Provenance,
) -> Result<(), ShapeGateError> {
    if prev.id != next.id {
        return Err(ShapeGateError::SequenceChanged {
            previous: prev.id.clone(),
            next: next.id.clone(),
        });
    }
    if prev.non_answer_projection() != next.non_answer_projection() {
        return Err(ShapeGateError::NonAnswerMutation {
            question_id: prev.id.clone(),
        });
    }
    if next.revision != prev.revision.saturating_add(1) {
        return Err(ShapeGateError::RevisionSkip {
            previous: prev.revision,
            expected: prev.revision.saturating_add(1),
            found: next.revision,
        });
    }
    if next.answer.version != prev.answer.version.saturating_add(1) {
        return Err(ShapeGateError::AnswerVersionSkip {
            previous: prev.answer.version,
            expected: prev.answer.version.saturating_add(1),
            found: next.answer.version,
        });
    }
    if let Some(owner) = immutable_owner(prev) {
        if actor.identity() != Some(owner) {
            return Err(ShapeGateError::ForeignAnswerImmutable {
                question_id: prev.id.clone(),
            });
        }
    }
    match next.status {
        QuestionStatus::Answered | QuestionStatus::Proposed => {
            let expected = match actor.kind {
                ProvenanceKind::User => QuestionStatus::Answered,
                ProvenanceKind::Agent => QuestionStatus::Proposed,
            };
            if next.status != expected {
                return Err(ShapeGateError::SelectionShape {
                    question_id: prev.id.clone(),
                    message: format!(
                        "a {} answer is recorded as {}",
                        actor.kind.as_str(),
                        expected.as_str()
                    ),
                });
            }
            check_selection(next, actor)
        }
        QuestionStatus::Open => {
            if !next.answer.is_empty() {
                return Err(ShapeGateError::SelectionShape {
                    question_id: prev.id.clone(),
                    message: "an open question carries an empty answer".to_string(),
                });
            }
            if prev.status == QuestionStatus::Open {
                return Ok(());
            }
            let author = prev
                .answer
                .answered_by
                .as_ref()
                .and_then(Provenance::identity);
            let same_author = author.is_some() && author == actor.identity();
            let user_clears_proposal =
                prev.status == QuestionStatus::Proposed && actor.kind == ProvenanceKind::User;
            if same_author || user_clears_proposal {
                Ok(())
            } else {
                Err(ShapeGateError::ClearNotAuthor {
                    question_id: prev.id.clone(),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Decision predicate
// ---------------------------------------------------------------------------

/// `g5.documents.question.decision`: an `answered` entry with `user`
/// provenance. An agent's answer is a proposal whatever the file says.
pub fn is_decision(question: &Question) -> bool {
    question.status == QuestionStatus::Answered
        && question
            .answer
            .answered_by
            .as_ref()
            .is_some_and(|provenance| provenance.kind == ProvenanceKind::User)
}

// ---------------------------------------------------------------------------
// Intrinsic ids and the revision rule
// ---------------------------------------------------------------------------

/// Lower-cased text with non-alphanumerics folded to single hyphens, trimmed
/// of hyphens, cut to `MAX_SLUG_LEN`.
fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut pending_hyphen = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(ch);
        } else {
            pending_hyphen = true;
        }
    }
    let mut cut = slug.len().min(MAX_SLUG_LEN);
    while !slug.is_char_boundary(cut) {
        cut -= 1;
    }
    slug.truncate(cut);
    slug.trim_end_matches('-').to_string()
}

/// `<slug>-<8 hex>`: the slug of the first six words of `prompt`, and the
/// first eight hex digits of SHA-256 over `prompt || nonce`.
pub fn generate_question_id(prompt: &str, nonce: &[u8]) -> String {
    let head: Vec<&str> = prompt.split_whitespace().take(6).collect();
    let mut slug = slugify(&head.join(" "));
    if slug.is_empty() {
        slug = "question".to_string();
    }
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    hasher.update(nonce);
    let digest = hex_encode(&hasher.finalize());
    format!("{slug}-{}", &digest[..8])
}

/// The slug of `label`, suffixed `-2`, `-3`, … while it collides with `taken`.
pub fn generate_option_id(label: &str, taken: &BTreeSet<String>) -> String {
    let mut base = slugify(label);
    if base.is_empty() {
        base = "option".to_string();
    }
    if !taken.contains(&base) {
        return base;
    }
    let mut counter: u64 = 2;
    loop {
        let candidate = format!("{base}-{counter}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        counter = counter.saturating_add(1);
    }
}

/// What an authoring edit to a question amounts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RevisionOutcome {
    /// The non-answer projection is identical; nothing to record.
    Unchanged,
    /// Wording changed; the caller bumps `revision` and keeps the answer
    /// (`answeredAtRevision` stays where the answer was given).
    NewRevision,
}

/// Typed rejection of an authoring edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(
    tag = "code",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum RevisionGateError {
    #[error("the question id changed from {previous:?} to {next:?}")]
    SequenceChanged { previous: String, next: String },
    #[error("question {question_id:?} holds a decision; {reason} needs a new question")]
    NeedsNewQuestion { question_id: String, reason: String },
}

impl RevisionGateError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SequenceChanged { .. } => "sequence-changed",
            Self::NeedsNewQuestion { .. } => "needs-new-question",
        }
    }
}

/// On a question whose answer is a decision, wording edits (`prompt`,
/// `context`, `severity`, option `description`/`tradeoffs`/`risks`/
/// `recommended`) are a new revision that keeps the answer; adding or
/// removing an option, changing any option `id` or `label`, or flipping
/// `multiSelect` needs a new question. `revision`, `status`, and `answer` are
/// ignored here — the caller carries the answer forward.
pub fn revision_gate(
    prev: &Question,
    next: &Question,
) -> Result<RevisionOutcome, RevisionGateError> {
    if prev.id != next.id {
        return Err(RevisionGateError::SequenceChanged {
            previous: prev.id.clone(),
            next: next.id.clone(),
        });
    }
    if prev.non_answer_projection() == next.non_answer_projection() {
        return Ok(RevisionOutcome::Unchanged);
    }
    if !is_decision(prev) {
        return Ok(RevisionOutcome::NewRevision);
    }
    let needs_new = |reason: &str| RevisionGateError::NeedsNewQuestion {
        question_id: prev.id.clone(),
        reason: reason.to_string(),
    };
    if prev.multi_select != next.multi_select {
        return Err(needs_new("changing multiSelect"));
    }
    if prev.options.len() != next.options.len() {
        return Err(needs_new("adding or removing an option"));
    }
    for (prev_option, next_option) in prev.options.iter().zip(&next.options) {
        if prev_option.id != next_option.id {
            return Err(needs_new("changing an option id"));
        }
        if prev_option.label != next_option.label {
            return Err(needs_new("changing an option label"));
        }
    }
    Ok(RevisionOutcome::NewRevision)
}

// ---------------------------------------------------------------------------
// Answer transitions and the replay receipt
// ---------------------------------------------------------------------------

/// What a caller proposes as the answer to one question.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerProposal {
    #[serde(default)]
    pub selected_option_ids: Vec<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub rationale: String,
}

/// SHA-256 over the canonical compact JSON of what a command asks for: the
/// operation, the revision it expected and, for an answer, the proposal.
pub fn receipt_payload_digest(
    operation: ReceiptOperation,
    expected_revision: u64,
    proposal: Option<&AnswerProposal>,
) -> String {
    let mut value = serde_json::json!({
        "operation": operation.as_str(),
        "expectedRevision": expected_revision,
    });
    if let (Some(proposal), Value::Object(map)) = (proposal, &mut value) {
        map.insert(
            "proposal".to_string(),
            serde_json::to_value(proposal).unwrap_or(Value::Null),
        );
    }
    let mut out = String::new();
    write_sorted(&mut out, &value, 0, 0);
    sha256_hex(out.as_bytes())
}

/// Build the receipt a command records.
pub fn answer_receipt(
    key: &str,
    actor: &Provenance,
    operation: ReceiptOperation,
    expected_revision: u64,
    proposal: Option<&AnswerProposal>,
) -> AnswerReceipt {
    AnswerReceipt {
        key: key.to_string(),
        actor: actor.actor_key(),
        operation,
        payload_digest: receipt_payload_digest(operation, expected_revision, proposal),
        extra: Map::new(),
    }
}

/// How a command's receipt relates to the entry's recorded one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayCheck {
    /// No recorded receipt carries this key: apply the command.
    Fresh,
    /// Same key, actor, operation and payload: the command already ran.
    Replay,
    /// Same key, different request: refuse.
    KeyReused,
}

/// Compare `receipt` with the receipt recorded on `question`.
pub fn replay_check(question: &Question, receipt: &AnswerReceipt) -> ReplayCheck {
    match &question.answer.receipt {
        Some(recorded) if recorded.key == receipt.key => {
            if recorded.actor == receipt.actor
                && recorded.operation == receipt.operation
                && recorded.payload_digest == receipt.payload_digest
            {
                ReplayCheck::Replay
            } else {
                ReplayCheck::KeyReused
            }
        }
        _ => ReplayCheck::Fresh,
    }
}

/// The next entry after recording `proposal` by `actor`: `answered` for a
/// user, `proposed` for an agent; `revision + 1`; `answer.version + 1`;
/// `answeredAt = now`; `answeredAtRevision` = the new revision; the receipt
/// stored. Passes the shape gate against `question` before returning.
pub fn apply_answer_transition(
    question: &Question,
    proposal: &AnswerProposal,
    actor: &Provenance,
    now: &str,
    receipt: AnswerReceipt,
) -> Result<Question, ShapeGateError> {
    let mut next = question.clone();
    next.revision = question.revision.saturating_add(1);
    next.status = match actor.kind {
        ProvenanceKind::User => QuestionStatus::Answered,
        ProvenanceKind::Agent => QuestionStatus::Proposed,
    };
    next.answer = Answer {
        selected_option_ids: proposal.selected_option_ids.clone(),
        text: proposal.text.clone(),
        rationale: proposal.rationale.clone(),
        answered_by: Some(actor.clone()),
        answered_at: Some(now.to_string()),
        answered_at_revision: Some(next.revision),
        version: question.answer.version.saturating_add(1),
        receipt: Some(receipt),
        extra: question.answer.extra.clone(),
    };
    answer_shape_gate(question, &next, actor)?;
    Ok(next)
}

/// The next entry after `actor` clears the answer: `open`, the answer
/// emptied, `revision + 1`, `answer.version + 1`, the receipt stored. Passes
/// the shape gate against `question` before returning.
pub fn apply_clear_transition(
    question: &Question,
    actor: &Provenance,
    receipt: AnswerReceipt,
) -> Result<Question, ShapeGateError> {
    let mut next = question.clone();
    next.revision = question.revision.saturating_add(1);
    next.status = QuestionStatus::Open;
    next.answer = Answer {
        version: question.answer.version.saturating_add(1),
        receipt: Some(receipt),
        extra: question.answer.extra.clone(),
        ..Answer::default()
    };
    answer_shape_gate(question, &next, actor)?;
    Ok(next)
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// `docs/<workspace_id>/questions/<set_id>.g5q.json`: where a workspace
/// authors a new set.
pub fn question_set_path(workspace_id: &str, set_id: &str) -> String {
    format!("docs/{workspace_id}/{QUESTION_SET_DIR}/{set_id}{QUESTION_SET_EXT}")
}

/// `docs/<workspace_id>/questions`: the directory a workspace's own sets
/// live in.
pub fn question_set_dir(workspace_id: &str) -> String {
    format!("docs/{workspace_id}/{QUESTION_SET_DIR}")
}

/// A validated set path: `docs/<owner>/questions/<set>.g5q.json`, where
/// `<owner>` is one plain path segment (the authoring workspace's id) and
/// `<set>` a valid set id. Anything else — traversal, extra segments, other
/// extensions, backslashes — is not a question set path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionSetPath {
    pub path: String,
    pub owner_segment: String,
    pub set_id: String,
}

/// Parse `path` as a question set path (see [`QuestionSetPath`]).
pub fn parse_question_set_path(path: &str) -> Option<QuestionSetPath> {
    if path.contains('\\') || path.chars().any(char::is_control) {
        return None;
    }
    let mut segments = path.split('/');
    let (Some("docs"), Some(owner), Some(dir), Some(name), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return None;
    };
    if dir != QUESTION_SET_DIR
        || owner.is_empty()
        || owner == "."
        || owner == ".."
        || owner.trim() != owner
    {
        return None;
    }
    let stem = name.strip_suffix(QUESTION_SET_EXT)?;
    let set_id = parse_set_id(stem).ok()?;
    Some(QuestionSetPath {
        path: path.to_string(),
        owner_segment: owner.to_string(),
        set_id,
    })
}

/// True when `path` names a question set file (the owner file routes run the
/// schema check for exactly these paths).
pub fn is_question_set_path(path: &str) -> bool {
    parse_question_set_path(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    const FIXTURE_FILES: &[&str] = &[
        "open-two-options",
        "answered-user",
        "proposed-agent",
        "unknown-fields",
        "non-canonical",
    ];

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("question_set")
    }

    fn alice() -> Provenance {
        Provenance {
            email: Some("alice@example.test".to_string()),
            avatar_url: Some("https://avatars.example.test/alice.png".to_string()),
            ..Provenance::user("u-alice")
        }
    }

    fn bob() -> Provenance {
        Provenance {
            email: Some("bob@example.test".to_string()),
            ..Provenance::user("u-bob")
        }
    }

    fn agent() -> Provenance {
        Provenance {
            via: Some("g5d".to_string()),
            ..Provenance::agent("u-alice")
        }
    }

    fn option(label: &str, description: &str, taken: &mut BTreeSet<String>) -> QuestionOption {
        let id = generate_option_id(label, taken);
        taken.insert(id.clone());
        QuestionOption {
            id,
            label: label.to_string(),
            description: description.to_string(),
            tradeoffs: None,
            risks: None,
            recommended: None,
            extra: Map::new(),
        }
    }

    fn storage_question(set_id: &str) -> Question {
        let prompt = "Which storage backend should the question store use?";
        let mut taken = BTreeSet::new();
        let mut file = option(
            "Set file in the docs repo",
            "One JSON file per set, written by the question service and committed with the branch.",
            &mut taken,
        );
        file.recommended = Some(true);
        file.tradeoffs = Some("Git stays the truth for accepted answers.".to_string());
        let mut database = option(
            "Database-owned answer store",
            "Answers live in Postgres and the checkout carries references only.",
            &mut taken,
        );
        database.risks = Some("Answers vanish from the checkout.".to_string());
        Question {
            id: generate_question_id(prompt, format!("fixture:{set_id}").as_bytes()),
            severity: Some(QuestionSeverity::High),
            context: Some("The plan needs one durable home for answers.".to_string()),
            options: vec![file, database],
            ..Question::new_open("", prompt)
        }
    }

    fn naming_question(set_id: &str) -> Question {
        let prompt = "Should display numbering stay renderer-derived?";
        Question {
            id: generate_question_id(prompt, format!("fixture:{set_id}:2").as_bytes()),
            ..Question::new_open("", prompt)
        }
    }

    fn base_file(set_id: &str) -> QuestionSetFile {
        QuestionSetFile::new(SetMeta {
            id: set_id.to_string(),
            title: "Question store design".to_string(),
            created_at: "2026-10-09T10:00:00Z".to_string(),
            created_by: Some(alice()),
            extra: Map::new(),
        })
    }

    fn proposal_for(question: &Question) -> AnswerProposal {
        let first_option = question.options.first().map(|o| o.id.clone());
        AnswerProposal {
            text: if first_option.is_none() {
                "Yes, keep it renderer-derived.".to_string()
            } else {
                String::new()
            },
            selected_option_ids: first_option.into_iter().collect(),
            rationale: String::new(),
        }
    }

    fn answered(question: &Question, actor: &Provenance, key: &str) -> Question {
        let proposal = proposal_for(question);
        let receipt = answer_receipt(
            key,
            actor,
            ReceiptOperation::Answer,
            question.revision,
            Some(&proposal),
        );
        apply_answer_transition(question, &proposal, actor, "2026-10-09T11:30:00Z", receipt)
            .expect("fixture answer transition")
    }

    fn clear(
        question: &Question,
        actor: &Provenance,
        key: &str,
    ) -> Result<Question, ShapeGateError> {
        let receipt = answer_receipt(key, actor, ReceiptOperation::Clear, question.revision, None);
        apply_clear_transition(question, actor, receipt)
    }

    /// Every fixture file, built from the module's own primitives.
    fn build_fixtures() -> BTreeMap<String, Vec<u8>> {
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut expected = Map::new();
        let mut record = |name: &str, bytes: Vec<u8>, file: &QuestionSetFile| {
            let decisions: Map<String, Value> = file
                .questions
                .iter()
                .map(|q| (q.id.clone(), Value::Bool(is_decision(q))))
                .collect();
            let statuses: Map<String, Value> = file
                .questions
                .iter()
                .map(|q| (q.id.clone(), Value::String(q.status.as_str().to_string())))
                .collect();
            expected.insert(
                format!("{name}{QUESTION_SET_EXT}"),
                json!({
                    "decisions": decisions,
                    "statuses": statuses,
                    "canonicalSha256": sha256_hex(&canonical_content_bytes(file)),
                }),
            );
            files.insert(format!("{name}{QUESTION_SET_EXT}"), bytes);
        };

        let mut open = base_file("open-two-options");
        open.questions.push(storage_question("open-two-options"));
        record("open-two-options", render_file(&open).into_bytes(), &open);

        let mut answered_user = base_file("answered-user");
        let q = storage_question("answered-user");
        answered_user
            .questions
            .push(answered(&q, &alice(), "idem-answered-user-1"));
        answered_user
            .questions
            .push(naming_question("answered-user"));
        record(
            "answered-user",
            render_file(&answered_user).into_bytes(),
            &answered_user,
        );

        let mut proposed = base_file("proposed-agent");
        let q = storage_question("proposed-agent");
        proposed
            .questions
            .push(answered(&q, &agent(), "agent:proposed-agent:q:1"));
        record(
            "proposed-agent",
            render_file(&proposed).into_bytes(),
            &proposed,
        );

        let mut unknown = base_file("unknown-fields");
        unknown
            .extra
            .insert("xVendor".to_string(), json!({"origin": "fixture"}));
        unknown
            .set
            .extra
            .insert("xNote".to_string(), json!("set-level unknown field"));
        let mut q = storage_question("unknown-fields");
        q.extra.insert("xTag".to_string(), json!(["a", "b"]));
        q.options[0].extra.insert("xHint".to_string(), json!(42));
        let mut q = answered(&q, &alice(), "idem-unknown-fields-1");
        q.answer
            .extra
            .insert("xTrace".to_string(), json!({"nested": {"deep": true}}));
        if let Some(by) = &mut q.answer.answered_by {
            by.extra.insert("xLocale".to_string(), json!("en"));
        }
        unknown.questions.push(q);
        record(
            "unknown-fields",
            render_file(&unknown).into_bytes(),
            &unknown,
        );

        // non-canonical: valid content, 4-space indent, reversed key order.
        let mut drift = base_file("non-canonical");
        drift.questions.push(storage_question("non-canonical"));
        let mut out = String::new();
        write_reversed(&mut out, &file_value(&drift), 0);
        out.push('\n');
        record("non-canonical", out.into_bytes(), &drift);

        files.insert(
            "expected.json".to_string(),
            render_sorted_json(&Value::Object(expected)).into_bytes(),
        );
        files
    }

    /// Test-only writer: reverse key order, four-space indent.
    fn write_reversed(out: &mut String, value: &Value, depth: usize) {
        match value {
            Value::Array(items) if !items.is_empty() => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push('\n');
                    out.push_str(&" ".repeat(4 * (depth + 1)));
                    write_reversed(out, item, depth + 1);
                }
                out.push('\n');
                out.push_str(&" ".repeat(4 * depth));
                out.push(']');
            }
            Value::Object(map) if !map.is_empty() => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                keys.reverse();
                out.push('{');
                for (index, key) in keys.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push('\n');
                    out.push_str(&" ".repeat(4 * (depth + 1)));
                    write_json_string(out, key);
                    out.push_str(": ");
                    if let Some(item) = map.get(*key) {
                        write_reversed(out, item, depth + 1);
                    }
                }
                out.push('\n');
                out.push_str(&" ".repeat(4 * depth));
                out.push('}');
            }
            other => write_sorted(out, other, 0, 0),
        }
    }

    fn read_fixture(name: &str) -> Vec<u8> {
        std::fs::read(fixture_dir().join(name))
            .unwrap_or_else(|error| panic!("fixture {name} missing: {error}"))
    }

    #[test]
    fn fixtures_are_current() {
        let built = build_fixtures();
        let dir = fixture_dir();
        if std::env::var("QUESTION_SET_REGEN_FIXTURES").as_deref() == Ok("1") {
            std::fs::create_dir_all(&dir).expect("fixture dir");
            for (name, bytes) in &built {
                std::fs::write(dir.join(name), bytes).expect("write fixture");
            }
        }
        for (name, bytes) in &built {
            assert!(
                &read_fixture(name) == bytes,
                "fixture {name} is stale; rerun with QUESTION_SET_REGEN_FIXTURES=1"
            );
        }
    }

    #[test]
    fn every_fixture_parses_round_trips_and_decides_as_expected() {
        let expected: Map<String, Value> =
            serde_json::from_slice(&read_fixture("expected.json")).unwrap();
        assert_eq!(expected.len(), FIXTURE_FILES.len());
        for name in FIXTURE_FILES {
            let file_name = format!("{name}{QUESTION_SET_EXT}");
            let bytes = read_fixture(&file_name);
            let file = parse_question_set(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(file.schema, 3);
            assert_eq!(file.set.id, *name);
            let rendered = render_file(&file);
            assert_eq!(parse_question_set(rendered.as_bytes()).unwrap(), file);
            if *name == "non-canonical" {
                assert_ne!(rendered.as_bytes(), &bytes[..]);
            } else {
                assert_eq!(
                    rendered.as_bytes(),
                    &bytes[..],
                    "{name}: not byte-identical"
                );
            }
            let want = &expected[&file_name];
            assert_eq!(
                sha256_hex(&canonical_content_bytes(&file)),
                want["canonicalSha256"].as_str().unwrap()
            );
            for question in &file.questions {
                assert_eq!(
                    is_decision(question),
                    want["decisions"][&question.id].as_bool().unwrap(),
                    "{name}/{}",
                    question.id
                );
            }
        }
    }

    #[test]
    fn unknown_fields_survive_verbatim() {
        let bytes = read_fixture("unknown-fields.g5q.json");
        let file = parse_question_set(&bytes).unwrap();
        assert_eq!(file.extra["xVendor"], json!({"origin": "fixture"}));
        assert_eq!(file.set.extra["xNote"], json!("set-level unknown field"));
        let q = &file.questions[0];
        assert_eq!(q.extra["xTag"], json!(["a", "b"]));
        assert_eq!(q.options[0].extra["xHint"], json!(42));
        assert_eq!(q.answer.extra["xTrace"], json!({"nested": {"deep": true}}));
        assert_eq!(
            q.answer.answered_by.as_ref().unwrap().extra["xLocale"],
            json!("en")
        );
        // An answer keeps the unknown answer fields.
        let mut prev = q.clone();
        prev.status = QuestionStatus::Answered;
        let next = answered(&prev, &alice(), "again");
        assert_eq!(
            next.answer.extra["xTrace"],
            json!({"nested": {"deep": true}})
        );
    }

    fn parse_err(value: Value) -> QuestionSetParseError {
        parse_question_set(value.to_string().as_bytes()).unwrap_err()
    }

    fn minimal_value() -> Value {
        json!({
            "schema": 3,
            "set": {"id": "minimal", "title": "t", "createdAt": "2026-01-01T00:00:00Z"},
            "questions": [{
                "id": "q-1", "revision": 1, "status": "open", "prompt": "p",
                "options": [{"id": "a", "label": "A"}, {"id": "b", "label": "B"}]
            }]
        })
    }

    #[test]
    fn parse_errors_carry_typed_codes() {
        assert_eq!(
            parse_question_set(&vec![b' '; MAX_QUESTION_SET_BYTES + 1])
                .unwrap_err()
                .code(),
            "too-large"
        );
        assert_eq!(parse_question_set(b"{").unwrap_err().code(), "invalid-json");
        assert_eq!(
            parse_question_set(b"[]").unwrap_err().code(),
            "not-an-object"
        );
        assert_eq!(
            parse_err(json!({"schema": 2, "set": {}})).code(),
            "unsupported-schema"
        );
        assert_eq!(
            parse_err(json!({"set": {}})),
            QuestionSetParseError::UnsupportedSchema { found: None }
        );
        let mut v = minimal_value();
        v["questions"][0]["revision"] = json!(1.0);
        assert_eq!(
            parse_err(v),
            QuestionSetParseError::FloatNumber {
                path: "questions[0].revision".to_string()
            }
        );
        let mut v = minimal_value();
        v["questions"][0]["status"] = json!("done");
        assert_eq!(parse_err(v).code(), "invalid-status");
        let mut v = minimal_value();
        v["set"]["id"] = json!("Bad_Id");
        assert_eq!(parse_err(v).code(), "invalid-set-id");
        let mut v = minimal_value();
        let q = v["questions"][0].clone();
        v["questions"].as_array_mut().unwrap().push(q);
        assert_eq!(parse_err(v).code(), "duplicate-question-id");
        let mut v = minimal_value();
        v["questions"][0]["options"][1]["id"] = json!("a");
        assert_eq!(parse_err(v).code(), "duplicate-option-id");
        let mut v = minimal_value();
        v["questions"][0]["options"][0]["recommended"] = json!(true);
        v["questions"][0]["options"][1]["recommended"] = json!(true);
        assert_eq!(parse_err(v).code(), "multiple-recommended");
        let mut v = minimal_value();
        v["questions"][0]["status"] = json!("answered");
        assert_eq!(parse_err(v).code(), "answered-without-provenance");
        let mut v = minimal_value();
        v["questions"][0]["status"] = json!("proposed");
        assert_eq!(parse_err(v).code(), "answered-without-provenance");
        let mut v = minimal_value();
        v["questions"][0]["revision"] = json!(0);
        assert_eq!(parse_err(v).code(), "invalid-shape");
        let mut v = minimal_value();
        v["questions"][0]["id"] = json!("a/b");
        assert_eq!(parse_err(v).code(), "invalid-shape");
        let mut v = minimal_value();
        v["questions"][0].as_object_mut().unwrap().remove("prompt");
        assert_eq!(parse_err(v).code(), "invalid-shape");
        // A schema-2 `integrity` block is just an unknown field here.
        let mut v = minimal_value();
        v["integrity"] = json!({"alg": "hmac-sha256", "keyId": "ws-v1", "signature": "00"});
        let file = parse_question_set(v.to_string().as_bytes()).unwrap();
        assert!(file.extra.contains_key("integrity"));
        assert_eq!(
            serde_json::to_value(QuestionSetParseError::DuplicateQuestionId {
                question_id: "q".to_string(),
            })
            .unwrap(),
            json!({"code": "duplicate-question-id", "questionId": "q"})
        );
    }

    #[test]
    fn minimal_file_applies_defaults() {
        let file = parse_question_set(minimal_value().to_string().as_bytes()).unwrap();
        let q = &file.questions[0];
        assert!(!q.multi_select);
        assert!(q.allow_free_text);
        assert!(q.answer.is_empty());
    }

    #[test]
    fn canonical_writer_sorts_nested_keys_and_escapes_like_serde_json() {
        let value = json!({
            "zeta": {"b": [ {"y": 1, "x": 2}, [] , {} ], "a": "q\"\\\n\t\u{1}/é"},
            "alpha": null,
            "Beta": true
        });
        let mut compact = String::new();
        write_sorted(&mut compact, &value, 0, 0);
        assert_eq!(
            compact,
            "{\"Beta\":true,\"alpha\":null,\"zeta\":{\"a\":\"q\\\"\\\\\\n\\t\\u0001/é\",\"b\":[{\"x\":2,\"y\":1},[],{}]}}"
        );
        let mut ours = String::new();
        write_json_string(&mut ours, "q\"\\\n\t\u{1}\u{8}\u{c}\r/é\u{7f}");
        assert_eq!(
            ours,
            serde_json::to_string("q\"\\\n\t\u{1}\u{8}\u{c}\r/é\u{7f}").unwrap()
        );
        assert_eq!(
            render_sorted_json(&json!({"b": [1, {"d": 2, "c": 3}], "a": {}})),
            "{\n  \"a\": {},\n  \"b\": [\n    1,\n    {\n      \"c\": 3,\n      \"d\": 2\n    }\n  ]\n}\n"
        );
    }

    #[test]
    fn shape_gate_admits_answer_confirm_and_clear() {
        let open = storage_question("gate");
        let by_alice = answered(&open, &alice(), "k1");
        assert_eq!(by_alice.status, QuestionStatus::Answered);
        assert_eq!(by_alice.revision, 2);
        assert_eq!(by_alice.answer.version, 1);
        assert_eq!(by_alice.answer.answered_at_revision, Some(2));
        assert_eq!(by_alice.answer.receipt.as_ref().unwrap().key, "k1");
        assert!(is_decision(&by_alice));

        let changed = answered(&by_alice, &alice(), "k2");
        assert_eq!(changed.revision, 3);
        let cleared = clear(&changed, &alice(), "k3").unwrap();
        assert_eq!(cleared.status, QuestionStatus::Open);
        assert!(cleared.answer.is_empty());
        assert_eq!(
            cleared.answer.receipt.as_ref().unwrap().operation,
            ReceiptOperation::Clear
        );
        assert!(!is_decision(&cleared));

        // An agent proposes; the owner confirms with her own provenance.
        let proposed = answered(&open, &agent(), "k4");
        assert_eq!(proposed.status, QuestionStatus::Proposed);
        assert!(!is_decision(&proposed));
        let confirmed = answered(&proposed, &alice(), "k5");
        assert!(is_decision(&confirmed));
        assert!(clear(&proposed, &bob(), "k6").is_ok());
        assert!(clear(&proposed, &agent(), "k7").is_ok());
        // An agent may replace another proposal.
        assert!(answered(&proposed, &agent(), "k8").status == QuestionStatus::Proposed);
    }

    #[test]
    fn every_shape_gate_error_is_reachable() {
        let open = storage_question("gate");
        let by_alice = answered(&open, &alice(), "k1");
        let mut other = by_alice.clone();
        other.id = "other".to_string();
        assert_eq!(
            answer_shape_gate(&open, &other, &alice())
                .unwrap_err()
                .code(),
            "sequence-changed"
        );
        let mut reworded = by_alice.clone();
        reworded.prompt = "Changed".to_string();
        assert_eq!(
            answer_shape_gate(&open, &reworded, &alice())
                .unwrap_err()
                .code(),
            "non-answer-mutation"
        );
        let mut skipped = by_alice.clone();
        skipped.revision = 3;
        assert_eq!(
            answer_shape_gate(&open, &skipped, &alice())
                .unwrap_err()
                .code(),
            "revision-skip"
        );
        let mut stale_version = by_alice.clone();
        stale_version.answer.version = 0;
        assert_eq!(
            answer_shape_gate(&open, &stale_version, &alice())
                .unwrap_err()
                .code(),
            "answer-version-skip"
        );
        // Another person's decision stays; an agent never clears or replaces it.
        let proposal = proposal_for(&open);
        let receipt = |actor: &Provenance| {
            answer_receipt("k", actor, ReceiptOperation::Answer, 2, Some(&proposal))
        };
        assert_eq!(
            apply_answer_transition(&by_alice, &proposal, &bob(), "t", receipt(&bob()))
                .unwrap_err()
                .code(),
            "foreign-answer-immutable"
        );
        assert_eq!(
            apply_answer_transition(&by_alice, &proposal, &agent(), "t", receipt(&agent()))
                .unwrap_err()
                .code(),
            "foreign-answer-immutable"
        );
        assert_eq!(
            clear(&by_alice, &agent(), "k").unwrap_err().code(),
            "foreign-answer-immutable"
        );
        let mut forged = answered(&open, &bob(), "k");
        forged.answer.answered_by = Some(alice());
        assert_eq!(
            answer_shape_gate(&open, &forged, &bob())
                .unwrap_err()
                .code(),
            "provenance-mismatch"
        );
        // An agent answer recorded as `answered` is refused.
        let mut agent_decides = answered(&open, &agent(), "k");
        agent_decides.status = QuestionStatus::Answered;
        assert_eq!(
            answer_shape_gate(&open, &agent_decides, &agent())
                .unwrap_err()
                .code(),
            "selection-shape"
        );
        let unknown = AnswerProposal {
            selected_option_ids: vec!["nope".to_string()],
            ..AnswerProposal::default()
        };
        assert_eq!(
            apply_answer_transition(&open, &unknown, &alice(), "t", receipt(&alice()))
                .unwrap_err()
                .code(),
            "unknown-option"
        );
        assert_eq!(
            apply_answer_transition(
                &open,
                &AnswerProposal::default(),
                &alice(),
                "t",
                receipt(&alice())
            )
            .unwrap_err()
            .code(),
            "selection-shape"
        );
        let two = AnswerProposal {
            selected_option_ids: vec![open.options[0].id.clone(), open.options[1].id.clone()],
            ..AnswerProposal::default()
        };
        assert_eq!(
            apply_answer_transition(&open, &two, &alice(), "t", receipt(&alice()))
                .unwrap_err()
                .code(),
            "selection-shape"
        );
        let mut multi = open.clone();
        multi.multi_select = true;
        assert!(apply_answer_transition(&multi, &two, &alice(), "t", receipt(&alice())).is_ok());
        let mut no_text = open.clone();
        no_text.allow_free_text = false;
        let text = AnswerProposal {
            text: "free".to_string(),
            ..AnswerProposal::default()
        };
        assert_eq!(
            apply_answer_transition(&no_text, &text, &alice(), "t", receipt(&alice()))
                .unwrap_err()
                .code(),
            "selection-shape"
        );
        let proposed = answered(&open, &agent(), "k");
        assert_eq!(
            clear(&proposed, &Provenance::agent("someone-else"), "k")
                .unwrap_err()
                .code(),
            "clear-not-author"
        );
    }

    #[test]
    fn replay_requires_key_actor_operation_and_payload() {
        let open = storage_question("replay");
        let proposal = proposal_for(&open);
        let receipt = answer_receipt("k1", &alice(), ReceiptOperation::Answer, 1, Some(&proposal));
        let next =
            apply_answer_transition(&open, &proposal, &alice(), "t", receipt.clone()).unwrap();
        assert_eq!(replay_check(&open, &receipt), ReplayCheck::Fresh);
        assert_eq!(replay_check(&next, &receipt), ReplayCheck::Replay);
        let other_payload = AnswerProposal {
            text: "different".to_string(),
            ..proposal.clone()
        };
        let reused = answer_receipt(
            "k1",
            &alice(),
            ReceiptOperation::Answer,
            1,
            Some(&other_payload),
        );
        assert_eq!(replay_check(&next, &reused), ReplayCheck::KeyReused);
        let other_actor =
            answer_receipt("k1", &agent(), ReceiptOperation::Answer, 1, Some(&proposal));
        assert_eq!(replay_check(&next, &other_actor), ReplayCheck::KeyReused);
        let other_op = answer_receipt("k1", &alice(), ReceiptOperation::Clear, 1, None);
        assert_eq!(replay_check(&next, &other_op), ReplayCheck::KeyReused);
        let other_revision =
            answer_receipt("k1", &alice(), ReceiptOperation::Answer, 7, Some(&proposal));
        assert_eq!(replay_check(&next, &other_revision), ReplayCheck::KeyReused);
        assert_eq!(alice().actor_key(), "user:u-alice");
        assert_eq!(agent().actor_key(), "agent:u-alice");
    }

    #[test]
    fn decision_predicate_truth_table() {
        let open = storage_question("decide");
        assert!(!is_decision(&open));
        assert!(is_decision(&answered(&open, &alice(), "k")));
        let proposed = answered(&open, &agent(), "k");
        assert!(!is_decision(&proposed));
        let mut forged = proposed.clone();
        forged.status = QuestionStatus::Answered;
        assert!(!is_decision(&forged));
        let mut relayed = forged.clone();
        relayed.answer.answered_by.as_mut().unwrap().extra.insert(
            "onBehalfOf".to_string(),
            json!({"userId": "u-alice", "sessionId": "s", "chatIntentId": "c"}),
        );
        assert!(!is_decision(&relayed), "onBehalfOf is not carried over");
    }

    #[test]
    fn intrinsic_ids_are_slug_plus_hash() {
        let id = generate_question_id(
            "Which storage backend should the question store use?",
            b"nonce",
        );
        let (slug, hex) = id.rsplit_once('-').unwrap();
        assert_eq!(slug, "which-storage-backend-should-the-question");
        assert_eq!(hex.len(), 8);
        assert_ne!(
            id,
            generate_question_id(
                "Which storage backend should the question store use?",
                b"other"
            )
        );
        assert!(generate_question_id("???", b"n").starts_with("question-"));
        assert_eq!(slugify("  Hello,  World!! Ünïcode "), "hello-world-ünïcode");
        let mut taken = BTreeSet::new();
        assert_eq!(generate_option_id("Set file", &taken), "set-file");
        taken.insert("set-file".to_string());
        assert_eq!(generate_option_id("Set file", &taken), "set-file-2");
        assert_eq!(generate_option_id("!!!", &taken), "option");
    }

    #[test]
    fn revision_gate_both_outcomes_and_rejection() {
        let open = storage_question("rev");
        assert_eq!(
            revision_gate(&open, &open).unwrap(),
            RevisionOutcome::Unchanged
        );
        let by_alice = answered(&open, &alice(), "k");
        let mut reworded = by_alice.clone();
        reworded.prompt = "Which backend should own question answers?".to_string();
        reworded.options[0].description = "Reworded".to_string();
        assert_eq!(
            revision_gate(&by_alice, &reworded).unwrap(),
            RevisionOutcome::NewRevision
        );
        let mut relabeled = by_alice.clone();
        relabeled.options[0].label = "File".to_string();
        assert_eq!(
            revision_gate(&by_alice, &relabeled).unwrap_err().code(),
            "needs-new-question"
        );
        let mut fewer = by_alice.clone();
        fewer.options.pop();
        assert_eq!(
            revision_gate(&by_alice, &fewer).unwrap_err().code(),
            "needs-new-question"
        );
        let mut flipped = by_alice.clone();
        flipped.multi_select = true;
        assert_eq!(
            revision_gate(&by_alice, &flipped).unwrap_err().code(),
            "needs-new-question"
        );
        let mut open_fewer = open.clone();
        open_fewer.options.pop();
        assert_eq!(
            revision_gate(&open, &open_fewer).unwrap(),
            RevisionOutcome::NewRevision
        );
        let mut other = open.clone();
        other.id = "other".to_string();
        assert_eq!(
            revision_gate(&open, &other).unwrap_err().code(),
            "sequence-changed"
        );
    }

    #[test]
    fn path_rules_confine_set_paths() {
        let ws = "9fc80f7a-89c4-4a82-9d3f-5f6eb99aa88a";
        let path = question_set_path(ws, "open-two-options");
        assert_eq!(
            path,
            "docs/9fc80f7a-89c4-4a82-9d3f-5f6eb99aa88a/questions/open-two-options.g5q.json"
        );
        assert_eq!(question_set_dir(ws), format!("docs/{ws}/questions"));
        let parsed = parse_question_set_path(&path).unwrap();
        assert_eq!(parsed.owner_segment, ws);
        assert_eq!(parsed.set_id, "open-two-options");
        // An inherited parent path is as valid as the current workspace's.
        assert!(is_question_set_path("docs/parent-ws/questions/x.g5q.json"));
        for bad in [
            "docs/../questions/x.g5q.json",
            "docs/./questions/x.g5q.json",
            "docs//questions/x.g5q.json",
            "docs/ws/questions/sub/x.g5q.json",
            "docs/ws/questions/Bad.g5q.json",
            "docs/ws/questions/x.json",
            "docs/ws/questions/.g5q.json",
            "docs/ws/questions/../x.g5q.json",
            "/docs/ws/questions/x.g5q.json",
            "docs\\ws\\questions\\x.g5q.json",
            "code/ws/questions/x.g5q.json",
            "docs/ws/other/x.g5q.json",
            "docs/ws/questions/x.g5q.json/",
        ] {
            assert!(!is_question_set_path(bad), "{bad}");
        }
        assert!(parse_set_id(&"a".repeat(64)).is_ok());
        assert!(parse_set_id(&"a".repeat(65)).is_err());
        assert!(is_valid_question_id("pick-color-0123abcd"));
        assert!(!is_valid_question_id(" x"));
        assert!(!is_valid_question_id("a/b"));
        assert!(!is_valid_question_id(""));
    }
}
