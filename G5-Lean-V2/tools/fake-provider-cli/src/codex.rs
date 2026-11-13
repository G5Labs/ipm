//! `fake-codex`: the pinned Codex App Server over stdio JSON-RPC, replayed
//! from a scenario.
//!
//! Supported invocation (`g5.chat.supervisor`):
//!
//! ```text
//! codex app-server --listen stdio:// [--strict-config] [--disable <f>]...
//!       -c approval_policy="never" -c sandbox_mode="workspace-write" ...
//! ```
//!
//! `codex_app_server::Session` performs the stable handshake and turns the
//! thread/turn requests into the scenario engine's internal [`CodexArgs`];
//! frames are then reported as App Server notifications. G5 never launches
//! `codex exec`, so the fake refuses it. The fake never emits approval frames,
//! so a `permission` scenario step is an error.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::argv::{self, parse_codex, CodexArgs, CodexInvocation};
use crate::emit::{stderr_line, FrameWriter};
use crate::mcp::{McpError, ToolCallOutcome};
use crate::mcp_config::{self, McpServerSpec};
use crate::receipt::Receipt;
use crate::replay::{self, PermissionDecision, ProviderHooks};
use crate::scenario::{Placeholders, Scenario};
use crate::session_files;
use crate::version;

/// `codex --help` text. G5 launches only `codex app-server`.
pub fn help_text() -> String {
    format!(
        "Codex CLI (fake-codex {version})\n\
\n\
Usage: codex app-server --listen stdio:// [OPTIONS]\n\
\n\
Commands:\n\
  app-server Serve one stdio App Server session (scenario replay)\n\
  login      Device authorization or login status (synthetic credentials only)\n\
\n\
Options:\n\
  -h, --help     Print help\n\
  -V, --version  Print version\n\
\n\
Environment: G5_FAKE_CODEX_SCENARIO or G5_FAKE_SCENARIO (optional; built-in reply otherwise), G5_FAKE_RECEIPT, CODEX_HOME.\n",
        version = version::codex_version()
    )
}

/// Entry point: returns the process exit code.
pub fn main_with_args(args: Vec<String>) -> i32 {
    match run(args) {
        Ok(code) => code,
        Err((code, err)) => {
            stderr_line(&err);
            code
        }
    }
}

