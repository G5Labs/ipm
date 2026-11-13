//! `fake-claude`: the Claude Code CLI in print mode, replayed from a scenario.
//!
//! Supported invocation (design v6 §8.3):
//!
//! ```text
//! claude -p --output-format stream-json --input-format stream-json --verbose
//!        --include-partial-messages [--resume <id> | --session-id <uuid>]
//!        --permission-mode <mode> --permission-prompts host
//!        --permission-prompt-tool stdio
//!        --mcp-config <file> --strict-mcp-config --settings <file>
//!        --setting-sources project --disallowedTools AskUserQuestion
//!        [--model <m>] [--max-turns <n>] [--append-system-prompt <text>]
//! ```
//!
//! Stdin: with `--input-format stream-json`, one JSON object per line; the
//! first `{"type":"user",...}` message starts the turn and the process exits
//! only after stdin reaches EOF, as the real CLI does. Without it, the prompt
//! is the positional argument or stdin until EOF.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Map, Value};

use crate::argv::{parse_claude, ParsedArgs};
use crate::emit::{stderr_line, FrameWriter};
use crate::mcp::{McpError, ToolCallOutcome};
use crate::mcp_config::{self, McpServerSpec};
use crate::receipt::Receipt;
use crate::replay::{self, PermissionDecision, ProviderHooks};
use crate::scenario::{rfc3339_now, Placeholders, Scenario};
use crate::session_files;
use crate::version;

/// `--effort` choices the pinned CLI accepts; others are warned about and
/// ignored.
const CLAUDE_EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

const PERMISSION_MODES: &[&str] = &[
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "default",
    "manual",
    "dontAsk",
    "plan",
];

/// Text printed by `fake-claude --help`; it advertises every flag the runner
/// passes so `scripts/check-claude-cli-flags.sh` accepts the fake too.
pub fn help_text() -> String {
    format!(
        "Usage: claude [options] [prompt]\n\
\n\
fake-claude {version}: replays a scenario (or the built-in hello turn) as stream-json frames.\n\
\n\
Options:\n\
  -p, --print                           Print response and exit\n\
  --output-format <format>              (choices: \"text\", \"json\", \"stream-json\"); only stream-json is implemented\n\
  --input-format <format>               (choices: \"text\", \"stream-json\")\n\
  --verbose                             Required with --output-format stream-json\n\
  --include-partial-messages            Include partial message chunks\n\
  --replay-user-messages                Echo consumed stream-json user messages\n\
  -r, --resume [value]                  Resume a conversation by session ID\n\
  --session-id <uuid>                   Use a specific session ID\n\
  --permission-mode <mode>              (choices: \"acceptEdits\", \"auto\", \"bypassPermissions\", \"default\", \"manual\", \"dontAsk\", \"plan\")\n\
  --permission-prompts <target>         (choices: \"host\", \"none\", default: \"host\")\n\
  --permission-prompt-tool <tool>       MCP tool that answers permission prompts (mcp__<server>__<tool>)\n\
  --mcp-config <configs...>             Load MCP servers from JSON files or strings\n\
  --strict-mcp-config                   Only use MCP servers from --mcp-config\n\
  --settings <file-or-json>             Settings file (recorded, not applied)\n\
  --add-dir <directories...>            Additional directories (recorded, not applied)\n\
  --setting-sources <sources>           Comma-separated setting sources (recorded, not applied)\n\
  --disallowedTools, --disallowed-tools <tools...>\n\
  --allowedTools, --allowed-tools <tools...>\n\
  --tools <tools...>                    Select the native built-in tool surface\n\
  --system-prompt <prompt>              Replace the system instruction\n\
  --append-system-prompt <prompt>       Recorded in the receipt\n\
  --max-turns <turns>                   Maximum number of agentic turns (recorded)\n\
  --model <model>                       Model name echoed in {{{{model}}}}\n\
  --effort <level>                      Effort level (low, medium, high, xhigh, max; recorded)\n\
  -v, --version                         Output the version number\n\
  -h, --help                            Display help\n\
\n\
Environment: G5_FAKE_CLAUDE_SCENARIO or G5_FAKE_SCENARIO (optional; built-in reply otherwise), G5_FAKE_RECEIPT, G5_FAKE_STRICT_RESUME,\n\
CLAUDE_CONFIG_DIR, MCP_TOOL_TIMEOUT.\n",
        version = version::claude_version()
    )
}

/// Entry point: returns the process exit code.
pub fn main_with_args(args: Vec<String>) -> i32 {
    match run(args) {
        Ok(code) => code,
        Err(err) => {
            stderr_line(&err);
            1
        }
    }
}

