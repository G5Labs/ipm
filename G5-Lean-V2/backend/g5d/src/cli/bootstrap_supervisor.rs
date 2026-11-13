//! `g5d bootstrap` — the trusted supervisor process behind the platform-shipped
//! `workspace.bootstrap` module's `run` operation (design v9 §6.7, Agent API
//! §6.6, plan P4.2b item 9).
//!
//! The dispatcher's module runner starts this process with a cleared
//! environment and writes exactly one invocation record on its stdin.
//! Repository-declared setup steps (`g5.yaml` → `setup.bootstrap`) run with a
//! constructed environment and stdin closed. Completion reports the setup result.
//!
//! Events are emitted as JSON lines on stdout (`{"kind": …, "payload": …}`);
//! the dispatcher journals them as `module.<kind>`. Step output goes to stderr,
//! which the dispatcher redacts and retains as bounded logs. The final line is
//! the `outcome` record; the process exit status mirrors its `status`.

use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use g5_contracts::error::G5Error;
use serde_json::{json, Value};

use crate::cli::config::RepoConfig;

/// Catalog identity this supervisor serves.
pub const MODULE_ID: &str = "workspace.bootstrap";
/// The single operation of the module.
pub const OPERATION: &str = "run";
/// Declared step profiles. `setup` maps to `g5.yaml` `setup.bootstrap`.
pub const PROFILES: &[&str] = &["setup"];
/// Upper bound on the invocation record read from stdin.
const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
pub async fn execute() -> Result<(), G5Error> {
    // The supervisor holds the capability in memory: it is never dumpable or
    // traceable by a same-identity process on Linux.
    #[cfg(target_os = "linux")]
    // SAFETY: prctl with constant arguments on the current process.
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }
    let outcome = match read_record() {
        Ok(record) => run(record).await,
        Err(reason) => failed("INVALID_INVOCATION_RECORD", json!({"detail":reason})),
    };
    emit("outcome", outcome.clone());
    if outcome["status"] == "succeeded" {
        Ok(())
    } else {
        Err(G5Error::Internal {
            message: format!(
                "workspace.bootstrap {}: {}",
                outcome["status"].as_str().unwrap_or("failed"),
                outcome["reason"].as_str().unwrap_or("")
            ),
        })
    }
}

struct Record {
    input: Value,
}

fn read_record() -> Result<Record, String> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .take(MAX_RECORD_BYTES as u64)
        .read_line(&mut line)
        .map_err(|e| format!("stdin: {e}"))?;
    let record: Value = serde_json::from_str(line.trim_end()).map_err(|e| format!("json: {e}"))?;
    let field = |key: &str| {
        record[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("missing {key}"))
    };
    let exec_id = field("exec_id")?;
    if field("module_id")? != MODULE_ID {
        return Err("record is not for workspace.bootstrap".into());
    }
    if field("operation")? != OPERATION {
        return Err("unsupported operation".into());
    }
    // The dispatcher sets the same identity in the environment; a mismatch
    // means this process was not started by the runner for this invocation.
    if std::env::var("G5D_EXEC_ID").ok().as_deref() != Some(exec_id.as_str()) {
        return Err("G5D_EXEC_ID does not match the invocation record".into());
    }
    let input = record.get("input").cloned().unwrap_or(json!({}));
    if !input.is_object() {
        return Err("input must be an object".into());
    }
    Ok(Record { input })
}

fn emit(kind: &str, payload: Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut stdout, &json!({"kind":kind,"payload":payload}));
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

fn failed(reason: &str, detail: Value) -> Value {
    json!({"status":"failed","reason":reason,"detail":detail})
}

#[derive(Clone)]
struct Member {
    repo_id: String,
    root: PathBuf,
}

