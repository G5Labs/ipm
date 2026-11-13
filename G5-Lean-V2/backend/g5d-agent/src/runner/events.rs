//! Provider-neutral turn events (design v6 §8.3 "Output").
//!
//! Both provider mappers (`claude.rs`, `codex.rs`) turn their CLI's JSON
//! frames into these events; the adapters project them onto the worker's
//! existing timeline vocabulary (`provided_agent.*` provider events, the
//! `output`/`thought`/`tool_call`/`tool_result`/`checkpoint` entry types)
//! through [`TimelineProjection`], so the chat UI keeps rendering the same
//! rows it rendered for the Node sidecars.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Token usage reported by a provider for one turn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    /// Provider-reported cost; `None` when the CLI reports no cost (Codex).
    pub total_cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub model: Option<String>,
    /// The provider's stop reason or result subtype, when reported.
    pub finish_reason: Option<String>,
}

impl TurnUsage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cache_read_input_tokens)
    }

    pub fn is_empty(&self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.cache_creation_input_tokens == 0
            && self.cache_read_input_tokens == 0
            && self.total_cost_usd.unwrap_or(0.0) == 0.0
    }
}

/// One item of Claude's live background work: a background shell command
/// (`local_bash`) or a background sub-agent (`local_agent`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub task_id: String,
    pub task_type: String,
    pub description: String,
}

/// Where a Claude background task is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPhase {
    Started,
    Progress,
    Finished,
}

/// One observation from a provider turn.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnEvent {
    /// The provider announced its session (Claude `system/init`, Codex
    /// `thread.started`). Carries the provider session id the next turn
    /// resumes with.
    SessionStarted {
        provider_session_id: String,
        model: Option<String>,
        /// `(name, status)` for every configured MCP server (Claude only).
        mcp_servers: Vec<(String, String)>,
        raw: Value,
    },
    /// A streamed text fragment, optionally tied to its native text block.
    TextDelta {
        text: String,
        unit_id: Option<String>,
    },
    /// A completed assistant text block with its native message/item identity.
    AssistantText { text: String, unit_id: String },
    /// A completed thinking/reasoning block.
    Thinking { text: String },
    /// A tool call was announced.
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
        input: Value,
    },
    /// A tool call finished (its result frame arrived).
    ToolCompleted {
        tool_call_id: String,
        tool_name: String,
        output: Value,
        is_error: bool,
        /// The result frame's content, for image-block harvesting.
        raw_result: Value,
    },
    /// The provider refused a tool under its permission policy.
    PermissionDenied {
        tool_name: String,
        tool_use_id: Option<String>,
        message: Option<String>,
    },
    /// Provider-reported usage for the turn so far.
    Usage { usage: TurnUsage, raw: Value },
    /// Live display only; deliberately has no durable timeline projection.
    SubscriptionUsage {
        observation: crate::subscription_usage::Observation,
    },
    /// The provider's terminal frame for the turn.
    Result {
        text: Option<String>,
        is_error: bool,
        subtype: Option<String>,
        raw: Value,
    },
    /// A later Claude `system/init` on the same live process: the provider
    /// began its own follow-up turn, for example to report finished
    /// background work. The first `init` is [`TurnEvent::SessionStarted`].
    TurnStarted { provider_session_id: String },
    /// Claude's whole live background set (`system/background_tasks_changed`),
    /// replacing any earlier set; empty once nothing remains.
    BackgroundTasks { tasks: Vec<BackgroundTask> },
    /// One step in the lifecycle of a Claude background task or sub-agent
    /// (`system/task_started`, `task_progress`, `task_notification`).
    TaskLifecycle {
        task_id: String,
        phase: TaskPhase,
        /// `local_bash` or `local_agent` when the frame names it.
        task_type: Option<String>,
        description: Option<String>,
        /// Terminal status from `task_notification` (`completed`, `stopped`, …).
        status: Option<String>,
        summary: Option<String>,
    },
    /// A provider-level failure (authentication failure, Codex stream `error`,
    /// or `turn.failed`).
    ProviderError { message: String, raw: Value },
    /// A recoverable provider diagnostic, such as a Codex error *item*.
    ProviderWarning { message: String, raw: Value },
    /// A line the CLI wrote to stderr.
    Stderr { line: String },
    /// A frame the mapper does not interpret; kept for diagnostics.
    Unknown { frame: Value },
}

