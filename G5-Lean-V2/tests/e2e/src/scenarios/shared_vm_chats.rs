//! Concurrent chat turns through one candidate workspace VM and the public API.

use crate::helpers::api_client::{restart_api_container, TestApiClient};
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use uuid::Uuid;

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

async fn file_content(
    api: &TestApiClient,
    workspace_id: Uuid,
    repo_id: &str,
    path: &str,
) -> anyhow::Result<Option<String>> {
    let response = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{workspace_id}/files/{path}?repoId={repo_id}",
            api.base_url()
        ))
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let status = response.status();
    let body: Value = response.json().await?;
    anyhow::ensure!(
        status.is_success(),
        "file {repo_id}:{path}: {status} {body}"
    );
    Ok(data(&body)["content"].as_str().map(str::to_owned))
}

async fn wait_for_file(
    api: &TestApiClient,
    workspace_id: Uuid,
    repo_id: &str,
    path: &str,
    expected: &str,
) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if file_content(api, workspace_id, repo_id, path)
                .await?
                .as_deref()
                == Some(expected)
            {
                return Ok::<(), anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out reading {repo_id}:{path} as {expected:?}"))?
}

async fn wait_for_runtime_file_after_restart(
    api: &TestApiClient,
    workspace_id: Uuid,
    repo_id: &str,
    path: &str,
    expected: &str,
) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let response = api
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{workspace_id}/files/{path}?repoId={repo_id}",
                    api.base_url()
                ))
                .send()
                .await?;
            if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            let status = response.status();
            let body: Value = response.json().await?;
            anyhow::ensure!(
                status.is_success(),
                "post-restart file read: {status} {body}"
            );
            anyhow::ensure!(
                data(&body)["content"] == expected,
                "post-restart file changed: {body}"
            );
            return Ok::<(), anyhow::Error>(());
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("host tunnel did not reconnect after API restart"))?
}

// The atomic start endpoint resolves the provider for a new conversation.
// The legacy empty-session endpoint always creates a Claude session.
async fn start_for_provider(
    api: &TestApiClient,
    workspace: Uuid,
    title: &str,
    content: &str,
    provider: &str,
) -> anyhow::Result<Value> {
    let content = if provider == "codex" {
        format!("{content} G5_REQUIRE_CODEX_APP_SERVER_E2E")
    } else {
        content.to_owned()
    };
    let response = api
        .authed_post(&format!(
            "{}/api/v1/workspaces/{workspace}/sessions/start",
            api.base_url()
        ))
        .json(
            &json!({"title":title,"content":content,"agentProvider":provider,
            "permissionMode":if provider=="codex" {"never/workspace-write"} else {"default"}}),
        )
        .send()
        .await?;
    let status = response.status();
    let body: Value = response.json().await?;
    anyhow::ensure!(status.is_success(), "chat start failed ({status}): {body}");
    anyhow::ensure!(
        data(&body)["session"]["agentProvider"] == provider,
        "chat start selected the wrong provider: {body}"
    );
    Ok(body)
}

async fn send_for_provider(
    api: &TestApiClient,
    session: &str,
    content: &str,
    provider: &str,
) -> anyhow::Result<Value> {
    let content = if provider == "codex" {
        format!("{content} G5_REQUIRE_CODEX_APP_SERVER_E2E")
    } else {
        content.to_owned()
    };
    let response = api
        .authed_post(&format!(
            "{}/api/v1/sessions/{session}/messages",
            api.base_url()
        ))
        .json(&json!({"content":content,"agentProvider":provider,
            "permissionMode":if provider=="codex" {"never/workspace-write"} else {"default"}}))
        .send()
        .await?;
    let status = response.status();
    let body: Value = response.json().await?;
    anyhow::ensure!(status.is_success(), "chat send failed ({status}): {body}");
    Ok(body)
}

async fn assistant_count(api: &TestApiClient, session_id: &str) -> anyhow::Result<usize> {
    let body = api.get_chat_messages(session_id).await?;
    Ok(data(&body)["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("session messages missing: {body}"))?
        .iter()
        .filter(|message| {
            message["role"] == "assistant"
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("Fake workspace write completed"))
        })
        .count())
}

