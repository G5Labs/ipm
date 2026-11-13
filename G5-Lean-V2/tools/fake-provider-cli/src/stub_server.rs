//! `fake-g5-tools`: a small MCP stdio server for provider transport tests.
//!
//! It deliberately exposes only an unrelated `echo` tool. Native provider
//! questions and permissions travel over the provider's stdio control
//! protocol and must never be simulated through MCP.
//!
//! Environment:
//! * `G5_FAKE_MCP_RESPONSES` — optional JSON file `{"tools": {"<name>":
//!   {"result": <tools/call result>, "delay_ms": n}}}` overriding built-ins.
//! * `G5_FAKE_MCP_DELAY_MS` — delay before answering every `tools/call`.
//! * `G5_FAKE_MCP_LOG` — append every received JSON-RPC message as one line.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Map, Value};

const SERVER_PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Debug, Clone)]
pub struct ScriptedTool {
    pub result: Value,
    pub delay: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct StubConfig {
    pub scripted: BTreeMap<String, ScriptedTool>,
    pub delay: Duration,
    pub log_path: Option<PathBuf>,
}

impl StubConfig {
    pub fn from_env() -> Result<Self, String> {
        let mut config = StubConfig {
            delay: std::env::var("G5_FAKE_MCP_DELAY_MS")
                .ok()
                .and_then(|raw| raw.trim().parse::<u64>().ok())
                .map(Duration::from_millis)
                .unwrap_or_default(),
            log_path: std::env::var_os("G5_FAKE_MCP_LOG").map(PathBuf::from),
            ..Default::default()
        };
        if let Some(path) = std::env::var_os("G5_FAKE_MCP_RESPONSES") {
            let text = std::fs::read_to_string(&path).map_err(|err| {
                format!(
                    "cannot read G5_FAKE_MCP_RESPONSES {}: {err}",
                    PathBuf::from(&path).display()
                )
            })?;
            let document: Value = serde_json::from_str(&text)
                .map_err(|err| format!("G5_FAKE_MCP_RESPONSES is not valid JSON: {err}"))?;
            let Some(tools) = document.get("tools").and_then(Value::as_object) else {
                return Err("G5_FAKE_MCP_RESPONSES needs a top-level \"tools\" object".to_string());
            };
            for (name, entry) in tools {
                let result = entry.get("result").cloned().ok_or_else(|| {
                    format!("G5_FAKE_MCP_RESPONSES tools.{name}.result is required")
                })?;
                let delay = entry
                    .get("delay_ms")
                    .and_then(Value::as_u64)
                    .map(Duration::from_millis)
                    .unwrap_or_default();
                config
                    .scripted
                    .insert(name.clone(), ScriptedTool { result, delay });
            }
        }
        Ok(config)
    }
}

pub fn tool_definitions() -> Value {
    json!([{
        "name": "echo",
        "description": "Echo a string for MCP transport tests.",
        "inputSchema": {
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        }
    }])
}

pub fn serve<R: BufRead, W: Write>(
    config: &StubConfig,
    reader: R,
    mut writer: W,
) -> std::io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        log_line(config, &line);
        let message: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(err) => {
                write_json(
                    &mut writer,
                    &error_response(Value::Null, -32700, &format!("Parse error: {err}")),
                )?;
                continue;
            }
        };
        if let Some(response) = handle(config, &message) {
            write_json(&mut writer, &response)?;
        }
    }
    writer.flush()
}

fn log_line(config: &StubConfig, line: &str) {
    if let Some(path) = &config.log_path {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

fn write_json<W: Write>(writer: &mut W, value: &Value) -> std::io::Result<()> {
    writer.write_all(serde_json::to_string(value).expect("json").as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

pub fn handle(config: &StubConfig, message: &Value) -> Option<Value> {
    let object = message.as_object()?;
    let id = object.get("id").cloned()?;
    let method = object.get("method").and_then(Value::as_str)?;
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(SERVER_PROTOCOL_VERSION),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "g5-tools", "title": "G5 Tools (fake)", "version": env!("CARGO_PKG_VERSION")}
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": tool_definitions()}),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new()));
            let (result, delay) = call_result(config, name, &arguments);
            let total = config.delay + delay;
            if !total.is_zero() {
                std::thread::sleep(total);
            }
            result
        }
        _ => {
            return Some(error_response(
                id,
                -32601,
                &format!("Method not found: {method}"),
            ))
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn call_result(config: &StubConfig, name: &str, arguments: &Value) -> (Value, Duration) {
    if let Some(scripted) = config.scripted.get(name) {
        return (scripted.result.clone(), scripted.delay);
    }
    match name {
        "echo" => (
            json!({
                "content": [{
                    "type": "text",
                    "text": arguments.get("text").and_then(Value::as_str).unwrap_or_default()
                }],
                "isError": false
            }),
            Duration::ZERO,
        ),
        other => (
            json!({"content": [{"type": "text", "text": format!("Unknown tool: {other}")}], "isError": true}),
            Duration::ZERO,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_list_and_echo_round_trip() {
        let config = StubConfig::default();
        let init = handle(&config, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        let list = handle(
            &config,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        )
        .unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 1);
        let echo = handle(&config, &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "echo", "arguments": {"text": "hello"}}})).unwrap();
        assert_eq!(echo["result"]["content"][0]["text"], "hello");
    }

    #[test]
    fn scripted_results_override_built_ins() {
        let mut config = StubConfig::default();
        config.scripted.insert(
            "echo".to_string(),
            ScriptedTool {
                result: json!({"content": [{"type": "text", "text": "scripted"}]}),
                delay: Duration::ZERO,
            },
        );
        let echo = handle(&config, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "echo", "arguments": {"text": "ignored"}}})).unwrap();
        assert_eq!(echo["result"]["content"][0]["text"], "scripted");
    }

    #[test]
    fn serve_handles_a_stream() {
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\nnot json\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
        let mut out = Vec::new();
        serve(&StubConfig::default(), input.as_bytes(), &mut out).unwrap();
        let lines: Vec<&str> = std::str::from_utf8(&out).unwrap().lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"id\":1"));
        assert!(lines[1].contains("-32700"));
    }
}
