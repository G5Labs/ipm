//! R2 guest lifecycle qualification through the public workspace terminal API.
//!
//! This keeps the transport boundary honest: the ticket-authenticated API
//! adapter, guest dispatcher PTY, reconnect identity, and workspace stop are
//! exercised together. Required CI selectors fail when the E2E stack is absent.

use crate::helpers::api_client::TestApiClient;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

async fn terminal_ticket(api: &TestApiClient, workspace_id: Uuid) -> anyhow::Result<String> {
    let response = api
        .authed_post(&format!("{}/api/v1/auth/ws-ticket", api.base_url()))
        .json(&serde_json::json!({"workspaceId": workspace_id}))
        .send()
        .await?
        .error_for_status()?;
    let body: serde_json::Value = response.json().await?;
    body["data"]["ticket"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("terminal WebSocket ticket missing from response"))
}

async fn next_binary<S>(socket: &mut S) -> anyhow::Result<Vec<u8>>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(30), socket.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("terminal WebSocket closed unexpectedly"))??;
        if let Message::Binary(bytes) = message {
            return Ok(bytes.to_vec());
        }
    }
}

async fn read_until<S>(socket: &mut S, marker: &str) -> anyhow::Result<String>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut output = Vec::new();
        loop {
            output.extend(next_binary(socket).await?);
            let decoded = String::from_utf8_lossy(&output);
            if decoded.contains(marker) {
                return Ok(decoded.into_owned());
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("PTY did not emit {marker} within 30 seconds"))?
}

#[tokio::test]
async fn test_guest_terminal_reconnects_and_stop_fences_the_session() -> anyhow::Result<()> {
    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let workspace = api.create_workspace(org.id, "r2-guest-lifecycle").await?;
    api.resume_workspace_vm(workspace.id).await?;

    let base = api
        .base_url()
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let connect = |ticket: String, terminal_id: Option<&str>| {
        let mut url = format!(
            "{base}/api/v1/workspaces/{}/terminal?token={ticket}&cols=80&rows=24",
            workspace.id
        );
        if let Some(terminal_id) = terminal_id {
            url.push_str("&terminal_id=");
            url.push_str(terminal_id);
        }
        tokio_tungstenite::connect_async(url)
    };

    let first_ticket = terminal_ticket(&api, workspace.id).await?;
    let (mut first, _) = connect(first_ticket, None).await?;
    let opened: serde_json::Value = serde_json::from_slice(&next_binary(&mut first).await?)?;
    anyhow::ensure!(
        opened["type"] == "opened",
        "unexpected PTY control: {opened}"
    );
    anyhow::ensure!(opened["reattached"] == false, "first PTY must be fresh");
    let terminal_id = opened["terminal_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("opened PTY did not return terminal_id"))?
        .to_owned();
    first
        .send(Message::Text(
            serde_json::json!({
                "type":"input",
                "data":"printf '%s%s\\n' R2_GUEST_ TERMINAL_PROOF\n"
            })
            .to_string()
            .into(),
        ))
        .await?;
    let mut proof = Vec::new();
    while !String::from_utf8_lossy(&proof).contains("R2_GUEST_TERMINAL_PROOF") {
        proof.extend(next_binary(&mut first).await?);
    }

    // A browser reload sends a WebSocket close frame. That is a transport
    // detach, not an explicit request to destroy the PTY.
    first.close(None).await?;
    let second_ticket = terminal_ticket(&api, workspace.id).await?;
    let (mut second, _) = connect(second_ticket, Some(&terminal_id)).await?;
    let reattached: serde_json::Value = serde_json::from_slice(&next_binary(&mut second).await?)?;
    anyhow::ensure!(
        reattached["type"] == "opened",
        "unexpected reconnect control: {reattached}"
    );
    anyhow::ensure!(
        reattached["terminal_id"] == terminal_id,
        "reconnect changed terminal identity"
    );
    anyhow::ensure!(
        reattached["reattached"] == true,
        "reconnect did not claim the retained PTY"
    );
    // Stop with the reattached PTY still open. The stop boundary must close
    // this stream and reject a fresh ticket's attempt to claim its identity.
    api.stop_workspace_vm(workspace.id).await?;
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let response = api
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{}/vm",
                    api.base_url(),
                    workspace.id
                ))
                .send()
                .await?
                .error_for_status()?;
            let body: serde_json::Value = response.json().await?;
            match body["data"]["status"].as_str() {
                Some("stopped") => break Ok::<(), anyhow::Error>(()),
                Some("failed") => anyhow::bail!("workspace stop failed: {}", body["data"]),
                _ => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("workspace VM did not stop within 120 seconds"))?;
    stopped?;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match second.next().await {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("stopped workspace left the PTY WebSocket open"))?;

    let post_stop_ticket = terminal_ticket(&api, workspace.id).await?;
    match connect(post_stop_ticket, Some(&terminal_id)).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            anyhow::ensure!(
                matches!(response.status().as_u16(), 404 | 409 | 503),
                "stopped workspace rejected PTY reconnect for an unrelated reason: {}",
                response.status()
            );
        }
        Err(error) => return Err(error.into()),
        Ok(_) => anyhow::bail!("stopped workspace accepted a new PTY attachment"),
    }
    Ok(())
}

