//! F8: a failed real guest flush preserves a live source, and the repaired
//! fork survives an API process restart. M11: a Stop whose flush cannot
//! succeed kills the VM with the loss recorded, and Start restores the last
//! complete receipt, also across an API process restart.

use crate::helpers::api_client::{restart_api_container, TestApiClient};
use futures_util::{SinkExt, StreamExt};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

type Terminal = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

async fn terminal(api: &TestApiClient, workspace_id: Uuid) -> anyhow::Result<Terminal> {
    let response = api
        .authed_post(&format!("{}/api/v1/auth/ws-ticket", api.base_url()))
        .json(&json!({"workspaceId": workspace_id}))
        .send()
        .await?
        .error_for_status()?;
    let ticket: Value = response.json().await?;
    let ticket = data(&ticket)["ticket"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("terminal ticket absent: {ticket}"))?;
    let base = api
        .base_url()
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let (mut socket, _) = tokio_tungstenite::connect_async(format!(
        "{base}/api/v1/workspaces/{workspace_id}/terminal?token={ticket}&cols=80&rows=24"
    ))
    .await?;
    let opened = next_binary(&mut socket).await?;
    let opened: Value = serde_json::from_slice(&opened)?;
    anyhow::ensure!(opened["type"] == "opened", "PTY did not open: {opened}");
    Ok(socket)
}

async fn next_binary(socket: &mut Terminal) -> anyhow::Result<Vec<u8>> {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(30), socket.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("terminal WebSocket closed"))??;
        if let Message::Binary(bytes) = message {
            return Ok(bytes.to_vec());
        }
    }
}

async fn shell(socket: &mut Terminal, command: &str, marker: &str) -> anyhow::Result<String> {
    socket
        .send(Message::Text(
            json!({"type":"input","data":format!("{command}\n")})
                .to_string()
                .into(),
        ))
        .await?;
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut output = Vec::new();
        loop {
            output.extend(next_binary(socket).await?);
            let text = String::from_utf8_lossy(&output);
            if text.contains(marker) {
                return Ok(text.into_owned());
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("PTY did not emit {marker}"))?
}

async fn write_file(
    api: &TestApiClient,
    workspace_id: Uuid,
    repo_id: &str,
    path: &str,
    content: &str,
) -> anyhow::Result<()> {
    let response = api
        .authed_put(&format!(
            "{}/api/v1/workspaces/{workspace_id}/files/{path}?repoId={repo_id}",
            api.base_url()
        ))
        .json(&json!({"content":content,"encoding":"utf-8","expectedHash":null}))
        .send()
        .await?;
    let status = response.status();
    let body: Value = response.json().await?;
    anyhow::ensure!(
        status.is_success(),
        "file write {repo_id}:{path}: {status} {body}"
    );
    Ok(())
}

async fn vm(api: &TestApiClient, workspace_id: Uuid) -> anyhow::Result<Value> {
    let response = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{workspace_id}/vm",
            api.base_url()
        ))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

async fn wait_for_vm_status(
    api: &TestApiClient,
    workspace_id: Uuid,
    status: &str,
) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let state = vm(api, workspace_id).await?;
            if data(&state)["status"] == status {
                return Ok::<Value, anyhow::Error>(state);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("workspace {workspace_id} did not reach {status}"))?
}

async fn terminal_after_api_restart(
    api: &TestApiClient,
    workspace_id: Uuid,
) -> anyhow::Result<Terminal> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        match terminal(api, workspace_id).await {
            Ok(terminal) => return Ok(terminal),
            Err(error) if tokio::time::Instant::now() >= deadline => return Err(error),
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}

