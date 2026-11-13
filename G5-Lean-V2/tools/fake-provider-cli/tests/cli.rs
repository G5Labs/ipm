//! Black-box tests of the three binaries: they spawn `fake-claude`,
//! `fake-codex` and `fake-g5-tools` exactly as the provider runner will.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};

const FAKE_CLAUDE: &str = env!("CARGO_BIN_EXE_fake-claude");
const FAKE_CODEX: &str = env!("CARGO_BIN_EXE_fake-codex");
const FAKE_G5_TOOLS: &str = env!("CARGO_BIN_EXE_fake-g5-tools");

#[test]
fn codex_device_login_status_and_failure_modes_match_the_native_contract() {
    for mode in ["success", "disabled", "failed", "malformed"] {
        let root = tempfile::tempdir().unwrap();
        let env = [
            ("CODEX_HOME", root.path().to_str().unwrap()),
            ("G5_FAKE_CODEX_LOGIN", mode),
        ];
        let status = run(FAKE_CODEX, root.path(), &["login", "status"], &env, None);
        assert_eq!(status.code(), 1);
        assert!(status.stderr.contains("Not logged in"));
        let login = run(
            FAKE_CODEX,
            root.path(),
            &[
                "login",
                "--device-auth",
                "-c",
                "cli_auth_credentials_store=\"file\"",
            ],
            &env,
            None,
        );
        if mode == "success" {
            assert_eq!(login.code(), 0);
            assert!(login
                .stdout
                .contains("https://auth.openai.com/codex/device"));
            assert!(login.stdout.contains("TEST-12345"));
            assert_eq!(
                run(FAKE_CODEX, root.path(), &["login", "status"], &env, None).code(),
                0
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(root.path().join("auth.json"))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        } else {
            assert_eq!(login.code(), 1);
            assert!(!root.path().join("auth.json").exists());
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn claude_pin() -> String {
    fs::read_to_string(repo_root().join("scripts/claude-code-cli-version"))
        .unwrap()
        .trim()
        .to_string()
}

fn codex_pin() -> String {
    fs::read_to_string(repo_root().join("scripts/codex-cli-version"))
        .unwrap()
        .trim()
        .to_string()
}

struct Run {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

impl Run {
    fn frames(&self) -> Vec<Value> {
        self.stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<Value>(line)
                    .unwrap_or_else(|err| panic!("stdout line is not JSON ({err}): {line}"))
            })
            .collect()
    }

    fn code(&self) -> i32 {
        self.status.code().expect("exit code")
    }
}

/// `bin` in `cwd` with the test environment scrubbed of ambient fake, provider
/// and Git configuration, then `env` applied.
fn fake_command(bin: &str, cwd: &Path, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(bin);
    command
        .current_dir(cwd)
        .env_remove("G5_FAKE_SCENARIO")
        .env_remove("G5_FAKE_CLAUDE_SCENARIO")
        .env_remove("G5_FAKE_CODEX_SCENARIO")
        .env_remove("G5_FAKE_RECEIPT")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("MCP_TOOL_TIMEOUT")
        .env_remove("G5_FAKE_STRICT_RESUME")
        .env_remove("G5_FAKE_REFUSE_RESUME")
        .env_remove("G5_FAKE_MCP_DELAY_MS")
        .env_remove("G5_FAKE_G5D_BIN")
        .env("G5_FAKE_GIT_BIN", native_git_binary())
        .env_remove("G5D_TOKEN")
        .env_remove("G5D_API_URL")
        .env_remove("G5D_WORKSPACE_ROOT")
        .env_remove("GIT_AUTHOR_NAME")
        .env_remove("GIT_AUTHOR_EMAIL")
        .env_remove("GIT_COMMITTER_NAME")
        .env_remove("GIT_COMMITTER_EMAIL")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", cwd);
    for (key, value) in env {
        command.env(key, value);
    }
    command
}

fn run(bin: &str, cwd: &Path, args: &[&str], env: &[(&str, &str)], stdin: Option<&str>) -> Run {
    let mut command = fake_command(bin, cwd, env);
    command
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn");
    if let Some(input) = stdin {
        let mut handle = child.stdin.take().unwrap();
        // A fake that rejects its arguments exits before reading stdin; the
        // resulting BrokenPipe on this write is not a test failure.
        if let Err(err) = handle.write_all(input.as_bytes()) {
            assert_eq!(
                err.kind(),
                std::io::ErrorKind::BrokenPipe,
                "stdin write failed: {err}"
            );
        }
        drop(handle);
    }
    let output = child.wait_with_output().expect("wait");
    Run {
        status: output.status,
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn user_message(text: &str) -> String {
    format!(
        "{}\n",
        json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": text}]}})
    )
}

fn claude_mcp_config(dir: &Path) -> PathBuf {
    let path = dir.join(".mcp.json");
    write(
        &path,
        &json!({"mcpServers": {"g5-tools": {"command": FAKE_G5_TOOLS, "args": [], "cwd": "."}}})
            .to_string(),
    );
    path
}

fn codex_home_with_stub(dir: &Path, extra: &str) -> PathBuf {
    let home = dir.join("codex-home");
    write(
        &home.join("config.toml"),
        &format!(
            "approval_policy = \"never\"\nsandbox_mode = \"workspace-write\"\n[mcp_servers.g5-tools]\ncommand = \"{FAKE_G5_TOOLS}\"\nargs = []\ncwd = \"{}\"\n{extra}",
            dir.display()
        ),
    );
    home
}

const CLAUDE_BASE_ARGS: &[&str] = &[
    "-p",
    "--output-format",
    "stream-json",
    "--input-format",
    "stream-json",
    "--verbose",
    "--include-partial-messages",
];

const TEXT_SCENARIO: &str = r#"{"type":"system","subtype":"init","cwd":"{{cwd}}","session_id":"{{session_id}}","model":"{{model}}","tools":[],"mcp_servers":[],"permissionMode":"bypassPermissions","uuid":"{{uuid}}"}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo: {{prompt}}"}},"session_id":"{{session_id}}","parent_tool_use_id":null,"uuid":"{{uuid}}"}
{"delay_ms":5,"type":"assistant","message":{"id":"{{uuid:msg}}","role":"assistant","content":[{"type":"text","text":"echo: {{prompt}}"}],"model":"{{model}}"},"session_id":"{{session_id}}","uuid":"{{uuid}}"}
{"type":"result","subtype":"success","is_error":false,"result":"echo: {{prompt}}","session_id":"{{session_id}}","num_turns":1,"uuid":"{{uuid}}"}
"#;

#[test]
fn claude_native_question_probes_respect_launch_restrictions() {
    for restrictions in [
        vec!["--permission-prompts", "none"],
        vec!["--disallowedTools", "AskUserQuestion"],
        vec!["--tools", "Read"],
        vec!["--permission-mode", "dontAsk"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let args = [
            CLAUDE_BASE_ARGS,
            &[
                "--permission-mode",
                "bypassPermissions",
                "--permission-prompts",
                "host",
                "--permission-prompt-tool",
                "stdio",
            ],
            &restrictions,
        ]
        .concat();
        let output = run(
            FAKE_CLAUDE,
            dir.path(),
            &args,
            &[],
            Some(&user_message(
                "G5_NATIVE_QUESTION_E2E:00000000-0000-4000-8000-000000000001",
            )),
        );
        assert_eq!(output.code(), 1);
        assert!(output.stderr.contains("native question probe requires"));
        assert!(!output
            .frames()
            .iter()
            .any(|frame| frame["type"] == "control_request"));
    }
}

#[test]
fn claude_native_question_scan_preserves_ordinary_resume_with_partial_lines() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().canonicalize().unwrap();
    let config = dir.path().join("native-home");
    let session = uuid::Uuid::new_v4().to_string();
    let transcript = fake_provider_cli::claude::transcript_path(&config, &cwd, &session);
    fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    fs::write(
        &transcript,
        "{\"type\":\"user\",\"message\":{\"content\":\"old input\"}}\npartial trailing line",
    )
    .unwrap();
    let result = run(
        FAKE_CLAUDE,
        &cwd,
        &[CLAUDE_BASE_ARGS, &["--resume", &session]].concat(),
        &[
            ("CLAUDE_CONFIG_DIR", config.to_str().unwrap()),
            ("G5_FAKE_STRICT_RESUME", "1"),
        ],
        Some(&user_message("ordinary resumed input")),
    );
    assert_eq!(result.code(), 0, "{}", result.stderr);
    assert!(result
        .frames()
        .iter()
        .any(|frame| frame["type"] == "result" && frame["is_error"] == false));
}

#[test]
fn claude_native_question_deferral_restores_the_same_tool_from_copied_jsonl() {
    fn next_control(reader: &mut BufReader<std::process::ChildStdout>) -> Value {
        loop {
            let mut line = String::new();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "missing native callback"
            );
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["type"] == "control_request" {
                return value;
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().canonicalize().unwrap();
    let first_home = dir.path().join("first");
    let restored_home = dir.path().join("restored");
    let native_session = uuid::Uuid::new_v4().to_string();
    let token = uuid::Uuid::new_v4().to_string();
    let spawn = |home: &Path, resume: bool| {
        Command::new(FAKE_CLAUDE)
            .args(CLAUDE_BASE_ARGS)
            .args([
                "--permission-mode",
                "bypassPermissions",
                "--permission-prompts",
                "host",
                "--permission-prompt-tool",
                "stdio",
                if resume { "--resume" } else { "--session-id" },
                &native_session,
            ])
            .current_dir(&cwd)
            .env_remove("G5_FAKE_SCENARIO")
            .env("HOME", home)
            .env("CLAUDE_CONFIG_DIR", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let mut child = spawn(&first_home, false);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    writeln!(stdin, "{}", json!({"type":"control_request","request_id":"initialize",
        "request":{"subtype":"initialize","hooks":{"PreToolUse":[{"matcher":"AskUserQuestion","hookCallbackIds":["probe-hook"]}]}}})).unwrap();
    stdin
        .write_all(user_message(&format!("G5_NATIVE_QUESTION_E2E:{token}")).as_bytes())
        .unwrap();
    let callback = next_control(&mut stdout);
    assert_eq!(callback["request"]["subtype"], "hook_callback");
    let tool_id = callback["request"]["tool_use_id"].clone();
    writeln!(stdin, "{}", json!({"type":"control_response","response":{"subtype":"success",
        "request_id":callback["request_id"],"response":{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"defer"}}}})).unwrap();
    drop(stdin);
    let frames: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert!(frames
        .iter()
        .any(|frame| frame["stop_reason"] == "tool_deferred"
            && frame["deferred_tool_use"]["id"] == tool_id));
    assert!(child.wait_with_output().unwrap().status.success());
    let original = fake_provider_cli::claude::transcript_path(&first_home, &cwd, &native_session);
    let restored =
        fake_provider_cli::claude::transcript_path(&restored_home, &cwd, &native_session);
    fs::create_dir_all(restored.parent().unwrap()).unwrap();
    fs::copy(original, &restored).unwrap();
    let mut child = spawn(&restored_home, true);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    // No user frame: restoration itself reopens the pending native call.
    let callback = next_control(&mut stdout);
    assert_eq!(callback["request"]["tool_use_id"], tool_id);
    let mut input = callback["request"]["input"].clone();
    let question = input["questions"][0]["question"]
        .as_str()
        .unwrap()
        .to_owned();
    input["answers"] = json!({question:"Blue"});
    writeln!(
        stdin,
        "{}",
        json!({"type":"control_response","response":{"subtype":"success",
        "request_id":callback["request_id"],"response":{"behavior":"allow","updatedInput":input}}})
    )
    .unwrap();
    drop(stdin);
    let frames: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert!(frames.iter().any(|frame| frame["result"]
        .as_str()
        .is_some_and(|text| text.contains("answered Blue"))));
    assert!(child.wait_with_output().unwrap().status.success());
    assert!(!fs::read_to_string(restored)
        .unwrap()
        .contains("Continue the same task"));
}

#[test]
fn claude_native_probes_wait_for_the_matching_host_response() {
    for (kind, response, expected) in [
        (
            "QUESTION",
            json!({"behavior":"allow","updatedInput":{"answers":{"Which color for 00000000-0000-4000-8000-000000000001?":"Blue"}}}),
            "Native question answered Blue",
        ),
        (
            "PERMISSION",
            json!({"behavior":"allow"}),
            "Native permission allow",
        ),
        (
            "PERMISSION",
            json!({"behavior":"deny","message":"No"}),
            "Native permission deny",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let token = "00000000-0000-4000-8000-000000000001";
        let mut child = Command::new(FAKE_CLAUDE)
            .args(CLAUDE_BASE_ARGS)
            .args([
                "--permission-mode",
                "bypassPermissions",
                "--permission-prompts",
                "host",
                "--permission-prompt-tool",
                "stdio",
            ])
            .current_dir(dir.path())
            .env_remove("G5_FAKE_SCENARIO")
            .env("HOME", dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let forbidden = dir.path().join("must-not-write-before-permission.txt");
        let side_effect_probe = if kind == "PERMISSION" && response["behavior"] == "deny" {
            format!(
                " G5_FS_WRITE_SET_E2E:{}",
                STANDARD.encode(
                    json!([{"path":forbidden,"contentBase64":STANDARD.encode("forbidden\n")}])
                        .to_string()
                )
            )
        } else {
            String::new()
        };
        stdin
            .write_all(
                user_message(&format!("G5_NATIVE_{kind}_E2E:{token}{side_effect_probe}"))
                    .as_bytes(),
            )
            .unwrap();
        let mut frames = Vec::new();
        let request_id = loop {
            let mut line = String::new();
            assert!(
                stdout.read_line(&mut line).unwrap() > 0,
                "no native control request"
            );
            let frame: Value = serde_json::from_str(&line).unwrap();
            let request_id = (frame["type"] == "control_request")
                .then(|| frame["request_id"].as_str().map(str::to_owned))
                .flatten();
            frames.push(frame);
            if let Some(request_id) = request_id {
                break request_id;
            }
        };
        assert_eq!(frames[0]["type"], "system");
        assert_eq!(frames.last().unwrap()["type"], "control_request");
        assert!(!forbidden.exists(), "workflow ran before the host answered");
        stdin
            .write_all(
                format!(
                    "{}\n",
                    json!({"type":"control_response","response":{"subtype":"success","request_id":request_id,"response":response}})
                )
                .as_bytes(),
            )
            .unwrap();
        drop(stdin);
        for line in stdout.lines() {
            frames.push(serde_json::from_str(&line.unwrap()).unwrap());
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(frames.iter().any(|frame| frame["type"] == "result"
            && frame["result"]
                .as_str()
                .is_some_and(|text| text.contains(expected))));
        assert_eq!(
            dir.path()
                .join(format!("e2e-native-permission-{token}.txt"))
                .exists(),
            kind == "PERMISSION" && response["behavior"] == "allow"
        );
        assert!(!forbidden.exists(), "denied permission still wrote a file");
    }

    let dir = tempfile::tempdir().unwrap();
    let exit = run(
        FAKE_CLAUDE,
        dir.path(),
        &[
            CLAUDE_BASE_ARGS,
            &[
                "--permission-mode",
                "bypassPermissions",
                "--permission-prompts",
                "host",
                "--permission-prompt-tool",
                "stdio",
            ],
        ]
        .concat(),
        &[],
        Some(&user_message(
            "G5_NATIVE_EXIT_E2E:00000000-0000-4000-8000-000000000001",
        )),
    );
    assert_eq!(exit.code(), 1);
    assert!(exit
        .frames()
        .iter()
        .any(|frame| frame["type"] == "control_request"));
}

#[test]
fn fake_claude_reports_the_pin_and_the_runner_flag_surface() {
    let dir = tempfile::tempdir().unwrap();
    let version = run(FAKE_CLAUDE, dir.path(), &["--version"], &[], None);
    assert_eq!(
        version.stdout.trim(),
        format!("{} (Claude Code)", claude_pin())
    );
    assert_eq!(version.code(), 0);

    let help = run(FAKE_CLAUDE, dir.path(), &["--help"], &[], None);
    for flag in [
        "--print",
        "--output-format",
        "--input-format",
        "--include-partial-messages",
        "--resume",
        "--session-id",
        "--permission-mode",
        "--permission-prompts",
        "--permission-prompt-tool",
        "--mcp-config",
        "--strict-mcp-config",
        "--settings",
        "--add-dir",
        "--setting-sources",
        "--disallowedTools",
        "--append-system-prompt",
        "--max-turns",
        "\"stream-json\"",
        "\"host\"",
    ] {
        assert!(help.stdout.contains(flag), "--help lacks {flag}");
    }

    // The hidden --max-turns option is validated before --version, as on 2.1.270.
    let probe = run(
        FAKE_CLAUDE,
        dir.path(),
        &["--max-turns", "abc", "--version"],
        &[],
        None,
    );
    assert_eq!(probe.code(), 1);
    assert!(probe.stderr.contains("--max-turns"), "{}", probe.stderr);

    let unknown = run(FAKE_CLAUDE, dir.path(), &["--bogus", "-p"], &[], None);
    assert_eq!(unknown.code(), 1);
    assert!(unknown.stderr.contains("unknown option '--bogus'"));

    // The repository's shared checker accepts the fake exactly like the real CLI.
    let checker = repo_root().join("scripts/check-claude-cli-flags.sh");
    let status = Command::new("sh")
        .arg(&checker)
        .arg(FAKE_CLAUDE)
        .arg(claude_pin())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(
        status.success(),
        "check-claude-cli-flags.sh rejected fake-claude"
    );
}

#[test]
fn fake_claude_replays_a_scenario_with_the_given_session_id() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("text.jsonl");
    write(&scenario, TEXT_SCENARIO);
    let receipt = dir.path().join("receipt.json");
    let session = "3d3f9a5c-1f0a-4bd7-9c3e-8c0a7a1f2b3c";
    let run = run(
        FAKE_CLAUDE,
        dir.path(),
        &[
            CLAUDE_BASE_ARGS,
            &[
                "--session-id",
                session,
                "--permission-mode",
                "bypassPermissions",
                "--model",
                "claude-test",
                "--max-turns",
                "3",
                "--append-system-prompt",
                "boundary",
                "--disallowedTools",
                "AskUserQuestion",
            ],
        ]
        .concat(),
        &[
            ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
            ("G5_FAKE_RECEIPT", receipt.to_str().unwrap()),
        ],
        Some(&user_message("hello runner")),
    );
    assert_eq!(run.code(), 0, "stderr: {}", run.stderr);
    let frames = run.frames();
    assert_eq!(frames.len(), 4);
    assert_eq!(frames[0]["type"], "system");
    assert_eq!(frames[0]["session_id"], session);
    assert_eq!(
        frames[0]["cwd"],
        dir.path().canonicalize().unwrap().display().to_string()
    );
    assert_eq!(frames[0]["model"], "claude-test");
    assert_eq!(frames[1]["event"]["delta"]["text"], "echo: hello runner");
    assert_eq!(frames[3]["type"], "result");
    assert_eq!(frames[3]["result"], "echo: hello runner");
    assert!(uuid::Uuid::parse_str(frames[2]["message"]["id"].as_str().unwrap()).is_ok());
    assert!(
        !run.stdout.contains("{{"),
        "unsubstituted placeholder in {}",
        run.stdout
    );

    let receipt: Value = serde_json::from_str(&fs::read_to_string(&receipt).unwrap()).unwrap();
    assert_eq!(receipt["provider"], "claude");
    assert_eq!(receipt["version"], claude_pin());
    assert_eq!(receipt["prompt"], "hello runner");
    assert_eq!(receipt["session_id"], session);
    assert_eq!(receipt["resume"], Value::Null);
    assert_eq!(receipt["options"]["--max-turns"][0], "3");
    assert_eq!(receipt["options"]["--append-system-prompt"][0], "boundary");
    assert_eq!(
        receipt["options"]["--disallowedTools"][0],
        "AskUserQuestion"
    );
    assert_eq!(receipt["options"]["--include-partial-messages"], true);
    assert_eq!(receipt["stdin_messages"].as_array().unwrap().len(), 1);
    assert_eq!(receipt["frames_emitted"], 4);
    assert_eq!(receipt["exit_code"], 0);
}

#[test]
fn fake_claude_generates_or_resumes_session_ids() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("text.jsonl");
    write(&scenario, TEXT_SCENARIO);
    let env = [("G5_FAKE_SCENARIO", scenario.to_str().unwrap())];
    let fresh = run(
        FAKE_CLAUDE,
        dir.path(),
        CLAUDE_BASE_ARGS,
        &env,
        Some(&user_message("x")),
    );
    let id = fresh.frames()[0]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(uuid::Uuid::parse_str(&id).is_ok());

    let resumed = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, &["--resume", "resumed-id"]].concat(),
        &env,
        Some(&user_message("x")),
    );
    assert_eq!(resumed.frames()[0]["session_id"], "resumed-id");

    let bad = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, &["--session-id", "not-a-uuid"]].concat(),
        &env,
        Some(&user_message("x")),
    );
    assert_eq!(bad.code(), 1);
    assert!(bad.stderr.contains("valid UUID"));
}

#[test]
fn fake_claude_validates_print_mode_like_the_real_cli() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("text.jsonl");
    write(&scenario, TEXT_SCENARIO);
    let env = [("G5_FAKE_SCENARIO", scenario.to_str().unwrap())];
    let no_verbose = run(
        FAKE_CLAUDE,
        dir.path(),
        &["-p", "--output-format", "stream-json"],
        &env,
        Some("hi"),
    );
    assert_eq!(no_verbose.code(), 1);
    assert!(no_verbose.stderr.contains("requires --verbose"));

    let bad_mode = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, &["--permission-mode", "yolo"]].concat(),
        &env,
        Some("hi"),
    );
    assert_eq!(bad_mode.code(), 1);
    assert!(bad_mode.stderr.contains("--permission-mode"));

    // Without a scenario file the built-in turn answers (plan P3.2).
    let no_scenario = run(
        FAKE_CLAUDE,
        dir.path(),
        CLAUDE_BASE_ARGS,
        &[],
        Some(&user_message("hi")),
    );
    assert_eq!(no_scenario.code(), 0, "{}", no_scenario.stderr);
    assert!(no_scenario.frames().last().unwrap()["result"]
        .as_str()
        .unwrap()
        .starts_with("Claude Code fake response:"));

    // Text input mode: the prompt is stdin until EOF.
    let text_mode = run(
        FAKE_CLAUDE,
        dir.path(),
        &["-p", "--output-format", "stream-json", "--verbose"],
        &env,
        Some("plain prompt\n"),
    );
    assert_eq!(text_mode.code(), 0, "{}", text_mode.stderr);
    assert_eq!(text_mode.frames()[3]["result"], "echo: plain prompt");

    // Stream input that closes before any user message is an error.
    let empty = run(FAKE_CLAUDE, dir.path(), CLAUDE_BASE_ARGS, &env, Some(""));
    assert_eq!(empty.code(), 1);
    assert!(empty.stderr.contains("before a user message"));
}

#[test]
fn fake_claude_writes_files_and_honours_exit_steps() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("exit.jsonl");
    write(
        &scenario,
        r#"{"type":"system","subtype":"init","session_id":"{{session_id}}","uuid":"{{uuid}}"}
{"write_file":{"path":"nested/out.txt","content":"prompt was {{prompt}}\n"}}
{"write_file":{"path":"nested/out.txt","content":"more\n","append":true}}
{"stderr":"fake diagnostic"}
{"exit":{"code":7,"stderr":"provider crashed"}}
{"type":"result","subtype":"success","is_error":false,"result":"unreachable","session_id":"{{session_id}}"}
"#,
    );
    let receipt = dir.path().join("receipt.json");
    let run = run(
        FAKE_CLAUDE,
        dir.path(),
        CLAUDE_BASE_ARGS,
        &[
            ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
            ("G5_FAKE_RECEIPT", receipt.to_str().unwrap()),
        ],
        Some(&user_message("go")),
    );
    assert_eq!(run.code(), 7);
    assert_eq!(
        run.frames().len(),
        1,
        "frames after exit must not be emitted"
    );
    assert!(run.stderr.contains("fake diagnostic"));
    assert!(run.stderr.contains("provider crashed"));
    assert_eq!(
        fs::read_to_string(dir.path().join("nested/out.txt")).unwrap(),
        "prompt was go\nmore\n"
    );
    let receipt: Value = serde_json::from_str(&fs::read_to_string(&receipt).unwrap()).unwrap();
    assert_eq!(receipt["exit_code"], 7);
    assert_eq!(
        receipt["written_files"],
        json!(["nested/out.txt", "nested/out.txt"])
    );
}

#[test]
fn fake_claude_mcp_timeout_becomes_an_error_tool_result() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("slow.jsonl");
    write(
        &scenario,
        r#"{"mcp_call":{"tool":"echo","arguments":{"text":"slow"},"timeout_ms":200}}
{"type":"result","subtype":"success","is_error":false,"result":"after","session_id":"{{session_id}}"}
"#,
    );
    let mcp = claude_mcp_config(dir.path());
    let timed_out = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, &["--mcp-config", mcp.to_str().unwrap()]].concat(),
        &[
            ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
            ("G5_FAKE_MCP_DELAY_MS", "3000"),
        ],
        Some(&user_message("go")),
    );
    assert_eq!(timed_out.code(), 0, "{}", timed_out.stderr);
    let frames = timed_out.frames();
    let result = &frames[1]["message"]["content"][0];
    assert_eq!(result["is_error"], true);
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("timed out after 200 ms"));
    assert_eq!(frames[2]["result"], "after");

    // An unconfigured server is a scenario error, reported with the line number.
    let missing = run(
        FAKE_CLAUDE,
        dir.path(),
        CLAUDE_BASE_ARGS,
        &[("G5_FAKE_SCENARIO", scenario.to_str().unwrap())],
        Some(&user_message("go")),
    );
    assert_eq!(missing.code(), 1);
    assert!(
        missing.stderr.contains("slow.jsonl:1"),
        "{}",
        missing.stderr
    );
    assert!(missing.stderr.contains("not configured"));
}

