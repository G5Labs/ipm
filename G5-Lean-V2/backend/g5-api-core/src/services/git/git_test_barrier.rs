//! Deterministic integration-test rendezvous at real Git capture/landing boundaries.
//! Compiled only for test-helper builds; no production lock or scheduling policy.
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::Notify;
use uuid::Uuid;

#[derive(Clone, Hash, PartialEq, Eq)]
enum Key {
    Read(Uuid, String),
}
struct Gate {
    reached: Notify,
    released: Mutex<bool>,
    wake: Condvar,
}
fn gates() -> &'static Mutex<HashMap<Key, Arc<Gate>>> {
    static GATES: OnceLock<Mutex<HashMap<Key, Arc<Gate>>>> = OnceLock::new();
    GATES.get_or_init(Default::default)
}
/// Dropping a test's handle always releases a paused operation, including on panic.
pub struct GitTestBarrier {
    key: Key,
    gate: Arc<Gate>,
}
impl GitTestBarrier {
    fn install(key: Key) -> Self {
        let gate = Arc::new(Gate {
            reached: Notify::new(),
            released: Mutex::new(false),
            wake: Condvar::new(),
        });
        assert!(gates()
            .lock()
            .unwrap()
            .insert(key.clone(), gate.clone())
            .is_none());
        Self { key, gate }
    }
    pub fn after_member_read(repo_id: Uuid, branch: &str) -> Self {
        Self::install(Key::Read(repo_id, branch.to_owned()))
    }
    pub async fn reached(&self) {
        tokio::time::timeout(Duration::from_secs(20), self.gate.reached.notified())
            .await
            .expect("Git operation did not reach the test rendezvous");
    }
    pub fn release(&self) {
        *self.gate.released.lock().unwrap() = true;
        self.gate.wake.notify_all();
    }
}
impl Drop for GitTestBarrier {
    fn drop(&mut self) {
        self.release();
        gates().lock().unwrap().remove(&self.key);
    }
}
fn wait(key: Key) {
    let gate = gates().lock().unwrap().remove(&key);
    if let Some(gate) = gate {
        gate.reached.notify_one();
        let (released, _) = gate
            .wake
            .wait_timeout_while(
                gate.released.lock().unwrap(),
                Duration::from_secs(30),
                |released| !*released,
            )
            .unwrap();
        assert!(*released, "Git test rendezvous timed out");
    }
}
pub(crate) fn after_read(repo_id: Uuid, branch: &str) {
    wait(Key::Read(repo_id, branch.to_owned()));
}

/// Test-only pauses in a landing, keyed by workspace and stage: after a cut is
/// prepared and before it lands (clone merges and remote-integration worker
/// landings), and after a landing's refs move but before its database commit.
pub(crate) mod clone_merge_test_barrier {
    use std::collections::HashMap;
    use std::sync::{Arc, LazyLock, Mutex};

    use tokio::sync::Semaphore;
    use uuid::Uuid;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Stage {
        Prepared,
        RefsApplied,
    }

    struct BarrierState {
        prepared: Semaphore,
        release: Semaphore,
    }

    type Barriers = HashMap<(Uuid, Stage), Arc<BarrierState>>;
    static BARRIERS: LazyLock<Mutex<Barriers>> = LazyLock::new(|| Mutex::new(HashMap::new()));

    pub struct CloneMergePreparationBarrier {
        key: (Uuid, Stage),
        state: Arc<BarrierState>,
    }

    pub fn install(workspace_id: Uuid) -> CloneMergePreparationBarrier {
        install_at(workspace_id, Stage::Prepared)
    }

    pub fn install_refs_applied(workspace_id: Uuid) -> CloneMergePreparationBarrier {
        install_at(workspace_id, Stage::RefsApplied)
    }

    fn install_at(workspace_id: Uuid, stage: Stage) -> CloneMergePreparationBarrier {
        let state = Arc::new(BarrierState {
            prepared: Semaphore::new(0),
            release: Semaphore::new(0),
        });
        let key = (workspace_id, stage);
        let previous = BARRIERS
            .lock()
            .expect("landing test barriers remain available")
            .insert(key, Arc::clone(&state));
        assert!(previous.is_none(), "duplicate landing test barrier");
        CloneMergePreparationBarrier { key, state }
    }

    impl CloneMergePreparationBarrier {
        pub async fn wait_until_prepared(&self) {
            self.state
                .prepared
                .acquire()
                .await
                .expect("landing test barrier remains open")
                .forget();
        }

        pub fn release(&self) {
            self.state.release.add_permits(1);
        }
    }

    impl Drop for CloneMergePreparationBarrier {
        fn drop(&mut self) {
            BARRIERS
                .lock()
                .expect("landing test barriers remain available")
                .remove(&self.key);
            self.state.release.add_permits(1);
        }
    }

    pub async fn wait(workspace_id: Uuid) {
        wait_at(workspace_id, Stage::Prepared).await;
    }

    pub async fn wait_at(workspace_id: Uuid, stage: Stage) {
        let state = BARRIERS
            .lock()
            .expect("landing test barriers remain available")
            .get(&(workspace_id, stage))
            .cloned();
        if let Some(state) = state {
            state.prepared.add_permits(1);
            state
                .release
                .acquire()
                .await
                .expect("landing test barrier remains open")
                .forget();
        }
    }
}
