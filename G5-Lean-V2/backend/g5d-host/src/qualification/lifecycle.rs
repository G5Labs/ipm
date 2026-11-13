//! Host-side workspace VM custody through the real admission, journal,
//! root-deletion and reconciliation paths, driven by a scripted launcher.
//!
//! These are the deterministic halves of the K1 host qualification: they
//! prove the refusals, superseded-incarnation cleanup, deletion of released
//! roots and boot-summary evidence that the real-KVM lanes then observe
//! against Cloud Hypervisor.

use std::sync::Arc;
use std::time::Duration;

use crate::ctl::{HostControlState, LeaseRenewalBroker};
use crate::runtime::WorkspaceHostState;
use crate::test_support::{materializer_available, FakeLauncher, HostFixture};
use crate::workspace_vm::WorkspaceVmRuntime;
use g5_internal_api::host_ctl::BootSummary;
use g5_internal_api::host_ctl::{
    CommandResult, CurrentIncarnation, HostVmState, SlotUnavailableReason, SpecRejectReason,
    StopMode, VmEventKind, VmStatus, VmStop,
};
use uuid::Uuid;

struct Harness {
    _temp: tempfile::TempDir,
    fixture: HostFixture,
    launcher: Arc<FakeLauncher>,
    journal: Arc<crate::ctl::EventJournal>,
    runtime: Arc<WorkspaceVmRuntime>,
}

async fn harness(slots_total: u32) -> Option<Harness> {
    let temp = tempfile::Builder::new()
        .prefix("g5-host-lifecycle-")
        .tempdir_in(
            std::env::var_os("G5_TEST_REFLINK_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap()),
        )
        .unwrap();
    let fixture = HostFixture::new(temp.path());
    if !materializer_available(&fixture).await {
        return None;
    }
    let launcher = Arc::new(FakeLauncher::default());
    let journal = fixture.journal().await;
    let runtime = fixture.runtime(
        launcher.clone(),
        journal.clone(),
        Arc::new(LeaseRenewalBroker::default()),
        slots_total,
    );
    Some(Harness {
        _temp: temp,
        fixture,
        launcher,
        journal,
        runtime,
    })
}

fn stop(workspace_id: Uuid, vm_id: Uuid, mode: StopMode) -> VmStop {
    VmStop {
        workspace_id,
        vm_id,
        mode,
        grace_secs: 30,
    }
}

fn ready_summary(events: &[g5_internal_api::host_ctl::VmEvent], vm_id: Uuid) -> BootSummary {
    let ready = events
        .iter()
        .find(|event| event.vm_id == vm_id && matches!(event.kind, VmEventKind::Ready { .. }))
        .expect("ready event journaled");
    ready
        .log_tail
        .as_deref()
        .expect("ready event carries the host boot summary")
        .parse()
        .expect("boot summary parses")
}

async fn wait_until<F, Fut>(what: &str, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !condition().await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Whether `workspaces_root` still holds the root of `workspace_id` or any
/// claimed root waiting for deletion.
fn root_or_claimed_root_exists(workspaces_root: &std::path::Path, workspace_id: Uuid) -> bool {
    workspaces_root.join(workspace_id.to_string()).exists()
        || std::fs::read_dir(workspaces_root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".trash-"))
        })
}

/// Released roots are claimed before the release returns and deleted in the
/// background; wait for the deletion to finish.
async fn wait_for_root_deletion(h: &Harness, workspace_id: Uuid) {
    let workspaces_root = h.fixture.workspaces_root.clone();
    wait_until("the released root to be deleted", || {
        let workspaces_root = workspaces_root.clone();
        async move { !root_or_claimed_root_exists(&workspaces_root, workspace_id) }
    })
    .await;
}