#[test]
fn fake_claude_persists_a_transcript_and_can_insist_on_it_for_resume() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("text.jsonl");
    write(&scenario, TEXT_SCENARIO);
    let config_dir = dir.path().join("claude-config");
    let cwd = dir.path().join("work");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let session = "6a0d2c1e-4b5f-4c6d-8e9f-0a1b2c3d4e5f";
    let env = [
        ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
        ("CLAUDE_CONFIG_DIR", config_dir.to_str().unwrap()),
        ("G5_FAKE_STRICT_RESUME", "1"),
    ];
    let missing = run(
        FAKE_CLAUDE,
        &cwd,
        &[CLAUDE_BASE_ARGS, &["--resume", session]].concat(),
        &env,
        Some(&user_message("x")),
    );
    assert_eq!(missing.code(), 1);
    assert!(missing.stderr.contains("No conversation found"));

    let first = run(
        FAKE_CLAUDE,
        &cwd,
        &[CLAUDE_BASE_ARGS, &["--session-id", session]].concat(),
        &env,
        Some(&user_message("first")),
    );
    assert_eq!(first.code(), 0, "{}", first.stderr);
    let transcript = fake_provider_cli::claude::transcript_path(&config_dir, &cwd, session);
    assert!(transcript.is_file(), "{}", transcript.display());
    let lines: Vec<Value> = fs::read_to_string(&transcript)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["type"], "user");
    assert_eq!(lines[1]["type"], "assistant");

    let resumed = run(
        FAKE_CLAUDE,
        &cwd,
        &[CLAUDE_BASE_ARGS, &["--resume", session]].concat(),
        &env,
        Some(&user_message("second")),
    );
    assert_eq!(resumed.code(), 0, "{}", resumed.stderr);
    assert_eq!(resumed.frames()[0]["session_id"], session);
    assert_eq!(fs::read_to_string(&transcript).unwrap().lines().count(), 4);
}

#[test]
fn fake_resume_refusal_after_input_has_no_provider_frame_for_either_cli() {
    let dir = tempfile::tempdir().unwrap();
    let session = "6a0d2c1e-4b5f-4c6d-8e9f-0a1b2c3d4e5f";
    let claude_home = dir.path().join("claude-home");
    let codex_home = codex_home_with_stub(dir.path(), "");
    let claude_env = [("CLAUDE_CONFIG_DIR", claude_home.to_str().unwrap())];
    let first_claude = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, ["--session-id", session].as_slice()].concat(),
        &claude_env,
        Some(&user_message("first")),
    );
    assert_eq!(first_claude.code(), 0, "{}", first_claude.stderr);
    let codex_env = [("CODEX_HOME", codex_home.to_str().unwrap())];
    let first_codex = run_codex(
        dir.path(),
        &codex_env,
        CodexTurn {
            resume: Some(session),
            ..codex_turn("first")
        },
    );
    assert_eq!(first_codex.code(), 0, "{}", first_codex.stderr());
    let codex_rollout =
        fake_provider_cli::session_files::find_codex_rollout(&codex_home, session).unwrap();
    let codex_saved = fs::read(&codex_rollout).unwrap();

    for use_directive in [false, true] {
        let prompt_text = if use_directive {
            "refused input G5_TOOL_ONLY_FACT_E2E:refuse-resume"
        } else {
            "refused input"
        };
        let mut refusal_env = vec![("G5_FAKE_STRICT_RESUME", "1")];
        if !use_directive {
            refusal_env.push(("G5_FAKE_REFUSE_RESUME", "1"));
        }

        let receipt_path = dir
            .path()
            .join(format!("claude-{use_directive}-refusal.json"));
        let mut env = refusal_env.clone();
        env.push(("CLAUDE_CONFIG_DIR", claude_home.to_str().unwrap()));
        env.push(("G5_FAKE_RECEIPT", receipt_path.to_str().unwrap()));
        let refusal = run(
            FAKE_CLAUDE,
            dir.path(),
            &[CLAUDE_BASE_ARGS, &["--resume", session]].concat(),
            &env,
            Some(&user_message(prompt_text)),
        );
        assert_eq!(refusal.code(), 1, "claude: {}", refusal.stderr);
        assert!(refusal.frames().is_empty(), "claude: {}", refusal.stdout);
        let receipt: Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        assert_eq!(receipt["resume"], session);
        assert_eq!(receipt["prompt"], prompt_text);
        assert_eq!(receipt["exit_code"], 1);
        assert_eq!(receipt["frames_emitted"], 0);
        assert!(receipt["stdin"].as_str().unwrap().contains("refused input"));

        // The App Server binds the thread, then refuses the resumed turn's
        // input before accepting it: no turn result, no notification and no
        // change to the saved rollout.
        let mut env = refusal_env;
        env.push(("CODEX_HOME", codex_home.to_str().unwrap()));
        let refusal = run_codex(
            dir.path(),
            &env,
            CodexTurn {
                resume: Some(session),
                ..codex_turn(prompt_text)
            },
        );
        assert_eq!(refusal.code(), 2, "codex: {}", refusal.stderr());
        assert_eq!(refusal.thread_id(), session);
        let turn = refusal.response("turn").expect("turn/start response");
        assert_eq!(turn["error"]["code"], -32600, "codex: {turn}");
        assert!(turn.get("result").is_none(), "codex: {turn}");
        assert!(
            refusal
                .frames
                .iter()
                .all(|(_, frame)| frame.get("method").is_none()),
            "codex: {}",
            refusal.run.stdout
        );
        assert_eq!(
            fs::read(&codex_rollout).unwrap(),
            codex_saved,
            "codex: refused input changed the rollout"
        );
    }
}