async fn wait_for_assistant_count(
    api: &TestApiClient,
    session_id: &str,
    expected: usize,
) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let count = assistant_count(api, session_id).await?;
            anyhow::ensure!(
                count <= expected,
                "session {session_id} duplicated an assistant turn: {count} > {expected}"
            );
            if count == expected {
                return Ok::<(), anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("session {session_id} did not reach {expected} replies"))?
}

fn write_set(
    code_root: &str,
    docs_root: &str,
    workspace_id: Uuid,
    token: &str,
    turn: &str,
) -> (String, String, String, String, String) {
    let code_path = format!("e2e-m5-{token}-{turn}.txt");
    let docs_path = format!("docs/{workspace_id}/e2e-m5-{token}-{turn}.md");
    let code_bytes = format!("M5 {token} {turn} code\n");
    let docs_bytes = format!("# M5 {token} {turn} docs\n");
    let writes = json!([
        {
            "path": format!("{code_root}/{code_path}"),
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(&code_bytes),
        },
        {
            "path": format!("{docs_root}/{docs_path}"),
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(&docs_bytes),
        },
    ]);
    let directive = base64::engine::general_purpose::STANDARD.encode(writes.to_string());
    (code_path, docs_path, code_bytes, docs_bytes, directive)
}

async fn release_gate(
    api: &TestApiClient,
    workspace_id: Uuid,
    code_repo_id: &str,
    path: &str,
) -> anyhow::Result<()> {
    let response = api
        .authed_put(&format!(
            "{}/api/v1/workspaces/{workspace_id}/files/{path}?repoId={code_repo_id}",
            api.base_url()
        ))
        .json(&json!({
            "content": "release\n",
            "encoding": "utf-8",
            "expectedHash": null,
        }))
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "release gate {path}: {status} {body}");
    Ok(())
}