#[tokio::test]
async fn start_then_stop_deletes_the_root_and_journals_the_boot_summary() {
    let Some(h) = harness(2).await else { return };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();

    assert_eq!(
        h.runtime
            .start(h.fixture.start(workspace_id, vm_id, 1))
            .await,
        CommandResult::Started
    );
    let inventory = h.runtime.inventory().await;
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].vm_id, vm_id);
    assert_eq!(inventory[0].state, HostVmState::Ready);

    let (_, launched) = h.launcher.started().await.pop().expect("one launch");
    assert_eq!(
        launched.mount_host_dir,
        h.fixture.workspaces_root.join(workspace_id.to_string())
    );
    assert_eq!(launched.mount_guest_dir, "/work");
    assert!(
        !launched.block_workload,
        "the existing host layout remains the default"
    );
    assert_eq!(launched.cmd, ["/opt/g5/bin/g5d", "dispatch"]);
    assert!(launched
        .extra_mounts
        .iter()
        .any(|mount| mount.host_path == h.fixture.runtime_root
            && mount.guest_path == "/opt/g5"
            && mount.access == g5d_isolation::isolation::MountAccess::ReadOnly));
    assert!(launched
        .env
        .iter()
        .any(|(key, value)| key == "G5D_WORKSPACE_TOKEN" && value.contains(&vm_id.to_string())));
    assert!(launched
        .env
        .iter()
        .any(|(key, _)| key == "G5D_DISPATCH_TOKEN"));
    assert!(launched.port_maps.iter().any(|(_, guest)| *guest == 7100));
    let layout: serde_json::Value = serde_json::from_slice(
        &std::fs::read(launched.mount_host_dir.join(".g5/layout.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(layout["repos"][0]["repo_id"], h.fixture.repo_id.to_string());
    assert!(launched.mount_host_dir.join("app/README.md").is_file());

    let events = h.journal.pending().await;
    assert!(matches!(events[0].kind, VmEventKind::Booted));
    assert!(matches!(
        events[1].kind,
        VmEventKind::Ready { epoch: 1, .. }
    ));
    ready_summary(&events, vm_id);
    // Output the guest wrote, such as ignored build output.
    std::fs::create_dir_all(launched.mount_host_dir.join("app/target")).unwrap();
    std::fs::write(
        launched.mount_host_dir.join("app/target/output.bin"),
        b"built",
    )
    .unwrap();

    assert_eq!(
        h.runtime
            .start(h.fixture.start(workspace_id, vm_id, 1))
            .await,
        CommandResult::AlreadyRunning
    );
    assert_eq!(
        h.runtime
            .start(h.fixture.start(workspace_id, Uuid::new_v4(), 2))
            .await,
        CommandResult::WorkspaceBusy,
        "a second incarnation of one workspace cannot share the root"
    );

    let stopped = h
        .runtime
        .stop(stop(workspace_id, vm_id, StopMode::Graceful))
        .await;
    match stopped {
        CommandResult::Stopped { exit } => assert_eq!(exit.code, Some(0)),
        other => panic!("expected stopped, got {other:?}"),
    }
    let signals = h.launcher.stopped().await;
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].mode, StopMode::Graceful);
    assert_eq!(signals[0].grace, Duration::from_secs(30));

    assert_eq!(h.runtime.active_count().await, 0);
    assert!(h.runtime.endpoint(vm_id).await.is_none());
    assert_eq!(
        h.runtime
            .status(VmStatus {
                workspace_id,
                vm_id
            })
            .await,
        CommandResult::NotFound
    );
    assert_eq!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::NotFound,
        "the retained command result, not the runtime, answers a stop replay"
    );
    assert!(
        !h.fixture
            .workspaces_root
            .join(workspace_id.to_string())
            .exists(),
        "the stop claims the root before it returns"
    );
    wait_for_root_deletion(&h, workspace_id).await;
    let events = h.journal.pending().await;
    assert!(matches!(
        events.last().unwrap().kind,
        VmEventKind::Exited { code: Some(0), .. }
    ));

    let restart_vm = Uuid::new_v4();
    assert_eq!(
        h.runtime
            .start(h.fixture.start(workspace_id, restart_vm, 2))
            .await,
        CommandResult::Started
    );
    ready_summary(&h.journal.pending().await, restart_vm);
    let root = h.fixture.workspaces_root.join(workspace_id.to_string());
    assert!(root.join("app/README.md").is_file(), "a fresh tree");
    assert!(
        !root.join("app/target/output.bin").exists(),
        "a restart sees nothing the previous guest wrote"
    );
    assert_eq!(h.runtime.active_count().await, 1);
}