#[test]
fn fake_claude_recalls_only_a_restored_tool_result() {
    use fake_provider_cli::resume_fact::{
        claude_fact, FACT_PREFIX, RECALL, RECORD, SKIP_PERSISTENCE,
    };

    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().canonicalize().unwrap();
    let original = dir.path().join("claude-original");
    let replacement = dir.path().join("claude-replacement");
    let session = "6a0d2c1e-4b5f-4c6d-8e9f-0a1b2c3d4e5f";
    let original_env = [
        ("CLAUDE_CONFIG_DIR", original.to_str().unwrap()),
        ("G5_FAKE_STRICT_RESUME", "1"),
    ];
    let first = run(
        FAKE_CLAUDE,
        &cwd,
        &[CLAUDE_BASE_ARGS, &["--session-id", session]].concat(),
        &original_env,
        Some(&user_message(RECORD)),
    );
    assert_eq!(first.code(), 0, "{}", first.stderr);
    let fact = claude_fact(&original, &cwd, session).expect("tool-only fact in transcript");
    assert!(fact.starts_with(FACT_PREFIX));
    assert!(!first.frames().iter().any(|frame| frame["type"] == "result"
        && frame["result"]
            .as_str()
            .is_some_and(|text| text.contains(&fact))));

    let replacement_env = [
        ("CLAUDE_CONFIG_DIR", replacement.to_str().unwrap()),
        ("G5_FAKE_STRICT_RESUME", "1"),
    ];
    let missing = run(
        FAKE_CLAUDE,
        &cwd,
        &[CLAUDE_BASE_ARGS, &["--resume", session]].concat(),
        &replacement_env,
        Some(&user_message(RECALL)),
    );
    assert_eq!(missing.code(), 1);
    let source = fake_provider_cli::claude::transcript_path(&original, &cwd, session);
    let target = fake_provider_cli::claude::transcript_path(&replacement, &cwd, session);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(source, target).unwrap();
    assert!(
        fake_provider_cli::resume_fact::claude_fact(&replacement, &cwd, session).is_none(),
        "the transcript reference alone must not contain the tool-only fact"
    );
    let source_result = original
        .join("projects")
        .join(fake_provider_cli::session_files::claude_project_key(&cwd))
        .join(session)
        .join("tool-results/fake-tool-result.txt");
    let target_result = replacement
        .join("projects")
        .join(fake_provider_cli::session_files::claude_project_key(&cwd))
        .join(session)
        .join("tool-results/fake-tool-result.txt");
    fs::create_dir_all(target_result.parent().unwrap()).unwrap();
    fs::copy(source_result, target_result).unwrap();
    let resumed = run(
        FAKE_CLAUDE,
        &cwd,
        &[CLAUDE_BASE_ARGS, &["--resume", session]].concat(),
        &replacement_env,
        Some(&user_message(RECALL)),
    );
    assert_eq!(resumed.code(), 0, "{}", resumed.stderr);
    assert!(resumed
        .frames()
        .iter()
        .any(|frame| frame["type"] == "result"
            && frame["result"]
                .as_str()
                .is_some_and(|text| text.contains(&fact))));

    let skipped = run(
        FAKE_CLAUDE,
        &cwd,
        &[
            CLAUDE_BASE_ARGS,
            &["--session-id", "4a0d2c1e-4b5f-4c6d-8e9f-0a1b2c3d4e5f"],
        ]
        .concat(),
        &replacement_env,
        Some(&user_message(&format!("{RECORD} {SKIP_PERSISTENCE}"))),
    );
    assert_eq!(skipped.code(), 0, "{}", skipped.stderr);
    assert!(claude_fact(&replacement, &cwd, "4a0d2c1e-4b5f-4c6d-8e9f-0a1b2c3d4e5f").is_none());
}

#[test]
fn fake_claude_stays_alive_until_stdin_closes_in_stream_json_mode() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("text.jsonl");
    write(&scenario, TEXT_SCENARIO);
    let mut child = Command::new(FAKE_CLAUDE)
        .args(CLAUDE_BASE_ARGS)
        .current_dir(dir.path())
        .env("G5_FAKE_SCENARIO", &scenario)
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(user_message("hold").as_bytes()).unwrap();
    stdin.flush().unwrap();
    // Read the four frames while stdin stays open.
    let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut seen = 0;
    while seen < 4 {
        let mut line = String::new();
        use std::io::BufRead as _;
        assert!(
            stdout.read_line(&mut line).unwrap() > 0,
            "stdout closed early"
        );
        if !line.trim().is_empty() {
            seen += 1;
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        child.try_wait().unwrap().is_none(),
        "fake-claude exited before stdin was closed"
    );
    drop(stdin);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
}

const CODEX_SCENARIO: &str = r##"{"type":"thread.started","thread_id":"{{session_id}}"}
{"type":"turn.started"}
{"mcp_call":{"server":"g5-tools","tool":"echo","arguments":{"text":"yes"}}}
{"write_file":{"path":"out/notes.md","content":"# {{prompt}}\n"}}
{"delay_ms":5,"type":"item.completed","item":{"id":"item_7","type":"agent_message","text":"All done for {{prompt}}"}}
{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}
"##;

/// One G5-shaped App Server turn: `codex app-server --listen stdio://` plus
/// `launch` flags, then `thread/start` (or `thread/resume` of `resume`) and a
/// single `turn/start`. Model, effort, attachments and resume travel in
/// JSON-RPC params, never argv, exactly as G5 launches Codex.
#[derive(Clone, Copy, Default)]
struct CodexTurn<'a> {
    prompt: &'a str,
    launch: &'a [&'a str],
    model: Option<&'a str>,
    resume: Option<&'a str>,
    ephemeral: bool,
    images: &'a [&'a str],
    effort: Option<&'a str>,
}

fn codex_turn(prompt: &str) -> CodexTurn<'_> {
    CodexTurn {
        prompt,
        ..CodexTurn::default()
    }
}

/// Bound on any single wait for the App Server peer: one frame, or its exit.
const CODEX_PEER_DEADLINE: Duration = Duration::from_secs(30);

/// Outcome of [`run_codex`]: the process outcome plus every stdout frame
/// (JSON-RPC responses and notifications) with its arrival time.
struct CodexRun {
    run: Run,
    frames: Vec<(Duration, Value)>,
}

impl CodexRun {
    fn code(&self) -> i32 {
        self.run.code()
    }

    fn stderr(&self) -> &str {
        &self.run.stderr
    }

    /// The response to the client request `id` (`init`, `thread` or `turn`).
    fn response(&self, id: &str) -> Option<&Value> {
        self.frames
            .iter()
            .map(|(_, frame)| frame)
            .find(|frame| frame["id"] == id && frame.get("method").is_none())
    }

    /// The thread the `thread/start` or `thread/resume` result bound.
    fn thread_id(&self) -> String {
        self.response("thread")
            .and_then(|frame| frame["result"]["thread"]["id"].as_str())
            .unwrap_or_else(|| panic!("no thread result: {}", self.run.stdout))
            .to_owned()
    }

    /// Notifications after `turn/started`: the App Server translation of the
    /// frames the shared scenario engine emitted for this turn.
    fn timed_events(&self) -> Vec<(Duration, &Value)> {
        self.frames
            .iter()
            .skip_while(|(_, frame)| frame["method"] != "turn/started")
            .skip(1)
            .map(|(at, frame)| (*at, frame))
            .collect()
    }

    fn events(&self) -> Vec<&Value> {
        self.timed_events()
            .into_iter()
            .map(|(_, frame)| frame)
            .collect()
    }

    /// `params.item` of each `item/completed` whose App Server item type is `kind`.
    fn completed_items(&self, kind: &str) -> Vec<&Value> {
        self.events()
            .into_iter()
            .filter(|event| event["method"] == "item/completed")
            .map(|event| &event["params"]["item"])
            .filter(|item| item["type"] == kind)
            .collect()
    }

    fn agent_texts(&self) -> Vec<&str> {
        self.completed_items("agentMessage")
            .into_iter()
            .map(|item| item["text"].as_str().expect("agent message text"))
            .collect()
    }

    /// `params.turn` of the `turn/completed` notification, if the turn ended.
    fn turn_completed(&self) -> Option<&Value> {
        self.events()
            .into_iter()
            .find(|event| event["method"] == "turn/completed")
            .map(|event| &event["params"]["turn"])
    }
}

/// A spawned `fake-codex app-server` with stdout read on a separate thread so
/// every wait is bounded. Dropping it kills a peer that is still running.
struct AppServerPeer {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    stderr: Option<std::thread::JoinHandle<String>>,
    receiver: std::sync::mpsc::Receiver<(Duration, String)>,
    stdout: String,
    frames: Vec<(Duration, Value)>,
}

impl AppServerPeer {
    fn spawn(cwd: &Path, env: &[(&str, &str)], launch: &[&str]) -> Self {
        let started = std::time::Instant::now();
        let mut child = fake_command(FAKE_CODEX, cwd, env)
            .args(["app-server", "--listen", "stdio://"])
            .args(launch)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn fake-codex app-server");
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send((started.elapsed(), line)).is_err() {
                    break;
                }
            }
        });
        let stderr = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
            text
        });
        Self {
            stdin: child.stdin.take(),
            child,
            stderr: Some(stderr),
            receiver,
            stdout: String::new(),
            frames: Vec::new(),
        }
    }

    fn send(&mut self, frame: Value) {
        // A peer that refused an earlier frame may already have exited.
        if let Some(stdin) = &mut self.stdin {
            if let Err(err) = writeln!(stdin, "{frame}").and_then(|()| stdin.flush()) {
                assert_eq!(
                    err.kind(),
                    std::io::ErrorKind::BrokenPipe,
                    "App Server stdin write failed: {err}"
                );
                self.stdin = None;
            }
        }
    }

    /// Collect frames until one satisfies `done` (returned) or stdout closes.
    fn until(&mut self, done: impl Fn(&Value) -> bool) -> Option<Value> {
        loop {
            match self.receiver.recv_timeout(CODEX_PEER_DEADLINE) {
                Ok((at, line)) => {
                    self.stdout.push_str(&line);
                    self.stdout.push('\n');
                    if line.trim().is_empty() {
                        continue;
                    }
                    let frame: Value = serde_json::from_str(&line)
                        .unwrap_or_else(|err| panic!("stdout line is not JSON ({err}): {line}"));
                    self.frames.push((at, frame.clone()));
                    if done(&frame) {
                        return Some(frame);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
                    "fake-codex App Server was silent for {CODEX_PEER_DEADLINE:?}; stdout so far:\n{}",
                    self.stdout
                ),
            }
        }
    }

    /// Close stdin like G5 after `turn/completed`, drain stdout and await exit.
    fn finish(mut self) -> CodexRun {
        self.stdin = None;
        self.until(|_| false);
        let deadline = std::time::Instant::now() + CODEX_PEER_DEADLINE;
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "fake-codex App Server ignored stdin EOF; stdout:\n{}",
                self.stdout
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let stderr = self.stderr.take().unwrap().join().unwrap();
        CodexRun {
            run: Run {
                status,
                stdout: std::mem::take(&mut self.stdout),
                stderr,
            },
            frames: std::mem::take(&mut self.frames),
        }
    }
}