async fn assert_one_attempt(api: &TestApiClient, task_id: &str) -> anyhow::Result<()> {
    let body: Value = api
        .authed_get(&format!(
            "{}/api/v1/tasks/{task_id}/attempts",
            api.base_url()
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let attempts = data(&body)["attempts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("task attempts missing: {body}"))?;
    anyhow::ensure!(
        attempts.len() == 1 && attempts[0]["attemptNumber"] == 1,
        "task {task_id} executed more than once: {body}"
    );
    Ok(())
}

async fn task(api: &TestApiClient, task_id: &str) -> anyhow::Result<Value> {
    let body: Value = api
        .authed_get(&format!("{}/api/v1/tasks/{task_id}", api.base_url()))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(data(&body).clone())
}

#[tokio::test]
async fn test_public_chat_image_reaches_synthetic_provider_with_exact_bytes() -> anyhow::Result<()>
{
    use sha2::{Digest, Sha256};

    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let workspace = api.create_workspace(org.id, "image-turn-delivery").await?;
    api.resume_workspace_vm(workspace.id).await?;
    let outcome: anyhow::Result<()> = async {
        let session = api.create_chat_session_with_provider(workspace.id, "Image delivery", "claude_code").await?;
        let session_id = data(&session)["id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("chat session omitted id: {session}"))?;
        let image_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
        let image = base64::engine::general_purpose::STANDARD.decode(image_base64)?;
        let digest = format!("{:x}", Sha256::digest(&image));
        let other_image_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNg+M8AAAICAQB7CYF4AAAAAElFTkSuQmCC";
        let other_image = base64::engine::general_purpose::STANDARD.decode(other_image_base64)?;
        let other_digest = format!("{:x}", Sha256::digest(&other_image));
        let response = api.authed_post(&format!("{}/api/v1/sessions/{session_id}/messages", api.base_url()))
            .json(&json!({
                "content": format!("Inspect this image. G5_IMAGE_SHA256_E2E:{digest}"),
                "permissionMode": "default",
                "attachments": [{
                    "fileName": "pixel.png",
                    "contentType": "image/png",
                    "contentBase64": image_base64,
                }],
            }))
            .send().await?.error_for_status()?;
        let sent: Value = response.json().await?;
        let task_id = data(&sent)["taskId"].as_str()
            .ok_or_else(|| anyhow::anyhow!("image send omitted task id: {sent}"))?;
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let state = task(&api, task_id).await?;
                match state["status"].as_str() {
                    Some("succeeded") => return Ok::<(), anyhow::Error>(()),
                    Some("failed" | "cancelled" | "timed_out") => anyhow::bail!("image turn failed: {state}"),
                    _ => tokio::time::sleep(Duration::from_millis(300)).await,
                }
            }
        }).await.map_err(|_| anyhow::anyhow!("image turn timed out"))??;
        assert_one_attempt(&api, task_id).await?;

        // The next current input has no image. Historical recovery may cite
        // the prior marker, but the fake refuses any replayed image bytes.
        let continuation: Value = api.authed_post(&format!("{}/api/v1/sessions/{session_id}/messages", api.base_url()))
            .json(&json!({"content":"Summarize the prior image without a new attachment.", "permissionMode":"default"}))
            .send().await?.error_for_status()?.json().await?;
        let continuation_task = data(&continuation)["taskId"].as_str()
            .ok_or_else(|| anyhow::anyhow!("continuation omitted task: {continuation}"))?;
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let state = task(&api, continuation_task).await?;
                match state["status"].as_str() {
                    Some("succeeded") => return Ok::<(), anyhow::Error>(()),
                    Some("failed" | "cancelled" | "timed_out") => anyhow::bail!("continuation failed: {state}"),
                    _ => tokio::time::sleep(Duration::from_millis(300)).await,
                }
            }
        }).await.map_err(|_| anyhow::anyhow!("image continuation timed out"))??;

        let fork: Value = api.authed_post(&format!("{}/api/v1/sessions/{session_id}/fork", api.base_url()))
            .json(&json!({"fromMessageIndex": 1, "title": "Image history fork"}))
            .send().await?.error_for_status()?.json().await?;
        let fork_id = data(&fork)["sessionId"].as_str()
            .ok_or_else(|| anyhow::anyhow!("image fork omitted session id: {fork}"))?;
        anyhow::ensure!(data(&fork)["messagesCopied"] == 2, "image fork copied unexpected history: {fork}");
        let fork_history = api.get_chat_messages(fork_id).await?;
        let copied = data(&fork_history)["messages"].as_array()
            .ok_or_else(|| anyhow::anyhow!("image fork omitted messages: {fork_history}"))?;
        anyhow::ensure!(copied.len() == 2, "image fork changed transcript length: {fork_history}");
        anyhow::ensure!(
            copied[0]["artifacts"].as_array().is_some_and(|artifacts| artifacts.iter().any(|artifact| artifact["sha256"] == digest)),
            "image fork lost historical attachment identity: {fork_history}"
        );
        let fork_turn: Value = api.authed_post(&format!("{}/api/v1/sessions/{fork_id}/messages", api.base_url()))
            .json(&json!({"content":"Continue from the copied image history without a new attachment.", "permissionMode":"default"}))
            .send().await?.error_for_status()?.json().await?;
        let fork_task = data(&fork_turn)["taskId"].as_str()
            .ok_or_else(|| anyhow::anyhow!("image fork turn omitted task: {fork_turn}"))?;
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let state = task(&api, fork_task).await?;
                match state["status"].as_str() {
                    Some("succeeded") => return Ok::<(), anyhow::Error>(()),
                    Some("failed" | "cancelled" | "timed_out") => anyhow::bail!("image fork turn failed: {state}"),
                    _ => tokio::time::sleep(Duration::from_millis(300)).await,
                }
            }
        }).await.map_err(|_| anyhow::anyhow!("image fork turn timed out"))??;
        assert_one_attempt(&api, fork_task).await?;

        // A fresh image in the fork must be the only image in this current
        // input; the copied image remains historical context.
        let fork_image_turn: Value = api.authed_post(&format!("{}/api/v1/sessions/{fork_id}/messages", api.base_url()))
            .json(&json!({
                "content": format!("Inspect this new image. G5_IMAGE_SHA256_E2E:{other_digest}"),
                "permissionMode": "default",
                "attachments": [{
                    "fileName": "other-pixel.png",
                    "contentType": "image/png",
                    "contentBase64": other_image_base64,
                }],
            }))
            .send().await?.error_for_status()?.json().await?;
        let fork_image_task = data(&fork_image_turn)["taskId"].as_str()
            .ok_or_else(|| anyhow::anyhow!("image fork input omitted task: {fork_image_turn}"))?;
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let state = task(&api, fork_image_task).await?;
                match state["status"].as_str() {
                    Some("succeeded") => return Ok::<(), anyhow::Error>(()),
                    Some("failed" | "cancelled" | "timed_out") => anyhow::bail!("image fork input failed: {state}"),
                    _ => tokio::time::sleep(Duration::from_millis(300)).await,
                }
            }
        }).await.map_err(|_| anyhow::anyhow!("image fork input timed out"))??;
        assert_one_attempt(&api, fork_image_task).await?;

        let start_url = format!("{}/api/v1/workspaces/{}/sessions/start", api.base_url(), workspace.id);
        let start_body = json!({
            "title": "Image replay identity",
            "content": format!("Inspect this image. G5_IMAGE_SHA256_E2E:{digest}"),
            "agentProvider": "claude_code",
            "permissionMode": "default",
            "requestKey": format!("image-{}", workspace.id),
            "attachments": [{
                "fileName": "pixel.png", "contentType": "IMAGE/PNG", "contentBase64": image_base64,
            }],
        });
        let first: Value = api.authed_post(&start_url).json(&start_body).send().await?.error_for_status()?.json().await?;
        let replay = api.authed_post(&start_url).json(&start_body).send().await?;
        anyhow::ensure!(replay.status().as_u16() == 200, "exact image replay status: {}", replay.status());
        let replay: Value = replay.json().await?;
        anyhow::ensure!(data(&first)["message"]["inputId"] == data(&replay)["message"]["inputId"], "exact image replay changed input: {first} {replay}");
        let mut changed = start_body.clone();
        changed["attachments"][0]["contentBase64"] = json!(other_image_base64);
        let conflict = api.authed_post(&start_url).json(&changed).send().await?;
        let conflict_status = conflict.status();
        let conflict_body: Value = conflict.json().await?;
        anyhow::ensure!(
            conflict_status.as_u16() == 422
                && conflict_body["error"]["details"]["field"] == "requestKey"
                && conflict_body["error"]["details"]["error"] == "already used for a different message",
            "changed image replay did not reject request-key identity: {conflict_status} {conflict_body}"
        );
        let first_task = data(&first)["taskId"].as_str().ok_or_else(|| anyhow::anyhow!("first image start omitted task: {first}"))?;
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let state = task(&api, first_task).await?;
                match state["status"].as_str() {
                    Some("succeeded") => return Ok::<(), anyhow::Error>(()),
                    Some("failed" | "cancelled" | "timed_out") => anyhow::bail!("image start failed: {state}"),
                    _ => tokio::time::sleep(Duration::from_millis(300)).await,
                }
            }
        }).await.map_err(|_| anyhow::anyhow!("image start timed out"))??;
        assert_one_attempt(&api, first_task).await?;
        Ok(())
    }.await;
    let _ = api.stop_workspace_vm(workspace.id).await;
    outcome
}

