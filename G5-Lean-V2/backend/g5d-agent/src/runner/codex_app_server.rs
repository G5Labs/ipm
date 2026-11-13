//! Attempt-local, stable JSON-RPC surface for the pinned Codex App Server.
//! Process lifetime, writes, deadlines, journaling and settlement belong to runtime.
use std::collections::HashSet;

use serde_json::{json, Value};

use super::codex::CodexFrameMapper;
use super::events::{TurnEvent, TurnUsage};

pub use g5_contracts::codex_models::{
    CODEX_APP_SERVER_TRANSPORT as TRANSPORT, CODEX_CLI_VERSION as VERSION,
};
const DISABLED_FEATURES: &str = include_str!("codex_disabled_features.txt");

/// CLI overrides win over both owner and project configuration. Never append
/// untrusted config/feature switches after these overrides.
pub fn configuration() -> Vec<String> {
    let mut args = vec!["app-server".into(), "--listen".into(), "stdio://".into()];
    args.extend(feature_configuration());
    args
}

/// The binary pin's runtime feature contract also applies to retained exec
/// paths, so upgrading the binary cannot silently enable a new default there.
pub(crate) fn feature_configuration() -> Vec<String> {
    let mut args = Vec::new();
    for feature in DISABLED_FEATURES.lines().chain(["daemon_auto_start"]) {
        args.extend(["-c".into(), format!("features.{feature}=false")]);
    }
    args
}

pub fn initialize() -> Value {
    json!({"id":"g5-initialize","method":"initialize","params":{
        "clientInfo":{"name":"g5_agent","version":"1"},
        "capabilities":{"experimentalApi":false}
    }})
}

const NATIVE_REQUEST_METHODS: &[&str] = &[
    "item/commandExecution/requestApproval",
    "item/fileChange/requestApproval",
    "item/tool/requestUserInput",
    "mcpServer/elicitation/request",
    "item/permissions/requestApproval",
    "item/tool/call",
    "account/chatgptAuthTokens/refresh",
    "attestation/generate",
    "applyPatchApproval",
    "execCommandApproval",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Initialize,
    Thread,
    Turn,
    Active,
    Terminal,
    Failed,
}

#[derive(Default)]
pub struct Effects {
    pub writes: Vec<Value>,
    pub events: Vec<TurnEvent>,
    pub accepted: bool,
    pub rejected: bool,
    pub failure: Option<&'static str>,
}

/// The entire state belongs to one supervisor invocation (and thus one exact
/// task/attempt/incarnation). No native identity is shared across attempts.
pub struct AppServer {
    phase: Phase,
    thread_params: Value,
    turn_params: Value,
    resume: Option<String>,
    /// A formal review's ephemeral, instruction-free thread under the
    /// launch's permission profile (`g5.agent-approval.codex-launch`).
    formal: bool,
    thread: Option<String>,
    turn: Option<String>,
    mapper: CodexFrameMapper,
    completed_items: HashSet<String>,
    usage_total: Option<TurnUsage>,
    interrupted: bool,
}

impl AppServer {
    pub fn new(thread_params: Value, turn_params: Value, resume: Option<String>) -> Self {
        Self {
            phase: Phase::Initialize,
            thread_params,
            turn_params,
            resume,
            formal: false,
            thread: None,
            turn: None,
            mapper: CodexFrameMapper::new(),
            completed_items: HashSet::new(),
            usage_total: None,
            interrupted: false,
        }
    }

    /// One fresh formal-review thread. Its thread parameters carry no
    /// sandbox, so the launch's root-denying permission profile governs, and
    /// it can never resume a native session.
    pub fn formal(thread_params: Value, turn_params: Value) -> Self {
        Self {
            formal: true,
            ..Self::new(thread_params, turn_params, None)
        }
    }

    pub fn ended(&self) -> bool {
        matches!(self.phase, Phase::Terminal | Phase::Failed)
    }

    pub fn was_interrupted(&self) -> bool {
        self.interrupted
    }

    fn fail(&mut self, reason: &'static str) -> Effects {
        self.phase = Phase::Failed;
        Effects {
            failure: Some(reason),
            ..Effects::default()
        }
    }

