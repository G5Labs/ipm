//! Host boot qualification on the real workspace-VM substrate (K1 / G5).
//!
//! Records the P4.4b cache recipe from `docs/worker-host-api-v9.md` §7:
//! three runs each of a cold restore, a restart of an existing workspace and
//! a new child with cached objects, all through the public lifecycle API
//! against the bound `g5d host` (real Cloud Hypervisor/KVM in Cloud CI, no process
//! fallback). Hosts delete a workspace's root when its VM is released, so a
//! restart builds a fresh root like any other boot; only the host's
//! repository cache can make it faster.
//!
//! Two clocks are recorded for every run:
//!
//! - `boot_to_ready_ms`: the host's own measurement from `vm.start`
//!   admission to the validated `runtime.ready`, carried on the `ready`
//!   event's log tail and stored in `worker_vm_events`. This is the
//!   authoritative observation.
//! - `api_observed_ms`: wall time from the resume request to the API
//!   reporting `running`, sampled every 250 ms. It bounds the user-visible
//!   latency and includes placement and command delivery.
//!
//! The scenario fails closed: a workspace that never reaches `running`
//! fails the run rather than skipping, because the lane promises a bound
//! host. Values are written as `HOST_BOOT_TIMINGS` JSON lines straight to
//! the process stderr (bypassing libtest's per-test capture, which would
//! swallow them for a passing test) so they land in the lane log, and, when
//! `G5_E2E_ARTIFACT_DIR` is set, to `host-boot-timings.json`.

use std::time::{Duration, Instant};

use g5_internal_api::host_ctl::BootSummary;
use serde::Serialize;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use crate::helpers::api_client::TestApiClient;

const RUNS: usize = 3;

/// Write one evidence line to the real stderr. `eprintln!` goes through
/// libtest's output capture and is discarded when the test passes; the
/// lane log needs these lines precisely when everything passed.
fn emit(line: impl std::fmt::Display) {
    use std::io::Write;
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{line}");
    let _ = stderr.flush();
}

const READY_DEADLINE: Duration = Duration::from_secs(180);
const STOP_DEADLINE: Duration = Duration::from_secs(180);
const POLL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize)]
struct Observation {
    scenario: &'static str,
    run: usize,
    workspace_id: Uuid,
    vm_id: Option<Uuid>,
    host_id: Option<Uuid>,
    api_observed_ms: u128,
    boot_to_ready_ms: u64,
    restore_fetch_kib: u64,
    /// Whether the host exposed hardware virtualization to this guest
    /// (`g5.architecture.microvm_backend.nested`), from the same summary.
    nested_virtualization: bool,
}

/// `G5_E2E_NESTED_VIRTUALIZATION=on|off` pins what the lane's hosts were
/// started with; the ready summary must agree, which proves the switch
/// travelled host environment → `g5d host` → `vm-exec` on a real boot.
fn expected_nested_virtualization() -> anyhow::Result<Option<bool>> {
    match std::env::var("G5_E2E_NESTED_VIRTUALIZATION") {
        Ok(value) if value == "on" => Ok(Some(true)),
        Ok(value) if value == "off" => Ok(Some(false)),
        Ok(value) => anyhow::bail!("G5_E2E_NESTED_VIRTUALIZATION={value:?}: expected on or off"),
        Err(_) => Ok(None),
    }
}

#[derive(Debug, Serialize)]
struct Summary {
    scenario: &'static str,
    runs: Vec<u64>,
    median_boot_to_ready_ms: u64,
    median_api_observed_ms: u128,
    restore_fetch_kib: Vec<u64>,
}