fn run(args: Vec<String>) -> Result<i32, String> {
    let parsed = parse_claude(&args)?;
    // The real CLI validates numeric options before --version short-circuits.
    if let Some(raw) = parsed.value("--max-turns") {
        if raw.parse::<u64>().is_err() {
            return Err(format!(
                "error: option '--max-turns <turns>' argument '{raw}' is invalid. must be a number"
            ));
        }
    }
    if parsed.has("--version") {
        println!("{}", version::claude_version_line());
        return Ok(0);
    }
    if parsed.has("--help") {
        print!("{}", help_text());
        return Ok(0);
    }
    validate(&parsed)?;

    let cwd =
        std::env::current_dir().map_err(|err| format!("fake-claude: cannot read cwd: {err}"))?;
    // Like the pinned CLI: a missing settings file fails the launch (exit 1)
    // before any frame, and an unknown effort is warned about and ignored.
    if let Some(settings) = parsed.value("--settings") {
        let trimmed = settings.trim();
        if !trimmed.starts_with('{') {
            let path = cwd.join(trimmed);
            if !path.is_file() {
                return Err(format!(
                    "Error: Settings file not found: {}",
                    path.display()
                ));
            }
        }
    }
    if let Some(effort) = parsed.value("--effort") {
        if !CLAUDE_EFFORT_LEVELS.contains(&effort) {
            stderr_line(&format!(
                "Warning: Unknown --effort value '{effort}' \u{2014} ignoring it and using the default effort. Valid values: {}.",
                CLAUDE_EFFORT_LEVELS.join(", ")
            ));
        }
    }
    let mut receipt = Receipt::new("claude", version::claude_version(), args.clone(), &cwd);
    receipt.options = options_json(&parsed);
    receipt.positional = parsed.positional.clone();

    let scenario_path = crate::scenario::scenario_path_from_env("G5_FAKE_CLAUDE_SCENARIO");

    let resume = parsed.value("--resume").map(String::from);
    let session_id = resume
        .clone()
        .or_else(|| parsed.value("--session-id").map(String::from))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    receipt.session_id = session_id.clone();
    receipt.resume = resume.clone();

    let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
    if let (Some(resume_id), Some(config_dir)) = (&resume, &config_dir) {
        if std::env::var("G5_FAKE_STRICT_RESUME").is_ok_and(|v| v == "1") {
            let transcript = session_files::claude_transcript_path(config_dir, &cwd, resume_id);
            if !transcript.is_file() {
                return Err(format!(
                    "No conversation found with session ID: {resume_id}"
                ));
            }
        }
    }

    let restored_probe = if let (Some(id), Some(config)) = (&resume, &config_dir) {
        restored_native_question(&session_files::claude_transcript_path(config, &cwd, id))?
    } else {
        None
    };

    // Read the prompt.
    let stream_json_input = parsed.value("--input-format") == Some("stream-json");
    let mut input = if stream_json_input {
        StreamInput::start()
    } else {
        StreamInput::text(parsed.positional.first().cloned())?
    };
    let prompt = if restored_probe.is_some() {
        String::new()
    } else if stream_json_input {
        match input.wait_for_user_message() {
            Some(message) => prompt_text(&message),
            None => {
                input.finish(&mut receipt);
                return Err("fake-claude: stdin closed before a user message arrived".to_string());
            }
        }
    } else {
        input.prompt.clone().unwrap_or_default()
    };
    receipt.prompt = prompt.clone();
    if resume.is_some()
        && (crate::resume_fact::refuse_resume(&prompt)
            || std::env::var("G5_FAKE_REFUSE_RESUME").is_ok_and(|value| value == "1"))
    {
        input.finish_nonblocking(&mut receipt);
        receipt.exit_code = 1;
        receipt.write_if_requested()?;
        return Err(format!(
            "No conversation found with session ID: {session_id}"
        ));
    }
    let reply_prompt = prompt.clone();
    if reply_prompt.contains(crate::workflows::IMAGE_SHA256_PREFIX) {
        let images = input
            .first_user_message
            .as_ref()
            .and_then(|message| message.pointer("/message/content"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "image")
            .map(|block| {
                let encoded = block
                    .pointer("/source/data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "fake-claude: image block has no base64 data".to_string())?;
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| "fake-claude: image block has invalid base64 data".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        crate::workflows::verify_image_payload(&reply_prompt, &images)
            .map_err(|error| format!("fake-claude: {error}"))?;
    }

    let model = parsed
        .value("--model")
        .map(String::from)
        .unwrap_or_else(|| "claude-fake".to_string());
    let placeholders = Placeholders::new(
        session_id.clone(),
        cwd.display().to_string(),
        prompt,
        model.clone(),
    );

    let tool_timeout =
        mcp_config::claude_tool_timeout_from_env().map_err(|err| format!("fake-claude: {err}"))?;
    let servers = mcp_config::claude_servers(parsed.values("--mcp-config"), &cwd, tool_timeout)
        .map_err(|err| {
            // The pinned CLI's own message for a missing config file.
            if err.starts_with("Error: ") {
                err
            } else {
                format!("fake-claude: {err}")
            }
        })?;
    let mut hooks = ClaudeHooks {
        session_id: session_id.clone(),
        model,
        permission_mode: parsed
            .value("--permission-mode")
            .unwrap_or("default")
            .to_string(),
        permission_prompts: parsed
            .value("--permission-prompts")
            .unwrap_or("host")
            .to_string(),
        permission_prompt_tool: parsed.value("--permission-prompt-tool").map(String::from),
        servers,
        transcript: Vec::new(),
        denials: Vec::new(),
        live_input: None,
        next_user: input.first_user_message.clone(),
        replay_users: parsed.has("--replay-user-messages"),
        recorded_replays: false,
        steered_text: String::new(),
        pending_tools: HashSet::new(),
    };
    if let Some(message) = input.first_user_message.clone() {
        hooks.transcript.push(message);
    }
    let native_probe = restored_probe.or_else(|| {
        scenario_path
            .is_none()
            .then(|| native_probe(&reply_prompt))
            .flatten()
    });
    if let Some(probe) = &native_probe {
        // Question probes must obey the production launch policy. Otherwise
        // browser tests could pass even when the runner disables questions.
        let tool_selected = |flag: &str, tool: &str| {
            parsed
                .values(flag)
                .iter()
                .flat_map(|value| value.split(','))
                .any(|value| value == tool)
        };
        if matches!(
            probe.kind,
            NativeProbeKind::Question | NativeProbeKind::Exit
        ) && (hooks.permission_prompts != "host"
            || hooks.permission_prompt_tool.as_deref() != Some("stdio")
            || hooks.permission_mode == "dontAsk"
            || tool_selected("--disallowedTools", "AskUserQuestion")
            || (parsed.has("--tools") && !tool_selected("--tools", "AskUserQuestion")))
        {
            return Err(
                "native question probe requires AskUserQuestion and host stdio input".to_string(),
            );
        }
    }
    let formal_reply = crate::workflows::formal_approval(&reply_prompt, &hooks.servers)?;
    // A native probe must not run a second prompt-selected workflow before
    // the host has answered its permission or question.
    let mut scenario = if native_probe.is_some() {
        None
    } else {
        Some(match &scenario_path {
            Some(path) => Scenario::load(path).map_err(|err| format!("fake-claude: {err}"))?,
            None if formal_reply.is_some() => crate::default_scenario::claude_default(
                formal_reply.as_deref().unwrap(),
                version::claude_version(),
                &hooks.permission_mode,
                hooks.servers.keys().cloned(),
            ),
            None => match crate::workflows::claude_workflow(
                &reply_prompt,
                &cwd,
                parsed
                    .value("--max-turns")
                    .and_then(|value| value.parse().ok()),
            )
            .map_err(|err| format!("fake-claude: {err}"))?
            {
                Some(reply) => {
                    let mut scenario = crate::default_scenario::claude_turn(
                        reply.text,
                        reply.read_path.as_deref(),
                        version::claude_version(),
                        &hooks.permission_mode,
                        hooks.servers.keys().cloned(),
                    );
                    if let Some(stream) = &reply.stream_deltas {
                        crate::default_scenario::stream_text_deltas(
                            &mut scenario,
                            &stream.deltas(),
                            stream.delay_ms,
                        );
                    }
                    if reply.budget_exhausted {
                        for step in &mut scenario.steps {
                            if let crate::scenario::Step::Emit(frame) = &mut step.step {
                                if frame["type"] == "result" {
                                    frame["subtype"] = serde_json::json!("error_max_turns");
                                    frame["is_error"] = serde_json::json!(true);
                                    frame["num_turns"] = serde_json::json!(parsed
                                        .value("--max-turns")
                                        .unwrap()
                                        .parse::<u64>()
                                        .unwrap());
                                }
                            }
                        }
                    }
                    scenario
                }
                None => crate::default_scenario::claude_default(
                    &reply_prompt,
                    version::claude_version(),
                    &hooks.permission_mode,
                    hooks.servers.keys().cloned(),
                ),
            },
        })
    };

    if scenario_path.is_none() {
        match crate::resume_fact::action(&reply_prompt) {
            Some(crate::resume_fact::Action::Record) => {
                if let Some(scenario) = &mut scenario {
                    crate::default_scenario::add_claude_tool_fact(
                        scenario,
                        &crate::resume_fact::new_fact(),
                    );
                }
            }
            Some(crate::resume_fact::Action::Recall) => {
                let fact = config_dir
                    .as_deref()
                    .and_then(|dir| crate::resume_fact::claude_fact(dir, &cwd, &session_id));
                scenario = Some(crate::default_scenario::claude_turn(
                    crate::resume_fact::recall_reply(fact),
                    None,
                    version::claude_version(),
                    &hooks.permission_mode,
                    hooks.servers.keys().cloned(),
                ));
            }
            None => {}
        }
    }

    let mut writer = FrameWriter::new();
    hooks.recorded_replays = scenario.as_ref().is_some_and(|scenario| {
        scenario.steps.iter().any(|step| {
            matches!(&step.step,
            crate::scenario::Step::Emit(frame) if frame["isReplay"] == true)
        })
    });
    let outcome = if let Some(probe) = native_probe {
        run_native_probe(
            &probe,
            &mut input,
            &placeholders,
            &mut hooks,
            &mut writer,
            &mut receipt,
            &cwd,
        )
    } else {
        // The replay hooks consume live input only at Claude's native tool
        // boundary. Other messages remain queued for the next turn.
        hooks.live_input = Some(std::mem::replace(
            &mut input,
            StreamInput::text(Some(String::new()))?,
        ));
        let outcome = replay::replay(
            scenario.as_ref().expect("non-native turn has a scenario"),
            &placeholders,
            &mut hooks,
            &mut writer,
            &mut receipt,
            &cwd,
        );
        input = hooks.live_input.take().unwrap();
        outcome
    };
    let exit_code = match &outcome {
        Ok(result) => result.requested_exit.unwrap_or(0),
        Err(_) => 1,
    };

    if let Some(config_dir) = &config_dir {
        if !parsed.has("--no-session-persistence")
            && !crate::resume_fact::skip_persistence(&reply_prompt)
        {
            let mut transcript = hooks.transcript.clone();
            if crate::resume_fact::action(&reply_prompt) == Some(crate::resume_fact::Action::Record)
            {
                crate::resume_fact::externalize_claude_fact(
                    config_dir,
                    &cwd,
                    &session_id,
                    &mut transcript,
                )
                .map_err(|error| format!("fake-claude: {error}"))?;
            }
            match session_files::write_claude_transcript(config_dir, &cwd, &session_id, &transcript)
            {
                Ok(path) =>
                {
                    #[cfg(unix)]
                    if crate::resume_fact::fail_save(&reply_prompt) {
                        if let Err(error) = crate::resume_fact::plant_unsafe_session_file(&path) {
                            stderr_line(&format!("fake-claude: {error}"));
                        }
                    }
                }
                Err(error) => stderr_line(&format!("fake-claude: {error}")),
            }
            if crate::resume_fact::block_after_save(&reply_prompt) {
                crate::resume_fact::mark_native_ready(&reply_prompt, &cwd, &session_id)?;
                // Let Stop kill a running provider after its native bytes are
                // durable, before the turn can finish its normal save.
                thread::sleep(Duration::from_secs(3_600));
            }
        }
    }

    if exit_code == 0 && outcome.is_ok() && stream_json_input {
        // A message with no tool boundary left becomes another turn in this
        // process, including when the host already closed stdin. EOF never
        // discards a complete queued user frame.
        while let Some(message) = input.wait_for_user_message() {
            let transcript_start = hooks.transcript.len();
            let prompt = prompt_text(&message);
            hooks.transcript.push(message.clone());
            hooks.next_user = Some(message);
            hooks.recorded_replays = false;
            hooks.steered_text.clear();
            let next = Placeholders::new(
                session_id.clone(),
                cwd.display().to_string(),
                prompt.clone(),
                hooks.model.clone(),
            );
            let scenario = crate::default_scenario::claude_default(
                &prompt,
                version::claude_version(),
                &hooks.permission_mode,
                hooks.servers.keys().cloned(),
            );
            replay::replay(
                &scenario,
                &next,
                &mut hooks,
                &mut writer,
                &mut receipt,
                &cwd,
            )?;
            if let Some(config_dir) = &config_dir {
                if !parsed.has("--no-session-persistence")
                    && !crate::resume_fact::skip_persistence(&prompt)
                {
                    if let Err(error) = session_files::write_claude_transcript(
                        config_dir,
                        &cwd,
                        &session_id,
                        &hooks.transcript[transcript_start..],
                    ) {
                        stderr_line(&format!("fake-claude: {error}"));
                    }
                }
            }
        }
        input.finish(&mut receipt);
    } else {
        input.finish_nonblocking(&mut receipt);
    }
    receipt.exit_code = exit_code;
    receipt
        .write_if_requested()
        .map_err(|err| format!("fake-claude: {err}"))?;
    outcome
        .map(|_| exit_code)
        .map_err(|err| format!("fake-claude: {err}"))
}

#[derive(Clone, Copy)]
enum NativeProbeKind {
    Question,
    Permission,
    Exit,
}

struct NativeProbe {
    kind: NativeProbeKind,
    token: String,
    restored_tool: Option<Value>,
}

fn restored_native_question(path: &Path) -> Result<Option<NativeProbe>, String> {
    let Ok(bytes) = std::fs::read_to_string(path) else {
        return Ok(None);
    };
    let mut pending = None;
    for line in bytes.lines() {
        let Ok(frame) = serde_json::from_str::<Value>(line) else {
            // This optional probe detector must not tighten ordinary fake
            // transcript recovery, which may contain an interrupted line.
            continue;
        };
        for block in frame
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if block["type"] == "tool_use"
                && block["name"] == "AskUserQuestion"
                && block["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("toolu-native-"))
            {
                pending = Some(block.clone());
            } else if block["type"] == "tool_result"
                && pending
                    .as_ref()
                    .is_some_and(|tool: &Value| tool["id"] == block["tool_use_id"])
            {
                pending = None;
            }
        }
    }
    Ok(pending.map(|tool| NativeProbe {
        kind: NativeProbeKind::Question,
        token: tool["id"]
            .as_str()
            .unwrap()
            .trim_start_matches("toolu-native-")
            .to_owned(),
        restored_tool: Some(tool),
    }))
}

/// Select a single native-control probe by an exact UUID in the prompt. The
/// default fake never prompts unless a test explicitly requests this path.
fn native_probe(prompt: &str) -> Option<NativeProbe> {
    for word in prompt.split_whitespace() {
        for (prefix, kind) in [
            ("G5_NATIVE_QUESTION_E2E:", NativeProbeKind::Question),
            ("G5_NATIVE_PERMISSION_E2E:", NativeProbeKind::Permission),
            ("G5_NATIVE_EXIT_E2E:", NativeProbeKind::Exit),
        ] {
            if let Some(token) = word.strip_prefix(prefix) {
                if uuid::Uuid::parse_str(token).is_ok() {
                    return Some(NativeProbe {
                        kind,
                        token: token.to_string(),
                        restored_tool: None,
                    });
                }
            }
        }
    }
    None
}

/// Emit the recorded Claude control-request shape, wait for the exact host
/// response on stream-json stdin, then complete a normal provider turn.
fn run_native_probe(
    probe: &NativeProbe,
    input: &mut StreamInput,
    placeholders: &Placeholders,
    hooks: &mut ClaudeHooks,
    writer: &mut FrameWriter,
    receipt: &mut Receipt,
    cwd: &Path,
) -> Result<replay::ReplayOutcome, String> {
    let kind = probe.kind;
    let token = probe.token.as_str();
    let request_id = uuid::Uuid::new_v4().to_string();
    let question = format!("Which color for {token}?");
    let marker = format!("e2e-native-permission-{token}.txt");
    let (tool_name, tool_input) = match kind {
        NativeProbeKind::Question | NativeProbeKind::Exit => (
            "AskUserQuestion",
            json!({"questions":[{
                "question": question,
                "header": "Color",
                "multiSelect": false,
                "options": [
                    {"label":"Red","description":"Red color"},
                    {"label":"Blue","description":"Blue color"}
                ]
            }]}),
        ),
        NativeProbeKind::Permission => (
            "Bash",
            json!({"command":format!("touch {marker}"),"description":"Create a permission probe file"}),
        ),
    };
    let mut scenario = crate::default_scenario::claude_turn(
        "native probe pending".to_string(),
        None,
        version::claude_version(),
        &hooks.permission_mode,
        hooks.servers.keys().cloned(),
    );
    let init = scenario.steps.remove(0);
    let crate::scenario::Step::Emit(init) = init.step else {
        return Err("native probe default scenario omitted init".to_string());
    };
    replay::emit(hooks, writer, receipt, &placeholders.substitute(&init))?;
    let tool_id = format!("toolu-native-{token}");
    let tool_input = probe
        .restored_tool
        .as_ref()
        .map(|tool| tool["input"].clone())
        .unwrap_or(tool_input);
    if matches!(kind, NativeProbeKind::Question) && probe.restored_tool.is_none() {
        let tool = hooks.assistant_tool_use(&tool_id, tool_name, &tool_input, None);
        replay::emit(hooks, writer, receipt, &tool)?;
        if let Some(callback_id) = &input.question_hook {
            let hook_id = uuid::Uuid::new_v4().to_string();
            let frame = json!({"type":"control_request","request_id":hook_id,
                "request":{"subtype":"hook_callback","callback_id":callback_id,
                    "tool_use_id":tool_id,"input":{"hook_event_name":"PreToolUse",
                    "tool_name":tool_name,"tool_input":tool_input,"tool_use_id":tool_id,
                    "session_id":hooks.session_id}}});
            replay::emit(hooks, writer, receipt, &frame)?;
            let response = input.wait_for_control_response(&hook_id)?;
            if response
                .pointer("/response/response/hookSpecificOutput/permissionDecision")
                .and_then(Value::as_str)
                == Some("defer")
            {
                let result = json!({"type":"result","subtype":"success","is_error":false,
                    "session_id":hooks.session_id,"stop_reason":"tool_deferred","result":"",
                    "deferred_tool_use":{"id":tool_id,"name":tool_name,"input":tool_input},"usage":{}});
                replay::emit(hooks, writer, receipt, &result)?;
                return Ok(replay::ReplayOutcome {
                    requested_exit: None,
                });
            }
        }
    }
    let control = json!({
        "type":"control_request",
        "request_id":request_id,
        "request":{
            "subtype":"can_use_tool",
            "tool_use_id":tool_id,
            "tool_name":tool_name,
            "display_name":tool_name,
            "input":tool_input,
            "requires_user_interaction":true
        },
        "session_id":hooks.session_id,
        "uuid":uuid::Uuid::new_v4().to_string()
    });
    replay::emit(hooks, writer, receipt, &control)?;
    if matches!(kind, NativeProbeKind::Exit) {
        return Err("native probe exited after opening the request".to_string());
    }
    let response = input.wait_for_control_response(&request_id)?;
    let answer = &response["response"]["response"];
    let reply = match kind {
        NativeProbeKind::Question => {
            let chosen = answer["updatedInput"]["answers"][&question]
                .as_str()
                .ok_or_else(|| format!("native question response omitted answer: {response}"))?;
            format!("Native question answered {chosen} for {token}.")
        }
        NativeProbeKind::Permission => {
            let behavior = answer["behavior"].as_str().ok_or_else(|| {
                format!("native permission response omitted behavior: {response}")
            })?;
            if behavior == "allow" {
                std::fs::write(cwd.join(&marker), b"native permission allowed\n")
                    .map_err(|err| format!("native permission effect failed: {err}"))?;
            }
            format!("Native permission {behavior} for {token}.")
        }
        NativeProbeKind::Exit => unreachable!("exit probe returns before waiting for a reply"),
    };
    if matches!(kind, NativeProbeKind::Question) {
        let result = hooks.user_tool_result(&tool_id, Value::String(reply.clone()), false);
        replay::emit(hooks, writer, receipt, &result)?;
    }
    let mut completion = crate::default_scenario::claude_turn(
        reply,
        None,
        version::claude_version(),
        &hooks.permission_mode,
        hooks.servers.keys().cloned(),
    );
    completion.steps.remove(0);
    replay::replay(&completion, placeholders, hooks, writer, receipt, cwd)
}

fn validate(parsed: &ParsedArgs) -> Result<(), String> {
    if !parsed.has("--print") {
        return Err("fake-claude: only print mode is implemented; pass -p/--print".to_string());
    }
    match parsed.value("--output-format") {
        Some("stream-json") => {}
        Some(other @ ("text" | "json")) => {
            return Err(format!(
                "fake-claude: --output-format {other} is not implemented; use stream-json"
            ))
        }
        Some(other) => {
            return Err(format!(
                "error: option '--output-format <format>' argument '{other}' is invalid. Allowed choices are text, json, stream-json."
            ))
        }
        None => {
            return Err("fake-claude: --output-format stream-json is required".to_string())
        }
    }
    if !parsed.has("--verbose") {
        return Err(
            "Error: When using --print, --output-format=stream-json requires --verbose".to_string(),
        );
    }
    if let Some(format) = parsed.value("--input-format") {
        if !matches!(format, "text" | "stream-json") {
            return Err(format!(
                "error: option '--input-format <format>' argument '{format}' is invalid. Allowed choices are text, stream-json."
            ));
        }
    }
    if let Some(id) = parsed.value("--session-id") {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("Error: --session-id must be a valid UUID".to_string());
        }
    }
    if let Some(mode) = parsed.value("--permission-mode") {
        if !PERMISSION_MODES.contains(&mode) {
            return Err(format!(
                "error: option '--permission-mode <mode>' argument '{mode}' is invalid. Allowed choices are {}.",
                PERMISSION_MODES.join(", ")
            ));
        }
    }
    if let Some(target) = parsed.value("--permission-prompts") {
        if !matches!(target, "host" | "none") {
            return Err(format!(
                "error: option '--permission-prompts <target>' argument '{target}' is invalid. Allowed choices are host, none."
            ));
        }
    }
    if let Some(tool) = parsed.value("--permission-prompt-tool") {
        if tool != "stdio" {
            return Err(format!(
                "fake-claude: --permission-prompt-tool must be stdio, got {tool:?}"
            ));
        }
    }
    Ok(())
}

fn options_json(parsed: &ParsedArgs) -> Value {
    let mut map = Map::new();
    for flag in &parsed.flags {
        map.insert(flag.clone(), Value::Bool(true));
    }
    for (name, values) in &parsed.options {
        map.insert(
            name.clone(),
            Value::Array(values.iter().cloned().map(Value::String).collect()),
        );
    }
    Value::Object(map)
}

/// The text of a `stream-json` user message: string content, or the text
/// blocks of array content joined with newlines.
pub fn prompt_text(message: &Value) -> String {
    let content = message
        .get("message")
        .and_then(|m| m.get("content"))
        .cloned()
        .unwrap_or(Value::Null);
    match content {
        Value::String(text) => text,
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Stdin handling for both input formats.
struct StreamInput {
    prompt: Option<String>,
    raw: String,
    messages: Vec<Value>,
    first_user_message: Option<Value>,
    question_hook: Option<String>,
    lines: Option<Receiver<Option<String>>>,
    reader: Option<JoinHandle<()>>,
    queued_users: VecDeque<Value>,
}

impl StreamInput {
    /// Text mode: positional prompt, else read stdin to EOF now.
    fn text(positional: Option<String>) -> Result<Self, String> {
        let mut raw = String::new();
        let prompt = match positional {
            Some(prompt) => prompt,
            None => {
                io::stdin()
                    .lock()
                    .read_to_string(&mut raw)
                    .map_err(|err| format!("fake-claude: cannot read stdin: {err}"))?;
                raw.trim_end_matches('\n').to_string()
            }
        };
        Ok(Self {
            prompt: Some(prompt),
            raw,
            messages: Vec::new(),
            first_user_message: None,
            queued_users: VecDeque::new(),
            question_hook: None,
            lines: None,
            reader: None,
        })
    }

    /// stream-json mode: read stdin line by line on a thread.
    fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let reader = thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        if sender.send(Some(line)).is_err() {
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = sender.send(None);
        });
        Self {
            queued_users: VecDeque::new(),
            prompt: None,
            raw: String::new(),
            messages: Vec::new(),
            first_user_message: None,
            question_hook: None,
            lines: Some(receiver),
            reader: Some(reader),
        }
    }

    fn accept(&mut self, line: String) -> Option<Value> {
        self.raw.push_str(&line);
        self.raw.push('\n');
        if line.trim().is_empty() {
            return None;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => {
                self.messages.push(value.clone());
                if value["type"] == "control_request" && value["request"]["subtype"] == "initialize"
                {
                    self.question_hook = value
                        .pointer("/request/hooks/PreToolUse/0/hookCallbackIds/0")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let response = json!({"type":"control_response","response":{"subtype":"success",
                        "request_id":value["request_id"],"response":{}}});
                    let _ = FrameWriter::new().emit(&response);
                }
                if value.get("type").and_then(Value::as_str) == Some("user") {
                    if self.first_user_message.is_none() {
                        self.first_user_message = Some(value.clone());
                    }
                    return Some(value);
                }
                None
            }
            Err(err) => {
                stderr_line(&format!("fake-claude: ignoring non-JSON stdin line: {err}"));
                None
            }
        }
    }

    /// Block until the first user message or EOF.
    fn wait_for_user_message(&mut self) -> Option<Value> {
        if let Some(message) = self.queued_users.pop_front() {
            return Some(message);
        }
        let receiver = self.lines.take()?;
        let mut found = None;
        while let Ok(Some(line)) = receiver.recv() {
            if let Some(message) = self.accept(line) {
                found = Some(message);
                break;
            }
        }
        self.lines = Some(receiver);
        found
    }

    fn wait_for_control_response(&mut self, request_id: &str) -> Result<Value, String> {
        let receiver = self
            .lines
            .take()
            .ok_or_else(|| "native probe requires stream-json input".to_string())?;
        let result = loop {
            match receiver.recv_timeout(Duration::from_secs(900)) {
                Ok(Some(line)) => {
                    let previous_count = self.messages.len();
                    if let Some(message) = self.accept(line) {
                        self.queued_users.push_back(message);
                    }
                    if self.messages.len() == previous_count {
                        continue;
                    }
                    let Some(value) = self.messages.last() else {
                        continue;
                    };
                    if value["type"] == "control_response"
                        && value["response"]["request_id"] == request_id
                    {
                        break Ok(value.clone());
                    }
                }
                Ok(None) => break Err("stdin closed before native response".to_string()),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    break Err("native response timed out".to_string());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break Err("stdin disconnected before native response".to_string());
                }
            }
        };
        self.lines = Some(receiver);
        result
    }

    /// Drain stdin to EOF (blocking), then fill the receipt.
    fn finish(&mut self, receipt: &mut Receipt) {
        if let Some(receiver) = self.lines.take() {
            for line in receiver.iter().flatten() {
                self.accept(line);
            }
        }
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
        self.fill(receipt);
    }

    /// Take whatever stdin has delivered so far without waiting for EOF.
    fn finish_nonblocking(&mut self, receipt: &mut Receipt) {
        if let Some(receiver) = self.lines.take() {
            while let Ok(Some(line)) = receiver.try_recv() {
                self.accept(line);
            }
        }
        self.fill(receipt);
    }

    fn take_queued_users(&mut self) -> Vec<Value> {
        if let Some(receiver) = self.lines.take() {
            while let Ok(Some(line)) = receiver.try_recv() {
                if let Some(message) = self.accept(line) {
                    self.queued_users.push_back(message);
                }
            }
            self.lines = Some(receiver);
        }
        self.queued_users.drain(..).collect()
    }

    fn fill(&self, receipt: &mut Receipt) {
        receipt.stdin = self.raw.clone();
        receipt.stdin_messages = self.messages.clone();
    }
}

use std::io::Read as _;

struct ClaudeHooks {
    session_id: String,
    model: String,
    permission_mode: String,
    permission_prompts: String,
    permission_prompt_tool: Option<String>,
    servers: BTreeMap<String, McpServerSpec>,
    transcript: Vec<Value>,
    /// Denied permission steps, reported on the next `result` frame the way
    /// the real CLI fills `permission_denials`.
    denials: Vec<Value>,
    live_input: Option<StreamInput>,
    next_user: Option<Value>,
    replay_users: bool,
    recorded_replays: bool,
    steered_text: String,
    pending_tools: HashSet<String>,
}

impl ClaudeHooks {
    fn replay_user(&self, mut message: Value) -> Option<Value> {
        if !self.replay_users {
            return None;
        }
        message["isReplay"] = json!(true);
        message["session_id"] = json!(self.session_id);
        if !message["uuid"].is_string() {
            message["uuid"] = json!(uuid::Uuid::new_v4().to_string());
        }
        Some(message)
    }
    fn envelope(&self, kind: &str, message: Value) -> Value {
        json!({
            "type": kind,
            "message": message,
            "parent_tool_use_id": null,
            "session_id": self.session_id,
            "uuid": uuid::Uuid::new_v4().to_string(),
            "timestamp": rfc3339_now(),
        })
    }

    /// An `assistant` frame carrying one `tool_use` block, shaped like the
    /// 2.1.270 recording (`caller`, `wire_tool_inputs`, and `tool_use_meta`
    /// for MCP tools).
    fn assistant_tool_use(
        &self,
        id: &str,
        name: &str,
        input: &Value,
        mcp_server: Option<&str>,
    ) -> Value {
        let mut frame = self.envelope(
            "assistant",
            json!({
                "model": self.model,
                "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
                "type": "message",
                "role": "assistant",
                "content": [{"type": "tool_use", "id": id, "name": name, "input": input, "caller": {"type": "direct"}}],
                "container": null,
                "stop_reason": null,
                "stop_sequence": null,
                "stop_details": null,
                "usage": {
                    "input_tokens": 0,
                    "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": 0,
                    "output_tokens": 0,
                    "service_tier": "standard"
                },
                "input_transformations": [],
                "diagnostics": null,
                "context_management": null
            }),
        );
        frame["request_id"] = Value::String(format!("req_{}", uuid::Uuid::new_v4().simple()));
        frame["wire_tool_inputs"] = json!({ id: input });
        if let Some(server) = mcp_server {
            let display = name
                .rsplit("__")
                .next()
                .unwrap_or(name)
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            frame["tool_use_meta"] =
                json!([{"id": id, "display_name": display, "server_display_name": server}]);
        }
        frame
    }

    /// A `user` frame carrying one `tool_result` block for an MCP tool: the
    /// content blocks the server returned, mirrored in `tool_use_result`.
    fn user_tool_result(&self, id: &str, content: Value, is_error: bool) -> Value {
        let mut block = json!({"tool_use_id": id, "type": "tool_result", "content": content});
        if is_error {
            block["is_error"] = Value::Bool(true);
        }
        let mut frame = self.envelope("user", json!({"role": "user", "content": [block]}));
        frame["tool_use_result"] =
            content_or_error(&frame["message"]["content"][0]["content"], is_error);
        frame
    }

    /// The `user` frame the real CLI emits for a permission denial.
    fn user_permission_denial(&self, id: &str, message: &str) -> Value {
        let mut frame = self.envelope(
            "user",
            json!({
                "role": "user",
                "content": [{"tool_use_id": id, "type": "tool_result", "content": message, "is_error": true}]
            }),
        );
        frame["tool_use_result"] = Value::String(format!("Error: {message}"));
        frame["tool_result_meta"] = json!([{"id": id, "non_execution_kind": "permission-rule"}]);
        frame
    }
}

fn content_or_error(content: &Value, is_error: bool) -> Value {
    if is_error {
        let text = content
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .or_else(|| content.as_str().map(String::from))
            .unwrap_or_default();
        Value::String(format!("Error: {text}"))
    } else {
        content.clone()
    }
}

impl ProviderHooks for ClaudeHooks {
    fn provider_name(&self) -> &'static str {
        "fake-claude"
    }

    fn resolve_server(&self, name: &str) -> Result<McpServerSpec, String> {
        self.servers.get(name).cloned().ok_or_else(|| {
            format!(
                "MCP server {name:?} is not configured; --mcp-config names: {}",
                self.servers.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })
    }

    fn new_tool_call_id(&self) -> String {
        let hex = uuid::Uuid::new_v4().simple().to_string();
        format!("toolu_{}", &hex[..24])
    }

    fn mcp_call_start_frames(
        &mut self,
        _placeholders: &Placeholders,
        server: &str,
        tool: &str,
        arguments: &Value,
        id: &str,
    ) -> Vec<Value> {
        let name = format!("mcp__{server}__{tool}");
        vec![self.assistant_tool_use(id, &name, arguments, Some(server))]
    }

    fn mcp_call_result_frames(
        &mut self,
        _placeholders: &Placeholders,
        _server: &str,
        _tool: &str,
        _arguments: &Value,
        id: &str,
        outcome: Result<&ToolCallOutcome, &McpError>,
    ) -> Vec<Value> {
        let (content, is_error) = match outcome {
            Ok(outcome) => (outcome.content.clone(), outcome.is_error),
            Err(err) => (
                json!([{"type": "text", "text": format!("MCP error: {err}")}]),
                true,
            ),
        };
        vec![self.user_tool_result(id, content, is_error)]
    }

    fn permission(
        &mut self,
        _placeholders: &Placeholders,
        tool_name: &str,
        input: &Value,
        id: &str,
        receipt: &mut Receipt,
    ) -> Result<PermissionDecision, String> {
        let prompt_tool = self.permission_prompt_tool.clone();
        let decision = if self.permission_mode == "bypassPermissions" {
            PermissionDecision::Allow
        } else if self.permission_prompts == "none" {
            PermissionDecision::Deny {
                message: format!("Permission to use {tool_name} was denied: permission prompts are disabled (--permission-prompts none)"),
            }
        } else {
            match prompt_tool.as_deref() {
                None => PermissionDecision::Deny {
                    message: format!(
                        "Claude requested permissions to use {tool_name}, but you haven't granted it yet."
                    ),
                },
                Some("stdio") => PermissionDecision::Deny {
                    message: "the standalone fake has no stdio control peer; native permission protocol coverage lives at the agent-turn boundary".to_string(),
                },
                Some(_) => unreachable!("permission prompt transport validated at startup"),
            }
        };
        replay::record_permission(receipt, tool_name, input, prompt_tool.as_deref(), &decision);
        if let PermissionDecision::Deny { .. } = &decision {
            self.denials
                .push(json!({"tool_name": tool_name, "tool_use_id": id, "tool_input": input}));
        }
        Ok(decision)
    }

    fn denial_frames(
        &mut self,
        _placeholders: &Placeholders,
        _tool_name: &str,
        id: &str,
        message: &str,
    ) -> Vec<Value> {
        vec![self.user_permission_denial(id, message)]
    }

    fn transform(&mut self, mut frame: Value) -> Result<Value, String> {
        // Explicit scenarios can make their answer depend on the input
        // consumed at a tool boundary, without changing the owner's frame.
        fn substitute(value: &mut Value, text: &str) {
            match value {
                Value::String(s) => *s = s.replace("{{steered_prompt}}", text),
                Value::Array(values) => values.iter_mut().for_each(|v| substitute(v, text)),
                Value::Object(values) => values.values_mut().for_each(|v| substitute(v, text)),
                _ => {}
            }
        }
        if frame["isReplay"] != true {
            substitute(&mut frame, &self.steered_text);
        }
        if frame.get("type").and_then(Value::as_str) == Some("result") && !self.denials.is_empty() {
            if let Some(object) = frame.as_object_mut() {
                let existing = object
                    .get("permission_denials")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if existing.is_empty() {
                    object.insert(
                        "permission_denials".to_string(),
                        Value::Array(self.denials.clone()),
                    );
                }
            }
        }
        Ok(frame)
    }

    fn observe(&mut self, frame: &Value) {
        let kind = frame.get("type").and_then(Value::as_str);
        if frame["isReplay"] != true && matches!(kind, Some("assistant") | Some("user")) {
            self.transcript.push(frame.clone());
            for block in frame["message"]["content"].as_array().into_iter().flatten() {
                if block["type"] == "tool_use" {
                    if let Some(id) = block["id"].as_str() {
                        self.pending_tools.insert(id.to_owned());
                    }
                } else if block["type"] == "tool_result" {
                    if let Some(id) = block["tool_use_id"].as_str() {
                        self.pending_tools.remove(id);
                    }
                }
            }
        }
    }

    fn after_emit(&mut self, frame: &Value) -> Result<Vec<Value>, String> {
        // A raw recording controls its own replay frames. Dynamic scenarios
        // use the native boundaries observed in the pinned CLI recordings.
        if self.recorded_replays {
            return Ok(Vec::new());
        }
        if frame["type"] == "system" && frame["subtype"] == "init" {
            return Ok(self
                .next_user
                .take()
                .and_then(|m| self.replay_user(m))
                .into_iter()
                .collect());
        }
        let tool_result = frame["type"] == "user"
            && frame["message"]["content"]
                .as_array()
                .is_some_and(|blocks| blocks.iter().any(|b| b["type"] == "tool_result"));
        if !tool_result || !self.pending_tools.is_empty() {
            return Ok(Vec::new());
        }
        let messages = self
            .live_input
            .as_mut()
            .map(StreamInput::take_queued_users)
            .unwrap_or_default();
        let mut echoes = Vec::new();
        for message in messages {
            let text = prompt_text(&message);
            if !self.steered_text.is_empty() {
                self.steered_text.push('\n');
            }
            self.steered_text.push_str(&text);
            let source_uuid = message["uuid"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            self.transcript.push(json!({"type":"attachment","attachment":{
                "type":"queued_command","prompt":message["message"]["content"],"source_uuid":source_uuid}}));
            let mut message = message;
            message["uuid"] = json!(source_uuid);
            if let Some(echo) = self.replay_user(message) {
                echoes.push(echo);
            }
        }
        Ok(echoes)
    }
}

/// Convenience for tests: the transcript path the fake writes for `cwd`.
pub fn transcript_path(config_dir: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    session_files::claude_transcript_path(config_dir, cwd, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_text_handles_string_and_block_content() {
        assert_eq!(
            prompt_text(&json!({"type": "user", "message": {"role": "user", "content": "hi"}})),
            "hi"
        );
        assert_eq!(
            prompt_text(
                &json!({"type": "user", "message": {"role": "user", "content": [
                    {"type": "text", "text": "a"},
                    {"type": "image", "source": {}},
                    {"type": "text", "text": "b"}
                ]}})
            ),
            "a\nb"
        );
        assert_eq!(prompt_text(&json!({"type": "user"})), "");
    }

    #[test]
    fn help_text_advertises_the_runner_flags() {
        let help = help_text();
        for flag in [
            "--print",
            "--output-format",
            "--input-format",
            "--verbose",
            "--include-partial-messages",
            "--resume",
            "--session-id",
            "--permission-mode",
            "--permission-prompts",
            "--permission-prompt-tool",
            "--mcp-config",
            "--strict-mcp-config",
            "--settings",
            "--setting-sources",
            "--disallowedTools",
            "--append-system-prompt",
            "--max-turns",
            "--model",
            "--effort",
            "\"stream-json\"",
            "\"host\"",
            "\"bypassPermissions\"",
        ] {
            assert!(help.contains(flag), "help lacks {flag}");
        }
    }
}