    pub fn receive(&mut self, frame: Value) -> Effects {
        if !frame.is_object()
            || (frame.get("id").is_none() && frame.get("method").and_then(Value::as_str).is_none())
        {
            return self.fail("codex_malformed_frame");
        }
        // Server requests are never silently treated as permission grants. A
        // native interaction remains unsupported for Codex, as with exec.
        if frame.get("method").is_some()
            && (frame.get("id").is_some()
                || frame["method"]
                    .as_str()
                    .is_some_and(|method| NATIVE_REQUEST_METHODS.contains(&method)))
        {
            let mut result = self.fail("unsupported_codex_native_request");
            if frame["id"].is_string() || frame["id"].is_i64() || frame["id"].is_u64() {
                result.writes.push(json!({"id":frame["id"],"error":{
                    "code":-32601,"message":"This native interaction is unsupported by G5"
                }}));
            }
            return result;
        }
        if let Some(response_id) = frame.get("id") {
            let Some(id) = response_id.as_str() else {
                return Effects::default();
            };
            let expected = match self.phase {
                Phase::Initialize => "g5-initialize",
                Phase::Thread => "g5-thread",
                Phase::Turn => "g5-turn",
                _ => return Effects::default(),
            };
            if id != expected {
                return Effects::default();
            }
            if frame.get("result").is_none()
                && frame["error"]["code"].as_i64().is_some()
                && frame["error"]["message"].is_string()
            {
                let validation_refusal =
                    matches!(frame["error"]["code"].as_i64(), Some(-32602..=-32600));
                let initial_turn_refused = self.phase == Phase::Turn && validation_refusal;
                let mut effects = self.fail(if validation_refusal {
                    "codex_rpc_refused"
                } else {
                    "codex_rpc_failed"
                });
                effects.rejected = initial_turn_refused;
                return effects;
            }
            if frame.get("error").is_some() || !frame["result"].is_object() {
                // A malformed/lost response is not evidence of rejection:
                // the initial turn request may already have been accepted.
                return self.fail("codex_malformed_frame");
            }
            match self.phase {
                Phase::Initialize => {
                    let agent = frame["result"]["userAgent"].as_str().unwrap_or_default();
                    if !agent.starts_with(&format!("g5_agent/{} ", VERSION.trim())) {
                        return self.fail("codex_binary_pin_mismatch");
                    }
                    self.phase = Phase::Thread;
                    let mut params = self.thread_params.clone();
                    if let Some(thread) = &self.resume {
                        params["threadId"] = json!(thread);
                        // G5 already owns transcript replay. Keep the resume
                        // response bounded without changing native model history.
                        params["excludeTurns"] = json!(true);
                    }
                    return Effects {
                        writes: vec![
                            json!({"method":"initialized"}),
                            json!({"id":"g5-thread","method":if self.resume.is_some() {"thread/resume"} else {"thread/start"},"params":params}),
                        ],
                        ..Effects::default()
                    };
                }
                Phase::Thread => {
                    let sandbox = match self.thread_params["sandbox"].as_str() {
                        None if self.formal => "readOnly",
                        Some("read-only") => "readOnly",
                        Some("workspace-write") => "workspaceWrite",
                        Some("danger-full-access") => "dangerFullAccess",
                        _ => "unsupported",
                    };
                    if frame["result"]["model"] != self.thread_params["model"]
                        || frame["result"]["approvalPolicy"] != "never"
                        || frame["result"]["sandbox"]["type"] != sandbox
                    {
                        return self.fail("codex_effective_configuration_mismatch");
                    }
                    // Stable evidence that no rollout persists (the unstable
                    // optional path must not contradict it) and that no
                    // owner/project instructions loaded. The permission
                    // profile itself is outside the stable schema and is
                    // proven by native confinement tests instead.
                    if self.formal
                        && (frame["result"]["thread"]["ephemeral"] != true
                            || !frame["result"]["thread"]["path"].is_null()
                            || frame["result"]["instructionSources"] != json!([]))
                    {
                        return self.fail("codex_formal_confinement_mismatch");
                    }
                    let Some(thread) = identity(&frame["result"]["thread"]["id"]) else {
                        return self.fail("codex_invalid_thread_identity");
                    };
                    if self
                        .resume
                        .as_deref()
                        .is_some_and(|expected| expected != thread)
                    {
                        return self.fail("codex_wrong_resumed_thread");
                    }
                    self.thread = Some(thread.to_owned());
                    self.phase = Phase::Turn;
                    let mut params = self.turn_params.clone();
                    params["threadId"] = json!(thread);
                    return Effects {
                        writes: vec![json!({"id":"g5-turn","method":"turn/start","params":params})],
                        ..Effects::default()
                    };
                }
                Phase::Turn => {
                    let turn = &frame["result"]["turn"];
                    let Some(id) = identity(&turn["id"]) else {
                        return self.fail("codex_invalid_turn_identity");
                    };
                    if turn["status"] != "inProgress" {
                        return self.fail("codex_invalid_initial_turn_status");
                    }
                    self.turn = Some(id.to_owned());
                    self.phase = Phase::Active;
                    return Effects {
                        accepted: true,
                        events: vec![TurnEvent::SessionStarted {
                            provider_session_id: self.thread.clone().unwrap(),
                            model: self.thread_params["model"].as_str().map(str::to_owned),
                            mcp_servers: Vec::new(),
                            raw: frame,
                        }],
                        ..Effects::default()
                    };
                }
                _ => unreachable!(),
            }
        }
        if !matches!(self.phase, Phase::Active | Phase::Terminal) {
            return Effects::default();
        }
        // Account notifications have no thread/turn identity. Only consume
        // them on this attempt's active process; ordinary output stays fenced.
        if self.phase == Phase::Active && frame["method"] == "account/rateLimits/updated" {
            return Effects {
                events: crate::subscription_usage::Observation::codex(&frame)
                    .map(|observation| vec![TurnEvent::SubscriptionUsage { observation }])
                    .unwrap_or_default(),
                ..Effects::default()
            };
        }
        let Some(method) = frame["method"].as_str() else {
            return self.fail("codex_malformed_frame");
        };
        let p = &frame["params"];
        let turn_event = matches!(
            method,
            "turn/completed"
                | "turn/started"
                | "error"
                | "item/started"
                | "item/completed"
                | "item/agentMessage/delta"
                | "thread/tokenUsage/updated"
        );
        if turn_event && identity(&p["threadId"]).is_none() {
            return self.fail("codex_malformed_frame");
        }
        if p["threadId"].as_str() != self.thread.as_deref() {
            return Effects::default();
        }
        let turn_id = if method == "turn/completed" || method == "turn/started" {
            p["turn"]["id"].as_str()
        } else {
            p["turnId"].as_str()
        };
        if turn_event && turn_id.is_none() {
            return self.fail("codex_malformed_frame");
        }
        // Thread-scoped warnings are diagnostic, never terminal evidence.
        if method == "warning" {
            return Effects {
                events: vec![TurnEvent::ProviderWarning {
                    message: p["message"].as_str().unwrap_or("Codex warning").into(),
                    raw: frame,
                }],
                ..Effects::default()
            };
        }
        if turn_id != self.turn.as_deref() {
            return Effects::default();
        }
        let events = match method {
            "turn/completed" => {
                if self.phase == Phase::Terminal {
                    return Effects::default();
                }
                let failed = match p["turn"]["status"].as_str() {
                    Some("completed") => false,
                    Some("failed" | "interrupted") => true,
                    _ => return self.fail("codex_invalid_terminal_status"),
                };
                if !failed && p["turn"].get("error").is_some_and(|error| !error.is_null()) {
                    return self.fail("codex_invalid_terminal_status");
                }
                self.phase = Phase::Terminal;
                self.interrupted = p["turn"]["status"] == "interrupted";
                if self.interrupted {
                    vec![TurnEvent::Result {
                        text: self.mapper.final_text(),
                        is_error: true,
                        // Native interruption has its own established G5 outcome;
                        // do not turn it into a generic provider-error subtype.
                        subtype: None,
                        raw: frame,
                    }]
                } else {
                    self.mapper.map_frame(if failed {
                        json!({"type":"turn.failed","error":p["turn"]["error"]})
                    } else {
                        json!({"type":"turn.completed"})
                    })
                }
            }
            "error" => {
                // Some errors announce a retry, which is not a failed turn.
                if p["willRetry"] == true {
                    Vec::new()
                } else {
                    vec![TurnEvent::ProviderError {
                        message: p["error"]["message"]
                            .as_str()
                            .unwrap_or("Codex provider error")
                            .into(),
                        raw: frame,
                    }]
                }
            }
            "item/agentMessage/delta" => p["delta"]
                .as_str()
                .zip(identity(&p["itemId"]))
                .map(|(text, id)| {
                    vec![TurnEvent::TextDelta {
                        text: text.into(),
                        unit_id: Some(id.into()),
                    }]
                })
                .unwrap_or_default(),
            "thread/tokenUsage/updated" => {
                let usage = &p["tokenUsage"];
                let mut delta = TurnUsage::default();
                let mut high_water = self.usage_total.clone().unwrap_or_default();
                // The pin reports thread-cumulative total and latest model-call
                // last. The first current-turn update establishes the pre-turn
                // baseline (total - last); later updates use total differences.
                // This counts repeated/equal-sized model calls within one tool
                // turn without importing earlier native-session usage.
                for (key, previous, value) in [
                    (
                        "inputTokens",
                        &mut high_water.input_tokens,
                        &mut delta.input_tokens,
                    ),
                    (
                        "outputTokens",
                        &mut high_water.output_tokens,
                        &mut delta.output_tokens,
                    ),
                    (
                        "cachedInputTokens",
                        &mut high_water.cache_read_input_tokens,
                        &mut delta.cache_read_input_tokens,
                    ),
                    (
                        "cacheWriteInputTokens",
                        &mut high_water.cache_creation_input_tokens,
                        &mut delta.cache_creation_input_tokens,
                    ),
                ] {
                    let total = usage["total"][key].as_u64().unwrap_or(0);
                    *value = if self.usage_total.is_none() {
                        usage["last"][key].as_u64().unwrap_or(0)
                    } else {
                        total.saturating_sub(*previous)
                    };
                    *previous = total.max(*previous);
                }
                self.usage_total = Some(high_water);
                if delta.is_empty() {
                    Vec::new()
                } else {
                    vec![TurnEvent::Usage {
                        usage: delta,
                        raw: frame,
                    }]
                }
            }
            "item/started" | "item/completed" => {
                let mut item = p["item"].clone();
                let Some(id) = identity(&item["id"]).map(str::to_owned) else {
                    return self.fail("codex_invalid_item_identity");
                };
                let completed = method == "item/completed";
                if self.completed_items.contains(&id) {
                    return Effects::default();
                }
                if completed {
                    self.completed_items.insert(id);
                }
                item["type"] = match item["type"].as_str().unwrap_or_default() {
                    "agentMessage" => json!("agent_message"),
                    "commandExecution" => {
                        item["aggregated_output"] = item["aggregatedOutput"].clone();
                        item["exit_code"] = item["exitCode"].clone();
                        json!("command_execution")
                    }
                    "mcpToolCall" => json!("mcp_tool_call"),
                    "fileChange" => json!("file_change"),
                    "webSearch" => json!("web_search"),
                    "reasoning" => json!("reasoning"),
                    "plan" => json!("plan_update"),
                    "dynamicToolCall"
                    | "collabAgentToolCall"
                    | "imageView"
                    | "imageGeneration"
                    | "functionCallOutput"
                    | "sleep" => {
                        let name = item["tool"]
                            .as_str()
                            .or_else(|| item["name"].as_str())
                            .or_else(|| item["type"].as_str())
                            .unwrap_or("tool")
                            .to_owned();
                        let native = item.clone();
                        item["name"] = json!(name);
                        item["input"] = native.clone();
                        item["output"] = native
                            .get("contentItems")
                            .or_else(|| native.get("agentsStates"))
                            .or_else(|| native.get("output"))
                            .cloned()
                            .unwrap_or(native);
                        if item["success"] == false
                            || item.get("failure").is_some_and(|v| !v.is_null())
                        {
                            item["status"] = json!("failed");
                        }
                        json!("tool_call")
                    }
                    _ => return Effects::default(),
                };
                if item["status"] == "declined" {
                    item["status"] = json!("failed");
                }
                self.mapper.map_frame(json!({"type":if completed {"item.completed"} else {"item.started"},"item":item}))
            }
            _ => Vec::new(),
        };
        Effects {
            events,
            ..Effects::default()
        }
    }
}

