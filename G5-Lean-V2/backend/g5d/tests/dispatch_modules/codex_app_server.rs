//! Actual dispatcher + production supervisor with an independently scripted peer.
use super::*;

fn provider(root: &Path, case: &str) -> std::path::PathBuf {
    executable_script(root, &format!("{case}.py"), &peer_script(case))
}

pub(super) fn peer_script(case: &str) -> String {
    let disabled: Vec<&str> =
        include_str!("../../../g5d-agent/src/runner/codex_disabled_features.txt")
            .lines()
            .collect();
    include_str!("../../../../tests/fixtures/codex-app-server/peer.py")
        .replace(r#"CASE = "normal""#, &format!("CASE = {case:?}"))
        .replace(
            r#"CODEX_VERSION = "injected-by-dispatcher-test""#,
            &format!(
                "CODEX_VERSION = {:?}",
                g5_contracts::codex_models::CODEX_CLI_VERSION.trim()
            ),
        )
        .replace(
            "EXPECTED_DISABLED = []",
            &format!("EXPECTED_DISABLED = {}", json!(disabled)),
        )
}

async fn dispatcher() -> TestDispatcher {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    d
}

fn invoke(d: &TestDispatcher, task: Uuid, script: &Path) -> Value {
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20000,"max_output_bytes":1048576}});
    let mut p = agent_turn_invoke(
        d,
        task,
        1,
        "codex",
        "human",
        "frozen initial input",
        policy,
        "G5_CODEX_PATH",
        script,
    );
    p["version"] = json!("1.0.8");
    p["input"]["provider"]["options"]["codex_transport"] = json!("app_server_stdio_v1");
    p["input"]["provider"]["options"]["effort"] = json!("high");
    p["input"]["recovery"] = json!({"included_messages":1,"omitted_messages":0,"truncated_messages":0,"attachment_references":0,"omitted_attachment_references":0});
    p
}

