//! Codex item-event translation shared by the App Server adapter, which feeds
//! it frames in this item vocabulary (originally `codex exec --json`, design
//! v6 §8.3; shapes from `docs/lean-provider-cli-contract.md` §5). G5 no
//! longer launches exec.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::events::{usage_u64, TurnEvent, TurnUsage};

/// Stateful mapper from Codex item frames to [`TurnEvent`]s.
#[derive(Debug, Default)]
pub struct CodexFrameMapper {
    in_flight_items: HashMap<String, String>,
    seen_messages: HashSet<String>,
    missing_item_ordinal: u64,
    agent_messages: Vec<String>,
    saw_turn_completed: bool,
    saw_turn_failed: bool,
}

impl CodexFrameMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_in_flight_tool(&self) -> bool {
        !self.in_flight_items.is_empty()
    }

    pub fn saw_turn_completed(&self) -> bool {
        self.saw_turn_completed
    }

    pub fn saw_turn_failed(&self) -> bool {
        self.saw_turn_failed
    }

    /// The last `agent_message` of the turn, the assistant's answer.
    pub fn final_text(&self) -> Option<String> {
        self.agent_messages
            .iter()
            .rev()
            .map(|t| t.trim())
            .find(|t| !t.is_empty())
            .map(ToString::to_string)
    }

    pub fn map_frame(&mut self, frame: Value) -> Vec<TurnEvent> {
        match frame.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                let Some(thread_id) = frame
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                else {
                    return vec![TurnEvent::Unknown { frame }];
                };
                vec![TurnEvent::SessionStarted {
                    provider_session_id: thread_id.to_string(),
                    model: None,
                    mcp_servers: Vec::new(),
                    raw: frame,
                }]
            }
            Some("item.started") => self.map_item(frame, false),
            Some("item.completed") => self.map_item(frame, true),
            Some("turn.completed") | Some("task_complete") | Some("turn_complete") => {
                self.saw_turn_completed = true;
                let mut events = Vec::new();
                if let Some(usage) = frame.get("usage") {
                    let record = TurnUsage {
                        input_tokens: usage_u64(usage, &["input_tokens"]),
                        output_tokens: usage_u64(usage, &["output_tokens"]),
                        cache_creation_input_tokens: usage_u64(
                            usage,
                            &["cache_write_input_tokens"],
                        ),
                        cache_read_input_tokens: usage_u64(usage, &["cached_input_tokens"]),
                        total_cost_usd: None,
                        duration_ms: None,
                        model: None,
                        finish_reason: None,
                    };
                    if !record.is_empty() {
                        events.push(TurnEvent::Usage {
                            usage: record,
                            raw: usage.clone(),
                        });
                    }
                }
                events.push(TurnEvent::Result {
                    text: self.final_text(),
                    is_error: false,
                    subtype: Some("turn.completed".into()),
                    raw: frame,
                });
                events
            }
            Some("turn.failed") => {
                self.saw_turn_failed = true;
                let message = error_message(frame.get("error"))
                    .unwrap_or_else(|| "Codex turn failed".to_string());
                vec![
                    TurnEvent::ProviderError {
                        message: message.clone(),
                        raw: frame.clone(),
                    },
                    TurnEvent::Result {
                        text: self.final_text(),
                        is_error: true,
                        subtype: Some("turn.failed".into()),
                        raw: frame,
                    },
                ]
            }
            Some("error") => {
                let message = error_message(frame.get("error"))
                    .or_else(|| {
                        frame
                            .get("message")
                            .and_then(Value::as_str)
                            .map(String::from)
                    })
                    .unwrap_or_else(|| "Codex error".to_string());
                vec![TurnEvent::ProviderError {
                    message,
                    raw: frame,
                }]
            }
            _ => vec![TurnEvent::Unknown { frame }],
        }
    }

    fn map_item(&mut self, frame: Value, completed: bool) -> Vec<TurnEvent> {
        let Some(item) = frame.get("item").cloned() else {
            return vec![TurnEvent::Unknown { frame }];
        };
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
        let item_id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("item")
            .to_string();
        match item_type {
            "agent_message" => {
                if !completed {
                    return Vec::new();
                }
                let Some(text) = item.get("text").and_then(Value::as_str) else {
                    return Vec::new();
                };
                if text.trim().is_empty() {
                    return Vec::new();
                }
                let unit_id = if item_id == "item" {
                    self.missing_item_ordinal += 1;
                    format!("codex-item-{}", self.missing_item_ordinal)
                } else {
                    item_id
                };
                if !self.seen_messages.insert(unit_id.clone()) {
                    return Vec::new();
                }
                self.agent_messages.push(text.to_string());
                vec![TurnEvent::AssistantText {
                    text: text.to_string(),
                    unit_id,
                }]
            }
            "reasoning" => {
                if !completed {
                    return Vec::new();
                }
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .or_else(|| summary_text(item.get("summary")));
                match text.filter(|t| !t.trim().is_empty()) {
                    Some(text) => vec![TurnEvent::Thinking { text }],
                    None => Vec::new(),
                }
            }
            "error" => {
                let message = item
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex reported an error item")
                    .to_string();
                // Codex's exec contract defines error items as non-fatal.
                // Custom-provider model metadata warnings use this shape;
                // stream errors and turn.failed remain failures above.
                vec![TurnEvent::ProviderWarning {
                    message,
                    raw: frame,
                }]
            }
            "command_execution" | "mcp_tool_call" | "file_change" | "web_search" | "tool_call"
            | "function_call" | "patch" | "apply_patch" | "plan_update" | "todo_list" => {
                let tool_name = tool_name(item_type, &item);
                let input = tool_input(item_type, &item);
                let mut events = Vec::new();
                let status = item
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let terminal =
                    completed || matches!(status.as_str(), "completed" | "failed" | "error");
                if !self.in_flight_items.contains_key(&item_id) {
                    self.in_flight_items
                        .insert(item_id.clone(), tool_name.clone());
                    events.push(TurnEvent::ToolStarted {
                        tool_call_id: item_id.clone(),
                        tool_name: tool_name.clone(),
                        input: input.clone(),
                    });
                }
                if terminal {
                    self.in_flight_items.remove(&item_id);
                    let output = item
                        .get("aggregated_output")
                        .or_else(|| item.get("output"))
                        .or_else(|| item.get("result"))
                        .cloned()
                        .or_else(|| item.get("error").filter(|e| !e.is_null()).cloned())
                        .unwrap_or(Value::Null);
                    let is_error = matches!(status.as_str(), "failed" | "error")
                        || item.get("error").is_some_and(|e| !e.is_null())
                        || item
                            .get("exit_code")
                            .and_then(Value::as_i64)
                            .is_some_and(|code| code != 0);
                    events.push(TurnEvent::ToolCompleted {
                        tool_call_id: item_id,
                        tool_name,
                        output,
                        is_error,
                        raw_result: item.get("result").cloned().unwrap_or(Value::Null),
                    });
                }
                events
            }
            _ => vec![TurnEvent::Unknown { frame }],
        }
    }
}