fn identity(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| {
        !s.is_empty() && s.len() <= 256 && s.trim() == *s && !s.chars().any(char::is_control)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(resume: Option<&str>) -> AppServer {
        AppServer::new(
            json!({"model":"frozen-model","sandbox":"read-only","approvalPolicy":"never"}),
            json!({"input":[{"type":"text","text":"frozen input"}],"effort":"high"}),
            resume.map(str::to_owned),
        )
    }
    fn initialized(a: &mut AppServer) -> Effects {
        a.receive(json!({"id":"g5-initialize","result":{"userAgent":format!("g5_agent/{} (Linux)", VERSION.trim())}}))
    }
    fn thread(a: &mut AppServer) -> Effects {
        a.receive(json!({"id":"g5-thread","result":{"thread":{"id":"thread-809"},"model":"frozen-model","approvalPolicy":"never","sandbox":{"type":"readOnly"}}}))
    }
    fn accepted(a: &mut AppServer) -> Effects {
        a.receive(json!({"id":"g5-turn","result":{"turn":{"id":"turn-809","status":"inProgress","items":[]}}}))
    }
    fn active() -> AppServer {
        let mut a = adapter(None);
        initialized(&mut a);
        thread(&mut a);
        assert!(accepted(&mut a).accepted);
        a
    }

    #[test]
    fn subscription_notifications_are_account_scoped_and_never_terminal_evidence() {
        let frame = json!({"method":"account/rateLimits/updated","params":{"rateLimits":{
            "limitId":"codex","primary":{"usedPercent":23,"windowDurationMins":300}
        }}});
        assert!(adapter(None).receive(frame.clone()).events.is_empty());
        let mut app = active();
        let result = app.receive(frame.clone());
        assert!(matches!(
            result.events.as_slice(),
            [TurnEvent::SubscriptionUsage { .. }]
        ));
        assert!(result.writes.is_empty());
        assert!(result.failure.is_none());
        assert!(!result.accepted && !app.ended());
        app.receive(completed("completed"));
        assert!(app.receive(frame).events.is_empty());
    }
    fn event(method: &str, mut params: Value) -> Value {
        params["threadId"] = json!("thread-809");
        params["turnId"] = json!("turn-809");
        json!({"method":method,"params":params})
    }
    fn completed(status: &str) -> Value {
        event(
            "turn/completed",
            json!({"turn":{"id":"turn-809","status":status}}),
        )
    }

    #[test]
    fn handshake_accepts_only_the_initial_turn_response_and_keeps_frozen_input() {
        let mut a = adapter(None);
        let init = initialized(&mut a);
        assert!(!init.accepted);
        assert_eq!(init.writes[0], json!({"method":"initialized"}));
        assert_eq!(init.writes[1]["method"], "thread/start");
        assert_eq!(init.writes[1]["params"]["approvalPolicy"], "never");
        let t = thread(&mut a);
        assert!(!t.accepted);
        assert!(t.events.is_empty());
        assert_eq!(t.writes[0]["method"], "turn/start");
        assert_eq!(t.writes[0]["params"]["input"][0]["text"], "frozen input");
        assert_eq!(t.writes[0]["params"]["effort"], "high");
        assert!(
            !a.receive(event(
                "turn/started",
                json!({"turn":{"id":"turn-809","status":"inProgress"}})
            ))
            .accepted
        );
        let input = accepted(&mut a);
        assert!(input.accepted);
        assert_eq!(input.events.len(), 1);
        assert!(!accepted(&mut a).accepted);
    }

    #[test]
    fn malformed_initial_acknowledgements_preserve_delivery_uncertainty() {
        for frame in [
            json!({"id":"g5-turn"}),
            json!({"id":"g5-turn","error":null}),
            json!({"id":"g5-turn","error":{"code":-32603,"message":"internal failure"}}),
            json!({"id":"g5-turn","error":{"code":-32600,"message":"refused"},"result":{}}),
        ] {
            let mut a = adapter(None);
            initialized(&mut a);
            thread(&mut a);
            let effect = a.receive(frame);
            assert!(effect.failure.is_some());
            assert!(!effect.rejected);
            assert!(!effect.accepted);
        }
    }

    #[test]
    fn native_resume_refuses_another_thread_and_never_delivers_input() {
        let mut a = adapter(Some("original"));
        let init = initialized(&mut a);
        assert_eq!(init.writes[1]["method"], "thread/resume");
        assert_eq!(init.writes[1]["params"]["threadId"], "original");
        assert_eq!(init.writes[1]["params"]["excludeTurns"], true);
        let effect = thread(&mut a);
        assert!(!effect.accepted);
        assert!(effect.failure.is_some());
        assert!(a.ended());
    }

    #[test]
    fn initialization_thread_and_turn_refusals_have_no_acceptance() {
        for code in [-32600, -32601, -32602, -32603] {
            for phase in 0..3 {
                let mut a = adapter(None);
                if phase > 0 {
                    initialized(&mut a);
                }
                if phase > 1 {
                    thread(&mut a);
                }
                let id = ["g5-initialize", "g5-thread", "g5-turn"][phase];
                let effect = a.receive(json!({"id":id,"error":{"code":code,"message":"refused"}}));
                assert_eq!(
                    effect.failure,
                    Some(if code == -32603 {
                        "codex_rpc_failed"
                    } else {
                        "codex_rpc_refused"
                    })
                );
                assert_eq!(effect.rejected, phase == 2 && code != -32603);
                assert!(!effect.accepted);
                assert!(effect.events.is_empty());
                assert!(a.ended());
            }
        }
    }

    #[test]
    fn actual_binary_pin_and_malformed_frames_fail_closed() {
        let mut a = adapter(None);
        assert_eq!(
            a.receive(
                json!({"id":"g5-initialize","result":{"userAgent":"g5_agent/0.159.3 (Linux)"}})
            )
            .failure,
            Some("codex_binary_pin_mismatch")
        );
        for f in [Value::Null, json!([]), json!({"result":{}})] {
            assert!(adapter(None).receive(f).failure.is_some());
        }
    }

    #[test]
    fn stale_or_unidentified_turn_output_cannot_publish_or_pollute_current_state() {
        for frame in [
            event(
                "item/agentMessage/delta",
                json!({"itemId":"answer","delta":"stale text"}),
            ),
            event(
                "item/started",
                json!({"item":{"id":"tool","type":"commandExecution","command":"ls","status":"inProgress"}}),
            ),
            event(
                "item/completed",
                json!({"item":{"id":"answer","type":"agentMessage","text":"stale final"}}),
            ),
            event(
                "thread/tokenUsage/updated",
                json!({"tokenUsage":{"last":{"inputTokens":99},"total":{"inputTokens":99}}}),
            ),
            event(
                "error",
                json!({"error":{"message":"stale error"},"willRetry":false}),
            ),
        ] {
            for field in ["threadId", "turnId"] {
                for missing in [false, true] {
                    let mut a = active();
                    let mut stale = frame.clone();
                    if missing {
                        stale["params"].as_object_mut().unwrap().remove(field);
                    } else {
                        stale["params"][field] = json!("foreign");
                    }
                    let effect = a.receive(stale.clone());
                    assert!(effect.events.is_empty(), "{stale}");
                    assert!(!effect.accepted && !effect.rejected, "{stale}");
                    assert_eq!(effect.failure.is_some(), missing, "{stale}");
                    if !missing {
                        // A foreign final/usage update must not consume the
                        // current item's identity or usage high-water mark.
                        assert!(!a.receive(frame.clone()).events.is_empty(), "{frame}");
                    }
                }
            }
        }
        // Native warnings are thread-scoped and have no turn identity in the
        // pinned schema. Only the matching thread can publish a diagnostic.
        for thread in [Value::Null, json!("foreign"), json!("thread-809")] {
            let effect = active().receive(
                json!({"method":"warning","params":{"threadId":thread,"message":"diagnostic"}}),
            );
            assert_eq!(effect.events.len(), usize::from(thread == "thread-809"));
        }
    }

    #[test]
    fn terminal_status_is_native_evidence_once_and_requires_exact_identity() {
        for status in ["completed", "failed", "interrupted"] {
            let mut a = active();
            for field in ["threadId", "turnId"] {
                let mut f = completed(status);
                if field == "turnId" {
                    f["params"]["turn"]["id"] = json!("stale-turn");
                } else {
                    f["params"][field] = json!("stale-thread");
                }
                assert!(a.receive(f).events.is_empty());
                assert!(!a.ended());
            }
            let terminal = a.receive(completed(status));
            assert!(a.ended());
            assert_eq!(a.was_interrupted(), status == "interrupted");
            assert_eq!(
                terminal
                    .events
                    .iter()
                    .filter(|e| matches!(e, TurnEvent::Result { .. }))
                    .count(),
                1
            );
            assert!(terminal.events.iter().any(
                |e| matches!(e,TurnEvent::Result{is_error,..} if *is_error==(status!="completed"))
            ));
            assert!(a.receive(completed(status)).events.is_empty());
        }
        assert!(active().receive(completed("inProgress")).failure.is_some());
        let mut conflicting = completed("completed");
        conflicting["params"]["turn"]["error"] = json!({"message":"failed"});
        assert!(active().receive(conflicting).failure.is_some());
    }

    #[test]
    fn unsupported_server_requests_reply_with_rejection_and_end_the_protocol() {
        for method in [
            "item/commandExecution/requestApproval",
            "item/tool/requestUserInput",
            "unknown/request",
        ] {
            for id in [json!(17), json!("request-809")] {
                let mut a = active();
                let result = a.receive(json!({"id":id,"method":method,"params":{}}));
                assert!(result.failure.is_some());
                assert!(a.ended());
                assert_eq!(result.writes[0]["id"], id);
                assert_eq!(result.writes[0]["error"]["code"], -32601);
                assert!(result.writes[0].get("result").is_none());
            }
        }
    }

    #[test]
    fn usage_deduplicates_session_snapshots_and_counts_each_current_turn_model_call() {
        let mut a = active();
        let usage = |total_input, total_output, input, output| {
            event(
                "thread/tokenUsage/updated",
                json!({"tokenUsage":{"last":{"inputTokens":input,"outputTokens":output,"cachedInputTokens":3,"cacheWriteInputTokens":2},
                "total":{"inputTokens":total_input,"outputTokens":total_output,"cachedInputTokens":30,"cacheWriteInputTokens":20}}}),
            )
        };
        let first = a.receive(usage(1000, 2000, 10, 5));
        assert!(
            matches!(&first.events[0],TurnEvent::Usage{usage,..} if usage.input_tokens==10 && usage.output_tokens==5 && usage.cache_read_input_tokens==3 && usage.cache_creation_input_tokens==2)
        );
        assert!(a.receive(usage(1000, 2000, 10, 5)).events.is_empty());
        // A second model call can have exactly the same last usage. It is
        // additional work, not a repeated cumulative snapshot.
        let second = a.receive(usage(1010, 2005, 10, 5));
        assert!(
            matches!(&second.events[0],TurnEvent::Usage{usage,..} if usage.input_tokens==10 && usage.output_tokens==5 && usage.cache_read_input_tokens==0 && usage.cache_creation_input_tokens==0)
        );
        assert!(a.receive(usage(1010, 2005, 10, 5)).events.is_empty());
        let third = a.receive(usage(1019, 2007, 9, 2));
        assert!(
            matches!(&third.events[0],TurnEvent::Usage{usage,..} if usage.input_tokens==9 && usage.output_tokens==2)
        );
        assert!(a.receive(usage(1009, 2002, 9, 2)).events.is_empty());
    }

    #[test]
    fn text_and_multi_tool_output_preserve_native_order_unicode_and_item_identity() {
        let mut a = active();
        let delta = a.receive(event(
            "item/agentMessage/delta",
            json!({"itemId":"answer","delta":"héllo 🦀"}),
        ));
        assert!(
            matches!(&delta.events[0],TurnEvent::TextDelta{unit_id:Some(id),text} if id=="answer" && text=="héllo 🦀")
        );
        for id in ["tool-one", "tool-two"] {
            let item =
                json!({"id":id,"type":"commandExecution","command":"ls","status":"inProgress"});
            assert!(
                matches!(&a.receive(event("item/started",json!({"item":item}))).events[0],TurnEvent::ToolStarted{tool_call_id,..} if tool_call_id==id)
            );
            let frame = event(
                "item/completed",
                json!({"item":{"id":id,"type":"commandExecution","command":"ls","status":"completed","aggregatedOutput":"saved output","exitCode":0}}),
            );
            assert!(
                matches!(&a.receive(frame.clone()).events[0],TurnEvent::ToolCompleted{output,is_error:false,..} if output=="saved output")
            );
            assert!(a.receive(frame).events.is_empty());
        }
        let frame = event(
            "item/completed",
            json!({"item":{"id":"answer","type":"agentMessage","text":"héllo 🦀"}}),
        );
        assert!(
            matches!(&a.receive(frame.clone()).events[0],TurnEvent::AssistantText{unit_id,..} if unit_id=="answer")
        );
        assert!(a.receive(frame).events.is_empty());
        assert!(
            matches!(&a.receive(completed("completed")).events[0],TurnEvent::Result{text:Some(text),..} if text=="héllo 🦀")
        );
    }

    #[test]
    fn declined_native_execution_is_an_error_tool_result() {
        let mut a = active();
        let result = a.receive(event(
            "item/completed",
            json!({"item":{
                "id":"denied-command","type":"commandExecution","command":"escalate",
                "status":"declined","aggregatedOutput":"Permission denied","exitCode":null
            }}),
        ));
        assert!(result
            .events
            .iter()
            .any(|event| matches!(event, TurnEvent::ToolCompleted { is_error: true, .. })));
    }

    #[test]
    fn every_pinned_native_request_fails_even_when_its_request_id_is_missing() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/codex-app-server/protocol.schemas.json"
        ))
        .unwrap();
        let methods: Vec<&str> = schema["definitions"]["ServerRequest"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .map(|request| request["properties"]["method"]["enum"][0].as_str().unwrap())
            .collect();
        assert_eq!(methods, NATIVE_REQUEST_METHODS);
        for method in methods {
            let mut a = active();
            let effects = a.receive(json!({"method":method,"params":{}}));
            assert!(a.ended());
            assert!(effects.failure.is_some());
            assert!(effects.writes.is_empty());
        }
        assert!(active().receive(json!({"method":"turn/completed","params":{"turn":{"id":"turn-809","status":"completed"}}})).failure.is_some());
    }

    #[test]
    fn wrong_effective_model_approval_or_sandbox_fails_before_sending_input() {
        for field in ["model", "approvalPolicy", "sandbox"] {
            let mut a = adapter(None);
            initialized(&mut a);
            let mut reply = json!({"id":"g5-thread","result":{"thread":{"id":"thread-809"},"model":"frozen-model","approvalPolicy":"never","sandbox":{"type":"readOnly"}}});
            reply["result"][field] = if field == "sandbox" {
                json!({"type":"dangerFullAccess"})
            } else {
                json!("wrong")
            };
            let effect = a.receive(reply);
            assert_eq!(
                effect.failure,
                Some("codex_effective_configuration_mismatch")
            );
            assert!(effect.writes.is_empty());
            assert!(!effect.accepted);
        }
    }

    #[test]
    fn formal_thread_is_fresh_ephemeral_and_instruction_free_before_input() {
        let formal = || {
            let mut a = AppServer::formal(
                json!({"model":"frozen-model","cwd":"/tmp/launch","approvalPolicy":"never","ephemeral":true}),
                json!({"input":[{"type":"text","text":"review"}],"model":"frozen-model"}),
            );
            let init = initialized(&mut a);
            assert_eq!(init.writes[1]["method"], "thread/start");
            assert!(init.writes[1]["params"].get("sandbox").is_none());
            assert_eq!(init.writes[1]["params"]["ephemeral"], true);
            a
        };
        let confirmed = json!({"id":"g5-thread","result":{"thread":{"id":"thread-910","ephemeral":true,"path":null},"model":"frozen-model","approvalPolicy":"never","sandbox":{"type":"readOnly"},"instructionSources":[]}});
        let mut a = formal();
        let turn = a.receive(confirmed.clone());
        assert_eq!(turn.writes[0]["method"], "turn/start");
        assert_eq!(turn.writes[0]["params"]["threadId"], "thread-910");
        for (pointer, value) in [
            ("/result/thread/ephemeral", json!(false)),
            (
                "/result/thread/path",
                json!("/tmp/home/.codex/sessions/rollout.jsonl"),
            ),
            ("/result/instructionSources", json!(["/tmp/AGENTS.md"])),
            ("/result/sandbox/type", json!("workspaceWrite")),
        ] {
            let mut reply = confirmed.clone();
            *reply.pointer_mut(pointer).unwrap() = value;
            let mut a = formal();
            let effect = a.receive(reply);
            assert!(effect.failure.is_some(), "{pointer}");
            assert!(effect.writes.is_empty(), "{pointer}");
            assert!(a.ended());
        }
        // `path` is unstable and optional; `ephemeral` is the stable evidence.
        let mut reply = confirmed.clone();
        reply["result"]["thread"]
            .as_object_mut()
            .unwrap()
            .remove("path");
        assert!(formal().receive(reply).failure.is_none());
        let mut reply = confirmed.clone();
        reply["result"]["thread"]
            .as_object_mut()
            .unwrap()
            .remove("ephemeral");
        assert_eq!(
            formal().receive(reply).failure,
            Some("codex_formal_confinement_mismatch")
        );
        let mut reply = confirmed;
        reply["result"]
            .as_object_mut()
            .unwrap()
            .remove("instructionSources");
        assert_eq!(
            formal().receive(reply).failure,
            Some("codex_formal_confinement_mismatch")
        );
    }

    #[test]
    fn configuration_disables_every_pinned_nonstable_feature_and_experimental_api() {
        let features: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/codex-app-server/features.json"
        ))
        .unwrap();
        assert_eq!(features["version"], VERSION.trim());
        assert_eq!(
            initialize()["params"]["capabilities"]["experimentalApi"],
            false
        );
        let args = configuration();
        assert_eq!(&args[..3], ["app-server", "--listen", "stdio://"]);
        for feature in features["features"].as_array().unwrap() {
            if matches!(
                feature["maturity"].as_str(),
                Some("experimental" | "under development")
            ) {
                let disabled = format!("features.{}=false", feature["name"].as_str().unwrap());
                assert!(args.contains(&disabled), "{disabled}");
            }
        }
        assert!(args.contains(&"features.instant_interrupt=false".into()));
        assert!(args.contains(&"features.daemon_auto_start=false".into()));
    }
}
