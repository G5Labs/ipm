use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRequestKind {
    Question,
    Permission,
}

impl NativeRequestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Permission => "permission",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeRequest {
    pub provider_request_id: String,
    pub tool_use_id: Option<String>,
    pub kind: NativeRequestKind,
    pub tool_name: String,
    pub input: Value,
}

pub enum ClaudeControl {
    Opened(NativeRequest),
    QuestionHook {
        request_id: String,
    },
    Cancelled {
        provider_request_id: String,
    },
    /// Claude's reply to a control request. Only the adapter issues control
    /// requests, so the runtime accepts a reply only for an id it issued.
    Response {
        request_id: String,
        success: bool,
    },
    Unsupported(Value),
}

pub const QUESTION_INIT_ID: &str = "g5-native-question-init";
const QUESTION_HOOK_ID: &str = "g5-native-question";

/// Native deferral, rather than an interrupted callback, preserves the tool
/// in Claude's session files for resumption without a replacement user prompt.
pub fn question_initialize() -> Value {
    json!({"type":"control_request","request_id":QUESTION_INIT_ID,
    "request":{"subtype":"initialize","hooks":{"PreToolUse":[{
        "matcher":"AskUserQuestion","hookCallbackIds":[QUESTION_HOOK_ID]
    }]}}})
}

pub fn question_hook_response(request_id: &str, defer: bool) -> Value {
    let response = if defer {
        json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"defer"}})
    } else {
        json!({})
    };
    json!({"type":"control_response","response":{"subtype":"success",
        "request_id":request_id,"response":response}})
}

/// Claude's in-band interrupt. Sent between turns, it stops Claude's
/// background sub-agents and clears queued work without starting a turn
/// (probed on 2.1.285); Claude answers with a `control_response`.
pub fn interrupt_request(request_id: &str) -> Value {
    json!({"type":"control_request","request_id":request_id,"request":{"subtype":"interrupt"}})
}

/// Reject a provider control request the runtime cannot serve. Claude's SDK
/// surfaces the error to the feature that asked, so the turn settles instead
/// of blocking on a response that never comes.
pub fn control_error_response(request_id: &str, error: &str) -> Value {
    json!({"type":"control_response","response":{"subtype":"error",
        "request_id":request_id,"error":error}})
}

pub fn parse_claude_control(frame: &Value) -> Option<ClaudeControl> {
    match frame.get("type").and_then(Value::as_str) {
        Some("control_request") => {
            let parsed = || {
                // Request identities pass through raw: normalizing them here
                // would admit an id the validator should reject and answer
                // with an id the provider never sent
                // (`g5.chat.native-request-boundary`).
                let provider_request_id = frame
                    .get("request_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())?;
                let request = frame.get("request")?;
                if request["subtype"] == "hook_callback"
                    && request["callback_id"] == QUESTION_HOOK_ID
                    && request["input"]["hook_event_name"] == "PreToolUse"
                    && request["input"]["tool_name"] == "AskUserQuestion"
                {
                    return Some(ClaudeControl::QuestionHook {
                        request_id: provider_request_id.to_owned(),
                    });
                }
                if request.get("subtype").and_then(Value::as_str) != Some("can_use_tool") {
                    return None;
                }
                let tool_name = request
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?;
                Some(ClaudeControl::Opened(NativeRequest {
                    provider_request_id: provider_request_id.to_string(),
                    tool_use_id: request["tool_use_id"].as_str().map(str::to_owned),
                    kind: if tool_name == "AskUserQuestion" {
                        NativeRequestKind::Question
                    } else {
                        NativeRequestKind::Permission
                    },
                    tool_name: tool_name.to_string(),
                    input: request.get("input").cloned().unwrap_or_else(|| json!({})),
                }))
            };
            Some(parsed().unwrap_or_else(|| ClaudeControl::Unsupported(frame.clone())))
        }
        Some("control_cancel_request") => Some(
            frame
                .get("request_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(|provider_request_id| ClaudeControl::Cancelled {
                    provider_request_id: provider_request_id.to_string(),
                })
                .unwrap_or_else(|| ClaudeControl::Unsupported(frame.clone())),
        ),
        Some("control_response") => {
            let parsed = || {
                let response = frame.get("response")?;
                let request_id = response
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?;
                let success = match response.get("subtype").and_then(Value::as_str)? {
                    "success" => true,
                    "error" => false,
                    _ => return None,
                };
                Some(ClaudeControl::Response {
                    request_id: request_id.to_string(),
                    success,
                })
            };
            Some(parsed().unwrap_or_else(|| ClaudeControl::Unsupported(frame.clone())))
        }
        Some(kind) if kind.starts_with("control_") => {
            Some(ClaudeControl::Unsupported(frame.clone()))
        }
        _ => None,
    }
}