#[tokio::test]
async fn reconcile_kills_superseded_incarnations_and_ignores_older_generations() {
    let Some(h) = harness(2).await else { return };
    let ws_a = Uuid::new_v4();
    let vm_a = Uuid::new_v4();
    let ws_b = Uuid::new_v4();
    let vm_b = Uuid::new_v4();
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_a, vm_a, 1)).await,
        CommandResult::Started
    );
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_b, vm_b, 1)).await,
        CommandResult::Started
    );
    let handle_a = h.launcher.started().await[0].0.clone();

    let host_state = WorkspaceHostState::new(
        Uuid::new_v4(),
        2,
        h.fixture.workspaces_root.clone(),
        h.runtime.clone(),
    );
    let current_b = CurrentIncarnation {
        workspace_id: ws_b,
        vm_id: vm_b,
        epoch: 1,
    };
    host_state
        .reconcile_current_incarnations(5, std::slice::from_ref(&current_b))
        .await;
    while !h
        .journal
        .pending()
        .await
        .iter()
        .any(|event| event.vm_id == vm_a && matches!(event.kind, VmEventKind::StaleKilled))
    {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let signals = h.launcher.stopped().await;
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].handle_id, handle_a);
    assert_eq!(signals[0].mode, StopMode::Kill);
    let inventory = h.runtime.inventory().await;
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].vm_id, vm_b);
    let events = h.journal.pending().await;
    let a_events = events
        .iter()
        .filter(|event| event.vm_id == vm_a)
        .map(|event| &event.kind)
        .collect::<Vec<_>>();
    assert!(matches!(
        a_events[a_events.len() - 2],
        VmEventKind::Exited { .. }
    ));
    assert!(matches!(
        a_events[a_events.len() - 1],
        VmEventKind::StaleKilled
    ));
    wait_for_root_deletion(&h, ws_a).await;
    assert!(
        h.fixture.workspaces_root.join(ws_b.to_string()).is_dir(),
        "the current incarnation keeps its root"
    );

    host_state.reconcile_current_incarnations(4, &[]).await;
    assert_eq!(
        h.runtime.active_count().await,
        1,
        "an older heartbeat ack cannot revoke a newer accepted set"
    );
    host_state.reconcile_current_incarnations(6, &[]).await;
    while h.runtime.active_count().await != 0 {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(h.runtime.active_count().await, 0);
    assert!(h.runtime.endpoint(vm_b).await.is_none());

    // Ownership, not currency, fences vm.stop: the exact (workspace, vm) pair
    // this host runs is stopped, a foreign pairing is not found.
    let ws_c = Uuid::new_v4();
    let vm_c = Uuid::new_v4();
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_c, vm_c, 1)).await,
        CommandResult::Started
    );
    assert_eq!(
        h.runtime
            .stop(stop(Uuid::new_v4(), vm_c, StopMode::Kill))
            .await,
        CommandResult::NotFound
    );
    assert!(matches!(
        h.runtime.stop(stop(ws_c, vm_c, StopMode::Kill)).await,
        CommandResult::Stopped { .. }
    ));
}

#[tokio::test]
async fn capacity_refusals_are_definite_and_release_with_the_slot() {
    let Some(h) = harness(1).await else { return };
    let ws_a = Uuid::new_v4();
    let vm_a = Uuid::new_v4();
    let ws_b = Uuid::new_v4();
    let vm_b = Uuid::new_v4();
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_a, vm_a, 1)).await,
        CommandResult::Started
    );
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_b, vm_b, 1)).await,
        CommandResult::SlotUnavailable {
            reason: SlotUnavailableReason::Slots
        }
    );
    assert!(matches!(
        h.runtime.stop(stop(ws_a, vm_a, StopMode::Kill)).await,
        CommandResult::Stopped { .. }
    ));
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_b, vm_b, 1)).await,
        CommandResult::Started
    );
}

#[tokio::test]
async fn concurrent_boots_beyond_the_boot_budget_are_refused_not_queued() {
    let Some(h) = harness(4).await else { return };
    h.launcher.hold_starts(true);
    let mut boots = Vec::new();
    for _ in 0..2 {
        let runtime = h.runtime.clone();
        let command = h.fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1);
        boots.push(tokio::spawn(async move { runtime.start(command).await }));
    }
    let launcher = h.launcher.clone();
    wait_until("two launches held open", || {
        let launcher = launcher.clone();
        async move { launcher.started().await.len() == 2 }
    })
    .await;
    assert_eq!(h.runtime.boots_in_progress().await, 2);
    assert_eq!(
        h.runtime
            .start(h.fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1))
            .await,
        CommandResult::SlotUnavailable {
            reason: SlotUnavailableReason::BootCapacity
        }
    );
    h.launcher.hold_starts(false);
    for boot in boots {
        assert_eq!(boot.await.unwrap(), CommandResult::Started);
    }
    assert_eq!(h.runtime.boots_in_progress().await, 0);
    assert_eq!(h.runtime.active_count().await, 2);
}

#[tokio::test]
async fn natural_exit_settles_through_status_and_through_the_exit_monitor() {
    let Some(h) = harness(2).await else { return };
    let ws_a = Uuid::new_v4();
    let vm_a = Uuid::new_v4();
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_a, vm_a, 1)).await,
        CommandResult::Started
    );
    let handle_a = h.launcher.last_handle_id().await.unwrap();
    h.launcher
        .reap_publisher(&handle_a)
        .await
        .unwrap()
        .publish_exit(Some(3));
    match h
        .runtime
        .status(VmStatus {
            workspace_id: ws_a,
            vm_id: vm_a,
        })
        .await
    {
        CommandResult::Stopped { exit } => assert_eq!(exit.code, Some(3)),
        other => panic!("status must settle a reaped VMM, got {other:?}"),
    }
    assert!(h.launcher.stopped().await.is_empty(), "no second signal");
    wait_for_root_deletion(&h, ws_a).await;
    assert!(h
        .journal
        .pending()
        .await
        .iter()
        .any(|event| event.vm_id == vm_a
            && matches!(event.kind, VmEventKind::Exited { code: Some(3), .. })));

    let ws_b = Uuid::new_v4();
    let vm_b = Uuid::new_v4();
    assert_eq!(
        h.runtime.start(h.fixture.start(ws_b, vm_b, 1)).await,
        CommandResult::Started
    );
    let handle_b = h.launcher.last_handle_id().await.unwrap();
    h.launcher
        .reap_publisher(&handle_b)
        .await
        .unwrap()
        .publish_exit(Some(0));
    let runtime = h.runtime.clone();
    wait_until("exit monitor to reap the VM", || {
        let runtime = runtime.clone();
        async move { runtime.active_count().await == 0 }
    })
    .await;
    assert!(h
        .journal
        .pending()
        .await
        .iter()
        .any(|event| event.vm_id == vm_b
            && matches!(event.kind, VmEventKind::Exited { code: Some(0), .. })));
    wait_for_root_deletion(&h, ws_b).await;
}

