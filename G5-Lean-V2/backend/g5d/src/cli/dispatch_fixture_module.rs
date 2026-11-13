//! `g5d dispatch-fixture-module` — a real module process for dispatcher
//! module-runtime tests (plan P4.2b items 4, 7, 11, 12). It speaks exactly the
//! runner's private contract: one invocation record on stdin, JSON events on
//! stdout, logs on stderr, `input.respond` control lines on stdin. It exists
//! only in `test-helpers` builds and is never part of a shipped catalog.

use std::io::{BufRead, Write};
use std::time::Duration;

use g5_contracts::error::G5Error;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const MODULE_ID: &str = "test.fixture";

pub async fn execute() -> Result<(), G5Error> {
    if std::env::var("G5_TEST_PREVIEW_SERVER").as_deref() == Ok("1") {
        return serve_preview_fixture().await;
    }
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?;
    let record: Value = serde_json::from_str(line.trim_end()).map_err(|e| G5Error::Internal {
        message: e.to_string(),
    })?;
    let input = record["input"].clone();
    let behavior = input["behavior"].as_str().unwrap_or("echo").to_owned();
    let outcome = match behavior.as_str() {
        "echo" => {
            emit("progress", json!({"step":1}));
            json!({"echo":input,"exec_id":record["exec_id"],"operation":record["operation"],"capabilities":record["capabilities"]})
        }
        "env_dump" => {
            let env: serde_json::Map<String, Value> = std::env::vars()
                .map(|(k, v)| (k, Value::String(v)))
                .collect();
            json!({"env":env,"cwd":std::env::current_dir().ok()})
        }
        "await_reply" => {
            let request_id = input["request_id"].as_str().unwrap_or("req-1").to_owned();
            let expected = input["replies_expected"].as_u64().unwrap_or(1);
            emit("input.request", json!({"request_id":request_id}));
            let mut received = Vec::new();
            let stdin = std::io::stdin();
            let mut control = String::new();
            loop {
                control.clear();
                match stdin.lock().read_line(&mut control) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let Ok(message) = serde_json::from_str::<Value>(control.trim_end()) else {
                    continue;
                };
                if message["kind"] == "input.respond" {
                    emit(
                        "reply",
                        json!({"request_id":message["request_id"],"provider_request_id":message["provider_request_id"],"validated_response":message["validated_response"]}),
                    );
                    received.push(message);
                    if received.len() as u64 >= expected {
                        break;
                    }
                }
            }
            if received.len() as u64 >= expected {
                // Optionally linger after the last reply so settlement can be
                // observed separately from delivery.
                if let Some(linger) = input["linger_ms"].as_u64() {
                    tokio::time::sleep(Duration::from_millis(linger)).await;
                }
                json!({"replies":received})
            } else {
                emit(
                    "outcome",
                    json!({"status":"failed","reason":"control_closed","replies":received}),
                );
                return Err(G5Error::Internal {
                    message: "control channel closed".into(),
                });
            }
        }
        "slow_reader" => {
            // Announce a request, then only start reading the control channel
            // after a delay: exercises delivery racing the RPC deadline and
            // the acknowledgement bound.
            let request_id = input["request_id"].as_str().unwrap_or("req-1").to_owned();
            let delay = input["delay_ms"].as_u64().unwrap_or(1_500);
            emit("input.request", json!({"request_id":request_id}));
            tokio::time::sleep(Duration::from_millis(delay)).await;
            let mut replies = Vec::new();
            let stdin = std::io::stdin();
            let mut control = String::new();
            loop {
                control.clear();
                match stdin.lock().read_line(&mut control) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if let Ok(message) = serde_json::from_str::<Value>(control.trim_end()) {
                    if message["kind"] == "input.respond" {
                        emit("reply", json!({"request_id":message["request_id"]}));
                        // Only the identity and size are reported: the
                        // response itself may exceed the module's output bound.
                        replies.push(json!({
                            "request_id": message["request_id"],
                            "bytes": control.trim_end().len(),
                        }));
                        break;
                    }
                }
            }
            if replies.is_empty() {
                emit(
                    "outcome",
                    json!({"status":"failed","reason":"control_closed","replies":replies}),
                );
                return Err(G5Error::Internal {
                    message: "control channel closed".into(),
                });
            }
            json!({"replies":replies})
        }
        "leak_file" => {
            // Wait for a file to appear, then print its contents to stderr in
            // two halves: a secret rotated after the module started must
            // still be masked from the retained logs.
            let path = input["path"].as_str().unwrap_or("").to_owned();
            let started = std::time::Instant::now();
            let text = loop {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if !text.is_empty() {
                        break text;
                    }
                }
                if started.elapsed() > Duration::from_secs(20) {
                    break String::new();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            };
            let split = text.len() / 2;
            let mut stderr = std::io::stderr();
            let _ = stderr.write_all(format!("rotated={}", &text[..split]).as_bytes());
            let _ = stderr.flush();
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = stderr.write_all(format!("{} end\n", &text[split..]).as_bytes());
            let _ = stderr.flush();
            json!({"printed":text.len()})
        }
        "sleep" => {
            let ms = input["ms"].as_u64().unwrap_or(30_000);
            emit("progress", json!({"sleeping_ms":ms}));
            tokio::time::sleep(Duration::from_millis(ms)).await;
            json!({"slept_ms":ms})
        }
        "spew" => {
            let bytes = input["bytes"].as_u64().unwrap_or(1024) as usize;
            let stream = input["stream"].as_str().unwrap_or("stdout");
            let chunk = if input["newlines"].as_bool().unwrap_or(true) {
                "x".repeat(1023) + "\n"
            } else {
                "x".repeat(1024)
            };
            let mut written = 0;
            while written < bytes {
                if stream == "stderr" {
                    let _ = std::io::stderr().write_all(chunk.as_bytes());
                } else {
                    let _ = std::io::stdout().write_all(chunk.as_bytes());
                }
                written += chunk.len();
            }
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            json!({"written":written})
        }
        "leak" => {
            // A secret split across two writes with a pause between them,
            // plus the same value inside an event payload.
            let text = input["text"].as_str().unwrap_or("").to_owned();
            let split = text.len() / 2;
            let mut stderr = std::io::stderr();
            let _ = stderr.write_all(format!("token={}", &text[..split]).as_bytes());
            let _ = stderr.flush();
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = stderr.write_all(format!("{} end\n", &text[split..]).as_bytes());
            let _ = stderr.flush();
            emit("progress", json!({"leaked":text}));
            json!({"echoed":text})
        }
        "plain_output" => {
            println!("hello plain world");
            json!({"printed":true})
        }
        "scripted" => {
            // Frames exactly as scripted — `frames: [{"payload": <json>,
            // "kind": "delta", "gap_ms": n}]` — each an event of the given
            // kind (`delta` by default) carrying the payload as given (a
            // string, an array of leaves, an object without text), followed
            // by its gap.
            let frames = input["frames"].as_array().cloned().unwrap_or_default();
            for frame in &frames {
                emit(
                    frame["kind"].as_str().unwrap_or("delta"),
                    frame["payload"].clone(),
                );
                if let Some(gap) = frame["gap_ms"].as_u64() {
                    tokio::time::sleep(Duration::from_millis(gap)).await;
                }
            }
            // The outcome is the scripted `outcome` when given.
            match input.get("outcome") {
                Some(outcome) => outcome.clone(),
                None => json!({"frames":frames.len()}),
            }
        }
        "split_events" => {
            // A secret split across consecutive structured text events, in
            // `parts` frames with an optional gap between them.
            let text = input["text"].as_str().unwrap_or("").to_owned();
            let parts = input["parts"].as_u64().unwrap_or(2).max(2) as usize;
            let gap = input["gap_ms"].as_u64().unwrap_or(0);
            // `interleave` puts a frame without any text between the parts.
            let interleave = input["interleave"].as_bool().unwrap_or(false);
            let size = text.len().div_ceil(parts);
            let mut offset = 0;
            for index in 0..parts {
                let end = (offset + size).min(text.len());
                let fragment = &text[offset..end];
                offset = end;
                let framed = if index == 0 {
                    format!("part one {fragment}")
                } else if index + 1 == parts {
                    format!("{fragment} last part")
                } else {
                    fragment.to_owned()
                };
                emit("delta", json!({"text":framed}));
                if interleave && index + 1 < parts {
                    emit("progress", json!({"tokens":index + 1}));
                }
                if gap > 0 && index + 1 < parts {
                    tokio::time::sleep(Duration::from_millis(gap)).await;
                }
            }
            json!({"frames":parts})
        }
        "split_into_outcome" => {
            // The second half of a secret arrives only in the outcome frame.
            let text = input["text"].as_str().unwrap_or("").to_owned();
            let split = text.len() / 2;
            emit(
                "delta",
                json!({"text":format!("part one {}", &text[..split])}),
            );
            if input["interleave"].as_bool().unwrap_or(false) {
                emit("progress", json!({"tokens":1}));
            }
            json!({"echo":format!("{} in outcome", &text[split..])})
        }
        "orphan" => {
            // A descendant that inherits our stdout/stderr and outlives us
            // unless the runner stops the whole process group.
            let ms = input["ms"].as_u64().unwrap_or(30_000);
            let marker = input["marker"].as_str().unwrap_or("").to_owned();
            let script = format!(
                "sleep {}; printf orphan-wrote > '{marker}'",
                ms as f64 / 1000.0
            );
            let mut command = std::process::Command::new("/bin/sh");
            command
                .arg("-c")
                .arg(script)
                .stdin(std::process::Stdio::null());
            if input["hold"].as_str() == Some("stderr") {
                // Hold only stderr open: the runner's stdout reader finishes
                // while the stderr reader has to be abandoned at the grace.
                command.stdout(std::process::Stdio::null());
            }
            if input["escape"].as_bool().unwrap_or(false) {
                // Leave the module's process group: the runner's group stop
                // cannot reach it, so only the bounded drain settles the run.
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            let child = command.spawn().map_err(|e| G5Error::Internal {
                message: e.to_string(),
            })?;
            json!({"orphan_pid":child.id()})
        }
        "exit" => {
            let code = input["code"].as_i64().unwrap_or(3) as i32;
            emit("outcome", json!({"reason":"requested_exit","code":code}));
            std::process::exit(code);
        }
        other => json!({"unknown_behavior":other}),
    };
    emit("outcome", outcome);
    Ok(())
}

/// Minimal customer preview process used by the production-boundary tests.
/// It deliberately has no G5 control-plane behavior: readiness, cancellation,
/// and the guest HTTP bridge remain owned by the real preview supervisor and
/// dispatcher.
async fn serve_preview_fixture() -> Result<(), G5Error> {
    let port = std::env::var("PORT")
        .map_err(|error| fixture_error(format!("missing preview PORT: {error}")))?
        .parse::<u16>()
        .map_err(|error| fixture_error(format!("invalid preview PORT: {error}")))?;
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| fixture_error(format!("bind preview fixture: {error}")))?;
    if let Ok(path) = std::env::var("G5_TEST_PREVIEW_PID_FILE") {
        std::fs::write(path, std::process::id().to_string())
            .map_err(|error| fixture_error(format!("write preview pid: {error}")))?;
    }
    loop {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|error| fixture_error(format!("accept preview request: {error}")))?;
        tokio::spawn(async move {
            let mut request = vec![0_u8; 8192];
            let read = socket.read(&mut request).await.unwrap_or(0);
            if read == 0 {
                return;
            }
            let request = &request[..read];
            if request
                .windows(b"Upgrade: websocket".len())
                .any(|window| window.eq_ignore_ascii_case(b"Upgrade: websocket"))
            {
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: fixture\r\n\r\n",
                    )
                    .await;
                let mut buffer = [0_u8; 4096];
                loop {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            if socket.write_all(&buffer[..read]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                return;
            }
            if let Ok(path) = std::env::var("G5_TEST_PREVIEW_WRITE_FILE") {
                let _ = std::fs::write(path, b"preview-request\n");
            }
            let _ = socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\nConnection: close\r\n\r\npreview-fixture\n",
                )
                .await;
            let _ = socket.shutdown().await;
        });
    }
}

fn fixture_error(message: String) -> G5Error {
    G5Error::Internal { message }
}

fn emit(kind: &str, payload: Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut stdout, &json!({"kind":kind,"payload":payload}));
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}
