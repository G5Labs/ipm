//! Local, credential-free ref refresh for workspace agents.
//! The socket is owned by the runtime's module identity. Requests cannot name
//! another workspace, URL, arbitrary command, credential or Git refspec.

use std::{sync::Arc, time::Duration};

use g5_contracts::workspace_git::LocalGitRequest;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::modules::ModuleRuntime;

const REQUEST_LIMIT: u64 = 4096;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(330);

pub async fn bind(rt: &ModuleRuntime) -> anyhow::Result<UnixListener> {
    use std::os::unix::fs::PermissionsExt;
    let path = rt.config.paths.workspace_git_socket();
    // This is a dispatcher-owned directory, never a repository path.
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    if let Some(uid) = rt.config.module_uid {
        std::os::unix::fs::chown(&path, Some(uid), rt.config.module_gid)?;
    }
    Ok(listener)
}

pub async fn serve(listener: UnixListener, rt: Arc<ModuleRuntime>) {
    let mut clients = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept(), if clients.len() < 8 => {
                let Ok((stream, _)) = accepted else { break; };
                let rt = rt.clone();
                clients.spawn(async move {
                    let _ = tokio::time::timeout(OPERATION_TIMEOUT, handle(stream, rt)).await;
                });
            }
            _ = clients.join_next(), if !clients.is_empty() => {},
        }
    }
}

async fn handle(stream: UnixStream, rt: Arc<ModuleRuntime>) -> anyhow::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read.take(REQUEST_LIMIT + 1));
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), reader.read_until(b'\n', &mut bytes)).await??;
    let response = if bytes.len() as u64 > REQUEST_LIMIT || bytes.last() != Some(&b'\n') {
        json!({"status":"failed","detail":{"reason":"INVALID_REQUEST"}})
    } else {
        match serde_json::from_slice::<LocalGitRequest>(&bytes) {
            Ok(request) => {
                // Settlement owns its lifetime. A disconnected or timed-out CLI
                // must not leave an unconsumed local operation in the journal.
                tokio::spawn(async move { execute(&rt, request).await }).await?
            }
            Err(_) => json!({"status":"failed","detail":{"reason":"INVALID_REQUEST"}}),
        }
    };
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    write.write_all(&bytes).await?;
    Ok(())
}

pub async fn execute(rt: &Arc<ModuleRuntime>, request: LocalGitRequest) -> Value {
    let (operation, input) = match request {
        LocalGitRequest::Refresh { repo_id } => ("refresh", json!({"repo_id":repo_id})),
    };
    let id = Uuid::new_v4();
    let exec_id = format!("operation.{id}.1.{}", rt.config.vm_id);
    let disposition = rt
        .invoke(
            &json!({
                "exec_id":exec_id,"module_id":"workspace.changes","version":"1.0.0",
                "operation":operation,"input":input,
                "grant":{"grant_id":id,"capabilities":["workspace.read","workspace.native_git"]},
            }),
            Some(rt.config.owner_user_id),
        )
        .await;
    if let Err(error) = disposition {
        return json!({"status":"failed","detail":error.value()});
    }
    loop {
        match rt.status(&json!({"exec_id":exec_id})) {
            Ok(value) if value["status"] == "finished" => {
                // This local caller owns the operation outcome; the control
                // plane has no task to acknowledge on its behalf.
                // Replay serves bounded pages; consume them all before the
                // acknowledgement so no retained event is left behind.
                let mut from_seq = 1_u64;
                loop {
                    match rt.replay(&json!({"exec_id":exec_id,"from_seq":from_seq})) {
                        Ok(events) => {
                            if let Some(seq) = events["events"]
                                .as_array()
                                .and_then(|events| events.last())
                                .and_then(|event| event["seq"].as_u64())
                            {
                                if let Err(error) =
                                    rt.ack(&json!({"exec_id":exec_id,"up_to_seq":seq}))
                                {
                                    tracing::warn!(%exec_id, error = ?error, "Local Git outcome acknowledgement failed");
                                    break;
                                }
                            }
                            if events["has_more"].as_bool() != Some(true) {
                                break;
                            }
                            from_seq = events["next_seq"].as_u64().unwrap_or(from_seq);
                        }
                        Err(error) => {
                            tracing::warn!(%exec_id, error = ?error, "Local Git outcome replay failed");
                            break;
                        }
                    }
                }
                return value["outcome"].clone();
            }
            Ok(value) if value["status"] == "running" => {}
            Ok(value) => return json!({"status":"failed","detail":value}),
            Err(error) => return json!({"status":"failed","detail":error.value()}),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Failed delivery is retried without waking or modifying a workspace.
/// Agent launch also performs a synchronous refresh before handing it inputs.
/// Sync agent memory after code pushes (`g5.personal.memory-sync`). The relay
/// keeps one wake-up while a sync runs, so a burst of pushes costs at most one
/// more sync.
pub async fn memory_sync_loop(rt: Arc<ModuleRuntime>, pushes: Arc<tokio::sync::Notify>) {
    if rt.config.layout.personal_repository().is_none() {
        return;
    }
    loop {
        pushes.notified().await;
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        let cancel = tokio_util::sync::CancellationToken::new();
        match crate::modules::builtin::sync_personal_memory(&rt, &cancel, Some(deadline)).await {
            Ok(report) => tracing::info!(%report, "personal memory sync"),
            Err(_) => tracing::info!("personal memory sync stopped before it finished"),
        }
    }
}

pub async fn refresh_loop(rt: Arc<ModuleRuntime>) {
    let period = Duration::from_secs(30);
    let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let result = execute(&rt, LocalGitRequest::Refresh { repo_id: None }).await;
        if result["status"] != "succeeded" {
            tracing::debug!("Workspace Git ref delivery deferred; runtime will retry");
        }
    }
}
