//! Built-in scenarios replayed when no scenario file is configured (plan
//! P3.2): the deterministic `hello` contract the worker's docker-ci lane and
//! the e2e suite relied on from the retired Node fake sidecars.
//!
//! Scenario precedence: `G5_FAKE_CLAUDE_SCENARIO` / `G5_FAKE_CODEX_SCENARIO`,
//! then the shared `G5_FAKE_SCENARIO`, then these built-ins.

use serde_json::{json, Value};

use crate::scenario::{Scenario, Step};

/// Reply to a prompt whose first paragraph is `hello` (see [`is_hello`]).
pub const CLAUDE_HELLO_REPLY: &str = "hello from claude code";
/// Prefix of the Claude reply to every other prompt.
pub const CLAUDE_REPLY_PREFIX: &str = "Claude Code fake response:";
/// The thinking block every built-in Claude turn opens its reply with, as the
/// retired fake sidecar did (the routing proof for the `thought` timeline row).
pub const CLAUDE_THINKING_SUMMARY: &str =
    "Claude Code fake thinking summary: planning the fake response.";
/// Reply to a prompt whose first paragraph is `hello` (see [`is_hello`]).
pub const CODEX_HELLO_REPLY: &str = "hello from codex";
/// Prefix of the Codex reply to every other prompt.
pub const CODEX_REPLY_PREFIX: &str = "Codex fake response:";

/// The prompt's first paragraph: the text before the first blank line,
/// trimmed.
fn first_paragraph(prompt: &str) -> String {
    prompt
        .trim_start()
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// The `hello` contract keys on the prompt's first paragraph
/// (case-insensitive): the worker adapters append their own context after a
/// blank line (the Codex adapter adds the `## Current workspace paths` section
/// to the prompt it pipes to `codex exec`), and that context must not change
/// the deterministic reply.
fn is_hello(prompt: &str) -> bool {
    first_paragraph(prompt).eq_ignore_ascii_case("hello")
}

fn prompt_or_empty(prompt: &str) -> &str {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        "(empty prompt)"
    } else {
        trimmed
    }
}

/// The built-in Claude reply: the `hello` contract, else the prefixed echo
/// carrying the fake's identity (the session id is substituted at replay).
pub fn claude_reply(prompt: &str, version: &str) -> String {
    if is_hello(prompt) {
        return CLAUDE_HELLO_REPLY.to_string();
    }
    format!(
        "{CLAUDE_REPLY_PREFIX} [fake-claude {version}; session={{{{session_id}}}}] {}",
        prompt_or_empty(prompt)
    )
}

/// The built-in Codex reply.
pub fn codex_reply(prompt: &str) -> String {
    if is_hello(prompt) {
        return CODEX_HELLO_REPLY.to_string();
    }
    format!("{CODEX_REPLY_PREFIX} {}", prompt_or_empty(prompt))
}

/// A text-only Claude turn shaped like `claude/text-only.jsonl`: `init`, the
/// thinking block (`stream_event` pair + `assistant`), the text block
/// (`stream_event` pair + `assistant`), and the `result`.
pub fn claude_default(
    prompt: &str,
    version: &str,
    permission_mode: &str,
    mcp_servers: impl IntoIterator<Item = String>,
) -> Scenario {
    claude_turn(
        claude_reply(prompt, version),
        None,
        version,
        permission_mode,
        mcp_servers,
    )
}