async fn vm_view(api: &TestApiClient, workspace_id: Uuid) -> anyhow::Result<serde_json::Value> {
    let body: serde_json::Value = api
        .authed_get(&format!(
            "{}/api/v1/workspaces/{workspace_id}/vm",
            api.base_url()
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(body["data"].clone())
}

async fn wait_for_status(
    api: &TestApiClient,
    workspace_id: Uuid,
    wanted: &str,
    deadline: Duration,
) -> anyhow::Result<serde_json::Value> {
    let until = Instant::now() + deadline;
    loop {
        let view = vm_view(api, workspace_id).await?;
        if view["status"] == wanted {
            return Ok(view);
        }
        if Instant::now() >= until {
            anyhow::bail!(
                "workspace {workspace_id} did not reach `{wanted}` within {deadline:?}: {view}"
            );
        }
        tokio::time::sleep(POLL).await;
    }
}

async fn resume_and_measure(
    api: &TestApiClient,
    db: &mut PgConnection,
    workspace_id: Uuid,
    scenario: &'static str,
    run: usize,
) -> anyhow::Result<Observation> {
    let started = Instant::now();
    api.authed_post(&format!(
        "{}/api/v1/workspaces/{workspace_id}/resume",
        api.base_url()
    ))
    .json(&serde_json::json!({"operationId": Uuid::new_v4()}))
    .send()
    .await?
    .error_for_status()?;
    let view = wait_for_status(api, workspace_id, "running", READY_DEADLINE).await?;
    let api_observed_ms = started.elapsed().as_millis();
    let vm_id = view["vmId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok());
    let host_id = view["hostId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok());
    let vm_id_text = vm_id
        .ok_or_else(|| anyhow::anyhow!("running workspace {workspace_id} has no vm id: {view}"))?
        .to_string();

    // The ready event and the `running` transition commit in one
    // transaction, so the host summary is durable once `running` is visible.
    let log_tail: Option<String> = sqlx::query_scalar(
        "SELECT payload ->> 'log_tail' FROM worker_vm_events \
         WHERE vm_id = $1 AND kind = 'ready' ORDER BY seq DESC LIMIT 1",
    )
    .bind(&vm_id_text)
    .fetch_optional(&mut *db)
    .await?;
    let log_tail = log_tail.ok_or_else(|| {
        anyhow::anyhow!("ready event for vm {vm_id_text} carries no host boot summary")
    })?;
    let summary = log_tail
        .parse::<BootSummary>()
        .map_err(|error| anyhow::anyhow!("host boot summary `{log_tail}`: {error}"))?;
    let observation = Observation {
        scenario,
        run,
        workspace_id,
        vm_id,
        host_id,
        api_observed_ms,
        boot_to_ready_ms: summary.boot_to_ready_ms,
        restore_fetch_kib: summary.restore_fetch_kib,
        nested_virtualization: summary.nested_virtualization,
    };
    if let Some(expected) = expected_nested_virtualization()? {
        anyhow::ensure!(
            summary.nested_virtualization == expected,
            "host boot summary `{log_tail}` reports nested_virtualization={} but the lane expects {}",
            summary.nested_virtualization,
            expected
        );
    }
    emit(format!(
        "HOST_BOOT_TIMINGS {}",
        serde_json::to_string(&observation).unwrap()
    ));
    Ok(observation)
}

async fn stop_and_wait(api: &TestApiClient, workspace_id: Uuid) -> anyhow::Result<()> {
    api.stop_workspace_vm(workspace_id).await?;
    wait_for_status(api, workspace_id, "stopped", STOP_DEADLINE).await?;
    Ok(())
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn summarize(scenario: &'static str, observations: &[Observation]) -> Summary {
    let runs = observations
        .iter()
        .map(|o| o.boot_to_ready_ms)
        .collect::<Vec<_>>();
    let mut api = observations
        .iter()
        .map(|o| o.api_observed_ms)
        .collect::<Vec<_>>();
    api.sort_unstable();
    Summary {
        scenario,
        median_boot_to_ready_ms: median(&runs),
        median_api_observed_ms: api[api.len() / 2],
        restore_fetch_kib: observations.iter().map(|o| o.restore_fetch_kib).collect(),
        runs,
    }
}

/// Cold restore ×3 (a fresh workspace forked from the seeded org's base each
/// run; the host's URL-keyed bare cache for the org repository is warm from
/// the second run on), restart ×3 of the first workspace, and new-child ×3
/// (durable forks of that workspace, whose objects are already cached on the
/// host).
///
/// Every workspace lives in the pre-seeded org because a worker host belongs
/// to exactly one org: a workspace in a fresh org would queue for capacity
/// forever.
#[tokio::test]
async fn host_boot_timings_cold_restart_and_new_child() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");
    let org = api.use_seeded_org().await?;
    let mut db = PgConnection::connect(&crate::helpers::env::e2e_db_url()).await?;

    let mut cold = Vec::with_capacity(RUNS);
    let mut cold_workspaces = Vec::with_capacity(RUNS);
    for run in 1..=RUNS {
        let workspace = api
            .create_workspace(org.id, &format!("host-timing-cold-{run}"))
            .await?;
        let observation = resume_and_measure(&api, &mut db, workspace.id, "cold", run).await?;
        stop_and_wait(&api, workspace.id).await?;
        cold.push(observation);
        cold_workspaces.push(workspace.id);
    }

    // Restart: the first cold workspace starts again from its last save.
    let restart_workspace = cold_workspaces[0];
    let mut restarts = Vec::with_capacity(RUNS);
    for run in 1..=RUNS {
        let observation =
            resume_and_measure(&api, &mut db, restart_workspace, "restart", run).await?;
        stop_and_wait(&api, restart_workspace).await?;
        restarts.push(observation);
    }

    // New child: a durable fork of the restarted workspace boots for the
    // first time with the parent's objects already in the host bare cache.
    let mut children = Vec::with_capacity(RUNS);
    for run in 1..=RUNS {
        let forked = api
            .clone_workspace(
                restart_workspace,
                &format!("host-timing-child-{run}-{}", Uuid::new_v4().simple()),
            )
            .await?;
        let child_id = forked["data"]["workspace"]["id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or_else(|| anyhow::anyhow!("fork response has no workspace id: {forked}"))?;
        let observation = resume_and_measure(&api, &mut db, child_id, "new_child", run).await?;
        stop_and_wait(&api, child_id).await?;
        children.push(observation);
    }

    let summaries = vec![
        summarize("cold", &cold),
        summarize("restart", &restarts),
        summarize("new_child", &children),
    ];
    let report = serde_json::json!({
        "runs_per_scenario": RUNS,
        "observations": cold.iter().chain(&restarts).chain(&children).collect::<Vec<_>>(),
        "summaries": summaries,
    });
    emit(format!("HOST_BOOT_TIMINGS_SUMMARY {report}"));
    if let Some(dir) = std::env::var_os("G5_E2E_ARTIFACT_DIR") {
        let path = std::path::PathBuf::from(dir).join("host-boot-timings.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
        emit(format!("HOST_BOOT_TIMINGS written to {}", path.display()));
    }
    Ok(())
}