#[tokio::test]
async fn a_release_deletes_only_roots_no_vm_holds() {
    let Some(h) = harness(3).await else { return };
    let running = Uuid::new_v4();
    let running_vm = Uuid::new_v4();
    let released = Uuid::new_v4();
    let released_vm = Uuid::new_v4();
    for (workspace_id, vm_id) in [(running, running_vm), (released, released_vm)] {
        assert_eq!(
            h.runtime
                .start(h.fixture.start(workspace_id, vm_id, 1))
                .await,
            CommandResult::Started
        );
    }
    // A third workspace is still booting: its root is prepared and its
    // launch is held open.
    h.launcher.hold_starts(true);
    let booting = Uuid::new_v4();
    let runtime = h.runtime.clone();
    let command = h.fixture.start(booting, Uuid::new_v4(), 1);
    let boot = tokio::spawn(async move { runtime.start(command).await });
    let launcher = h.launcher.clone();
    wait_until("the third launch to be held", || {
        let launcher = launcher.clone();
        async move { launcher.started().await.len() == 3 }
    })
    .await;

    assert!(matches!(
        h.runtime
            .stop(stop(released, released_vm, StopMode::Graceful))
            .await,
        CommandResult::Stopped { .. }
    ));
    wait_for_root_deletion(&h, released).await;
    let root = |workspace_id: Uuid| h.fixture.workspaces_root.join(workspace_id.to_string());
    assert!(root(running).join("app/README.md").is_file());
    assert!(
        root(booting).join("app/README.md").is_file(),
        "a booting VM's root is held from admission"
    );

    h.launcher.hold_starts(false);
    assert_eq!(boot.await.unwrap(), CommandResult::Started);
    assert!(root(booting).join("app/README.md").is_file());
}

#[tokio::test]
async fn stop_during_boot_cancels_the_reservation_and_frees_the_slot() {
    let Some(h) = harness(1).await else { return };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    h.launcher.hold_starts(true);
    let runtime = h.runtime.clone();
    let command = h.fixture.start(workspace_id, vm_id, 1);
    let boot = tokio::spawn(async move { runtime.start(command).await });
    let launcher = h.launcher.clone();
    wait_until("the launch to be held", || {
        let launcher = launcher.clone();
        async move { launcher.started().await.len() == 1 }
    })
    .await;
    assert_eq!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::Accepted,
        "a stop during boot is accepted and cancels the reservation"
    );
    h.launcher.hold_starts(false);
    assert_eq!(
        boot.await.unwrap(),
        CommandResult::BootFailed {
            reason: "boot_cancelled".to_string(),
            log_tail: None,
        }
    );
    assert_eq!(h.launcher.killed().await.len(), 1, "the late VMM is reaped");
    assert_eq!(h.runtime.active_count().await, 0);
    assert!(h
        .journal
        .pending()
        .await
        .iter()
        .any(|event| event.vm_id == vm_id && matches!(event.kind, VmEventKind::Exited { .. })));
    assert_eq!(
        h.runtime
            .start(h.fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1))
            .await,
        CommandResult::Started,
        "the cancelled boot released its slot"
    );
}