#[tokio::test]
async fn test_guest_terminal_transfer_fences_old_owner_and_resets_identity() -> anyhow::Result<()> {
    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let old_owner = TestApiClient::from_env_authed().await?;
    // CI binds its candidate image and host to the seeded organization.
    // A fresh organization has no image policy, so Resume would fail before
    // the terminal or transfer boundary this scenario qualifies.
    let org = old_owner.use_seeded_org().await?;
    let workspace = old_owner
        .create_workspace(org.id, "r2-guest-transfer")
        .await?;
    let new_owner_email = format!("r2-transfer-{}@g5.dev", Uuid::new_v4());
    let (access, refresh) = old_owner.obtain_test_tokens(&new_owner_email).await?;
    let new_owner = TestApiClient::from_env_authed()
        .await?
        .with_auth_tokens(access, refresh);
    old_owner
        .authed_post(&format!(
            "{}/api/v1/orgs/{}/members",
            old_owner.base_url(),
            org.id
        ))
        .json(&serde_json::json!({"email":new_owner_email,"role":"member"}))
        .send()
        .await?
        .error_for_status()?;
    new_owner.switch_active_org(org.id).await?;
    let who: serde_json::Value = new_owner
        .authed_get(&format!("{}/api/v1/users/me", new_owner.base_url()))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let new_owner_id: Uuid = who["data"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("new owner user ID missing"))?
        .parse()?;

    old_owner.resume_workspace_vm(workspace.id).await?;
    let base = old_owner
        .base_url()
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let connect = |ticket: String, terminal_id: Option<&str>| {
        let mut url = format!(
            "{base}/api/v1/workspaces/{}/terminal?token={ticket}&cols=80&rows=24",
            workspace.id
        );
        if let Some(terminal_id) = terminal_id {
            url.push_str("&terminal_id=");
            url.push_str(terminal_id);
        }
        tokio_tungstenite::connect_async(url)
    };
    let (mut old_socket, _) =
        connect(terminal_ticket(&old_owner, workspace.id).await?, None).await?;
    let opened: serde_json::Value = serde_json::from_slice(&next_binary(&mut old_socket).await?)?;
    anyhow::ensure!(
        opened["type"] == "opened",
        "old owner PTY did not open: {opened}"
    );
    let old_terminal_id = opened["terminal_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("old terminal identity missing"))?
        .to_owned();
    let docs_repo_id = old_owner.get_docs_repo_id(org.id).await?;
    let changes: serde_json::Value = old_owner
        .authed_get(&format!(
            "{}/api/v1/workspaces/{}/changes",
            old_owner.base_url(),
            workspace.id
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    anyhow::ensure!(
        changes["data"]["source"] == "live",
        "terminal file proof requires the attached workspace: {changes}"
    );
    let code_member = changes["data"]["members"]
        .as_array()
        .and_then(|members| {
            members
                .iter()
                .find(|member| member["repo_id"] != docs_repo_id.to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("attached workspace has no code member: {changes}"))?;
    let code_repo_id = code_member["repo_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("code member omitted repo_id: {changes}"))?;
    let code_root = code_member["root_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("code member omitted root_path: {changes}"))?;
    anyhow::ensure!(
        !code_root.contains('\''),
        "unexpected quote in code root: {code_root}"
    );
    let file_name = format!("m13-transfer-{}.txt", Uuid::new_v4().simple());
    let file_bytes = format!("M13_TRANSFER_RESTORED_{}", Uuid::new_v4().simple());
    old_socket
        .send(Message::Text(
            serde_json::json!({"type":"input","data":"stty -echo\n"})
                .to_string()
                .into(),
        ))
        .await?;
    old_socket
        .send(Message::Text(
            serde_json::json!({
                "type":"input",
                "data":format!(
                    "printf '%s' '{file_bytes}' > '{code_root}/{file_name}'; printf '%s_%s\\n' M13 FILE_WRITTEN\n"
                )
            })
            .to_string()
            .into(),
        ))
        .await?;
    read_until(&mut old_socket, "M13_FILE_WRITTEN").await?;
    // Mint before transfer so the stale-owner probe is independent of the
    // ticket endpoint's post-transfer authorization policy.
    let stale_ticket = terminal_ticket(&old_owner, workspace.id).await?;
    let transfer_key = Uuid::new_v4().to_string();
    // Transfer may finish its durable Stop after this client's HTTP timeout.
    // Retry the same key so the test observes that one operation's outcome
    // instead of failing while the server is still draining the old owner.
    let mut transfer_attempts = 0_u32;
    let mut last_transfer_outcome = String::from("no response");
    let transfer_result = tokio::time::timeout(std::time::Duration::from_secs(360), async {
        loop {
            transfer_attempts += 1;
            let response = old_owner
                .authed_post(&format!(
                    "{}/api/v1/workspaces/{}/transfer",
                    old_owner.base_url(),
                    workspace.id
                ))
                .json(&serde_json::json!({
                    "toUserId": new_owner_id,
                    "idempotencyKey": transfer_key,
                }))
                .send()
                .await;
            let response = match response {
                Ok(response) => response.error_for_status()?,
                Err(error) if error.is_timeout() || error.is_connect() => {
                    last_transfer_outcome = if error.is_timeout() {
                        "request timed out".to_owned()
                    } else {
                        "connection failed".to_owned()
                    };
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let body: serde_json::Value = response.json().await?;
            match body["data"]["status"].as_str() {
                Some("completed") => break Ok::<(), anyhow::Error>(()),
                Some("pending") => {
                    last_transfer_outcome = format!("pending: {}", body["data"]);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                _ => anyhow::bail!("unexpected transfer outcome: {body}"),
            }
        }
    })
    .await;
    match transfer_result {
        Ok(result) => result?,
        Err(_) => {
            let vm_at_deadline = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                old_owner
                    .authed_get(&format!(
                        "{}/api/v1/workspaces/{}/vm",
                        old_owner.base_url(),
                        workspace.id
                    ))
                    .send()
                    .await?
                    .text()
                    .await
            })
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_else(|| "VM state unavailable".to_owned());
            let cleanup = match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                old_owner.stop_workspace_vm(workspace.id),
            )
            .await
            {
                Ok(Ok(())) => "accepted",
                Ok(Err(_)) => "rejected",
                Err(_) => "timed out",
            };
            anyhow::bail!(
                "workspace ownership transfer did not complete: workspace={}, attempts={transfer_attempts}, last_outcome={last_transfer_outcome}, vm_at_deadline={vm_at_deadline}, cleanup_stop={cleanup}",
                workspace.id
            );
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match old_socket.next().await {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("transfer left old-owner PTY WebSocket open"))?;
    match connect(stale_ticket, Some(&old_terminal_id)).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            anyhow::ensure!(
                matches!(response.status().as_u16(), 403 | 404 | 409 | 503),
                "old owner rejected for unrelated reason: {}",
                response.status()
            );
        }
        Err(error) => return Err(error.into()),
        Ok(_) => anyhow::bail!("old owner reattached after transfer"),
    }

    new_owner.resume_workspace_vm(workspace.id).await?;
    let (mut rejected_socket, _) = connect(
        terminal_ticket(&new_owner, workspace.id).await?,
        Some(&old_terminal_id),
    )
    .await?;
    let rejected: serde_json::Value =
        serde_json::from_slice(&next_binary(&mut rejected_socket).await?)?;
    anyhow::ensure!(
        rejected["type"] == "rejected",
        "old PTY identity survived transfer: {rejected}"
    );
    let (mut fresh_socket, _) =
        connect(terminal_ticket(&new_owner, workspace.id).await?, None).await?;
    let fresh: serde_json::Value = serde_json::from_slice(&next_binary(&mut fresh_socket).await?)?;
    anyhow::ensure!(
        fresh["type"] == "opened",
        "new owner PTY did not open: {fresh}"
    );
    anyhow::ensure!(
        fresh["terminal_id"] != old_terminal_id && fresh["reattached"] == false,
        "new owner did not receive a fresh PTY identity: {fresh}"
    );
    fresh_socket
        .send(Message::Text(
            serde_json::json!({"type":"input","data":"stty -echo\n"})
                .to_string()
                .into(),
        ))
        .await?;
    fresh_socket
        .send(Message::Text(
            serde_json::json!({
                "type":"input",
                "data":format!(
                    "printf '%s_%s:' M13 READ_BEGIN; cat '{code_root}/{file_name}'; printf ':%s_%s\\n' M13 READ_END\n"
                )
            })
            .to_string()
            .into(),
        ))
        .await?;
    let output = read_until(&mut fresh_socket, ":M13_READ_END").await?;
    anyhow::ensure!(
        output.contains(&format!("M13_READ_BEGIN:{file_bytes}:M13_READ_END")),
        "new owner PTY did not restore exact file bytes: {output:?}"
    );
    anyhow::ensure!(
        new_owner
            .get_workspace_branches(workspace.id)
            .await?
            .iter()
            .any(|branch| branch.repo_id == code_repo_id),
        "transferred workspace dropped its code branch {code_repo_id}"
    );
    let file_response = new_owner
        .authed_get(&format!(
            "{}/api/v1/workspaces/{}/files/{}?repoId={code_repo_id}",
            new_owner.base_url(),
            workspace.id,
            file_name
        ))
        .send()
        .await?
        .error_for_status()?;
    let file: serde_json::Value = file_response.json().await?;
    anyhow::ensure!(
        file["data"]["content"] == file_bytes,
        "transferred code bytes differ through the public file API: {file}"
    );
    fresh_socket.close(None).await?;
    new_owner.stop_workspace_vm(workspace.id).await?;
    Ok(())
}
