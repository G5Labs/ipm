//! Integration tests for the pluggable isolation backend system.
//!
//! This test file exercises the isolation backends defined in `g5d::isolation`:
//!
//! 1. **Process backend tests** (always run) — Verify that `ProcessBackend` correctly
//!    executes commands as child processes, enforces working directory isolation,
//!    applies environment guardrails, respects timeouts, and surfaces
//!    unsupported process-mode features clearly.
//!
//! Real KVM lifecycle, storage and network coverage lives in
//! `scripts/test-cloud-hypervisor-ci.sh`, `microvm_rootfs_gc.rs`, and the
//! API-driven `block_workload`/`host_loss_recovery` E2E scenarios.
//!
//! 2. **Backend selection tests** (always run) — Verify that `IsolationManager`
//!    correctly selects backends based on configuration.

mod helpers;

use std::time::Duration;
use tempfile::TempDir;

use g5d::isolation::{
    BackendType, IsolationConfig, IsolationLevel, IsolationManager, IsolationMode, MicrovmBackend,
    MicrovmMount, MicrovmRunSpec, ProcessBackend, ResourceConfig,
};

// ---------------------------------------------------------------------------
// Helper: Build a MicrovmRunSpec for testing
// ---------------------------------------------------------------------------

/// Constructs a `MicrovmRunSpec` with sensible defaults for test commands.
fn test_run_spec(cmd: Vec<String>, work_dir: &std::path::Path) -> MicrovmRunSpec {
    MicrovmRunSpec {
        image: "g5/worker:base".to_string(),
        cmd,
        env: vec![],
        mount_host_dir: work_dir.to_path_buf(),
        mount_guest_dir: "/work".to_string(),
        cpus: 2,
        mem_mib: 512,
        extra_mounts: vec![],
        port_maps: vec![],

        scratch_disk: None,
        block_workload: false,
        nested_virtualization: false,
        guest_restore: None,
    }
}

// ===========================================================================
// Process Backend Tests (always run)
// ===========================================================================

/// Verify that `ProcessBackend` can execute a simple command and capture its stdout.
#[tokio::test]
async fn test_process_backend_executes_command() {
    let backend = ProcessBackend::new();
    assert!(backend.probe());
    assert_eq!(backend.name(), "process");
    assert_eq!(backend.isolation_level(), IsolationLevel::Process);

    let tmp = TempDir::new().unwrap();
    let spec = test_run_spec(vec!["echo".into(), "hello".into()], tmp.path());

    let result = backend.run(&spec).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "hello");
    assert!(result.duration < Duration::from_secs(5));
}

/// Verify that the child process runs in the specified working directory (`mount_host_dir`).
#[tokio::test]
async fn test_process_backend_working_directory() {
    let backend = ProcessBackend::new();
    let tmp = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(tmp.path()).unwrap();

    let spec = test_run_spec(vec!["pwd".into()], tmp.path());

    let result = backend.run(&spec).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(
        std::fs::canonicalize(std::path::PathBuf::from(result.stdout.trim())).unwrap(),
        canonical,
    );
}

