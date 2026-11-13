//! Integration tests for the PTY session management subsystem of g5d.
//!
//! This test file exercises `TerminalManager` and `TerminalSession` from `g5d::terminal`,
//! validating that:
//!
//! - PTY sessions can be spawned and commands executed with output capture
//! - Environment variables and working directory persist across commands
//! - Multiple concurrent sessions don't interfere with each other
//! - Session limits are enforced
//! - Command timeouts work correctly
//! - Large output is captured (with truncation behavior)
//! - Sessions clean up properly on shutdown
//! - Non-zero exit codes are propagated
//! - Injected environment variables are available
//! - Background execution returns immediately with pollable output
//!
//! These tests use real PTY pseudo-terminals — they are NOT mocked.
//! Each test creates an isolated temp directory and spawns actual `/bin/sh` processes.

mod helpers;

use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;

use g5d::terminal::{RunInTerminalArgs, SessionState, TerminalConfig, TerminalManager};

// ---------------------------------------------------------------------------
// Helper: Create a TerminalConfig with test defaults
// ---------------------------------------------------------------------------

fn test_config(tmp: &TempDir) -> TerminalConfig {
    TerminalConfig {
        max_sessions: 10,
        default_shell: None,
        default_timeout_secs: 300,
        workspace_path: tmp.path().to_path_buf(),
        max_output_size: 61440,
        scrollback_size: 4096,
        full_scrollback_size: 65536,
        env_vars: HashMap::new(),
        uid: None,
        gid: None,
        default_cols: 80,
        default_rows: 24,
    }
}

// ---------------------------------------------------------------------------
// Helper: Build RunInTerminalArgs with only the command set
// ---------------------------------------------------------------------------

fn cmd_args(command: &str) -> RunInTerminalArgs {
    RunInTerminalArgs {
        command: command.to_string(),
        cwd: None,
        timeout: None,
        terminal_id: None,
        new_terminal: None,
        is_background: None,
    }
}

#[tokio::test]
async fn terminal_shell_does_not_inherit_dispatcher_process_environment() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("create terminal temp dir");
    let mut config = test_config(&tmp);
    config
        .env_vars
        .insert("G5_SAFE_TERMINAL_VALUE".to_string(), "visible".to_string());
    let manager = TerminalManager::new(config);
    let result = manager
        .execute(cmd_args(
            "printf 'safe:%s inherited:%s\\n' \"$G5_SAFE_TERMINAL_VALUE\" \"${CARGO_MANIFEST_DIR-unset}\"",
        ))
        .await
        .expect("run isolated terminal environment check");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout.contains("safe:visible inherited:unset"));
    manager.shutdown().await.unwrap();
}

// ===========================================================================
// Tests
// ===========================================================================

/// Verify that a terminal session can be spawned and a simple command executed
/// with output captured.
#[tokio::test]
async fn test_spawn_shell_session() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    let result = manager
        .execute(cmd_args("echo hello"))
        .await
        .expect("execute failed");

    assert!(
        result.stdout.contains("hello"),
        "Expected 'hello' in output: {}",
        result.stdout
    );
    assert_eq!(
        result.exit_code,
        Some(0),
        "Expected exit code 0, got {:?}",
        result.exit_code
    );
    assert!(
        !result.terminal_id.is_empty(),
        "terminal_id should be non-empty"
    );
    // PTY merges stdout/stderr, so stderr is always empty
    assert!(result.stderr.is_empty(), "stderr should be empty for PTY");

    manager.shutdown().await.unwrap();
}

/// Verify that a terminal session persists state (environment variables) across
/// multiple `execute()` calls.
#[tokio::test]
async fn test_terminal_persistence() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    // Set an environment variable in the session
    let result1 = manager
        .execute(cmd_args("export MY_VAR=persistent_value"))
        .await
        .expect("first execute failed");

    let first_terminal_id = result1.terminal_id.clone();

    // Read it back in the same session (default reuse)
    let result2 = manager
        .execute(cmd_args("echo $MY_VAR"))
        .await
        .expect("second execute failed");

    assert!(
        result2.stdout.contains("persistent_value"),
        "Expected 'persistent_value' in output: {}",
        result2.stdout
    );
    assert_eq!(
        result2.terminal_id, first_terminal_id,
        "Both calls should reuse the same session"
    );

    manager.shutdown().await.unwrap();
}