/// A timeline row derived from one [`TurnEvent`]: the entry type, the
/// `providerEvent` name, the persisted phase, the row content, and extra
/// metadata. Provider-neutral; the adapters add their base metadata and
/// redaction on top.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineProjection {
    pub entry_type: &'static str,
    pub provider_event: &'static str,
    pub phase: &'static str,
    pub content: String,
    pub extra: Value,
}

/// Provider display name used in row content ("Claude Code used Bash").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderLabel {
    pub display: &'static str,
    pub block_prefix: &'static str,
}

impl ProviderLabel {
    pub const CLAUDE: ProviderLabel = ProviderLabel {
        display: "Claude Code",
        block_prefix: "claude-code",
    };
    pub const CODEX: ProviderLabel = ProviderLabel {
        display: "Codex",
        block_prefix: "codex",
    };
}

/// Per-turn counters the projection needs: text/thought segment indices per
/// assistant message.
#[derive(Debug, Default)]
pub struct ProjectionState {
    text_segments: u32,
    thought_segments: u32,
}

impl ProjectionState {
    /// The segment index the next completed text block will take, so live
    /// partial chunks can name the block they precede.
    pub fn next_text_segment(&self) -> u32 {
        self.text_segments.saturating_add(1)
    }

    /// Map one event to at most one timeline row. `message_id` is the
    /// assistant message the turn belongs to (block ids embed it so the UI
    /// can key rows per turn). Text has already been redacted by the caller.
    pub fn project(
        &mut self,
        label: ProviderLabel,
        message_id: &str,
        event: &TurnEvent,
    ) -> Option<TimelineProjection> {
        match event {
            TurnEvent::AssistantText { text, .. } => {
                let text = text.trim();
                if text.is_empty() {
                    return None;
                }
                self.text_segments = self.text_segments.saturating_add(1);
                Some(TimelineProjection {
                    entry_type: "output",
                    provider_event: "provided_agent.output.delta",
                    phase: "output",
                    content: text.to_string(),
                    extra: json!({
                        "blockId": format!("{}-{message_id}-text-{}", label.block_prefix, self.text_segments),
                        "textSegmentIndex": self.text_segments,
                        "streamPersistence": "semantic",
                    }),
                })
            }
            TurnEvent::Thinking { text } => {
                let text = text.trim();
                if text.is_empty() {
                    return None;
                }
                self.thought_segments = self.thought_segments.saturating_add(1);
                Some(TimelineProjection {
                    entry_type: "thought",
                    provider_event: "provided_agent.thought",
                    phase: "thought",
                    content: text.to_string(),
                    extra: json!({
                        "blockId": format!("{}-{message_id}-thought-{}", label.block_prefix, self.thought_segments),
                        "thoughtSegmentIndex": self.thought_segments,
                        "streamPersistence": "semantic",
                    }),
                })
            }
            TurnEvent::ToolStarted {
                tool_call_id,
                tool_name,
                input,
            } => Some(TimelineProjection {
                entry_type: "tool_call",
                provider_event: "provided_agent.tool.started",
                phase: "tool_call",
                content: format!("{} used {tool_name}", label.display),
                extra: json!({
                    "toolName": tool_name,
                    "toolCallId": tool_call_id,
                    "args": input,
                }),
            }),
            TurnEvent::ToolCompleted {
                tool_call_id,
                tool_name,
                output,
                is_error,
                ..
            } => Some(TimelineProjection {
                entry_type: "tool_result",
                provider_event: "provided_agent.tool.completed",
                phase: "tool_result",
                content: format!("{} completed {tool_name}", label.display),
                extra: json!({
                    "toolName": tool_name,
                    "toolCallId": tool_call_id,
                    "isError": is_error,
                    "contentSnippet": value_snippet(output, 1_500),
                }),
            }),
            TurnEvent::PermissionDenied {
                tool_name,
                tool_use_id,
                message,
            } => Some(TimelineProjection {
                entry_type: "tool_result",
                provider_event: "provided_agent.tool.denied",
                phase: "tool_result",
                content: message
                    .clone()
                    .unwrap_or_else(|| format!("{} denied {tool_name}", label.display)),
                extra: json!({
                    "toolName": tool_name,
                    "toolCallId": tool_use_id,
                    "denied": true,
                }),
            }),
            TurnEvent::Usage { raw, .. } => Some(TimelineProjection {
                entry_type: "checkpoint",
                provider_event: "provided_agent.usage.updated",
                phase: "status",
                content: format!("{} usage updated", label.display),
                extra: json!({ "usage": raw }),
            }),
            TurnEvent::ProviderError { message, .. }
            | TurnEvent::ProviderWarning { message, .. } => Some(TimelineProjection {
                entry_type: "checkpoint",
                provider_event: "provided_agent.warning",
                phase: "status",
                content: message.clone(),
                extra: json!({ "visibility": "user" }),
            }),
            TurnEvent::SessionStarted { .. }
            | TurnEvent::SubscriptionUsage { .. }
            | TurnEvent::TurnStarted { .. }
            | TurnEvent::BackgroundTasks { .. }
            | TurnEvent::TaskLifecycle { .. }
            | TurnEvent::TextDelta { .. }
            | TurnEvent::Result { .. }
            | TurnEvent::Stderr { .. }
            | TurnEvent::Unknown { .. } => None,
        }
    }
}