#[tokio::test]
async fn test_failed_live_flush_then_api_restart_forks_one_complete_child() -> anyhow::Result<()> {
    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let source = api.create_workspace(org.id, "f8-flush-recovery").await?;
    api.resume_workspace_vm(source.id).await?;
    let initial_vm = vm(&api, source.id).await?;
    let source_vm_id = data(&initial_vm)["vmId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("live source omitted VM identity: {initial_vm}"))?
        .to_owned();

    let mut oversized_cleanup: Option<(String, String)> = None;
    let mut running_child = None;
    let outcome: anyhow::Result<()> = async {
        let source_detail = api.get_workspace(source.id).await?;
        let docs_repo_id = data(&source_detail)["docsRepoId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("source omitted docs repo: {source_detail}"))?;
        let changes: Value = api
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/changes",
                api.base_url(),
                source.id
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let members = data(&changes)["members"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("live member vector absent: {changes}"))?;
        anyhow::ensure!(members.len() == 2, "expected code and docs: {changes}");
        let code = members
            .iter()
            .find(|member| member["repo_id"] != docs_repo_id)
            .ok_or_else(|| anyhow::anyhow!("code member absent: {changes}"))?;
        let code_repo_id = code["repo_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("code repo id absent: {changes}"))?;
        let code_root = code["root_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("code root absent: {changes}"))?;
        anyhow::ensure!(!code_root.contains('\''), "unsafe code root: {code_root}");
        let token = Uuid::new_v4().simple().to_string();
        let code_path = format!("f8-code-{token}.txt");
        let docs_path = format!("docs/{}/f8-doc-{token}.md", source.id);
        let after_failure_path = format!("f8-after-failure-{token}.txt");
        let oversized_path = format!("f8-oversized-{token}.txt");
        oversized_cleanup = Some((code_repo_id.to_owned(), oversized_path.clone()));
        let name = format!("f8-recovered-{token}");
        let code_bytes = format!("F8 code {token}\n");
        let docs_bytes = format!("# F8 docs {token}\n");
        let after_failure_bytes = format!("F8 reopened {token}\n");
        write_file(&api, source.id, code_repo_id, &code_path, &code_bytes).await?;
        write_file(&api, source.id, docs_repo_id, &docs_path, &docs_bytes).await?;

        let mut terminal = terminal(&api, source.id).await?;
        let oversize_file = format!("{code_root}/{oversized_path}");
        shell(
            &mut terminal,
            &format!(
                "truncate -s 101M '{oversize_file}' && printf '%s_%s\\n' F8 OVERSIZE_READY"
            ),
            "F8_OVERSIZE_READY",
        )
        .await?;
        let fork_url = format!("{}/api/v1/workspaces/{}/clone", api.base_url(), source.id);
        let failed_key = Uuid::new_v4().to_string();
        let failed = api
            .authed_post(&fork_url)
            .json(&json!({"name":name,"source":"live","interrupt":false,"idempotencyKey":failed_key}))
            .send()
            .await?;
        let failed_status = failed.status();
        let failed_body: Value = failed.json().await?;
        anyhow::ensure!(
            failed_status == StatusCode::CONFLICT
                && failed_body.to_string().contains("FLUSH_TOO_LARGE"),
            "oversized source did not fail flush: {failed_status} {failed_body}"
        );
        anyhow::ensure!(
            api.find_workspace_by_name(org.id, &name).await?.is_none(),
            "failed fork published a child: {failed_body}"
        );
        let source_vm = vm(&api, source.id).await?;
        anyhow::ensure!(
            data(&source_vm)["status"] == "running"
                && data(&source_vm)["vmId"] == source_vm_id,
            "failed fork changed its source incarnation: before={initial_vm}, after={source_vm}"
        );
        write_file(
            &api,
            source.id,
            code_repo_id,
            &after_failure_path,
            &after_failure_bytes,
        )
        .await?;
        shell(
            &mut terminal,
            &format!("rm '{oversize_file}' && printf '%s_%s\\n' F8 FILE_REPAIRED"),
            "F8_FILE_REPAIRED",
        )
        .await?;
        oversized_cleanup = None;
        terminal.close(None).await?;

        restart_api_container(&api).await?;
        let retry_key = Uuid::new_v4().to_string();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        let child = loop {
            match api
                .authed_post(&fork_url)
                .json(&json!({"name":name,"source":"live","interrupt":false,"idempotencyKey":retry_key}))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    break response.json::<Value>().await?;
                }
                Ok(response) => {
                    let status = response.status();
                    let body: Value = response.json().await?;
                    if status != StatusCode::SERVICE_UNAVAILABLE
                        || tokio::time::Instant::now() >= deadline
                    {
                        anyhow::bail!("fork retry after API restart failed: {status} {body}");
                    }
                }
                Err(error) if tokio::time::Instant::now() >= deadline => {
                    return Err(error.into());
                }
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
        let child_id = data(&child)["workspace"]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("successful fork omitted child id: {child}"))?;
        let child_id = Uuid::parse_str(child_id)?;
        anyhow::ensure!(child_id != source.id, "fork reused source identity: {child}");
        let child_vm = vm(&api, child_id).await?;
        anyhow::ensure!(
            data(&child_vm)["status"] == "stopped"
                && data(&child_vm)["lastFlushId"].as_str().is_some(),
            "retry published a running or receiptless child: {child_vm}"
        );
        // The retained fork cut is private until the owner starts the child.
        running_child = Some(child_id);
        api.resume_workspace_vm(child_id).await?;
        wait_for_vm_status(&api, child_id, "running").await?;
        let cut: Value = api
            .authed_get(&format!(
                "{}/api/v1/workspaces/{child_id}/changes",
                api.base_url()
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let cut_members = data(&cut)["members"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("child cut omitted members: {cut}"))?;
        anyhow::ensure!(cut_members.len() == 2, "child has a partial vector: {cut}");
        for (repo_id, path, expected) in [
            (code_repo_id, code_path.as_str(), code_bytes.as_str()),
            (docs_repo_id, docs_path.as_str(), docs_bytes.as_str()),
            (
                code_repo_id,
                after_failure_path.as_str(),
                after_failure_bytes.as_str(),
            ),
        ] {
            let file: Value = api
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{child_id}/files/{path}?repoId={repo_id}",
                    api.base_url()
                ))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let member = cut_members
                .iter()
                .find(|member| member["repo_id"] == repo_id)
                .ok_or_else(|| anyhow::anyhow!("missing member {repo_id}: {cut}"))?;
            anyhow::ensure!(
                data(&file)["content"] == expected
                    && data(&file)["repoOid"] == member["local_head_oid"],
                "child file differs from its pinned member: {repo_id}:{path} {file} {cut}"
            );
        }
        api.stop_workspace_vm(child_id).await?;
        wait_for_vm_status(&api, child_id, "stopped").await?;
        running_child = None;
        let children = api
            .list_workspaces(org.id)
            .await?
            .into_iter()
            .filter(|workspace| workspace.name == name)
            .collect::<Vec<_>>();
        anyhow::ensure!(
            children.len() == 1 && children[0].id == child_id.to_string(),
            "retry produced duplicate child workspaces: {children:?}, fork={child}"
        );
        let final_source_vm = vm(&api, source.id).await?;
        anyhow::ensure!(
            data(&final_source_vm)["status"] == "running"
                && data(&final_source_vm)["vmId"] == source_vm_id,
            "successful retry changed its source incarnation: before={initial_vm}, after={final_source_vm}, fork={child}"
        );
        Ok(())
    }
    .await;

    if let Some(child_id) = running_child {
        let _ = api.stop_workspace_vm(child_id).await;
    }
    // Reclaim the source's microVM slot even when an assertion fails.
    if let Some((repo_id, path)) = oversized_cleanup {
        let _ = api
            .authed_delete(&format!(
                "{}/api/v1/workspaces/{}/files/{path}?repoId={repo_id}",
                api.base_url(),
                source.id
            ))
            .json(&json!({"expectedHash":null}))
            .send()
            .await;
    }
    let _ = api.stop_workspace_vm(source.id).await;
    outcome
}

#[tokio::test]
async fn test_oversized_stop_kills_the_vm_and_start_restores_the_last_receipt() -> anyhow::Result<()>
{
    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let source = api.create_workspace(org.id, "m11-forced-stop").await?;
    api.resume_workspace_vm(source.id).await?;

    let outcome: anyhow::Result<()> = async {
        let source_detail = api.get_workspace(source.id).await?;
        let docs_repo_id = data(&source_detail)["docsRepoId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("source omitted docs repo: {source_detail}"))?
            .to_owned();
        let changes: Value = api
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/changes",
                api.base_url(),
                source.id
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let members = data(&changes)["members"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("live member vector absent: {changes}"))?;
        anyhow::ensure!(members.len() == 2, "expected code and docs: {changes}");
        let code = members
            .iter()
            .find(|member| member["repo_id"] != docs_repo_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("code member absent: {changes}"))?;
        let code_repo_id = code["repo_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("code repo id absent: {changes}"))?
            .to_owned();
        let code_root = code["root_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("code root absent: {changes}"))?
            .to_owned();
        anyhow::ensure!(!code_root.contains('\''), "unsafe code root: {code_root}");
        let token = Uuid::new_v4().simple().to_string();

        // A clean Stop saves a complete two-member receipt.
        let saved_code_path = format!("m11-saved-{token}.txt");
        let saved_docs_path = format!("docs/{}/m11-saved-{token}.md", source.id);
        let saved_code = format!("M11 saved code {token}\n");
        let saved_docs = format!("# M11 saved docs {token}\n");
        write_file(&api, source.id, &code_repo_id, &saved_code_path, &saved_code).await?;
        write_file(&api, source.id, &docs_repo_id, &saved_docs_path, &saved_docs).await?;
        api.stop_workspace_vm(source.id).await?;
        let saved = wait_for_vm_status(&api, source.id, "stopped").await?;
        let receipt = data(&saved)["lastFlushId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("clean Stop omitted its receipt: {saved}"))?
            .to_owned();
        anyhow::ensure!(
            data(&saved)["unflushedLossAt"].is_null(),
            "clean Stop recorded a loss: {saved}"
        );

        // After a fresh Start, unsaved work includes a file over the capture
        // limit, which no retry can save.
        api.resume_workspace_vm(source.id).await?;
        let unsaved_vm = vm(&api, source.id).await?;
        let unsaved_vm_id = data(&unsaved_vm)["vmId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("live source omitted VM identity: {unsaved_vm}"))?
            .to_owned();
        let unsaved_code_path = format!("m11-unsaved-{token}.txt");
        let oversized_path = format!("m11-oversized-{token}.txt");
        write_file(
            &api,
            source.id,
            &code_repo_id,
            &unsaved_code_path,
            "M11 unsaved code\n",
        )
        .await?;
        let mut writer = terminal(&api, source.id).await?;
        shell(
            &mut writer,
            &format!(
                "truncate -s 101M '{code_root}/{oversized_path}' && printf '%s_%s\\n' M11 OVERSIZE_READY"
            ),
            "M11_OVERSIZE_READY",
        )
        .await?;
        let _ = writer.close(None).await;

        // Stop gives up saving at once and kills the VM (g5.lifecycle.stop.force).
        // A replica without the host tunnel answers `stopping` and leaves the
        // kill to the tunnel owner, so wait for the stopped state.
        let stop_requested = tokio::time::Instant::now();
        let stop = api
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/stop",
                api.base_url(),
                source.id
            ))
            .json(&json!({"operationId": Uuid::new_v4()}))
            .send()
            .await?;
        let stop_status = stop.status();
        let stop_body: Value = stop.json().await?;
        anyhow::ensure!(
            stop_status == StatusCode::OK
                && matches!(data(&stop_body)["status"].as_str(), Some("stopped" | "stopping")),
            "Stop with an unsaveable file did not complete: {stop_status} {stop_body}"
        );
        let forced = wait_for_vm_status(&api, source.id, "stopped").await?;
        anyhow::ensure!(
            stop_requested.elapsed() < Duration::from_secs(90),
            "a save that cannot succeed held the Stop for {:?}",
            stop_requested.elapsed()
        );
        anyhow::ensure!(
            data(&forced)["unflushedLossAt"].is_string()
                && data(&forced)["lastFlushId"] == receipt.as_str(),
            "forced Stop hid its loss or moved the last receipt: {forced}"
        );

        let database_url = std::env::var("G5_E2E_DATABASE_URL")?;
        let db = sqlx::PgPool::connect(&database_url).await?;
        let flush_rows = || async {
            sqlx::query_as::<_, (i64, i64, i64)>(
                "SELECT count(*), count(*) FILTER (WHERE error IS NOT NULL),
                        count(*) FILTER (WHERE complete_at IS NOT NULL)
                 FROM workspace_flushes WHERE workspace_id = $1 AND origin = 'flush'",
            )
            .bind(source.id)
            .fetch_one(&db)
            .await
        };
        let before = flush_rows().await?;
        anyhow::ensure!(
            before == (2, 1, 1),
            "expected one complete and one failed flush: {before:?}"
        );

        // An API restart neither retries the save nor revives the VM.
        restart_api_container(&api).await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        let persisted = vm(&api, source.id).await?;
        let after = flush_rows().await?;
        anyhow::ensure!(
            data(&persisted)["status"] == "stopped"
                && data(&persisted)["lastFlushId"] == receipt.as_str()
                && after == before,
            "API restart changed the forced Stop: {persisted}, flushes {before:?} -> {after:?}"
        );

        // Start boots a new VM from the last complete receipt.
        api.resume_workspace_vm(source.id).await?;
        let restored_vm = vm(&api, source.id).await?;
        anyhow::ensure!(
            data(&restored_vm)["vmId"] != unsaved_vm_id.as_str(),
            "Start reused the killed incarnation: {restored_vm}"
        );
        let mut reader = terminal_after_api_restart(&api, source.id).await?;
        shell(
            &mut reader,
            &format!(
                "test ! -e '{code_root}/{oversized_path}' && test ! -e '{code_root}/{unsaved_code_path}' && test -e '{code_root}/{saved_code_path}' && printf '%s_%s\\n' M11 FRESH"
            ),
            "M11_FRESH",
        )
        .await?;
        let _ = reader.close(None).await;
        for (repo_id, path, expected) in [
            (code_repo_id.as_str(), saved_code_path.as_str(), saved_code.as_str()),
            (docs_repo_id.as_str(), saved_docs_path.as_str(), saved_docs.as_str()),
        ] {
            let file: Value = api
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{}/files/{path}?repoId={repo_id}",
                    api.base_url(),
                    source.id
                ))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            anyhow::ensure!(
                data(&file)["content"] == expected,
                "the saved file did not restore: {repo_id}:{path} {file}"
            );
        }
        Ok(())
    }
    .await;

    let _ = api.stop_workspace_vm(source.id).await;
    outcome
}
