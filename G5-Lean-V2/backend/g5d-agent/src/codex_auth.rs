//! Native subscription login before prompt delivery. CLI output is private:
//! only a validated device code crosses the authenticated HTTP channel.
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use g5_internal_api::codex_auth::{
    valid_device_code, CodexAuthPhase, CodexAuthUpdate, DEVICE_AUTH_URL, DEVICE_AUTH_WAIT_MS,
};
use g5_internal_api::InternalApiClient;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{timeout, Instant};

use crate::types::AgentTurnInputV1;

const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const CHALLENGE_TIMEOUT: Duration = Duration::from_secs(30);
const LOGIN_EXIT_GRACE: Duration = Duration::from_secs(2);
const MAX_LOGIN_OUTPUT: usize = 16 * 1024;

pub(crate) async fn authorize(
    input: &AgentTurnInputV1,
    client: &InternalApiClient,
    program: &Path,
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> Result<(), &'static str> {
    let gate = Gate {
        task: &input.task,
        client,
    };
    gate.update(CodexAuthPhase::Checking).await?;
    // Keep our copy through the terminal acknowledgment. The login's inherited
    // stdin holds another copy until its last descendant exits (see below).
    let mut held_lock = None;
    let result = authorize_inner(
        &gate,
        program,
        cwd,
        env,
        Duration::from_millis(DEVICE_AUTH_WAIT_MS),
        &mut held_lock,
    )
    .await;
    if let Err(code) = result {
        let _ = gate
            .update(CodexAuthPhase::Ended { code: code.into() })
            .await;
    }
    result
}

struct Gate<'a> {
    task: &'a crate::types::AgentTurnTaskV1,
    client: &'a InternalApiClient,
}

impl Gate<'_> {
    async fn update(&self, update: CodexAuthPhase) -> Result<(), &'static str> {
        let request = CodexAuthUpdate {
            task_id: self.task.task_id.parse().map_err(|_| "codex_auth_failed")?,
            attempt: self
                .task
                .attempt
                .try_into()
                .map_err(|_| "codex_auth_failed")?,
            update,
        };
        let response = timeout(
            CHECK_TIMEOUT,
            self.client
                .update_codex_auth(&self.task.session_id, &request),
        )
        .await
        .map_err(|_| "codex_auth_failed")?
        .map_err(|_| "codex_auth_failed")?;
        if response.proceed {
            Ok(())
        } else {
            Err(match response.code.as_deref() {
                Some("codex_auth_timeout") => "codex_auth_timeout",
                // A control plane older than follow-up queueing during login
                // may still answer with this code.
                Some("codex_auth_superseded") => "codex_auth_superseded",
                _ => "codex_auth_interrupted",
            })
        }
    }
}

fn command(program: &Path, cwd: &Path, env: &BTreeMap<String, String>) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Keep the dispatcher module's process group. Its settlement always kills
    // that group, including the npm launcher and native CLI after cancellation.
    command
}

async fn logged_in(
    program: &Path,
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> Result<bool, &'static str> {
    let output = timeout(
        CHECK_TIMEOUT,
        command(program, cwd, env)
            .args([
                "login",
                "status",
                "-c",
                "cli_auth_credentials_store=\"file\"",
            ])
            .output(),
    )
    .await
    .map_err(|_| "codex_auth_failed")?
    .map_err(|_| "codex_auth_failed")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() {
        if text.to_ascii_lowercase().contains("chatgpt") {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let path =
                    Path::new(env.get("CODEX_HOME").ok_or("codex_auth_failed")?).join("auth.json");
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                    .map_err(|_| "codex_auth_failed")?;
            }
            return Ok(true);
        }
        // A cached API key is not proof of a subscription login.
        return Ok(false);
    }
    if output.status.code() == Some(1) && text.to_ascii_lowercase().contains("not logged in") {
        Ok(false)
    } else {
        Err("codex_auth_failed")
    }
}