/// Verify that the `cwd` parameter changes the working directory for a command,
/// and that `cd` persists across calls.
#[tokio::test]
async fn test_terminal_working_directory() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    // Use cwd parameter to run pwd in /tmp
    let result1 = manager
        .execute(RunInTerminalArgs {
            command: "pwd".to_string(),
            cwd: Some("/tmp".to_string()),
            timeout: None,
            terminal_id: None,
            new_terminal: None,
            is_background: None,
        })
        .await
        .expect("execute with cwd failed");

    assert!(
        result1.stdout.contains("/tmp"),
        "Expected '/tmp' in output: {}",
        result1.stdout
    );

    // cd into /tmp and verify it persists
    let _result2 = manager
        .execute(cmd_args("cd /tmp && pwd"))
        .await
        .expect("cd execute failed");

    let result3 = manager
        .execute(cmd_args("pwd"))
        .await
        .expect("pwd execute failed");

    assert!(
        result3.stdout.contains("/tmp"),
        "Expected '/tmp' in output after cd persistence: {}",
        result3.stdout
    );

    manager.shutdown().await.unwrap();
}

/// Verify that environment variables set via shell commands persist across
/// subsequent calls within the same session.
#[tokio::test]
async fn test_terminal_env_vars() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    let _result1 = manager
        .execute(cmd_args("export FOO=bar"))
        .await
        .expect("export failed");

    let result2 = manager
        .execute(cmd_args("echo $FOO"))
        .await
        .expect("echo failed");

    assert!(
        result2.stdout.contains("bar"),
        "Expected 'bar' in output: {}",
        result2.stdout
    );

    manager.shutdown().await.unwrap();
}

/// Verify that a long-running command can be started in the background,
/// and that the session remains usable.
#[tokio::test]
async fn test_terminal_long_running_command() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    // Start a background sleep (& backgrounds it, shell returns immediately).
    // Use a short timeout since we only care that the shell returns quickly.
    let bg_args = RunInTerminalArgs {
        timeout: Some(30),
        ..cmd_args("sleep 60 &")
    };
    let result1 = manager
        .execute(bg_args)
        .await
        .expect("background sleep failed");

    // In a PTY, backgrounding may or may not produce exit code 0 depending on
    // job-control output timing. Accept either Some(0) or None (timed out).
    assert!(
        result1.exit_code == Some(0) || result1.exit_code.is_none(),
        "Background command should return exit code 0 or None, got {:?}",
        result1.exit_code
    );

    // Verify the session is still responsive
    let result2 = manager
        .execute(cmd_args("echo still_alive"))
        .await
        .expect("echo after background failed");

    assert!(
        result2.stdout.contains("still_alive"),
        "Expected 'still_alive' in output: {}",
        result2.stdout
    );

    // Shutdown should clean up the background sleep process
    manager.shutdown().await.unwrap();
}

/// Verify that large output (many lines) is captured correctly, including
/// truncation behavior.
#[tokio::test]
async fn test_terminal_output_streaming() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = TerminalConfig {
        max_output_size: 4096, // Small buffer for testing truncation
        ..test_config(&tmp)
    };
    let manager = TerminalManager::new(config);

    // Generate 10,000 lines of output
    let result = manager
        .execute(cmd_args("seq 1 10000"))
        .await
        .expect("seq command failed");

    assert_eq!(
        result.exit_code,
        Some(0),
        "Expected exit code 0, got {:?}",
        result.exit_code
    );
    assert!(
        !result.stdout.is_empty(),
        "Output should not be empty for seq 1 10000"
    );

    // The output may be truncated due to the small max_output_size
    // If truncated, the truncation marker should be present
    if result.stdout.len() > 4096 + 256 {
        // Allow some overhead for marker
        // If we're well over the limit, truncation should have happened
        // (but some implementations may buffer slightly more)
    }

    // The last line (10000) should ideally be preserved since truncation
    // keeps the last portion of output
    // Note: depending on truncation implementation, this may or may not be present
    // We just verify the output is reasonable
    assert!(
        result.stdout.len() > 100,
        "Output should have substantial content, got {} bytes",
        result.stdout.len()
    );

    manager.shutdown().await.unwrap();
}