/// A built-in Claude turn replying `reply`. With `read_path` the turn first
/// "reads" that file: an `assistant` `tool_use` block for `Read` and its
/// `user` `tool_result`, as the retired citation fixture emitted.
pub fn claude_turn(
    reply: String,
    read_path: Option<&str>,
    version: &str,
    permission_mode: &str,
    mcp_servers: impl IntoIterator<Item = String>,
) -> Scenario {
    let servers: Vec<Value> = mcp_servers
        .into_iter()
        .map(|name| json!({"name": name, "status": "connected"}))
        .collect();
    let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let usage = json!({
        "input_tokens": 1,
        "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": 0,
        "output_tokens": 1,
        "service_tier": "standard"
    });
    let mut steps = vec![Step::Emit(json!({
        "type": "system",
        "subtype": "init",
        "cwd": "{{cwd}}",
        "session_id": "{{session_id}}",
        "tools": ["Task", "Bash", "Edit", "Read", "Write", "ToolSearch"],
        "mcp_servers": servers,
        "model": "{{model}}",
        "permissionMode": permission_mode,
        "slash_commands": [],
        "apiKeySource": "none",
        "claude_code_version": version,
        "output_style": "default",
        "agents": [],
        "skills": [],
        "plugins": [],
        "uuid": "{{uuid}}"
    }))];
    if let Some(path) = read_path {
        steps.push(Step::Emit(json!({
            "type": "assistant",
            "message": {
                "model": "{{model}}",
                "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "fake-citation-{{uuid:citation}}",
                    "name": "Read",
                    "input": {"file_path": path}
                }],
                "stop_reason": "tool_use",
                "stop_sequence": null,
                "usage": usage
            },
            "parent_tool_use_id": null,
            "session_id": "{{session_id}}",
            "uuid": "{{uuid}}"
        })));
        steps.push(Step::Emit(json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "fake-citation-{{uuid:citation}}",
                    "is_error": false,
                    "content": "deterministic citation fixture read succeeded"
                }]
            },
            "parent_tool_use_id": null,
            "session_id": "{{session_id}}",
            "uuid": "{{uuid}}"
        })));
    }
    steps.extend([
        Step::Emit(json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "thinking", "thinking": "", "signature": ""}
            },
            "session_id": "{{session_id}}",
            "parent_tool_use_id": null,
            "uuid": "{{uuid}}"
        })),
        Step::Emit(json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "thinking_delta", "thinking": CLAUDE_THINKING_SUMMARY}
            },
            "session_id": "{{session_id}}",
            "parent_tool_use_id": null,
            "uuid": "{{uuid}}"
        })),
        Step::Emit(json!({
            "type": "assistant",
            "message": {
                "model": "{{model}}",
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "thinking",
                    "thinking": CLAUDE_THINKING_SUMMARY,
                    "signature": ""
                }],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": usage
            },
            "parent_tool_use_id": null,
            "session_id": "{{session_id}}",
            "uuid": "{{uuid}}"
        })),
        Step::Emit(json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_start",
                "index": 1,
                "content_block": {"type": "text", "text": ""}
            },
            "session_id": "{{session_id}}",
            "parent_tool_use_id": null,
            "uuid": "{{uuid}}"
        })),
        Step::Emit(json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_delta",
                "index": 1,
                "delta": {"type": "text_delta", "text": reply}
            },
            "session_id": "{{session_id}}",
            "parent_tool_use_id": null,
            "uuid": "{{uuid}}"
        })),
        Step::Emit(json!({
            "type": "assistant",
            "message": {
                "model": "{{model}}",
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": reply}],
                "stop_reason": "end_turn",
                "stop_sequence": null,
                "usage": usage
            },
            "parent_tool_use_id": null,
            "session_id": "{{session_id}}",
            "uuid": "{{uuid}}"
        })),
        Step::Emit(json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "duration_ms": 1,
            "duration_api_ms": 1,
            "num_turns": 1,
            "result": reply,
            "stop_reason": "end_turn",
            "session_id": "{{session_id}}",
            "total_cost_usd": 0.0,
            "usage": usage,
            "permission_denials": [],
            "uuid": "{{uuid}}"
        })),
    ]);
    Scenario::from_steps("<builtin:claude-default>", steps)
}

/// Stream the Claude turn's text block as `deltas`: the single `text_delta`
/// frame becomes one `content_block_delta` frame per delta, each slept
/// `delay_ms` before it is emitted. The `assistant` and `result` frames keep
/// the whole reply, as the provider's frames do after a long answer. A turn
/// without a text delta frame is left unchanged.
pub fn stream_text_deltas(scenario: &mut Scenario, deltas: &[String], delay_ms: u64) {
    let Some(position) = scenario.steps.iter().position(|step| {
        matches!(
            &step.step,
            Step::Emit(frame)
                if frame["type"] == "stream_event"
                    && frame["event"]["type"] == "content_block_delta"
                    && frame["event"]["delta"]["type"] == "text_delta"
        )
    }) else {
        return;
    };
    let template = scenario.steps[position].clone();
    let frames = deltas.iter().map(|text| {
        let mut step = template.clone();
        if let Step::Emit(frame) = &mut step.step {
            frame["event"]["delta"]["text"] = json!(text);
        }
        step.delay_ms = delay_ms;
        step
    });
    scenario.steps.splice(position..=position, frames);
}