fn run(args: Vec<String>) -> Result<i32, (i32, String)> {
    if args.first().is_some_and(|arg| arg == "app-server")
        && args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("[experimental] Run the app server\nUsage: codex app-server --listen stdio:// [-c key=value]");
        return Ok(0);
    }
    if args.first().is_some_and(|arg| arg == "login") {
        return login(&args[1..]);
    }
    let app_server = if args.first().is_some_and(|arg| arg == "app-server") {
        Some(crate::codex_app_server::Session::handshake(&args)?)
    } else {
        None
    };
    let invocation = if let Some(session) = &app_server {
        CodexInvocation::Exec(session.exec.clone())
    } else {
        parse_codex(&args).map_err(|err| (2, err))?
    };
    let exec = match invocation {
        CodexInvocation::Version => {
            println!("{}", version::codex_version_line());
            return Ok(0);
        }
        CodexInvocation::Help { subcommand: None } => {
            print!("{}", help_text());
            return Ok(0);
        }
        // G5 never launches `codex exec`; the fake serves App Server only.
        CodexInvocation::Help { .. } | CodexInvocation::Exec(_) if app_server.is_none() => {
            return Err((
                2,
                "fake-codex: only `codex app-server --listen stdio://` is implemented".to_string(),
            ))
        }
        CodexInvocation::Exec(exec) => exec,
        CodexInvocation::Help { .. } => unreachable!("App Server sessions carry a turn"),
    };

    let cwd = match &exec.cd {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            std::env::set_current_dir(&dir).map_err(|err| {
                (
                    1,
                    format!("fake-codex: cannot cd to {}: {err}", dir.display()),
                )
            })?;
            std::env::current_dir()
                .map_err(|err| (1, format!("fake-codex: cannot read cwd: {err}")))?
        }
        None => std::env::current_dir()
            .map_err(|err| (1, format!("fake-codex: cannot read cwd: {err}")))?,
    };
    let mut receipt = Receipt::new("codex", version::codex_version(), args.clone(), &cwd);
    receipt.options = options_json(&exec);
    if let Some(session) = &app_server {
        receipt.stdin_messages = session.input.clone();
    }

    let scenario_path = crate::scenario::scenario_path_from_env("G5_FAKE_CODEX_SCENARIO");

    let session_id = app_server
        .as_ref()
        .map(|s| s.thread.clone())
        .or_else(|| exec.resume.clone())
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    receipt.session_id = session_id.clone();
    receipt.resume = exec.resume.clone();

    // The handshake already applied strict and refused resumes; the turn's
    // text input is the prompt.
    let prompt = exec.prompt.clone().unwrap_or_default();
    receipt.prompt = prompt.clone();
    let reply_prompt = prompt.clone();
    if reply_prompt.contains(crate::workflows::IMAGE_SHA256_PREFIX) {
        let images = exec
            .images
            .iter()
            .map(|path| {
                std::fs::read(path)
                    .map_err(|error| (1, format!("fake-codex: cannot read image {path}: {error}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        crate::workflows::verify_image_payload(&reply_prompt, &images)
            .map_err(|error| (1, format!("fake-codex: {error}")))?;
    }

    let overrides = argv::codex_overrides_map(&exec.config_overrides);
    let model = exec
        .model
        .clone()
        .or_else(|| overrides.get("model").cloned())
        .unwrap_or_else(|| "codex-fake".to_string());
    let placeholders =
        Placeholders::new(session_id.clone(), cwd.display().to_string(), prompt, model);

    let codex_home = mcp_config::codex_home().map_err(|err| (1, format!("fake-codex: {err}")))?;
    let mut servers = if exec.ignore_user_config {
        BTreeMap::new()
    } else {
        mcp_config::codex_servers(&codex_home).map_err(|err| (1, format!("fake-codex: {err}")))?
    };
    servers.extend(
        mcp_config::codex_override_servers(&exec.config_overrides, &codex_home)
            .map_err(|err| (1, format!("fake-codex: {err}")))?,
    );
    let formal_reply =
        crate::workflows::formal_approval(&reply_prompt, &servers).map_err(|err| (1, err))?;
    let mut hooks = CodexHooks {
        servers,
        next_item: 0,
        last_agent_message: None,
        frames: Vec::new(),
    };
    let mut scenario = match &scenario_path {
        Some(path) => Scenario::load(path).map_err(|err| (1, format!("fake-codex: {err}")))?,
        None if formal_reply.is_some() => {
            crate::default_scenario::codex_default(formal_reply.as_deref().unwrap())
        }
        None => match crate::workflows::codex_workspace_workflow(&reply_prompt, &cwd)
            .map_err(|error| (1, format!("fake-codex: {error}")))?
        {
            Some(reply) => crate::default_scenario::codex_default(&reply),
            None => match crate::workflows::citation(&reply_prompt) {
                Some((observed, canonical)) => {
                    crate::default_scenario::codex_citation(&observed, &canonical)
                }
                None => crate::default_scenario::codex_default(&reply_prompt),
            },
        },
    };

    if scenario_path.is_none() {
        match crate::resume_fact::action(&reply_prompt) {
            Some(crate::resume_fact::Action::Record) => {
                crate::default_scenario::add_codex_tool_fact(
                    &mut scenario,
                    &crate::resume_fact::new_fact(),
                );
            }
            Some(crate::resume_fact::Action::Recall) => {
                let fact = crate::resume_fact::codex_fact(&codex_home, &session_id);
                scenario =
                    crate::default_scenario::codex_default(&crate::resume_fact::recall_reply(fact));
            }
            None => {}
        }
    }

    let mut writer = if let Some(session) = &app_server {
        FrameWriter::app_server(session.thread.clone(), session.turn.clone())
    } else {
        FrameWriter::new()
    };
    let outcome = replay::replay(
        &scenario,
        &placeholders,
        &mut hooks,
        &mut writer,
        &mut receipt,
        &cwd,
    );
    let exit_code = match &outcome {
        Ok(result) => result.requested_exit.unwrap_or(0),
        Err(_) => 1,
    };

    if !exec.ephemeral && !crate::resume_fact::skip_persistence(&reply_prompt) {
        match session_files::write_codex_rollout(
            &codex_home,
            &cwd,
            &session_id,
            version::codex_version(),
            &hooks.frames,
        ) {
            Ok(path) =>
            {
                #[cfg(unix)]
                if crate::resume_fact::fail_save(&reply_prompt) {
                    if let Err(error) = crate::resume_fact::plant_unsafe_session_file(&path) {
                        stderr_line(&format!("fake-codex: {error}"));
                    }
                }
            }
            Err(error) => stderr_line(&format!("fake-codex: {error}")),
        }
        if crate::resume_fact::block_after_save(&reply_prompt) {
            crate::resume_fact::mark_native_ready(&reply_prompt, &cwd, &session_id)
                .map_err(|error| (1, format!("fake-codex: {error}")))?;
            std::thread::sleep(std::time::Duration::from_secs(3_600));
        }
    }
    receipt.exit_code = exit_code;
    receipt
        .write_if_requested()
        .map_err(|err| (1, format!("fake-codex: {err}")))?;
    if outcome
        .as_ref()
        .is_ok_and(|result| result.requested_exit.is_none())
    {
        if let Some(session) = &app_server {
            session.wait_eof()?;
        }
    }
    outcome
        .map(|_| exit_code)
        .map_err(|err| (1, format!("fake-codex: {err}")))
}

/// Native login fixture. `wait` completes only after the test creates
/// CODEX_HOME/device-login-approved; its credential is synthetic and VM-local.
fn login(args: &[String]) -> Result<i32, (i32, String)> {
    if args == ["--help"] {
        println!(
            "Usage: codex login [--device-auth] [status] [-c cli_auth_credentials_store=\"file\"]"
        );
        return Ok(0);
    }
    let Some(action) = args.first() else {
        return Err((
            2,
            "fake-codex: use login --device-auth or login status".into(),
        ));
    };
    if !matches!(action.as_str(), "status" | "--device-auth")
        || !(args.len() == 1
            || (args.len() == 3
                && args[1] == "-c"
                && args[2] == "cli_auth_credentials_store=\"file\""))
    {
        return Err((2, "fake-codex: unsupported login arguments".into()));
    }
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .ok_or((2, "fake-codex: login requires CODEX_HOME".into()))?;
    if action == "status" {
        if home.join("auth.json").exists() {
            eprintln!("Logged in using ChatGPT");
            return Ok(0);
        }
        eprintln!("Not logged in");
        return Ok(1);
    }
    let mode = std::env::var("G5_FAKE_CODEX_LOGIN").unwrap_or_else(|_| "success".into());
    if mode == "disabled" {
        return Err((1, "Device code authorization is disabled".into()));
    }
    if mode == "failed" {
        return Err((1, "Could not request device code".into()));
    }
    if mode == "malformed" {
        println!("unexpected login output");
        return Ok(1);
    }
    println!("1. Open this link in your browser and sign in  https://auth.openai.com/codex/device\n\n2. Enter this one-time code after you are signed in (expires in 15 minutes)\n\n  TEST-12345\n");
    std::io::Write::flush(&mut std::io::stdout()).map_err(|e| (1, e.to_string()))?;
    if mode == "wait" {
        while !home.join("device-login-approved").exists() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    std::fs::create_dir_all(&home).map_err(|e| (1, e.to_string()))?;
    let path = home.join("auth.json");
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| (1, e.to_string()))?;
    std::io::Write::write_all(
        &mut file,
        br#"{"auth_mode":"chatgpt","tokens":{"access_token":"fake-device-token"}}"#,
    )
    .map_err(|e| (1, e.to_string()))?;
    println!("Successfully logged in.");
    Ok(0)
}

fn options_json(exec: &CodexArgs) -> Value {
    let mut map = Map::new();
    map.insert("json".into(), Value::Bool(exec.json));
    map.insert(
        "skip_git_repo_check".into(),
        Value::Bool(exec.skip_git_repo_check),
    );
    map.insert("full_auto".into(), Value::Bool(exec.full_auto));
    map.insert(
        "dangerously_bypass_approvals_and_sandbox".into(),
        Value::Bool(exec.bypass_approvals_and_sandbox),
    );
    map.insert("ephemeral".into(), Value::Bool(exec.ephemeral));
    for (key, value) in [
        ("resume", &exec.resume),
        ("output_last_message", &exec.output_last_message),
        ("output_schema", &exec.output_schema),
        ("model", &exec.model),
        ("profile", &exec.profile),
        ("sandbox", &exec.sandbox),
        ("ask_for_approval", &exec.ask_for_approval),
        ("cd", &exec.cd),
        ("color", &exec.color),
        ("local_provider", &exec.local_provider),
    ] {
        map.insert(
            key.into(),
            value.clone().map(Value::String).unwrap_or(Value::Null),
        );
    }
    map.insert(
        "add_dirs".into(),
        Value::Array(exec.add_dirs.iter().cloned().map(Value::String).collect()),
    );
    map.insert(
        "images".into(),
        Value::Array(exec.images.iter().cloned().map(Value::String).collect()),
    );
    map.insert(
        "config_overrides".into(),
        Value::Array(
            exec.config_overrides
                .iter()
                .map(|(k, v)| json!({"key": k, "value": v, "parsed": argv::unquote_toml_string(v)}))
                .collect(),
        ),
    );
    Value::Object(map)
}

struct CodexHooks {
    servers: BTreeMap<String, McpServerSpec>,
    next_item: usize,
    last_agent_message: Option<String>,
    frames: Vec<Value>,
}

impl ProviderHooks for CodexHooks {
    fn provider_name(&self) -> &'static str {
        "fake-codex"
    }

    fn resolve_server(&self, name: &str) -> Result<McpServerSpec, String> {
        self.servers.get(name).cloned().ok_or_else(|| {
            format!(
                "MCP server {name:?} is not configured in $CODEX_HOME/config.toml; known: {}",
                self.servers.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })
    }

    fn new_tool_call_id(&self) -> String {
        format!("item_{}", self.next_item)
    }

    fn mcp_call_start_frames(
        &mut self,
        _placeholders: &Placeholders,
        server: &str,
        tool: &str,
        arguments: &Value,
        id: &str,
    ) -> Vec<Value> {
        // Shapes from the codex-cli 0.153.3 recording
        // (tests/fixtures/provider-frames/codex/mcp-ask-user*.jsonl).
        vec![json!({
            "type": "item.started",
            "item": {
                "id": id,
                "type": "mcp_tool_call",
                "server": server,
                "tool": tool,
                "arguments": arguments,
                "result": null,
                "error": null,
                "status": "in_progress"
            }
        })]
    }

    fn mcp_call_result_frames(
        &mut self,
        _placeholders: &Placeholders,
        server: &str,
        tool: &str,
        arguments: &Value,
        id: &str,
        outcome: Result<&ToolCallOutcome, &McpError>,
    ) -> Vec<Value> {
        let completed_item = match outcome {
            Ok(outcome) if !outcome.is_error => json!({
                "id": id,
                "type": "mcp_tool_call",
                "server": server,
                "tool": tool,
                "arguments": arguments,
                "result": {
                    "content": outcome.content,
                    "structured_content": outcome.structured_content
                },
                "error": null,
                "status": "completed"
            }),
            // A tool-level error (`isError: true`) was not recorded; the fake
            // renders it as a failed item carrying the server's content.
            Ok(outcome) => json!({
                "id": id,
                "type": "mcp_tool_call",
                "server": server,
                "tool": tool,
                "arguments": arguments,
                "result": {
                    "content": outcome.content,
                    "structured_content": outcome.structured_content
                },
                "error": {"message": outcome.text()},
                "status": "failed"
            }),
            Err(err) => json!({
                "id": id,
                "type": "mcp_tool_call",
                "server": server,
                "tool": tool,
                "arguments": arguments,
                "result": null,
                "error": {"message": err.to_string()},
                "status": "failed"
            }),
        };
        vec![json!({"type": "item.completed", "item": completed_item})]
    }

    /// `codex exec` never emits approval frames (design v6 §8.6): refuse any
    /// frame that carries an approval `type` anywhere in it, whichever
    /// scenario line produced it (raw frame, `emit`, or an approval item
    /// wrapped in `item.started` / `event_msg`).
    fn transform(&mut self, frame: Value) -> Result<Value, String> {
        match approval_type(&frame) {
            Some(kind) => Err(format!(
                "refusing to emit an approval frame (type {kind:?}): codex exec never emits approval frames (design v6 §8.6)"
            )),
            None => Ok(frame),
        }
    }

    fn permission(
        &mut self,
        _placeholders: &Placeholders,
        tool_name: &str,
        _input: &Value,
        _id: &str,
        _receipt: &mut Receipt,
    ) -> Result<PermissionDecision, String> {
        Err(format!(
            "scenario step 'permission' ({tool_name}) is not supported: `codex exec` reads its prompt from stdin and cannot answer approval prompts, so fake-codex never emits approval frames (design v6 §8.3)"
        ))
    }

    fn denial_frames(
        &mut self,
        _placeholders: &Placeholders,
        _tool_name: &str,
        _id: &str,
        _message: &str,
    ) -> Vec<Value> {
        Vec::new()
    }

    fn observe(&mut self, frame: &Value) {
        self.frames.push(frame.clone());
        if let Some(item) = frame.get("item") {
            if let Some(index) = item
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| id.strip_prefix("item_"))
                .and_then(|n| n.parse::<usize>().ok())
            {
                self.next_item = self.next_item.max(index + 1);
            }
            if frame.get("type").and_then(Value::as_str) == Some("item.completed")
                && item.get("type").and_then(Value::as_str) == Some("agent_message")
            {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    self.last_agent_message = Some(text.to_string());
                }
            }
        }
    }
}

/// The first `type` string mentioning an approval anywhere in `value`.
pub fn approval_type(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(kind) = map.get("type").and_then(Value::as_str) {
                if kind.contains("approval") {
                    return Some(kind.to_string());
                }
            }
            map.values().find_map(approval_type)
        }
        Value::Array(items) => items.iter().find_map(approval_type),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_types_are_found_at_any_depth() {
        assert_eq!(
            approval_type(&json!({"type": "exec_approval_request", "id": "a"})).as_deref(),
            Some("exec_approval_request")
        );
        assert_eq!(
            approval_type(
                &json!({"type": "item.started", "item": {"type": "apply_patch_approval_request"}})
            )
            .as_deref(),
            Some("apply_patch_approval_request")
        );
        assert_eq!(
            approval_type(&json!({"type": "event_msg", "payload": {"type": "approval_request"}}))
                .as_deref(),
            Some("approval_request")
        );
        assert_eq!(
            approval_type(&json!({"type": "turn.completed", "usage": {}})),
            None
        );
        assert_eq!(
            approval_type(
                &json!({"type": "item.completed", "item": {"type": "mcp_tool_call", "arguments": {"approval_mode": "approve"}}})
            ),
            None,
            "keys mentioning approval are fine; only type values count"
        );
    }

    #[test]
    fn help_names_only_the_app_server() {
        assert!(help_text().contains("app-server"));
        assert!(!help_text().contains("exec"));
    }
}
