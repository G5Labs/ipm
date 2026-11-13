//! Controlled App Server peer. Reuses scenario effects, never G5's supervisor.
use crate::{
    argv::{parse_codex, CodexArgs, CodexInvocation},
    emit::FrameWriter,
};
use serde_json::{json, Value};
use std::io;

type Failure = (i32, String);
fn invalid(message: &str) -> Failure {
    (2, format!("fake-codex App Server: {message}"))
}
fn read() -> Result<Value, Failure> {
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| invalid(&e.to_string()))?;
    serde_json::from_str(&line).map_err(|_| invalid("expected a JSON-RPC frame"))
}
fn write(writer: &mut FrameWriter, frame: Value) -> Result<(), Failure> {
    writer
        .emit(&frame)
        .map_err(|_| invalid("consumer closed output"))
}

pub struct Session {
    pub thread: String,
    pub turn: String,
    pub exec: Box<CodexArgs>,
    pub input: Vec<Value>,
}
impl Session {
    pub fn handshake(args: &[String]) -> Result<Self, Failure> {
        let mut exec_args = vec!["exec".into(), "--json".into()];
        let mut iter = args.iter().skip(1);
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--listen" => {
                    if iter.next().map(String::as_str) != Some("stdio://") {
                        return Err(invalid("only local stdio is supported"));
                    }
                }
                "-c" | "--config" => {
                    exec_args.push(arg.clone());
                    exec_args.push(
                        iter.next()
                            .ok_or_else(|| invalid("missing config"))?
                            .clone(),
                    );
                }
                // Formal review's strict, feature-restricted launch.
                "--strict-config" => {}
                "--disable" => {
                    let feature = iter.next().ok_or_else(|| invalid("missing feature"))?;
                    exec_args.push("-c".into());
                    exec_args.push(format!("features.{feature}=false"));
                }
                _ => return Err(invalid("unsupported launch argument")),
            }
        }
        let mut exec = match parse_codex(&exec_args).map_err(|e| invalid(&e))? {
            CodexInvocation::Exec(exec) => exec,
            _ => unreachable!(),
        };
        let mut writer = FrameWriter::new();
        let init = read()?;
        if init["method"] != "initialize"
            || init["params"]["capabilities"]["experimentalApi"] != false
        {
            return Err(invalid("experimental APIs must be explicitly disabled"));
        }
        write(
            &mut writer,
            json!({"id":init["id"],"result":{"userAgent":format!("{}/{} (fake Linux)",init["params"]["clientInfo"]["name"].as_str().unwrap_or("g5_agent"),crate::version::codex_version())}}),
        )?;
        let initialized = read()?;
        if initialized["method"] != "initialized" {
            return Err(invalid("missing initialized notification"));
        }
        let start = read()?;
        let resume = match start["method"].as_str() {
            Some("thread/start") => None,
            Some("thread/resume") => start["params"]["threadId"].as_str().map(str::to_owned),
            _ => return Err(invalid("expected thread start/resume")),
        };
        let thread = resume
            .clone()
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        if resume.is_some()
            && std::env::var("G5_FAKE_STRICT_RESUME").is_ok_and(|s| s == "1")
            && crate::session_files::find_codex_rollout(
                &crate::mcp_config::codex_home().map_err(|e| invalid(&e.to_string()))?,
                &thread,
            )
            .is_none()
        {
            write(
                &mut writer,
                json!({"id":start["id"],"error":{"code":-32600,"message":"no rollout found"}}),
            )?;
            return Err(invalid("resume refused"));
        }
        // An ephemeral thread materializes no rollout, like the pinned CLI.
        let ephemeral = start["params"]["ephemeral"] == true;
        exec.ephemeral = ephemeral;
        write(
            &mut writer,
            json!({"id":start["id"],"result":{"thread":{"id":thread,"ephemeral":ephemeral,"path":null},"model":start["params"]["model"],"approvalPolicy":start["params"]["approvalPolicy"],"sandbox":{"type":match start["params"]["sandbox"].as_str() {Some("workspace-write")=>"workspaceWrite",Some("danger-full-access")=>"dangerFullAccess",_=>"readOnly"}},"instructionSources":[]}}),
        )?;
        let turn_start = read()?;
        if turn_start["method"] != "turn/start" || turn_start["params"]["threadId"] != thread {
            return Err(invalid("wrong turn/thread"));
        }
        let parts = turn_start["params"]["input"]
            .as_array()
            .ok_or_else(|| invalid("missing input"))?;
        exec.prompt = Some(
            parts
                .iter()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        // Scenario validation must precede native acceptance. A resumed turn
        // that the peer refuses must not confirm a session eligible for harvest.
        if resume.is_some()
            && (crate::resume_fact::refuse_resume(exec.prompt.as_deref().unwrap_or_default())
                || std::env::var("G5_FAKE_REFUSE_RESUME").is_ok_and(|value| value == "1"))
        {
            write(
                &mut writer,
                json!({"id":turn_start["id"],"error":{"code":-32600,"message":"resumed turn refused"}}),
            )?;
            return Err(invalid("resumed turn refused"));
        }
        exec.images = parts
            .iter()
            .filter(|p| p["type"] == "localImage")
            .filter_map(|p| p["path"].as_str().map(str::to_owned))
            .collect();
        exec.resume = resume;
        exec.model = start["params"]["model"].as_str().map(str::to_owned);
        if let Some(effort) = turn_start["params"]["effort"].as_str() {
            exec.config_overrides
                .push(("model_reasoning_effort".into(), format!("\"{effort}\"")));
        }
        let turn = uuid::Uuid::now_v7().to_string();
        write(
            &mut writer,
            json!({"id":turn_start["id"],"result":{"turn":{"id":turn,"items":[],"status":"inProgress"}}}),
        )?;
        write(
            &mut writer,
            json!({"method":"turn/started","params":{"threadId":thread,"turn":{"id":turn,"items":[],"status":"inProgress"}}}),
        )?;
        Ok(Self {
            thread,
            turn,
            exec,
            input: vec![init, initialized, start, turn_start],
        })
    }
    pub fn wait_eof(&self) -> Result<(), Failure> {
        let mut line = String::new();
        while io::stdin()
            .read_line(&mut line)
            .map_err(|e| invalid(&e.to_string()))?
            > 0
        {
            line.clear();
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Binding {
    pub thread: String,
    pub turn: String,
}
impl Binding {
    pub fn translate(&self, frame: &Value) -> Vec<Value> {
        let event = |method: &str, mut params: Value| {
            params["threadId"] = json!(self.thread);
            params["turnId"] = json!(self.turn);
            json!({"method":method,"params":params})
        };
        match frame["type"].as_str() {
            Some("thread.started" | "turn.started") => Vec::new(),
            Some("turn.completed" | "task_complete" | "turn_complete") => {
                let u = &frame["usage"];
                let input = u["input_tokens"].as_u64().unwrap_or(0);
                let output = u["output_tokens"].as_u64().unwrap_or(0);
                let usage = json!({"inputTokens":input,"outputTokens":output,"cachedInputTokens":u["cached_input_tokens"].as_u64().unwrap_or(0),"totalTokens":input.saturating_add(output),"reasoningOutputTokens":0});
                vec![
                    event(
                        "thread/tokenUsage/updated",
                        json!({"tokenUsage":{"last":usage,"total":usage}}),
                    ),
                    event(
                        "turn/completed",
                        json!({"turn":{"id":self.turn,"items":[],"status":"completed","error":null}}),
                    ),
                ]
            }
            Some("turn.failed") => vec![event(
                "turn/completed",
                json!({"turn":{"id":self.turn,"items":[],"status":"failed","error":frame["error"]}}),
            )],
            Some("item.started" | "item.completed") => {
                let mut item = frame["item"].clone();
                item["type"] = json!(match item["type"].as_str().unwrap_or_default() {
                    "agent_message" => "agentMessage",
                    "command_execution" => "commandExecution",
                    "mcp_tool_call" => "mcpToolCall",
                    "file_change" => "fileChange",
                    "web_search" => "webSearch",
                    other => other,
                });
                if item["type"] == "commandExecution" {
                    item["aggregatedOutput"] = item["aggregated_output"].clone();
                    item["exitCode"] = item["exit_code"].clone();
                }
                vec![event(
                    if frame["type"] == "item.started" {
                        "item/started"
                    } else {
                        "item/completed"
                    },
                    json!({"item":item}),
                )]
            }
            Some("error") => vec![event(
                "error",
                json!({"error":{"message":frame["message"]},"willRetry":false}),
            )],
            Some(_) => Vec::new(),
            None => vec![frame.clone()],
        }
    }
}