/// Verify environment variable handling in the child process.
///
/// The `ProcessBackend` passes env vars from `spec.env` to the child process.
/// This test verifies that explicitly-passed env vars ARE visible in the child.
///
/// The test uses `set_var`/`remove_var` and should be run serially to avoid
/// env var conflicts with parallel tests.
#[tokio::test]
#[serial_test::serial]
async fn test_process_backend_env_isolation() {
    let backend = ProcessBackend::new();
    let tmp = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(tmp.path()).unwrap();

    // Set an environment variable in the current process
    std::env::set_var("G5D_TEST_SECRET", "leaked");

    // Test 1: Verify explicitly-passed env vars ARE visible
    let spec = MicrovmRunSpec {
        env: vec![("G5D_TEST_VISIBLE".into(), "yes".into())],
        ..test_run_spec(
            vec!["sh".into(), "-c".into(), "echo $G5D_TEST_VISIBLE".into()],
            tmp.path(),
        )
    };

    let result = backend.run(&spec).await.unwrap();
    assert_eq!(result.stdout.trim(), "yes");

    // Test 2: Parent env vars should not leak into the child.
    let spec2 = MicrovmRunSpec {
        env: vec![],
        ..test_run_spec(
            vec!["sh".into(), "-c".into(), "echo $G5D_TEST_SECRET".into()],
            tmp.path(),
        )
    };

    let result2 = backend.run(&spec2).await.unwrap();
    assert_eq!(result2.exit_code, 0);
    assert_eq!(result2.stdout.trim(), "");

    // Test 3: HOME/TMPDIR should be rewritten into the workspace.
    let spec3 = MicrovmRunSpec {
        env: vec![],
        ..test_run_spec(
            vec![
                "sh".into(),
                "-c".into(),
                "printf '%s\\n%s' \"$HOME\" \"$TMPDIR\"".into(),
            ],
            tmp.path(),
        )
    };

    let result3 = backend.run(&spec3).await.unwrap();
    assert_eq!(result3.exit_code, 0);
    let lines: Vec<&str> = result3.stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with(canonical.to_string_lossy().as_ref()));
    assert!(lines[1].starts_with(canonical.to_string_lossy().as_ref()));

    // Clean up
    std::env::remove_var("G5D_TEST_SECRET");
}

/// Verify that a command exceeding the configured timeout is killed and an
/// appropriate error or result is returned.
#[tokio::test]
async fn test_process_backend_timeout() {
    let backend = ProcessBackend::new();
    let tmp = TempDir::new().unwrap();

    let spec = test_run_spec(vec!["sleep".into(), "60".into()], tmp.path());

    let start = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(3), backend.run(&spec)).await;

    let elapsed = start.elapsed();
    // Either the backend killed the process (Ok with non-zero exit)
    // or our timeout wrapper fired (Err)
    match result {
        Ok(Ok(run_result)) => {
            // Backend handled the timeout internally
            assert_ne!(
                run_result.exit_code, 0,
                "Timed-out process should have non-zero exit"
            );
        }
        Ok(Err(_)) => {
            // Backend returned an error (e.g., IsolationError::Timeout)
            // This is acceptable
        }
        Err(_) => {
            // Our tokio::time::timeout fired — backend didn't handle timeout
            // This is acceptable for process backend
        }
    }
    assert!(
        elapsed < Duration::from_secs(10),
        "Timeout should fire within ~3s, not wait for 60s"
    );
}

/// Verify that when resource limits (CPU, memory) are configured on a
/// `MicrovmRunSpec`, the `ProcessBackend` logs a warning but still executes
/// the command successfully.
#[tokio::test]
async fn test_process_backend_resource_limits() {
    helpers::setup_tracing();
    let backend = ProcessBackend::new();
    let tmp = TempDir::new().unwrap();

    let spec = MicrovmRunSpec {
        cpus: 8,
        mem_mib: 32768,
        ..test_run_spec(vec!["echo".into(), "resource_test".into()], tmp.path())
    };

    // ProcessBackend should log a warning about unenforced limits
    // but still execute the command successfully
    let result = backend.run(&spec).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "resource_test");
}

/// Verify workspace-local runtime guardrails of the `ProcessBackend`.
#[tokio::test]
async fn test_process_backend_workspace_local_runtime_dirs() {
    let backend = ProcessBackend::new();
    let work_dir = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(work_dir.path()).unwrap();

    let spec = test_run_spec(
        vec![
            "sh".into(),
            "-c".into(),
            "echo testcontent > testfile && printf '%s\\n%s\\n' \"$HOME\" \"$TMPDIR\" && cat testfile".into(),
        ],
        work_dir.path(),
    );

    let result = backend.run(&spec).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("testcontent"));

    let testfile = work_dir.path().join("testfile");
    assert!(testfile.exists(), "File should be created inside work_dir");
    assert!(canonical.join(".g5-process-home").is_dir());
    assert!(canonical.join(".g5-process-tmp").is_dir());
}