fn login_lock(home: &Path) -> Result<File, &'static str> {
    std::fs::create_dir_all(home).map_err(|_| "codex_auth_failed")?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(home.join("g5-device-auth.lock"))
        .map_err(|_| "codex_auth_failed")?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // The open file is owned by this module; flock is released on all exit
        // paths, including dispatcher SIGKILL and VM teardown.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("codex_auth_busy");
        }
    }
    Ok(file)
}

async fn authorize_inner(
    gate: &Gate<'_>,
    program: &Path,
    cwd: &Path,
    env: &BTreeMap<String, String>,
    wait: Duration,
    held_lock: &mut Option<File>,
) -> Result<(), &'static str> {
    // With an explicit CODEX_HOME, the pinned CLI rejects a missing directory
    // before it can report `Not logged in`. Fresh VMs only provision HOME.
    let home = env.get("CODEX_HOME").ok_or("codex_auth_failed")?;
    std::fs::create_dir_all(home).map_err(|_| "codex_auth_failed")?;
    if logged_in(program, cwd, env).await? {
        return gate.update(CodexAuthPhase::Ready).await;
    }
    *held_lock = Some(login_lock(Path::new(home))?);
    // Another chat may have finished between the first check and the lock.
    if logged_in(program, cwd, env).await? {
        return gate.update(CodexAuthPhase::Ready).await;
    }
    let mut child = command(program, cwd, env)
        // The pinned npm launcher passes stdio through to the native CLI.
        // Device login never reads stdin; the empty lock file is EOF just like
        // /dev/null. Unlike an arbitrary inheritable FD, stdin survives Node's
        // child-spawn descriptor filtering. If the launcher must be killed,
        // its native child retains the flock until dispatcher group cleanup,
        // so another chat cannot start an overlapping credential writer.
        .stdin(Stdio::from(
            held_lock
                .as_ref()
                .ok_or("codex_auth_failed")?
                .try_clone()
                .map_err(|_| "codex_auth_failed")?,
        ))
        .args([
            "login",
            "--device-auth",
            "-c",
            "cli_auth_credentials_store=\"file\"",
        ])
        .spawn()
        .map_err(|_| "codex_auth_failed")?;
    let (tx, mut rx) = mpsc::channel(16);
    let stdout = read_private_output(child.stdout.take().ok_or("codex_auth_failed")?, tx.clone());
    let stderr = read_private_output(child.stderr.take().ok_or("codex_auth_failed")?, tx);
    let mut output = Vec::new();
    let mut challenge_sent = false;
    let mut deadline = Instant::now() + CHALLENGE_TIMEOUT;
    let mut closed = 0;
    let mut exited = None;
    let result = loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => break Err("codex_auth_timeout"),
            status = child.wait(), if exited.is_none() => {
                match status {
                    Ok(status) => exited = Some(status),
                    Err(_) => break Err("codex_auth_failed"),
                }
            }
            chunk = rx.recv(), if closed < 2 => {
                match chunk {
                    Some(Some(bytes)) => {
                        output.extend(bytes);
                        if output.len() > MAX_LOGIN_OUTPUT { break Err("codex_auth_failed"); }
                        if !challenge_sent {
                            if let Some(code) = parse_challenge(&output) {
                                deadline = Instant::now() + wait;
                                let deadline_unix_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
                                    + wait.as_millis() as u64;
                                if let Err(error) = gate.update(CodexAuthPhase::Waiting { user_code: code, deadline_unix_ms }).await {
                                    break Err(error);
                                }
                                challenge_sent = true;
                            }
                        }
                    }
                    Some(None) | None => closed += 1,
                }
            }
        }
        if exited.is_some() && closed == 2 {
            break if exited.as_ref().is_some_and(|status| status.success()) {
                Ok(())
            } else if String::from_utf8_lossy(&output)
                .to_ascii_lowercase()
                .contains("disabled")
            {
                Err("codex_auth_disabled")
            } else {
                Err("codex_auth_failed")
            };
        }
    };
    // Never log stdout/stderr: they contain the device code. Give the pinned
    // npm launcher time to forward SIGTERM and reap its native CLI. A stubborn
    // descendant still holds the stdin flock until dispatcher group cleanup;
    // killing only the launcher must never admit a second login writer.
    if result.is_err() {
        stop_login(&mut child).await;
    } else {
        let _ = timeout(LOGIN_EXIT_GRACE, child.wait()).await;
    }
    stdout.abort();
    stderr.abort();
    result?;
    if !logged_in(program, cwd, env).await? {
        return Err("codex_auth_failed");
    }
    gate.update(CodexAuthPhase::Ready).await
}

