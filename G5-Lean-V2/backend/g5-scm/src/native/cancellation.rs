use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{watch, Notify};

/// A caller-owned lifetime shared by native operations. Cancellation does not
/// undo accepted writes. `wait_idle` proves owned child cleanup has finished.
#[derive(Clone, Debug)]
pub struct NativeCancellation(Arc<State>);

#[derive(Debug)]
struct State {
    cancelled: watch::Sender<bool>,
    active: AtomicUsize,
    ran: AtomicBool,
    idle: Notify,
}

impl Default for NativeCancellation {
    fn default() -> Self {
        Self(Arc::new(State {
            cancelled: watch::channel(false).0,
            active: AtomicUsize::new(0),
            ran: AtomicBool::new(false),
            idle: Notify::new(),
        }))
    }
}

impl NativeCancellation {
    pub fn cancel(&self) {
        self.0.cancelled.send_replace(true);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.0.cancelled.borrow()
    }

    /// Whether any native operation has started under this lifetime, so an
    /// interruption can say Git effects may have been accepted.
    pub fn has_run(&self) -> bool {
        self.0.ran.load(Ordering::Acquire)
    }

    pub async fn wait_idle(&self) {
        loop {
            let notified = self.0.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.0.active.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    pub(super) async fn cancelled(&self) {
        let mut receiver = self.0.cancelled.subscribe();
        loop {
            if *receiver.borrow_and_update() {
                return;
            }
            receiver
                .changed()
                .await
                .expect("cancellation sender stays owned");
        }
    }

    pub(super) fn started(&self) -> Active {
        self.0.ran.store(true, Ordering::Release);
        self.0.active.fetch_add(1, Ordering::AcqRel);
        Active(self.clone())
    }
}

pub(super) struct Active(NativeCancellation);
impl Drop for Active {
    fn drop(&mut self) {
        if self.0 .0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0 .0.idle.notify_waiters();
        }
    }
}
