//! Real-boot E2E for per-VM rootfs GC (ontology `g5/g5d/isolation` D40/I14,
//! issue #707): after a `CloudHypervisorBackend::run` microVM exits, the per-VM
//! rootfs clone under `G5D_ROOTFS_STATE_DIR` must be removed while the
//! assembled-rootfs template (D29) survives for the next boot.
//!
//! Uses the OCI fixture from `prepare-cloud-hypervisor-test.py`, real KVM,
//! the complete runtime disk, and the normal owned-root reaper. The dedicated
//! Linux gate supplies these inputs; generic integration lanes do not boot VMs.

#![cfg(all(feature = "microvm-e2e", feature = "test-helpers"))]

use std::path::Path;
use std::time::{Duration, Instant};

use g5d::isolation::{CloudHypervisorBackend, MicrovmBackend, MicrovmRunSpec};
use tempfile::TempDir;

fn spec(mount: &Path, marker: &str) -> MicrovmRunSpec {
    MicrovmRunSpec {
        image: std::env::var("G5_TEST_OCI_LAYOUT").expect("prepared test OCI layout"),
        cmd: vec!["/bin/sh".into(), "-c".into(), format!("echo {marker}")],
        env: vec![],
        mount_host_dir: mount.to_path_buf(),
        mount_guest_dir: "/work".to_string(),
        cpus: 1,
        mem_mib: 512,
        extra_mounts: vec![g5d::isolation::MicrovmMount::read_only(
            std::env::var("G5_TEST_RUNTIME").expect("prepared test runtime"),
            "/opt/g5",
        )],
        port_maps: vec![],

        scratch_disk: Some(g5d::isolation::ScratchDiskSpec { size_mib: 512 }),
        block_workload: true,
        nested_virtualization: false,
        guest_restore: None,
    }
}

/// Non-hidden entries directly under the rootfs state root, excluding the
/// `templates/` subtree (which must persist across boots).
fn leftover_vm_dirs(state_root: &Path) -> Vec<String> {
    std::fs::read_dir(state_root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n != "templates" && !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default()
}

/// Cleanup is a detached background tokio task; give it a bounded window.
/// Must yield to the runtime (async sleep) — a blocking sleep would starve
/// the very task being awaited.
async fn await_gc(state_root: &Path, existing: &[String]) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let left: Vec<_> = leftover_vm_dirs(state_root)
            .into_iter()
            .filter(|entry| !existing.contains(entry))
            .collect();
        if left.is_empty() || Instant::now() >= deadline {
            return left;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

// Multi-thread flavor: the GC runs as a detached task and must be able to
// progress while the test task polls.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rootfs_gc_removes_per_vm_clone_after_run() {
    if std::env::var_os("G5_TEST_OCI_LAYOUT").is_none() {
        eprintln!("KVM fixture not supplied; strict gate owns real boots");
        return;
    }
    let backend = CloudHypervisorBackend::new();
    assert!(backend.probe(), "supplied KVM fixture must be usable");

    let state_root = std::path::PathBuf::from(std::env::var("G5_TEST_VM_ROOT").unwrap());
    let existing = leftover_vm_dirs(&state_root);
    let cache = TempDir::new().unwrap();
    let mount = TempDir::new().unwrap();
    // Safe: this integration-test binary runs these statements before any
    // concurrent reader of the same vars (single #[tokio::test] in the file).
    std::env::set_var("G5D_ROOTFS_STATE_DIR", &state_root);
    std::env::set_var("G5D_OCI_CACHE_DIR", cache.path());
    std::env::set_var("G5D_TEST_VM_EXEC_BIN", env!("CARGO_BIN_EXE_g5d"));

    // The host prepares the environment before accepting starts. Exercise that
    // same CLI in this state root; per-VM launch intentionally cannot format a
    // missing template on the startup path.
    let preparation = tokio::process::Command::new(env!("CARGO_BIN_EXE_g5d"))
        .args([
            "oci",
            "prepare-workload",
            &std::env::var("G5_TEST_OCI_LAYOUT").unwrap(),
            "--size-mib",
            "512",
            "--slots",
            "1",
        ])
        .output()
        .await
        .expect("prepare host environment");
    assert!(
        preparation.status.success(),
        "preparation failed: {}",
        String::from_utf8_lossy(&preparation.stderr)
    );

    // Boot 1 clones the prepared image and boots the real guest.
    let result = backend
        .run(&spec(mount.path(), "GC_E2E_BOOT_1_OK"))
        .await
        .expect("boot 1 runs");
    assert_eq!(result.exit_code, 0, "stderr: {}", result.stderr);
    assert!(
        result.stdout.contains("GC_E2E_BOOT_1_OK"),
        "guest output missing marker: {:?}",
        result.stdout
    );

    let left = await_gc(&state_root, &existing).await;
    assert!(
        left.is_empty(),
        "per-VM rootfs clone(s) not GC'd after boot 1: {left:?}"
    );
    let templates = state_root.join("templates").join("sha256");
    assert!(
        templates.is_dir() && std::fs::read_dir(&templates).unwrap().next().is_some(),
        "assembled-rootfs template must survive the per-VM GC"
    );

    // Boot 2 — warm: template cache hit + clone; the clone must again be
    // removed after exit and the template must still survive.
    let result = backend
        .run(&spec(mount.path(), "GC_E2E_BOOT_2_OK"))
        .await
        .expect("boot 2 runs");
    assert_eq!(result.exit_code, 0, "stderr: {}", result.stderr);
    assert!(result.stdout.contains("GC_E2E_BOOT_2_OK"));

    let left = await_gc(&state_root, &existing).await;
    assert!(
        left.is_empty(),
        "per-VM rootfs clone(s) not GC'd after boot 2: {left:?}"
    );
    assert!(
        std::fs::read_dir(&templates).unwrap().next().is_some(),
        "template must survive across boots"
    );
}