/// Insert a generated fact into a tool result, leaving the assistant's reply
/// unchanged. The fact becomes available only from the native transcript.
pub fn add_claude_tool_fact(scenario: &mut Scenario, fact: &str) {
    let mut probe = claude_turn(
        "unused".to_string(),
        Some("/synthetic/tool-only-fact"),
        "fake",
        "default",
        std::iter::empty(),
    );
    let mut tool_steps: Vec<_> = probe.steps.drain(1..3).collect();
    if let Step::Emit(frame) = &mut tool_steps[1].step {
        frame["message"]["content"][0]["content"] = json!(fact);
    }
    scenario.steps.splice(1..1, tool_steps);
}

/// A text-only Codex turn shaped like `codex/text-only.jsonl`.
pub fn codex_default(prompt: &str) -> Scenario {
    codex_turn(codex_reply(prompt), None)
}

/// The citation fixture's Codex turn: a completed `file_change` item naming
/// the observed path, then the `Citation proof: <canonical>.` reply.
pub fn codex_citation(observed: &str, canonical: &str) -> Scenario {
    codex_turn(format!("Citation proof: {canonical}."), Some(observed))
}

fn codex_turn(reply: String, changed_path: Option<&str>) -> Scenario {
    let mut steps = vec![
        Step::Emit(json!({"type": "thread.started", "thread_id": "{{session_id}}"})),
        Step::Emit(json!({"type": "turn.started"})),
    ];
    if let Some(path) = changed_path {
        steps.push(Step::Emit(json!({
            "type": "item.completed",
            "item": {
                "id": "fake-citation-{{uuid:citation}}",
                "type": "file_change",
                "status": "completed",
                "changes": [{"path": path, "kind": "update"}]
            }
        })));
    }
    steps.extend([
        Step::Emit(json!({
            "type": "item.completed",
            "item": {"id": "item_0", "type": "agent_message", "text": reply}
        })),
        Step::Emit(json!({
            "type": "turn.completed",
            "usage": {
                "input_tokens": 1,
                "cached_input_tokens": 0,
                "cache_write_input_tokens": 0,
                "output_tokens": 1,
                "reasoning_output_tokens": 0
            }
        })),
    ]);
    Scenario::from_steps("<builtin:codex-default>", steps)
}