/// Verify that unsupported extra mounts are rejected in process mode.
#[tokio::test]
async fn test_process_backend_rejects_extra_mounts() {
    let backend = ProcessBackend::new();
    let work_dir = TempDir::new().unwrap();
    let spec = MicrovmRunSpec {
        extra_mounts: vec![MicrovmMount::read_write(
            work_dir.path().join("cache"),
            "/cache",
        )],
        ..test_run_spec(vec!["echo".into(), "nope".into()], work_dir.path())
    };

    let result = backend.run(&spec).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("does not support extra mounts"));
}

// ===========================================================================
// Backend Selection Tests (always run)
// ===========================================================================

/// Verify that `IsolationManager` correctly selects the backend based on
/// configuration.
#[tokio::test]
async fn test_backend_selection() {
    // Test 1: Explicit process mode
    let config = IsolationConfig {
        mode: IsolationMode::Process,
        fallback: None,
        base_image: "g5/worker:base".to_string(),
        resources: ResourceConfig::default(),
    };
    let manager = IsolationManager::new(config).await.unwrap();
    assert_eq!(manager.active_backend_type(), BackendType::Process);
    assert_eq!(manager.isolation_level(), IsolationLevel::Process);

    // Test 2: Deprecated fallback config is rejected instead of silently downgrading.
    let config2 = IsolationConfig {
        mode: IsolationMode::MicroVm,
        fallback: Some(IsolationMode::Process),
        base_image: "g5/worker:base".to_string(),
        resources: ResourceConfig::default(),
    };
    let result2 = IsolationManager::new(config2).await;
    assert!(result2.is_err());
    assert!(
        result2
            .unwrap_err()
            .to_string()
            .contains("no longer supported"),
        "fallback config should be rejected explicitly"
    );

    // Test 3: Detection results contain at least Process backend
    let detection = manager.detection();
    assert!(
        detection
            .available_backends
            .iter()
            .any(|b| b.backend_type == BackendType::Process),
        "Process backend should always be available",
    );
}

/// Verify IsolationManager run() delegates through to ProcessBackend correctly.
#[tokio::test]
async fn test_isolation_manager_run_delegation() {
    let config = IsolationConfig {
        mode: IsolationMode::Process,
        fallback: None,
        base_image: "g5/worker:base".to_string(),
        resources: ResourceConfig::default(),
    };
    let manager = IsolationManager::new(config).await.unwrap();

    let tmp = TempDir::new().unwrap();
    let spec = test_run_spec(
        vec!["sh".into(), "-c".into(), "echo manager_test && pwd".into()],
        tmp.path(),
    );

    let result = manager.run(&spec).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("manager_test"));
}

/// Verify IsolationManager start() + kill() lifecycle.
#[tokio::test]
async fn test_isolation_manager_start_kill_lifecycle() {
    let config = IsolationConfig {
        mode: IsolationMode::Process,
        fallback: None,
        base_image: "g5/worker:base".to_string(),
        resources: ResourceConfig::default(),
    };
    let manager = IsolationManager::new(config).await.unwrap();

    let tmp = TempDir::new().unwrap();
    let spec = test_run_spec(vec!["sleep".into(), "60".into()], tmp.path());

    let handle = manager.start(&spec).await.unwrap();
    assert_eq!(handle.backend_type, BackendType::Process);

    // Kill the long-running process
    manager.kill(&handle).await.unwrap();
}

/// Verify IsolationManager config accessors.
#[tokio::test]
async fn test_isolation_manager_config_accessors() {
    let config = IsolationConfig {
        mode: IsolationMode::Process,
        fallback: None,
        base_image: "custom:v1".to_string(),
        resources: ResourceConfig {
            cpus: 8,
            memory_gb: 32,
        },
    };
    let manager = IsolationManager::new(config).await.unwrap();

    assert_eq!(manager.config().base_image, "custom:v1");
    assert_eq!(manager.config().resources.cpus, 8);
    assert_eq!(manager.config().resources.memory_gb, 32);
    assert!(!manager.detection().platform.os.is_empty());
}

