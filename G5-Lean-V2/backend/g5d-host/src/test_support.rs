//! Deterministic host fixtures for qualification tests.
//!
//! The real substrate (Cloud Hypervisor, `/dev/kvm`, the pinned `/opt/g5` bundle) is
//! exercised by the Cloud KVM lanes. These helpers let the workspace-VM host
//! be driven through its real admission, custody, journal and control-stream
//! code with a scripted launcher instead, so lifecycle races and refusals are
//! deterministic on any developer machine.

use std::collections::{BTreeMap, HashMap};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use g5_internal_api::host_ctl::{
    StopMode, VmImageSpec, VmIncarnationSpec, VmNetwork, VmRepoSpec, VmSizeSpec, VmSpec, VmStart,
    WorkspaceRootSpec, DISPATCHER_CONTRACT_VERSION, VM_SPEC_VERSION,
};
use g5d_foundation::workspace::{RepoCache, WorkspaceCheckout};
use g5d_isolation::{
    ChildReapReceipt, MicrovmRunSpec, RunHandle, RunResult, TestChildReapPublisher,
};
use sha2::{Digest, Sha256};
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use crate::ctl::{EventJournal, LeaseRenewalBroker};
use crate::runtime::{NoOtherSlotUsage, VmLauncher};
use crate::workspace_vm::{WorkspaceVmRuntime, WorkspaceVmRuntimeConfig};

/// One recorded stop request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopRecord {
    pub handle_id: String,
    pub mode: StopMode,
    pub grace: Duration,
}

#[derive(Default)]
struct FakeLauncherState {
    next_id: u32,
    started: Vec<(String, MicrovmRunSpec)>,
    killed: Vec<String>,
    stopped: Vec<StopRecord>,
    failed_stops: std::collections::HashSet<String>,
    publishers: HashMap<String, TestChildReapPublisher>,
}

/// Scripted [`VmLauncher`]: records every launch and signal, hands out
/// handles whose natural exit a test can publish, and can hold a launch open
/// so stop-during-boot races are reproducible.
pub struct FakeLauncher {
    state: Mutex<FakeLauncherState>,
    hold_starts: watch::Sender<bool>,
    hold_stops: watch::Sender<bool>,
    fail_kills: std::sync::atomic::AtomicBool,
    fail_stops: std::sync::atomic::AtomicBool,
}

