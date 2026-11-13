//! Private live-input transport. Module stdin acknowledgement is insufficient:
//! only a journaled worker event proves a write to Claude's own stdin.
use super::*;

pub(super) struct SteerRecord {
    binding: Value,
    digest: String,
    disposition: &'static str,
}

impl ModuleRuntime {
    pub async fn steer(&self, params: &Value, idempotency_key: &str) -> RpcResult {
        let exec_id = parse_exec_id(params, &self.config)?;
        let task_id = string_field(params, "task_id", 64)?;
        let attempt = params["attempt"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| RpcError::new(-32602, "INVALID_ATTEMPT"))?;
        if exec_id.kind() != ExecKind::Task
            || exec_id.as_str() != format!("task.{task_id}.{attempt}.{}", self.config.vm_id)
        {
            return Err(RpcError::new(-32602, "INVALID_EXEC_ID"));
        }
        let input_id = string_field(params, "input_id", 36)?;
        let reservation_id = string_field(params, "reservation_id", 36)?;
        if uuid::Uuid::parse_str(&input_id).is_err()
            || uuid::Uuid::parse_str(&reservation_id).is_err()
        {
            return Err(RpcError::new(-32602, "INVALID_INPUT_ID"));
        }
        let revision = params["revision"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .ok_or_else(|| RpcError::new(-32602, "INVALID_REVISION"))?;
        let generation = params["delivery_generation"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .ok_or_else(|| RpcError::new(-32602, "INVALID_GENERATION"))?;
        let request_id = format!("{input_id}:{generation}");
        if idempotency_key != request_id {
            return Err(RpcError::new(-32602, "INVALID_IDEMPOTENCY_KEY"));
        }
        // Preserve every byte, including leading/trailing whitespace.
        let content = params["content"]
            .as_str()
            .filter(|text| text.len() <= 512 * 1024)
            .ok_or_else(|| RpcError::new(-32602, "INVALID_CONTENT"))?;
        let binding = json!({"request_id":request_id,"input_id":input_id,
            "reservation_id":reservation_id,"revision":revision,"delivery_generation":generation});
        let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
        let guard = self.delivery.clone().lock_owned().await;
        let mut receiver = self.events.subscribe();
        let control = {
            let mut executions = self.executions.lock().unwrap();
            if let Some(execution) = executions.get_mut(&exec_id) {
                if let Some(previous) = execution.steers.get(&request_id) {
                    if previous.binding != binding || previous.digest != digest {
                        return Err(RpcError::new(409, "INPUT_CONFLICT"));
                    }
                    return Ok(json!({"disposition":previous.disposition,"request_id":request_id}));
                }
                if execution.module_id != "agent.turn" {
                    return Ok(json!({"disposition":"not_steerable","request_id":request_id}));
                }
                match &execution.state {
                    ExecState::Running { control, .. } if execution.accepts_steer => {
                        let control = control.clone();
                        execution.steers.insert(
                            request_id.clone(),
                            SteerRecord {
                                binding: binding.clone(),
                                digest: digest.clone(),
                                disposition: "pending",
                            },
                        );
                        Some(control)
                    }
                    ExecState::Running { .. } => {
                        return Ok(json!({"disposition":"not_steerable","request_id":request_id}));
                    }
                    ExecState::Finished { .. } if execution.accepts_steer => {
                        // This process retained every admission; an unseen
                        // key was never forwarded during this execution.
                        return Ok(json!({"disposition":"not_steerable","request_id":request_id}));
                    }
                    ExecState::Finished { .. } => None,
                }
            } else {
                None
            }
        };
        let Some(control) = control else {
            // A restarted/evicted execution must never silently redeliver a
            // previously reserved request. Recover its positive evidence.
            let events = self
                .journal
                .replay_retained(&exec_id)
                .map_err(journal_error)?;
            // A pruned prefix may contain the reservation/write evidence.
            // Its absence cannot authorize another delivery generation.
            let mut disposition = "pending";
            for event in events {
                if event.payload["request_id"] != request_id {
                    continue;
                }
                let kind = event.kind.strip_prefix("module.").unwrap_or(&event.kind);
                if kind == "input.steer_reserved" {
                    if event.payload["content_digest"] != digest
                        || !binding
                            .as_object()
                            .unwrap()
                            .iter()
                            .all(|(key, value)| event.payload[key] == *value)
                    {
                        return Err(RpcError::new(409, "INPUT_CONFLICT"));
                    }
                    disposition = "pending";
                }
                match kind {
                    "input.shipped" => disposition = "shipped",
                    "input.steer_rejected" => disposition = "not_steerable",
                    _ => {}
                }
            }
            return Ok(json!({"disposition":disposition,"request_id":request_id}));
        };
        let mut reserved = binding.clone();
        reserved["content_digest"] = json!(digest);
        self.emit(&exec_id, "input.steer_reserved", reserved, false)?;
        let mut line = binding.clone();
        line["kind"] = json!("input.steer");
        line["content"] = json!(content);
        let runtime = self.clone_handle();
        let request = request_id.clone();
        let exec = exec_id.clone();
        let (done_tx, done_rx) = oneshot::channel();
        // Own the delivery beyond any caller cancellation or RPC timeout.
        tokio::spawn(async move {
            let _guard = guard;
            let (ack_tx, ack_rx) = oneshot::channel();
            let queued = control
                .send(ControlMessage::Steer { line, ack: ack_tx })
                .await
                .is_ok();
            if !queued {
                let _ = runtime.emit(&exec, "input.steer_rejected", binding.clone(), false);
            } else if matches!(
                tokio::time::timeout(DELIVERY_ACK_TIMEOUT * 2, ack_rx).await,
                Ok(Ok(true))
            ) {
                let _ = tokio::time::timeout(DELIVERY_ACK_TIMEOUT * 2, async {
                    loop {
                        if runtime.steer_disposition(&exec, &request) != "pending" {
                            break;
                        }
                        if matches!(
                            receiver.recv().await,
                            Err(broadcast::error::RecvError::Closed)
                        ) {
                            break;
                        }
                    }
                })
                .await;
            }
            let disposition = runtime.steer_disposition(&exec, &request);
            let _ = done_tx.send(disposition);
        });
        let disposition = done_rx.await.unwrap_or("pending");
        Ok(json!({"disposition":disposition,"request_id":request_id}))
    }

    fn steer_disposition(&self, exec: &ExecId, request_id: &str) -> &'static str {
        self.executions
            .lock()
            .unwrap()
            .get(exec)
            .and_then(|e| e.steers.get(request_id))
            .map_or("pending", |entry| entry.disposition)
    }

    /// Called only after append succeeds, before broadcasting the same event.
    pub(super) fn observe_steering(&self, exec: &ExecId, event: &JournalEvent) {
        let disposition = match event.kind.strip_prefix("module.").unwrap_or(&event.kind) {
            "input.shipped" => "shipped",
            "input.steer_rejected" => "not_steerable",
            _ => return,
        };
        let Some(id) = event.payload["request_id"].as_str() else {
            return;
        };
        let mut executions = self.executions.lock().unwrap();
        let Some(entry) = executions.get_mut(exec).and_then(|e| e.steers.get_mut(id)) else {
            return;
        };
        if entry
            .binding
            .as_object()
            .unwrap()
            .iter()
            .all(|(key, value)| event.payload[key] == *value)
            && entry.disposition == "pending"
        {
            entry.disposition = disposition;
        }
    }
}