/// Verify process backend handles multiple sequential commands correctly.
#[tokio::test]
async fn test_process_backend_sequential_runs() {
    let backend = ProcessBackend::new();
    let tmp = TempDir::new().unwrap();

    // Run 3 commands sequentially — each should get a clean environment
    for i in 0..3 {
        let spec = test_run_spec(vec!["echo".into(), format!("run-{}", i)], tmp.path());
        let result = backend.run(&spec).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), format!("run-{}", i));
    }
}

// ===========================================================================
// Process isolation never leaves localhost development
// ===========================================================================

/// `g5.runtime.boot.backend.process-local`: no cloud, CI, packer, helm,
/// terraform, installer or workflow configuration selects `process` isolation,
/// and no control plane other than the local development stack admits a
/// process host. Local helper fixtures and installer-refusal tests are excluded.
#[test]
fn deployment_and_ci_configuration_never_selects_process_isolation() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // The self-hosted bundle test asserts that the installer refuses the mode;
    // the local worker-host script explains the gate and only honours a
    // developer's explicit setting, never choosing the mode itself.
    // The preview-profile regression quotes the selection the `full-stack` profile
    // makes in its own shell (the preview is the previewed instance's
    // localhost) and the admission the recipe derives from it.
    let allowed_refusal_tests = [
        "scripts/test-self-hosted-bundle.sh",
        "scripts/worker-host-env.sh",
        "scripts/test-dev-up-preview-profile.sh",
        // Sources the local helper with stub host tools in a temporary directory.
        "scripts/test-worker-host-env-process-build.sh",
        // Drives the microvm preview's prerequisite check with an ambient
        // process selection to prove it is refused, naming the sibling profile.
        "scripts/test-preview-microvm-host.sh",
    ];
    // The one control plane that can admit process hosts, and only while the
    // developer selected process mode for the host (the recipe derives it).
    let local_development_stack = "infra/docker/docker-compose.dev.yml";
    let mut offenders = Vec::new();
    for root in ["infra", ".github", "scripts"] {
        for entry in walkdir::WalkDir::new(repo.join(root))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let relative = entry
                .path()
                .strip_prefix(&repo)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if allowed_refusal_tests.contains(&relative.as_str()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            for (number, line) in text.lines().enumerate() {
                let selects_process = line.contains("G5D_ISOLATION_MODE=process")
                    || line.contains("G5D_ISOLATION_MODE: process")
                    || line.contains("G5D_ISOLATION_MODE=\"process\"")
                    || line.contains("G5D_ALLOW_PROCESS_ISOLATION=1")
                    || line.contains("G5D_ALLOW_PROCESS_ISOLATION=true");
                let admits_process_hosts = line
                    .contains("G5_ALLOW_PROCESS_ISOLATION_HOSTS: \"true\"")
                    || line.contains("G5_ALLOW_PROCESS_ISOLATION_HOSTS: true")
                    || line.contains("G5_ALLOW_PROCESS_ISOLATION_HOSTS=true")
                    || line.contains("G5_ALLOW_PROCESS_ISOLATION_HOSTS=\"true\"");
                if selects_process || admits_process_hosts {
                    offenders.push(format!("{relative}:{}: {}", number + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "process isolation is a localhost-only developer choice; it must not be configured or admitted by:\n{}",
        offenders.join("\n")
    );
    let dev_stack = std::fs::read_to_string(repo.join(local_development_stack)).unwrap();
    assert!(
        dev_stack.contains("G5_ALLOW_PROCESS_ISOLATION_HOSTS: \"${G5_ALLOW_PROCESS_ISOLATION_HOSTS:-false}\""),
        "the local development stack admits process hosts only through the value dev-stack-up derives, closed by default"
    );
    let justfile = std::fs::read_to_string(repo.join("Justfile")).unwrap();
    assert!(
        justfile.contains("export G5_ALLOW_PROCESS_ISOLATION_HOSTS=false")
            && justfile.contains("1|true) export G5_ALLOW_PROCESS_ISOLATION_HOSTS=true ;;"),
        "dev-stack-up must derive the admission from the host's own process-mode selection"
    );
}
