use serde::{Deserialize, Serialize};

/// Classification of a source file for grounding directory placement.
///
/// Maps to sub-folders under `intent_record/` and `intent_grounding/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    View,
    Ontology,
    Code,
    Chat,
    /// `external_design_doc` is the legacy on-disk spelling (the directory name
    /// and the `file_type` field stored in committed records); accept it on
    /// deserialize so external-doc intent records actually parse. Serialization
    /// still emits the canonical `external_doc`.
    #[serde(alias = "external_design_doc")]
    ExternalDoc,
}

impl FileType {
    /// Returns the sub-folder name used under `intent_record/` and `intent_grounding/`.
    pub fn subfolder(&self) -> &'static str {
        match self {
            FileType::View => "views",
            FileType::Ontology => "ontology",
            FileType::Code => "code",
            FileType::Chat => "chat",
            FileType::ExternalDoc => "external",
        }
    }

    /// Parse from the string values used in MCP tool parameters.
    pub fn from_str_loose(s: &str) -> Option<FileType> {
        match s.to_lowercase().as_str() {
            "view" => Some(FileType::View),
            "ontology" => Some(FileType::Ontology),
            "code" => Some(FileType::Code),
            "chat" => Some(FileType::Chat),
            "external_doc" | "external doc" => Some(FileType::ExternalDoc),
            _ => None,
        }
    }
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileType::View => write!(f, "view"),
            FileType::Ontology => write!(f, "ontology"),
            FileType::Code => write!(f, "code"),
            FileType::Chat => write!(f, "chat"),
            FileType::ExternalDoc => write!(f, "external_doc"),
        }
    }
}

/// A single intent record stored as one line in a JSONL file.
///
/// Natural key (used for identity and deduplication):
/// - view/ontology/external_doc: `(file_type, file_name, intent_record_number)`
/// - code: `(file_type, file_name, code_block_name)`
/// - chat: `(file_type, chat_session_id, intent_record_number)` — content hash dedup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRecord {
    pub file_type: FileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_record_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_block_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Drift-metadata hash of the canonical extracted block content (first 16
    /// lowercase hex of SHA-256, `block_extraction::content_fingerprint`).
    /// Non-chat rows persist this instead of the block body per
    /// `grounding_identity_and_routing` D10; a non-chat row without it is a
    /// legacy "unhashed" row, refreshed on next registration. Never serves as
    /// display content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_fingerprint: Option<String>,
    #[serde(default)]
    pub is_human: bool,
    /// Persisted canonical `uncertain` metadata. Omitted when false so legacy
    /// rows and untagged records retain their compact JSONL shape.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_uncertain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certainty: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated_at: Option<String>,
    /// Typed creation provenance beside the legacy `created_by` display string
    /// (`grounding_identity_and_routing` D13; types/grounding spec D9).
    /// Absent on legacy rows; never part of the natural key or chat hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_actor: Option<ProvenanceActor>,
    /// Typed last-refresh provenance beside `last_updated_by`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_actor: Option<ProvenanceActor>,
    /// Executing-agent identity persisted when an agent writes during a
    /// user's job (`grounding_identity_and_routing` D14): the user is the
    /// author, the agent is the executor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed_by: Option<ProvenanceActor>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// A directed grounding edge between two intent records.
///
/// Stored as one line per edge in JSONL files under `intent_grounding/`.
/// Natural key for deduplication: `(source_file, target_file, source_id, target_id)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingEdge {
    pub source_file_type: FileType,
    pub target_file_type: FileType,
    pub source_file: String,
    pub target_file: String,
    pub source_id: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Typed creation provenance beside the legacy `created_by` display string
    /// (`grounding_identity_and_routing` D13). Absent on legacy edges; never
    /// part of the edge natural key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_actor: Option<ProvenanceActor>,
    /// Executing-agent identity persisted when an agent writes during a
    /// user's job (`grounding_identity_and_routing` D14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed_by: Option<ProvenanceActor>,
}

// ── Typed provenance actor (grounding_identity_and_routing D13–D16) ─────

/// Closed actor-kind vocabulary (spec types/grounding D9).
pub const ALLOWED_ACTOR_KINDS: [&str; 5] =
    ["user", "agent", "supervisor_agent", "system", "unknown"];

/// Closed actor-source vocabulary (spec types/grounding D9).
pub const ALLOWED_ACTOR_SOURCES: [&str; 4] =
    ["platform_auth", "local_git", "explicit_system", "unknown"];

