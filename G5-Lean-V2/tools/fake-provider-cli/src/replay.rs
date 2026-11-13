//! The scenario replay engine shared by both fakes. Provider-specific frame
//! synthesis (what an MCP tool call or a permission denial looks like on the
//! wire) is supplied through [`ProviderHooks`].

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::emit::{stderr_line, FrameWriter};
use crate::mcp::{self, McpError, ToolCallOutcome};
use crate::mcp_config::McpServerSpec;
use crate::receipt::{McpCallRecord, PermissionRecord, Receipt};
use crate::scenario::{Placeholders, Scenario, Step};

/// What the permission prompt decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny { message: String },
}

/// Provider-specific behaviour the engine calls into.
pub trait ProviderHooks {
    /// Name used in diagnostics and as the MCP client name.
    fn provider_name(&self) -> &'static str;

    /// Locate the MCP server named in a scenario step.
    fn resolve_server(&self, name: &str) -> Result<McpServerSpec, String>;

    /// A fresh tool-call id in the provider's style.
    fn new_tool_call_id(&self) -> String;

    /// Frames announcing an MCP tool call, written **before** the blocking
    /// call so a consumer sees the in-progress call while the server (for
    /// example a deliberately delayed MCP test call) holds the answer.
    fn mcp_call_start_frames(
        &mut self,
        placeholders: &Placeholders,
        server: &str,
        tool: &str,
        arguments: &Value,
        id: &str,
    ) -> Vec<Value>;

    /// Frames carrying the call's outcome, written after the call returns.
    fn mcp_call_result_frames(
        &mut self,
        placeholders: &Placeholders,
        server: &str,
        tool: &str,
        arguments: &Value,
        id: &str,
        outcome: Result<&ToolCallOutcome, &McpError>,
    ) -> Vec<Value>;

    /// Decide a permission step, performing whatever prompt the provider
    /// supports. Codex returns `Err` because `codex exec` cannot prompt.
    fn permission(
        &mut self,
        placeholders: &Placeholders,
        tool_name: &str,
        input: &Value,
        id: &str,
        receipt: &mut Receipt,
    ) -> Result<PermissionDecision, String>;

    /// Frames emitted after a denied permission step.
    fn denial_frames(
        &mut self,
        placeholders: &Placeholders,
        tool_name: &str,
        id: &str,
        message: &str,
    ) -> Vec<Value>;

    /// Adjust or refuse a frame before it is written: a Claude `result` frame
    /// gains the `permission_denials` the turn accumulated; a Codex approval
    /// frame is refused, whatever scenario line produced it.
    fn transform(&mut self, frame: Value) -> Result<Value, String> {
        Ok(frame)
    }

    /// Called with every frame written, in order.
    fn observe(&mut self, frame: &Value);

    /// Provider-native input consumed at this output boundary. Claude may
    /// replay a user message after init or a tool result; Codex has no live
    /// input and keeps the default empty result.
    fn after_emit(&mut self, _frame: &Value) -> Result<Vec<Value>, String> {
        Ok(Vec::new())
    }
}

/// Result of replaying a scenario: the exit code requested by an `exit`
/// step, or `None` when the scenario ran to its end.
pub struct ReplayOutcome {
    pub requested_exit: Option<i32>,
}

/// Replay `scenario` to `writer`.
pub fn replay<H: ProviderHooks>(
    scenario: &Scenario,
    placeholders: &Placeholders,
    hooks: &mut H,
    writer: &mut FrameWriter,
    receipt: &mut Receipt,
    cwd: &Path,
) -> Result<ReplayOutcome, String> {
    let provider = hooks.provider_name();
    let mut index = 0;
    while index < scenario.steps.len() {
        let step = &scenario.steps[index];
        index += 1;
        if step.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(step.delay_ms));
        }
        let location = format!("{}:{}", scenario.path.display(), step.line);
        match &step.step {
            Step::Emit(frame) => {
                let frame = placeholders.substitute(frame);
                emit(hooks, writer, receipt, &frame).map_err(|err| format!("{location}: {err}"))?;
            }
            Step::WriteFile {
                path,
                content,
                append,
            } => {
                let relative = placeholders.substitute_str(path);
                crate::scenario::validate_relative_path(&relative)
                    .map_err(|err| format!("{location}: {err}"))?;
                let target = cwd.join(&relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|err| {
                        format!("{location}: cannot create {}: {err}", parent.display())
                    })?;
                }
                let content = placeholders.substitute_str(content);
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(*append)
                    .truncate(!*append)
                    .open(&target)
                    .map_err(|err| {
                        format!("{location}: cannot open {}: {err}", target.display())
                    })?;
                file.write_all(content.as_bytes()).map_err(|err| {
                    format!("{location}: cannot write {}: {err}", target.display())
                })?;
                receipt.written_files.push(relative);
            }
            Step::McpCall {
                server,
                tool,
                arguments,
                id,
                timeout_ms,
            } => {
                let arguments = placeholders.substitute(arguments);
                let id = id
                    .as_deref()
                    .map(|raw| placeholders.substitute_str(raw))
                    .unwrap_or_else(|| hooks.new_tool_call_id());
                let spec = hooks
                    .resolve_server(server)
                    .map_err(|err| format!("{location}: {err}"))?;
                let timeout = timeout_ms
                    .map(Duration::from_millis)
                    .unwrap_or(spec.tool_timeout);
                // The real CLIs announce the call, then block on the server.
                let start_frames =
                    hooks.mcp_call_start_frames(placeholders, server, tool, &arguments, &id);
                for frame in start_frames {
                    emit(hooks, writer, receipt, &frame)
                        .map_err(|err| format!("{location}: {err}"))?;
                }
                let outcome = mcp::call_once(&spec, provider, tool, &arguments, timeout);
                receipt.mcp_calls.push(match &outcome {
                    Ok(outcome) => McpCallRecord {
                        server: server.clone(),
                        tool: tool.clone(),
                        arguments: arguments.clone(),
                        is_error: outcome.is_error,
                        content: outcome.content.clone(),
                        transport_error: None,
                    },
                    Err(err) => McpCallRecord {
                        server: server.clone(),
                        tool: tool.clone(),
                        arguments: arguments.clone(),
                        is_error: true,
                        content: Value::Array(Vec::new()),
                        transport_error: Some(err.to_string()),
                    },
                });
                let result_frames = hooks.mcp_call_result_frames(
                    placeholders,
                    server,
                    tool,
                    &arguments,
                    &id,
                    outcome.as_ref(),
                );
                for frame in result_frames {
                    emit(hooks, writer, receipt, &frame)
                        .map_err(|err| format!("{location}: {err}"))?;
                }
            }
            Step::Permission {
                tool_name,
                input,
                id,
                on_deny,
            } => {
                let input = placeholders.substitute(input);
                let id = id
                    .as_deref()
                    .map(|raw| placeholders.substitute_str(raw))
                    .unwrap_or_else(|| hooks.new_tool_call_id());
                let decision = hooks
                    .permission(placeholders, tool_name, &input, &id, receipt)
                    .map_err(|err| format!("{location}: {err}"))?;
                if let PermissionDecision::Deny { message } = decision {
                    let frames = hooks.denial_frames(placeholders, tool_name, &id, &message);
                    for frame in frames {
                        emit(hooks, writer, receipt, &frame)
                            .map_err(|err| format!("{location}: {err}"))?;
                    }
                    if let Some(label) = on_deny {
                        index = scenario.label_index(label).ok_or_else(|| {
                            format!("{location}: on_deny label {label:?} is not defined")
                        })?;
                    }
                }
            }
            Step::Label(_) => {}
            Step::Stderr(text) => stderr_line(&placeholders.substitute_str(text)),
            Step::Exit { code, stderr } => {
                if let Some(text) = stderr {
                    stderr_line(&placeholders.substitute_str(text));
                }
                return Ok(ReplayOutcome {
                    requested_exit: Some(*code),
                });
            }
        }
    }
    Ok(ReplayOutcome {
        requested_exit: None,
    })
}

