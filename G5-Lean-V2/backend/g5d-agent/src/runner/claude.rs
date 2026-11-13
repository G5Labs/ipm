//! Claude Code: stdin message and `stream-json` frame mapper
//! (design v6 §8.3; shapes from `docs/lean-provider-cli-contract.md` §2–§4).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use g5_contracts::types::{AgentAuthModeV1, ProviderAccountKind};
use serde_json::{json, Value};

use super::events::{
    image_blocks, text_of_content, usage_f64, usage_u64, BackgroundTask, TaskPhase, TurnEvent,
    TurnUsage,
};
use super::Attachment;

/// Host-owned model capabilities merge into the caller's settings. Ordinary
/// turns retain workspace/memory settings; formal turns supply isolated settings.
pub(crate) fn apply_model_overrides(model: &str, settings: &mut Value) -> Result<(), &'static str> {
    if let Some(overrides) = g5_contracts::claude_models::claude_native_model_overrides(model) {
        let settings = settings
            .as_object_mut()
            .ok_or("Claude settings must be an object")?;
        let existing = settings
            .entry("modelOverrides")
            .or_insert_with(|| json!({}));
        let existing = existing
            .as_object_mut()
            .ok_or("Claude modelOverrides must be an object")?;
        existing.extend(
            overrides
                .as_object()
                .expect("catalog model overrides are an object")
                .clone(),
        );
    }
    Ok(())
}

pub(crate) fn model_arguments(model: &str, effort: Option<&str>) -> Vec<String> {
    let mut args = vec!["--model".into(), model.into()];
    if let Some(effort) = effort.filter(|effort| *effort != "none") {
        args.extend(["--effort".into(), effort.into()]);
    }
    args
}

/// The single `stream-json` user message written to stdin: text plus one
/// image block per image attachment (contract §2.1).
pub fn build_stdin(prompt: &str, attachments: &[Attachment]) -> Vec<u8> {
    build_stdin_checked(prompt, attachments).expect("Claude attachment materialization failed")
}

/// Active turns must fail if an image cannot be read; an omitted block would
/// falsely claim successful delivery of the reserved attachment.
pub fn build_stdin_checked(prompt: &str, attachments: &[Attachment]) -> std::io::Result<Vec<u8>> {
    let mut blocks = Vec::new();
    for attachment in attachments {
        if let Some(block) = image_block_checked(attachment)? {
            blocks.push(block);
        }
    }
    let content = if blocks.is_empty() {
        Value::String(prompt.to_string())
    } else {
        blocks.push(json!({"type": "text", "text": prompt}));
        Value::Array(blocks)
    };
    let message = json!({
        "type": "user",
        "message": {"role": "user", "content": content},
    });
    let mut line = message.to_string();
    line.push('\n');
    Ok(line.into_bytes())
}

fn image_block_checked(attachment: &Attachment) -> std::io::Result<Option<Value>> {
    let Some(media_type) = attachment.media_type.as_deref() else {
        return Ok(None);
    };
    if !media_type.starts_with("image/") {
        return Ok(None);
    }
    let bytes = std::fs::read(&attachment.path)?;
    use base64::Engine as _;
    Ok(Some(json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": media_type,
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    })))
}