/// Validate an actor kind against the canonical vocabulary.
pub fn validate_actor_kind(kind: &str) -> Result<(), String> {
    if ALLOWED_ACTOR_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(format!(
            "invalid actor_kind '{kind}' (allowed: {})",
            ALLOWED_ACTOR_KINDS.join(", ")
        ))
    }
}

/// Validate an actor source against the canonical vocabulary.
pub fn validate_actor_source(source: &str) -> Result<(), String> {
    if ALLOWED_ACTOR_SOURCES.contains(&source) {
        Ok(())
    } else {
        Err(format!(
            "invalid actor source '{source}' (allowed: {})",
            ALLOWED_ACTOR_SOURCES.join(", ")
        ))
    }
}

/// Typed writer identity stamped on new intent records and grounding edges
/// beside the legacy `created_by` display string.
///
/// Realizes `grounding_identity_and_routing` D13: `actor_kind` and `source`
/// are drawn from closed, validated vocabularies; `actor_id` is the stable
/// identity key (a user's UUID as a string, or an agent/system slug);
/// `display` is the canonical email or display name — audit evidence, not
/// the identity key. Provenance only: these fields never enter natural keys,
/// dedup, merge comparison identity, or the chat content hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceActor {
    pub actor_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    pub source: String,
}

impl ProvenanceActor {
    /// An authenticated platform user (`grounding_identity_and_routing` D14):
    /// `actor_id` = stable user UUID, `display` = canonical email.
    pub fn user(actor_id: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            actor_kind: "user".to_string(),
            actor_id: Some(actor_id.into()),
            display: Some(email.into()),
            source: "platform_auth".to_string(),
        }
    }

    /// An executing agent identity (executor field, or autonomous author).
    pub fn agent(slug: impl Into<String>) -> Self {
        Self {
            actor_kind: "agent".to_string(),
            actor_id: Some(slug.into()),
            display: None,
            source: "platform_auth".to_string(),
        }
    }

    /// A legitimate system operation with no initiating user
    /// (`grounding_identity_and_routing` D15): always explicitly asserted,
    /// never inferred from repo git config.
    pub fn system(slug: impl Into<String>) -> Self {
        Self {
            actor_kind: "system".to_string(),
            actor_id: Some(slug.into()),
            display: None,
            source: "explicit_system".to_string(),
        }
    }

    /// Validate the closed vocabularies and their trust-source pairings.
    pub fn validate(&self) -> Result<(), String> {
        validate_actor_kind(&self.actor_kind)?;
        validate_actor_source(&self.source)?;
        let valid_pair = match self.actor_kind.as_str() {
            "user" => matches!(self.source.as_str(), "platform_auth" | "local_git"),
            "agent" | "supervisor_agent" => self.source == "platform_auth",
            "system" => self.source == "explicit_system",
            "unknown" => self.source == "unknown",
            _ => false,
        };
        if valid_pair {
            return Ok(());
        }
        if self.actor_kind == "system" {
            return Err("system actors must use source 'explicit_system'".to_string());
        }
        Err(format!(
            "{} actors must use a compatible source (got '{}')",
            self.actor_kind, self.source
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsolete_review_fields_do_not_reject_historical_intent_records() {
        for obsolete in [
            serde_json::json!(null),
            serde_json::json!({"schema_version": 999}),
            serde_json::json!("malformed legacy assessment"),
        ] {
            let row = serde_json::json!({
                "file_type": "external_doc", "file_name": "docs/workspace/empty.md",
                "intent_record_number": "Q1", "content": "Keep this history", "is_human": true,
                "review_metadata": obsolete, "review_action_state": obsolete
            });
            let record: IntentRecord = serde_json::from_value(row).unwrap();
            assert_eq!(record.content.as_deref(), Some("Keep this history"));
            assert_eq!(record.intent_record_number.as_deref(), Some("Q1"));
            assert!(record.is_human);
            let encoded = serde_json::to_value(record).unwrap();
            assert!(encoded.get("review_metadata").is_none());
            assert!(encoded.get("review_action_state").is_none());
        }
    }

    #[test]
    fn file_type_accepts_legacy_external_design_doc_alias() {
        assert_eq!(
            serde_json::from_str::<FileType>("\"external_design_doc\"").unwrap(),
            FileType::ExternalDoc
        );
        assert_eq!(
            serde_json::from_str::<FileType>("\"external_doc\"").unwrap(),
            FileType::ExternalDoc
        );
        // Serialization still emits the canonical snake_case spelling.
        assert_eq!(
            serde_json::to_string(&FileType::ExternalDoc).unwrap(),
            "\"external_doc\""
        );
    }

    #[test]
    fn intent_record_parses_real_external_design_doc_shape() {
        // Mirrors a committed external-doc record (legacy file_type spelling).
        let line = r#"{"file_type":"external_design_doc","file_name":"designs/foo.md","intent_record_number":"dd1","content":"x","is_human":true,"created_by":"Ziniu","created_at":"2026-05-07T23:00:20Z"}"#;
        let rec: IntentRecord = serde_json::from_str(line).expect("must parse");
        assert_eq!(rec.file_type, FileType::ExternalDoc);
        assert!(rec.is_human);
        assert_eq!(rec.file_name.as_deref(), Some("designs/foo.md"));
    }

    #[test]
    fn provenance_actor_vocabularies_are_closed() {
        for kind in ALLOWED_ACTOR_KINDS {
            assert!(validate_actor_kind(kind).is_ok());
        }
        for source in ALLOWED_ACTOR_SOURCES {
            assert!(validate_actor_source(source).is_ok());
        }
        assert!(validate_actor_kind("robot")
            .unwrap_err()
            .contains("invalid actor_kind 'robot'"));
        assert!(validate_actor_source("git")
            .unwrap_err()
            .contains("invalid actor source 'git'"));

        let mut actor = ProvenanceActor::user("11111111-2222-3333-4444-555555555555", "u@x.dev");
        actor.validate().unwrap();
        actor.actor_kind = "robot".to_string();
        assert!(actor.validate().is_err());

        let mut system = ProvenanceActor::system("bootstrap");
        system.source = "local_git".to_string();
        assert!(system
            .validate()
            .unwrap_err()
            .contains("system actors must use source 'explicit_system'"));
    }

    #[test]
    fn provenance_actor_constructors() {
        let user = ProvenanceActor::user("uuid-1", "user@example.com");
        assert_eq!(user.actor_kind, "user");
        assert_eq!(user.source, "platform_auth");

        let agent = ProvenanceActor::agent("claude_code");
        assert_eq!(agent.actor_kind, "agent");

        let system = ProvenanceActor::system("bootstrap");
        assert_eq!(system.source, "explicit_system");
    }

    #[test]
    fn typed_actor_fields_round_trip_and_stay_absent_on_legacy_rows() {
        // Legacy row bytes (no typed fields) deserialize with all actors None
        // and re-serialize without inventing the new fields.
        let legacy = r#"{"file_type":"ontology","file_name":"specs/a.semlift.md","intent_record_number":"I1","is_human":false,"created_by":"user@example.com","created_at":"2026-05-07T23:00:20Z"}"#;
        let record: IntentRecord = serde_json::from_str(legacy).unwrap();
        assert!(record.created_actor.is_none());
        assert!(record.updated_actor.is_none());
        assert!(record.executed_by.is_none());
        let encoded = serde_json::to_string(&record).unwrap();
        assert!(!encoded.contains("created_actor"));
        assert!(!encoded.contains("updated_actor"));
        assert!(!encoded.contains("executed_by"));

        // A typed row round-trips both actor and executor.
        let mut typed = record.clone();
        typed.created_actor = Some(ProvenanceActor::user("uuid-1", "user@example.com"));
        typed.executed_by = Some(ProvenanceActor::agent("claude_code"));
        let encoded = serde_json::to_string(&typed).unwrap();
        assert!(encoded.contains("\"actor_kind\":\"user\""));
        assert!(encoded.contains("\"source\":\"platform_auth\""));
        let back: IntentRecord = serde_json::from_str(&encoded).unwrap();
        assert_eq!(back.created_actor, typed.created_actor);
        assert_eq!(back.executed_by, typed.executed_by);

        // Legacy edge bytes likewise deserialize/serialize untouched.
        let legacy_edge = r#"{"source_file_type":"ontology","target_file_type":"code","source_file":"specs/a.semlift.md","target_file":"src/a.rs","source_id":"I1","target_id":"foo"}"#;
        let edge: GroundingEdge = serde_json::from_str(legacy_edge).unwrap();
        assert!(edge.created_actor.is_none());
        assert!(edge.executed_by.is_none());
        assert!(!serde_json::to_string(&edge)
            .unwrap()
            .contains("created_actor"));
    }
}
