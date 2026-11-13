//! Exercise the fake's live stdin, rather than replaying an already-consumed
//! user frame. Protocol assumptions are recorded from the pinned real CLI in
//! tests/fixtures/provider-frames/claude/steer-*.jsonl.
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{json, Value};

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    output: Receiver<Value>,
    dir: tempfile::TempDir,
}

impl Session {
    fn start(scenario: &str, replay: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scenario.jsonl");
        std::fs::write(&path, scenario).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_fake-claude"));
        cmd.args([
            "-p",
            "--output-format",
            "stream-json",
            "--input-format",
            "stream-json",
            "--verbose",
        ])
        .current_dir(dir.path())
        .env("G5_FAKE_CLAUDE_SCENARIO", path)
        .env("CLAUDE_CONFIG_DIR", dir.path().join("config"))
        .env("G5_FAKE_RECEIPT", dir.path().join("receipt.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
        if replay {
            cmd.arg("--replay-user-messages");
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let value = serde_json::from_str(&line).unwrap();
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            output,
            dir,
        }
    }

    fn send(&mut self, id: &str, content: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"type":"user","uuid":id,
            "message":{"role":"user","content":content}})
        )
        .unwrap();
        stdin.flush().unwrap();
    }

    fn next(&self) -> Value {
        self.output
            .recv_timeout(Duration::from_secs(10))
            .expect("fake provider stalled")
    }

    fn finish(&mut self) -> Vec<Value> {
        self.stdin.take();
        let mut frames = Vec::new();
        loop {
            match self.output.recv_timeout(Duration::from_secs(10)) {
                Ok(frame) => frames.push(frame),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(error) => panic!("fake provider did not reach EOF: {error}"),
            }
        }
        assert!(self.child.wait().unwrap().success());
        frames
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const TOOL: &str = r#"{"type":"system","subtype":"init","session_id":"{{session_id}}"}
{"type":"assistant","message":{"id":"{{uuid:tool}}","content":[{"type":"tool_use","id":"tool_1","name":"Bash","input":{"command":"sleep 1"}}]}}
{"delay_ms":1000,"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool_1","content":"first-done"}]}}
{"type":"assistant","message":{"id":"{{uuid:answer}}","content":[{"type":"text","text":"Answer: {{steered_prompt}}"}]}}
{"type":"result","subtype":"success","result":"Answer: {{steered_prompt}}","num_turns":2}
"#;

const TEXT: &str = r#"{"type":"system","subtype":"init","session_id":"{{session_id}}"}
{"type":"assistant","message":{"id":"{{uuid:answer}}","content":[{"type":"text","text":"First answer"}]}}
{"type":"result","subtype":"success","result":"First answer","num_turns":1}
"#;