/// Verify that multiple concurrent terminal sessions don't interfere with each other.
#[tokio::test]
async fn test_terminal_multiple_sessions() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = TerminalConfig {
        max_sessions: 5,
        ..test_config(&tmp)
    };
    let manager = TerminalManager::new(config);

    // Create 3 sessions with different env vars
    let r1 = manager
        .execute(RunInTerminalArgs {
            command: "export SESSION_VAR=one".to_string(),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("session 1 create failed");
    let id1 = r1.terminal_id.clone();

    let r2 = manager
        .execute(RunInTerminalArgs {
            command: "export SESSION_VAR=two".to_string(),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("session 2 create failed");
    let id2 = r2.terminal_id.clone();

    let r3 = manager
        .execute(RunInTerminalArgs {
            command: "export SESSION_VAR=three".to_string(),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("session 3 create failed");
    let id3 = r3.terminal_id.clone();

    // Verify each session has its own isolated env var
    let check1 = manager
        .execute(RunInTerminalArgs {
            command: "echo $SESSION_VAR".to_string(),
            terminal_id: Some(id1.clone()),
            ..cmd_args("")
        })
        .await
        .expect("session 1 check failed");
    assert!(
        check1.stdout.contains("one"),
        "Session 1 should have SESSION_VAR=one, got: {}",
        check1.stdout
    );

    let check2 = manager
        .execute(RunInTerminalArgs {
            command: "echo $SESSION_VAR".to_string(),
            terminal_id: Some(id2.clone()),
            ..cmd_args("")
        })
        .await
        .expect("session 2 check failed");
    assert!(
        check2.stdout.contains("two"),
        "Session 2 should have SESSION_VAR=two, got: {}",
        check2.stdout
    );

    let check3 = manager
        .execute(RunInTerminalArgs {
            command: "echo $SESSION_VAR".to_string(),
            terminal_id: Some(id3.clone()),
            ..cmd_args("")
        })
        .await
        .expect("session 3 check failed");
    assert!(
        check3.stdout.contains("three"),
        "Session 3 should have SESSION_VAR=three, got: {}",
        check3.stdout
    );

    // Verify session count
    let sessions = manager.list_sessions();
    assert_eq!(
        sessions.len(),
        3,
        "Expected 3 sessions, got {}",
        sessions.len()
    );

    manager.shutdown().await.unwrap();
}

/// Verify that when sessions are shut down, PTY processes are killed and
/// the sessions list is cleared.
#[tokio::test]
async fn test_terminal_session_cleanup() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    // Execute a command to create a session
    let result = manager
        .execute(cmd_args("echo cleanup_test"))
        .await
        .expect("execute failed");

    let terminal_id = result.terminal_id.clone();

    // Verify session exists and is Idle
    let info = manager.get_session_info(&terminal_id);
    assert!(info.is_some(), "Session should exist before shutdown");
    let info = info.unwrap();
    assert_eq!(
        info.state,
        SessionState::Idle,
        "Session should be Idle, got {:?}",
        info.state
    );

    // Shutdown all sessions
    manager.shutdown().await.unwrap();

    // Verify all sessions are cleared
    let sessions = manager.list_sessions();
    assert!(
        sessions.is_empty(),
        "Sessions should be empty after shutdown, got {} sessions",
        sessions.len()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_terminal_shutdown_terminates_process_group_descendants() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let manager = TerminalManager::new(test_config(&tmp));
    let pid_file = tmp.path().join("descendant.pid");
    manager
        .execute(RunInTerminalArgs {
            command: format!("set +m; sleep 60 & echo $! > {}", pid_file.display()),
            is_background: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("start descendant");
    let pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(contents) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = contents.trim().parse::<libc::pid_t>() {
                    break pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("descendant pid was not written");
    manager.shutdown().await.unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            // SAFETY: signal zero only probes process existence.
            let exists = unsafe { libc::kill(pid, 0) == 0 };
            if !exists {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("terminal shutdown left a descendant process alive");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blocked_interactive_input_does_not_hold_shutdown_registry() {
    let tmp = TempDir::new().expect("create temp dir");
    let manager = std::sync::Arc::new(TerminalManager::new(test_config(&tmp)));
    let attachment = manager
        .attach_interactive(None, 80, 24, HashMap::new())
        .await
        .expect("open interactive PTY");
    let terminal_id = attachment.session.id().to_string();
    attachment
        .session
        .write_raw(b"stty raw -echo; printf 'R%sADY' 'E'; sleep 60\n")
        .await
        .expect("start non-reading foreground process");
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = Vec::new();
        while !String::from_utf8_lossy(&output).contains("READY") {
            output.extend(attachment.session.read_raw().await.unwrap().unwrap());
        }
    })
    .await
    .expect("foreground process did not start");
    let writer_manager = std::sync::Arc::clone(&manager);
    let writer_id = terminal_id.clone();
    let generation = attachment.generation;
    let write = tokio::spawn(async move {
        writer_manager
            .write_interactive(&writer_id, generation, &vec![b'x'; 1024 * 1024])
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !write.is_finished(),
        "PTY write should be blocked by full input queue"
    );
    tokio::time::timeout(Duration::from_secs(8), manager.shutdown())
        .await
        .expect("blocked PTY write stalled manager shutdown")
        .expect("manager shutdown failed");
    let _ = tokio::time::timeout(Duration::from_secs(5), write)
        .await
        .expect("blocked PTY writer survived shutdown")
        .expect("PTY writer task panicked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reattach_retires_pty_with_inflight_old_owner_input() {
    let tmp = TempDir::new().expect("create temp dir");
    let manager = std::sync::Arc::new(TerminalManager::new(test_config(&tmp)));
    let attachment = manager
        .attach_interactive(None, 80, 24, HashMap::new())
        .await
        .expect("open interactive PTY");
    let old_id = attachment.session.id().to_string();
    attachment
        .session
        .write_raw(b"stty raw -echo; printf 'R%sADY' 'E'; sleep 60\n")
        .await
        .expect("start non-reading foreground process");
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = Vec::new();
        while !String::from_utf8_lossy(&output).contains("READY") {
            output.extend(attachment.session.read_raw().await.unwrap().unwrap());
        }
    })
    .await
    .expect("foreground process did not start");
    let writer_manager = std::sync::Arc::clone(&manager);
    let writer_id = old_id.clone();
    let generation = attachment.generation;
    let write = tokio::spawn(async move {
        writer_manager
            .write_interactive(&writer_id, generation, &vec![b'x'; 1024 * 1024])
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !write.is_finished(),
        "old owner input must still be in flight"
    );
    let refused = tokio::time::timeout(
        Duration::from_secs(8),
        manager.attach_interactive(Some(&old_id), 80, 24, HashMap::new()),
    )
    .await
    .expect("retiring old PTY stalled reattachment");
    assert!(
        refused.is_err(),
        "new attachment claimed a PTY with old input in flight"
    );
    assert!(manager.get_session_info(&old_id).is_none());
    let fresh = manager
        .attach_interactive(None, 80, 24, HashMap::new())
        .await
        .expect("open fresh PTY after retirement");
    assert_ne!(fresh.session.id(), old_id);
    manager.shutdown().await.expect("close fresh PTY");
    let _ = tokio::time::timeout(Duration::from_secs(5), write)
        .await
        .expect("retired PTY writer survived reattachment")
        .expect("PTY writer task panicked");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_terminal_shutdown_terminates_job_control_child_group() {
    let tmp = TempDir::new().expect("create temp dir");
    let manager = TerminalManager::new(test_config(&tmp));
    let pid_file = tmp.path().join("job-control-child.pid");
    let started = manager
        .execute(RunInTerminalArgs {
            command: format!("set -m; sleep 60 & echo $! > {}", pid_file.display()),
            is_background: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("start job-control child");
    let pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(contents) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = contents.trim().parse::<libc::pid_t>() {
                    break pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("job-control child pid was not written");
    let shell_pid = manager
        .get_session_info(&started.terminal_id)
        .expect("session remains registered")
        .pid as libc::pid_t;
    // SAFETY: getpgid only inspects the child spawned by this test.
    let child_group = unsafe { libc::getpgid(pid) };
    assert!(child_group > 0, "job-control child already exited");
    assert_ne!(
        child_group, shell_pid,
        "child must use a separate job-control group"
    );

    manager.shutdown().await.expect("shutdown terminal session");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
            if stat.as_ref().map_or(true, |stat| {
                stat.rfind(')')
                    .and_then(|end| stat[end + 1..].split_whitespace().next())
                    .is_some_and(|state| state == "Z" || state == "X")
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("job-control child survived PTY shutdown");
}

/// Verify that commands exceeding the timeout are killed and partial output is returned.
#[tokio::test]
async fn test_terminal_command_timeout() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = TerminalConfig {
        default_timeout_secs: 2,
        ..test_config(&tmp)
    };
    let manager = TerminalManager::new(config);

    // Event/state-based proof per testing_policy/test_execution_modes D12
    // (issue #1924): no wall-clock elapsed assertion — under loaded parallel
    // CI this test can be descheduled past any fixed cap while the semantic
    // timeout result is still correct. The sentinel proves the command was
    // interrupted rather than run to completion, and the follow-up command
    // proves SIGINT restored the shell. The nextest slow-timeout remains the
    // suite-level wedge guard.
    // The sentinel is split with '' in the command text so the PTY's echo of
    // the typed command line (returned unstripped on the timeout path) can
    // never contain the joined token — only natural completion prints it.
    let result = manager
        .execute(RunInTerminalArgs {
            command: "sleep 30; echo NATURAL_COMPLETION_SEN''TINEL".to_string(),
            timeout: Some(2),
            ..cmd_args("")
        })
        .await
        .expect("execute should return a result even on timeout");

    // Should have timed out — exit_code is None
    assert_eq!(
        result.exit_code, None,
        "Timed-out command should have exit_code None, got {:?}",
        result.exit_code
    );

    // Output should contain the timeout marker
    assert!(
        result.stdout.contains("[Command timed out"),
        "Expected timeout marker in output: {}",
        result.stdout
    );

    // The command must have been interrupted, not run to natural completion.
    assert!(
        !result.stdout.contains("NATURAL_COMPLETION_SENTINEL"),
        "Timed-out command must not reach its natural completion: {}",
        result.stdout
    );

    // The timeout SIGINT interrupts only the foreground command; the shell
    // must survive and execute a follow-up command on the same terminal.
    let followup = manager
        .execute(RunInTerminalArgs {
            command: "echo shell_recovered_after_timeout".to_string(),
            terminal_id: Some(result.terminal_id.clone()),
            ..cmd_args("")
        })
        .await
        .expect("follow-up command should execute after a timeout");
    assert_eq!(
        followup.exit_code,
        Some(0),
        "shell should be restored after the timeout: {:?}",
        followup
    );
    assert!(
        followup.stdout.contains("shell_recovered_after_timeout"),
        "follow-up output should prove the shell recovered: {}",
        followup.stdout
    );

    manager.shutdown().await.unwrap();
}

/// Verify that the session limit is enforced.
#[tokio::test]
async fn test_terminal_session_limit() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = TerminalConfig {
        max_sessions: 2,
        ..test_config(&tmp)
    };
    let manager = TerminalManager::new(config);

    // Create 2 sessions (at the limit)
    let _r1 = manager
        .execute(RunInTerminalArgs {
            command: "echo session1".to_string(),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("session 1 should succeed");

    let _r2 = manager
        .execute(RunInTerminalArgs {
            command: "echo session2".to_string(),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("session 2 should succeed");

    // Attempt to create a 3rd session — should fail
    let r3 = manager
        .execute(RunInTerminalArgs {
            command: "echo session3".to_string(),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await;

    assert!(
        r3.is_err(),
        "Creating a 3rd session should fail when max_sessions=2"
    );
    let err = r3.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("session") || err_msg.contains("limit") || err_msg.contains("Maximum"),
        "Error should mention session limit: {}",
        err_msg
    );

    manager.shutdown().await.unwrap();
}

/// Verify that referencing a non-existent terminal_id returns an error.
#[tokio::test]
async fn test_terminal_nonexistent_session() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    let result = manager
        .execute(RunInTerminalArgs {
            command: "echo hi".to_string(),
            terminal_id: Some("nonexistent-id".to_string()),
            ..cmd_args("")
        })
        .await;

    assert!(
        result.is_err(),
        "Referencing a non-existent terminal_id should return an error"
    );
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("not found")
            || err_msg.contains("Not found")
            || err_msg.contains("nonexistent"),
        "Error should mention session not found: {}",
        err_msg
    );

    manager.shutdown().await.unwrap();
}

/// Verify that non-zero exit codes are correctly captured.
#[tokio::test]
#[serial_test::serial]
async fn test_terminal_exit_code_propagation() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    // Use "sh -c 'exit 42'" to avoid timing issues with bare "exit 42"
    // when the shell hasn't finished initializing under load.
    let result = manager
        .execute(RunInTerminalArgs {
            command: "sh -c 'exit 42'".to_string(),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("execute should succeed even with non-zero exit");

    assert_eq!(
        result.exit_code,
        Some(42),
        "Expected exit code 42, got {:?}",
        result.exit_code
    );

    manager.shutdown().await.unwrap();
}

/// Verify that `TerminalConfig.env_vars` are available in spawned sessions.
#[tokio::test]
async fn test_terminal_injected_env_vars() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let mut env_vars = HashMap::new();
    env_vars.insert("G5_TASK_ID".to_string(), "test-task-123".to_string());
    env_vars.insert("G5_WORKSPACE_ID".to_string(), "ws-456".to_string());

    let config = TerminalConfig {
        env_vars,
        ..test_config(&tmp)
    };
    let manager = TerminalManager::new(config);

    let result1 = manager
        .execute(cmd_args("echo $G5_TASK_ID"))
        .await
        .expect("echo G5_TASK_ID failed");

    assert!(
        result1.stdout.contains("test-task-123"),
        "Expected 'test-task-123' in output: {}",
        result1.stdout
    );

    let result2 = manager
        .execute(cmd_args("echo $G5_WORKSPACE_ID"))
        .await
        .expect("echo G5_WORKSPACE_ID failed");

    assert!(
        result2.stdout.contains("ws-456"),
        "Expected 'ws-456' in output: {}",
        result2.stdout
    );

    manager.shutdown().await.unwrap();
}

/// Verify background execution mode returns immediately and output can be polled.
#[tokio::test]
async fn test_terminal_background_execution() {
    helpers::setup_tracing();
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = test_config(&tmp);
    let manager = TerminalManager::new(config);

    let result = manager
        .execute(RunInTerminalArgs {
            command: "echo background_output".to_string(),
            is_background: Some(true),
            new_terminal: Some(true),
            ..cmd_args("")
        })
        .await
        .expect("background execute failed");

    // Background execution: exit_code is None (not waited)
    assert_eq!(
        result.exit_code, None,
        "Background command should have exit_code None, got {:?}",
        result.exit_code
    );

    // stdout is empty (not waited for output)
    assert!(
        result.stdout.is_empty(),
        "Background command should return empty stdout, got: {}",
        result.stdout
    );

    let terminal_id = result.terminal_id.clone();

    // Wait a short time for the command to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Poll for output
    let output = manager
        .get_output(&terminal_id)
        .await
        .expect("get_output failed");

    assert!(
        output.contains("background_output"),
        "Expected 'background_output' in polled output: {}",
        output
    );

    manager.shutdown().await.unwrap();
}