#[tokio::test]
async fn a_boot_whose_kill_cannot_prove_reap_keeps_its_root() {
    let Some(h) = harness(2).await else { return };
    let held = Uuid::new_v4();
    let held_vm = Uuid::new_v4();
    h.launcher.hold_starts(true);
    let runtime = h.runtime.clone();
    let command = h.fixture.start(held, held_vm, 1);
    let boot = tokio::spawn(async move { runtime.start(command).await });
    let launcher = h.launcher.clone();
    wait_until("the launch to be held", || {
        let launcher = launcher.clone();
        async move { launcher.started().await.len() == 1 }
    })
    .await;
    assert_eq!(
        h.runtime.stop(stop(held, held_vm, StopMode::Kill)).await,
        CommandResult::Accepted
    );
    // The cancelled boot's kill cannot prove the VMM gone.
    h.launcher.fail_kills(true);
    h.launcher.hold_starts(false);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!boot.is_finished(), "unproven boot retains its owner");
    assert_eq!(h.runtime.active_count().await, 1);
    let root = |workspace_id: Uuid| h.fixture.workspaces_root.join(workspace_id.to_string());

    // Another workspace's release must not take the possibly live VMM's root.
    let other = Uuid::new_v4();
    let other_vm = Uuid::new_v4();
    assert_eq!(
        h.runtime.start(h.fixture.start(other, other_vm, 1)).await,
        CommandResult::Started
    );
    assert!(matches!(
        h.runtime
            .stop(stop(other, other_vm, StopMode::Graceful))
            .await,
        CommandResult::Stopped { .. }
    ));
    wait_for_root_deletion(&h, other).await;
    assert!(
        root(held).join("app/README.md").is_file(),
        "an unreaped VMM keeps its root"
    );

    // Even a valid replacement cannot take over an unproven incarnation.
    assert_eq!(
        h.runtime
            .start(h.fixture.start(held, Uuid::new_v4(), 2))
            .await,
        CommandResult::WorkspaceBusy
    );
    h.launcher.fail_kills(false);
    let result = tokio::time::timeout(Duration::from_secs(10), boot)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, CommandResult::BootFailed { .. }));
    assert_eq!(h.runtime.active_count().await, 0);

    // A replacement is admitted only after exit proof releases custody.
    let restart_vm = Uuid::new_v4();
    assert_eq!(
        h.runtime.start(h.fixture.start(held, restart_vm, 2)).await,
        CommandResult::Started
    );
    assert!(matches!(
        h.runtime
            .stop(stop(held, restart_vm, StopMode::Graceful))
            .await,
        CommandResult::Stopped { .. }
    ));
    wait_for_root_deletion(&h, held).await;
}

#[tokio::test]
async fn a_disk_refusal_retries_deleting_released_roots() {
    let Some(h) = harness(1).await else { return };
    // A released root an earlier pass could not delete still holds space.
    let claimed = h
        .fixture
        .workspaces_root
        .join(format!(".trash-{}", Uuid::new_v4()));
    std::fs::create_dir_all(claimed.join("app/target")).unwrap();
    std::fs::write(claimed.join("app/target/output.bin"), b"built").unwrap();
    // Every start needs more free disk than the volume has.
    let mut config = h.fixture.config(1);
    config.scratch_disk_mib = u32::MAX;
    let runtime = h.fixture.runtime_from_config(
        config,
        h.launcher.clone(),
        h.journal.clone(),
        Arc::new(LeaseRenewalBroker::default()),
    );
    let mut command = h.fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1);
    command.spec.size.scratch_disk_mib = u32::MAX;
    assert_eq!(
        runtime.start(command).await,
        CommandResult::SlotUnavailable {
            reason: SlotUnavailableReason::Disk
        }
    );
    wait_until("the refusal to retry deleting the claimed root", || {
        let claimed = claimed.clone();
        async move { !claimed.exists() }
    })
    .await;
}

/// Host admission and wiring only: the launcher records the populated workspace
/// input. Actual ext4 population/boot must be qualified separately with KVM.
/// Copy fallback keeps this test independent of the runner's reflink support.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn block_workload_forwards_one_published_disk_and_guest_source_restore() {
    use g5d_foundation::workspace::{RepoCache, WorkspaceCheckout};

    let temp = tempfile::tempdir().unwrap();
    let fixture = HostFixture::new(temp.path());
    let launcher = Arc::new(FakeLauncher::default());
    let mut config = fixture.config(1);
    config.block_workload = true;
    // Small fixture budget; this does not assert that a real OCI image fits.
    config.scratch_disk_mib = 1024;
    let runtime = WorkspaceVmRuntime::new(
        config,
        launcher.clone(),
        Arc::new(
            WorkspaceCheckout::new(
                RepoCache::new(fixture.cache_root.clone()),
                fixture.workspaces_root.clone(),
            )
            .with_copy_when_reflink_unsupported(true),
        ),
        fixture.journal().await,
        Arc::new(LeaseRenewalBroker::default()),
        Arc::new(crate::runtime::NoOtherSlotUsage),
    );
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let mut command = fixture.start(workspace_id, vm_id, 1);
    command.spec.size.scratch_disk_mib = 1024;
    assert_eq!(runtime.slot_size(), command.spec.size);
    assert_eq!(runtime.start(command).await, CommandResult::Started);
    let launches = launcher.started().await;
    assert_eq!(launches.len(), 1);
    let launched = &launches[0].1;
    assert!(launched.block_workload);
    let disk = launched.scratch_disk.as_ref().expect("one disk budget");
    assert_eq!(disk.size_mib, 1024);
    assert_eq!(launched.mount_guest_dir, "/work");
    assert!(
        launched.guest_restore.is_some(),
        "ordinary boots restore through trusted guest SCM"
    );
    assert!(
        !launched.mount_host_dir.join("app/README.md").exists(),
        "source must not be staged twice on the host"
    );
    assert!(launched.mount_host_dir.join(".g5").is_dir());
    assert!(
        !launched.mount_host_dir.join(".g5/layout.json").exists(),
        "the guest constructs layout from its complete restore specification"
    );
    assert!(launched
        .extra_mounts
        .iter()
        .any(|mount| mount.guest_path == "/opt/g5"
            && mount.access == g5d_isolation::isolation::MountAccess::ReadOnly));
    assert!(matches!(
        runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::Stopped { .. }
    ));
    assert!(
        !launched.mount_host_dir.exists(),
        "released input is claimed after exit proof"
    );
}