/// Render a tool output as a bounded snippet: strings verbatim, structured
/// values pretty-printed, image blocks removed first (they reach the timeline
/// only as attachment references).
pub fn value_snippet(value: &Value, max_chars: usize) -> Option<String> {
    let value = strip_image_blocks(value);
    let text = match &value {
        Value::Null => return None,
        Value::String(text) => text.clone(),
        other => match text_of_content(other) {
            Some(text) => text,
            None => serde_json::to_string_pretty(other).ok()?,
        },
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(g5_contracts::utils::safe_truncate(trimmed, max_chars).to_string())
}

/// Text carried by Anthropic-style content (a string, a `{"type":"text"}`
/// block, or an array of those).
pub fn text_of_content(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(text_of_content)
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(text)) = map.get("text") {
                return Some(text.clone());
            }
            if let Some(content) = map.get("content") {
                return text_of_content(content);
            }
            None
        }
        _ => None,
    }
}

/// Remove image content blocks from a result's `content` array or a bare
/// content array.
pub fn strip_image_blocks(value: &Value) -> Value {
    let is_image = |block: &Value| block.get("type").and_then(Value::as_str) == Some("image");
    match value {
        Value::Array(items) => {
            Value::Array(items.iter().filter(|b| !is_image(b)).cloned().collect())
        }
        Value::Object(_) => {
            let mut out = value.clone();
            if let Some(items) = out.get_mut("content").and_then(Value::as_array_mut) {
                items.retain(|block| !is_image(block));
            }
            out
        }
        other => other.clone(),
    }
}