impl Drop for AppServerPeer {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Run one App Server turn the way G5's Codex runner does: initialize, bind
/// the thread, start the turn, read until `turn/completed`, an error response
/// or exit, then close stdin and await the exit.
fn run_codex(cwd: &Path, env: &[(&str, &str)], turn: CodexTurn) -> CodexRun {
    let mut peer = AppServerPeer::spawn(cwd, env, turn.launch);
    drive_codex_turn(&mut peer, cwd, &turn);
    peer.finish()
}

/// The handshake; `None` once the peer refused a request or exited.
fn drive_codex_turn(peer: &mut AppServerPeer, cwd: &Path, turn: &CodexTurn) -> Option<()> {
    peer.send(json!({"id":"init","method":"initialize","params":{"clientInfo":{"name":"g5_agent","version":"1"},"capabilities":{"experimentalApi":false}}}));
    peer.until(|frame| frame["id"] == "init")?.get("result")?;
    peer.send(json!({"method":"initialized"}));
    let mut params =
        json!({"model":turn.model,"cwd":cwd,"approvalPolicy":"never","sandbox":"workspace-write"});
    if turn.ephemeral {
        params["ephemeral"] = json!(true);
    }
    let method = match turn.resume {
        Some(thread) => {
            params["threadId"] = json!(thread);
            "thread/resume"
        }
        None => "thread/start",
    };
    peer.send(json!({"id":"thread","method":method,"params":params}));
    let thread = peer.until(|frame| frame["id"] == "thread")?["result"]["thread"]["id"]
        .as_str()?
        .to_owned();
    let mut input = vec![json!({"type":"text","text":turn.prompt})];
    input.extend(
        turn.images
            .iter()
            .map(|path| json!({"type":"localImage","path":path})),
    );
    let mut params = json!({"threadId":thread,"input":input});
    if let Some(effort) = turn.effort {
        params["effort"] = json!(effort);
    }
    if let Some(model) = turn.model {
        params["model"] = json!(model);
    }
    peer.send(json!({"id":"turn","method":"turn/start","params":params}));
    peer.until(|frame| {
        frame["method"] == "turn/completed"
            || (frame["id"] == "turn" && frame.get("error").is_some())
    })?;
    Some(())
}

/// No frame but the thread result (which echoes `approvalPolicy`) mentions an
/// approval: the fake never asks G5 to approve anything.
fn assert_no_approval_frames(outcome: &CodexRun) {
    for (_, frame) in &outcome.frames {
        if frame["id"] != "thread" {
            assert!(
                !frame.to_string().to_ascii_lowercase().contains("approval"),
                "an approval frame reached stdout: {frame}"
            );
        }
    }
}

const CODEX_EXEC_REFUSAL: &str =
    "fake-codex: only `codex app-server --listen stdio://` is implemented";

#[test]
fn fake_codex_reports_the_pin_and_serves_only_the_app_server() {
    let dir = tempfile::tempdir().unwrap();
    let version = run(FAKE_CODEX, dir.path(), &["--version"], &[], None);
    assert_eq!(version.stdout.trim(), format!("codex-cli {}", codex_pin()));
    let help = run(FAKE_CODEX, dir.path(), &["app-server", "--help"], &[], None);
    assert_eq!(help.code(), 0, "{}", help.stderr);
    assert!(help.stdout.contains("--listen stdio://"), "{}", help.stdout);
    for (args, message) in [
        (
            &["app-server", "--listen", "stdio://", "--bogus"][..],
            "unsupported launch argument",
        ),
        (
            &["app-server", "--listen", "stdio://", "--model", "gpt-test"][..],
            "unsupported launch argument",
        ),
        (
            &["app-server", "--listen", "ws://127.0.0.1:1"][..],
            "only local stdio is supported",
        ),
    ] {
        let refused = run(FAKE_CODEX, dir.path(), args, &[], None);
        assert_eq!(refused.code(), 2, "{args:?}: {}", refused.stderr);
        assert!(
            refused.stderr.contains(message),
            "{args:?}: {}",
            refused.stderr
        );
        assert!(refused.stdout.is_empty(), "{args:?}: {}", refused.stdout);
    }
    for args in [
        &["exec", "--json"][..],
        &["exec", "--json", "--skip-git-repo-check", "hi"][..],
        &[
            "exec",
            "resume",
            "019c0d2b-0000-7000-8000-00000000abcd",
            "--json",
        ][..],
    ] {
        let refused = run(FAKE_CODEX, dir.path(), args, &[], Some("x\n"));
        assert_eq!(refused.code(), 2, "{args:?}: {}", refused.stderr);
        assert!(
            refused.stderr.contains(CODEX_EXEC_REFUSAL),
            "{args:?}: {}",
            refused.stderr
        );
        assert!(refused.stdout.is_empty(), "{args:?}: {}", refused.stdout);
    }
}

#[test]
fn fake_codex_replays_a_scenario_with_mcp_and_session_files() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("codex.jsonl");
    write(&scenario, CODEX_SCENARIO);
    let home = codex_home_with_stub(dir.path(), "tool_timeout_sec = 5\n");
    let receipt = dir.path().join("receipt.json");
    let env = [
        ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
        ("G5_FAKE_RECEIPT", receipt.to_str().unwrap()),
        ("CODEX_HOME", home.to_str().unwrap()),
    ];
    let outcome = run_codex(
        dir.path(),
        &env,
        CodexTurn {
            launch: &[
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "sandbox_mode=\"workspace-write\"",
            ],
            model: Some("gpt-test"),
            images: &["shot.png"],
            effort: Some("high"),
            ..codex_turn("write notes")
        },
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    let thread_id = outcome.thread_id();
    assert!(uuid::Uuid::parse_str(&thread_id).is_ok());
    assert_eq!(
        outcome.response("thread").unwrap()["result"]["model"],
        "gpt-test"
    );
    let turn_id = outcome.response("turn").unwrap()["result"]["turn"]["id"].clone();
    // `thread.started`/`turn.started` have no App Server notification, and
    // `write_file` emits no frame.
    let events = outcome.events();
    assert_eq!(
        events
            .iter()
            .map(|event| event["method"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "item/started",
            "item/completed",
            "item/completed",
            "thread/tokenUsage/updated",
            "turn/completed"
        ],
        "{}",
        outcome.run.stdout
    );
    for event in &events {
        assert_eq!(event["params"]["threadId"], thread_id);
        assert_eq!(event["params"]["turnId"], turn_id);
    }
    let call = &events[0]["params"]["item"];
    assert_eq!(call["type"], "mcpToolCall");
    assert_eq!(call["server"], "g5-tools");
    assert_eq!(call["tool"], "echo");
    assert_eq!(call["status"], "in_progress");
    let result = &events[1]["params"]["item"];
    assert_eq!(result["type"], "mcpToolCall");
    assert_eq!(result["status"], "completed");
    assert_eq!(result["error"], Value::Null);
    assert_eq!(result["result"]["content"][0]["text"], "yes");
    assert_eq!(outcome.agent_texts(), ["All done for write notes"]);
    assert_eq!(events[3]["params"]["tokenUsage"]["last"]["totalTokens"], 2);
    assert_eq!(outcome.turn_completed().unwrap()["status"], "completed");
    assert_no_approval_frames(&outcome);

    assert_eq!(
        fs::read_to_string(dir.path().join("out/notes.md")).unwrap(),
        "# write notes\n"
    );
    let rollout = fake_provider_cli::session_files::find_codex_rollout(&home, &thread_id)
        .expect("rollout file written under CODEX_HOME/sessions");
    let first: Value = serde_json::from_str(
        fs::read_to_string(&rollout)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["type"], "session_meta");
    assert_eq!(first["payload"]["id"], thread_id);

    let recorded: Value = serde_json::from_str(&fs::read_to_string(&receipt).unwrap()).unwrap();
    assert_eq!(recorded["provider"], "codex");
    assert_eq!(recorded["prompt"], "write notes");
    assert_eq!(recorded["session_id"], thread_id);
    assert_eq!(recorded["resume"], Value::Null);
    assert_eq!(recorded["options"]["model"], "gpt-test");
    assert_eq!(recorded["options"]["images"], json!(["shot.png"]));
    let overrides = &recorded["options"]["config_overrides"];
    assert_eq!(overrides[0]["key"], "approval_policy");
    assert_eq!(overrides[0]["parsed"], "never");
    assert_eq!(overrides[2]["key"], "model_reasoning_effort");
    assert_eq!(overrides[2]["parsed"], "high");
    assert_eq!(recorded["stdin_messages"][3]["method"], "turn/start");
    assert_eq!(recorded["mcp_calls"][0]["tool"], "echo");

    // thread/resume binds the given thread id and appends to its rollout.
    let resumed = run_codex(
        dir.path(),
        &env,
        CodexTurn {
            resume: Some(&thread_id),
            ..codex_turn("again")
        },
    );
    assert_eq!(resumed.code(), 0, "{}", resumed.stderr());
    assert_eq!(resumed.thread_id(), thread_id);
    assert_eq!(resumed.agent_texts(), ["All done for again"]);
    let recorded: Value = serde_json::from_str(&fs::read_to_string(&receipt).unwrap()).unwrap();
    assert_eq!(recorded["resume"], thread_id);
    assert!(fs::read_to_string(&rollout).unwrap().lines().count() > 7);
}

/// Plan P3.4: under `G5_FAKE_STRICT_RESUME=1`, `thread/resume` of a thread
/// without a rollout is refused like the real CLI (JSON-RPC error, no turn,
/// exit 2), and succeeds once the rollout is back under `CODEX_HOME/sessions`.
#[test]
fn fake_codex_strict_resume_needs_the_rollout() {
    let dir = tempfile::tempdir().unwrap();
    let home = codex_home_with_stub(dir.path(), "");
    let thread_id = "019c0d2b-0000-7000-8000-00000000abcd";
    let resume = |strict: bool| {
        let mut env = vec![("CODEX_HOME", home.to_str().unwrap())];
        if strict {
            env.push(("G5_FAKE_STRICT_RESUME", "1"));
        }
        run_codex(
            dir.path(),
            &env,
            CodexTurn {
                resume: Some(thread_id),
                ..codex_turn("again")
            },
        )
    };

    let missing = resume(true);
    assert_eq!(missing.code(), 2, "stderr: {}", missing.stderr());
    let refusal = &missing.response("thread").expect("thread/resume response")["error"];
    assert_eq!(refusal["code"], -32600);
    assert!(
        refusal["message"]
            .as_str()
            .unwrap()
            .contains("no rollout found"),
        "{refusal}"
    );
    assert!(missing.response("turn").is_none(), "no turn after refusal");
    assert!(
        missing
            .frames
            .iter()
            .all(|(_, frame)| frame.get("method").is_none()),
        "no notification before the failure: {}",
        missing.run.stdout
    );

    // Without strict resume the fake starts the thread's rollout itself.
    let lenient = resume(false);
    assert_eq!(lenient.code(), 0, "stderr: {}", lenient.stderr());
    assert!(fake_provider_cli::session_files::find_codex_rollout(&home, thread_id).is_some());

    let strict = resume(true);
    assert_eq!(strict.code(), 0, "stderr: {}", strict.stderr());
    assert_eq!(strict.thread_id(), thread_id);
    assert_eq!(strict.turn_completed().unwrap()["status"], "completed");
}

#[test]
fn fake_codex_recalls_only_a_restored_tool_result() {
    use fake_provider_cli::resume_fact::{codex_fact, RECALL, RECORD};

    let dir = tempfile::tempdir().unwrap();
    let original = codex_home_with_stub(dir.path(), "");
    let replacement = dir.path().join("codex-replacement");
    fs::create_dir_all(&replacement).unwrap();
    fs::copy(
        original.join("config.toml"),
        replacement.join("config.toml"),
    )
    .unwrap();
    let first = run_codex(
        dir.path(),
        &[
            ("CODEX_HOME", original.to_str().unwrap()),
            ("G5_FAKE_STRICT_RESUME", "1"),
        ],
        codex_turn(RECORD),
    );
    assert_eq!(first.code(), 0, "{}", first.stderr());
    let session = first.thread_id();
    let fact = codex_fact(&original, &session).expect("tool-only fact in rollout");
    assert!(!first.agent_texts().iter().any(|text| text.contains(&fact)));
    let replacement_env = [
        ("CODEX_HOME", replacement.to_str().unwrap()),
        ("G5_FAKE_STRICT_RESUME", "1"),
    ];
    let recall = CodexTurn {
        resume: Some(&session),
        ..codex_turn(RECALL)
    };
    let unrestored = run_codex(dir.path(), &replacement_env, recall);
    assert_eq!(unrestored.code(), 2, "{}", unrestored.stderr());
    assert!(unrestored
        .response("thread")
        .unwrap()
        .get("error")
        .is_some());
    let source = fake_provider_cli::session_files::find_codex_rollout(&original, &session).unwrap();
    let target = replacement.join(source.strip_prefix(&original).unwrap());
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(source, target).unwrap();
    let resumed = run_codex(dir.path(), &replacement_env, recall);
    assert_eq!(resumed.code(), 0, "{}", resumed.stderr());
    assert!(resumed
        .agent_texts()
        .iter()
        .any(|text| text.contains(&fact)));
}

#[test]
fn fake_codex_takes_the_turn_text_and_rejects_permission_steps() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("codex.jsonl");
    write(
        &scenario,
        r#"{"type":"thread.started","thread_id":"{{session_id}}"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"{{prompt}}"}}
{"type":"turn.completed","usage":{}}
"#,
    );
    let home = codex_home_with_stub(dir.path(), "");
    let env = [
        ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
        ("CODEX_HOME", home.to_str().unwrap()),
    ];
    // Only text input parts form the prompt; attachments do not.
    let turn = run_codex(
        dir.path(),
        &env,
        CodexTurn {
            images: &["shot.png"],
            ..codex_turn("turn prompt")
        },
    );
    assert_eq!(turn.code(), 0, "{}", turn.stderr());
    assert_eq!(turn.agent_texts(), ["turn prompt"]);

    let perm = dir.path().join("perm.jsonl");
    write(
        &perm,
        r#"{"type":"thread.started","thread_id":"{{session_id}}"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"before"}}
{"permission":{"tool_name":"Write","input":{}}}
{"type":"turn.completed","usage":{}}
"#,
    );
    let rejected = run_codex(
        dir.path(),
        &[
            ("G5_FAKE_SCENARIO", perm.to_str().unwrap()),
            ("CODEX_HOME", home.to_str().unwrap()),
        ],
        codex_turn("x"),
    );
    assert_eq!(rejected.code(), 1);
    assert!(
        rejected.stderr().contains("never emits approval frames"),
        "{}",
        rejected.stderr()
    );
    assert_eq!(
        rejected.events().len(),
        1,
        "only the frames before the step: {}",
        rejected.run.stdout
    );
    assert_eq!(rejected.agent_texts(), ["before"]);
    assert_no_approval_frames(&rejected);
}

#[test]
fn fake_codex_exit_step_and_missing_server() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("exit.jsonl");
    write(
        &scenario,
        r#"{"type":"thread.started","thread_id":"{{session_id}}"}
{"type":"error","message":"provider unavailable"}
{"exit":{"code":1}}
"#,
    );
    let home = dir.path().join("empty-home");
    fs::create_dir_all(&home).unwrap();
    let run_exit = run_codex(
        dir.path(),
        &[
            ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
            ("CODEX_HOME", home.to_str().unwrap()),
        ],
        codex_turn("x"),
    );
    assert_eq!(run_exit.code(), 1);
    let events = run_exit.events();
    assert_eq!(events.len(), 1, "{}", run_exit.run.stdout);
    assert_eq!(events[0]["method"], "error");
    assert_eq!(
        events[0]["params"]["error"]["message"],
        "provider unavailable"
    );
    assert!(run_exit.turn_completed().is_none());

    let missing = dir.path().join("missing.jsonl");
    write(&missing, r#"{"mcp_call":{"tool":"echo"}}"#);
    let run_missing = run_codex(
        dir.path(),
        &[
            ("G5_FAKE_SCENARIO", missing.to_str().unwrap()),
            ("CODEX_HOME", home.to_str().unwrap()),
        ],
        codex_turn("x"),
    );
    assert_eq!(run_missing.code(), 1);
    assert!(
        run_missing.stderr().contains("config.toml"),
        "{}",
        run_missing.stderr()
    );
}

#[test]
fn fake_g5_tools_speaks_mcp_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    let input = format!(
        "{}\n{}\n{}\n{}\n",
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "echo", "arguments": {"text": "option B"}}}),
    );
    let run = run(FAKE_G5_TOOLS, dir.path(), &[], &[], Some(&input));
    assert_eq!(run.code(), 0, "{}", run.stderr);
    let replies = run.frames();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "g5-tools");
    assert_eq!(replies[1]["result"]["tools"][0]["name"], "echo");
    assert_eq!(replies[2]["result"]["content"][0]["text"], "option B");
}

/// Read stdout line by line with the wall-clock time each frame arrived.
fn timed_frames(child: &mut std::process::Child, expected: usize) -> Vec<(Duration, Value)> {
    use std::io::BufRead as _;
    let started = std::time::Instant::now();
    let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut frames = Vec::new();
    while frames.len() < expected {
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "stdout closed after {} frames",
            frames.len()
        );
        if line.trim().is_empty() {
            continue;
        }
        frames.push((started.elapsed(), serde_json::from_str(&line).unwrap()));
    }
    frames
}