pub(crate) fn emit<H: ProviderHooks>(
    hooks: &mut H,
    writer: &mut FrameWriter,
    receipt: &mut Receipt,
    frame: &Value,
) -> Result<(), String> {
    let frame = hooks.transform(frame.clone())?;
    hooks.observe(&frame);
    writer
        .emit(&frame)
        .map_err(|_| "stdout closed by the consumer".to_string())?;
    receipt.frames_emitted = writer.frames_written();
    for extra in hooks.after_emit(&frame)? {
        emit(hooks, writer, receipt, &extra)?;
    }
    Ok(())
}

/// Parse the permission prompt tool's answer: a JSON object in the tool
/// result text with `behavior` (`allow` | `deny`), `updatedInput` and
/// `message`.
pub fn parse_permission_answer(outcome: &ToolCallOutcome) -> Result<PermissionDecision, String> {
    if outcome.is_error {
        return Err(format!(
            "permission prompt tool returned an error: {}",
            outcome.text()
        ));
    }
    let text = outcome.text();
    let value: Value = serde_json::from_str(text.trim())
        .map_err(|err| format!("permission prompt tool returned non-JSON text {text:?}: {err}"))?;
    match value.get("behavior").and_then(Value::as_str) {
        Some("allow") => Ok(PermissionDecision::Allow),
        Some("deny") => Ok(PermissionDecision::Deny {
            message: value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Permission denied")
                .to_string(),
        }),
        other => Err(format!(
            "permission prompt tool returned an unknown behavior {other:?} in {text:?}"
        )),
    }
}

/// Record a permission decision on the receipt.
pub fn record_permission(
    receipt: &mut Receipt,
    tool_name: &str,
    input: &Value,
    prompt_tool: Option<&str>,
    decision: &PermissionDecision,
) {
    receipt.permissions.push(PermissionRecord {
        tool_name: tool_name.to_string(),
        input: input.clone(),
        prompt_tool: prompt_tool.map(String::from),
        behavior: match decision {
            PermissionDecision::Allow => "allow".to_string(),
            PermissionDecision::Deny { .. } => "deny".to_string(),
        },
        message: match decision {
            PermissionDecision::Allow => None,
            PermissionDecision::Deny { message } => Some(message.clone()),
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn outcome(text: &str, is_error: bool) -> ToolCallOutcome {
        ToolCallOutcome {
            content: json!([{"type": "text", "text": text}]),
            is_error,
            structured_content: None,
            raw: Value::Null,
            rpc_error: None,
        }
    }

    #[test]
    fn permission_answers_parse() {
        assert_eq!(
            parse_permission_answer(&outcome(r#"{"behavior":"allow","updatedInput":{}}"#, false))
                .unwrap(),
            PermissionDecision::Allow
        );
        assert_eq!(
            parse_permission_answer(&outcome(r#"{"behavior":"deny","message":"no"}"#, false))
                .unwrap(),
            PermissionDecision::Deny {
                message: "no".to_string()
            }
        );
        assert!(parse_permission_answer(&outcome("nope", false)).is_err());
        assert!(parse_permission_answer(&outcome(r#"{"behavior":"maybe"}"#, false)).is_err());
        assert!(parse_permission_answer(&outcome(r#"{"behavior":"allow"}"#, true)).is_err());
    }
}
