//! A candidate host loss after a verified two-repository receipt. The public
//! API and real guest prove the restore and Preview fencing boundary together.

use crate::helpers::api_client::TestApiClient;
use anyhow::{anyhow, Context};
use base64::Engine;
use reqwest::header::{COOKIE, HOST, SET_COOKIE};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use uuid::Uuid;

fn basic_auth_header(token: &str) -> String {
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    format!("Authorization: Basic {encoded}")
}

async fn run_git(dir: &Path, auth_header: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(["-c", &format!("http.extraHeader={auth_header}")])
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .with_context(|| format!("failed to start git {args:?}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn run_local_git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .with_context(|| format!("failed to start local git {args:?}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn data(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

async fn get(api: &TestApiClient, path: &str) -> anyhow::Result<Value> {
    let preview_deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        let response = api
            .authed_get(&format!("{}{path}", api.base_url()))
            .send()
            .await?;
        if path.ends_with("/preview") && response.status() == StatusCode::SERVICE_UNAVAILABLE {
            if tokio::time::Instant::now() < preview_deadline {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            let body = response.text().await?;
            anyhow::bail!(
                "Preview status remained unavailable for 90 seconds at {path}: {}",
                body.chars().take(512).collect::<String>()
            );
        }
        return Ok(response.error_for_status()?.json().await?);
    }
}

async fn vm(api: &TestApiClient, workspace_id: Uuid) -> anyhow::Result<Value> {
    get(api, &format!("/api/v1/workspaces/{workspace_id}/vm")).await
}

async fn wait_vm(
    api: &TestApiClient,
    workspace_id: Uuid,
    status: &str,
    timeout_secs: u64,
) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        loop {
            let current = vm(api, workspace_id).await?;
            if data(&current)["status"] == status {
                return Ok::<Value, anyhow::Error>(current);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("VM {workspace_id} did not reach {status}"))?
}

async fn write(
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
    let body = response.text().await?;
    anyhow::ensure!(
        status.is_success(),
        "write {repo_id}:{path}: {status} {body}"
    );
    Ok(())
}

async fn assert_live_file_content(
    api: &TestApiClient,
    workspace_id: Uuid,
    repo_id: &str,
    path: &str,
    bytes: &str,
) -> anyhow::Result<()> {
    let file = get(
        api,
        &format!("/api/v1/workspaces/{workspace_id}/files/{path}?repoId={repo_id}"),
    )
    .await?;
    anyhow::ensure!(
        data(&file)["content"] == bytes && data(&file)["source"] == "live",
        "live {repo_id}:{path} does not match restored content: {file}"
    );
    Ok(())
}

async fn wait_preview(api: &TestApiClient, workspace_id: Uuid) -> anyhow::Result<String> {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let body = get(api, &format!("/api/v1/workspaces/{workspace_id}/preview")).await?;
            let preview = data(&body);
            if matches!(
                preview["status"].as_str(),
                Some("failed" | "cancelled" | "succeeded")
            ) {
                anyhow::bail!("Preview ended before ready: {body}");
            }
            if preview["ready"] == true {
                return preview["taskId"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow::anyhow!("ready Preview omitted task: {body}"));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Preview did not become ready"))?
}

async fn start_preview(
    api: &TestApiClient,
    workspace_id: Uuid,
    code_repo_id: &str,
) -> anyhow::Result<Value> {
    let key = Uuid::new_v4();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        let response = api
            .authed_post(&format!(
                "{}/api/v1/workspaces/{workspace_id}/preview",
                api.base_url()
            ))
            .json(&json!({"repoId":code_repo_id,"idempotencyKey":key}))
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await?;
        anyhow::ensure!(
            !body["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("FILE_MISSING")),
            "Preview configuration is missing: {body}"
        );
        if status == StatusCode::SERVICE_UNAVAILABLE && tokio::time::Instant::now() < deadline {
            // An API or host restart can finish before the process-local host
            // tunnel reconnects. The same key keeps this retry idempotent.
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        anyhow::ensure!(status.is_success(), "Preview start: {status} {body}");
        return Ok(body);
    }
}

async fn restart_bound_host(
    api: &TestApiClient,
    org_id: Uuid,
    workspace_id: Uuid,
) -> anyhow::Result<()> {
    if let Ok(service) = std::env::var("G5_E2E_LIVE_HOST_SERVICE") {
        // The self-hosted qualification stack uses the real systemd boundary.
        // Refuse shared/remote APIs, other services, or a host with a neighbor.
        let url = reqwest::Url::parse(api.base_url())?;
        anyhow::ensure!(
            service == "g5d-host.service" && matches!(url.host_str(), Some("127.0.0.1" | "[::1]")),
            "systemd host-loss qualification requires a local isolated API"
        );
        let current = vm(api, workspace_id).await?;
        let host_id = data(&current)["hostId"]
            .as_str()
            .context("running workspace omitted its host")?;
        let hosts = get(api, &format!("/api/v1/orgs/{org_id}/worker-hosts")).await?;
        let host = data(&hosts)["hosts"]
            .as_array()
            .context("host list missing")?
            .iter()
            .find(|host| host["hostId"] == host_id)
            .context("bound host missing")?;
        anyhow::ensure!(host["occupancy"] == 1, "refusing host loss with a neighbor");
        let pid = Command::new("systemctl")
            .args(["show", &service, "--property=MainPID", "--value"])
            .output()
            .await?;
        anyhow::ensure!(pid.status.success(), "cannot inspect test host service");
        let pid: u32 = std::str::from_utf8(&pid.stdout)?.trim().parse()?;
        anyhow::ensure!(pid > 1, "test host service is not running");
        let identity = format!("G5D_HOST_ID={host_id}");
        anyhow::ensure!(
            std::fs::read(format!("/proc/{pid}/environ"))?
                .split(|byte| *byte == 0)
                .any(|value| value == identity.as_bytes()),
            "service does not own the workspace's host identity"
        );
        let group = Command::new("systemctl")
            .args(["show", &service, "--property=ControlGroup", "--value"])
            .output()
            .await?;
        anyhow::ensure!(group.status.success(), "cannot inspect test host cgroup");
        let group = std::str::from_utf8(&group.stdout)?.trim();
        anyhow::ensure!(
            group == "/system.slice/g5d-host.service",
            "refusing to kill an unexpected service cgroup"
        );
        // One kernel operation kills the entire delegated tree. systemd 255's
        // per-process `kill --kill-whom=all` can report EINVAL while iterating
        // the dying tree, after successfully killing the supervisor/VMM.
        std::fs::write(format!("/sys/fs/cgroup{group}/cgroup.kill"), b"1")?;
        let result = tokio::time::timeout(
            Duration::from_secs(90),
            Command::new("systemctl")
                .args(["restart", &service])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("systemd test host restart timed out")??;
        anyhow::ensure!(result.status.success(), "systemd test host restart failed");
        return Ok(());
    }
    let container = std::env::var("G5_E2E_LIVE_HOST_CONTAINER")?;
    anyhow::ensure!(
        matches!(
            container.as_str(),
            "g5d-host-vl-1" | "g5d-host-re-1" | "g5d-host-1"
        ),
        "refusing to restart unassigned host {container}"
    );
    // Zero grace prevents an orderly workspace Stop from creating a new
    // receipt. The recovery must use the verified one captured below.
    let mut command = tokio::process::Command::new("docker");
    command
        .args(["restart", "--time", "0", &container])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(90), command.output())
        .await
        .map_err(|_| anyhow::anyhow!("host container restart timed out: {container}"))??;
    anyhow::ensure!(
        output.status.success(),
        "host restart failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

async fn assert_stale_incarnation(
    api: &TestApiClient,
    workspace_id: Uuid,
    code_repo_id: &str,
    token: &str,
    checkout: &std::path::Path,
    auth_header: &str,
    branch: &str,
) -> anyhow::Result<()> {
    let manifest = api
        .client()
        .get(format!(
            "{}/api/internal/workspaces/{workspace_id}/manifest",
            api.base_url()
        ))
        .bearer_auth(token)
        .send()
        .await?;
    anyhow::ensure!(
        matches!(
            manifest.status(),
            StatusCode::CONFLICT | StatusCode::FORBIDDEN
        ),
        "old incarnation still read its manifest: {}",
        manifest.status()
    );
    let flush = api
        .client()
        .post(format!(
            "{}/api/internal/workspace-incarnations/{workspace_id}/flushes/start",
            api.base_url()
        ))
        .bearer_auth(token)
        .json(&json!({"reason":"shutdown","idempotency_key":format!("stale-{}", Uuid::new_v4())}))
        .send()
        .await?;
    let status = flush.status();
    let body: Value = flush.json().await?;
    anyhow::ensure!(
        status == StatusCode::CONFLICT && body["error"]["code"] == "STALE_INCARNATION",
        "old incarnation still started a flush: {status} {body}"
    );
    if let Ok(output) = run_git(
        checkout,
        auth_header,
        &["push", "origin", &format!("HEAD:refs/heads/{branch}")],
    )
    .await
    {
        anyhow::bail!("old incarnation push unexpectedly succeeded: {output}");
    }
    // The advertisement uses the same require_push gate as receive-pack.
    // Check its status directly so the proof does not depend on git's stderr.
    let stale_push_advertisement = api
        .client()
        .get(format!(
            "{}/git/{code_repo_id}/info/refs?service=git-receive-pack",
            api.base_url()
        ))
        .basic_auth("x-access-token", Some(token))
        .send()
        .await?;
    anyhow::ensure!(
        stale_push_advertisement.status() == StatusCode::FORBIDDEN,
        "old incarnation retained Git push authority: {}",
        stale_push_advertisement.status()
    );
    Ok(())
}

#[tokio::test]
async fn test_verified_receipt_restores_after_host_loss_and_fences_preview() -> anyhow::Result<()> {
    let health = TestApiClient::from_env();
    anyhow::ensure!(health.healthz().await?, "g5-api not healthy");
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let workspace = api.create_workspace(org.id, "m4-host-loss").await?;
    api.resume_workspace_vm(workspace.id).await?;

    let outcome: anyhow::Result<()> = async {
        let owner: Value = api
            .authed_get(&format!("{}/api/v1/users/me", api.base_url()))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let owner_id: Uuid = data(&owner)["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("current owner omitted ID: {owner}"))?
            .parse()?;
        let detail = api.get_workspace(workspace.id).await?;
        let docs_repo_id = data(&detail)["docsRepoId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("workspace omitted docs repo: {detail}"))?
            .to_owned();
        let changes = get(
            &api,
            &format!("/api/v1/workspaces/{}/changes", workspace.id),
        )
        .await?;
        let members = data(&changes)["members"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("workspace omitted member vector: {changes}"))?;
        anyhow::ensure!(members.len() == 2, "expected code and docs: {changes}");
        let code_repo_id = members
            .iter()
            .find(|member| member["repo_id"] != docs_repo_id)
            .and_then(|member| member["repo_id"].as_str())
            .ok_or_else(|| anyhow::anyhow!("code repo missing: {changes}"))?
            .to_owned();
        let token = Uuid::new_v4().simple().to_string();
        let code_path = format!("m4-durable-{token}.txt");
        let docs_path = format!("docs/{}/m4-durable-{token}.md", workspace.id);
        let lost_path = format!("m4-unflushed-{token}.txt");
        let code_bytes = format!("M4 durable code {token}\n");
        let docs_bytes = format!("# M4 durable Project Doc {token}\n");
        // A workspace created directly by the API has no seeded Preview app.
        // Persist one alongside the code/doc cut that the replacement VM must
        // restore, using the same production profile and server shape as the
        // browser's seeded Preview workspace.
        let preview_config =
            "preview:\n  command: node e2e-preview-server.js\n  port: 4173\n";
        let preview_script = "const http = require('http');\nhttp.createServer((_request, response) => { response.writeHead(200, { 'content-type': 'text/plain' }); response.end('G5_WORKSPACE_PREVIEW_RENDERED\\n'); }).listen(4173, '0.0.0.0');\n";
        write(&api, workspace.id, &code_repo_id, &code_path, &code_bytes).await?;
        write(&api, workspace.id, &docs_repo_id, &docs_path, &docs_bytes).await?;
        write(
            &api,
            workspace.id,
            &code_repo_id,
            ".g5/workspace.yaml",
            preview_config,
        )
        .await?;
        write(
            &api,
            workspace.id,
            &code_repo_id,
            "e2e-preview-server.js",
            preview_script,
        )
        .await?;
        api.stop_workspace_vm(workspace.id).await?;
        let stopped = wait_vm(&api, workspace.id, "stopped", 120).await?;
        let receipt = data(&stopped)["lastFlushId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Stop omitted verified receipt: {stopped}"))?
            .to_owned();
        // Stopped browsing exposes the pushed cut, not the private receipt.
        // Restart and prove every restored byte through the real guest.
        api.resume_workspace_vm(workspace.id).await?;
        for (repo, path, bytes) in [
            (code_repo_id.as_str(), code_path.as_str(), code_bytes.as_str()),
            (docs_repo_id.as_str(), docs_path.as_str(), docs_bytes.as_str()),
            (code_repo_id.as_str(), ".g5/workspace.yaml", preview_config),
        ] {
            assert_live_file_content(&api, workspace.id, repo, path, bytes).await?;
        }
        let before_loss = vm(&api, workspace.id).await?;
        let old_vm_id = data(&before_loss)["vmId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("running VM omitted incarnation: {before_loss}"))?
            .to_owned();
        let old_epoch = data(&before_loss)["epoch"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("running VM omitted epoch: {before_loss}"))?;
        let old_token = g5_auth::JwtMinter::new(
            &std::env::var("G5_E2E_JWT_PRIVATE_KEY").context(
                "host-loss recovery needs the running stack signing key in G5_E2E_JWT_PRIVATE_KEY",
            )?,
            "g5-api",
            "g5",
        )?
        .mint_workspace_incarnation(
            org.id,
            workspace.id,
            owner_id,
            old_vm_id.parse()?,
            old_epoch,
            3600,
        )?;
        let live_manifest = api
            .client()
            .get(format!(
                "{}/api/internal/workspaces/{}/manifest",
                api.base_url(),
                workspace.id
            ))
            .bearer_auth(&old_token)
            .send()
            .await?;
        anyhow::ensure!(
            live_manifest.status().is_success(),
            "old incarnation credential was not valid before loss: {}",
            live_manifest.status()
        );
        let auth_header = basic_auth_header(&old_token);
        // Push advertisement calls the Git route's require_push boundary
        // without changing the receipt-backed workspace branch.
        let push_advertisement = api
            .client()
            .get(format!(
                "{}/git/{code_repo_id}/info/refs?service=git-receive-pack",
                api.base_url()
            ))
            .basic_auth("x-access-token", Some(&old_token))
            .send()
            .await?;
        anyhow::ensure!(
            push_advertisement.status() == StatusCode::OK,
            "old incarnation lacked Git push authority before loss: {}",
            push_advertisement.status()
        );
        let git_dir = tempfile::tempdir()?;
        let checkout = git_dir.path().join("code");
        run_git(
            git_dir.path(),
            &auth_header,
            &[
                "clone",
                "--branch",
                &workspace.name,
                &format!("{}/git/{code_repo_id}", api.base_url()),
                checkout.to_str().ok_or_else(|| anyhow::anyhow!("non-UTF8 checkout path"))?,
            ],
        )
        .await?;
        std::fs::write(checkout.join("stale-host-push.txt"), b"old host must not push\n")?;
        run_local_git(&checkout, &["add", "stale-host-push.txt"]).await?;
        run_local_git(&checkout, &["-c", "user.name=G5 E2E", "-c", "user.email=e2e@g5.dev", "commit", "-m", "stale host push probe"]).await?;
        anyhow::ensure!(data(&before_loss)["lastFlushId"] == receipt);
        write(
            &api,
            workspace.id,
            &code_repo_id,
            &lost_path,
            "not flushed\n",
        )
        .await?;
        let started = start_preview(&api, workspace.id, &code_repo_id).await?;
        let preview_task = wait_preview(&api, workspace.id).await?;
        anyhow::ensure!(data(&started)["taskId"] == preview_task);
        let ticket_response = api
            .authed_post(&format!(
                "{}/api/v1/preview/session/{preview_task}",
                api.base_url()
            ))
            .json(&json!({}))
            .send()
            .await?
            .error_for_status()?;
        anyhow::ensure!(
            ticket_response.headers().get(SET_COOKIE).is_none(),
            "Preview mint must not set a platform-origin cookie"
        );
        let ticket: Value = ticket_response.json().await?;
        let raw_url = data(&ticket)["previewUrl"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Preview omitted URL: {ticket}"))?;
        // The handoff URL names the preview host. This lane reaches the API
        // directly rather than through the edge, so present that host on the
        // API's own listener: host translation keys on `Host`, not on DNS.
        let handoff = reqwest::Url::parse(raw_url)?;
        let preview_host = handoff
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("Preview URL has no host: {raw_url}"))?
            .to_owned();
        let on_preview_host = |path_and_query: &str| format!("{}{path_and_query}", api.base_url());
        let preview_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let exchange = preview_client
            .get(on_preview_host(&format!(
                "{}?{}",
                handoff.path(),
                handoff.query().unwrap_or_default()
            )))
            .header(HOST, &preview_host)
            .send()
            .await?;
        anyhow::ensure!(
            exchange.status() == StatusCode::SEE_OTHER,
            "Preview handoff was not exchanged: {}",
            exchange.status()
        );
        let cookie = exchange
            .headers()
            .get(SET_COOKIE)
            .ok_or_else(|| anyhow::anyhow!("Preview handoff omitted its session cookie"))?
            .to_str()?
            .split(';')
            .next()
            .ok_or_else(|| anyhow::anyhow!("Preview session cookie is empty"))?
            .to_owned();
        anyhow::ensure!(cookie.starts_with("__Host-g5_preview="));
        let preview_url = on_preview_host("/");
        let served = preview_client
            .get(&preview_url)
            .header(HOST, &preview_host)
            .header(COOKIE, &cookie)
            .send()
            .await?;
        anyhow::ensure!(
            served.status() == StatusCode::OK,
            "Preview did not serve before loss: {}",
            served.status()
        );
        anyhow::ensure!(served
            .text()
            .await?
            .contains("G5_WORKSPACE_PREVIEW_RENDERED"));

        restart_bound_host(&api, org.id, workspace.id).await?;
        let failed = wait_vm(&api, workspace.id, "failed", 180).await?;
        anyhow::ensure!(
            data(&failed)["lastFlushId"] == receipt && data(&failed)["vmId"].is_null(),
            "host loss changed the verified receipt or retained the dead VM: {failed}"
        );
        assert_stale_incarnation(
            &api,
            workspace.id,
            &code_repo_id,
            &old_token,
            &checkout,
            &auth_header,
            &workspace.name,
        )
        .await?;
        let old_preview = preview_client
            .get(&preview_url)
            .header(HOST, &preview_host)
            .header(COOKIE, &cookie)
            .send()
            .await?;
        anyhow::ensure!(
            old_preview.status() != StatusCode::OK,
            "dead Preview URL still served"
        );
        let stale_mint = api
            .authed_post(&format!(
                "{}/api/v1/preview/session/{preview_task}",
                api.base_url()
            ))
            .json(&json!({}))
            .send()
            .await?;
        anyhow::ensure!(
            !stale_mint.status().is_success(),
            "dead Preview minted a new ticket"
        );
        let settlement_deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            let task = get(&api, &format!("/api/v1/tasks/{preview_task}")).await?;
            if matches!(data(&task)["status"].as_str(), Some("cancelled" | "failed")) {
                break;
            }
            if tokio::time::Instant::now() >= settlement_deadline {
                anyhow::bail!("lost Preview did not settle: {task}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        api.resume_workspace_vm(workspace.id).await?;
        let restored = vm(&api, workspace.id).await?;
        anyhow::ensure!(
            data(&restored)["vmId"] != old_vm_id && data(&restored)["lastFlushId"] == receipt,
            "Resume did not restore the last verified receipt in a new incarnation: {restored}"
        );
        assert_stale_incarnation(
            &api,
            workspace.id,
            &code_repo_id,
            &old_token,
            &checkout,
            &auth_header,
            &workspace.name,
        )
        .await?;
        assert_live_file_content(
            &api,
            workspace.id,
            &code_repo_id,
            &code_path,
            &code_bytes,
        )
        .await?;
        assert_live_file_content(
            &api,
            workspace.id,
            &docs_repo_id,
            &docs_path,
            &docs_bytes,
        )
        .await?;
        assert_live_file_content(
            &api,
            workspace.id,
            &code_repo_id,
            ".g5/workspace.yaml",
            preview_config,
        )
        .await?;
        assert_live_file_content(
            &api,
            workspace.id,
            &code_repo_id,
            "e2e-preview-server.js",
            preview_script,
        )
        .await?;
        let lost = api
            .authed_get(&format!(
                "{}/api/v1/workspaces/{}/files/{lost_path}?repoId={code_repo_id}",
                api.base_url(),
                workspace.id
            ))
            .send()
            .await?;
        anyhow::ensure!(
            lost.status() == StatusCode::NOT_FOUND,
            "unflushed bytes survived host loss: {}",
            lost.status()
        );
        let no_preview = get(
            &api,
            &format!("/api/v1/workspaces/{}/preview", workspace.id),
        )
        .await?;
        anyhow::ensure!(
            data(&no_preview)["taskId"]
                .as_str()
                .is_none_or(|task| task == preview_task),
            "Preview auto-relaunched after loss: {no_preview}"
        );
        let fresh = start_preview(&api, workspace.id, &code_repo_id).await?;
        let fresh_task = wait_preview(&api, workspace.id).await?;
        anyhow::ensure!(fresh_task != preview_task && data(&fresh)["taskId"] == fresh_task);
        Ok(())
    }
    .await;

    let _ = api
        .authed_post(&format!(
            "{}/api/v1/workspaces/{}/preview/cancel",
            api.base_url(),
            workspace.id
        ))
        .json(&json!({}))
        .send()
        .await;
    if api.stop_workspace_vm(workspace.id).await.is_ok() {
        let _ = wait_vm(&api, workspace.id, "stopped", 120).await;
    }
    outcome
}