fn tool_name(item_type: &str, item: &Value) -> String {
    match item_type {
        "mcp_tool_call" => {
            let server = item.get("server").and_then(Value::as_str).unwrap_or("mcp");
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("tool");
            format!("mcp__{server}__{tool}")
        }
        _ => item
            .get("name")
            .or_else(|| item.get("tool"))
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(item_type)
            .to_string(),
    }
}

fn tool_input(item_type: &str, item: &Value) -> Value {
    match item_type {
        "command_execution" => {
            json!({"command": item.get("command").cloned().unwrap_or(Value::Null)})
        }
        "mcp_tool_call" => item.get("arguments").cloned().unwrap_or(json!({})),
        "file_change" => json!({"changes": item.get("changes").cloned().unwrap_or(Value::Null)}),
        "web_search" => json!({"query": item.get("query").cloned().unwrap_or(Value::Null)}),
        _ => item
            .get("input")
            .or_else(|| item.get("arguments"))
            .cloned()
            .unwrap_or_else(|| item.clone()),
    }
}

fn summary_text(value: Option<&Value>) -> Option<String> {
    let mut parts = Vec::new();
    fn collect(value: &Value, parts: &mut Vec<String>) {
        match value {
            Value::String(text) if !text.trim().is_empty() => parts.push(text.trim().to_string()),
            Value::Array(items) => items.iter().take(20).for_each(|item| collect(item, parts)),
            Value::Object(map) => {
                if let Some(Value::String(text)) = map.get("text") {
                    if !text.trim().is_empty() {
                        parts.push(text.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    collect(value?, &mut parts);
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn error_message(error: Option<&Value>) -> Option<String> {
    let error = error?;
    if let Some(message) = error.get("message").and_then(Value::as_str) {
        return Some(message.to_string());
    }
    error.as_str().map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(mapper: &mut CodexFrameMapper, frame: &str) -> Vec<TurnEvent> {
        mapper.map_frame(serde_json::from_str(frame).unwrap())
    }

    #[test]
    fn mapper_tracks_session_items_and_completion() {
        let mut mapper = CodexFrameMapper::new();
        let started = line(&mut mapper, r#"{"type":"thread.started","thread_id":"t1"}"#);
        assert!(
            matches!(&started[0], TurnEvent::SessionStarted { provider_session_id, .. } if provider_session_id == "t1")
        );
        assert!(line(&mut mapper, r#"{"type":"turn.started"}"#)
            .iter()
            .all(|e| matches!(e, TurnEvent::Unknown { .. })));
        let cmd = line(
            &mut mapper,
            r#"{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"bash -lc ls","status":"in_progress"}}"#,
        );
        assert!(
            matches!(&cmd[0], TurnEvent::ToolStarted { tool_name, input, .. } if tool_name == "command_execution" && input["command"] == "bash -lc ls")
        );
        assert!(mapper.has_in_flight_tool());
        let done = line(
            &mut mapper,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"bash -lc ls","aggregated_output":"a\n","exit_code":0,"status":"completed"}}"#,
        );
        assert!(
            matches!(&done[0], TurnEvent::ToolCompleted { is_error: false, output, .. } if output == "a\n")
        );
        assert!(!mapper.has_in_flight_tool());
        let mcp = line(
            &mut mapper,
            r#"{"type":"item.started","item":{"id":"item_1","type":"mcp_tool_call","server":"g5-tools","tool":"fetch_all_intent","arguments":{},"status":"in_progress"}}"#,
        );
        assert!(
            matches!(&mcp[0], TurnEvent::ToolStarted { tool_name, .. } if tool_name == "mcp__g5-tools__fetch_all_intent")
        );
        let mcp_failed = line(
            &mut mapper,
            r#"{"type":"item.completed","item":{"id":"item_1","type":"mcp_tool_call","server":"g5-tools","tool":"fetch_all_intent","arguments":{},"result":null,"error":{"message":"tool failed"},"status":"failed"}}"#,
        );
        assert!(matches!(
            &mcp_failed[0],
            TurnEvent::ToolCompleted { is_error: true, .. }
        ));
        let msg = line(
            &mut mapper,
            r#"{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"OK"}}"#,
        );
        assert!(matches!(&msg[0], TurnEvent::AssistantText { text, .. } if text == "OK"));
        let end = line(
            &mut mapper,
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"cache_write_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":1}}"#,
        );
        assert!(
            matches!(&end[0], TurnEvent::Usage { usage, .. } if usage.input_tokens == 10 && usage.cache_read_input_tokens == 2)
        );
        assert!(
            matches!(&end[1], TurnEvent::Result { text: Some(t), is_error: false, .. } if t == "OK")
        );
        assert!(mapper.saw_turn_completed());
    }

    #[test]
    fn nonfatal_error_items_do_not_replace_the_turn_outcome() {
        let mut mapper = CodexFrameMapper::new();
        let warning = line(
            &mut mapper,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Model metadata unavailable; using fallback metadata"}}"#,
        );
        assert!(matches!(&warning[0], TurnEvent::ProviderWarning { .. }));
        let completed = line(&mut mapper, r#"{"type":"turn.completed"}"#);
        assert!(matches!(
            &completed[0],
            TurnEvent::Result {
                is_error: false,
                ..
            }
        ));
        assert!(!mapper.saw_turn_failed());
        let fatal = line(
            &mut mapper,
            r#"{"type":"error","message":"authentication failed"}"#,
        );
        assert!(matches!(&fatal[0], TurnEvent::ProviderError { .. }));
    }

    #[test]
    fn mapper_reports_turn_failures() {
        let mut mapper = CodexFrameMapper::new();
        let failed = line(
            &mut mapper,
            r#"{"type":"turn.failed","error":{"message":"boom"}}"#,
        );
        assert!(
            matches!(&failed[0], TurnEvent::ProviderError { message, .. } if message == "boom")
        );
        assert!(matches!(
            &failed[1],
            TurnEvent::Result { is_error: true, .. }
        ));
        assert!(mapper.saw_turn_failed());
    }

    /// Historical `codex exec --json` stream recorded from the pinned 0.159.3
    /// CLI on Linux (a command turn under a ChatGPT sign-in); it remains the
    /// reference for this item vocabulary.
    const EXEC_0_159_3: &str = r#"{"type":"thread.started","thread_id":"01a0f559-c77a-7031-8b6f-f4cb38ff864e"}
{"type":"turn.started"}
{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"/bin/bash -lc 'echo hello-from-codex'","aggregated_output":"","exit_code":null,"status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"/bin/bash -lc 'echo hello-from-codex'","aggregated_output":"hello-from-codex\n","exit_code":0,"status":"completed"}}
{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"done"}}
{"type":"turn.completed","usage":{"input_tokens":23246,"cached_input_tokens":19968,"cache_write_input_tokens":0,"output_tokens":69,"reasoning_output_tokens":12}}"#;

    #[test]
    fn pinned_cli_exec_stream_maps_to_one_completed_turn() {
        let mut mapper = CodexFrameMapper::new();
        let events: Vec<TurnEvent> = EXEC_0_159_3
            .lines()
            .flat_map(|frame| line(&mut mapper, frame))
            .collect();
        assert!(
            matches!(&events[0], TurnEvent::SessionStarted { provider_session_id, .. }
            if provider_session_id == "01a0f559-c77a-7031-8b6f-f4cb38ff864e")
        );
        assert!(
            matches!(&events[1], TurnEvent::Unknown { .. }),
            "turn.started"
        );
        assert!(
            matches!(&events[2], TurnEvent::ToolStarted { tool_name, .. } if tool_name == "command_execution")
        );
        assert!(
            matches!(&events[3], TurnEvent::ToolCompleted { is_error: false, output, .. }
            if output == "hello-from-codex\n")
        );
        assert!(
            matches!(&events[4], TurnEvent::AssistantText { text, unit_id } if text == "done" && unit_id == "item_1")
        );
        assert!(matches!(&events[5], TurnEvent::Usage { usage, .. }
            if usage.input_tokens == 23246 && usage.cache_read_input_tokens == 19968 && usage.output_tokens == 69));
        assert!(
            matches!(&events[6], TurnEvent::Result { text: Some(text), is_error: false, .. } if text == "done")
        );
        assert_eq!(events.len(), 7);
        assert!(mapper.saw_turn_completed());
    }
}
