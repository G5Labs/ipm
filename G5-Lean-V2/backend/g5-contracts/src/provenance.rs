//! Shared intent-record provenance classification helpers.
//!
//! These pure functions derive author-type, actor display, and timestamp
//! provenance from an [`IntentRecord`]'s persisted metadata. They implement the
//! authorship precedence defined by `system_invariants/grounding_identity_and_routing`
//! (D16: persisted typed provenance is authoritative; legacy rows fall back to
//! string heuristics; unattributable rows render `unknown`, never `human`).
//!
//! Extracted from `g5-api`'s `intent_log_service` so the Source Control Diff
//! Graph "Human intent" lens and the Intent Log surface share one
//! implementation instead of diverging.

use chrono::{DateTime, Utc};

use crate::types::grounding::IntentRecord;

/// Classify the author type of an intent record.
///
/// Returns one of `"human" | "agent" | "supervisor_agent" | "system" |
/// "unknown"`. Persisted typed provenance (`created_actor.actor_kind`) is
/// authoritative; only legacy rows without typed provenance fall back to the
/// string heuristic, and a row with no resolvable provenance renders `unknown`.
pub fn classify_author_type(record: &IntentRecord) -> String {
    // Persisted typed provenance is authoritative
    // (`grounding_identity_and_routing` D16; intent_log_service spec D3):
    // string-marker heuristics never override a typed actor_kind.
    if let Some(actor) = record.created_actor.as_ref() {
        return match actor.actor_kind.as_str() {
            "user" => "human".to_string(),
            "supervisor_agent" => "supervisor_agent".to_string(),
            "system" => "system".to_string(),
            "unknown" => "unknown".to_string(),
            // "agent" plus any future kind classifies as agent rather than
            // silently promoting to human.
            _ => "agent".to_string(),
        };
    }
    // Legacy rows without typed fields fall back to the string heuristic.
    if record_author_values(record).any(is_supervisor_author_value) {
        return "supervisor_agent".to_string();
    }
    // Issue #1752 / O3 disposition: a legacy `system@g5.dev` display value is
    // unattributable until the historical-repair PR restores evidence-backed
    // typing — it renders `unknown`, never `human` (RED-4), and never claims
    // `agent` authorship it cannot prove.
    if record_author_values(record).any(is_legacy_system_author_value) {
        return "unknown".to_string();
    }
    if !record.is_human || record_author_values(record).any(is_agent_author_value) {
        return "agent".to_string();
    }
    // A legacy row with no resolvable provenance at all renders `Unknown`,
    // never `human` (`grounding_identity_and_routing` D16).
    if record
        .created_by
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
        && record
            .last_updated_by
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return "unknown".to_string();
    }
    "human".to_string()
}

/// Classify an author-type from a raw git-blame author string.
pub fn classify_blame_author_type(author: &str) -> String {
    if is_supervisor_author_value(author) {
        "supervisor_agent".to_string()
    } else if is_legacy_system_author_value(author) {
        "unknown".to_string()
    } else if is_agent_author_value(author) {
        "agent".to_string()
    } else if author.trim().is_empty() {
        "unknown".to_string()
    } else {
        "human".to_string()
    }
}

/// Resolve the display actor for a record and its provenance source.
///
/// Returns `(actor_display, source)` where `source` is `"record"` when a
/// display value was found on the record, else `"unknown"`.
pub fn record_actor(record: &IntentRecord) -> (Option<String>, String) {
    match record
        .created_by
        .clone()
        .or_else(|| record.last_updated_by.clone())
    {
        Some(a) => (Some(a), "record".to_string()),
        None => (None, "unknown".to_string()),
    }
}

/// Parse either ISO-8601 spelling (`...+00:00` µs or `...Z` s) to UTC.
pub fn parse_iso(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn is_legacy_system_author_value(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("system@g5.dev")
}

fn record_author_values(record: &IntentRecord) -> impl Iterator<Item = &str> {
    record
        .created_by
        .as_deref()
        .into_iter()
        .chain(record.last_updated_by.as_deref())
        .chain(record.chat_context.as_deref())
}

fn is_supervisor_author_value(value: &str) -> bool {
    value
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("human_like_agent:")
}

fn is_agent_author_value(value: &str) -> bool {
    const AGENT_MARKERS: &[&str] = &[
        "agent",
        "anthropic",
        "assistant",
        "claude",
        "codex",
        "gemini",
        "gpt",
        "openai",
        "opus",
        "sonnet",
    ];
    let lower = value.to_ascii_lowercase();
    AGENT_MARKERS.iter().any(|marker| lower.contains(marker))
}