pub fn native_response_envelope(
    request: &NativeRequest,
    validated_response: &Value,
) -> Result<(Value, &'static str), String> {
    let response = match request.kind {
        NativeRequestKind::Question => {
            let values = g5_contracts::types::native_question::resolve_native_question_answers(
                &request.input,
                validated_response,
            )
            .map_err(str::to_owned)?;
            let mut answers = serde_json::Map::new();
            for (question, value) in request.input["questions"]
                .as_array()
                .unwrap()
                .iter()
                .zip(values)
            {
                let key = question["question"].as_str().unwrap();
                if answers
                    .insert(key.to_owned(), Value::String(value))
                    .is_some()
                {
                    return Err("DUPLICATE_PROVIDER_QUESTION".into());
                }
            }
            let mut updated_input = request
                .input
                .as_object()
                .cloned()
                .ok_or_else(|| "INVALID_PROVIDER_QUESTION".to_string())?;
            updated_input.insert("answers".into(), Value::Object(answers));
            (
                json!({
                    "type":"control_response",
                    "response":{
                        "subtype":"success",
                        "request_id":request.provider_request_id,
                        "response":{"behavior":"allow","updatedInput":updated_input}
                    }
                }),
                "answered",
            )
        }
        NativeRequestKind::Permission => {
            let behavior = validated_response
                .get("behavior")
                .and_then(Value::as_str)
                .ok_or_else(|| "PERMISSION_BEHAVIOR_REQUIRED".to_string())?;
            match behavior {
                "allow" => {
                    let updated_input = validated_response
                        .get("updated_input")
                        .cloned()
                        .unwrap_or_else(|| request.input.clone());
                    (
                        json!({
                            "type":"control_response",
                            "response":{
                                "subtype":"success",
                                "request_id":request.provider_request_id,
                                "response":{"behavior":"allow","updatedInput":updated_input}
                            }
                        }),
                        "allowed",
                    )
                }
                "deny" => (
                    json!({
                        "type":"control_response",
                        "response":{
                            "subtype":"success",
                            "request_id":request.provider_request_id,
                            "response":{
                                "behavior":"deny",
                                "message":validated_response.get("message").and_then(Value::as_str).unwrap_or("Denied by workspace owner")
                            }
                        }
                    }),
                    "denied",
                ),
                _ => return Err("INVALID_PERMISSION_BEHAVIOR".into()),
            }
        }
    };
    Ok(response)
}

