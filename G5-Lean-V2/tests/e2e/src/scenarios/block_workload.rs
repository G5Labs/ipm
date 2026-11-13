//! Opt-in qualification of the production block workload root through API + PTY.
//! Selecting this lane requires real KVM guests with private workload disks
//! and an independent read-only platform runtime; missing infrastructure fails.

use crate::helpers::api_client::TestApiClient;
use anyhow::{bail, ensure, Context};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

type Terminal = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

fn enabled() -> anyhow::Result<bool> {
    match std::env::var("G5_E2E_BLOCK_WORKLOAD").as_deref() {
        Ok("true") => Ok(true),
        Err(std::env::VarError::NotPresent) | Ok("false") => Ok(false),
        _ => bail!("G5_E2E_BLOCK_WORKLOAD must be true or false"),
    }
}

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

async fn next_binary(socket: &mut Terminal) -> anyhow::Result<Vec<u8>> {
    loop {
        match socket.next().await.context("terminal WebSocket closed")?? {
            Message::Binary(bytes) => return Ok(bytes.to_vec()),
            Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await?,
            Message::Close(_) => bail!("terminal closed before the command completed"),
            _ => {}
        }
    }
}

async fn terminal(api: &TestApiClient, id: Uuid) -> anyhow::Result<Terminal> {
    let body: Value = api
        .authed_post(&format!("{}/api/v1/auth/ws-ticket", api.base_url()))
        .json(&json!({"workspaceId": id}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let ticket = data(&body)["ticket"]
        .as_str()
        .context("terminal ticket missing")?;
    let base = api.base_url().replacen("http", "ws", 1);
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(30),
        tokio_tungstenite::connect_async(format!(
            "{base}/api/v1/workspaces/{id}/terminal?token={ticket}&cols=120&rows=40"
        )),
    )
    .await??;
    let opened: Value = serde_json::from_slice(
        &tokio::time::timeout(Duration::from_secs(30), next_binary(&mut socket)).await??,
    )?;
    ensure!(opened["type"] == "opened", "PTY did not open: {opened}");
    Ok(socket)
}

/// The complete completion marker is never present in terminal input, so input
/// echo cannot satisfy the oracle. Chunking also avoids canonical PTY line limits.
async fn shell(socket: &mut Terminal, script: &str, seconds: u64) -> anyhow::Result<String> {
    let token = Uuid::new_v4().simple().to_string();
    let path = format!("/tmp/g5-block-command-{token}");
    let mut command = format!("stty -echo\n: > {path}.b64\n");
    for chunk in STANDARD.encode(script).as_bytes().chunks(768) {
        command.push_str(&format!(
            "printf '%s' '{}' >> {path}.b64\n",
            std::str::from_utf8(chunk)?
        ));
    }
    command.push_str(&format!(
        "if base64 -d {path}.b64 > {path}.sh; then /bin/sh -eu {path}.sh; g5_block_status=$?; else g5_block_status=125; fi\nrm -f {path}.b64 {path}.sh\nprintf '\\n%s%s %s\\n' 'G5_BLOCK_DONE_' '{token}' \"$g5_block_status\"\nstty echo\n"
    ));
    socket
        .send(Message::Text(
            json!({"type":"input","data":command}).to_string().into(),
        ))
        .await?;
    let marker = format!("G5_BLOCK_DONE_{token} ");
    tokio::time::timeout(Duration::from_secs(seconds), async {
        let mut output = Vec::new();
        loop {
            output.extend(next_binary(socket).await?);
            ensure!(
                output.len() <= 256 * 1024,
                "PTY command exceeded output bound"
            );
            let text = String::from_utf8_lossy(&output);
            for line in text.split_inclusive('\n') {
                if !line.ends_with('\n') {
                    continue;
                }
                if let Some(status) = line.trim_end_matches(['\r', '\n']).strip_prefix(&marker) {
                    ensure!(status == "0", "guest command exited {status}: {text}");
                    return Ok::<String, anyhow::Error>(text.into_owned());
                }
            }
        }
    })
    .await
    .with_context(|| format!("guest command did not finish within {seconds}s"))?
}

const LAYOUT_PROBE: &str = r#"
/opt/g5/bin/node <<'G5_NODE'
const fs = require('fs');
const assert = require('assert/strict');
const decode = s => s.replace(/\\([0-7]{3})/g, (_, n) => String.fromCharCode(parseInt(n, 8)));
function mounts(pid) {
  return fs.readFileSync(`/proc/${pid}/mountinfo`, 'utf8').trim().split('\n').map(line => {
    const [a,b] = line.split(' - '), x = a.split(' '), y = b.split(' ');
    return {dev:x[2], root:decode(x[3]), path:decode(x[4]), options:x[5].split(','), fs:y[0], source:decode(y[1])};
  });
}
function at(list, path) {
  return list.filter(m => m.path === '/' || path === m.path || path.startsWith(m.path + '/'))
    .sort((a,b) => b.path.length - a.path.length)[0];
}
const list = mounts('self');
const root = at(list, '/');
assert.equal(root.fs, 'ext4', JSON.stringify(root));
assert.equal(root.root, '/', 'workload root must be the filesystem root');
const rootStat = fs.statSync('/');
assert.equal(fs.statSync('/proc/self/root').ino, rootStat.ino);
assert.equal(fs.statSync('/proc/self/root').dev, rootStat.dev);
assert.equal(at(list, '/proc').fs, 'proc');
// Unlike /proc/1/root dereferencing, mountinfo is readable by the normal user.
// Trusted PID 1 must retain the same workload root as the dispatcher.
const initRoot = at(mounts('1'), '/');
assert.equal(initRoot.fs, 'ext4', JSON.stringify(initRoot));
assert.equal(initRoot.dev, root.dev);
assert.equal(initRoot.root, '/');
assert.match(fs.readFileSync('/proc/1/status', 'utf8'), /^Umask:\s+0022$/m,
  'bootstrap secret staging must not restrict dispatcher/Git creation permissions');
assert.ok(process.env.HOME && process.env.HOME.startsWith('/'), 'absolute HOME required');
const paths = ['/', '/usr', '/tmp', process.env.HOME, '/work', '/var/lib/docker'];
const hot = paths.map(path => {
  const m = at(list, fs.realpathSync(path));
  assert.equal(m.fs, 'ext4', `${path}: ${JSON.stringify(m)}`);
  assert.equal(m.dev, root.dev, `${path}: wrong block device`);
  assert.equal(fs.statSync(path).dev, rootStat.dev, `${path}: wrong stat device`);
  return {path, mount:m};
});
assert.equal(at(list, '/var/lib/docker').path, '/var/lib/docker', 'preserve Docker mount-point detection');
const runtime = at(list, '/opt/g5');
assert.equal(runtime.path, '/opt/g5');
assert.equal(runtime.fs, 'ext4');
assert.notEqual(runtime.dev, root.dev, 'runtime must use an independent block device');
assert.ok(runtime.options.includes('ro'), 'runtime must be a separate read-only mount');
assert.ok(!runtime.options.includes('noexec'), 'runtime must be executable');
const writePath = '/opt/g5/.g5-block-write-' + require('crypto').randomUUID();
let denied = false;
try { fs.writeFileSync(writePath, 'must fail', {flag:'wx'}); }
catch (e) { assert.ok(['EROFS','EACCES','EPERM'].includes(e.code), String(e)); denied = true; }
if (!denied) { fs.unlinkSync(writePath); throw new Error('runtime was writable'); }
console.log('G5_BLOCK_LAYOUT ' + JSON.stringify({bootId:fs.readFileSync('/proc/sys/kernel/random/boot_id','utf8').trim(), root, initRoot, hot, runtime, uid:process.getuid()}));
G5_NODE
/opt/g5/bin/g5d --version
"#;

async fn layout(socket: &mut Terminal) -> anyhow::Result<Value> {
    let output = shell(socket, LAYOUT_PROBE, 45).await?;
    let line = output
        .lines()
        .find_map(|line| line.strip_prefix("G5_BLOCK_LAYOUT "))
        .context("layout probe did not emit its result")?;
    let result: Value = serde_json::from_str(line.trim())?;
    ensure!(
        result["uid"] == 1000,
        "terminal must run as the workspace user: {result}"
    );
    ensure!(result["bootId"].as_str().is_some(), "boot identity missing");
    eprintln!("block workload layout: {result}");
    Ok(result)
}

async fn vm(api: &TestApiClient, id: Uuid) -> anyhow::Result<Value> {
    let body: Value = api
        .authed_get(&format!("{}/api/v1/workspaces/{id}/vm", api.base_url()))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(data(&body).clone())
}

async fn stop(api: &TestApiClient, id: Uuid) -> anyhow::Result<Value> {
    api.stop_workspace_vm(id).await?;
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let state = vm(api, id).await?;
            if state["status"] == "stopped" {
                return Ok(state);
            }
            ensure!(state["status"] != "failed", "stop failed: {state}");
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .context("stop did not complete within 120s")?
}

async fn cleanup(api: &TestApiClient, ids: &[Uuid]) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for id in ids {
        let operation = Uuid::new_v4();
        let result: anyhow::Result<()> = tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let response = api
                    .authed_post(&format!("{}/api/v1/workspaces/{id}/delete", api.base_url()))
                    .json(&json!({"force":true,"operationId":operation}))
                    .send()
                    .await?;
                let status = response.status().as_u16();
                if matches!(status, 204 | 404) {
                    return Ok(());
                }
                let body = response.text().await?;
                ensure!(matches!(status, 409 | 503), "delete {id}: {status} {body}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("delete {id} timed out")));
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    ensure!(
        failures.is_empty(),
        "owned workspace cleanup failed: {}",
        failures.join("; ")
    );
    Ok(())
}

async fn finish(
    api: &TestApiClient,
    ids: &[Uuid],
    outcome: anyhow::Result<()>,
) -> anyhow::Result<()> {
    let cleanup = cleanup(api, ids).await;
    match (outcome, cleanup) {
        (Err(error), Err(cleanup)) => Err(error.context(format!("also: {cleanup}"))),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

async fn read_file(
    api: &TestApiClient,
    id: Uuid,
    repo: &str,
    path: &str,
) -> anyhow::Result<String> {
    let body: Value = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{id}/files/{path}?repoId={repo}",
            api.base_url()
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(
        data(&body)["source"] == "live",
        "Files must read the running VM, not a snapshot: {body}"
    );
    data(&body)["content"]
        .as_str()
        .map(str::to_owned)
        .context("Files response has no text content")
}

#[tokio::test]
async fn test_block_workload_layout_files_isolation_and_restore() -> anyhow::Result<()> {
    if !enabled()? {
        eprintln!("SKIP block workload opt-in: set G5_E2E_BLOCK_WORKLOAD=true");
        return Ok(());
    }
    ensure!(
        TestApiClient::from_env().healthz().await?,
        "g5-api not healthy"
    );
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let mut owned = Vec::new();
    let outcome = async {
        let first = api.create_workspace(org.id, "block-primary").await?;
        owned.push(first.id);
        let second = api.create_workspace(org.id, "block-neighbor").await?;
        owned.push(second.id);
        // Keep both live on the bound host: a single-VM proof misses shared-disk bugs.
        tokio::try_join!(api.resume_workspace_vm(first.id), api.resume_workspace_vm(second.id))?;
        let first_vm = vm(&api, first.id).await?;
        let second_vm = vm(&api, second.id).await?;
        ensure!(first_vm["hostId"].is_string() && first_vm["hostId"] == second_vm["hostId"],
            "capacity/isolation proof needs two VMs on one host: {first_vm} {second_vm}");
        ensure!(first_vm["vmId"].is_string() && second_vm["vmId"].is_string() && first_vm["vmId"] != second_vm["vmId"], "VM identities must differ");
        let mut first_terminal = terminal(&api, first.id).await?;
        let mut second_terminal = terminal(&api, second.id).await?;
        let before = layout(&mut first_terminal).await?;
        let neighbor = layout(&mut second_terminal).await?;
        ensure!(before["bootId"] != neighbor["bootId"], "separate VMs share a boot identity");
        let token = Uuid::new_v4().simple().to_string();
        let transient = format!("/tmp/g5-block-private-{token}");
        shell(&mut first_terminal, &format!("printf '%s' primary > {}", quote(&transient)), 30).await?;
        shell(&mut second_terminal, &format!("test ! -e {0}\nprintf '%s' neighbor > {0}", quote(&transient)), 30).await?;

        let detail = api.get_workspace(first.id).await?;
        let docs_repo = data(&detail)["docsRepoId"].as_str().context("docs repo absent")?;
        let changes: Value = api.authed_get(&format!("{}/api/v1/workspaces/{}/changes", api.base_url(), first.id))
            .send().await?.error_for_status()?.json().await?;
        let members = data(&changes)["members"].as_array().context("live members absent")?;
        ensure!(members.len() == 2, "fixture must have code and docs: {changes}");
        ensure!(members.iter().filter(|member| member["repo_id"] == docs_repo).count() == 1,
            "fixture must contain exactly one docs member: {changes}");
        let mut proofs = Vec::new();
        for member in members {
            let repo = member["repo_id"].as_str().context("repo_id absent")?;
            let root = member["root_path"].as_str().context("root_path absent")?;
            // Changes exposes the repository's workspace-relative root, as
            // stored in workspace_repository_rows, not a guest absolute path.
            ensure!(!root.is_empty() && std::path::Path::new(root).components().all(|part|
                matches!(part, std::path::Component::Normal(_))), "invalid member root: {root}");
            let root = format!("/work/{root}");
            let path = if repo == docs_repo { format!("docs/{}/block-proof-{token}.md", first.id) }
                else { format!("block-proof-{token}.txt") };
            let content = format!("Files API proof {repo} {token}\n");
            api.authed_put(&format!("{}/api/v1/workspaces/{}/files/{path}?repoId={repo}", api.base_url(), first.id))
                .json(&json!({"content":content,"encoding":"utf-8","expectedHash":null}))
                .send().await?.error_for_status()?;
            let expected = format!("{content}terminal proof\n");
            shell(&mut first_terminal, &format!(
                "/opt/g5/bin/node -e {}", quote(&format!(
                    "const fs=require('fs'),a=require('assert/strict');const p={};a.equal(fs.readFileSync(p,'utf8'),{});fs.appendFileSync(p,'terminal proof\\n');",
                    serde_json::to_string(&format!("{root}/{path}"))?, serde_json::to_string(&content)?
                ))), 30).await?;
            ensure!(read_file(&api, first.id, repo, &path).await? == expected, "Files did not observe terminal write in {repo}");
            proofs.push((repo.to_owned(), root.to_owned(), path, expected));
        }
        shell(&mut first_terminal, &format!("test \"$(cat {})\" = primary", quote(&transient)), 30).await?;
        let stopped = stop(&api, first.id).await?;
        ensure!(stopped["lastFlushId"].is_string() && stopped["unflushedLossAt"].is_null(), "Stop did not preserve a complete receipt: {stopped}");
        api.resume_workspace_vm(first.id).await?;
        let restarted = vm(&api, first.id).await?;
        ensure!(restarted["vmId"].is_string() && restarted["vmId"] != first_vm["vmId"], "Start reused old VM identity");
        ensure!(restarted["hostId"] == second_vm["hostId"], "restarted VM moved away from the neighbor host");
        let mut restored_terminal = terminal(&api, first.id).await?;
        let restored = layout(&mut restored_terminal).await?;
        ensure!(restored["bootId"] != before["bootId"], "Start reused old kernel");
        shell(&mut restored_terminal, &format!("test ! -e {}", quote(&transient)), 30).await?;
        for (repo, root, path, expected) in proofs {
            ensure!(read_file(&api, first.id, &repo, &path).await? == expected, "receipt did not restore {repo}:{path}");
            shell(&mut restored_terminal, &format!(
                "/opt/g5/bin/node -e {}", quote(&format!(
                    "require('assert/strict').equal(require('fs').readFileSync({},'utf8'),{});",
                    serde_json::to_string(&format!("{root}/{path}"))?, serde_json::to_string(&expected)?
                ))), 30).await?;
        }
        let still_running = vm(&api, second.id).await?;
        ensure!(still_running["status"] == "running" && still_running["vmId"] == second_vm["vmId"], "neighbor changed during Stop/Start");
        // Image population can outlast an idle terminal's heartbeat. This
        // proves the neighbor VM and its data survived, independently of an
        // unused WebSocket session's lifetime.
        let mut neighbor_terminal = terminal(&api, second.id).await?;
        shell(&mut neighbor_terminal, &format!("test \"$(cat {})\" = neighbor", quote(&transient)), 30).await?;
        Ok(())
    }.await;
    finish(&api, &owned, outcome).await
}

#[tokio::test]
#[ignore = "requires block host with a Docker-capable guest image and a pinned trusted container image"]
async fn test_block_workload_docker_overlay_on_block() -> anyhow::Result<()> {
    ensure!(
        enabled()?,
        "Docker qualification requires G5_E2E_BLOCK_WORKLOAD=true"
    );
    let image = std::env::var("G5_E2E_BLOCK_DOCKER_IMAGE")
        .context("set G5_E2E_BLOCK_DOCKER_IMAGE to a trusted image@sha256:digest with /bin/sh")?;
    let (name, digest) = image
        .rsplit_once("@sha256:")
        .context("Docker image must be pinned by sha256 digest")?;
    ensure!(
        !name.is_empty()
            && !image.chars().any(char::is_whitespace)
            && digest.len() == 64
            && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid pinned Docker image"
    );
    ensure!(
        TestApiClient::from_env().healthz().await?,
        "g5-api not healthy"
    );
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    let workspace = api.create_workspace(org.id, "block-docker").await?;
    let outcome = async {
        api.resume_workspace_vm(workspace.id).await?;
        let mut terminal = terminal(&api, workspace.id).await?;
        layout(&mut terminal).await?;
        let token = Uuid::new_v4().simple().to_string();
        let directory = format!("/tmp/g5-block-docker-{token}");
        let container = format!("g5-block-{token}");
        let script = format!(r#"
command -v docker
attempt=0
until docker info >/dev/null 2>&1; do
  attempt=$((attempt + 1)); test "$attempt" -lt 45; sleep 1
done
test "$(docker info --format '{{{{.Driver}}}}')" = overlay2
docker_root=$(docker info --format '{{{{.DockerRootDir}}}}')
/opt/g5/bin/node -e 'const fs=require("fs"),a=require("assert/strict");a.equal(fs.statSync(process.argv[1]).dev,fs.statSync("/").dev);' "$docker_root"
mkdir {directory}
trap 'docker rm -f {container} >/dev/null 2>&1 || true; rm -rf {directory}' EXIT
docker pull --quiet {image}
docker run --rm --network none --name {container} --entrypoint /bin/sh --mount type=bind,src={directory},dst=/probe {image} -ec 'printf "%s" overlay-proof > /g5-block-overlay; test "$(cat /g5-block-overlay)" = overlay-proof; printf "%s" bind-proof > /probe/from-container'
test "$(cat {directory}/from-container)" = bind-proof
docker info --format 'G5_BLOCK_DOCKER {{{{.Driver}}}} {{{{.DockerRootDir}}}}'
"#, image=quote(&image));
        shell(&mut terminal, &script, 240).await?;
        Ok(())
    }.await;
    finish(&api, &[workspace.id], outcome).await
}