async fn trace_contains(script: &Path, phase: &str) {
    timeout(Duration::from_secs(10), async {
        loop {
            if std::fs::read_to_string(script.with_extension("trace"))
                .unwrap_or_default()
                .lines()
                .any(|line| line == phase)
            {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing {phase} for {}", script.display()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_transport_contract_is_refused_before_provider_launch() {
    let scripts = tempfile::tempdir().unwrap();
    let script = provider(scripts.path(), "normal");
    let d = dispatcher().await;
    let mut rpc = d.connect().await;
    for (version, transport, provider_id) in [
        ("1.0.5", Some(json!("app_server_stdio_v1")), "codex"),
        ("1.0.6", Some(json!("app_server_stdio_v1")), "codex"),
        ("1.0.7", Some(json!("app_server_stdio_v1")), "codex"),
        ("1.0.8", None, "codex"),
        ("1.0.8", Some(json!("exec")), "codex"),
        ("1.0.8", Some(Value::Null), "codex"),
        ("1.0.8", Some(json!("app_server_stdio_v1")), "claude_code"),
        // Exec-contract Codex attempts: this guest has no exec launcher.
        ("1.0.5", None, "codex"),
        ("1.0.6", None, "codex"),
        ("1.0.7", None, "codex"),
        ("1.0.9", None, "codex"),
        ("1.0.10", None, "codex"),
        ("1.0.10", Some(json!("app_server_stdio_v1")), "claude_code"),
        ("1.0.11", None, "codex"),
        ("1.0.11", Some(json!("app_server_stdio_v1")), "codex"),
        ("1.0.12", None, "codex"),
        ("1.0.12", Some(json!("app_server_stdio_v1")), "claude_code"),
        ("1.0.13", None, "codex"),
        ("1.0.13", Some(json!("app_server_stdio_v1")), "codex"),
        ("1.0.14", None, "codex"),
        ("1.0.14", Some(json!("app_server_stdio_v1")), "claude_code"),
    ] {
        let mut params = invoke(&d, Uuid::new_v4(), &script);
        params["version"] = json!(version);
        params["input"]["provider"]["provider_id"] = json!(provider_id);
        let options = params["input"]["provider"]["options"]
            .as_object_mut()
            .unwrap();
        if let Some(value) = transport {
            options.insert("codex_transport".into(), value);
        } else {
            options.remove("codex_transport");
        }
        let response = rpc.call("modules.invoke", params).await;
        error(response, "CODEX_TRANSPORT_CONTRACT_MISMATCH");
        assert!(
            !script.with_extension("pid").exists(),
            "a refused contract launched the provider"
        );
    }
}

/// A line past the 16 MiB ceiling is dropped with a notice and proves
/// nothing: an oversized acceptance leaves the input unaccepted until the
/// setup deadline ends the attempt with its delivery unknown, and an
/// oversized completion leaves the accepted turn running until the owner
/// stops it. Neither ends on an output limit (`g5.agents.output-bounds`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_oversized_acceptance_and_completion_never_count_as_native_evidence() {
    let scripts = tempfile::tempdir().unwrap();
    let d = dispatcher().await;
    let mut rpc = d.connect().await;
    for (case, accepted) in [("oversized_ack", false), ("oversized_completed", true)] {
        let script = provider(scripts.path(), case);
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = invoke(&d, task, &script);
        params["input"]["limits"]["setup_timeout_ms"] = json!(5_000);
        result(rpc.call("modules.invoke", params).await);
        timeout(
            Duration::from_secs(120),
            rpc.event_where("module.event", |params| {
                params["exec_id"] == exec.as_str()
                    && params["kind"] == "module.notice"
                    && params["payload"]["code"] == "provider_line_skipped"
            }),
        )
        .await
        .unwrap_or_else(|_| panic!("{case}: the oversized line is noted"));
        if accepted {
            // Past the setup deadline the accepted turn is still running:
            // the dropped completion did not finish it and nothing limits it.
            sleep(Duration::from_millis(5_400)).await;
            assert!(
                events_of(
                    &replay_events(&mut rpc, &d, &exec).await,
                    "invocation.outcome"
                )
                .is_empty(),
                "{case}: only the owner ends the turn"
            );
            result(
                rpc.call(
                    "modules.cancel",
                    d.params(true, json!({"exec_id":exec,"reason":"owner_stop"})),
                )
                .await,
            );
        }
        let outcome = timeout(Duration::from_secs(120), rpc.wait_outcome(&exec))
            .await
            .unwrap();
        if accepted {
            assert_eq!(outcome["status"], "cancelled", "{case}: {outcome}");
        } else {
            assert_eq!(
                outcome["result"]["status"], "timed_out",
                "{case}: {outcome}"
            );
            assert_eq!(
                outcome["result"]["input_delivery_unknown"], true,
                "{case}: {outcome}"
            );
        }
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert_eq!(
            events_of(&events, "module.input.accepted").len(),
            usize::from(accepted)
        );
        assert!(
            events_of(&events, "module.notice")
                .iter()
                .any(|event| event["payload"]["code"] == "provider_line_skipped"),
            "{events:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_saves_and_resumes_native_tool_history_in_same_and_replacement_homes() {
    let d = dispatcher().await;
    let mut rpc = d.connect().await;
    let fake = bin().parent().unwrap().join("fake-codex");
    let home = d.root.path().join("home/agent");
    let replacement = d.root.path().join("replacement-home");
    let session = Uuid::new_v4();
    let mut native_id = None::<String>;
    let mut fact = None::<String>;
    for (index, current_home) in [&home, &home, &replacement].into_iter().enumerate() {
        if index == 2 {
            // Transfer exactly the saved rollout, as the session store does
            // on replacement. No fake receipt or G5 transcript supplies recall.
            let publication = d
                .origin
                .as_ref()
                .unwrap()
                .published_session_file_bundle()
                .unwrap();
            for file in publication["files"].as_array().unwrap() {
                let relative = file["relativePath"].as_str().unwrap();
                let source = home.join(".codex").join(relative);
                let target = replacement.join(".codex").join(relative);
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::copy(&source, &target).unwrap();
                assert_eq!(
                    format!("{:x}", Sha256::digest(std::fs::read(target).unwrap())),
                    file["sha256"]
                );
            }
        }
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = invoke(&d, task, &fake);
        params["input"].as_object_mut().unwrap().remove("recovery");
        let prompt = if index == 0 {
            "G5_TOOL_ONLY_FACT_E2E:record"
        } else {
            "G5_TOOL_ONLY_FACT_E2E:recall"
        };
        params["input"]["prompt"] =
            json!({"text":prompt,"sha256":format!("{:x}", Sha256::digest(prompt.as_bytes()))});
        params["input"]["task"]["session_id"] = json!(session);
        params["secrets"]["HOME"] = json!(current_home);
        params["secrets"]["G5_FAKE_RECEIPT"] = json!(current_home.join("receipt.json"));
        if let Some(id) = &native_id {
            params["input"]["session_binding"] =
                json!({"session_files_state":"present","provider_session_id":id});
            params["input"]["resume_prompt"] = params["input"]["prompt"].clone();
        }
        result(rpc.call("modules.invoke", params).await);
        let outcome = timeout(Duration::from_secs(15), rpc.wait_outcome(&exec))
            .await
            .unwrap();
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "turn {index}: {outcome}"
        );
        assert_eq!(
            outcome["result"]["session_files_state"], "present",
            "{outcome}"
        );
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert_eq!(events_of(&events, "module.input.accepted").len(), 1);
        let receipt: Value =
            serde_json::from_slice(&std::fs::read(current_home.join("receipt.json")).unwrap())
                .unwrap();
        assert_eq!(receipt["argv"][0], "app-server");
        if let Some(id) = &native_id {
            assert_eq!(outcome["result"]["provider_session_id"], *id);
            assert_eq!(receipt["stdin_messages"][2]["method"], "thread/resume");
            assert_eq!(receipt["stdin_messages"][2]["params"]["threadId"], *id);
            assert!(events_of(&events, "module.message.final")
                .iter()
                .any(|event| event["payload"]["content"]
                    .as_str()
                    .unwrap()
                    .contains(fact.as_ref().unwrap())));
        } else {
            native_id = Some(
                outcome["result"]["provider_session_id"]
                    .as_str()
                    .unwrap()
                    .into(),
            );
            fact = events_of(&events, "module.tool.completed")
                .iter()
                .find_map(|event| {
                    event["payload"]["output"]
                        .as_str()
                        .filter(|text| text.starts_with("tool-only-e2e-"))
                        .map(str::to_owned)
                });
            assert!(
                fact.is_some(),
                "a fact must exist only in native tool history: {events:?}"
            );
            assert!(!events_of(&events, "module.message.final")
                .iter()
                .any(|event| event["payload"]["content"]
                    .as_str()
                    .unwrap()
                    .contains(fact.as_ref().unwrap())));
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_completion_exit_status_output_and_acceptance_matrix() {
    let scripts = tempfile::tempdir().unwrap();
    let d = dispatcher().await;
    let mut rpc = d.connect().await;
    for (case, status, accepted, unknown) in [
        ("normal", "succeeded", true, false),
        ("tool_only", "succeeded", true, false),
        ("multi_tool", "succeeded", true, false),
        ("large_tool", "succeeded", true, false),
        ("secret", "succeeded", true, false),
        ("split_secret", "succeeded", true, false),
        ("nul", "succeeded", true, false),
        ("duplicate", "succeeded", true, false),
        ("trailing", "succeeded", true, false),
        ("nonzero", "failed", true, false),
        ("no_terminal", "failed", true, false),
        ("failed_no_terminal", "failed", true, false),
        ("failed", "failed", true, false),
        ("interrupted", "interrupted", true, false),
        ("interrupted_nonzero", "failed", true, false),
        ("initialize_refusal", "failed", false, false),
        ("thread_refusal", "failed", false, false),
        ("turn_refusal", "failed", false, false),
        ("turn_method_refusal", "failed", false, false),
        ("turn_params_refusal", "failed", false, false),
        ("turn_internal_failure", "failed", false, true),
        ("lost_ack", "failed", false, true),
        ("malformed_ack", "failed", false, true),
        ("malformed", "failed", true, false),
        ("unsupported", "failed", true, false),
        ("unsupported_string_id", "failed", true, false),
        ("unsupported_missing_id", "failed", true, false),
        ("malformed_identity", "failed", true, false),
        ("wrong_model", "failed", false, false),
        ("wrong_approval", "failed", false, false),
        ("wrong_sandbox", "failed", false, false),
    ] {
        let script = provider(scripts.path(), case);
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = invoke(&d, task, &script);
        params["secrets"]["APP_SERVER_SECRET"] = json!("app-server-synthetic-secret-809");
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{case}"
        );
        let outcome = timeout(Duration::from_secs(12), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{case} did not settle"));
        assert_eq!(outcome["result"]["status"], status, "{case}: {outcome}");
        assert_eq!(
            outcome["result"]["input_delivery_unknown"], unknown,
            "{case}: {outcome}"
        );
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert_eq!(
            events_of(&events, "module.input.accepted").len(),
            usize::from(accepted),
            "{case}: {events:?}"
        );
        let reconstruction = events.iter().position(|event| {
            event["kind"] == "module.notice"
                && event["payload"]["code"] == "conversation_reconstructed"
        });
        assert_eq!(
            reconstruction.is_some(),
            accepted,
            "no recovery delivery claim before native acceptance: {case}"
        );
        if let Some(reconstruction) = reconstruction {
            assert!(
                events
                    .iter()
                    .position(|event| event["kind"] == "module.input.accepted")
                    .unwrap()
                    < reconstruction
            );
        }
        assert_eq!(events_of(&events, "invocation.outcome").len(), 1, "{case}");
        if status == "succeeded" {
            assert_eq!(
                outcome["result"]["usage"]["input_tokens"], 10,
                "{case}: {outcome}"
            );
            assert_eq!(
                outcome["result"]["usage"]["output_tokens"], 5,
                "{case}: {outcome}"
            );
            trace_contains(&script, "eof").await;
        }
        if case == "large_tool" {
            let tools = events_of(&events, "module.tool.completed");
            let output = json!("large héllo 🦀 ".repeat(100_000));
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0]["payload"]["output_truncated"], true);
            assert_eq!(
                tools[0]["payload"]["output_ref"],
                format!("sha256:{:x}", Sha256::digest(output.to_string().as_bytes()))
            );
            assert!(tools[0]["payload"]["output"].as_str().unwrap().len() < 40_000);
        }
        if matches!(case, "secret" | "split_secret") {
            let recorded = format!("{events:?}{outcome}");
            assert!(!recorded.contains("app-server-synthetic-secret-809"));
            assert!(recorded.contains("[REDACTED]"));
            if case == "split_secret" {
                let deltas: String = events_of(&events, "module.message.delta")
                    .iter()
                    .map(|event| {
                        event["payload"]["text_delta"]
                            .as_str()
                            .expect("streamed text")
                    })
                    .collect();
                assert!(
                    !deltas.is_empty(),
                    "the split-secret scenario must stream output"
                );
                assert!(
                    !deltas.contains("app-server-synthetic-secret-809"),
                    "{deltas}"
                );
            }
        }
        if case == "nul" {
            let finals = events_of(&events, "module.message.final");
            assert_eq!(finals.len(), 1);
            assert!(finals[0]["payload"]["content"]
                .as_str()
                .unwrap()
                .contains("preserved after NUL"));
            assert!(!serde_json::to_string(&events).unwrap().contains("\\u0000"));
        }
        if case == "multi_tool" {
            assert_eq!(events_of(&events, "module.tool.completed").len(), 3);
        }
        if case == "tool_only" {
            assert!(events_of(&events, "module.message.final").is_empty());
        }
        if case == "trailing" {
            assert!(events_of(&events, "module.message.final")
                .iter()
                .any(|e| e["payload"].to_string().contains("trailing output")));
        }
        if matches!(case, "unsupported" | "unsupported_string_id") {
            trace_contains(&script, "rejected").await;
        }
        if case == "interrupted" {
            assert_eq!(outcome["result"]["code"], "provider_interrupted");
        }
        let input = events
            .iter()
            .position(|e| e["kind"] == "module.input.accepted");
        let output = events
            .iter()
            .position(|e| e["kind"] == "module.message.delta");
        if let (Some(i), Some(o)) = (input, output) {
            assert!(i < o, "acceptance precedes output: {case}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_native_completion_waits_for_process_exit_and_matching_turn() {
    for case in ["delayed_exit", "wrong_identity"] {
        let scripts = tempfile::tempdir().unwrap();
        let script = provider(scripts.path(), case);
        let d = dispatcher().await;
        let mut rpc = d.connect().await;
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        result(rpc.call("modules.invoke", invoke(&d, task, &script)).await);
        trace_contains(
            &script,
            if case == "delayed_exit" {
                "eof"
            } else {
                "wrong-completion"
            },
        )
        .await;
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert!(
            events_of(&events, "invocation.outcome").is_empty(),
            "native evidence cannot settle: {events:?}"
        );
        if case == "wrong_identity" {
            assert!(!std::fs::read_to_string(script.with_extension("trace"))
                .unwrap()
                .contains("eof"));
        }
        std::fs::write(script.with_extension("release"), b"continue").unwrap();
        let outcome = timeout(Duration::from_secs(10), rpc.wait_outcome(&exec))
            .await
            .unwrap();
        assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_setup_deadlines_cover_each_phase_and_blocked_turn_write() {
    let scripts = tempfile::tempdir().unwrap();
    let d = dispatcher().await;
    let mut rpc = d.connect().await;
    for case in [
        "initialize_silent",
        "thread_silent",
        "turn_silent",
        "blocked_turn_write",
    ] {
        let script = provider(scripts.path(), case);
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut p = invoke(&d, task, &script);
        // This budget includes process startup, sandbox preparation and the
        // handshake, not just the selected stall. Prove that the peer reaches
        // that phase before asserting which input delivery is uncertain.
        p["input"]["limits"]["setup_timeout_ms"] = json!(5_000);
        if case == "blocked_turn_write" {
            let prompt = "x".repeat(400_000);
            p["input"]["prompt"] =
                json!({"text":prompt,"sha256":format!("{:x}",Sha256::digest(prompt.as_bytes()))});
        }
        result(rpc.call("modules.invoke", p).await);
        let outcome = timeout(Duration::from_secs(12), async {
            trace_contains(&script, case).await;
            rpc.wait_outcome(&exec).await
        })
        .await
        .unwrap();
        assert_eq!(
            outcome["result"]["status"], "timed_out",
            "{case}: {outcome}"
        );
        assert!(events_of(
            &replay_events(&mut rpc, &d, &exec).await,
            "module.input.accepted"
        )
        .is_empty());
        assert_eq!(
            outcome["result"]["input_delivery_unknown"],
            matches!(case, "turn_silent" | "blocked_turn_write"),
            "{case}: {outcome}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_exit_grace_and_descendant_output_drain_are_bounded() {
    for (case, status, lower) in [("ignore_eof", "failed", 14), ("held_pipe", "succeeded", 4)] {
        let scripts = tempfile::tempdir().unwrap();
        let script = provider(scripts.path(), case);
        let d = dispatcher().await;
        let mut rpc = d.connect().await;
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let start = std::time::Instant::now();
        result(rpc.call("modules.invoke", invoke(&d, task, &script)).await);
        // Start the upper-bound clock at EOF, after process startup. The
        // descendant holds the pipe for 12 s; the supervisor drains for 5 s.
        trace_contains(&script, "eof").await;
        let drain_started = std::time::Instant::now();
        let outcome = timeout(Duration::from_secs(23), rpc.wait_outcome(&exec))
            .await
            .unwrap();
        assert!(start.elapsed() >= Duration::from_secs(lower));
        if case == "held_pipe" {
            assert!(
                drain_started.elapsed() < Duration::from_secs(9),
                "waited for the descendant instead of the drain bound"
            );
        }
        assert_eq!(outcome["result"]["status"], status, "{case}: {outcome}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_cancel_during_setup_work_and_shutdown_preserves_siblings() {
    let scripts = tempfile::tempdir().unwrap();
    let d = dispatcher().await;
    let mut rpc = d.connect().await;
    let sibling = provider(scripts.path(), "active_gate");
    let sibling_task = Uuid::new_v4();
    let sibling_exec = d.task_exec_id(sibling_task, 1);
    result(
        rpc.call("modules.invoke", invoke(&d, sibling_task, &sibling))
            .await,
    );
    trace_contains(&sibling, "accepted").await;
    for case in ["initialize_silent", "active_cancel", "delayed_exit"] {
        let script = provider(scripts.path(), case);
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        result(rpc.call("modules.invoke", invoke(&d, task, &script)).await);
        trace_contains(
            &script,
            match case {
                "initialize_silent" => "read:initialize",
                "delayed_exit" => "eof",
                _ => "accepted",
            },
        )
        .await;
        result(
            rpc.call(
                "modules.cancel",
                d.params(true, json!({"exec_id":exec,"reason":"owner_stop"})),
            )
            .await,
        );
        let outcome = timeout(Duration::from_secs(15), rpc.wait_outcome(&exec))
            .await
            .unwrap();
        assert_eq!(outcome["status"], "cancelled", "{case}: {outcome}");
        #[cfg(target_os = "linux")]
        {
            let pid: i32 = std::fs::read_to_string(script.with_extension("pid"))
                .unwrap()
                .parse()
                .unwrap();
            assert!(
                !process_exists(pid),
                "{case} provider remained resident after settlement"
            );
        }
        assert!(events_of(
            &replay_events(&mut rpc, &d, &sibling_exec).await,
            "invocation.outcome"
        )
        .is_empty());
    }
    std::fs::write(sibling.with_extension("release"), b"continue").unwrap();
    let outcome = timeout(Duration::from_secs(10), rpc.wait_outcome(&sibling_exec))
        .await
        .unwrap();
    assert_eq!(outcome["result"]["status"], "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_acceptance_lifts_setup_deadline_and_output_pressure_runs_until_stopped() {
    let scripts = tempfile::tempdir().unwrap();
    let d = dispatcher().await;
    let mut rpc = d.connect().await;
    let script = provider(scripts.path(), "active_gate");
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = invoke(&d, task, &script);
    params["input"]["limits"]["setup_timeout_ms"] = json!(5_000);
    result(rpc.call("modules.invoke", params).await);
    trace_contains(&script, "accepted").await;
    sleep(Duration::from_millis(5_400)).await;
    assert!(
        events_of(
            &replay_events(&mut rpc, &d, &exec).await,
            "invocation.outcome"
        )
        .is_empty(),
        "setup deadline must not limit active work"
    );
    std::fs::write(script.with_extension("release"), b"continue").unwrap();
    let outcome = timeout(Duration::from_secs(10), rpc.wait_outcome(&exec))
        .await
        .unwrap();
    assert_eq!(outcome["result"]["status"], "succeeded");

    let script = provider(scripts.path(), "output_pressure");
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = invoke(&d, task, &script);
    // The retired limit travels as an older API sends it, far below what
    // this turn records, and is ignored (`g5.agents.output-bounds`).
    params["input"]["limits"]["max_output_bytes"] = json!(32768);
    result(rpc.call("modules.invoke", params).await);
    // Each delta is about 2.5 KB: forty are three times that value.
    for _ in 0..40 {
        timeout(
            Duration::from_secs(120),
            rpc.module_event(&exec, "module.message.delta"),
        )
        .await
        .expect("output keeps being recorded past the retired limit");
    }
    assert_eq!(
        result(
            rpc.call("modules.status", d.params(false, json!({"exec_id":exec})))
                .await
        )["status"],
        "running",
    );
    result(
        rpc.call(
            "modules.cancel",
            d.params(true, json!({"exec_id":exec,"reason":"owner_stop"})),
        )
        .await,
    );
    let outcome = timeout(Duration::from_secs(120), rpc.wait_outcome(&exec))
        .await
        .unwrap();
    assert_eq!(outcome["status"], "cancelled", "{outcome}");
    let events = replay_events(&mut rpc, &d, &exec).await;
    assert!(events_of(&events, "module.message.delta").len() >= 40);
    assert_eq!(events_of(&events, "module.input.accepted").len(), 1);
}
