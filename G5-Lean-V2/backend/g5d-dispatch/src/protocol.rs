//! Private facade envelope and admission barrier. The host authenticates the
//! facade transport; this guest checks the asserted owner and incarnation again.
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use g5_internal_api::stream_mux::MAX_LIVE_STREAMS;

use crate::config::DispatchConfig;

#[derive(Clone, Debug)]
pub struct RpcError {
    pub code: i64,
    pub name: &'static str,
    pub data: Value,
}
impl RpcError {
    pub fn new(code: i64, name: &'static str) -> Self {
        Self {
            code,
            name,
            data: Value::Null,
        }
    }
    pub fn value(&self) -> Value {
        json!({"code":self.code,"message":self.name,"data":self.data})
    }
}
pub type RpcResult = Result<Value, RpcError>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    workspace_id: uuid::Uuid,
    expected_vm_id: uuid::Uuid,
    expected_epoch: u64,
    actor_user_id: Option<uuid::Uuid>,
    /// Only the trusted facade may assert this private lifecycle authority.
    lifecycle: Option<bool>,
    /// A method-scoped operator grant already checked by the trusted facade.
    inspection_grant_id: Option<uuid::Uuid>,
    pub deadline_ms: u64,
    pub idempotency_key: String,
}
impl Context {
    /// The actor the facade authenticated for a public operation, if any.
    pub fn actor_user_id(&self) -> Option<uuid::Uuid> {
        self.actor_user_id
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    /// The method-level authority rule, separate from incarnation and deadline
    /// validation so a facade-side test peer can apply exactly this rule to
    /// the requests the API sends (the API's own pre-send check is not
    /// method-aware).
    pub fn authorize(&self, method: &str, owner_user_id: uuid::Uuid) -> Result<(), RpcError> {
        // Private control-plane protocol (design v9 §6.6): only the trusted
        // facade/lifecycle service, never an owner session or support grant.
        let private = matches!(
            method,
            "health"
                | "admission.close"
                | "admission.open"
                | "modules.input.respond"
                | "modules.input.steer"
                | "modules.background.end"
                | "events.replay"
                | "events.ack"
                | "outcomes.pending"
                | "workspace.flush"
        );
        let lifecycle_or_owner = matches!(method, "modules.cancel" | "modules.status");
        if private {
            if self.lifecycle != Some(true)
                || self.actor_user_id.is_some()
                || self.inspection_grant_id.is_some()
            {
                return Err(RpcError::new(403, "LIFECYCLE_AUTHORITY_REQUIRED"));
            }
        } else if lifecycle_or_owner {
            let lifecycle = self.lifecycle == Some(true)
                && self.actor_user_id.is_none()
                && self.inspection_grant_id.is_none();
            let owner = self.lifecycle.is_none()
                && self.actor_user_id == Some(owner_user_id)
                && self.inspection_grant_id.is_none();
            if !(lifecycle || owner) {
                return Err(RpcError::new(403, "CANCEL_AUTHORITY_REQUIRED"));
            }
        } else {
            let owner =
                self.actor_user_id == Some(owner_user_id) && self.inspection_grant_id.is_none();
            // A separately scoped operator support grant may read bounded,
            // redacted diagnostics; it never gains module invocation, files,
            // ports or terminal.
            let inspection = matches!(
                method,
                "runtime.inspect.summary"
                    | "modules.logs"
                    | "modules.events"
                    | "modules.history"
                    | "diagnostics.capture"
                    | "diagnostics.probe"
            ) && self.inspection_grant_id.is_some()
                && self.actor_user_id.is_none();
            if self.lifecycle == Some(true) || !(owner || inspection) {
                return Err(RpcError::new(403, "OWNER_REQUIRED"));
            }
        }
        Ok(())
    }

    pub fn validate(
        params: &Value,
        config: &DispatchConfig,
        method: &str,
    ) -> Result<Self, RpcError> {
        let ctx: Self =
            serde_json::from_value(params.get("context").cloned().unwrap_or(Value::Null))
                .map_err(|_| RpcError::new(-32602, "INVALID_CONTEXT"))?;
        if ctx.workspace_id != config.workspace_id
            || ctx.expected_vm_id != config.vm_id
            || ctx.expected_epoch != config.epoch
        {
            return Err(RpcError::new(409, "STALE_INCARNATION"));
        }
        ctx.authorize(method, config.owner_user_id)?;
        if ctx.idempotency_key.is_empty() || ctx.idempotency_key.len() > 256 {
            return Err(RpcError::new(-32602, "INVALID_IDEMPOTENCY_KEY"));
        }
        if ctx.deadline_ms > now_ms().saturating_add(86_400_000) {
            return Err(RpcError::new(-32602, "INVALID_DEADLINE"));
        }
        if ctx.deadline_ms <= now_ms() {
            return Err(RpcError::new(408, "DEADLINE_EXCEEDED"));
        }
        Ok(ctx)
    }
}
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Default)]
struct AdmissionState {
    closed: bool,
    closed_by: Option<String>,
    inflight: usize,
    streams: usize,
    stream_cancel: CancellationToken,
    stream_close_reason: Arc<Mutex<Option<g5d_foundation::terminal::TerminalCloseReason>>>,
}
#[derive(Default)]
pub struct Admission {
    state: Mutex<AdmissionState>,
    drained: Notify,
}
pub struct MutationPermit(Arc<Admission>);
pub struct StreamPermit {
    admission: Arc<Admission>,
    cancel: CancellationToken,
    close_reason: Arc<Mutex<Option<g5d_foundation::terminal::TerminalCloseReason>>>,
}
impl StreamPermit {
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn close_reason(&self) -> g5d_foundation::terminal::TerminalCloseReason {
        self.close_reason
            .lock()
            .unwrap()
            .unwrap_or(g5d_foundation::terminal::TerminalCloseReason::WorkspaceStop)
    }
}
const MAX_INFLIGHT_MUTATIONS: usize = 32;
impl Admission {
    pub fn enter(self: &Arc<Self>) -> Result<MutationPermit, RpcError> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(RpcError::new(409, "WORKSPACE_STOPPING"));
        }
        if state.inflight >= MAX_INFLIGHT_MUTATIONS {
            return Err(RpcError::new(429, "TOO_MANY_REQUESTS"));
        }
        state.inflight += 1;
        Ok(MutationPermit(self.clone()))
    }
    /// A long-lived PTY or preview bridge must drain for stop/transfer, but
    /// must not make a non-interrupting live fork wait for an idle connection.
    pub fn enter_stream(self: &Arc<Self>) -> Result<StreamPermit, RpcError> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(RpcError::new(409, "WORKSPACE_STOPPING"));
        }
        if state.streams >= MAX_LIVE_STREAMS {
            return Err(RpcError::new(429, "TOO_MANY_REQUESTS"));
        }
        state.streams += 1;
        Ok(StreamPermit {
            admission: self.clone(),
            cancel: state.stream_cancel.clone(),
            close_reason: Arc::clone(&state.stream_close_reason),
        })
    }
    pub fn inflight(&self) -> usize {
        self.state.lock().unwrap().inflight
    }
    pub fn active_streams(&self) -> usize {
        self.state.lock().unwrap().streams
    }
    pub fn is_open(&self) -> bool {
        !self.state.lock().unwrap().closed
    }
    pub fn refuse_new(&self) {
        self.refuse_new_for(None);
    }
    pub fn refuse_new_for(&self, operation_id: Option<&str>) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        if let Some(operation_id) = operation_id {
            state.closed_by = Some(operation_id.to_string());
        }
    }
    pub async fn close(&self) {
        self.close_for(None).await;
    }
    pub async fn close_for(&self, operation_id: Option<&str>) {
        self.close_for_reason(
            operation_id,
            g5d_foundation::terminal::TerminalCloseReason::WorkspaceStop,
        )
        .await;
    }
    pub async fn close_for_reason(
        &self,
        operation_id: Option<&str>,
        reason: g5d_foundation::terminal::TerminalCloseReason,
    ) {
        self.close_for_inner(operation_id, true, Some(reason)).await;
    }
    pub async fn close_for_fork(&self, operation_id: Option<&str>) {
        self.close_for_inner(operation_id, false, None).await;
    }
    async fn close_for_inner(
        &self,
        operation_id: Option<&str>,
        drain_streams: bool,
        reason: Option<g5d_foundation::terminal::TerminalCloseReason>,
    ) {
        self.refuse_new_for(operation_id);
        if drain_streams {
            // A scanned preview listener need not belong to a cancellable
            // module or terminal. End its bridge before waiting for permits.
            let state = self.state.lock().unwrap();
            *state.stream_close_reason.lock().unwrap() = reason;
            state.stream_cancel.cancel();
        }
        loop {
            let changed = self.drained.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let drained = {
                let state = self.state.lock().unwrap();
                state.inflight == 0 && (!drain_streams || state.streams == 0)
            };
            if drained {
                return;
            }
            changed.await;
        }
    }
    pub fn open(&self) {
        let _ = self.open_for(None);
    }
    pub fn open_for(&self, operation_id: Option<&str>) -> bool {
        let mut state = self.state.lock().unwrap();
        if operation_id.is_some() && state.closed_by.as_deref() != operation_id {
            return false;
        }
        state.closed = false;
        state.closed_by = None;
        if state.stream_cancel.is_cancelled() {
            state.stream_cancel = CancellationToken::new();
            state.stream_close_reason = Arc::new(Mutex::new(None));
        }
        true
    }
}
impl Drop for MutationPermit {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.inflight -= 1;
        if state.inflight == 0 {
            self.0.drained.notify_waiters();
        }
    }
}
impl Drop for StreamPermit {
    fn drop(&mut self) {
        let mut state = self.admission.state.lock().unwrap();
        state.streams -= 1;
        if state.streams == 0 {
            self.admission.drained.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn close_drains_and_refuses_until_reopened() {
        let admission = Arc::new(Admission::default());
        let permit = admission.enter().unwrap();
        let close = tokio::spawn({
            let a = admission.clone();
            async move {
                a.close().await;
            }
        });
        tokio::task::yield_now().await;
        assert!(!close.is_finished());
        assert!(admission.enter().is_err());
        drop(permit);
        close.await.unwrap();
        assert!(admission.enter().is_err());
        admission.open();
        assert!(admission.enter().is_ok());
    }

    #[tokio::test]
    async fn stale_lifecycle_operation_cannot_reopen_admission() {
        let admission = Admission::default();
        admission.close_for(Some("stop-old")).await;
        admission.refuse_new_for(Some("stop-new"));

        assert!(!admission.open_for(Some("stop-old")));
        assert!(!admission.is_open());
        assert!(admission.open_for(Some("stop-new")));
        assert!(admission.is_open());
    }

    #[tokio::test]
    async fn noninterrupt_fork_drains_mutations_but_not_live_streams() {
        let admission = Arc::new(Admission::default());
        let stream = admission.enter_stream().unwrap();
        let stream_cancel = stream.cancellation();
        let mutation = admission.enter().unwrap();
        let fork = tokio::spawn({
            let admission = admission.clone();
            async move { admission.close_for_fork(Some("fork")).await }
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while admission.is_open() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!fork.is_finished());
        assert!(admission.enter_stream().is_err());
        drop(mutation);
        fork.await.unwrap();
        assert_eq!(admission.inflight(), 0);
        assert!(!admission.is_open());
        assert!(!stream_cancel.is_cancelled());
        assert!(admission.open_for(Some("fork")));
        drop(stream);
    }

    #[tokio::test]
    async fn stop_still_drains_live_streams() {
        let admission = Arc::new(Admission::default());
        let stream = admission.enter_stream().unwrap();
        let stream_cancel = stream.cancellation();
        let stop = tokio::spawn({
            let admission = admission.clone();
            async move { admission.close_for(Some("stop")).await }
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while admission.is_open() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!stop.is_finished());
        assert_eq!(admission.active_streams(), 1);
        assert!(stream_cancel.is_cancelled());
        drop(stream);
        stop.await.unwrap();
        assert_eq!(admission.active_streams(), 0);
        assert!(admission.open_for(Some("stop")));
        assert!(!admission
            .enter_stream()
            .unwrap()
            .cancellation()
            .is_cancelled());
    }

    #[tokio::test]
    async fn interrupted_fork_labels_canceled_streams_and_resets_on_reopen() {
        use g5d_foundation::terminal::TerminalCloseReason;

        let admission = Arc::new(Admission::default());
        let stream = admission.enter_stream().unwrap();
        let interrupt = tokio::spawn({
            let admission = Arc::clone(&admission);
            async move {
                admission
                    .close_for_reason(Some("fork"), TerminalCloseReason::ForkInterrupt)
                    .await;
            }
        });
        stream.cancellation().cancelled().await;
        assert_eq!(stream.close_reason().as_str(), "fork_interrupt");
        drop(stream);
        interrupt.await.unwrap();
        assert!(admission.open_for(Some("fork")));
        let next = admission.enter_stream().unwrap();
        assert!(!next.cancellation().is_cancelled());
    }

    #[test]
    fn long_lived_streams_do_not_consume_mutation_slots() {
        let admission = Arc::new(Admission::default());
        let streams: Vec<_> = (0..MAX_LIVE_STREAMS)
            .map(|_| admission.enter_stream().expect("stream below mux limit"))
            .collect();
        assert!(admission.enter_stream().is_err());
        let mutations: Vec<_> = (0..MAX_INFLIGHT_MUTATIONS)
            .map(|_| {
                admission
                    .enter()
                    .expect("idle streams must not consume mutation capacity")
            })
            .collect();
        assert!(admission.enter().is_err());
        drop(mutations);
        drop(streams);
    }
}