impl Default for FakeLauncher {
    fn default() -> Self {
        let (hold_starts, _) = watch::channel(false);
        Self {
            state: Mutex::new(FakeLauncherState::default()),
            hold_starts,
            hold_stops: watch::channel(false).0,
            fail_kills: std::sync::atomic::AtomicBool::new(false),
            fail_stops: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl FakeLauncher {
    /// While held, `start` blocks after the launch is recorded until released.
    pub fn hold_starts(&self, hold: bool) {
        self.hold_starts.send_replace(hold);
    }

    /// While set, `kill` records the signal but cannot prove reap: the VMM
    /// stays running and `kill` fails.
    pub fn fail_kills(&self, fail: bool) {
        self.fail_kills
            .store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn hold_stops(&self, hold: bool) {
        self.hold_stops.send_replace(hold);
    }

    pub fn fail_stops(&self, fail: bool) {
        self.fail_stops
            .store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    pub async fn fail_stop_for(&self, handle_id: &str, fail: bool) {
        let mut state = self.state.lock().await;
        if fail {
            state.failed_stops.insert(handle_id.to_string());
        } else {
            state.failed_stops.remove(handle_id);
        }
    }

    pub async fn started(&self) -> Vec<(String, MicrovmRunSpec)> {
        self.state.lock().await.started.clone()
    }

    pub async fn killed(&self) -> Vec<String> {
        self.state.lock().await.killed.clone()
    }

    pub async fn stopped(&self) -> Vec<StopRecord> {
        self.state.lock().await.stopped.clone()
    }

    /// Reap publisher for a launched handle, so a test can report a natural
    /// VMM exit.
    pub async fn reap_publisher(&self, handle_id: &str) -> Option<TestChildReapPublisher> {
        self.state.lock().await.publishers.get(handle_id).cloned()
    }

    /// Handle id of the most recent launch.
    pub async fn last_handle_id(&self) -> Option<String> {
        self.state
            .lock()
            .await
            .started
            .last()
            .map(|(id, _)| id.clone())
    }

    async fn wait_released(&self) {
        let mut released = self.hold_starts.subscribe();
        while *released.borrow() {
            if released.changed().await.is_err() {
                return;
            }
        }
    }
}

#[async_trait]
impl VmLauncher for FakeLauncher {
    async fn run(&self, _spec: MicrovmRunSpec) -> Result<RunResult> {
        Err(anyhow!("workspace hosts never use one-shot runs"))
    }

    async fn start(&self, spec: MicrovmRunSpec) -> Result<RunHandle> {
        let id = {
            let mut state = self.state.lock().await;
            state.next_id += 1;
            let id = format!("fake-vm-{}", state.next_id);
            state.started.push((id.clone(), spec));
            id
        };
        self.wait_released().await;
        let (handle, publisher) = RunHandle::test_microvm_with_reap_publisher(
            id.clone(),
            Some(Ipv4Addr::LOCALHOST),
            Some(4242),
        );
        self.state.lock().await.publishers.insert(id, publisher);
        Ok(handle)
    }

    async fn kill(&self, handle: &RunHandle) -> Result<ChildReapReceipt> {
        let mut state = self.state.lock().await;
        state.killed.push(handle.id.clone());
        if self.fail_kills.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(anyhow!("scripted kill could not confirm reap"));
        }
        if let Some(publisher) = state.publishers.get(&handle.id) {
            publisher.publish_exit(None);
        }
        Ok(ChildReapReceipt::confirmed_for_test(None))
    }

    async fn stop(
        &self,
        handle: &RunHandle,
        mode: StopMode,
        grace: Duration,
    ) -> Result<ChildReapReceipt> {
        let mut state = self.state.lock().await;
        let code = match mode {
            StopMode::Graceful => Some(0),
            StopMode::Kill => None,
        };
        state.stopped.push(StopRecord {
            handle_id: handle.id.clone(),
            mode,
            grace,
        });
        if self.fail_stops.load(std::sync::atomic::Ordering::SeqCst)
            || state.failed_stops.contains(&handle.id)
        {
            return Err(anyhow!("scripted stop could not confirm reap"));
        }
        let publisher = state.publishers.get(&handle.id).cloned();
        drop(state);
        let mut held = self.hold_stops.subscribe();
        while *held.borrow_and_update() {
            if held.changed().await.is_err() {
                break;
            }
        }
        if let Some(publisher) = publisher {
            publisher.publish_exit(code);
        }
        Ok(ChildReapReceipt::confirmed_for_test(code))
    }
}

/// A committed single-repository fixture plus the runtime layout a host
/// requires, rooted under one test-owned directory.
pub struct HostFixture {
    pub root: PathBuf,
    pub workspaces_root: PathBuf,
    pub runtime_root: PathBuf,
    pub cache_root: PathBuf,
    pub org_id: Uuid,
    pub owner_user_id: Uuid,
    pub repo_id: Uuid,
    pub bare_repo: PathBuf,
    pub branch_oid: String,
    pub catalog_version: String,
    pub catalog_hash: String,
}

pub const FIXTURE_BRANCH: &str = "shared";
pub const FIXTURE_CPUS: u8 = 2;
pub const FIXTURE_MEM_MIB: u32 = 4096;

impl HostFixture {
    /// Build the fixture under `root`, which the caller owns.
    pub fn new(root: &Path) -> Self {
        let source = root.join("source");
        let bare_repo = root.join("origin.git");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-q", "-b", FIXTURE_BRANCH]);
        std::fs::write(source.join("README.md"), b"# fixture\n").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-q", "-m", "fixture"]);
        let branch_oid = git(&source, &["rev-parse", "HEAD"]);
        std::fs::create_dir_all(&bare_repo).unwrap();
        git(&bare_repo, &["init", "-q", "--bare"]);
        git(
            &source,
            &["remote", "add", "origin", bare_repo.to_str().unwrap()],
        );
        git(&source, &["push", "-q", "origin", FIXTURE_BRANCH]);

        let runtime_root = root.join("runtime");
        std::fs::create_dir_all(runtime_root.join("modules")).unwrap();
        std::fs::create_dir_all(runtime_root.join("bin")).unwrap();
        let catalog = br#"{"catalog_version":"fixture-v1","modules":[]}"#;
        std::fs::write(runtime_root.join("modules/catalog.json"), catalog).unwrap();
        let catalog_hash = format!("{:x}", Sha256::digest(catalog));
        std::fs::write(
            runtime_root.join("modules/catalog.sha256"),
            format!("{catalog_hash}\n"),
        )
        .unwrap();

        let workspaces_root = root.join("workspaces");
        let cache_root = root.join("cache");
        std::fs::create_dir_all(&workspaces_root).unwrap();
        Self {
            root: root.to_path_buf(),
            workspaces_root,
            runtime_root,
            cache_root,
            org_id: Uuid::new_v4(),
            owner_user_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
            bare_repo,
            branch_oid,
            catalog_version: "fixture-v1".to_string(),
            catalog_hash,
        }
    }

    pub fn config(&self, slots_total: u32) -> WorkspaceVmRuntimeConfig {
        WorkspaceVmRuntimeConfig {
            org_id: self.org_id,
            slots_total,
            cpus_per_vm: FIXTURE_CPUS,
            mem_mib_per_vm: FIXTURE_MEM_MIB,
            scratch_disk_mib: 0,
            block_workload: false,
            nested_virtualization: false,
            api_url: "http://api.invalid".to_string(),

            runtime_root: self.runtime_root.clone(),
            workspaces_root: self.workspaces_root.clone(),
            // No dispatcher listens behind the scripted launcher.
            dispatcher_ready_timeout: Duration::ZERO,
            process_host_executable: None,
        }
    }

    /// Build a runtime from an explicit config (for example a cluster-local
    /// control-plane URL with a trusted alias).
    pub fn runtime_from_config(
        &self,
        config: WorkspaceVmRuntimeConfig,
        launcher: Arc<dyn VmLauncher>,
        journal: Arc<EventJournal>,
        lease_renewals: Arc<LeaseRenewalBroker>,
    ) -> Arc<WorkspaceVmRuntime> {
        Arc::new(WorkspaceVmRuntime::new(
            config,
            launcher,
            Arc::new(WorkspaceCheckout::new(
                RepoCache::new(self.cache_root.clone()),
                self.workspaces_root.clone(),
            )),
            journal,
            lease_renewals,
            Arc::new(NoOtherSlotUsage),
        ))
    }

    pub async fn journal(&self) -> Arc<EventJournal> {
        Arc::new(
            EventJournal::open(self.root.join("state/host-control/journal.json"))
                .await
                .unwrap(),
        )
    }

    pub fn runtime(
        &self,
        launcher: Arc<dyn VmLauncher>,
        journal: Arc<EventJournal>,
        lease_renewals: Arc<LeaseRenewalBroker>,
        slots_total: u32,
    ) -> Arc<WorkspaceVmRuntime> {
        self.runtime_from_config(self.config(slots_total), launcher, journal, lease_renewals)
    }

    /// A spec the fixture host accepts as-is.
    pub fn spec(&self, workspace_id: Uuid, vm_id: Uuid, epoch: u64) -> VmSpec {
        VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: DISPATCHER_CONTRACT_VERSION,
            incarnation: VmIncarnationSpec {
                workspace_id,
                vm_id,
                epoch,
                org_id: self.org_id,
                owner_user_id: self.owner_user_id,
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: VmImageSpec {
                image_ref: format!("registry.invalid/g5/workspace@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: VmSizeSpec {
                cpus: FIXTURE_CPUS,
                mem_mib: FIXTURE_MEM_MIB,
                scratch_disk_mib: 0,
            },
            workspace_root: WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: vec![VmRepoSpec {
                repo_id: self.repo_id,
                root_path: "app".to_string(),
                role: None,
                git_url: self.bare_repo.to_string_lossy().into_owned(),
                branch: FIXTURE_BRANCH.to_string(),
                branch_oid: self.branch_oid.clone(),
                upstream_branch: Some("main".to_string()),
                follow_branch: false,
            }],
            reference_repos: Vec::new(),
            restore: None,
            env: BTreeMap::new(),
            network: VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".to_string(), "dispatch".to_string()],
            module_catalog_version: self.catalog_version.clone(),
            module_catalog_hash: self.catalog_hash.clone(),
        }
    }

    pub fn start(&self, workspace_id: Uuid, vm_id: Uuid, epoch: u64) -> VmStart {
        VmStart {
            workspace_id,
            vm_id,
            spec: self.spec(workspace_id, vm_id, epoch),
            workspace_token: format!("incarnation-token-{vm_id}"),
        }
    }
}

/// Run git in a fixture directory with a hermetic identity.
pub fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Host Fixture")
        .env("GIT_AUTHOR_EMAIL", "host-fixture@example.test")
        .env("GIT_COMMITTER_NAME", "Host Fixture")
        .env("GIT_COMMITTER_EMAIL", "host-fixture@example.test")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// True when this machine can materialize a workspace root: the checkout
/// materializer requires reflink (Linux) or clonefile (macOS APFS) support.
/// Tests call this once and skip loudly instead of counting a non-run as a
/// pass.
pub async fn materializer_available(fixture: &HostFixture) -> bool {
    let probe = fixture.root.join("materializer-probe");
    let checkout =
        WorkspaceCheckout::new(RepoCache::new(fixture.cache_root.clone()), probe.clone());
    let repos = vec![g5_contracts::types::RepoInfo {
        repo_id: fixture.repo_id.to_string(),
        name: fixture.repo_id.to_string(),
        root_path: Some("app".to_string()),
        root_paths: Vec::new(),
        repo_role: g5_contracts::types::RepoRole::Code,
        git_url: Some(fixture.bare_repo.to_string_lossy().into_owned()),
        branch_name: Some(FIXTURE_BRANCH.to_string()),
        base_commit: Some(fixture.branch_oid.clone()),
    }];
    match checkout
        .materialize_with_git_token(Uuid::new_v4(), &repos, "probe-token")
        .await
    {
        Ok(_) => true,
        Err(error) if format!("{error:#}").contains("reflinks unsupported") => {
            eprintln!("SKIP: workspace materializer unavailable here: {error:#}");
            false
        }
        Err(error) => panic!("materializer probe failed: {error:#}"),
    }
}