/// The tool-call announcement must reach the consumer while the MCP server
/// still holds the answer (a delayed `echo` call): start frames
/// are written before the blocking call, result frames after it.
#[test]
fn fake_claude_announces_the_mcp_call_before_the_server_answers() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("slow.jsonl");
    write(
        &scenario,
        r#"{"mcp_call":{"tool":"echo","arguments":{"text":"slow"},"timeout_ms":10000}}
{"type":"result","subtype":"success","is_error":false,"result":"after","session_id":"{{session_id}}"}
"#,
    );
    let mcp = claude_mcp_config(dir.path());
    let mut child = Command::new(FAKE_CLAUDE)
        .args([CLAUDE_BASE_ARGS, &["--mcp-config", mcp.to_str().unwrap()]].concat())
        .current_dir(dir.path())
        .env("G5_FAKE_SCENARIO", &scenario)
        .env("G5_FAKE_MCP_DELAY_MS", "2000")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(user_message("go").as_bytes())
        .unwrap();
    let frames = timed_frames(&mut child, 3);
    assert!(child.wait().unwrap().success());
    let (start_at, start) = &frames[0];
    let (result_at, result) = &frames[1];
    assert_eq!(start["type"], "assistant");
    assert_eq!(start["message"]["content"][0]["type"], "tool_use");
    assert_eq!(result["type"], "user");
    assert_eq!(result["message"]["content"][0]["type"], "tool_result");
    assert_eq!(
        result["message"]["content"][0]["content"][0]["text"],
        "slow"
    );
    // Ordering, not wall-clock position: under load the fake may take longer
    // than the server delay to start, so only the gap between the announce
    // frame and the result frame is asserted (it must cover the delay).
    assert!(
        start_at < result_at,
        "tool_use ({start_at:?}) must precede tool_result ({result_at:?})"
    );
    assert!(
        *result_at - *start_at >= Duration::from_millis(1500),
        "tool_result arrived {:?} after tool_use; the 2 s server delay was not awaited in between",
        *result_at - *start_at
    );
    assert_eq!(frames[2].1["result"], "after");
}

#[test]
fn fake_codex_announces_the_mcp_call_before_the_server_answers() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = dir.path().join("slow.jsonl");
    write(
        &scenario,
        r#"{"type":"thread.started","thread_id":"{{session_id}}"}
{"mcp_call":{"tool":"echo","arguments":{"text":"slow"},"timeout_ms":10000}}
{"type":"turn.completed","usage":{}}
"#,
    );
    let home = codex_home_with_stub(dir.path(), "");
    let outcome = run_codex(
        dir.path(),
        &[
            ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
            ("CODEX_HOME", home.to_str().unwrap()),
            ("G5_FAKE_MCP_DELAY_MS", "2000"),
        ],
        codex_turn("go"),
    );
    assert_eq!(outcome.code(), 0, "{}", outcome.stderr());
    // Arrival times are taken by the reader thread as each line lands.
    let events = outcome.timed_events();
    let (start_at, start) = events[0];
    let (result_at, result) = events[1];
    assert_eq!(start["method"], "item/started");
    assert_eq!(start["params"]["item"]["type"], "mcpToolCall");
    assert_eq!(start["params"]["item"]["status"], "in_progress");
    assert_eq!(result["method"], "item/completed");
    assert_eq!(result["params"]["item"]["status"], "completed");
    assert!(
        start_at < result_at,
        "item/started ({start_at:?}) must precede item/completed ({result_at:?})"
    );
    assert!(
        result_at - start_at >= Duration::from_millis(1500),
        "item/completed arrived {:?} after item/started",
        result_at - start_at
    );
    assert_eq!(outcome.turn_completed().unwrap()["status"], "completed");
}

/// The fake never emits approval frames (design v6 §8.6): the invariant holds
/// at the emission boundary, not only for `permission` steps.
#[test]
fn fake_codex_refuses_approval_frames_from_any_scenario_line() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("empty-home");
    fs::create_dir_all(&home).unwrap();
    let cases: [(&str, &str); 4] = [
        (
            "raw",
            r#"{"type":"exec_approval_request","id":"approval1","command":"rm -rf /"}"#,
        ),
        (
            "emit",
            r#"{"emit":{"type":"apply_patch_approval_request","id":"approval2"}}"#,
        ),
        (
            "item",
            r#"{"type":"item.started","item":{"id":"item_0","type":"exec_approval_request","command":"ls"}}"#,
        ),
        (
            "payload",
            r#"{"type":"event_msg","payload":{"type":"approval_request","id":"approval3"}}"#,
        ),
    ];
    for (name, line) in cases {
        let scenario = dir.path().join(format!("{name}.jsonl"));
        write(
            &scenario,
            &format!(
                "{{\"type\":\"thread.started\",\"thread_id\":\"{{{{session_id}}}}\"}}\n{line}\n{{\"type\":\"turn.completed\",\"usage\":{{}}}}\n"
            ),
        );
        let run = run_codex(
            dir.path(),
            &[
                ("G5_FAKE_SCENARIO", scenario.to_str().unwrap()),
                ("CODEX_HOME", home.to_str().unwrap()),
            ],
            CodexTurn {
                launch: &["-c", "approval_policy=\"never\""],
                ..codex_turn("x")
            },
        );
        assert_eq!(run.code(), 1, "{name}: {}", run.stderr());
        assert!(
            run.stderr().contains("never emits approval frames")
                && run.stderr().contains(&format!("{name}.jsonl:2")),
            "{name}: {}",
            run.stderr()
        );
        // `thread.started` has no App Server notification, so nothing follows
        // `turn/started` before the refusal.
        assert!(
            run.events().is_empty(),
            "{name}: only thread.started may precede the refusal: {}",
            run.run.stdout
        );
        assert_no_approval_frames(&run);
    }
}

#[test]
fn fake_claude_replies_from_the_builtin_scenario_without_a_scenario_file() {
    let dir = tempfile::tempdir().unwrap();
    let mcp_config = claude_mcp_config(dir.path());
    let session = "6e2b1c0a-5d4f-4a3b-8c2d-1f0e9d8c7b6a";
    let outcome = run(
        FAKE_CLAUDE,
        dir.path(),
        &[
            CLAUDE_BASE_ARGS,
            &[
                "--session-id",
                session,
                "--mcp-config",
                mcp_config.to_str().unwrap(),
                "--permission-mode",
                "default",
            ],
        ]
        .concat(),
        &[],
        Some(&user_message("hello")),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    let frames = outcome.frames();
    assert_eq!(frames[0]["type"], "system");
    assert_eq!(frames[0]["mcp_servers"][0]["name"], "g5-tools");
    assert_eq!(frames[0]["permissionMode"], "default");
    let result = frames.last().unwrap();
    assert_eq!(result["type"], "result");
    assert_eq!(result["result"], "hello from claude code");
    assert_eq!(result["session_id"], session);

    // Any other prompt gets the prefixed echo with the session substituted.
    let outcome = run(
        FAKE_CLAUDE,
        dir.path(),
        &[
            CLAUDE_BASE_ARGS,
            &[
                "--session-id",
                session,
                "--mcp-config",
                mcp_config.to_str().unwrap(),
            ],
        ]
        .concat(),
        &[],
        Some(&user_message("write a poem")),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    let text = outcome.frames().last().unwrap()["result"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        text.starts_with("Claude Code fake response: [fake-claude ")
            && text.contains(&format!("session={session}"))
            && text.ends_with("write a poem"),
        "{text}"
    );
}

/// Review round 1: like the pinned CLI, a `--settings` path that does not
/// exist fails the launch with exit 1 before any frame.
#[test]
fn fake_claude_refuses_a_missing_settings_file_like_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join(".claude/settings.json");
    let outcome = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, &["--settings", missing.to_str().unwrap()]].concat(),
        &[],
        Some(&user_message("hello")),
    );
    assert_eq!(outcome.code(), 1, "stderr: {}", outcome.stderr);
    assert!(outcome.frames().is_empty(), "no frame before the refusal");
    assert!(
        outcome.stderr.contains(&format!(
            "Error: Settings file not found: {}",
            missing.display()
        )),
        "{}",
        outcome.stderr
    );
}

/// Review round 1: like the pinned CLI, a missing `--mcp-config` file fails
/// the launch with exit 1 before any frame.
#[test]
fn fake_claude_refuses_a_missing_mcp_config_like_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join(".mcp.json");
    let outcome = run(
        FAKE_CLAUDE,
        dir.path(),
        &[
            CLAUDE_BASE_ARGS,
            &[
                "--mcp-config",
                missing.to_str().unwrap(),
                "--strict-mcp-config",
            ],
        ]
        .concat(),
        &[],
        Some(&user_message("hello")),
    );
    assert_eq!(outcome.code(), 1, "stderr: {}", outcome.stderr);
    assert!(outcome.frames().is_empty(), "no frame before the refusal");
    assert!(
        outcome.stderr.contains(&format!(
            "Error: Invalid MCP configuration:\nMCP config file not found: {}",
            missing.display()
        )),
        "{}",
        outcome.stderr
    );
}

/// Review round 1: `--effort` is recorded; an unknown value is warned about
/// and ignored like the pinned CLI, and never fails the turn.
#[test]
fn fake_claude_records_the_effort_and_warns_about_unknown_values() {
    let dir = tempfile::tempdir().unwrap();
    let receipt = dir.path().join("receipt.json");
    let outcome = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, &["--effort", "low"]].concat(),
        &[("G5_FAKE_RECEIPT", receipt.to_str().unwrap())],
        Some(&user_message("hello")),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    assert!(
        !outcome.stderr.contains("Unknown --effort"),
        "{}",
        outcome.stderr
    );
    let recorded: Value = serde_json::from_str(&fs::read_to_string(&receipt).unwrap()).unwrap();
    let argv: Vec<&str> = recorded["argv"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        argv.windows(2).any(|pair| pair == ["--effort", "low"]),
        "{argv:?}"
    );

    let outcome = run(
        FAKE_CLAUDE,
        dir.path(),
        &[CLAUDE_BASE_ARGS, &["--effort", "ultra"]].concat(),
        &[],
        Some(&user_message("hello")),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stderr.contains("Unknown --effort value 'ultra'"),
        "{}",
        outcome.stderr
    );
}

