//! Wake-ups from hosts' module-event hints for the attempt followers that
//! stream a live attempt's journal (`g5.chat.live-delivery`). Owned by the
//! hosts context: the host control stream delivers the hint, and the later
//! agents context waits on it.
//!
//! A host forwards a content-free `vm.module_events` hint naming the
//! executions whose guest journal grew (`g5.runtime.module-event-hints`).
//! The hint carries no data and is never acknowledged: a follower that is
//! woken fetches the journal, and one that misses a hint fetches on its poll.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Notify;

/// Process-local registry of followers waiting for journal growth, keyed by
/// canonical execution id.
#[derive(Default)]
pub struct LiveHints {
    waiters: DashMap<String, Arc<Notify>>,
}

impl LiveHints {
    /// The notifier a follower of `exec_id` waits on. One notifier per
    /// execution; a hint that arrives before the follower subscribes is kept
    /// as a stored permit, so it cannot be lost between a fetch and a wait.
    pub fn subscribe(&self, exec_id: &str) -> Arc<Notify> {
        self.waiters.entry(exec_id.to_string()).or_default().clone()
    }

    /// Wake the follower of `exec_id`, if any. Unknown executions are
    /// ignored: the hint is advisory and the journal remains authoritative.
    pub fn hint(&self, exec_id: &str) {
        if let Some(notify) = self.waiters.get(exec_id) {
            notify.notify_one();
        }
    }

    /// Forget a finished follower's notifier.
    pub fn release(&self, exec_id: &str) {
        self.waiters.remove(exec_id);
    }

    #[must_use]
    pub fn subscribed(&self, exec_id: &str) -> bool {
        self.waiters.contains_key(exec_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_hint_before_the_wait_is_not_lost() {
        let hints = LiveHints::default();
        let notify = hints.subscribe("task.a");
        hints.hint("task.a");
        tokio::time::timeout(std::time::Duration::from_millis(100), notify.notified())
            .await
            .expect("stored permit wakes the first wait");
    }

    #[tokio::test]
    async fn hints_for_unknown_executions_are_ignored_and_release_forgets() {
        let hints = LiveHints::default();
        hints.hint("task.unknown");
        let _notify = hints.subscribe("task.b");
        assert!(hints.subscribed("task.b"));
        hints.release("task.b");
        assert!(!hints.subscribed("task.b"));
    }
}