#[test]
fn live_user_messages_are_consumed_after_tool_result_in_write_order() {
    for replay in [true, false] {
        let mut session = Session::start(TOOL, replay);
        session.send("initial", json!("Run the tool"));
        let mut frames = Vec::new();
        loop {
            let frame = session.next();
            let tool = frame["type"] == "assistant";
            frames.push(frame);
            if tool {
                break;
            }
        }
        let first = " exact \ntext with \"quotes\" and literal {{steered_prompt}} ";
        session.send("steer-1", json!(first));
        let image = json!([{"type":"text","text":"second"},
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}}]);
        session.send("steer-2", image.clone());
        // Closing stdin after writing is permitted by the real CLI: pending
        // messages must still be consumed rather than silently dropped.
        frames.extend(session.finish());
        assert_eq!(frames.iter().filter(|f| f["type"] == "result").count(), 1);
        assert_eq!(
            frames.last().unwrap()["result"],
            format!("Answer: {first}\nsecond")
        );
        let echoes: Vec<_> = frames.iter().filter(|f| f["isReplay"] == true).collect();
        if replay {
            assert_eq!(
                echoes
                    .iter()
                    .map(|f| f["uuid"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["initial", "steer-1", "steer-2"]
            );
            assert_eq!(echoes[1]["message"]["content"], first);
            assert_eq!(echoes[2]["message"]["content"], image);
            let result = frames
                .iter()
                .position(|f| f["message"]["content"][0]["type"] == "tool_result")
                .unwrap();
            assert_eq!(frames[result + 1]["uuid"], "steer-1");
            assert_eq!(frames[result + 2]["uuid"], "steer-2");
        } else {
            assert!(echoes.is_empty());
        }
        let receipt: Value = serde_json::from_slice(
            &std::fs::read(session.dir.path().join("receipt.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["stdin_messages"].as_array().unwrap().len(), 3);
        let sid = receipt["session_id"].as_str().unwrap();
        let path = fake_provider_cli::claude::transcript_path(
            &session.dir.path().join("config"),
            session.dir.path(),
            sid,
        );
        let transcript = std::fs::read_to_string(path).unwrap();
        assert_eq!(transcript.matches("queued_command").count(), 2);
        assert!(transcript.contains("steer-1"));
        assert!(transcript.contains("steer-2"));
    }
}

#[test]
fn message_during_final_text_runs_as_another_turn_in_the_same_process() {
    let mut session = Session::start(TEXT, true);
    session.send("initial", json!("First"));
    let mut frames = Vec::new();
    loop {
        let frame = session.next();
        let text = frame["type"] == "assistant";
        frames.push(frame);
        if text {
            break;
        }
    }
    session.send("follow-up", json!("second request"));
    frames.extend(session.finish());
    assert_eq!(frames.iter().filter(|f| f["type"] == "result").count(), 2);
    assert_eq!(
        frames
            .iter()
            .filter(|f| f["type"] == "system" && f["subtype"] == "init")
            .count(),
        2
    );
    let first_result = frames.iter().position(|f| f["type"] == "result").unwrap();
    let echo = frames
        .iter()
        .position(|f| f["isReplay"] == true && f["uuid"] == "follow-up")
        .unwrap();
    assert!(echo > first_result);
    assert!(frames.last().unwrap()["result"]
        .as_str()
        .unwrap()
        .contains("second request"));
    let receipt: Value =
        serde_json::from_slice(&std::fs::read(session.dir.path().join("receipt.json")).unwrap())
            .unwrap();
    let path = fake_provider_cli::claude::transcript_path(
        &session.dir.path().join("config"),
        session.dir.path(),
        receipt["session_id"].as_str().unwrap(),
    );
    let transcript: Vec<Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(transcript.iter().filter(|f| f["type"] == "user").count(), 2);
    assert_eq!(
        transcript
            .iter()
            .filter(|f| f["type"] == "assistant")
            .collect::<Vec<_>>(),
        frames
            .iter()
            .filter(|f| f["type"] == "assistant")
            .collect::<Vec<_>>(),
        "each completed assistant block must be persisted exactly once",
    );
}

#[test]
fn steering_waits_for_all_parallel_tool_results() {
    let scenario = r#"{"type":"system","subtype":"init"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"Bash"},{"type":"tool_use","id":"b","name":"Bash"}]}}
{"delay_ms":1000,"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","content":"a done"}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b","content":"b done"}]}}
{"type":"result","subtype":"success","result":"{{steered_prompt}}"}
"#;
    let mut session = Session::start(scenario, true);
    session.send("initial", json!("Run two tools"));
    loop {
        if session.next()["type"] == "assistant" {
            break;
        }
    }
    session.send("steer", json!("After both tools"));
    let frames = session.finish();
    assert_eq!(frames[0]["message"]["content"][0]["tool_use_id"], "a");
    assert_eq!(frames[1]["message"]["content"][0]["tool_use_id"], "b");
    assert_eq!(frames[2]["uuid"], "steer");
    assert_eq!(frames[3]["result"], "After both tools");
}