#[test]
fn fake_codex_replies_from_the_builtin_scenario_without_a_scenario_file() {
    let dir = tempfile::tempdir().unwrap();
    let home = codex_home_with_stub(dir.path(), "");
    let receipt = dir.path().join("receipt.json");
    // Formal review's strict, feature-restricted launch reaches the same engine.
    let outcome = run_codex(
        dir.path(),
        &[
            ("CODEX_HOME", home.to_str().unwrap()),
            ("G5_FAKE_RECEIPT", receipt.to_str().unwrap()),
        ],
        CodexTurn {
            launch: &["--strict-config", "--disable", "plugins"],
            ..codex_turn("hello")
        },
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    assert_eq!(outcome.agent_texts(), ["hello from codex"]);
    assert_eq!(outcome.events().last().unwrap()["method"], "turn/completed");
    let recorded: Value = serde_json::from_str(&fs::read_to_string(&receipt).unwrap()).unwrap();
    assert!(recorded["options"]["config_overrides"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["key"] == "features.plugins" && o["value"] == "false"));
    assert!(
        fake_provider_cli::session_files::find_codex_rollout(&home, &outcome.thread_id()).is_some()
    );

    let outcome = run_codex(
        dir.path(),
        &[("CODEX_HOME", home.to_str().unwrap())],
        codex_turn("summarize"),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    assert_eq!(outcome.agent_texts(), ["Codex fake response: summarize"]);

    // The worker's Codex adapter appends its workspace-paths section to the
    // prompt after a blank line; the built-in `hello` turn keys on the first
    // paragraph, so the deterministic reply survives that context.
    let outcome = run_codex(
        dir.path(),
        &[("CODEX_HOME", home.to_str().unwrap())],
        codex_turn("hello\n\n## Current workspace paths\n\nCurrent working directory:\n`/work`\n"),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    assert_eq!(outcome.agent_texts(), ["hello from codex"]);

    // An ephemeral thread replies the same but materializes no rollout.
    let outcome = run_codex(
        dir.path(),
        &[("CODEX_HOME", home.to_str().unwrap())],
        CodexTurn {
            ephemeral: true,
            ..codex_turn("hello")
        },
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    assert_eq!(
        outcome.response("thread").unwrap()["result"]["thread"]["ephemeral"],
        true
    );
    assert_eq!(outcome.agent_texts(), ["hello from codex"]);
    assert!(
        fake_provider_cli::session_files::find_codex_rollout(&home, &outcome.thread_id()).is_none()
    );
}

#[test]
fn provider_specific_scenario_env_wins_over_the_shared_one() {
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared.jsonl");
    write(
        &shared,
        r#"{"type":"thread.started","thread_id":"{{session_id}}"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"from shared"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}
"#,
    );
    let specific = dir.path().join("codex.jsonl");
    write(
        &specific,
        r#"{"type":"thread.started","thread_id":"{{session_id}}"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"from codex-specific"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}
"#,
    );
    let home = codex_home_with_stub(dir.path(), "");
    let outcome = run_codex(
        dir.path(),
        &[
            ("CODEX_HOME", home.to_str().unwrap()),
            ("G5_FAKE_SCENARIO", shared.to_str().unwrap()),
            ("G5_FAKE_CODEX_SCENARIO", specific.to_str().unwrap()),
        ],
        codex_turn("hi"),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    assert_eq!(outcome.agent_texts(), ["from codex-specific"]);

    // The shared variable still applies when no provider-specific one is set.
    let outcome = run_codex(
        dir.path(),
        &[
            ("CODEX_HOME", home.to_str().unwrap()),
            ("G5_FAKE_SCENARIO", shared.to_str().unwrap()),
        ],
        codex_turn("hi"),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    assert_eq!(outcome.agent_texts(), ["from shared"]);
}

// Review round 1 (C4): the prompt-selected workflows ported from the retired
// Node fake sidecar (contract §7.5).

/// A fake `g5d` that appends its resolved cwd and argv (one per line) to a
/// log, writes `stderr`, and exits with `exit_code`.
fn recording_g5d(dir: &Path, exit_code: i32, stderr: &str) -> (PathBuf, PathBuf) {
    recording_g5d_with(dir, exit_code, stderr, "", None)
}

/// A fake `g5d` that appends its working directory and arguments to the
/// returned log, optionally copies its stdin to `stdin_capture`, prints
/// `stdout`, writes `stderr` and exits with `exit_code`.
fn recording_g5d_with(
    dir: &Path,
    exit_code: i32,
    stderr: &str,
    stdout: &str,
    stdin_capture: Option<&Path>,
) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let log = dir.join("g5d-calls.log");
    let script = dir.join("fake-bin").join("g5d");
    // Like the real g5d, the stub always reads its stdin: exiting before the
    // caller finishes writing would fail the write with a broken pipe.
    let capture = stdin_capture
        .map(|path| format!("cat > '{}'\n", path.display()))
        .unwrap_or_else(|| "cat > /dev/null\n".to_string());
    write(
        &script,
        &format!(
            "#!/bin/sh\n{{ pwd -P; for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done; }} >> '{}'\n{capture}printf '%s' '{stdout}'\nprintf '%s' '{stderr}' >&2\nexit {exit_code}\n",
            log.display()
        ),
    );
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    (script, log)
}

fn claude_workflow_turn(workspace: &Path, prompt: &str, env: &[(&str, &str)]) -> Run {
    let mcp_config = claude_mcp_config(workspace);
    run(
        FAKE_CLAUDE,
        workspace,
        &[
            CLAUDE_BASE_ARGS,
            &["--mcp-config", mcp_config.to_str().unwrap()],
        ]
        .concat(),
        env,
        Some(&user_message(prompt)),
    )
}

fn result_text(outcome: &Run) -> String {
    outcome.frames().last().expect("result frame")["result"]
        .as_str()
        .expect("result text")
        .to_string()
}

/// The reply's `boundary=g5d=<pin>,claude=<exe>` token names the pinned g5d
/// and this fake.
fn assert_boundary(reply: &str, g5d: &str) {
    let (_, token) = reply.split_once("boundary=").expect("boundary token");
    let token = token.strip_suffix('.').expect("reply ends with a period");
    let (g5d_part, claude_part) = token.split_once(",claude=").expect("claude part");
    assert_eq!(g5d_part, format!("g5d={g5d}"), "{reply}");
    assert_eq!(
        Path::new(claude_part).canonicalize().unwrap(),
        Path::new(FAKE_CLAUDE).canonicalize().unwrap(),
        "{reply}"
    );
}

#[test]
fn fake_claude_stream_deltas_workflow_streams_the_reply_as_one_frame_per_delta() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = claude_workflow_turn(dir.path(), "Stream it. G5_STREAM_DELTAS_E2E:300:8", &[]);
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    let frames = outcome.frames();
    let deltas: Vec<String> = frames
        .iter()
        .filter(|frame| {
            frame["type"] == "stream_event"
                && frame["event"]["type"] == "content_block_delta"
                && frame["event"]["delta"]["type"] == "text_delta"
        })
        .map(|frame| {
            frame["event"]["delta"]["text"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(deltas.len(), 300);
    assert_eq!(deltas[0], "w00001a ");
    assert_eq!(deltas[299], "w00300a ");
    assert!(
        deltas.iter().all(|delta| delta.len() == 8),
        "every delta is 8 bytes"
    );
    let reply = deltas.concat();
    let assistant = frames
        .iter()
        .find(|frame| {
            frame["type"] == "assistant" && frame["message"]["content"][0]["type"] == "text"
        })
        .expect("assistant text frame");
    assert_eq!(assistant["message"]["content"][0]["text"], reply);
    assert_eq!(assistant["message"]["stop_reason"], "end_turn");
    assert_eq!(result_text(&outcome), reply);
    // The text block frames stay ordered: start, every delta, then the message.
    let start = frames
        .iter()
        .position(|frame| {
            frame["event"]["type"] == "content_block_start" && frame["event"]["index"] == 1
        })
        .unwrap();
    assert_eq!(
        frames[start + 1]["event"]["delta"]["text"],
        "w00001a ",
        "first delta follows the text block start"
    );
    assert_eq!(frames[start + 300]["event"]["delta"]["text"], "w00300a ");
    assert_eq!(frames[start + 301]["type"], "assistant");

    // A malformed directive fails the turn like the other fixtures.
    let outcome = claude_workflow_turn(dir.path(), "G5_STREAM_DELTAS_E2E:0:8", &[]);
    assert_ne!(outcome.code(), 0);
    assert!(
        outcome.stderr.contains("count must be"),
        "{}",
        outcome.stderr
    );
}

#[test]
fn fake_claude_citation_workflow_reads_the_observed_path_and_cites_the_canonical_one() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = claude_workflow_turn(
        dir.path(),
        "Cite the helper.\nG5_CITATION_E2E:/work/repo/src/lib.rs|src/lib.rs\n",
        &[],
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    let frames = outcome.frames();
    let tool_use = frames
        .iter()
        .map(|frame| &frame["message"]["content"][0])
        .find(|block| block["type"] == "tool_use")
        .expect("tool_use block");
    assert_eq!(tool_use["name"], "Read");
    assert_eq!(tool_use["input"]["file_path"], "/work/repo/src/lib.rs");
    let tool_id = tool_use["id"].as_str().unwrap();
    assert!(tool_id.starts_with("fake-citation-"), "{tool_id}");
    let tool_result = frames
        .iter()
        .find(|frame| frame["type"] == "user")
        .expect("tool_result frame");
    assert_eq!(tool_result["message"]["content"][0]["tool_use_id"], tool_id);
    assert_eq!(
        tool_result["message"]["content"][0]["content"],
        "deterministic citation fixture read succeeded"
    );
    assert!(frames
        .iter()
        .any(|frame| frame["message"]["content"][0]["thinking"]
            == "Claude Code fake thinking summary: planning the fake response."));
    assert_eq!(result_text(&outcome), "Citation proof: src/lib.rs.");
}

#[test]
fn fake_codex_citation_workflow_reports_the_observed_file_change() {
    let dir = tempfile::tempdir().unwrap();
    let home = codex_home_with_stub(dir.path(), "");
    let outcome = run_codex(
        dir.path(),
        &[("CODEX_HOME", home.to_str().unwrap())],
        codex_turn("Cite it. G5_CITATION_E2E:/work/repo/a.rs|a.rs"),
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    let change = outcome
        .events()
        .into_iter()
        .find(|event| event["params"]["item"]["type"] == "fileChange")
        .expect("fileChange item");
    assert_eq!(change["method"], "item/completed");
    let item = &change["params"]["item"];
    assert_eq!(item["changes"][0]["path"], "/work/repo/a.rs");
    assert!(item["id"].as_str().unwrap().starts_with("fake-citation-"));
    assert_eq!(outcome.agent_texts(), ["Citation proof: a.rs."]);
}

#[test]
fn fake_claude_workspace_write_and_task_completion_run_the_pinned_g5d() {
    let dir = tempfile::tempdir().unwrap();
    let (g5d, log) = recording_g5d(dir.path(), 0, "");
    let prompt = format!(
        "Write it.\nG5_FS_WRITE_E2E:out/result.txt|{} G5_TASK_COMPLETE_E2E:{}\n",
        STANDARD.encode("fixture bytes\n"),
        STANDARD.encode("all done")
    );
    let outcome = claude_workflow_turn(
        dir.path(),
        &prompt,
        &[("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())],
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    assert_eq!(
        fs::read_to_string(dir.path().join("out/result.txt")).unwrap(),
        "fixture bytes\n"
    );
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        format!(
            "{}\ntask\ncomplete\n-m\nall done\n",
            dir.path().canonicalize().unwrap().display()
        )
    );
    let reply = result_text(&outcome);
    assert!(
        reply.starts_with(
            "Fake workspace write completed: out/result.txt; task completion recorded; boundary="
        ),
        "{reply}"
    );
    assert_boundary(&reply, g5d.to_str().unwrap());
}

#[test]
fn fake_claude_workspace_save_uses_the_cli_receipt_and_never_replays_history() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls");
    let script = dir.path().join("g5d");
    write(&script, &format!("#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nprintf '%s\\n' '{{\"data\":{{\"saved\":true,\"generation\":7}}}}'\n", log.display()));
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let env = [("G5_FAKE_G5D_BIN", script.to_str().unwrap())];
    let outcome = claude_workflow_turn(dir.path(), "Save changes. G5_WORKSPACE_SAVE_E2E", &env);
    assert_eq!(outcome.code(), 0, "{}", outcome.stderr);
    assert!(result_text(&outcome).contains("Fake workspace Save completed: generation 7"));
    assert_eq!(fs::read_to_string(&log).unwrap(), "workspace\nsave\n");
    let recovered = "Old request: G5_WORKSPACE_SAVE_E2E\nEnd of historical context. Current input follows:\nContinue without another Save.";
    let outcome = claude_workflow_turn(dir.path(), recovered, &env);
    assert_eq!(outcome.code(), 0, "{}", outcome.stderr);
    assert_eq!(fs::read_to_string(&log).unwrap(), "workspace\nsave\n");
}

#[test]
fn fake_claude_workspace_save_refuses_failed_or_unconfirmed_execution() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("g5d");
    for command in [
        "echo refused >&2; exit 1",
        "echo '{}'; exit 0",
        "echo '{\"data\":{\"saved\":false,\"generation\":7}}'; exit 0",
        "echo '{\"saved\":true,\"generation\":7}'; exit 0",
    ] {
        write(&script, &format!("#!/bin/sh\n{command}\n"));
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let outcome = claude_workflow_turn(
            dir.path(),
            "Save changes. G5_WORKSPACE_SAVE_E2E",
            &[("G5_FAKE_G5D_BIN", script.to_str().unwrap())],
        );
        assert_ne!(
            outcome.code(),
            0,
            "a fake reply must not qualify a failed Save"
        );
        assert!(!outcome.stdout.contains("Fake workspace Save completed"));
    }
}

#[test]
fn fake_claude_question_cli_lists_then_proposes_through_g5d() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("g5d-calls.log");
    let script = dir.path().join("fake-bin").join("g5d");
    write(
        &script,
        &format!(
            "#!/bin/sh\n{{ for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done; }} >> '{}'\nif [ \"$2\" = list ]; then echo '  #1 pick-0123abcd  [open] revision 1  decision: false'; else echo 'Recorded proposal on pick-0123abcd'; fi\n",
            log.display()
        ),
    );
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let spec = json!({
        "path": "docs/ws/questions/plan.g5q.json",
        "id": "pick-0123abcd",
        "expectedRevision": 1,
        "options": ["set-file"],
    });
    let prompt = format!(
        "Propose it. G5_QUESTION_CLI_E2E:{}",
        STANDARD.encode(serde_json::to_vec(&spec).unwrap())
    );
    let outcome = claude_workflow_turn(
        dir.path(),
        &prompt,
        &[("G5_FAKE_G5D_BIN", script.to_str().unwrap())],
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "question\nlist\n--path\ndocs/ws/questions/plan.g5q.json\nquestion\nanswer\n--path\ndocs/ws/questions/plan.g5q.json\n--id\npick-0123abcd\n--expected-revision\n1\n--option\nset-file\n"
    );
    let reply = result_text(&outcome);
    assert!(
        reply.starts_with("Fake question proposal recorded: Recorded proposal on pick-0123abcd;"),
        "{reply}"
    );
}

#[test]
fn fake_claude_meeting_extraction_creates_the_set_and_the_decisions_document() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = "0a0a0a0a-0a0a-4a0a-8a0a-0a0a0a0a0a0a";
    // The VM's `project_docs` alias → the workspace's Project Docs folder,
    // inside the workspace like the approval-review fixture requires.
    let docs_folder = dir.path().join("docs-repo").join("docs").join(workspace);
    fs::create_dir_all(docs_folder.join("meetings")).unwrap();
    write(
        &docs_folder.join("meetings/ui-discussion.md"),
        "Tim: the timeline is the landing page.\n",
    );
    std::os::unix::fs::symlink(&docs_folder, dir.path().join("project_docs")).unwrap();
    let stdin_log = dir.path().join("g5d-stdin.json");
    let set_path = format!("docs/{workspace}/questions/meeting-ui-discussion.g5q.json");
    let (g5d, log) = recording_g5d_with(
        dir.path(),
        0,
        "",
        &format!(
            "{{\"data\":{{\"created\":true,\"set\":{{\"path\":\"{set_path}\"}},\"questionIds\":[\"timeline-0001aaaa\",\"ribbon-0002bbbb\"]}}}}"
        ),
        Some(&stdin_log),
    );
    let prompt = format!(
        "You are the Business Engineering persona.\n\nExtract the decisions from the meeting transcript.\n\nThe current document is the transcript …\n\nContext: the current document is `docs/{workspace}/meetings/ui-discussion.md`."
    );
    let outcome = claude_workflow_turn(
        dir.path(),
        &prompt,
        &[("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())],
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        format!(
            "{}\n--json\nquestion\ncreate\n--set\nmeeting-ui-discussion\n--title\nMeeting ui-discussion\n--file\n-\n",
            dir.path().canonicalize().unwrap().display()
        )
    );
    let questions: Value = serde_json::from_str(&fs::read_to_string(&stdin_log).unwrap()).unwrap();
    let questions = questions.as_array().unwrap();
    assert_eq!(questions.len(), 2);
    // Stable ids, so a second extraction edits instead of appending.
    assert_eq!(
        questions
            .iter()
            .map(|question| question["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["timeline-landing-page", "ribbon-templated-chats"]
    );
    for question in questions {
        assert_eq!(question["allowFreeText"], true);
        assert_eq!(
            question["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["attach", "detach", "reject"]
        );
        assert!(question["context"]
            .as_str()
            .unwrap()
            .contains(&format!("docs/{workspace}/requirements.md")));
    }
    let decisions =
        fs::read_to_string(docs_folder.join("meetings/ui-discussion.decisions.md")).unwrap();
    assert!(
        decisions.starts_with(
            "# Decisions — Meeting ui-discussion\n\nTranscript: [ui-discussion.md](./ui-discussion.md)\n"
        ),
        "{decisions}"
    );
    assert!(
        decisions.contains(
            "[Q1. The project timeline becomes the landing page]\n\n```g5-question-ref\n"
        ),
        "{decisions}"
    );
    assert!(
        decisions.contains(&format!(
            "{{\"id\":\"timeline-0001aaaa\",\"path\":\"{set_path}\"}}"
        )),
        "{decisions}"
    );
    assert!(
        decisions.contains(&format!(
            "{{\"id\":\"ribbon-0002bbbb\",\"path\":\"{set_path}\"}}"
        )),
        "{decisions}"
    );
    let reply = result_text(&outcome);
    assert!(
        reply.starts_with(&format!(
            "Fake meeting decisions recorded: docs/{workspace}/meetings/ui-discussion.decisions.md; 2 decisions in {set_path}; boundary="
        )),
        "{reply}"
    );
}

#[test]
fn fake_claude_meeting_extraction_bounds_the_set_id_and_refuses_bad_inputs() {
    // A long transcript name: the slug is cut at 56 characters before the
    // `meeting-` prefix, so the id stays within the store's limit.
    let dir = tempfile::tempdir().unwrap();
    let workspace = "0a0a0a0a-0a0a-4a0a-8a0a-0a0a0a0a0a0a";
    let docs_folder = dir.path().join("docs-repo").join("docs").join(workspace);
    fs::create_dir_all(docs_folder.join("meetings")).unwrap();
    std::os::unix::fs::symlink(&docs_folder, dir.path().join("project_docs")).unwrap();
    let long = "weekly_ui.discussion_with_a_very_long_name_that_keeps_going_2026-10-06";
    let (g5d, log) = recording_g5d_with(
        dir.path(),
        0,
        "",
        &format!(
            "{{\"data\":{{\"created\":true,\"set\":{{\"path\":\"docs/{workspace}/questions/x.g5q.json\"}},\"questionIds\":[\"a-0001aaaa\",\"b-0002bbbb\"]}}}}"
        ),
        None,
    );
    let prompt = format!(
        "Extract the decisions from the meeting transcript.\n\nContext: the current document is `docs/{workspace}/meetings/{long}.md`."
    );
    let outcome = claude_workflow_turn(
        dir.path(),
        &prompt,
        &[("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())],
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    let calls = fs::read_to_string(&log).unwrap();
    let set_id = calls.lines().nth(5).unwrap();
    assert_eq!(
        set_id,
        "meeting-weekly-ui-discussion-with-a-very-long-name-that-keeps-go"
    );
    assert_eq!(set_id.len(), 64, "{set_id}");

    // Boundary punctuation follows the template's rule, hyphens kept.
    let outcome = claude_workflow_turn(
        dir.path(),
        &format!(
            "Extract the decisions from the meeting transcript.\n\nContext: the current document is `docs/{workspace}/meetings/_notes.md`."
        ),
        &[("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())],
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    let calls = fs::read_to_string(&log).unwrap();
    assert!(calls.contains("\nmeeting--notes\n"), "{calls}");

    // A transcript outside `meetings/` is refused before g5d runs.
    let outcome = claude_workflow_turn(
        dir.path(),
        &format!(
            "Extract the decisions from the meeting transcript.\n\nContext: the current document is `docs/{workspace}/requirements.md`."
        ),
        &[("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())],
    );
    assert_ne!(outcome.code(), 0);
    assert!(
        outcome
            .stderr
            .contains("needs a transcript under docs/<workspace>/meetings/"),
        "{}",
        outcome.stderr
    );

    // An alias that leaves the workspace is refused before g5d runs.
    let outside = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), outside.path().join("project_docs")).unwrap();
    let (g5d_outside, log_outside) = recording_g5d(outside.path(), 0, "");
    let outcome = claude_workflow_turn(
        outside.path(),
        &format!(
            "Extract the decisions from the meeting transcript.\n\nContext: the current document is `docs/{workspace}/meetings/notes.md`."
        ),
        &[("G5_FAKE_G5D_BIN", g5d_outside.to_str().unwrap())],
    );
    // A failure after the input checks is the turn's reply, as from an
    // agent whose command failed: the worker publishes no provider stderr.
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    let reply = result_text(&outcome);
    assert!(
        reply.contains(
            "Fake meeting extraction failed: fake meeting extraction Project Docs alias is invalid"
        ),
        "{reply}"
    );
    assert!(
        !log_outside.exists(),
        "g5d must not run for an invalid alias"
    );
}

#[test]
fn fake_claude_write_set_changes_code_and_project_docs_in_one_turn() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    let cwd = root.join("app");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    let writes = json!([
        {"path": "app/out/result.txt", "contentBase64": STANDARD.encode("code\n")},
        {"path": "docs/docs/workspace/design.md", "contentBase64": STANDARD.encode("# doc\n")},
    ]);
    let prompt = format!(
        "G5_FS_WRITE_SET_E2E:{}",
        STANDARD.encode(serde_json::to_vec(&writes).unwrap())
    );
    let outcome = claude_workflow_turn(
        &cwd,
        &prompt,
        &[("G5D_WORKSPACE_ROOT", root.to_str().unwrap())],
    );
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    assert_eq!(
        fs::read_to_string(root.join("app/out/result.txt")).unwrap(),
        "code\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("docs/docs/workspace/design.md")).unwrap(),
        "# doc\n"
    );
    assert!(result_text(&outcome).contains("Fake workspace write completed: two workspace files"));
}

#[test]
fn fake_claude_waits_for_a_separate_workspace_release_after_writing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    let prompt = format!(
        "G5_FS_WRITE_E2E:started.txt|{} G5_WAIT_FOR_FILE_E2E:release.txt",
        STANDARD.encode("started\n")
    );
    let workspace = root.clone();
    let runner = std::thread::spawn(move || {
        let root_text = workspace.to_str().unwrap().to_owned();
        claude_workflow_turn(&workspace, &prompt, &[("G5D_WORKSPACE_ROOT", &root_text)])
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !root.join("started.txt").is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "fake provider did not write before waiting"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        !runner.is_finished(),
        "provider turn completed before release"
    );
    write(&root.join("release.txt"), "release\n");
    let outcome = runner.join().unwrap();
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr);
    assert_eq!(
        fs::read_to_string(root.join("started.txt")).unwrap(),
        "started\n"
    );
    assert!(result_text(&outcome).contains("Fake workspace write completed: started.txt"));
}

#[test]
fn recovered_history_does_not_replay_fixture_side_effects_in_an_isolated_cli() {
    let workspace = tempfile::tempdir().unwrap();
    let write_set = |prefix: &str| {
        STANDARD.encode(
            serde_json::json!([
                {"path":format!("{prefix}-code.txt"),"contentBase64":STANDARD.encode(prefix)},
                {"path":format!("{prefix}-docs.md"),"contentBase64":STANDARD.encode(prefix)}
            ])
            .to_string(),
        )
    };
    let prompt = format!(
            "Prior delivered conversation:\n{{\"content\":\"G5_FS_WRITE_SET_E2E:{} G5_WAIT_FOR_FILE_E2E:never.txt\"}}{}G5_FS_WRITE_SET_E2E:{}",
            write_set("old"), "\nEnd of historical context. Current input follows:\n", write_set("new")
        );
    let outcome = claude_workflow_turn(workspace.path(), &prompt, &[]);
    assert_eq!(outcome.code(), 0, "{}", outcome.stderr);
    assert!(result_text(&outcome).contains("Fake workspace write completed"));
    assert_eq!(
        std::fs::read(workspace.path().join("new-code.txt")).unwrap(),
        b"new"
    );
    assert_eq!(
        std::fs::read(workspace.path().join("new-docs.md")).unwrap(),
        b"new"
    );
    assert!(!workspace.path().join("old-code.txt").exists());
}

#[test]
fn fake_codex_write_set_waits_for_release_and_ignores_recovery_history() {
    struct ReleaseOnDrop {
        path: PathBuf,
        runner: Option<std::thread::JoinHandle<CodexRun>>,
    }
    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            let _ = fs::write(&self.path, "release\n");
            if let Some(runner) = self.runner.take() {
                // Keep the temporary workspace alive until the child sees
                // the release file, including when an assertion unwinds.
                let _ = runner.join();
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    let writes = json!([
        {"path":"app/code.txt","contentBase64":STANDARD.encode("code\n")},
        {"path":"docs/design.md","contentBase64":STANDARD.encode("# doc\n")},
    ]);
    let prompt = format!(
        "Prior delivered conversation: G5_FS_WRITE_E2E:old.txt|{} G5_WAIT_FOR_FILE_E2E:never.txt\nEnd of historical context. Current input follows:\nG5_FS_WRITE_SET_E2E:{} G5_WAIT_FOR_FILE_E2E:release.txt",
        STANDARD.encode("old"), STANDARD.encode(serde_json::to_vec(&writes).unwrap())
    );
    let workspace = root.clone();
    let runner = std::thread::spawn(move || {
        let root_text = workspace.to_str().unwrap().to_owned();
        run_codex(
            &workspace,
            &[
                ("G5D_WORKSPACE_ROOT", &root_text),
                ("CODEX_HOME", &root_text),
            ],
            codex_turn(&prompt),
        )
    });
    let mut release = ReleaseOnDrop {
        path: root.join("release.txt"),
        runner: Some(runner),
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fs::read_to_string(root.join("docs/design.md"))
        .ok()
        .as_deref()
        != Some("# doc\n")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "Codex did not write before waiting"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !release.runner.as_ref().unwrap().is_finished(),
        "Codex completed before the release gate"
    );
    assert!(
        !root.join("old.txt").exists(),
        "historical directives must not run again"
    );
    assert_eq!(
        fs::read_to_string(root.join("app/code.txt")).unwrap(),
        "code\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("docs/design.md")).unwrap(),
        "# doc\n"
    );
    write(&release.path, "release\n");
    let outcome = release.runner.take().unwrap().join().unwrap();
    assert_eq!(outcome.code(), 0, "stderr: {}", outcome.stderr());
    assert!(outcome
        .agent_texts()
        .iter()
        .any(|text| text.contains("Fake workspace write completed: two workspace files")));
    assert_eq!(outcome.turn_completed().unwrap()["status"], "completed");
}

#[test]
fn fake_claude_workflows_fail_the_turn_loudly() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let (failing_g5d, _) = recording_g5d(dir.path(), 3, "task is not running");
    let complete = format!("G5_TASK_COMPLETE_E2E:{}", STANDARD.encode("all done"));
    // (prompt, extra env, expected stderr fragment)
    type RefusalCase<'a> = (&'a str, Vec<(&'a str, &'a str)>, &'a str);
    let cases: [RefusalCase; 5] = [
        (
            "G5_FS_WRITE_E2E:../escape.txt|YQ==",
            vec![],
            "escapes the stable workspace",
        ),
        (&complete, vec![], "requires G5_FAKE_G5D_BIN"),
        (
            &complete,
            vec![("G5_FAKE_G5D_BIN", failing_g5d.to_str().unwrap())],
            "task is not running",
        ),
        (
            "G5_REPEATED_COMMIT_E2E:e30=",
            vec![],
            "invalid native repeated-commit payload",
        ),
        (
            "G5_BUDGET_RESUME_E2E:e30=",
            vec![],
            "invalid native budget-resume payload",
        ),
    ];
    for (prompt, env, expected) in cases {
        let outcome = claude_workflow_turn(&workspace, prompt, &env);
        assert_eq!(outcome.code(), 1, "{prompt:?} stdout: {}", outcome.stdout);
        assert!(
            outcome.stdout.trim().is_empty(),
            "{prompt:?} emitted frames: {}",
            outcome.stdout
        );
        assert!(
            outcome.stderr.contains(expected),
            "{prompt:?} stderr: {}",
            outcome.stderr
        );
    }
    assert!(!dir.path().join("escape.txt").exists());
}

// Run the provider against real local Git repositories. Only the task-context
// service is stubbed; commits, fetches, rebase, ref policy and readback are real.
fn native_git_binary() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("Git tests require PATH"))
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .expect("Git tests require git on PATH")
        .canonicalize()
        .expect("resolve absolute Git executable")
}

fn native_git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new(native_git_binary())
        .current_dir(cwd)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "CI Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@g5.internal")
        .env("GIT_COMMITTER_NAME", "CI Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@g5.internal")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn native_commit_fixture(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let workspace = dir.join("workspace");
    let repo = workspace.join("code");
    let origin = dir.join("origin.git");
    fs::create_dir_all(&workspace).unwrap();
    native_git(dir, &["init", "--bare", origin.to_str().unwrap()]);
    native_git(&workspace, &["clone", origin.to_str().unwrap(), "code"]);
    // Guest clones have no Git identity. Seed commands supply their own
    // process-only identity; the provider must do the same for its commits.
    native_git(&repo, &["checkout", "-b", "workspace/fixture"]);
    write(&repo.join("README.md"), "seed\n");
    native_git(&repo, &["add", "README.md"]);
    native_git(&repo, &["commit", "-m", "seed"]);
    native_git(
        &repo,
        &["push", "origin", "HEAD:refs/heads/workspace/fixture"],
    );
    // Current materialization uses a local task branch, while the authenticated
    // self-context owns the workspace ref to which publication is permitted.
    native_git(&repo, &["checkout", "-b", "task/local-only"]);
    let context = json!({
        "repoMounts": [{"repoId":"repo-1", "mountPath":"code"}],
        "branch": {"repoBranches": [{"repoId":"repo-1", "branchName":"workspace/fixture"}]}
    });
    write(&dir.join("context.json"), &context.to_string());
    let g5d = dir.join("g5d");
    write(&g5d, &format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n  'task context') cat '{}' ;;\n  *) echo unexpected-lifecycle-command >&2; exit 17 ;;\nesac\n",
        dir.join("context.json").display()
    ));
    fs::set_permissions(&g5d, fs::Permissions::from_mode(0o755)).unwrap();
    (workspace, origin, g5d)
}

fn native_commit_prompt(relative: &str, commits: Value) -> String {
    format!(
        "G5_REPEATED_COMMIT_E2E:{}",
        STANDARD.encode(
            json!({
                "mode":"chat", "relativePath":relative, "commits":commits,
                "measureTimings":true
            })
            .to_string()
        )
    )
}

#[test]
fn native_provider_authenticates_smart_http_without_persisting_or_leaking_credentials() {
    use std::io::BufRead;
    struct Server(std::process::Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let (workspace, origin, g5d) = native_commit_fixture(dir.path());
    native_git(&origin, &["config", "http.receivepack", "true"]);
    let requests = dir.path().join("http-requests.txt");
    let mut server = Server(
        Command::new("python3")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/git_http_server.py"))
            .arg(dir.path())
            .arg(&requests)
            .arg(native_git_binary())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut port = String::new();
    std::io::BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut port)
        .unwrap();
    let api = format!("http://127.0.0.1:{}", port.trim());
    let repo = workspace.join("code");
    native_git(
        &repo,
        &["remote", "set-url", "origin", &format!("{api}/git/repo-1")],
    );
    let prompt = native_commit_prompt(
        "code/http-proof.txt",
        json!([
            {"content":"authenticated checkpoint\n", "message":"authenticated checkpoint"}
        ]),
    );
    let env = [
        ("G5_FAKE_G5D_BIN", g5d.to_str().unwrap()),
        ("G5D_API_URL", api.as_str()),
        ("G5D_TOKEN", "test-worker-token"),
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "user.useConfigOnly"),
        ("GIT_CONFIG_VALUE_0", "true"),
    ];
    let result = claude_workflow_turn(&workspace, &prompt, &env);
    assert_eq!(result.code(), 0, "{}", result.stderr);
    assert_eq!(
        native_git(&origin, &["show", "workspace/fixture:http-proof.txt"]),
        "authenticated checkpoint"
    );
    assert_eq!(
        native_git(
            &origin,
            &["show", "-s", "--format=%an <%ae> / %cn <%ce>", "workspace/fixture"]
        ),
        "G5 Fake Provider <fake-provider@g5.internal> / G5 Fake Provider <fake-provider@g5.internal>"
    );
    let log = fs::read_to_string(&requests).unwrap();
    assert!(log.contains("GET /git/repo-1/info/refs auth=true"), "{log}");
    assert!(
        log.contains("POST /git/repo-1/git-receive-pack auth=true"),
        "{log}"
    );
    assert!(!fs::read_to_string(repo.join(".git/config"))
        .unwrap()
        .contains("Authorization"));
    assert!(!fs::read_to_string(repo.join(".git/config"))
        .unwrap()
        .contains("[user]"));
    assert!(!format!("{}{}", result.stdout, result.stderr).contains("test-worker-token"));

    // Even another repo on the same server must not receive this scoped header.
    native_git(
        &repo,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("{api}/git/unassigned"),
        ],
    );
    let denied = claude_workflow_turn(&workspace, &prompt, &env);
    assert_ne!(denied.code(), 0);
    let log = fs::read_to_string(&requests).unwrap();
    assert!(
        log.contains("GET /git/unassigned/info/refs auth=false"),
        "{log}"
    );
    assert!(!log.contains("/git/unassigned/info/refs auth=true"));
}

#[test]
fn native_provider_commits_publish_to_assigned_ref_and_preserve_peer_advance() {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, origin, g5d) = native_commit_fixture(dir.path());
    let repo = workspace.join("code");
    let seed = native_git(&repo, &["rev-parse", "HEAD"]);
    let env = [("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())];
    let prompt = native_commit_prompt(
        "code/proof.txt",
        json!([
            {"content":"first\n", "message":"first checkpoint"},
            {"content":"second\n", "message":"second checkpoint"}
        ]),
    );
    let first = claude_workflow_turn(&workspace, &prompt, &env);
    assert_eq!(first.code(), 0, "{}", first.stderr);
    assert!(result_text(&first).contains("commits=2; commitDurationsMs="));
    assert!(result_text(&first).contains("pushDurationsMs="));
    assert_eq!(
        native_git(&origin, &["show", "workspace/fixture:proof.txt"]),
        "second"
    );
    assert_eq!(
        native_git(
            &origin,
            &["rev-list", "--count", &format!("{seed}..workspace/fixture")]
        ),
        "2"
    );
    assert!(
        native_git(&origin, &["for-each-ref", "--format=%(refname)"])
            .lines()
            .all(|s| !s.contains("task/"))
    );

    let peer = dir.path().join("peer");
    native_git(
        dir.path(),
        &[
            "clone",
            "--branch",
            "workspace/fixture",
            origin.to_str().unwrap(),
            peer.to_str().unwrap(),
        ],
    );
    write(&peer.join("peer.txt"), "preserve peer\n");
    native_git(&peer, &["add", "peer.txt"]);
    native_git(&peer, &["commit", "-m", "peer commit"]);
    native_git(
        &peer,
        &["push", "origin", "HEAD:refs/heads/workspace/fixture"],
    );
    let peer_head = native_git(&peer, &["rev-parse", "HEAD"]);
    let next = claude_workflow_turn(
        &workspace,
        &native_commit_prompt(
            "code/proof.txt",
            json!([
                {"content":"third\n", "message":"third checkpoint"}
            ]),
        ),
        &env,
    );
    assert_eq!(next.code(), 0, "{}", next.stderr);
    assert_eq!(
        native_git(&origin, &["show", "workspace/fixture:proof.txt"]),
        "third"
    );
    assert_eq!(
        native_git(&origin, &["show", "workspace/fixture:peer.txt"]),
        "preserve peer"
    );
    native_git(
        &origin,
        &[
            "merge-base",
            "--is-ancestor",
            &peer_head,
            "workspace/fixture",
        ],
    );
}

#[test]
fn native_provider_does_not_report_success_when_publication_is_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (workspace, origin, g5d) = native_commit_fixture(dir.path());
    let before = native_git(&origin, &["rev-parse", "workspace/fixture"]);
    let hook = origin.join("hooks/pre-receive");
    write(
        &hook,
        "#!/bin/sh\necho injected-policy-rejection >&2\nexit 1\n",
    );
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let outcome = claude_workflow_turn(
        &workspace,
        &native_commit_prompt(
            "code/proof.txt",
            json!([
                {"content":"unpublished\n", "message":"rejected checkpoint"}
            ]),
        ),
        &[("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())],
    );
    assert_eq!(outcome.code(), 1);
    assert!(
        outcome.stderr.contains("injected-policy-rejection"),
        "{}",
        outcome.stderr
    );
    assert!(outcome.stdout.is_empty());
    assert_eq!(
        native_git(&origin, &["rev-parse", "workspace/fixture"]),
        before
    );
}

#[test]
fn native_provider_rejects_unassigned_paths_and_preserves_staged_work() {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, origin, g5d) = native_commit_fixture(dir.path());
    let before = native_git(&origin, &["rev-parse", "workspace/fixture"]);
    let env = [("G5_FAKE_G5D_BIN", g5d.to_str().unwrap())];
    for path in ["../escape", "unknown/proof.txt"] {
        let outcome = claude_workflow_turn(
            &workspace,
            &native_commit_prompt(
                path,
                json!([
                    {"content":"invalid\n", "message":"must not commit"}
                ]),
            ),
            &env,
        );
        assert_eq!(outcome.code(), 1, "{path}: {}", outcome.stdout);
    }
    let repo = workspace.join("code");
    write(&repo.join("peer.txt"), "unfinished peer work\n");
    native_git(&repo, &["add", "peer.txt"]);
    let outcome = claude_workflow_turn(
        &workspace,
        &native_commit_prompt(
            "code/proof.txt",
            json!([
                {"content":"invalid\n", "message":"must not capture peer work"}
            ]),
        ),
        &env,
    );
    assert_eq!(outcome.code(), 1, "{}", outcome.stdout);
    assert_eq!(
        native_git(&repo, &["diff", "--cached", "--name-only"]),
        "peer.txt"
    );
    assert_eq!(
        native_git(&origin, &["rev-parse", "workspace/fixture"]),
        before
    );
}

#[test]
fn native_budget_resume_publishes_before_exhaustion_and_retries_completion_without_an_extra_commit()
{
    let dir = tempfile::tempdir().unwrap();
    let (workspace, origin, g5d) = native_commit_fixture(dir.path());
    let seed = native_git(&origin, &["rev-parse", "workspace/fixture"]);
    let completion = dir.path().join("completed");
    let fail = dir.path().join("fail-completion");
    write(&g5d, &format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n  'task context') cat '{}' ;;\n  'task complete') if test -f '{}'; then echo injected-completion-rejection >&2; exit 12; fi; touch '{}' ;;\n  *) exit 17 ;;\nesac\n",
        dir.path().join("context.json").display(), fail.display(), completion.display()
    ));
    let payload = json!({
        "relativePath":"code/proof.txt", "checkpointContent":"checkpoint\n",
        "completedContent":"checkpoint\ncompleted\n", "checkpointMessage":"checkpoint",
        "completionCommitMessage":"complete", "completionMessage":"done", "expectedMaxTurns":2000
    });
    let prompt = format!(
        "G5_BUDGET_RESUME_E2E:{}",
        STANDARD.encode(payload.to_string())
    );
    let config = claude_mcp_config(&workspace);
    let turn = || {
        run(
            FAKE_CLAUDE,
            &workspace,
            &[
                CLAUDE_BASE_ARGS,
                &[
                    "--max-turns",
                    "2000",
                    "--mcp-config",
                    config.to_str().unwrap(),
                ],
            ]
            .concat(),
            &[
                ("G5_FAKE_G5D_BIN", g5d.to_str().unwrap()),
                ("G5_FAKE_BUDGET_RESUME_E2E", "1"),
            ],
            Some(&user_message(&prompt)),
        )
    };
    let hook = origin.join("hooks/pre-receive");
    write(
        &hook,
        "#!/bin/sh\necho injected-push-rejection >&2\nexit 1\n",
    );
    fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let rejected_checkpoint = turn();
    assert_eq!(rejected_checkpoint.code(), 1);
    assert!(rejected_checkpoint
        .stderr
        .contains("injected-push-rejection"));
    assert!(!completion.exists());
    assert_eq!(
        native_git(&origin, &["rev-parse", "workspace/fixture"]),
        seed
    );
    fs::remove_file(&hook).unwrap();
    let first = turn();
    assert_eq!(first.code(), 0, "{}", first.stderr);
    let frames = first.frames();
    let result = frames.last().unwrap();
    assert_eq!(result["subtype"], "error_max_turns");
    assert_eq!(result["is_error"], true);
    assert_eq!(result["num_turns"], 2000);
    assert!(!completion.exists());
    assert_eq!(
        native_git(&origin, &["show", "workspace/fixture:proof.txt"]),
        "checkpoint"
    );

    write(
        &hook,
        "#!/bin/sh\necho injected-push-rejection >&2\nexit 1\n",
    );
    fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let rejected_completion = turn();
    assert_eq!(rejected_completion.code(), 1);
    assert!(rejected_completion
        .stderr
        .contains("injected-push-rejection"));
    assert!(!completion.exists());
    assert_eq!(
        native_git(&origin, &["show", "workspace/fixture:proof.txt"]),
        "checkpoint"
    );
    fs::remove_file(&hook).unwrap();

    write(&fail, "reject lifecycle after publication");
    let second = turn();
    assert_eq!(second.code(), 1);
    assert!(
        second.stderr.contains("injected-completion-rejection"),
        "{}",
        second.stderr
    );
    assert!(!completion.exists());
    assert_eq!(
        native_git(&origin, &["show", "workspace/fixture:proof.txt"]),
        "checkpoint\ncompleted"
    );
    let completed_head = native_git(&origin, &["rev-parse", "workspace/fixture"]);
    fs::remove_file(&fail).unwrap();
    let third = turn();
    assert_eq!(third.code(), 0, "{}", third.stderr);
    assert_eq!(third.frames().last().unwrap()["subtype"], "success");
    assert!(completion.exists());
    assert_eq!(
        native_git(&origin, &["rev-parse", "workspace/fixture"]),
        completed_head
    );
    assert_eq!(
        native_git(
            &origin,
            &["rev-list", "--count", &format!("{seed}..workspace/fixture")]
        ),
        "2"
    );
}

/// A value loop that demands App Server proof cannot be satisfied by `codex
/// exec`: the fake refuses exec outright (before reading the prompt), and the
/// same prompt completes over App Server.
#[test]
fn codex_exec_cannot_satisfy_an_app_server_only_value_loop() {
    let root = tempfile::tempdir().unwrap();
    let output = run(
        FAKE_CODEX,
        root.path(),
        &["exec", "--json"],
        &[],
        Some("G5_REQUIRE_CODEX_APP_SERVER_E2E\n"),
    );
    assert_eq!(output.code(), 2, "{}", output.stderr);
    assert!(
        output.stderr.contains(CODEX_EXEC_REFUSAL),
        "{}",
        output.stderr
    );
    assert!(output.stdout.is_empty());

    let app_server = run_codex(
        root.path(),
        &[],
        codex_turn("hello G5_REQUIRE_CODEX_APP_SERVER_E2E"),
    );
    assert_eq!(app_server.code(), 0, "{}", app_server.stderr());
    assert_eq!(app_server.turn_completed().unwrap()["status"], "completed");
}

#[test]
fn codex_app_server_fresh_resume_and_eof_match_the_attempt_transport() {
    fn roundtrip(home: &Path, resume: Option<&str>, scenario: Option<(&str, i32, bool)>) -> String {
        let mut command = Command::new(FAKE_CODEX);
        command.env_clear();
        if let Some((text, _, _)) = scenario {
            let path = home.join("app-server-scenario.jsonl");
            write(&path, text);
            command.env("G5_FAKE_SCENARIO", path);
        }
        command
            .args([
                "app-server",
                "--listen",
                "stdio://",
                "-c",
                "approval_policy=\"never\"",
            ])
            .env("PATH", std::env::var("PATH").unwrap())
            .env("HOME", home)
            .env("CODEX_HOME", home)
            .env("G5_FAKE_STRICT_RESUME", "1")
            .current_dir(home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender
                    .send(serde_json::from_str::<Value>(&line).unwrap())
                    .is_err()
                {
                    break;
                }
            }
        });
        let send = |input: &mut std::process::ChildStdin, frame: Value| {
            writeln!(input, "{frame}").unwrap();
            input.flush().unwrap();
        };
        let receive = || {
            output
                .recv_timeout(Duration::from_secs(10))
                .expect("native peer response deadline")
        };
        send(
            &mut input,
            json!({"id":"init","method":"initialize","params":{"clientInfo":{"name":"g5_agent","version":"1"},"capabilities":{"experimentalApi":false}}}),
        );
        let initialized = receive();
        assert_eq!(initialized["id"], "init");
        assert!(initialized["result"]["userAgent"]
            .as_str()
            .unwrap()
            .contains(&codex_pin()));
        send(&mut input, json!({"method":"initialized"}));
        let mut params =
            json!({"model":"fixture-model","sandbox":"read-only","approvalPolicy":"never"});
        if let Some(id) = resume {
            params["threadId"] = json!(id);
        }
        send(
            &mut input,
            json!({"id":"thread","method":if resume.is_some() {"thread/resume"} else {"thread/start"},"params":params}),
        );
        let started = receive();
        let thread = started["result"]["thread"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        if let Some(id) = resume {
            assert_eq!(thread, id);
        }
        send(
            &mut input,
            json!({"id":"turn","method":"turn/start","params":{"threadId":thread,"input":[{"type":"text","text":"hello G5_REQUIRE_CODEX_APP_SERVER_E2E"}]}}),
        );
        let accepted = receive();
        let turn = accepted["result"]["turn"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(accepted["result"]["turn"]["status"], "inProgress");
        let mut completed = false;
        let mut usage_seen = false;
        loop {
            let event = match output.recv_timeout(Duration::from_secs(10)) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) if scenario.is_some() => break,
                other => panic!("missing native output: {other:?}"),
            };
            assert!(
                event.get("id").is_some() || event["method"].is_string(),
                "{event}"
            );
            if event["method"] == "thread/tokenUsage/updated" {
                usage_seen = true;
                let usage = &event["params"]["tokenUsage"]["last"];
                assert_eq!(
                    usage["totalTokens"].as_u64().unwrap(),
                    usage["inputTokens"].as_u64().unwrap()
                        + usage["outputTokens"].as_u64().unwrap()
                );
            }
            if event["method"] == "turn/completed" {
                assert_eq!(event["params"]["threadId"], thread);
                assert_eq!(event["params"]["turn"]["id"], turn);
                assert_eq!(event["params"]["turn"]["status"], "completed");
                completed = true;
                break;
            }
        }
        let expected_completion = scenario.is_none_or(|(_, _, completion)| completion);
        assert_eq!(completed, expected_completion, "native completion evidence");
        if expected_completion {
            assert!(usage_seen, "completed scenario must publish native usage");
        }
        if scenario.is_none() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "native completion must still await client EOF"
            );
            drop(input);
        }
        // Requested exits and replay errors must terminate with stdin still open.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(scenario.map_or(0, |(_, code, _)| code)));
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                panic!("peer ignored EOF");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        reader.join().unwrap();
        assert!(fake_provider_cli::session_files::find_codex_rollout(home, &thread).is_some());
        thread
    }
    let root = tempfile::tempdir().unwrap();
    let thread = roundtrip(root.path(), None, None);
    assert_eq!(roundtrip(root.path(), Some(&thread), None), thread);
    let native_file =
        fake_provider_cli::session_files::find_codex_rollout(root.path(), &thread).unwrap();
    let saved_bytes = fs::read(&native_file).unwrap();
    for (prompt, refuse_env) in [("G5_TOOL_ONLY_FACT_E2E:refuse-resume", "0"), ("hello", "1")] {
        let input = [
            json!({"id":"init","method":"initialize","params":{"clientInfo":{"name":"g5_agent"},"capabilities":{"experimentalApi":false}}}),
            json!({"method":"initialized"}),
            json!({"id":"thread","method":"thread/resume","params":{"threadId":thread,"model":"fixture-model","sandbox":"read-only","approvalPolicy":"never"}}),
            json!({"id":"turn","method":"turn/start","params":{"threadId":thread,"input":[{"type":"text","text":prompt}]}}),
        ].into_iter().map(|frame| format!("{frame}\n")).collect::<String>();
        let refused = run(
            FAKE_CODEX,
            root.path(),
            &["app-server", "--listen", "stdio://"],
            &[
                ("HOME", root.path().to_str().unwrap()),
                ("CODEX_HOME", root.path().to_str().unwrap()),
                ("G5_FAKE_STRICT_RESUME", "1"),
                ("G5_FAKE_REFUSE_RESUME", refuse_env),
            ],
            Some(&input),
        );
        assert_eq!(refused.code(), 2, "{}", refused.stderr);
        let frames = refused.frames();
        assert!(frames
            .iter()
            .any(|frame| frame["id"] == "turn" && frame["error"]["code"] == -32600));
        assert!(!frames
            .iter()
            .any(|frame| frame["id"] == "turn" && frame.get("result").is_some()));
        assert!(!frames.iter().any(|frame| frame["method"] == "turn/started"));
        assert_eq!(
            fs::read(&native_file).unwrap(),
            saved_bytes,
            "rejected native set changed"
        );
    }
    for scenario in [
        ("{\"exit\":0}\n", 0, false),
        ("{\"type\":\"unrecognized_native_event\"}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":11,\"output_tokens\":3}}\n{\"exit\":7}\n", 7, true),
        ("{\"permission\":{\"tool_name\":\"Bash\",\"input\":{}}}\n", 1, false),
    ] {
        roundtrip(root.path(), None, Some(scenario));
    }
}