/// Transcript location the CLI writes under `CLAUDE_CONFIG_DIR`
/// (contract §2.1): `projects/<project key>/<session id>.jsonl`, where the
/// project key is the absolute cwd with every character outside
/// `[A-Za-z0-9-]` replaced by `-`.
pub fn project_key_for_cwd(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub fn transcript_path(config_dir: &Path, cwd: &Path, session_id: &str) -> std::path::PathBuf {
    config_dir
        .join("projects")
        .join(project_key_for_cwd(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// Stateful mapper from `stream-json` frames to [`TurnEvent`]s.
#[derive(Debug, Default)]
pub struct ClaudeFrameMapper {
    in_flight_tools: HashMap<String, String>,
    current_message_id: Option<String>,
    assistant_ordinal: u64,
    seen_text_units: HashSet<String>,
    /// Text blocks announced by completed `assistant` frames; the turn's
    /// final text when the `result` frame carries none.
    text_blocks: Vec<String>,
    /// Ids of tool uses already announced, so a repeated completed block
    /// never produces a second `tool_call` row.
    announced_tool_uses: HashSet<String>,
    saw_result: bool,
    /// Whether this process already announced its session; a later `init`
    /// starts one of Claude's own follow-up turns.
    saw_init: bool,
    /// Tool-use ids that launched background tasks. Frames of a background
    /// sub-agent carry its launch id as `parent_tool_use_id`; that work is
    /// reported through the background set and Claude's report turns, not
    /// as the reply.
    background_parents: HashSet<String>,
    /// Ids of tasks Claude runs in the background: started backgrounded or
    /// listed in the background set. Claude 2.1.285 also registers long
    /// foreground commands (its own and its sub-agents') as tasks; their
    /// lifecycle frames are not background work.
    background_task_ids: HashSet<String>,
    /// The launch's frozen auth mode, so an authentication failure names
    /// the account kind whose credential the provider rejected.
    auth_mode: Option<AgentAuthModeV1>,
    /// The launch's base URL, so advice for an API key tells Anthropic's own
    /// host from a compatible gateway with keys of its own shape.
    base_url: Option<String>,
}

impl ClaudeFrameMapper {
    pub fn new() -> Self {
        Self::default()
    }

    /// A mapper that knows the launch's auth mode and base URL (see
    /// [`Self::new`]).
    pub fn with_auth(auth_mode: Option<AgentAuthModeV1>, base_url: Option<&str>) -> Self {
        Self {
            auth_mode,
            base_url: base_url.map(str::to_string),
            ..Self::default()
        }
    }

    pub fn has_in_flight_tool(&self) -> bool {
        !self.in_flight_tools.is_empty()
    }

    pub fn saw_result(&self) -> bool {
        self.saw_result
    }

    pub fn accumulated_text(&self) -> Option<String> {
        let joined = self
            .text_blocks
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        (!joined.is_empty()).then_some(joined)
    }

    /// Map one stdout line. Non-JSON lines and unknown frames become
    /// [`TurnEvent::Unknown`]; the runner ignores them (contract §3: new
    /// frame types appear between releases).
    pub fn map_line(&mut self, line: &str) -> Vec<TurnEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let frame: Value = match serde_json::from_str(trimmed) {
            Ok(frame) => frame,
            Err(_) => {
                return vec![TurnEvent::Unknown {
                    frame: json!({"nonJsonLine": g5_contracts::utils::safe_truncate(trimmed, 1_000)}),
                }]
            }
        };
        self.map_frame(frame)
    }

    pub fn map_frame(&mut self, frame: Value) -> Vec<TurnEvent> {
        if frame
            .get("parent_tool_use_id")
            .and_then(Value::as_str)
            .is_some_and(|parent| self.background_parents.contains(parent))
        {
            return Vec::new();
        }
        match frame.get("type").and_then(Value::as_str) {
            Some("system") => self.map_system(frame),
            Some("assistant") => self.map_assistant(frame),
            Some("user") => self.map_user(frame),
            Some("stream_event") => self.map_stream_event(frame),
            Some("result") => self.map_result(frame),
            Some("rate_limit_event") => crate::subscription_usage::Observation::claude(&frame)
                .map(|observation| vec![TurnEvent::SubscriptionUsage { observation }])
                .unwrap_or_default(),
            _ => vec![TurnEvent::Unknown { frame }],
        }
    }

    fn map_system(&mut self, frame: Value) -> Vec<TurnEvent> {
        match frame.get("subtype").and_then(Value::as_str) {
            Some("init") => self.map_init(frame),
            Some("background_tasks_changed") => {
                let events = map_background_tasks(frame);
                if let [TurnEvent::BackgroundTasks { tasks }] = events.as_slice() {
                    self.background_task_ids
                        .extend(tasks.iter().map(|task| task.task_id.clone()));
                }
                events
            }
            Some("task_started") => {
                if frame.get("is_backgrounded").and_then(Value::as_bool) != Some(true) {
                    return Vec::new();
                }
                if let Some(parent) = frame_text(&frame, "tool_use_id") {
                    self.background_parents.insert(parent);
                }
                if let Some(task_id) = frame_text(&frame, "task_id") {
                    self.background_task_ids.insert(task_id);
                }
                map_task_lifecycle(frame, TaskPhase::Started)
            }
            Some(subtype @ ("task_progress" | "task_notification")) => {
                if !frame_text(&frame, "task_id")
                    .is_some_and(|task_id| self.background_task_ids.contains(&task_id))
                {
                    return Vec::new();
                }
                let phase = if subtype == "task_progress" {
                    TaskPhase::Progress
                } else {
                    TaskPhase::Finished
                };
                map_task_lifecycle(frame, phase)
            }
            _ => vec![TurnEvent::Unknown { frame }],
        }
    }

    fn map_init(&mut self, frame: Value) -> Vec<TurnEvent> {
        let Some(session_id) = frame
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return vec![TurnEvent::Unknown { frame }];
        };
        let mcp_servers = frame
            .get("mcp_servers")
            .and_then(Value::as_array)
            .map(|servers| {
                servers
                    .iter()
                    .filter_map(|server| {
                        Some((
                            server.get("name")?.as_str()?.to_string(),
                            server
                                .get("status")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if self.saw_init {
            return vec![TurnEvent::TurnStarted {
                provider_session_id: session_id.to_string(),
            }];
        }
        self.saw_init = true;
        vec![TurnEvent::SessionStarted {
            provider_session_id: session_id.to_string(),
            model: frame
                .get("model")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            mcp_servers,
            raw: frame,
        }]
    }

    fn map_assistant(&mut self, frame: Value) -> Vec<TurnEvent> {
        let mut events = Vec::new();
        let message = frame.get("message").cloned().unwrap_or(Value::Null);
        if let Some(error) = message
            .get("error")
            .or_else(|| frame.get("error"))
            .and_then(Value::as_str)
            .filter(|_| {
                frame
                    .get("is_api_error_message")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
        {
            let text = message
                .get("content")
                .and_then(text_of_content)
                .unwrap_or_default();
            events.push(TurnEvent::ProviderError {
                message: api_error_message(
                    error,
                    text.trim(),
                    self.auth_mode,
                    self.base_url.as_deref(),
                ),
                raw: frame.clone(),
            });
            return events;
        }
        let blocks = message
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let message_id = message
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(ToString::to_string)
            .or_else(|| self.current_message_id.take())
            .unwrap_or_else(|| {
                self.assistant_ordinal += 1;
                format!("claude-message-{}", self.assistant_ordinal)
            });
        self.current_message_id = None;
        for (index, block) in blocks.into_iter().enumerate() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        if !text.trim().is_empty() {
                            let unit_id = format!("{message_id}:{index}");
                            if self.seen_text_units.insert(unit_id.clone()) {
                                self.text_blocks.push(text.to_string());
                                events.push(TurnEvent::AssistantText {
                                    text: text.to_string(),
                                    unit_id,
                                });
                            }
                        }
                    }
                }
                Some("thinking") => {
                    if let Some(text) = block.get("thinking").and_then(Value::as_str) {
                        if !text.trim().is_empty() {
                            events.push(TurnEvent::Thinking {
                                text: text.to_string(),
                            });
                        }
                    }
                }
                Some("tool_use") => {
                    let tool_call_id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("claude-tool")
                        .to_string();
                    let tool_name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("claude_tool")
                        .to_string();
                    self.in_flight_tools
                        .insert(tool_call_id.clone(), tool_name.clone());
                    // The completed block carries the final arguments: the
                    // stream only announced `input: {}` and then streamed
                    // `input_json_delta` fragments. Announce each tool once,
                    // from here, before the tool runs.
                    if self.announced_tool_uses.insert(tool_call_id.clone()) {
                        events.push(TurnEvent::ToolStarted {
                            tool_call_id,
                            tool_name,
                            input: block.get("input").cloned().unwrap_or(Value::Null),
                        });
                    }
                }
                _ => {}
            }
        }
        events
    }

    fn map_user(&mut self, frame: Value) -> Vec<TurnEvent> {
        let mut events = Vec::new();
        let blocks = frame
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let denial = frame
            .get("tool_result_meta")
            .and_then(Value::as_array)
            .map(|meta| {
                meta.iter().any(|entry| {
                    entry.get("non_execution_kind").and_then(Value::as_str)
                        == Some("permission-rule")
                })
            })
            .unwrap_or(false);
        for block in blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let tool_call_id = block
                .get("tool_use_id")
                .and_then(Value::as_str)
                .unwrap_or("claude-tool")
                .to_string();
            let tool_name = self
                .in_flight_tools
                .remove(&tool_call_id)
                .unwrap_or_else(|| "claude_tool".to_string());
            let output = block.get("content").cloned().unwrap_or(Value::Null);
            let is_error = block
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if denial {
                events.push(TurnEvent::PermissionDenied {
                    tool_name: tool_name.clone(),
                    tool_use_id: Some(tool_call_id.clone()),
                    message: text_of_content(&output),
                });
            }
            events.push(TurnEvent::ToolCompleted {
                tool_call_id,
                tool_name,
                output: output.clone(),
                is_error,
                raw_result: block.clone(),
            });
        }
        events
    }

    fn map_stream_event(&mut self, frame: Value) -> Vec<TurnEvent> {
        let Some(event) = frame.get("event") else {
            return vec![TurnEvent::Unknown { frame }];
        };
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                self.current_message_id = event
                    .get("message")
                    .and_then(|message| message.get("id"))
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .map(ToString::to_string);
                Vec::new()
            }
            Some("content_block_delta") => {
                let delta = event.get("delta");
                if delta.and_then(|d| d.get("type")).and_then(Value::as_str) == Some("text_delta") {
                    if let Some(text) = delta.and_then(|d| d.get("text")).and_then(Value::as_str) {
                        if !text.is_empty() {
                            return vec![TurnEvent::TextDelta {
                                text: text.to_string(),
                                unit_id: self.current_message_id.as_ref().and_then(|id| {
                                    event
                                        .get("index")
                                        .and_then(Value::as_u64)
                                        .map(|index| format!("{id}:{index}"))
                                }),
                            }];
                        }
                    }
                }
                Vec::new()
            }
            // `content_block_start` for a `tool_use` block carries `input: {}`
            // and the arguments arrive as `input_json_delta` fragments. The
            // completed `assistant` frame follows the last fragment and
            // precedes the tool's execution, so tools are announced from that
            // frame only (contract §3.4).
            _ => Vec::new(),
        }
    }

    fn map_result(&mut self, frame: Value) -> Vec<TurnEvent> {
        self.saw_result = true;
        let mut events = Vec::new();
        if let Some(usage) = frame.get("usage") {
            let usage_record = TurnUsage {
                input_tokens: usage_u64(usage, &["input_tokens"]),
                output_tokens: usage_u64(usage, &["output_tokens"]),
                cache_creation_input_tokens: usage_u64(usage, &["cache_creation_input_tokens"]),
                cache_read_input_tokens: usage_u64(usage, &["cache_read_input_tokens"]),
                total_cost_usd: usage_f64(&frame, &["total_cost_usd"]),
                duration_ms: frame.get("duration_ms").and_then(Value::as_u64),
                model: frame
                    .get("modelUsage")
                    .and_then(Value::as_object)
                    .and_then(|models| models.keys().next().cloned()),
                finish_reason: frame
                    .get("stop_reason")
                    .and_then(Value::as_str)
                    .or_else(|| frame.get("subtype").and_then(Value::as_str))
                    .map(ToString::to_string),
            };
            if !usage_record.is_empty() {
                events.push(TurnEvent::Usage {
                    usage: usage_record,
                    raw: usage.clone(),
                });
            }
        }
        for denial in frame
            .get("permission_denials")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            // The denial `user` frame already produced the timeline row; the
            // result list only names tools whose denial frame was missed.
            if denial.get("tool_use_id").and_then(Value::as_str).is_some() {
                continue;
            }
            events.push(TurnEvent::PermissionDenied {
                tool_name: denial
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("claude_tool")
                    .to_string(),
                tool_use_id: None,
                message: None,
            });
        }
        let is_error = frame
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || frame
                .get("subtype")
                .and_then(Value::as_str)
                .is_some_and(|subtype| subtype.starts_with("error"));
        let subtype = frame
            .get("subtype")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let text = frame
            .get("result")
            .and_then(text_of_content)
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        events.push(TurnEvent::Result {
            text,
            is_error,
            subtype,
            raw: frame,
        });
        events
    }
}

/// `system/background_tasks_changed`: Claude's whole live background set.
/// An entry without a usable id still counts as live work, so the set is
/// never reported empty while Claude lists something.
fn map_background_tasks(frame: Value) -> Vec<TurnEvent> {
    let Some(items) = frame.get("tasks").and_then(Value::as_array) else {
        return vec![TurnEvent::Unknown { frame }];
    };
    let tasks = items
        .iter()
        .enumerate()
        .map(|(index, item)| BackgroundTask {
            task_id: frame_text(item, "task_id").unwrap_or_else(|| format!("unnamed-{index}")),
            task_type: frame_text(item, "task_type").unwrap_or_else(|| "unknown".into()),
            description: frame_text(item, "description").unwrap_or_default(),
        })
        .collect();
    vec![TurnEvent::BackgroundTasks { tasks }]
}

/// `system/task_started`, `task_progress` and `task_notification`.
fn map_task_lifecycle(frame: Value, phase: TaskPhase) -> Vec<TurnEvent> {
    let Some(task_id) = frame_text(&frame, "task_id") else {
        return vec![TurnEvent::Unknown { frame }];
    };
    vec![TurnEvent::TaskLifecycle {
        task_id,
        phase,
        task_type: frame_text(&frame, "task_type"),
        description: frame_text(&frame, "description"),
        status: frame_text(&frame, "status"),
        summary: frame_text(&frame, "summary"),
    }]
}

fn frame_text(value: &Value, name: &str) -> Option<String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

/// Image blocks of a tool result, for browser screenshot persistence.
pub fn result_image_blocks(raw_result: &Value) -> Vec<(String, String)> {
    image_blocks(raw_result)
}

/// The user-facing text of an `assistant` frame flagged `is_api_error_message`.
///
/// Claude Code answers every 401 whose body says `invalid x-api-key` with
/// "Not logged in · Please run /login" (`authentication_failed`), advice that
/// does not apply inside a workspace VM, where the credential is the chat's
/// selected account or the runtime configuration's secret. Name the
/// rejected credential kind and where to fix it instead, and drop the login
/// advice; every other code keeps Claude's own text.
fn api_error_message(
    error: &str,
    text: &str,
    auth_mode: Option<AgentAuthModeV1>,
    base_url: Option<&str>,
) -> String {
    if error == "authentication_failed" {
        let on_anthropic_host = ProviderAccountKind::AnthropicApiKey.secret_shape_applies(base_url);
        let fix = match auth_mode {
            Some(AgentAuthModeV1::AnthropicOauth) => {
                "Anthropic rejected the Claude subscription token this chat runs with. \
                 Replace it, in Settings → Accounts or in the runtime configuration's \
                 CLAUDE_CODE_OAUTH_TOKEN, with the token printed by `claude setup-token` \
                 (it starts with sk-ant-oat01-); an Anthropic API key (sk-ant-api03-…) \
                 belongs in an Anthropic API key account."
            }
            Some(AgentAuthModeV1::AnthropicApiKey) if on_anthropic_host => {
                "Anthropic rejected the API key this chat runs with. Check, in Settings → \
                 Accounts or in the runtime configuration's ANTHROPIC_API_KEY, that it is \
                 a current key starting with sk-ant-api03-."
            }
            Some(AgentAuthModeV1::OpenRouterApiKey) => {
                "OpenRouter rejected the API key this chat runs with. Check the OpenRouter \
                 account's key in Settings → Accounts, or the runtime configuration's \
                 OPENROUTER_API_KEY."
            }
            Some(AgentAuthModeV1::AnthropicApiKey | AgentAuthModeV1::AnthropicBearer) => {
                "The endpoint at the configured base URL rejected the credential this chat \
                 runs with. Check the account's key and base URL in Settings → Accounts, or \
                 the runtime configuration's ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY."
            }
            _ => {
                "The provider rejected the credential this chat runs with. Check the \
                 selected account in Settings → Accounts, or the runtime configuration's \
                 provider secret."
            }
        };
        let reported = text
            .replace("Please run /login", "")
            .trim()
            .trim_end_matches('·')
            .trim()
            .to_string();
        let reported = if reported.is_empty() {
            String::new()
        } else {
            format!(" Claude Code reported: {reported}.")
        };
        return format!(
            "Claude Code could not authenticate (authentication_failed). {fix} Then send \
             the message again.{reported}"
        );
    }
    if text.is_empty() {
        format!("Claude Code API error: {error}")
    } else {
        format!("Claude Code API error ({error}): {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_steering_replay_frames_do_not_change_assistant_or_tool_projection() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/provider-frames/claude");
        for name in [
            "tool",
            "text",
            "image",
            "permission",
            "question",
            "background",
            "resume",
        ] {
            let raw = std::fs::read_to_string(root.join(format!("steer-{name}.jsonl"))).unwrap();
            let mut full = ClaudeFrameMapper::new();
            let mut without_replays = ClaudeFrameMapper::new();
            let mut expected = Vec::new();
            let mut actual = Vec::new();
            let mut echoes = 0;
            for line in raw.lines() {
                let frame: Value = serde_json::from_str(line).unwrap();
                if frame["isReplay"] == true {
                    echoes += 1;
                    assert!(
                        full.map_frame(frame).is_empty(),
                        "replay became output in {name}"
                    );
                } else {
                    expected.extend(without_replays.map_frame(frame.clone()));
                    actual.extend(full.map_frame(frame));
                }
            }
            assert!(
                echoes >= 2,
                "fixture {name} must include initial and steered input"
            );
            assert_eq!(actual, expected, "replay disturbed state in {name}");
        }
    }
    #[test]
    fn stdin_is_one_stream_json_user_message() {
        let bytes = build_stdin("hello", &[]);
        let line = String::from_utf8(bytes).unwrap();
        assert!(line.ends_with('\n'));
        let value: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["type"], "user");
        assert_eq!(value["message"]["role"], "user");
        assert_eq!(value["message"]["content"], "hello");
    }

    #[test]
    fn stdin_carries_image_attachments_as_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("shot.png");
        std::fs::write(&image, b"\x89PNG").unwrap();
        let bytes = build_stdin(
            "look",
            &[
                Attachment {
                    path: image,
                    media_type: Some("image/png".into()),
                },
                Attachment {
                    path: dir.path().join("notes.txt"),
                    media_type: Some("text/plain".into()),
                },
            ],
        );
        let value: Value = serde_json::from_slice(bytes.trim_ascii_end()).unwrap();
        let content = value["message"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[1], json!({"type": "text", "text": "look"}));
    }

    #[test]
    fn project_key_matches_the_cli_encoding() {
        assert_eq!(
            project_key_for_cwd(Path::new("/private/tmp/x/G5-Lean-wt-P3.1")),
            "-private-tmp-x-G5-Lean-wt-P3-1"
        );
    }

    #[test]
    fn mapper_preserves_native_message_and_block_identity_through_streaming() {
        let mut mapper = ClaudeFrameMapper::new();
        mapper.map_line(r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"native-message"}}}"#);
        let delta = mapper.map_line(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"part"}}}"#);
        assert!(
            matches!(&delta[0], TurnEvent::TextDelta { text, unit_id: Some(id) } if text == "part" && id == "native-message:1")
        );
        let frame = r#"{"type":"assistant","message":{"id":"native-message","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"complete"}]}}"#;
        let complete = mapper.map_line(frame);
        assert!(
            matches!(&complete[1], TurnEvent::AssistantText { text, unit_id } if text == "complete" && unit_id == "native-message:1")
        );
        assert!(
            mapper
                .map_line(frame)
                .iter()
                .all(|event| !matches!(event, TurnEvent::AssistantText { .. })),
            "a repeated completed message cannot duplicate its text block"
        );
    }

    #[test]
    fn mapper_correlates_tool_results_with_their_names() {
        let mut mapper = ClaudeFrameMapper::new();
        let init = mapper.map_line(r#"{"type":"system","subtype":"init","session_id":"s1","model":"m","mcp_servers":[{"name":"g5-tools","status":"connected"}]}"#);
        assert!(
            matches!(&init[0], TurnEvent::SessionStarted { provider_session_id, mcp_servers, .. } if provider_session_id == "s1" && mcp_servers[0].0 == "g5-tools")
        );
        let started = mapper.map_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Write","input":{"file_path":"a"}}]}}"#);
        assert!(
            matches!(&started[0], TurnEvent::ToolStarted { tool_name, .. } if tool_name == "Write")
        );
        assert!(mapper.has_in_flight_tool());
        let done = mapper.map_line(r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"File created"}]}}"#);
        assert!(
            matches!(&done[0], TurnEvent::ToolCompleted { tool_name, is_error: false, .. } if tool_name == "Write")
        );
        assert!(!mapper.has_in_flight_tool());
    }

    #[test]
    fn mapper_announces_a_streamed_tool_use_once_with_its_final_input() {
        let mut mapper = ClaudeFrameMapper::new();
        let start = mapper.map_line(r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_2","name":"Bash","input":{}}}}"#);
        assert!(
            start.is_empty(),
            "the stream start carries no arguments yet: {start:?}"
        );
        let delta = mapper.map_line(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\": \"ls\"}"}}}"#);
        assert!(delta.is_empty(), "{delta:?}");
        let completed = mapper.map_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_2","name":"Bash","input":{"command":"ls"}}]}}"#);
        assert!(
            matches!(&completed[..], [TurnEvent::ToolStarted { tool_name, input, .. }] if tool_name == "Bash" && input["command"] == "ls"),
            "{completed:?}"
        );
        let repeated = mapper.map_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_2","name":"Bash","input":{"command":"ls"}}]}}"#);
        assert!(
            repeated.is_empty(),
            "a tool use is announced once: {repeated:?}"
        );
    }

    /// Review round 1: replaying the recorded Write turn must keep the tool's
    /// arguments on its one `tool_call` row (the stream start announced
    /// `input: {}`; the completed block carries `file_path` and `content`).
    #[test]
    fn recorded_write_turn_keeps_the_tool_arguments() {
        use crate::runner::events::{ProjectionState, ProviderLabel};
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/provider-frames/claude/write-file.jsonl"
        );
        let frames = std::fs::read_to_string(fixture).expect("recorded claude/write-file.jsonl");
        let mut mapper = ClaudeFrameMapper::new();
        let mut projection = ProjectionState::default();
        let mut tool_rows = Vec::new();
        for line in frames.lines() {
            for event in mapper.map_line(line) {
                if matches!(event, TurnEvent::ToolStarted { .. }) {
                    tool_rows.push(
                        projection
                            .project(ProviderLabel::CLAUDE, "message-1", &event)
                            .expect("tool_call row"),
                    );
                }
            }
        }
        assert_eq!(tool_rows.len(), 1, "one tool_call row: {tool_rows:?}");
        assert_eq!(tool_rows[0].extra["toolName"], "Write");
        let args = &tool_rows[0].extra["args"];
        assert!(
            args["file_path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/hello.txt")),
            "the recorded file_path must survive: {args}"
        );
        assert_eq!(
            args["content"], "hello",
            "the recorded content must survive"
        );
    }

    #[test]
    fn mapper_marks_denials_and_results() {
        let mut mapper = ClaudeFrameMapper::new();
        mapper.map_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_3","name":"Write","input":{}}]}}"#);
        let denied = mapper.map_line(r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_3","type":"tool_result","content":"no","is_error":true}]},"tool_result_meta":[{"id":"toolu_3","non_execution_kind":"permission-rule"}]}"#);
        assert!(
            matches!(&denied[0], TurnEvent::PermissionDenied { tool_name, .. } if tool_name == "Write")
        );
        assert!(matches!(
            &denied[1],
            TurnEvent::ToolCompleted { is_error: true, .. }
        ));
        let result = mapper.map_line(r#"{"type":"result","subtype":"success","is_error":false,"result":"OK","session_id":"s1","total_cost_usd":0.01,"usage":{"input_tokens":3,"output_tokens":2,"cache_creation_input_tokens":1,"cache_read_input_tokens":0},"modelUsage":{"claude-x":{}},"permission_denials":[{"tool_name":"Write","tool_use_id":"toolu_3"}]}"#);
        assert!(
            matches!(&result[0], TurnEvent::Usage { usage, .. } if usage.input_tokens == 3 && usage.total_cost_usd == Some(0.01) && usage.model.as_deref() == Some("claude-x"))
        );
        assert!(
            matches!(&result[1], TurnEvent::Result { text: Some(t), is_error: false, .. } if t == "OK")
        );
        assert!(mapper.saw_result());
    }

    #[test]
    fn mapper_reports_api_errors_and_tolerates_unknown_frames() {
        let mut mapper = ClaudeFrameMapper::new();
        let err = mapper.map_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Not logged in"}],"error":"authentication_failed"},"is_api_error_message":true}"#);
        assert!(
            matches!(&err[0], TurnEvent::ProviderError { message, .. } if message.contains("authentication_failed"))
        );
        let other = mapper.map_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Out of credits"}],"error":"billing_error"},"is_api_error_message":true}"#);
        assert!(matches!(
            &other[0],
            TurnEvent::ProviderError { message, .. }
                if message == "Claude Code API error (billing_error): Out of credits"
        ));
        assert!(mapper.map_line(r#"{"type":"rate_limit_event"}"#).is_empty());
        assert!(matches!(
            &mapper.map_line("not json")[0],
            TurnEvent::Unknown { .. }
        ));
        assert!(mapper.map_line("   ").is_empty());
    }

    #[test]
    fn authentication_failure_names_the_rejected_account_kind() {
        // QA 2026-10-27: an API key pasted into a Claude subscription account
        // made every turn end with Claude's "Please run /login" advice.
        let frame = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Not logged in · Please run /login"}],"error":"authentication_failed"},"is_api_error_message":true}"#;
        let message = |auth_mode, base_url| {
            let mut mapper = ClaudeFrameMapper::with_auth(auth_mode, base_url);
            match mapper.map_line(frame).remove(0) {
                TurnEvent::ProviderError { message, .. } => message,
                other => panic!("expected a provider error, got {other:?}"),
            }
        };
        let oauth = message(Some(AgentAuthModeV1::AnthropicOauth), None);
        assert!(oauth.contains("Claude subscription token"), "{oauth}");
        assert!(oauth.contains("claude setup-token"), "{oauth}");
        assert!(oauth.contains("Settings → Accounts"), "{oauth}");
        assert!(oauth.contains("send the message again"), "{oauth}");
        // Claude's own advice cannot be followed in a VM and is dropped.
        assert!(!oauth.contains("/login"), "{oauth}");
        assert!(
            oauth.ends_with("Claude Code reported: Not logged in."),
            "{oauth}"
        );
        assert!(!oauth.starts_with("Claude Code API error"), "{oauth}");

        let api_key = message(
            Some(AgentAuthModeV1::AnthropicApiKey),
            Some("https://api.anthropic.com"),
        );
        assert!(api_key.contains("API key"), "{api_key}");
        assert!(api_key.contains("sk-ant-api03-"), "{api_key}");
        assert!(!api_key.contains("setup-token"), "{api_key}");

        // Behind a compatible gateway the key has the gateway's own shape.
        let gateway = message(
            Some(AgentAuthModeV1::AnthropicApiKey),
            Some("https://llm.example.test/anthropic"),
        );
        assert!(gateway.contains("configured base URL"), "{gateway}");
        assert!(!gateway.contains("sk-ant-api03-"), "{gateway}");

        let router = message(
            Some(AgentAuthModeV1::OpenRouterApiKey),
            Some("https://openrouter.ai/api"),
        );
        assert!(router.contains("OpenRouter rejected"), "{router}");
        assert!(!router.contains("base URL"), "{router}");
        let bearer = message(
            Some(AgentAuthModeV1::AnthropicBearer),
            Some("https://api.z.ai/api/anthropic"),
        );
        assert!(bearer.contains("configured base URL"), "{bearer}");

        let unknown = message(None, None);
        assert!(
            unknown.contains("Check the selected account in Settings"),
            "{unknown}"
        );
    }

    // Frames below are from Claude Code 2.1.285 on Linux: a background Bash
    // job started in the first turn, then a self-started report turn.
    const INIT_2_1_285: &str = r#"{"type":"system","subtype":"init","session_id":"dd1079ed-0932-4266-ab5e-95d05a20d07d","model":"claude-haiku-4-5-20251001","mcp_servers":[],"claude_code_version":"2.1.285"}"#;

    #[test]
    fn a_later_init_starts_a_follow_up_turn_without_a_new_session() {
        let mut mapper = ClaudeFrameMapper::new();
        assert!(matches!(
            mapper.map_line(INIT_2_1_285).as_slice(),
            [TurnEvent::SessionStarted { provider_session_id, .. }]
                if provider_session_id == "dd1079ed-0932-4266-ab5e-95d05a20d07d"
        ));
        assert_eq!(
            mapper.map_line(INIT_2_1_285),
            vec![TurnEvent::TurnStarted {
                provider_session_id: "dd1079ed-0932-4266-ab5e-95d05a20d07d".into()
            }]
        );
    }

    #[test]
    fn background_set_and_task_lifecycle_frames_are_mapped() {
        let mut mapper = ClaudeFrameMapper::new();
        assert_eq!(
            mapper.map_line(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b1nohmzbo","task_type":"local_bash","description":"python3 -c \"import time; time.sleep(25)\""}],"uuid":"u1","session_id":"s"}"#),
            vec![TurnEvent::BackgroundTasks {
                tasks: vec![BackgroundTask {
                    task_id: "b1nohmzbo".into(),
                    task_type: "local_bash".into(),
                    description: "python3 -c \"import time; time.sleep(25)\"".into(),
                }]
            }]
        );
        assert_eq!(
            mapper.map_line(r#"{"type":"system","subtype":"task_started","task_id":"a8bce50d36d7e4d1c","tool_use_id":"toolu_01","description":"Background sleep task","subagent_type":"general-purpose","is_backgrounded":true,"spawn_depth":1,"task_type":"local_agent","prompt":"run it"}"#),
            vec![TurnEvent::TaskLifecycle {
                task_id: "a8bce50d36d7e4d1c".into(),
                phase: TaskPhase::Started,
                task_type: Some("local_agent".into()),
                description: Some("Background sleep task".into()),
                status: None,
                summary: None,
            }]
        );
        assert_eq!(
            mapper.map_line(r#"{"type":"system","subtype":"task_progress","task_id":"a8bce50d36d7e4d1c","tool_use_id":"toolu_01","description":"Running Sleep for 25 seconds","subagent_type":"general-purpose","usage":{"total_tokens":11072,"tool_uses":1,"duration_ms":2006},"last_tool_name":"Bash"}"#),
            vec![TurnEvent::TaskLifecycle {
                task_id: "a8bce50d36d7e4d1c".into(),
                phase: TaskPhase::Progress,
                task_type: None,
                description: Some("Running Sleep for 25 seconds".into()),
                status: None,
                summary: None,
            }]
        );
        assert_eq!(
            mapper.map_line(r#"{"type":"system","subtype":"task_notification","task_id":"b1nohmzbo","tool_use_id":"toolu_02","status":"completed","output_file":"/tmp/claude-999/x/tasks/b1nohmzbo.output","summary":"Background command completed (exit code 0)"}"#),
            vec![TurnEvent::TaskLifecycle {
                task_id: "b1nohmzbo".into(),
                phase: TaskPhase::Finished,
                task_type: None,
                description: None,
                status: Some("completed".into()),
                summary: Some("Background command completed (exit code 0)".into()),
            }]
        );
        assert_eq!(
            mapper.map_line(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[]}"#),
            vec![TurnEvent::BackgroundTasks { tasks: vec![] }]
        );
        // `task_updated` repeats what the set and the notification carry.
        assert!(matches!(
            mapper.map_line(r#"{"type":"system","subtype":"task_updated","task_id":"b1nohmzbo","patch":{"status":"completed","end_time":1790822464142}}"#).as_slice(),
            [TurnEvent::Unknown { .. }]
        ));
    }

    #[test]
    fn foreground_commands_claude_registers_as_tasks_are_not_background_work() {
        // Recorded from Claude 2.1.285 on Linux (2026-09-30): a long
        // foreground command of Claude's own turn and one of its background
        // sub-agent, both stopped by an interrupt.
        let mut mapper = ClaudeFrameMapper::new();
        for frame in [
            r#"{"type":"system","subtype":"task_started","task_id":"b0x935c9g","tool_use_id":"toolu_012X","description":"Sleep for 30 seconds","is_backgrounded":false,"task_type":"local_bash"}"#,
            r#"{"type":"system","subtype":"task_started","task_id":"bfb2rb4d7","owned_by_subagent":true,"tool_use_id":"toolu_01Vt","description":"Run Python sleep command","is_backgrounded":false,"task_type":"local_bash"}"#,
            r#"{"type":"system","subtype":"task_notification","task_id":"bfb2rb4d7","tool_use_id":"toolu_01Vt","status":"stopped","output_file":"","summary":"Run Python sleep command"}"#,
            r#"{"type":"system","subtype":"task_notification","task_id":"b0x935c9g","tool_use_id":"toolu_012X","status":"stopped","output_file":"","summary":"Sleep for 30 seconds"}"#,
        ] {
            assert_eq!(mapper.map_line(frame), vec![], "{frame}");
        }
        // Their tool output stays part of the reply.
        assert!(!mapper.background_parents.contains("toolu_012X"));
        // A task first seen in the background set is background work.
        mapper.map_line(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b158yoszj","task_type":"local_bash","description":"docker-ci"}]}"#);
        assert_eq!(
            mapper.map_line(r#"{"type":"system","subtype":"task_notification","task_id":"b158yoszj","status":"stopped","summary":"docker-ci"}"#),
            vec![TurnEvent::TaskLifecycle {
                task_id: "b158yoszj".into(),
                phase: TaskPhase::Finished,
                task_type: None,
                description: None,
                status: Some("stopped".into()),
                summary: Some("docker-ci".into()),
            }]
        );
    }

    #[test]
    fn a_background_entry_without_an_id_still_counts_as_live_work() {
        let mut mapper = ClaudeFrameMapper::new();
        assert_eq!(
            mapper.map_line(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"description":"unnamed"}]}"#),
            vec![TurnEvent::BackgroundTasks {
                tasks: vec![BackgroundTask {
                    task_id: "unnamed-0".into(),
                    task_type: "unknown".into(),
                    description: "unnamed".into(),
                }]
            }]
        );
        // Without a task list the set is unknown, not empty.
        assert!(matches!(
            mapper
                .map_line(r#"{"type":"system","subtype":"background_tasks_changed"}"#)
                .as_slice(),
            [TurnEvent::Unknown { .. }]
        ));
    }
}