pub fn display_projection(request: &NativeRequest) -> Value {
    match request.kind {
        NativeRequestKind::Question => json!({
            "questions": request.input.get("questions").cloned().unwrap_or_else(|| json!([]))
        }),
        NativeRequestKind::Permission => json!({
            "tool_name": request.tool_name,
            "input": request.input,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_frames(name: &str) -> Vec<Value> {
        let body = match name {
            "question" => {
                include_str!("../../../tests/fixtures/agent-turn-native/claude/question.jsonl")
            }
            "bypass-question" => include_str!("../../../tests/fixtures/agent-turn-native/claude/bypass-question.jsonl"),
            "permission-allow" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/permission-allow.jsonl"
            ),
            "permission-deny" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/permission-deny.jsonl"
            ),
            "cancel" => {
                include_str!("../../../tests/fixtures/agent-turn-native/claude/cancel.jsonl")
            }
            "result-before-eof" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/result-before-eof.jsonl"
            ),
            "background-bash-report" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/background-bash-report.jsonl"
            ),
            "background-bash-eof" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/background-bash-eof.jsonl"
            ),
            "background-agent-interrupt" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/background-agent-interrupt.jsonl"
            ),
            "background-agent-interrupt-turn" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/background-agent-interrupt-turn.jsonl"
            ),
            _ => panic!("unknown fixture"),
        };
        body.lines()
            .map(|line| serde_json::from_str(line).expect("recorded JSON frame"))
            .collect()
    }

    fn recorded_response(name: &str) -> Value {
        let body = match name {
            "question" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/question.responses.jsonl"
            ),
            "bypass-question" => include_str!("../../../tests/fixtures/agent-turn-native/claude/bypass-question.responses.jsonl"),
            "permission-allow" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/permission-allow.responses.jsonl"
            ),
            "permission-deny" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/permission-deny.responses.jsonl"
            ),
            "background-agent-interrupt" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/background-agent-interrupt.responses.jsonl"
            ),
            "background-agent-interrupt-turn" => include_str!(
                "../../../tests/fixtures/agent-turn-native/claude/background-agent-interrupt-turn.responses.jsonl"
            ),
            _ => panic!("unknown response fixture"),
        };
        serde_json::from_str(body.trim()).expect("recorded control response")
    }

    #[test]
    fn question_preserves_provider_input_and_adds_native_answers() {
        for key in ["Pick", "__proto__", " Pick "] {
            let frame = json!({
                "type":"control_request",
                "request_id":"provider-1",
                "request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{"question":key,"options":[{"label":"A"},{"label":"B"}]}],"opaque":"keep"}}
            });
            let Some(ClaudeControl::Opened(request)) = parse_claude_control(&frame) else {
                panic!("expected request")
            };
            assert_eq!(request.kind, NativeRequestKind::Question);
            let (response, disposition) = native_response_envelope(
                &request,
                &json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}}),
            )
            .unwrap();
            assert_eq!(disposition, "answered");
            assert_eq!(response["response"]["request_id"], "provider-1");
            assert_eq!(
                response["response"]["response"]["updatedInput"]["opaque"],
                "keep"
            );
            assert_eq!(
                response["response"]["response"]["updatedInput"]["answers"][key],
                "B"
            );
        }
    }

    #[test]
    fn permission_allow_and_deny_use_distinct_native_dispositions() {
        let request = NativeRequest {
            provider_request_id: "provider-2".into(),
            tool_use_id: None,
            kind: NativeRequestKind::Permission,
            tool_name: "Bash".into(),
            input: json!({"command":"make test"}),
        };
        let (allowed, disposition) =
            native_response_envelope(&request, &json!({"behavior":"allow"})).unwrap();
        assert_eq!(disposition, "allowed");
        assert_eq!(
            allowed["response"]["response"]["updatedInput"]["command"],
            "make test"
        );
        let (denied, disposition) =
            native_response_envelope(&request, &json!({"behavior":"deny","message":"No"})).unwrap();
        assert_eq!(disposition, "denied");
        assert_eq!(denied["response"]["response"]["behavior"], "deny");
    }

    #[test]
    fn exact_pin_deferral_restores_the_same_native_tool() {
        let recording: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/agent-turn-native/claude/deferred-question.json"
        ))
        .unwrap();
        assert!(matches!(
            parse_claude_control(&recording["hook"]),
            Some(ClaudeControl::QuestionHook { .. })
        ));
        let Some(ClaudeControl::Opened(request)) =
            parse_claude_control(&recording["restored_request"])
        else {
            panic!("restored native question");
        };
        assert_eq!(recording["deferred"]["stop_reason"], "tool_deferred");
        assert_eq!(
            request.tool_use_id.as_deref(),
            recording["deferred"]["deferred_tool_use"]["id"].as_str()
        );
        assert_eq!(
            request.input,
            recording["deferred"]["deferred_tool_use"]["input"]
        );
        let (response, _) = native_response_envelope(
            &request,
            &json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}}),
        )
        .unwrap();
        assert_eq!(
            response["response"]["request_id"],
            recording["restored_request"]["request_id"]
        );
        assert_eq!(
            response["response"]["response"]["updatedInput"]["answers"]["Choose a color"],
            recording["answered"]["result"]
        );
    }

    #[test]
    fn current_pin_native_control_recordings_match_the_admitted_protocol() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/agent-turn-native/claude-2.1.321");
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("recording.json")).unwrap())
                .unwrap();
        let pin = g5_contracts::claude_models::CLAUDE_CODE_CLI_VERSION.trim();
        assert_eq!(
            manifest["package"],
            format!("@anthropic-ai/claude-code@{pin}")
        );
        for name in [
            "question",
            "bypass-question",
            "permission-allow",
            "permission-deny",
        ] {
            let frames: Vec<Value> = std::fs::read_to_string(root.join(format!("{name}.jsonl")))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(frames[0]["claude_code_version"], pin);
            let request = frames
                .iter()
                .find_map(|frame| match parse_claude_control(frame) {
                    Some(ClaudeControl::Opened(request)) => Some(request),
                    _ => None,
                })
                .expect("native request is admitted");
            let recorded: Value = serde_json::from_str(
                std::fs::read_to_string(root.join(format!("{name}.responses.jsonl")))
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap(),
            )
            .unwrap();
            let validated = match request.kind {
                NativeRequestKind::Question => {
                    json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}})
                }
                NativeRequestKind::Permission => recorded["response"]["response"].clone(),
            };
            let (actual, _) = native_response_envelope(&request, &validated).unwrap();
            assert_eq!(actual, recorded, "{name}: current native envelope drift");
        }
    }

    #[test]
    fn current_pin_cancel_and_eof_recordings_keep_their_distinct_lifetimes() {
        use crate::runner::{claude::ClaudeFrameMapper, events::TurnEvent};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/agent-turn-native/claude-2.1.321");
        let frames = |name| -> Vec<Value> {
            std::fs::read_to_string(root.join(format!("{name}.jsonl")))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        };
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("recording.json")).unwrap())
                .unwrap();
        let cancel = frames("cancel");
        let opened = cancel
            .iter()
            .find_map(|frame| match parse_claude_control(frame) {
                Some(ClaudeControl::Opened(request)) => Some(request.provider_request_id),
                _ => None,
            })
            .expect("native question before cancellation");
        assert!(cancel.iter().any(|frame| matches!(parse_claude_control(frame),
            Some(ClaudeControl::Cancelled { provider_request_id }) if provider_request_id == opened)));
        // 2.1.308 exited 0 without a result after SIGINT on a pending question;
        // 2.1.321 emits an `error_during_execution` result first. Either shape is
        // an interrupted turn: no successful result may be mapped, and G5's
        // forced Stop/Cancel outcome stays authoritative over the exit status.
        let mut mapper = ClaudeFrameMapper::new();
        let results: Vec<(bool, Option<String>)> = cancel
            .into_iter()
            .flat_map(|frame| mapper.map_frame(frame))
            .filter_map(|event| match event {
                TurnEvent::Result {
                    is_error, subtype, ..
                } => Some((is_error, subtype)),
                _ => None,
            })
            .collect();
        assert!(
            results.iter().all(|(is_error, _)| *is_error),
            "an interrupted exit must not map a successful result: {results:?}"
        );
        assert_eq!(manifest["probes"]["cancel"]["exit_code"], 0);
        assert_eq!(
            manifest["probes"]["cancel"]["cancel_result_received"],
            !results.is_empty()
        );
        if let Some((_, subtype)) = results.first() {
            assert_eq!(subtype.as_deref(), Some("error_during_execution"));
        }

        for (name, results) in [("result-before-eof", 1), ("background-bash-eof", 2)] {
            let mut mapper = ClaudeFrameMapper::new();
            let events: Vec<_> = frames(name)
                .into_iter()
                .flat_map(|frame| mapper.map_frame(frame))
                .collect();
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(
                        event,
                        TurnEvent::Result {
                            is_error: false,
                            ..
                        }
                    ))
                    .count(),
                results,
                "{name}"
            );
            if name == "background-bash-eof" {
                let first_result = events
                    .iter()
                    .position(|event| matches!(event, TurnEvent::Result { .. }))
                    .unwrap();
                let emptied = events.iter().position(|event| matches!(event, TurnEvent::BackgroundTasks { tasks } if tasks.is_empty())).unwrap();
                assert!(first_result < emptied);
                assert!(events[emptied..].iter().any(|event| matches!(
                    event,
                    TurnEvent::Result {
                        is_error: false,
                        ..
                    }
                )));
            }
        }
        assert_eq!(
            manifest["probes"]["result-before-eof"]["alive_one_second_after_result"],
            true
        );
    }

    #[test]
    fn historical_native_recordings_match_the_admitted_protocol() {
        for name in [
            "question",
            "bypass-question",
            "permission-allow",
            "permission-deny",
        ] {
            let frames = fixture_frames(name);
            assert_eq!(frames[0]["claude_code_version"], "2.1.285");
            let opened = frames
                .iter()
                .find_map(|frame| match parse_claude_control(frame) {
                    Some(ClaudeControl::Opened(request)) => Some(request),
                    _ => None,
                })
                .expect("recording must contain one native request");
            let validated = match name {
                "question" | "bypass-question" => {
                    json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}})
                }
                "permission-allow" => json!({"behavior":"allow"}),
                "permission-deny" => {
                    json!({"behavior":"deny","message":"Denied by pinned fixture"})
                }
                _ => unreachable!(),
            };
            let (actual, _) = native_response_envelope(&opened, &validated).unwrap();
            assert_eq!(actual, recorded_response(name));
        }
    }

    #[test]
    fn historical_cancel_and_result_lifetime_are_classified() {
        let cancel = fixture_frames("cancel");
        assert_eq!(cancel[0]["claude_code_version"], "2.1.285");
        let opened = cancel
            .iter()
            .find_map(|frame| match parse_claude_control(frame) {
                Some(ClaudeControl::Opened(request)) => Some(request.provider_request_id),
                _ => None,
            })
            .expect("pending request");
        assert!(cancel.iter().any(|frame| matches!(
            parse_claude_control(frame),
            Some(ClaudeControl::Cancelled { provider_request_id }) if provider_request_id == opened
        )));
        let terminal = cancel.last().expect("terminal result");
        assert_eq!(terminal["type"], "result");
        assert_eq!(terminal["terminal_reason"], "aborted_streaming");
        assert_eq!(terminal["is_error"], true);

        let result = fixture_frames("result-before-eof");
        assert_eq!(result[0]["claude_code_version"], "2.1.285");
        let terminal = result.last().expect("terminal result");
        assert_eq!(terminal["type"], "result");
        assert_eq!(terminal["terminal_reason"], "completed");
        assert_eq!(terminal["is_error"], false);
    }

    #[test]
    fn malformed_and_unknown_control_frames_fail_closed() {
        for frame in [
            json!({"type":"control_request","request_id":"missing-request"}),
            json!({"type":"control_request","request_id":"x","request":{"subtype":"future"}}),
            json!({"type":"control_cancel_request","request_id":""}),
            json!({"type":"control_future","payload":{}}),
            json!({"type":"control_response","response":{"subtype":"success"}}),
            json!({"type":"control_response","response":{"subtype":"future","request_id":"g5-end-1"}}),
        ] {
            assert!(matches!(
                parse_claude_control(&frame),
                Some(ClaudeControl::Unsupported(_))
            ));
        }
    }

    #[test]
    fn control_request_identities_parse_raw_without_normalization() {
        let frame = json!({"type":"control_request","request_id":" padded ",
            "request":{"subtype":"can_use_tool","tool_name":"Bash","input":{}}});
        let Some(ClaudeControl::Opened(request)) = parse_claude_control(&frame) else {
            panic!("a padded id still parses as an opened request");
        };
        assert_eq!(request.provider_request_id, " padded ");
        let cancel = json!({"type":"control_cancel_request","request_id":" padded "});
        assert!(matches!(
            parse_claude_control(&cancel),
            Some(ClaudeControl::Cancelled { provider_request_id }) if provider_request_id == " padded "
        ));
    }

    #[test]
    fn control_error_response_names_the_request_and_error() {
        let frame = control_error_response("req-9", "This control request is unsupported.");
        assert_eq!(frame["type"], "control_response");
        assert_eq!(frame["response"]["subtype"], "error");
        assert_eq!(frame["response"]["request_id"], "req-9");
        assert_eq!(
            frame["response"]["error"],
            "This control request is unsupported."
        );
    }

    #[test]
    fn interrupt_replies_are_classified_by_request_id() {
        // Claude 2.1.285 answering an idle in-band interrupt.
        let reply = json!({"type":"control_response","response":{"subtype":"success","request_id":"g5-end-1","response":{"still_queued":[]}}});
        assert!(matches!(
            parse_claude_control(&reply),
            Some(ClaudeControl::Response { request_id, success: true }) if request_id == "g5-end-1"
        ));
        let failed = json!({"type":"control_response","response":{"subtype":"error","request_id":"g5-end-2","error":"busy"}});
        assert!(matches!(
            parse_claude_control(&failed),
            Some(ClaudeControl::Response { success: false, .. })
        ));
        assert_eq!(
            interrupt_request("g5-end-1"),
            json!({"type":"control_request","request_id":"g5-end-1","request":{"subtype":"interrupt"}})
        );
    }

    fn mapped(name: &str) -> Vec<crate::runner::TurnEvent> {
        let mut mapper = crate::runner::claude::ClaudeFrameMapper::new();
        fixture_frames(name)
            .into_iter()
            .filter(|frame| parse_claude_control(frame).is_none())
            .flat_map(|frame| mapper.map_frame(frame))
            .collect()
    }

    #[test]
    fn exact_pin_report_turn_follows_an_emptied_background_set() {
        use crate::runner::TurnEvent;
        let frames = fixture_frames("background-bash-report");
        assert_eq!(frames[0]["claude_code_version"], "2.1.285");
        let events = mapped("background-bash-report");
        let position =
            |test: &dyn Fn(&TurnEvent) -> bool| events.iter().position(test).expect("event");
        let first_result = position(&|event| matches!(event, TurnEvent::Result { .. }));
        let busy = position(
            &|event| matches!(event, TurnEvent::BackgroundTasks { tasks } if tasks.len() == 1),
        );
        let idle = position(
            &|event| matches!(event, TurnEvent::BackgroundTasks { tasks } if tasks.is_empty()),
        );
        let report = position(&|event| matches!(event, TurnEvent::TurnStarted { .. }));
        // The job is reported live before the first turn ends, the set empties
        // after it, and only then does Claude start its own report turn.
        assert!(busy < first_result && first_result < idle && idle < report);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, TurnEvent::SessionStarted { .. }))
                .count(),
            1
        );
        let results: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TurnEvent::Result { is_error, .. } => Some(*is_error),
                _ => None,
            })
            .collect();
        assert_eq!(results, [false, false]);
        assert!(events[report..]
            .iter()
            .any(|event| matches!(event, TurnEvent::Result { .. })));
    }

    #[test]
    fn exact_pin_input_eof_stops_background_commands() {
        use crate::runner::events::TaskPhase;
        use crate::runner::TurnEvent;
        let events = mapped("background-bash-eof");
        let result = events
            .iter()
            .position(|event| matches!(event, TurnEvent::Result { .. }))
            .expect("result");
        assert!(events[result..].iter().any(|event| matches!(
            event,
            TurnEvent::TaskLifecycle { phase: TaskPhase::Finished, status: Some(status), .. } if status == "stopped"
        )));
        assert!(!events[result..]
            .iter()
            .any(|event| matches!(event, TurnEvent::TurnStarted { .. })));
    }

    #[test]
    fn exact_pin_interrupt_stops_background_agents_without_a_report_turn() {
        use crate::runner::events::TaskPhase;
        use crate::runner::TurnEvent;
        let frames = fixture_frames("background-agent-interrupt");
        assert_eq!(
            recorded_response("background-agent-interrupt"),
            interrupt_request("g5-end-fixture")
        );
        assert!(frames.iter().any(|frame| matches!(
            parse_claude_control(frame),
            Some(ClaudeControl::Response { request_id, success: true }) if request_id == "g5-end-fixture"
        )));
        let events = mapped("background-agent-interrupt");
        assert!(events.iter().any(|event| matches!(
            event,
            TurnEvent::TaskLifecycle { phase: TaskPhase::Started, task_type: Some(kind), .. } if kind == "local_agent"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            TurnEvent::TaskLifecycle { phase: TaskPhase::Finished, status: Some(status), .. } if status == "stopped"
        )));
        assert!(!events
            .iter()
            .any(|event| matches!(event, TurnEvent::TurnStarted { .. })));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, TurnEvent::Result { .. }))
                .count(),
            1
        );
        // The background sub-agent's own Bash call and its rejected result
        // (frames with `parent_tool_use_id`) are not the reply's tool rows.
        let tools: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TurnEvent::ToolStarted { tool_name, .. } => Some(tool_name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tools, ["Agent"]);
        assert!(!events
            .iter()
            .any(|event| matches!(event, TurnEvent::ToolCompleted { is_error: true, .. })));
    }

    #[test]
    fn exact_pin_interrupt_during_a_turn_aborts_it_and_stops_background_agents() {
        use crate::runner::events::TaskPhase;
        use crate::runner::TurnEvent;
        let frames = fixture_frames("background-agent-interrupt-turn");
        assert_eq!(frames[0]["claude_code_version"], "2.1.285");
        assert_eq!(
            recorded_response("background-agent-interrupt-turn"),
            interrupt_request("g5-end-fixture")
        );
        // Claude answers the interrupt, then ends the aborted turn.
        let reply = frames
            .iter()
            .position(|frame| {
                matches!(
                    parse_claude_control(frame),
                    Some(ClaudeControl::Response { request_id, success: true })
                        if request_id == "g5-end-fixture"
                )
            })
            .expect("interrupt reply");
        let terminal = frames.last().expect("terminal result");
        assert_eq!(terminal["type"], "result");
        assert_eq!(terminal["subtype"], "error_during_execution");
        // The adapter recognizes the End's effect by this terminal reason.
        assert!(terminal["terminal_reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("aborted_")));
        assert!(reply < frames.len() - 1);

        let events = mapped("background-agent-interrupt-turn");
        let results: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TurnEvent::Result {
                    is_error, subtype, ..
                } => Some((*is_error, subtype.as_deref())),
                _ => None,
            })
            .collect();
        assert_eq!(results, [(true, Some("error_during_execution"))]);
        assert!(!events
            .iter()
            .any(|event| matches!(event, TurnEvent::TurnStarted { .. })));
        // Only the background sub-agent is background work; Claude also
        // registers the turn's long foreground command as a task.
        let agent = frames
            .iter()
            .find(|frame| frame["subtype"] == "task_started" && frame["is_backgrounded"] == true)
            .and_then(|frame| frame["task_id"].as_str())
            .expect("background agent");
        assert!(frames
            .iter()
            .any(|frame| frame["subtype"] == "task_started" && frame["is_backgrounded"] == false));
        let lifecycle: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TurnEvent::TaskLifecycle {
                    task_id,
                    phase,
                    status,
                    ..
                } => Some((task_id.as_str(), *phase, status.as_deref())),
                _ => None,
            })
            .collect();
        assert_eq!(
            lifecycle,
            [
                (agent, TaskPhase::Started, None),
                (agent, TaskPhase::Progress, None),
                (agent, TaskPhase::Finished, Some("stopped")),
            ]
        );
        // The turn's own foreground command stays a tool row of the reply.
        let tools: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TurnEvent::ToolStarted { tool_name, .. } => Some(tool_name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tools, ["Agent", "Bash"]);
    }
}