#[tokio::test]
async fn block_workload_refuses_unsupported_host_config_before_launch() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = HostFixture::new(temp.path());
    for (process, host_capacity, requested_capacity) in
        [(false, 0, 0), (true, 1024, 1024), (false, 1024, 1025)]
    {
        let launcher = Arc::new(FakeLauncher::default());
        let mut config = fixture.config(1);
        config.block_workload = true;
        config.scratch_disk_mib = host_capacity;
        config.process_host_executable = process.then(|| temp.path().join("g5d"));
        // Refuse zero capacity, process mode and a specification that differs
        // from the published budget, before materializing or reserving a VM.
        let runtime = fixture.runtime_from_config(
            config,
            launcher.clone(),
            fixture.journal().await,
            Arc::new(LeaseRenewalBroker::default()),
        );
        let mut command = fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1);
        command.spec.size.scratch_disk_mib = requested_capacity;
        let root = fixture
            .workspaces_root
            .join(command.workspace_id.to_string());
        assert_eq!(
            runtime.start(command).await,
            CommandResult::SpecRejected {
                reason: if process || !cfg!(target_os = "linux") {
                    SpecRejectReason::RuntimeMismatch
                } else {
                    SpecRejectReason::SizeMismatch
                }
            }
        );
        assert!(launcher.started().await.is_empty());
        assert_eq!(runtime.active_count().await, 0);
        assert!(!root.exists(), "refusal precedes workspace materialization");
    }
}

#[tokio::test]
async fn spec_refusals_name_the_first_failed_check() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = HostFixture::new(temp.path());
    let journal = fixture.journal().await;
    let runtime = fixture.runtime(
        Arc::new(FakeLauncher::default()),
        journal,
        Arc::new(LeaseRenewalBroker::default()),
        1,
    );
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();

    let mut wrong_org = fixture.start(workspace_id, vm_id, 1);
    wrong_org.spec.incarnation.org_id = Uuid::new_v4();
    assert_eq!(
        runtime.start(wrong_org).await,
        CommandResult::SpecRejected {
            reason: SpecRejectReason::OrgMismatch
        }
    );

    let mut wrong_size = fixture.start(workspace_id, vm_id, 1);
    wrong_size.spec.size.mem_mib += 1;
    assert_eq!(
        runtime.start(wrong_size).await,
        CommandResult::SpecRejected {
            reason: SpecRejectReason::SizeMismatch
        }
    );

    let mut secret_env = fixture.start(workspace_id, vm_id, 1);
    secret_env
        .spec
        .env
        .insert("ANTHROPIC_API_KEY".into(), "sk-never".into());
    assert_eq!(
        runtime.start(secret_env).await,
        CommandResult::SpecRejected {
            reason: SpecRejectReason::CredentialInEnv
        }
    );

    let mut tag_only = fixture.start(workspace_id, vm_id, 1);
    tag_only.spec.image.image_ref = "registry.invalid/g5/workspace:latest".into();
    assert_eq!(
        runtime.start(tag_only).await,
        CommandResult::SpecRejected {
            reason: SpecRejectReason::ImageDigestMismatch
        }
    );

    let mut stale_catalog = fixture.start(workspace_id, vm_id, 1);
    stale_catalog.spec.module_catalog_hash = "0".repeat(64);
    assert_eq!(
        runtime.start(stale_catalog).await,
        CommandResult::SpecRejected {
            reason: SpecRejectReason::RuntimeMismatch
        },
        "a guest cannot boot against a catalog the host does not ship"
    );

    let mut no_repos = fixture.start(workspace_id, vm_id, 1);
    no_repos.spec.repos.clear();
    assert_eq!(
        runtime.start(no_repos).await,
        CommandResult::SpecRejected {
            reason: SpecRejectReason::InvalidPath
        }
    );
    assert_eq!(runtime.active_count().await, 0);
    assert_eq!(runtime.boots_in_progress().await, 0);
}

