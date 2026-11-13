//! A minimal MCP stdio client: JSON-RPC 2.0, one message per line, enough to
//! `initialize`, send `notifications/initialized`, and issue one `tools/call`.
//!
//! This is what the fakes use to reach a configured MCP server (such as the
//! `fake-g5-tools` stub) for ordinary MCP transport tests.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::mcp_config::McpServerSpec;

/// The MCP protocol version the fakes announce.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Why a tool call could not be completed at the transport level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    Spawn(String),
    Timeout {
        phase: &'static str,
        after: Duration,
    },
    ServerExited {
        phase: &'static str,
    },
    Protocol(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Spawn(err) => write!(f, "could not start MCP server: {err}"),
            McpError::Timeout { phase, after } => {
                write!(f, "MCP {phase} timed out after {} ms", after.as_millis())
            }
            McpError::ServerExited { phase } => {
                write!(f, "MCP server closed its stdout during {phase}")
            }
            McpError::Protocol(err) => write!(f, "MCP protocol error: {err}"),
        }
    }
}

/// The outcome of one `tools/call`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallOutcome {
    /// `result.content` (an array of content blocks), or a synthesised text
    /// block for JSON-RPC errors.
    pub content: Value,
    /// `result.isError`, or true for JSON-RPC errors.
    pub is_error: bool,
    /// `result.structuredContent` when present.
    pub structured_content: Option<Value>,
    /// The raw JSON-RPC response.
    pub raw: Value,
    /// The JSON-RPC error message when the server refused the call.
    pub rpc_error: Option<String>,
}

impl ToolCallOutcome {
    /// Concatenated text of every `text` content block.
    pub fn text(&self) -> String {
        self.content
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|block| block.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }
}

/// One MCP stdio session against a spawned server.
pub struct McpClient {
    child: Child,
    stdin: std::process::ChildStdin,
    incoming: Receiver<Option<Value>>,
    next_id: u64,
    pub spec: McpServerSpec,
}

impl McpClient {
    /// Spawn the server and complete the `initialize` handshake.
    pub fn connect(spec: &McpServerSpec, client_name: &str) -> Result<Self, McpError> {
        let mut command = Command::new(&spec.command);
        command
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let mut child = command.spawn().map_err(|err| {
            McpError::Spawn(format!("{} {}: {err}", spec.command, spec.args.join(" ")))
        })?;
        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let (sender, incoming) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<Value>(&line) {
                            Ok(value) => {
                                if sender.send(Some(value)).is_err() {
                                    return;
                                }
                            }
                            Err(_) => {
                                // Non-JSON chatter on stdout: ignore, as the real
                                // clients tolerate it poorly but do not crash.
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = sender.send(None);
        });
        let mut client = Self {
            child,
            stdin,
            incoming,
            next_id: 1,
            spec: spec.clone(),
        };
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": client_name, "version": env!("CARGO_PKG_VERSION")}
            }),
            spec.startup_timeout,
            "initialize",
        )?;
        if init.get("error").is_some() {
            return Err(McpError::Protocol(format!("initialize rejected: {init}")));
        }
        client.notify("notifications/initialized", json!({}))?;
        Ok(client)
    }

    fn write_message(&mut self, message: &Value) -> Result<(), McpError> {
        let mut line = serde_json::to_string(message).expect("json");
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|err| McpError::Protocol(format!("cannot write to MCP server stdin: {err}")))
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), McpError> {
        self.write_message(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        phase: &'static str,
    ) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.incoming.recv_timeout(remaining) {
                Ok(Some(message)) => {
                    if message.get("id").and_then(Value::as_u64) == Some(id)
                        && message.get("method").is_none()
                    {
                        return Ok(message);
                    }
                    // A server-initiated request we cannot serve: answer with
                    // "method not found" so the server does not block on it.
                    if let (Some(req_id), Some(_)) = (message.get("id"), message.get("method")) {
                        let reply = json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "error": {"code": -32601, "message": "Method not found"}
                        });
                        self.write_message(&reply)?;
                    }
                    // Notifications and unrelated responses are ignored.
                }
                Ok(None) => return Err(McpError::ServerExited { phase }),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(McpError::Timeout {
                        phase,
                        after: timeout,
                    })
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(McpError::ServerExited { phase })
                }
            }
        }
    }

    /// Call `tool` with `arguments`, waiting up to `timeout`.
    pub fn call_tool(
        &mut self,
        tool: &str,
        arguments: &Value,
        timeout: Duration,
    ) -> Result<ToolCallOutcome, McpError> {
        let response = self.request(
            "tools/call",
            json!({"name": tool, "arguments": arguments}),
            timeout,
            "tools/call",
        )?;
        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string();
            return Ok(ToolCallOutcome {
                content: json!([{"type": "text", "text": message.clone()}]),
                is_error: true,
                structured_content: None,
                raw: response.clone(),
                rpc_error: Some(message),
            });
        }
        let result = response.get("result").cloned().unwrap_or(Value::Null);
        let content = match result.get("content") {
            Some(Value::Array(blocks)) => Value::Array(blocks.clone()),
            Some(Value::String(text)) => json!([{"type": "text", "text": text}]),
            _ => json!([]),
        };
        Ok(ToolCallOutcome {
            content,
            is_error: result
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            structured_content: result.get("structuredContent").cloned(),
            raw: response,
            rpc_error: None,
        })
    }

    /// Close the server: drop stdin, give it a moment to exit, then kill.
    pub fn shutdown(mut self) {
        drop(self.stdin);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Convenience: connect, call once, shut down.
pub fn call_once(
    spec: &McpServerSpec,
    client_name: &str,
    tool: &str,
    arguments: &Value,
    timeout: Duration,
) -> Result<ToolCallOutcome, McpError> {
    let mut client = McpClient::connect(spec, client_name)?;
    let outcome = client.call_tool(tool, arguments, timeout);
    client.shutdown();
    outcome
}

/// Split `mcp__<server>__<tool>` into its parts.
pub fn split_mcp_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server, tool))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_mcp_tool_names() {
        assert_eq!(
            split_mcp_tool_name("mcp__g5-tools__echo"),
            Some(("g5-tools", "echo"))
        );
        assert_eq!(split_mcp_tool_name("Write"), None);
        assert_eq!(split_mcp_tool_name("mcp__x__"), None);
    }

    #[test]
    fn outcome_text_joins_text_blocks() {
        let outcome = ToolCallOutcome {
            content: json!([{"type": "text", "text": "a"}, {"type": "image", "data": ""}, {"type": "text", "text": "b"}]),
            is_error: false,
            structured_content: None,
            raw: Value::Null,
            rpc_error: None,
        };
        assert_eq!(outcome.text(), "a\nb");
    }
}