/// Extract `(media_type, base64_data)` for every image block in a tool
/// result (Claude `{"source":{"type":"base64",...}}` and MCP-native
/// `{"data","mimeType"}` encodings).
pub fn image_blocks(value: &Value) -> Vec<(String, String)> {
    let items = match value {
        Value::Array(items) => items,
        Value::Object(map) => match map.get("content").and_then(Value::as_array) {
            Some(items) => items,
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("image"))
        .filter_map(|block| {
            if let Some(source) = block.get("source") {
                if source.get("type").and_then(Value::as_str) != Some("base64") {
                    return None;
                }
                let media_type = source
                    .get("media_type")
                    .and_then(Value::as_str)
                    .unwrap_or("image/png")
                    .to_string();
                let data = source.get("data").and_then(Value::as_str)?.to_string();
                return Some((media_type, data));
            }
            let data = block.get("data").and_then(Value::as_str)?.to_string();
            let media_type = block
                .get("mimeType")
                .or_else(|| block.get("mime_type"))
                .and_then(Value::as_str)
                .unwrap_or("image/png")
                .to_string();
            Some((media_type, data))
        })
        .collect()
}

/// Lenient numeric read for usage counters (numbers or numeric strings).
pub fn usage_u64(value: &Value, names: &[&str]) -> u64 {
    names
        .iter()
        .find_map(|name| value.get(*name))
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64().or_else(|| {
                number
                    .as_f64()
                    .filter(|v| *v >= 0.0)
                    .map(|v| v.round() as u64)
            }),
            Value::String(text) => text.trim().parse::<u64>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

pub fn usage_f64(value: &Value, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .find_map(|name| value.get(*name))
        .and_then(|value| match value {
            Value::Number(number) => number.as_f64().filter(|v| v.is_finite() && *v >= 0.0),
            Value::String(text) => text.trim().parse::<f64>().ok().filter(|v| v.is_finite()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_segments_and_thoughts_index_independently() {
        let mut state = ProjectionState::default();
        let first = state
            .project(
                ProviderLabel::CLAUDE,
                "m1",
                &TurnEvent::AssistantText {
                    text: "hello".into(),
                    unit_id: "unit-1".into(),
                },
            )
            .expect("row");
        assert_eq!(first.entry_type, "output");
        assert_eq!(first.provider_event, "provided_agent.output.delta");
        assert_eq!(first.extra["blockId"], "claude-code-m1-text-1");
        let thought = state
            .project(
                ProviderLabel::CLAUDE,
                "m1",
                &TurnEvent::Thinking { text: "hmm".into() },
            )
            .expect("row");
        assert_eq!(thought.entry_type, "thought");
        assert_eq!(thought.extra["blockId"], "claude-code-m1-thought-1");
        let second = state
            .project(
                ProviderLabel::CLAUDE,
                "m1",
                &TurnEvent::AssistantText {
                    text: "again".into(),
                    unit_id: "unit-2".into(),
                },
            )
            .expect("row");
        assert_eq!(second.extra["textSegmentIndex"], 2);
        assert!(state
            .project(
                ProviderLabel::CLAUDE,
                "m1",
                &TurnEvent::AssistantText {
                    text: "  ".into(),
                    unit_id: "unit-3".into()
                }
            )
            .is_none());
    }

    #[test]
    fn tool_rows_carry_names_ids_and_bounded_snippets() {
        let mut state = ProjectionState::default();
        let started = state
            .project(
                ProviderLabel::CODEX,
                "m1",
                &TurnEvent::ToolStarted {
                    tool_call_id: "item_1".into(),
                    tool_name: "command_execution".into(),
                    input: json!({"command": "ls"}),
                },
            )
            .expect("row");
        assert_eq!(started.content, "Codex used command_execution");
        assert_eq!(started.extra["args"]["command"], "ls");
        let completed = state
            .project(
                ProviderLabel::CODEX,
                "m1",
                &TurnEvent::ToolCompleted {
                    tool_call_id: "item_1".into(),
                    tool_name: "command_execution".into(),
                    output: json!("x".repeat(5_000)),
                    is_error: false,
                    raw_result: Value::Null,
                },
            )
            .expect("row");
        assert_eq!(completed.provider_event, "provided_agent.tool.completed");
        assert!(
            completed.extra["contentSnippet"]
                .as_str()
                .expect("snippet")
                .len()
                <= 1_500
        );
    }

    #[test]
    fn image_blocks_are_stripped_from_snippets_but_extracted_for_upload() {
        let result = json!({"content": [
            {"type": "text", "text": "shot"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "AAAA"}},
            {"type": "image", "data": "BBBB", "mimeType": "image/png"}
        ]});
        assert_eq!(value_snippet(&result, 100).as_deref(), Some("shot"));
        assert_eq!(
            image_blocks(&result),
            vec![
                ("image/jpeg".to_string(), "AAAA".to_string()),
                ("image/png".to_string(), "BBBB".to_string())
            ]
        );
    }
}