fn members(workspace_root: &Path, selected: Option<&str>) -> Result<Vec<Member>, Value> {
    let bytes = std::fs::read(workspace_root.join(".g5/layout.json"))
        .map_err(|e| failed("LAYOUT_UNREADABLE", json!({"detail":e.to_string()})))?;
    let layout: Value = serde_json::from_slice(&bytes)
        .map_err(|e| failed("LAYOUT_INVALID", json!({"detail":e.to_string()})))?;
    let all: Vec<Member> = layout["repos"]
        .as_array()
        .map(|repos| {
            repos
                .iter()
                .filter_map(|repo| {
                    Some(Member {
                        repo_id: repo["repo_id"].as_str()?.to_owned(),
                        root: workspace_root.join(repo["root_path"].as_str()?),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    match selected {
        None => Ok(all),
        Some(repo_id) => {
            let member = all.into_iter().find(|m| m.repo_id == repo_id);
            member
                .map(|m| vec![m])
                .ok_or_else(|| failed("UNKNOWN_MEMBER", json!({"repo_id":repo_id})))
        }
    }
}

#[cfg(unix)]
fn stderr_stdio() -> Stdio {
    use std::os::fd::{AsRawFd, FromRawFd};
    // SAFETY: a fresh duplicate of our own stderr, owned by the returned Stdio.
    unsafe {
        let fd = libc::dup(std::io::stderr().as_raw_fd());
        if fd < 0 {
            return Stdio::null();
        }
        Stdio::from_raw_fd(fd)
    }
}

#[cfg(not(unix))]
fn stderr_stdio() -> Stdio {
    Stdio::null()
}

/// Environment for repository setup steps; credentials remain excluded.
fn step_environment(extra: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut env: BTreeMap<String, String> = std::env::vars()
        .filter(|(key, _)| !key.starts_with("G5D_SUPERVISOR_") && key != "G5D_WORKSPACE_TOKEN")
        .collect();
    for (key, value) in extra {
        env.insert(key.clone(), value.clone());
    }
    env.into_iter().collect()
}

async fn run(record: Record) -> Value {
    let workspace_root = match std::env::var("G5D_WORKSPACE_ROOT") {
        Ok(root) => PathBuf::from(root),
        Err(_) => return failed("WORKSPACE_ROOT_MISSING", json!({})),
    };
    let profile = record.input["profile"].as_str().unwrap_or("setup");
    if !PROFILES.contains(&profile) {
        return failed(
            "UNKNOWN_PROFILE",
            json!({"profile":profile,"known":PROFILES}),
        );
    }
    let step_timeout = record.input["step_timeout_ms"]
        .as_u64()
        .map(Duration::from_millis);
    let members = match members(&workspace_root, record.input["repo_id"].as_str()) {
        Ok(members) => members,
        Err(outcome) => return outcome,
    };

    let mut reports = Vec::new();
    for member in &members {
        let config = match RepoConfig::load(&member.root) {
            Ok(config) => config,
            Err(error) => {
                reports.push(json!({"repo_id":member.repo_id,"ok":false,"error":"CONFIG_INVALID","detail":error.to_string()}));
                return failed("STEP_CONFIG_INVALID", json!({"members":reports}));
            }
        };
        let (steps, extra_env) = config
            .and_then(|c| c.setup)
            .map(|setup| {
                (
                    setup.bootstrap,
                    setup.env.into_iter().collect::<BTreeMap<_, _>>(),
                )
            })
            .unwrap_or_default();
        let env = step_environment(&extra_env);
        let mut executed = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            emit(
                "bootstrap.step",
                json!({"repo_id":member.repo_id,"index":index,"command":step,"phase":"started"}),
            );
            let result = run_step(&member.root, step, &env, step_timeout).await;
            let ok = result["ok"] == true;
            emit(
                "bootstrap.step",
                json!({"repo_id":member.repo_id,"index":index,"command":step,"phase":"finished","result":result}),
            );
            executed.push(json!({"index":index,"command":step,"result":result}));
            if !ok {
                reports.push(
                    json!({"repo_id":member.repo_id,"ok":false,"profile":profile,"steps":executed}),
                );
                return failed(
                    "STEP_FAILED",
                    json!({"repo_id":member.repo_id,"index":index,"members":reports}),
                );
            }
        }
        reports
            .push(json!({"repo_id":member.repo_id,"ok":true,"profile":profile,"steps":executed}));
    }

    json!({"status":"succeeded","profile":profile,"members":reports})
}

async fn run_step(
    root: &Path,
    step: &str,
    env: &[(String, String)],
    timeout: Option<Duration>,
) -> Value {
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(step)
        .current_dir(root)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        // The platform binary may need an injected loader path, but arbitrary
        // repository bootstrap commands must not inherit that privilege.
        .stdin(Stdio::null())
        .stdout(stderr_stdio())
        .stderr(stderr_stdio())
        .kill_on_drop(true);
    g5d_foundation::runtime_environment::remove_native_loader_paths(&mut command);
    // Repository steps run under the host-provisioned unprivileged identity
    // the dispatcher passed along; the supervisor itself keeps the trusted
    // identity, so a step cannot inspect its memory or private state.
    #[cfg(unix)]
    {
        if let Some(uid) = identity("G5D_MODULE_UID") {
            command.uid(uid);
        }
        if let Some(gid) = identity("G5D_MODULE_GID") {
            command.gid(gid);
        }
    }
    // Steps stay in the supervisor's own process group: the dispatcher's
    // runner stops that whole group at settlement, so a step's descendants
    // can never outlive a cancelled or timed-out bootstrap.
    let started = std::time::Instant::now();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return json!({"ok":false,"error":"SPAWN_FAILED","detail":error.to_string()}),
    };
    let waited = match timeout {
        Some(limit) => tokio::time::timeout(limit, child.wait()).await,
        None => Ok(child.wait().await),
    };
    match waited {
        Ok(Ok(status)) => json!({
            "ok": status.success(),
            "exit_code": status.code(),
            "elapsed_ms": started.elapsed().as_millis() as u64,
        }),
        Ok(Err(error)) => json!({"ok":false,"error":"WAIT_FAILED","detail":error.to_string()}),
        Err(_) => {
            let _ = child.kill().await;
            json!({"ok":false,"error":"STEP_TIMED_OUT","timeout_ms":timeout.map(|t| t.as_millis() as u64)})
        }
    }
}

#[cfg(unix)]
fn identity(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|v| v.parse::<u32>().ok())
}