#[tokio::test]
async fn test_two_chats_and_a_queued_turn_share_one_vm_without_releasing_held_work(
) -> anyhow::Result<()> {
    two_chats_and_a_follow_up_share_one_vm(false).await
}

#[tokio::test]
async fn test_native_claude_steering_is_enabled_without_releasing_another_chat(
) -> anyhow::Result<()> {
    two_chats_and_a_follow_up_share_one_vm(true).await
}

async fn two_chats_and_a_follow_up_share_one_vm(native_steering: bool) -> anyhow::Result<()> {
    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let workspace = api.create_workspace(org.id, "m5-shared-vm-chats").await?;
    api.resume_workspace_vm(workspace.id).await?;
    let vm_before: Value = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{}/vm",
            api.base_url(),
            workspace.id
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let vm_id = data(&vm_before)["vmId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("running workspace omitted VM identity: {vm_before}"))?;
    let workspace_detail = api.get_workspace(workspace.id).await?;
    let docs_repo_id = data(&workspace_detail)["docsRepoId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("workspace omitted docsRepoId: {workspace_detail}"))?;
    let changes: Value = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{}/changes",
            api.base_url(),
            workspace.id
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let members = data(&changes)["members"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("attached member vector missing: {changes}"))?;
    let docs = members
        .iter()
        .find(|member| member["repo_id"] == docs_repo_id)
        .ok_or_else(|| anyhow::anyhow!("docs member missing: {changes}"))?;
    let code = members
        .iter()
        .find(|member| member["repo_id"] != docs_repo_id)
        .ok_or_else(|| anyhow::anyhow!("code member missing: {changes}"))?;
    let code_repo_id = code["repo_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("code member omitted repo_id: {changes}"))?
        .to_owned();
    let code_root = code["root_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("code member omitted root_path: {changes}"))?
        .to_owned();
    let docs_root = docs["root_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("docs member omitted root_path: {changes}"))?
        .to_owned();
    let token = Uuid::new_v4().simple().to_string();
    let gate_a = format!("e2e-m5-{token}-gate-a.txt");
    let gate_b = format!("e2e-m5-{token}-gate-b.txt");
    let session_a = api
        .create_chat_session_with_provider(workspace.id, "M5 held A", "claude_code")
        .await?;
    // Codex retains the original queue journey; every new Claude attempt
    // exercises steering without any organization configuration.
    let b_provider = if native_steering {
        "claude_code"
    } else {
        "codex"
    };
    let session_a_id = data(&session_a)["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("first chat session omitted id: {session_a}"))?
        .to_owned();

    let outcome: anyhow::Result<()> = async {
        let (a_code, a_docs, a_code_bytes, a_docs_bytes, a_writes) =
            write_set(&code_root, &docs_root, workspace.id, &token, "a");
        let a = api
            .send_chat_message(
                &session_a_id,
                &format!(
                    "Hold A after writing. G5_FS_WRITE_SET_E2E:{a_writes} G5_WAIT_FOR_FILE_E2E:{code_root}/{gate_a}"
                ),
                "default",
            )
            .await?;
        let a_task = data(&a)["taskId"].as_str().ok_or_else(|| {
            anyhow::anyhow!("first session did not expose task identity: {a}")
        })?;
        wait_for_file(&api, workspace.id, &code_repo_id, &a_code, &a_code_bytes).await?;
        wait_for_file(&api, workspace.id, docs_repo_id, &a_docs, &a_docs_bytes).await?;
        anyhow::ensure!(assistant_count(&api, &session_a_id).await? == 0, "A was not held");

        let (b_code, b_docs, b_code_bytes, b_docs_bytes, b_writes) =
            write_set(&code_root, &docs_root, workspace.id, &token, "b");
        let b = start_for_provider(&api,
                workspace.id,
                "M5 held B",
                &format!(
                    "Hold B after writing. G5_FS_WRITE_SET_E2E:{b_writes} G5_WAIT_FOR_FILE_E2E:{code_root}/{gate_b}"
                ),
                b_provider,
            )
            .await?;
        let session_b_id = data(&b)["session"]["id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("second chat session omitted id: {b}"))?.to_owned();
        let b_task = data(&b)["taskId"].as_str().ok_or_else(|| {
            anyhow::anyhow!("second session did not expose task identity: {b}")
        })?;
        anyhow::ensure!(a_task != b_task, "sessions collapsed into one task: {a} {b}");
        wait_for_file(&api, workspace.id, &code_repo_id, &b_code, &b_code_bytes).await?;
        wait_for_file(&api, workspace.id, docs_repo_id, &b_docs, &b_docs_bytes).await?;

        let (next_code, next_docs, next_code_bytes, next_docs_bytes, next_writes) =
            write_set(&code_root, &docs_root, workspace.id, &token, "next");
        let steering_marker = format!("Native steering {token}");
        let follow_up = if native_steering {
            format!("  {steering_marker}\nkeep this exact message  ")
        } else {
            format!("Queued B turn. G5_FS_WRITE_SET_E2E:{next_writes}")
        };
        if native_steering {
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let current: Value = api.authed_get(&format!(
                        "{}/api/v1/sessions/{session_b_id}/task", api.base_url()
                    )).send().await?.error_for_status()?.json().await?;
                    if data(&current)["id"] == b_task && data(&current)["nativeSteeringAvailable"] == true {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<(), anyhow::Error>(())
            }).await??;
        }
        let queued = send_for_provider(&api,
                &session_b_id,
                &follow_up,
                b_provider,
            )
            .await?;
        // The RPC acknowledgement can beat ordered journal projection. Below,
        // require delivered history and an empty tray before releasing Claude.
        anyhow::ensure!(
            if native_steering { matches!(data(&queued)["delivery"].as_str(),Some("queued" | "delivered")) } else { data(&queued)["delivery"] == "queued" }
                && data(&queued)["taskId"] == b_task,
            "follow-up took the wrong delivery path: {queued}; B={b}"
        );
        if native_steering {
            // The provider is still held and cannot read the correction yet.
            // A flushed message is already delivered in this same attempt.
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let history = api.get_chat_messages(&session_b_id).await?;
                    if let Some(message) = data(&history)["messages"].as_array().unwrap().iter()
                        .find(|message| message["content"] == follow_up) {
                        anyhow::ensure!(message["id"] == data(&queued)["inputId"],
                            "steering binding changed: {message}");
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<(), anyhow::Error>(())
            }).await??;
            let inputs: Value = api.authed_get(&format!(
                "{}/api/v1/sessions/{session_b_id}/inputs", api.base_url()
            )).send().await?.error_for_status()?.json().await?;
            anyhow::ensure!(data(&inputs).as_array().unwrap().iter().all(
                |input| input["id"] != data(&queued)["inputId"]), "flushed input stayed in tray: {inputs}");
        }
        anyhow::ensure!(
            file_content(&api, workspace.id, &code_repo_id, &next_code)
                .await?
                .is_none(),
            "queued B turn ran before the held B turn was released"
        );
        release_gate(&api, workspace.id, &code_repo_id, &gate_b).await?;
        if native_steering {
            tokio::time::timeout(Duration::from_secs(90), async {
                loop {
                    let history = api.get_chat_messages(&session_b_id).await?;
                    let mut messages = data(&history)["messages"].as_array().unwrap().clone();
                    messages.sort_by_key(|message| message["messageIndex"].as_i64());
                    if messages.iter().any(|message| message["role"] == "assistant"
                        && message["status"] == "complete"
                        && message["content"].as_str().is_some_and(|text| text.contains(&steering_marker))) {
                        let users = messages.iter().filter(|message| message["role"] == "user"
                            && message["content"] == follow_up).count();
                        anyhow::ensure!(users == 1, "steering duplicated or changed: {history}");
                        let index = messages.iter().position(|message| message["content"] == follow_up).unwrap();
                        anyhow::ensure!(index > 0 && messages[index - 1]["role"] == "assistant"
                            && messages.get(index + 1).is_some_and(|message| message["role"] == "assistant"
                                && message["content"].as_str().is_some_and(|text| text.contains(&steering_marker))),
                            "steering transcript order changed: {history}");
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<(), anyhow::Error>(())
            }).await??;
        } else {
            wait_for_assistant_count(&api, &session_b_id, 2).await?;
            wait_for_file(&api, workspace.id, &code_repo_id, &next_code, &next_code_bytes).await?;
            wait_for_file(&api, workspace.id, docs_repo_id, &next_docs, &next_docs_bytes).await?;
        }
        anyhow::ensure!(
            assistant_count(&api, &session_a_id).await? == 0,
            "B's completion released A's still-held provider process"
        );
        release_gate(&api, workspace.id, &code_repo_id, &gate_a).await?;
        wait_for_assistant_count(&api, &session_a_id, 1).await?;

        let vm_after: Value = api
            .authed_get(&format!("{}/api/v1/workspaces/{}/vm", api.base_url(), workspace.id))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        anyhow::ensure!(
            data(&vm_after)["vmId"] == vm_id,
            "chat overlap changed shared VM incarnation: before={vm_before}, after={vm_after}"
        );
        anyhow::ensure!(assistant_count(&api, &session_b_id).await? == if native_steering { 1 } else { 2 });
        assert_one_attempt(&api, a_task).await?;
        assert_one_attempt(&api, b_task).await?;
        Ok(())
    }
    .await;

    // Release either held provider even when an assertion fails, then reclaim
    // its workspace capacity before other real-stack scenarios run.
    let _ = release_gate(&api, workspace.id, &code_repo_id, &gate_a).await;
    let _ = release_gate(&api, workspace.id, &code_repo_id, &gate_b).await;
    let _ = api.cancel_all_workspace_tasks(workspace.id).await;
    let _ = api.stop_workspace_vm(workspace.id).await;
    outcome
}

#[tokio::test]
async fn test_streamed_chat_survives_api_restart_and_continues_same_task() -> anyhow::Result<()> {
    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let workspace = api.create_workspace(org.id, "m6-chat-restart").await?;
    api.resume_workspace_vm(workspace.id).await?;

    let mut gate_to_release: Option<(String, String)> = None;
    let outcome: anyhow::Result<()> = async {
    let detail = api.get_workspace(workspace.id).await?;
    let docs_repo_id = data(&detail)["docsRepoId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("workspace omitted docs repo: {detail}"))?
        .to_owned();
    let changes: Value = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{}/changes",
            api.base_url(),
            workspace.id
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let members = data(&changes)["members"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("attached member vector missing: {changes}"))?;
    let code = members
        .iter()
        .find(|member| member["repo_id"] != docs_repo_id)
        .ok_or_else(|| anyhow::anyhow!("code member missing: {changes}"))?;
    let docs = members
        .iter()
        .find(|member| member["repo_id"] == docs_repo_id)
        .ok_or_else(|| anyhow::anyhow!("docs member missing: {changes}"))?;
    let code_repo_id = code["repo_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("code repo id missing: {changes}"))?
        .to_owned();
    let code_root = code["root_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("code root missing: {changes}"))?
        .to_owned();
    let docs_root = docs["root_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("docs root missing: {changes}"))?
        .to_owned();
    let vm_before: Value = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{}/vm",
            api.base_url(),
            workspace.id
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let vm_id = data(&vm_before)["vmId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("running VM omitted incarnation: {vm_before}"))?
        .to_owned();
    let token = Uuid::new_v4().simple().to_string();
    let gate = format!("e2e-m6-{token}-gate.txt");
    gate_to_release = Some((code_repo_id.clone(), gate.clone()));

        let (first_code, first_docs, first_code_bytes, first_docs_bytes, first_writes) =
            write_set(&code_root, &docs_root, workspace.id, &token, "before-restart");
        let started = start_for_provider(&api,
                workspace.id,
                "M6 API restart",
                &format!(
                    "Write the first code and Project Doc, then wait. G5_FS_WRITE_SET_E2E:{first_writes} G5_WAIT_FOR_FILE_E2E:{code_root}/{gate}"
                ),
                "codex",
            )
            .await?;
        let session_id = data(&started)["session"]["id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("chat session omitted id: {started}"))?.to_owned();
        let task_id = data(&started)["taskId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("first turn omitted task identity: {started}"))?
            .to_owned();
        wait_for_file(&api, workspace.id, &code_repo_id, &first_code, &first_code_bytes).await?;
        wait_for_file(&api, workspace.id, &docs_repo_id, &first_docs, &first_docs_bytes).await?;
        anyhow::ensure!(
            assistant_count(&api, &session_id).await? == 0,
            "first turn completed before API restart: task={task_id}"
        );
        let before = task(&api, &task_id).await?;
        // Preserve task dependency identity across restarts as
        // part of the logical task identity across restart and Continue.
        let dependencies = before["dependsOn"].clone();
        anyhow::ensure!(dependencies.is_array(), "task dependencies absent: {before}");
        let before_ids = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let timeline_before: Value = api
                    .authed_get(&format!("{}/api/v1/tasks/{task_id}/timeline", api.base_url()))
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                let ids = data(&timeline_before)["entries"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("pre-restart timeline absent: {timeline_before}"))?
                    .iter()
                    .map(|entry| {
                        entry["id"]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| anyhow::anyhow!("pre-restart entry omitted id: {entry}"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                if !ids.is_empty() {
                    return Ok::<Vec<String>, anyhow::Error>(ids);
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("held turn never produced pre-restart timeline evidence"))??;

        restart_api_container(&api).await?;
        let after_restart = task(&api, &task_id).await?;
        anyhow::ensure!(
            after_restart["dependsOn"] == dependencies,
            "API restart changed task dependencies: before={before}, after={after_restart}"
        );
        let vm_after: Value = api
            .authed_get(&format!("{}/api/v1/workspaces/{}/vm", api.base_url(), workspace.id))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        anyhow::ensure!(
            data(&vm_after)["vmId"] == vm_id,
            "API restart changed guest incarnation: before={vm_before}, after={vm_after}"
        );
        let persisted = api.get_chat_session(&session_id).await?;
        anyhow::ensure!(
            data(&persisted)["id"] == session_id,
            "API restart lost chat session {session_id}: {persisted}"
        );
        anyhow::ensure!(assistant_count(&api, &session_id).await? == 0);
        wait_for_runtime_file_after_restart(
            &api,
            workspace.id,
            &code_repo_id,
            &first_code,
            &first_code_bytes,
        )
        .await?;
        wait_for_runtime_file_after_restart(
            &api,
            workspace.id,
            &docs_repo_id,
            &first_docs,
            &first_docs_bytes,
        )
        .await?;
        let (next_code, next_docs, next_code_bytes, next_docs_bytes, next_writes) =
            write_set(&code_root, &docs_root, workspace.id, &token, "continue");
        let continued = send_for_provider(&api,
                &session_id,
                &format!("Continue in this chat. G5_FS_WRITE_SET_E2E:{next_writes}"),
                "codex",
            )
            .await?;
        anyhow::ensure!(
            data(&continued)["taskId"] == task_id && data(&continued)["delivery"] == "queued",
            "Continue did not queue on the held logical task: first={started}, next={continued}"
        );
        anyhow::ensure!(
            file_content(&api, workspace.id, &code_repo_id, &next_code)
                .await?
                .is_none(),
            "Continue ran before the held streamed turn was released"
        );
        release_gate(&api, workspace.id, &code_repo_id, &gate).await?;
        wait_for_assistant_count(&api, &session_id, 2).await?;
        wait_for_file(&api, workspace.id, &code_repo_id, &next_code, &next_code_bytes).await?;
        wait_for_file(&api, workspace.id, &docs_repo_id, &next_docs, &next_docs_bytes).await?;
        let final_task = task(&api, &task_id).await?;
        anyhow::ensure!(
            final_task["dependsOn"] == dependencies,
            "Continue changed dependency identity: before={before}, after={final_task}"
        );
        assert_one_attempt(&api, &task_id).await?;

        let messages = api.get_chat_messages(&session_id).await?;
        let messages = data(&messages)["messages"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("chat history absent after Continue"))?;
        let user_count = messages.iter().filter(|message| message["role"] == "user").count();
        anyhow::ensure!(
            user_count == 2,
            "API restart or Continue duplicated user effects: task={task_id}, messages={messages:?}"
        );
        let timeline: Value = api
            .authed_get(&format!("{}/api/v1/tasks/{task_id}/timeline", api.base_url()))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let entries = data(&timeline)["entries"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("task timeline absent: {timeline}"))?;
        anyhow::ensure!(!entries.is_empty(), "task timeline lost after restart: {timeline}");
        anyhow::ensure!(
            data(&timeline)["hasMore"] == false,
            "task timeline exceeded the inspected page: {timeline}"
        );
        let ids = entries
            .iter()
            .map(|entry| {
                entry["id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("timeline entry omitted id: {entry}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        anyhow::ensure!(
            ids.len() == ids.iter().collect::<HashSet<_>>().len(),
            "task timeline duplicated entries after restart: {timeline}"
        );
        anyhow::ensure!(
            before_ids.iter().all(|before_id| ids.contains(&before_id.as_str()))
                && ids.len() > before_ids.len(),
            "restart dropped earlier evidence or Continue added none: before={before_ids:?}, after={timeline}"
        );
        let positions = entries
            .iter()
            .map(|entry| {
                Ok::<_, anyhow::Error>((
                    entry["createdAt"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("timeline entry omitted time: {entry}"))?,
                    entry["id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("timeline entry omitted id: {entry}"))?,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        anyhow::ensure!(
            positions.windows(2).all(|pair| pair[0] <= pair[1]),
            "task timeline lost chronological ordering after restart: {timeline}"
        );
        Ok(())
    }
    .await;

    if let Some((code_repo_id, gate)) = gate_to_release {
        let _ = release_gate(&api, workspace.id, &code_repo_id, &gate).await;
    }
    let _ = api.cancel_all_workspace_tasks(workspace.id).await;
    let _ = api.stop_workspace_vm(workspace.id).await;
    outcome
}