/// A `vm.start` the host has received but not yet reserved is held custody:
/// status and inventory report it as booting, so the control plane never
/// reads absence for a Start that can still land (`g5.runtime.start.settlement`).
#[tokio::test]
async fn a_received_start_is_booting_inventory_before_it_reserves() {
    let Some(h) = harness(2).await else { return };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let command = h.fixture.start(workspace_id, vm_id, 1);

    // Nothing is held before the host dequeues the Start.
    assert_eq!(
        h.runtime
            .status(VmStatus {
                workspace_id,
                vm_id
            })
            .await,
        CommandResult::NotFound
    );

    let guard = h.runtime.register_start(&command).await;
    match h
        .runtime
        .status(VmStatus {
            workspace_id,
            vm_id,
        })
        .await
    {
        CommandResult::Status { vm } => {
            assert_eq!(vm.state, HostVmState::Booting);
            assert_eq!(vm.workspace_id, workspace_id);
            assert_eq!(vm.epoch, 1);
        }
        other => panic!("a received start reports booting, got {other:?}"),
    }
    let inventory = h.runtime.inventory().await;
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].vm_id, vm_id);
    assert_eq!(inventory[0].state, HostVmState::Booting);
    // The complete hello inventory carries the pending Start, but the partial
    // heartbeat does not: a heartbeat id the control plane cannot later clear
    // on exit evidence would leave phantom occupancy (G5-Lean#626, Codex P2).
    assert!(
        !h.runtime
            .heartbeat_vms()
            .await
            .iter()
            .any(|vm| vm.vm_id == vm_id),
        "a pending Start is not advertised in the partial heartbeat"
    );

    // Reserving the slot hands the incarnation to its active record, and the
    // pending entry no longer double-counts it.
    assert_eq!(h.runtime.start(command).await, CommandResult::Started);
    guard.settle().await;
    let inventory = h.runtime.inventory().await;
    assert_eq!(inventory.len(), 1, "no duplicate booting entry survives");
    assert_eq!(inventory[0].vm_id, vm_id);

    assert!(matches!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Graceful))
            .await,
        CommandResult::Stopped { .. }
    ));
    wait_for_root_deletion(&h, workspace_id).await;
}

/// Each execution holds pending custody even if an earlier copy expires
/// without reserving (G5-Lean#999).
#[tokio::test]
async fn a_resent_start_stays_booting_until_its_last_execution_settles() {
    let Some(h) = harness(2).await else { return };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let command = h.fixture.start(workspace_id, vm_id, 1);
    let original = h.runtime.register_start(&command).await;
    let resend = h.runtime.register_start(&command).await;

    // The original expires before reservation while the resend can still run.
    original.settle().await;
    match h
        .runtime
        .status(VmStatus {
            workspace_id,
            vm_id,
        })
        .await
    {
        CommandResult::Status { vm } => {
            assert_eq!(vm.state, HostVmState::Booting);
            assert_eq!(vm.workspace_id, workspace_id);
            assert_eq!(vm.epoch, 1);
        }
        other => panic!("the resend must retain pending custody, got {other:?}"),
    }
    let inventory = h.runtime.inventory().await;
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].vm_id, vm_id);
    assert_eq!(inventory[0].state, HostVmState::Booting);
    assert!(h.runtime.heartbeat_vms().await.is_empty());
    assert_eq!(h.runtime.active_count().await, 0);

    resend.settle().await;
    assert_eq!(
        h.runtime
            .status(VmStatus {
                workspace_id,
                vm_id
            })
            .await,
        CommandResult::NotFound
    );
    assert!(h.runtime.inventory().await.is_empty());
    assert!(h.launcher.started().await.is_empty());
}

#[tokio::test]
async fn settling_a_reserved_start_preserves_a_later_pending_registration() {
    let Some(h) = harness(2).await else { return };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let command = h.fixture.start(workspace_id, vm_id, 1);
    let original = h.runtime.register_start(&command).await;
    let resend = h.runtime.register_start(&command).await;

    assert_eq!(
        h.runtime.start(command.clone()).await,
        CommandResult::Started
    );
    let active_replay = h.runtime.register_start(&command).await;
    resend.settle().await;
    let inventory = h.runtime.inventory().await;
    assert_eq!(inventory.len(), 1, "reservation replaces the pending entry");
    assert_eq!(inventory[0].state, HostVmState::Ready);
    assert!(matches!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::Stopped { .. }
    ));
    wait_for_root_deletion(&h, workspace_id).await;

    // A later request may register this VM id again before the old guards
    // settle. Those guards own no part of the new pending entry.
    let later = h.runtime.register_start(&command).await;
    original.settle().await;
    active_replay.settle().await;
    assert!(matches!(
        h.runtime.status(VmStatus { workspace_id, vm_id }).await,
        CommandResult::Status { vm } if vm.state == HostVmState::Booting
    ));
    assert_eq!(h.runtime.inventory().await.len(), 1);
    later.settle().await;
    assert!(h.runtime.inventory().await.is_empty());
    assert_eq!(h.runtime.active_count().await, 0);
    assert_eq!(h.launcher.started().await.len(), 1);
}