async fn stop_login(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // The launcher and native CLI deliberately remain in the module's
        // process group so dispatcher cancellation still reaches both.
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    }
    #[cfg(not(unix))]
    let _ = child.start_kill();
    if !matches!(timeout(LOGIN_EXIT_GRACE, child.wait()).await, Ok(Ok(_))) {
        let _ = child.start_kill();
        let _ = timeout(LOGIN_EXIT_GRACE, child.wait()).await;
    }
}

fn read_private_output<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    reader: R,
    tx: mpsc::Sender<Option<Vec<u8>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(Some(buf[..n].to_vec())).await.is_err() {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(None).await;
    })
}

fn parse_challenge(bytes: &[u8]) -> Option<String> {
    // The pinned CLI uses ANSI styling even on pipes (0.153.3 and 0.159.3).
    // Strip CSI sequences before recognizing its fixed verification URL and a
    // complete code line.
    let text = String::from_utf8_lossy(bytes);
    let mut clean = String::new();
    let mut ansi = false;
    for c in text.chars() {
        if c == '\u{1b}' {
            ansi = true;
            continue;
        }
        if ansi {
            if c.is_ascii_alphabetic() {
                ansi = false;
            }
            continue;
        }
        clean.push(c);
    }
    if !clean.split_whitespace().any(|part| part == DEVICE_AUTH_URL) {
        return None;
    }
    clean
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n'))
        .find_map(|line| {
            let code = line.trim();
            valid_device_code(code).then(|| code.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_only_the_pinned_device_challenge_not_arbitrary_output() {
        let text = format!("1. Open this link in your browser and sign in  {DEVICE_AUTH_URL}\n\n2. Enter this one-time code (expires in 15 minutes)\n\n  \x1b[1mLO5E-I4FHN\x1b[0m\n");
        assert_eq!(
            parse_challenge(text.as_bytes()).as_deref(),
            Some("LO5E-I4FHN")
        );
        assert_eq!(
            parse_challenge(
                text.replace(DEVICE_AUTH_URL, "https://evil.example/device")
                    .as_bytes()
            ),
            None
        );
        assert_eq!(parse_challenge(b"LO5E-I4FHN\n"), None);
        assert_eq!(
            parse_challenge(format!("{DEVICE_AUTH_URL}\nLO5E-I4F").as_bytes()),
            None
        );
    }
    #[test]
    fn parses_the_pinned_cli_device_challenge_layout() {
        // `codex login --device-auth` on 0.159.3, stdout to a pipe; the code
        // is a placeholder for the one-time code the CLI printed.
        let text = "\nWelcome to Codex [v\x1b[90m0.159.3\x1b[0m]\n\x1b[90mOpenAI's command-line coding agent\x1b[0m\n\nFollow these steps to sign in with ChatGPT using device code authorization:\n\n1. Open this link in your browser and sign in to your account\n   \x1b[94mhttps://auth.openai.com/codex/device\x1b[0m\n\n2. Enter this one-time code \x1b[90m(expires in 15 minutes)\x1b[0m\n   \x1b[94mAB12-CD34E\x1b[0m\n\n\x1b[90mContinue only if you started this login in Codex. If a website or another person gave you this code, cancel.\x1b[0m\n";
        assert_eq!(
            parse_challenge(text.as_bytes()).as_deref(),
            Some("AB12-CD34E")
        );
    }

    #[test]
    fn only_one_login_may_write_a_codex_home() {
        let home = tempfile::tempdir().unwrap();
        let first = login_lock(home.path()).unwrap();
        assert!(matches!(login_lock(home.path()), Err("codex_auth_busy")));
        drop(first);
        assert!(login_lock(home.path()).is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_reaps_forwarded_children_and_fences_stubborn_descendants() {
        use crate::types::{AgentSessionInputReservationV1, AgentTurnTaskV1, InputOrigin};
        use std::os::unix::fs::PermissionsExt;
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

        struct NativeChild(Option<i32>);
        impl Drop for NativeChild {
            fn drop(&mut self) {
                // Only a pid written by this test's freshly spawned fixture.
                if let Some(pid) = self.0 {
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
            }
        }

        for stubborn in [false, true] {
            let api = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "proceed": true
                })))
                .mount(&api)
                .await;
            let root = tempfile::tempdir().unwrap();
            let program = root.path().join("codex");
            std::fs::write(
                &program,
                r#"#!/bin/sh
if [ "$2" = status ]; then echo 'Not logged in' >&2; exit 1; fi
# Model the pinned npm launcher: inherit all three stdio descriptors, forward
# SIGTERM to its native child, and wait for that child before exiting.
# Save stdin before starting the asynchronous shell list. Linux /bin/sh (dash)
# otherwise replaces fd 0 with /dev/null before applying a redundant <&0.
exec 3<&0
/bin/sh -c '
  if [ "$STUBBORN" = yes ]; then trap "" TERM; fi
  echo $$ > "$CODEX_HOME/native-pid"
  printf "https://auth.openai.com/codex/device\nTEST-12345\n"
  while :; do :; done
' <&3 3<&- &
native=$!
exec 3<&-
trap 'kill -TERM "$native"; wait "$native"; exit 0' TERM
wait "$native"
"#,
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            let task = AgentTurnTaskV1 {
                task_id: uuid::Uuid::new_v4().to_string(),
                attempt: 1,
                session_id: uuid::Uuid::new_v4().to_string(),
                input_origin: InputOrigin::Human,
                initial_input: AgentSessionInputReservationV1 {
                    reservation_id: uuid::Uuid::new_v4().to_string(),
                    input_id: uuid::Uuid::new_v4().to_string(),
                    revision: 1,
                    delivery_generation: 1,
                    input_origin: InputOrigin::Human,
                    system_notice_ids: vec![],
                },
            };
            let client = InternalApiClient::new(&api.uri());
            let gate = Gate {
                task: &task,
                client: &client,
            };
            let home = root.path().join("fresh-home");
            let env = BTreeMap::from([
                ("CODEX_HOME".into(), home.display().to_string()),
                (
                    "STUBBORN".into(),
                    if stubborn { "yes" } else { "no" }.into(),
                ),
            ]);
            let mut held_lock = None;
            let result = timeout(
                Duration::from_secs(5),
                authorize_inner(
                    &gate,
                    &program,
                    root.path(),
                    &env,
                    Duration::from_millis(80),
                    &mut held_lock,
                ),
            )
            .await
            .unwrap();
            assert_eq!(result, Err("codex_auth_timeout"));
            let native_pid = std::fs::read_to_string(home.join("native-pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let mut native = NativeChild(Some(native_pid));
            // The caller retains its own lock through the terminal HTTP ack.
            assert!(matches!(login_lock(&home), Err("codex_auth_busy")));
            drop(held_lock);
            if stubborn {
                assert_eq!(unsafe { libc::kill(native_pid, 0) }, 0);
                // Even after launcher SIGKILL and the caller dropping its
                // copy, the native process's stdin prevents a second login.
                assert!(matches!(login_lock(&home), Err("codex_auth_busy")));
                assert_eq!(unsafe { libc::kill(native_pid, libc::SIGKILL) }, 0);
            } else {
                assert_ne!(
                    unsafe { libc::kill(native_pid, 0) },
                    0,
                    "forwarded child was not reaped"
                );
            }
            native.0 = None;
            timeout(Duration::from_secs(2), async {
                while login_lock(&home).is_err() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("dead login retained its lock");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cli_failures_and_timeout_end_the_wait_without_becoming_ready() {
        use crate::types::{AgentSessionInputReservationV1, AgentTurnTaskV1, InputOrigin};
        use std::os::unix::fs::PermissionsExt;
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        for (mode, expected) in [
            ("wait", "codex_auth_timeout"),
            ("disabled", "codex_auth_disabled"),
            ("malformed", "codex_auth_failed"),
            ("late-success", "codex_auth_timeout"),
            ("api-rejected", "codex_auth_failed"),
        ] {
            let api = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(if mode == "api-rejected" {
                    ResponseTemplate::new(401)
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "proceed": mode != "late-success", "code":"codex_auth_timeout"
                    }))
                })
                .mount(&api)
                .await;
            let root = tempfile::tempdir().unwrap();
            let program = root.path().join("codex");
            std::fs::write(
                &program,
                r#"#!/bin/sh
if [ "$2" = status ]; then
  if [ -f "$CODEX_HOME/auth.json" ]; then echo 'Logged in using ChatGPT'; exit 0; fi
  echo 'Not logged in' >&2; exit 1
fi
if [ "$MODE" = disabled ]; then echo 'Device authorization is disabled' >&2; exit 1; fi
if [ "$MODE" = malformed ]; then echo 'private invalid output'; exit 1; fi
if [ "$MODE" = late-success ]; then echo fake > "$CODEX_HOME/auth.json"; exit 0; fi
printf 'https://auth.openai.com/codex/device\nTEST-12345\n'
while :; do :; done
"#,
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            let task = AgentTurnTaskV1 {
                task_id: uuid::Uuid::new_v4().to_string(),
                attempt: 1,
                session_id: uuid::Uuid::new_v4().to_string(),
                input_origin: InputOrigin::Human,
                initial_input: AgentSessionInputReservationV1 {
                    reservation_id: uuid::Uuid::new_v4().to_string(),
                    input_id: uuid::Uuid::new_v4().to_string(),
                    revision: 1,
                    delivery_generation: 1,
                    input_origin: InputOrigin::Human,
                    system_notice_ids: vec![],
                },
            };
            let client = InternalApiClient::new(&api.uri());
            let gate = Gate {
                task: &task,
                client: &client,
            };
            let env = BTreeMap::from([
                ("CODEX_HOME".into(), root.path().display().to_string()),
                ("MODE".into(), mode.into()),
            ]);
            let mut held_lock = None;
            let result = timeout(
                Duration::from_secs(3),
                authorize_inner(
                    &gate,
                    &program,
                    root.path(),
                    &env,
                    Duration::from_millis(80),
                    &mut held_lock,
                ),
            )
            .await
            .unwrap();
            assert_eq!(result, Err(expected), "{mode}");
            let requests = api.received_requests().await.unwrap();
            if mode != "late-success" {
                assert!(!requests
                    .iter()
                    .any(
                        |r| serde_json::from_slice::<serde_json::Value>(&r.body).unwrap()["phase"]
                            == "ready"
                    ));
            }
            drop(held_lock);
            // Concurrent tests can briefly inherit the descriptor between fork
            // and exec (CLOEXEC closes it at exec). Require eventual release,
            // as the descendant-cleanup test above does, without an unlock
            // that could admit another live credential writer.
            timeout(Duration::from_secs(2), async {
                loop {
                    if login_lock(root.path()).is_ok() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap_or_else(|_| panic!("lock must be released after {mode}"));
        }
    }
}