/// Insert a synthetic command result before the Codex assistant reply.
pub fn add_codex_tool_fact(scenario: &mut Scenario, fact: &str) {
    let tool_steps = Scenario::from_steps(
        "<builtin:codex-tool-fact>",
        vec![
            Step::Emit(json!({
                "type": "item.started",
                "item": {"id": "item_17", "type": "command_execution", "command": "read synthetic tool-only fact", "status": "in_progress"}
            })),
            Step::Emit(json!({
                "type": "item.completed",
                "item": {"id": "item_17", "type": "command_execution", "command": "read synthetic tool-only fact", "aggregated_output": fact, "exit_code": 0, "status": "completed"}
            })),
        ],
    );
    scenario.steps.splice(2..2, tool_steps.steps);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_maps_to_the_fixed_replies_and_other_prompts_get_the_prefix() {
        assert_eq!(claude_reply(" Hello \n", "2.1.270"), CLAUDE_HELLO_REPLY);
        assert_eq!(codex_reply("HELLO",), CODEX_HELLO_REPLY);
        let reply = claude_reply("write a poem", "2.1.270");
        assert!(reply.starts_with(CLAUDE_REPLY_PREFIX), "{reply}");
        assert!(reply.ends_with("write a poem"), "{reply}");
        assert!(reply.contains("{{session_id}}"), "{reply}");
        assert_eq!(
            codex_reply("  "),
            format!("{CODEX_REPLY_PREFIX} (empty prompt)")
        );
    }

    #[test]
    fn hello_matches_the_first_paragraph_so_adapter_context_keeps_the_reply() {
        let with_context =
            "hello\n\n## Current workspace paths\n\nCurrent working directory:\n`/work`\n";
        assert_eq!(codex_reply(with_context), CODEX_HELLO_REPLY);
        assert_eq!(
            claude_reply("\r\nHello\r\n\r\n## Context\r\n", "2.1.270"),
            CLAUDE_HELLO_REPLY
        );
        // Only an exact `hello` first paragraph maps; everything else echoes.
        for prompt in ["hello there", "say\nhello", "context\n\nhello"] {
            let reply = codex_reply(prompt);
            assert!(
                reply.starts_with(CODEX_REPLY_PREFIX),
                "{prompt:?} -> {reply}"
            );
        }
    }

    #[test]
    fn builtin_scenarios_end_with_the_provider_result_frame() {
        let claude = claude_default("hello", "2.1.270", "default", vec!["g5-tools".to_string()]);
        assert_eq!(claude.steps.len(), 8);
        match &claude.steps[3].step {
            Step::Emit(frame) => {
                assert_eq!(frame["type"], "assistant");
                assert_eq!(
                    frame["message"]["content"][0]["thinking"],
                    CLAUDE_THINKING_SUMMARY
                );
            }
            other => panic!("unexpected thinking step {other:?}"),
        }
        match &claude.steps[0].step {
            Step::Emit(frame) => {
                assert_eq!(frame["mcp_servers"][0]["name"], "g5-tools");
                assert_eq!(frame["permissionMode"], "default");
            }
            other => panic!("unexpected first step {other:?}"),
        }
        match &claude.steps[7].step {
            Step::Emit(frame) => {
                assert_eq!(frame["type"], "result");
                assert_eq!(frame["result"], CLAUDE_HELLO_REPLY);
            }
            other => panic!("unexpected last step {other:?}"),
        }
        let codex = codex_default("hello");
        assert_eq!(codex.steps.len(), 4);
        match &codex.steps[2].step {
            Step::Emit(frame) => assert_eq!(frame["item"]["text"], CODEX_HELLO_REPLY),
            other => panic!("unexpected step {other:?}"),
        }
    }

    #[test]
    fn streamed_text_deltas_replace_the_single_delta_and_keep_the_reply_whole() {
        let deltas = vec!["ab ".to_string(), "cd ".to_string(), "ef ".to_string()];
        let mut scenario = claude_turn(
            deltas.concat(),
            None,
            "2.1.270",
            "default",
            std::iter::empty(),
        );
        let before = scenario.steps.len();
        stream_text_deltas(&mut scenario, &deltas, 7);
        assert_eq!(scenario.steps.len(), before + deltas.len() - 1);
        let text_deltas: Vec<(u64, String)> = scenario
            .steps
            .iter()
            .filter_map(|step| match &step.step {
                Step::Emit(frame)
                    if frame["event"]["type"] == "content_block_delta"
                        && frame["event"]["delta"]["type"] == "text_delta" =>
                {
                    Some((
                        step.delay_ms,
                        frame["event"]["delta"]["text"]
                            .as_str()
                            .unwrap()
                            .to_string(),
                    ))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            text_deltas,
            deltas
                .iter()
                .map(|text| (7, text.clone()))
                .collect::<Vec<_>>()
        );
        // The deltas sit inside the text block, after its start frame.
        let start = scenario
            .steps
            .iter()
            .position(|step| {
                matches!(&step.step, Step::Emit(frame)
                if frame["event"]["type"] == "content_block_start" && frame["event"]["index"] == 1)
            })
            .unwrap();
        let first_delta = scenario
            .steps
            .iter()
            .position(|step| {
                matches!(&step.step, Step::Emit(frame)
                if frame["event"]["delta"]["type"] == "text_delta")
            })
            .unwrap();
        assert_eq!(first_delta, start + 1);
        let assistant = &scenario.steps[first_delta + deltas.len()];
        let Step::Emit(frame) = &assistant.step else {
            panic!("assistant frame expected");
        };
        assert_eq!(frame["type"], "assistant");
        assert_eq!(frame["message"]["content"][0]["text"], "ab cd ef ");
        let Step::Emit(result) = &scenario.steps.last().unwrap().step else {
            panic!("result frame expected");
        };
        assert_eq!(result["type"], "result");
        assert_eq!(result["result"], "ab cd ef ");
        // The thinking delta is not a text delta and stays single.
        let thinking = scenario
            .steps
            .iter()
            .filter(|step| {
                matches!(&step.step, Step::Emit(frame)
            if frame["event"]["delta"]["type"] == "thinking_delta")
            })
            .count();
        assert_eq!(thinking, 1);
        // Without a text delta frame nothing changes.
        let mut codex = codex_default("hello");
        let codex_before = codex.steps.clone();
        stream_text_deltas(&mut codex, &deltas, 0);
        assert_eq!(codex.steps, codex_before);
    }
}