/// A Stop's cancellation covers every pending execution, including one
/// registered after the Stop and one whose original has already settled.
#[tokio::test]
async fn a_resent_start_keeps_stop_cancellation_after_the_original_settles() {
    let Some(h) = harness(2).await else { return };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let command = h.fixture.start(workspace_id, vm_id, 1);
    let original = h.runtime.register_start(&command).await;
    let resend = h.runtime.register_start(&command).await;
    assert_eq!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::NotFound
    );
    original.settle().await;
    let later_resend = h.runtime.register_start(&command).await;

    for execution in [resend, later_resend] {
        assert_eq!(
            h.runtime
                .status(VmStatus {
                    workspace_id,
                    vm_id
                })
                .await,
            CommandResult::NotFound
        );
        assert!(h.runtime.inventory().await.is_empty());
        assert_eq!(
            h.runtime.start(command.clone()).await,
            CommandResult::BootFailed {
                reason: "boot_cancelled".to_string(),
                log_tail: None,
            }
        );
        execution.settle().await;
    }
    assert_eq!(h.runtime.active_count().await, 0);
    assert!(h.launcher.started().await.is_empty());
    assert!(h.journal.pending().await.is_empty());
    assert!(!h
        .fixture
        .workspaces_root
        .join(workspace_id.to_string())
        .exists());
}

/// A stop that arrives while a Start is still pending cancels it before
/// answering absent, so reservation refuses the Start and nothing can admit
/// the incarnation afterwards (`g5.runtime.start.settlement`, G5-Lean#626 D2/H1).
#[tokio::test]
async fn a_stop_cancels_a_pending_start_so_it_can_never_reserve() {
    let Some(h) = harness(2).await else { return };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let command = h.fixture.start(workspace_id, vm_id, 1);
    let guard = h.runtime.register_start(&command).await;

    // The stop finds the Start pending (no active record yet) and reports the
    // incarnation absent after cancelling it.
    assert_eq!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::NotFound
    );
    // A cancelled pending Start leaves no booting inventory for the control
    // plane to see.
    assert!(h.runtime.inventory().await.is_empty());
    assert_eq!(
        h.runtime
            .status(VmStatus {
                workspace_id,
                vm_id
            })
            .await,
        CommandResult::NotFound
    );

    // The Start whose reply the control plane is still awaiting now lands: the
    // host refuses it at reservation instead of booting a released incarnation.
    assert!(matches!(
        h.runtime.start(command).await,
        CommandResult::BootFailed { reason, .. } if reason == "boot_cancelled"
    ));
    guard.settle().await;
    assert_eq!(h.runtime.active_count().await, 0);
    assert!(h.runtime.inventory().await.is_empty());
}

/// Cleanup belongs to the runtime after admission, even when the RPC or
/// reconciliation caller disappears while exit is unproven.
#[tokio::test]
async fn cancelling_a_stop_caller_keeps_cleanup_and_custody_until_proof() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = HostFixture::new(temp.path());
    let launcher = Arc::new(FakeLauncher::default());
    let journal = fixture.journal().await;
    let runtime = fixture.runtime(
        launcher.clone(),
        journal.clone(),
        Arc::new(LeaseRenewalBroker::default()),
        1,
    );
    let h = Harness {
        _temp: temp,
        fixture,
        launcher,
        journal,
        runtime,
    };
    let workspace_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    h.runtime
        .seed_running_vm(h.fixture.start(workspace_id, vm_id, 1).spec)
        .await;
    h.launcher.fail_stops(true);
    let stopping = tokio::spawn({
        let runtime = h.runtime.clone();
        async move {
            runtime
                .stop(stop(workspace_id, vm_id, StopMode::Kill))
                .await
        }
    });
    while h.launcher.stopped().await.is_empty() {
        tokio::task::yield_now().await;
    }
    stopping.abort();
    assert!(stopping.await.unwrap_err().is_cancelled());
    assert_eq!(h.runtime.inventory().await[0].state, HostVmState::Stopping);
    assert!(h
        .fixture
        .workspaces_root
        .join(workspace_id.to_string())
        .is_dir());
    assert!(!h
        .journal
        .pending()
        .await
        .iter()
        .any(|e| matches!(e.kind, VmEventKind::Exited { .. })));
    assert_eq!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::Busy
    );
    h.launcher.fail_stops(false);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !h
            .journal
            .pending()
            .await
            .iter()
            .any(|e| matches!(e.kind, VmEventKind::Exited { .. }))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the runtime still owns the stop after caller cancellation");
    assert_eq!(h.runtime.active_count().await, 0);
    assert_eq!(
        h.journal
            .pending()
            .await
            .iter()
            .filter(|e| matches!(e.kind, VmEventKind::Exited { .. }))
            .count(),
        1
    );
    wait_for_root_deletion(&h, workspace_id).await;
    assert_eq!(
        h.runtime
            .stop(stop(workspace_id, vm_id, StopMode::Kill))
            .await,
        CommandResult::NotFound
    );
}
